use ctappearance::liquid_glass;
use ctdesign::{GlassLook, LIQUID_GLASS_BLUR_CAP, LIQUID_GLASS_LENS};
use grade_graphics::shaders;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::{Bind, Frame, Offscreen, Renderer, Texture};
use smithay::utils::{Buffer as BufferCoords, Physical, Rectangle, Size, Transform};

use super::look::{GlassPost, GlassShape, Look, Post};

const NARROWEST_LOW_SIGMA: f32 = 3.4;
const MOST_HALVINGS: u32 = 3;
const REACH_IN_SIGMAS: f32 = 3.0;
const MOST_TAPS: f32 = 64.0;
const MOST_GLASS_LEVELS: usize = 13;
const GLASS_SIGMAS: [f32; MOST_GLASS_LEVELS] = [
    0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 13.0, 16.0, 20.0, 24.0, 28.0,
];
const GLASS_READ_FACTOR: u32 = 2;
const RIM_NAMES: [&str; 24] = [
    "angle1_a", "angle1_b", "angle1_c", "angle2_a", "angle2_b", "angle2_c", "dist1_a", "dist1_b",
    "dist1_c", "dist1_d", "dist1_e", "dist1_f", "dist1_g", "dist1_h", "dist1_i", "dist2_a",
    "dist2_b", "dist2_c", "dist2_d", "dist2_e", "dist2_f", "dist2_g", "dist2_h", "dist2_i",
];
const BASE_POST_UNIFORMS: usize = 25;
const POST_UNIFORMS: usize = BASE_POST_UNIFORMS + RIM_NAMES.len();
const ANGLE_CHUNKS: usize = 3;
const DISTANCE_CHUNKS: usize = 9;

pub(super) struct Programs {
    reduce: GlesTexProgram,
    gauss: GlesTexProgram,
    normalize: GlesTexProgram,
    glass: GlesTexProgram,
    post: GlesTexProgram,
}

pub(super) fn compile(renderer: &mut GlesRenderer) -> Option<Programs> {
    let mut program = |shader, uniforms: &[UniformName]| {
        renderer
            .compile_custom_texture_shader(shader, uniforms)
            .inspect_err(|error| tracing::warn!(?error, "failed to compile blur shader"))
            .ok()
    };
    Some(Programs {
        reduce: program(
            shaders::BLUR_REDUCE,
            &[
                UniformName::new("source_size", UniformType::_2f),
                UniformName::new("target_size", UniformType::_2f),
                UniformName::new("region", UniformType::_4f),
                UniformName::new("span", UniformType::_1f),
                UniformName::new("straight", UniformType::_1f),
            ],
        )?,
        gauss: program(
            shaders::BLUR_GAUSS,
            &[
                UniformName::new("size", UniformType::_2f),
                UniformName::new("direction", UniformType::_2f),
                UniformName::new("decay", UniformType::_2f),
                UniformName::new("reach", UniformType::_1f),
            ],
        )?,
        normalize: program(
            shaders::BLUR_NORMALIZE,
            &[
                UniformName::new("coords_scale", UniformType::_2f),
                UniformName::new("edge", UniformType::_2f),
            ],
        )?,
        glass: program(
            shaders::BLUR_GLASS,
            &[
                UniformName::new("coords_scale", UniformType::_2f),
                UniformName::new("edge", UniformType::_2f),
                UniformName::new("glass_size", UniformType::_2f),
                UniformName::new("glass_rect", UniformType::_4f),
                UniformName::new("glass_corner", UniformType::_1f),
                UniformName::new("glass_exponent", UniformType::_1f),
                UniformName::new("glass_unit", UniformType::_1f),
                UniformName::new("lens", UniformType::_4f),
                UniformName::new("profile", UniformType::_4f),
                UniformName::new("share_a", UniformType::_4f),
                UniformName::new("share_b", UniformType::_4f),
                UniformName::new("constant_weight", UniformType::_1f),
            ],
        )?,
        post: program(shaders::BLUR_POST, &post_uniforms())?,
    })
}

fn post_uniforms() -> Vec<UniformName<'static>> {
    let mut names = vec![
        UniformName::new("shell", UniformType::_1f),
        UniformName::new("saturation", UniformType::_1f),
        UniformName::new("veil", UniformType::_4f),
        UniformName::new("limits", UniformType::_4f),
        UniformName::new("levels_a", UniformType::_4f),
        UniformName::new("levels_b", UniformType::_4f),
        UniformName::new("levels_c", UniformType::_4f),
        UniformName::new("tone_a", UniformType::_4f),
        UniformName::new("tone_b", UniformType::_4f),
        UniformName::new("tone_c", UniformType::_4f),
        UniformName::new("knots_a", UniformType::_4f),
        UniformName::new("knots_b", UniformType::_1f),
        UniformName::new("chroma_a", UniformType::_4f),
        UniformName::new("chroma_b", UniformType::_1f),
        UniformName::new("face", UniformType::_4f),
        UniformName::new("fill", UniformType::_4f),
        UniformName::new("delta_a", UniformType::_4f),
        UniformName::new("delta_b", UniformType::_4f),
        UniformName::new("delta_c", UniformType::_1f),
        UniformName::new("glass_size", UniformType::_2f),
        UniformName::new("glass_rect", UniformType::_4f),
        UniformName::new("glass_corner", UniformType::_1f),
        UniformName::new("glass_exponent", UniformType::_1f),
        UniformName::new("glass_unit", UniformType::_1f),
        UniformName::new("rim_on", UniformType::_1f),
    ];
    for name in RIM_NAMES {
        names.push(UniformName::new(name, UniformType::_4f));
    }
    names
}

#[derive(Clone, Copy)]
pub(super) enum Blend {
    Replace,
    Over(f32),
}

pub(super) fn draw_pass(
    renderer: &mut GlesRenderer,
    program: Option<&GlesTexProgram>,
    from: &GlesTexture,
    to: &mut GlesTexture,
    uniforms: &[Uniform<'_>],
    blend: Blend,
) -> Option<()> {
    let from_size = from.size();
    let to_size = to.size();
    let dst = Rectangle::from_size(Size::<i32, Physical>::from((to_size.w, to_size.h)));
    let mut target = renderer.bind(to).ok()?;
    let mut frame = renderer
        .render(&mut target, dst.size, Transform::Normal)
        .ok()?;
    let alpha = match blend {
        Blend::Replace => {
            frame.clear([0.0; 4].into(), &[dst]).ok()?;
            1.0
        }
        Blend::Over(alpha) => alpha,
    };
    frame
        .render_texture_from_to(
            from,
            Rectangle::from_size(from_size.to_f64()),
            dst,
            &[dst],
            &[],
            Transform::Normal,
            alpha,
            program,
            uniforms,
        )
        .ok()?;
    frame.finish().ok().map(|_| ())
}

fn texture(renderer: &mut GlesRenderer, size: Size<i32, BufferCoords>) -> Option<GlesTexture> {
    Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).ok()
}

fn shrunk(size: Size<i32, BufferCoords>, halvings: u32) -> Size<i32, BufferCoords> {
    let step = 1 << halvings;
    Size::from(((size.w + step - 1) / step, (size.h + step - 1) / step))
}

fn pair(size: Size<i32, BufferCoords>) -> [f32; 2] {
    [size.w as f32, size.h as f32]
}

pub(super) fn reduction_for(sigma: f32) -> u32 {
    let mut factor = 1;
    while factor < 1 << MOST_HALVINGS && sigma / (2 * factor) as f32 >= NARROWEST_LOW_SIGMA {
        factor <<= 1;
    }
    factor
}

pub(super) fn reach_in_pixels(sigma: f32, scale: f64) -> i32 {
    (REACH_IN_SIGMAS * sigma * scale as f32).ceil() as i32
}

pub(super) fn low_sigma(sigma: f32, factor: u32) -> f32 {
    if factor == 1 {
        return sigma;
    }
    let factor = factor as f32;
    (sigma * sigma - factor * factor / 4.0).max(0.0).sqrt() / factor
}

struct Chain {
    factor: u32,
    levels: Vec<GlesTexture>,
    across: GlesTexture,
    blurred: GlesTexture,
}

impl Chain {
    fn new(
        renderer: &mut GlesRenderer,
        size: Size<i32, BufferCoords>,
        factor: u32,
    ) -> Option<Self> {
        let sizes: Vec<_> = if factor == 1 {
            vec![size]
        } else {
            (1..=factor.trailing_zeros())
                .map(|halvings| shrunk(size, halvings))
                .collect()
        };
        let levels = sizes
            .iter()
            .map(|size| texture(renderer, *size))
            .collect::<Option<Vec<_>>>()?;
        let last = *sizes.last()?;
        Some(Self {
            factor,
            levels,
            across: texture(renderer, last)?,
            blurred: texture(renderer, last)?,
        })
    }

    fn run(
        &mut self,
        renderer: &mut GlesRenderer,
        programs: &Programs,
        source: &GlesTexture,
        region: [f32; 4],
        sigma: f32,
    ) -> Option<()> {
        let span = if self.factor == 1 { 1.0 } else { 2.0 };
        for level in 0..self.levels.len() {
            let (done, rest) = self.levels.split_at_mut(level);
            let from = done.last().unwrap_or(source);
            let target = &mut rest[0];
            let from_size = from.size();
            let bounds = if level == 0 {
                region
            } else {
                [0.0, 0.0, from_size.w as f32, from_size.h as f32]
            };
            let uniforms = [
                Uniform::new("source_size", pair(from_size)),
                Uniform::new("target_size", pair(target.size())),
                Uniform::new("region", bounds),
                Uniform::new("span", span),
                Uniform::new("straight", f32::from(u8::from(level == 0))),
            ];
            draw_pass(
                renderer,
                Some(&programs.reduce),
                from,
                target,
                &uniforms,
                Blend::Replace,
            )?;
        }
        let sigma = low_sigma(sigma, self.factor);
        let reach = (REACH_IN_SIGMAS * sigma).ceil().min(MOST_TAPS);
        let last = self.levels.last()?;
        let size = pair(last.size());
        let variance = sigma.max(f32::EPSILON).powi(2);
        let decay = [(-0.5 / variance).exp(), (-1.0 / variance).exp()];
        let along = |direction: [f32; 2]| {
            [
                Uniform::new("size", size),
                Uniform::new("direction", direction),
                Uniform::new("decay", decay),
                Uniform::new("reach", reach),
            ]
        };
        draw_pass(
            renderer,
            Some(&programs.gauss),
            last,
            &mut self.across,
            &along([1.0, 0.0]),
            Blend::Replace,
        )?;
        draw_pass(
            renderer,
            Some(&programs.gauss),
            &self.across,
            &mut self.blurred,
            &along([0.0, 1.0]),
            Blend::Replace,
        )
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Layout {
    size: Size<i32, BufferCoords>,
    core: Option<u32>,
    halo: Option<u32>,
    glass: Option<[u32; MOST_GLASS_LEVELS]>,
    finished: bool,
}

fn shaped_glass(look: &Look) -> Option<(GlassPost, GlassShape)> {
    match look.post {
        Post::Glass(glass) => glass.shape.map(|shape| (glass, shape)),
        _ => None,
    }
}

fn glass_sigmas(glass: &GlassPost) -> ([f32; MOST_GLASS_LEVELS], usize) {
    let widest = glass
        .look
        .blur_opacity
        .iter()
        .map(|opacity| {
            let level = GlassLook {
                blur_opacity: [*opacity; 4],
                blur_distance: [0.0; 4],
                ..glass.look
            };
            liquid_glass::blur_sigma(&level, 0.0)
        })
        .fold(0.0_f32, f32::max);
    let mut sigmas = [0.0; MOST_GLASS_LEVELS];
    let mut count = 0;
    for sigma in GLASS_SIGMAS {
        sigmas[count] = sigma;
        count += 1;
        if sigma >= widest {
            break;
        }
    }
    if sigmas[count - 1] < widest {
        sigmas[count - 1] = LIQUID_GLASS_BLUR_CAP;
    }
    (sigmas, count)
}

impl Layout {
    fn of(size: Size<i32, BufferCoords>, look: &Look, scale: f64) -> Self {
        let factor = |sigma: f32| reduction_for(sigma * scale as f32);
        let shaped = shaped_glass(look);
        let read = |factor: u32| {
            if shaped.is_some() {
                factor.max(GLASS_READ_FACTOR)
            } else {
                factor
            }
        };
        Self {
            size,
            core: (!look.solid() && shaped.is_none()).then(|| factor(look.sigma)),
            halo: look.halo.map(|(sigma, _)| read(factor(sigma))),
            glass: shaped.map(|(glass, _)| {
                let (sigmas, count) = glass_sigmas(&glass);
                let mut factors = [0; MOST_GLASS_LEVELS];
                for (at, sigma) in sigmas[..count].iter().enumerate() {
                    factors[at] = read(factor(*sigma));
                }
                factors
            }),
            finished: look.post != Post::Plain,
        }
    }
}

pub(super) struct Backdrop {
    layout: Layout,
    core: Option<Chain>,
    halo: Option<Chain>,
    levels: Vec<Chain>,
    mixed: GlesTexture,
    shown: Option<GlesTexture>,
}

impl Backdrop {
    pub(super) fn fits(&self, size: Size<i32, BufferCoords>, look: &Look, scale: f64) -> bool {
        self.layout == Layout::of(size, look, scale)
    }

    pub(super) fn new(
        renderer: &mut GlesRenderer,
        size: Size<i32, BufferCoords>,
        look: &Look,
        scale: f64,
    ) -> Option<Self> {
        let layout = Layout::of(size, look, scale);
        let chain = |renderer: &mut GlesRenderer, factor: Option<u32>| match factor {
            Some(factor) => Chain::new(renderer, size, factor).map(Some),
            None => Some(None),
        };
        let levels = layout
            .glass
            .iter()
            .flatten()
            .filter(|factor| **factor > 0)
            .map(|factor| Chain::new(renderer, size, *factor))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            layout,
            core: chain(renderer, layout.core)?,
            halo: chain(renderer, layout.halo)?,
            levels,
            mixed: texture(renderer, size)?,
            shown: if layout.finished {
                Some(texture(renderer, size)?)
            } else {
                None
            },
        })
    }

    pub(super) fn result(&self) -> &GlesTexture {
        self.shown.as_ref().unwrap_or(&self.mixed)
    }

    pub(super) fn run(
        &mut self,
        renderer: &mut GlesRenderer,
        programs: &Programs,
        source: &GlesTexture,
        region: Rectangle<i32, Physical>,
        look: &Look,
        scale: f64,
    ) -> Option<()> {
        let size = source.size();
        let scale = scale as f32;
        let bounds = |rect: Rectangle<i32, Physical>| {
            [
                rect.loc.x as f32,
                rect.loc.y as f32,
                (rect.loc.x + rect.size.w) as f32,
                (rect.loc.y + rect.size.h) as f32,
            ]
        };
        if let Some((glass, shape)) = shaped_glass(look) {
            let (sigmas, count) = glass_sigmas(&glass);
            for (chain, sigma) in self.levels.iter_mut().zip(&sigmas[..count]) {
                chain.run(renderer, programs, source, bounds(region), sigma * scale)?;
            }
            let at = GlassAt {
                glass: &glass,
                shape,
                scale,
            };
            for (index, chain) in self.levels.iter().enumerate() {
                let lower = if index == 0 { -1.0 } else { sigmas[index - 1] };
                let blend = if index == 0 {
                    Blend::Replace
                } else {
                    Blend::Over(1.0)
                };
                glass_pass(
                    renderer,
                    programs,
                    (&chain.blurred, chain.factor),
                    &mut self.mixed,
                    &at,
                    GlassWeight::Depth(lower, sigmas[index]),
                    blend,
                )?;
            }
            if let (Some(halo), Some((sigma, weight))) = (self.halo.as_mut(), look.halo) {
                halo.run(
                    renderer,
                    programs,
                    source,
                    bounds(Rectangle::<i32, Physical>::from_size(Size::from((
                        size.w, size.h,
                    )))),
                    sigma * scale,
                )?;
                glass_pass(
                    renderer,
                    programs,
                    (&halo.blurred, halo.factor),
                    &mut self.mixed,
                    &at,
                    GlassWeight::Constant,
                    Blend::Over(weight),
                )?;
            }
            return match self.shown.as_mut() {
                Some(shown) => post_pass(renderer, programs, &self.mixed, shown, look.post, scale),
                None => Some(()),
            };
        }
        let Some(core) = self.core.as_mut() else {
            return post_pass(
                renderer,
                programs,
                source,
                self.shown.as_mut()?,
                look.post,
                scale,
            );
        };
        core.run(
            renderer,
            programs,
            source,
            bounds(region),
            look.sigma * scale,
        )?;
        normalize(
            renderer,
            programs,
            (&core.blurred, core.factor),
            &mut self.mixed,
            Blend::Replace,
        )?;
        if let (Some(halo), Some((sigma, weight))) = (self.halo.as_mut(), look.halo) {
            halo.run(
                renderer,
                programs,
                source,
                bounds(Rectangle::<i32, Physical>::from_size(Size::from((
                    size.w, size.h,
                )))),
                sigma * scale,
            )?;
            normalize(
                renderer,
                programs,
                (&halo.blurred, halo.factor),
                &mut self.mixed,
                Blend::Over(weight),
            )?;
        }
        match self.shown.as_mut() {
            Some(shown) => post_pass(renderer, programs, &self.mixed, shown, look.post, scale),
            None => Some(()),
        }
    }
}

struct GlassAt<'a> {
    glass: &'a GlassPost,
    shape: GlassShape,
    scale: f32,
}

enum GlassWeight {
    Depth(f32, f32),
    Constant,
}

fn glass_pass(
    renderer: &mut GlesRenderer,
    programs: &Programs,
    (from, factor): (&GlesTexture, u32),
    to: &mut GlesTexture,
    at: &GlassAt<'_>,
    weight: GlassWeight,
    blend: Blend,
) -> Option<()> {
    let (from_size, to_size) = (from.size(), to.size());
    let look = &at.glass.look;
    let per_radius = liquid_glass::blur_sigma(
        &GlassLook {
            blur: 1.0,
            blur_opacity: [1.0; 4],
            blur_distance: [0.0; 4],
            ..*look
        },
        0.0,
    );
    let (lower, upper, constant) = match weight {
        GlassWeight::Depth(lower, upper) => (lower, upper, 0.0),
        GlassWeight::Constant => (0.0, 1.0, 1.0),
    };
    draw_pass(
        renderer,
        Some(&programs.glass),
        from,
        to,
        &[
            Uniform::new(
                "coords_scale",
                [
                    to_size.w as f32 / (factor as f32 * from_size.w as f32),
                    to_size.h as f32 / (factor as f32 * from_size.h as f32),
                ],
            ),
            Uniform::new("edge", [0.5 / from_size.w as f32, 0.5 / from_size.h as f32]),
            Uniform::new("glass_size", pair(to_size)),
            Uniform::new("glass_rect", at.shape.rect),
            Uniform::new("glass_corner", at.shape.radius),
            Uniform::new("glass_exponent", at.shape.exponent),
            Uniform::new("glass_unit", at.scale),
            Uniform::new(
                "lens",
                [
                    look.refraction[0],
                    look.refraction[1],
                    LIQUID_GLASS_LENS.scale,
                    LIQUID_GLASS_LENS.decay,
                ],
            ),
            Uniform::new(
                "profile",
                [per_radius * look.blur, LIQUID_GLASS_BLUR_CAP, lower, upper],
            ),
            Uniform::new("share_a", look.blur_opacity),
            Uniform::new("share_b", look.blur_distance),
            Uniform::new("constant_weight", constant),
        ],
        blend,
    )
}

fn normalize(
    renderer: &mut GlesRenderer,
    programs: &Programs,
    (from, factor): (&GlesTexture, u32),
    to: &mut GlesTexture,
    blend: Blend,
) -> Option<()> {
    let (from_size, to_size) = (from.size(), to.size());
    let coords_scale = [
        to_size.w as f32 / (factor as f32 * from_size.w as f32),
        to_size.h as f32 / (factor as f32 * from_size.h as f32),
    ];
    draw_pass(
        renderer,
        Some(&programs.normalize),
        from,
        to,
        &[
            Uniform::new("coords_scale", coords_scale),
            Uniform::new("edge", [0.5 / from_size.w as f32, 0.5 / from_size.h as f32]),
        ],
        blend,
    )
}

fn knots<const N: usize>(values: &[f32; N], from: usize) -> [f32; 4] {
    std::array::from_fn(|at| values.get(from + at).copied().unwrap_or(0.0))
}

fn rim_uniform(at: usize, angles: &[[f32; 12]; 2], distances: &[[f32; 35]; 2]) -> Uniform<'static> {
    let value = match at {
        0..ANGLE_CHUNKS => knots(&angles[0], at * 4),
        ANGLE_CHUNKS..6 => knots(&angles[1], (at - ANGLE_CHUNKS) * 4),
        6..15 => knots(&distances[0], (at - 6) * 4),
        _ => knots(&distances[1], (at - 6 - DISTANCE_CHUNKS) * 4),
    };
    Uniform::new(RIM_NAMES[at], value)
}

fn post_pass(
    renderer: &mut GlesRenderer,
    programs: &Programs,
    from: &GlesTexture,
    to: &mut GlesTexture,
    post: Post,
    scale: f32,
) -> Option<()> {
    let (shell, saturation, veil, limits) = match post {
        Post::Material(look) => (
            0.0,
            look.saturation,
            [look.veil.r, look.veil.g, look.veil.b, look.veil.a],
            [look.cap, look.floor, look.gain, look.bias],
        ),
        Post::Shell(_) => (1.0, 1.0, [0.0; 4], [1.0, 0.0, 1.0, 0.0]),
        Post::Glass(_) => (2.0, 1.0, [0.0; 4], [1.0, 0.0, 1.0, 0.0]),
        Post::Plain => return None,
    };
    let (face, fill, delta) = match post {
        Post::Glass(glass) => (
            [
                glass.look.white,
                glass.look.black,
                glass.look.saturation,
                glass.look.max_luma,
            ],
            [
                glass.look.fill.r,
                glass.look.fill.g,
                glass.look.fill.b,
                glass.look.fill.a,
            ],
            glass.look.tone_delta,
        ),
        _ => ([0.0; 4], [0.0; 4], [0.0; 9]),
    };
    let (levels, tone, chroma_knots, chroma) = match post {
        Post::Shell(glass) => (
            ctdesign::SHELL_GLASS_LEVELS,
            glass.tone,
            ctdesign::SHELL_GLASS_CHROMA_KNOTS,
            glass.chroma,
        ),
        _ => ([0.0; 12], [0.0; 12], [0.0; 5], [0.0; 5]),
    };
    let (rim_on, rect, corner, exponent) = match post {
        Post::Glass(GlassPost {
            shape: Some(shape), ..
        }) => (1.0, shape.rect, shape.radius, shape.exponent),
        _ => (0.0, [0.0; 4], 0.0, ctdesign::CIRCLE_EXPONENT),
    };
    let size = pair(to.size());
    let (angles, distances) = match post {
        Post::Glass(glass) => (
            [glass.rim.angle_1, glass.rim.angle_2],
            [glass.rim.distance_1, glass.rim.distance_2],
        ),
        _ => ([[0.0; 12]; 2], [[0.0; 35]; 2]),
    };
    let uniforms: [Uniform<'_>; POST_UNIFORMS] = std::array::from_fn(|at| match at {
        0 => Uniform::new("shell", shell),
        1 => Uniform::new("saturation", saturation),
        2 => Uniform::new("veil", veil),
        3 => Uniform::new("limits", limits),
        4 => Uniform::new("levels_a", knots(&levels, 0)),
        5 => Uniform::new("levels_b", knots(&levels, 4)),
        6 => Uniform::new("levels_c", knots(&levels, 8)),
        7 => Uniform::new("tone_a", knots(&tone, 0)),
        8 => Uniform::new("tone_b", knots(&tone, 4)),
        9 => Uniform::new("tone_c", knots(&tone, 8)),
        10 => Uniform::new("knots_a", knots(&chroma_knots, 0)),
        11 => Uniform::new("knots_b", chroma_knots[4]),
        12 => Uniform::new("chroma_a", knots(&chroma, 0)),
        13 => Uniform::new("chroma_b", chroma[4]),
        14 => Uniform::new("face", face),
        15 => Uniform::new("fill", fill),
        16 => Uniform::new("delta_a", knots(&delta, 0)),
        17 => Uniform::new("delta_b", knots(&delta, 4)),
        18 => Uniform::new("delta_c", delta[8]),
        19 => Uniform::new("glass_size", size),
        20 => Uniform::new("glass_rect", rect),
        21 => Uniform::new("glass_corner", corner),
        22 => Uniform::new("glass_exponent", exponent),
        23 => Uniform::new("glass_unit", scale),
        24 => Uniform::new("rim_on", rim_on),
        _ => rim_uniform(at - BASE_POST_UNIFORMS, &angles, &distances),
    });
    draw_pass(
        renderer,
        Some(&programs.post),
        from,
        to,
        &uniforms,
        Blend::Replace,
    )
}
