use std::f32::consts::{FRAC_PI_2, PI};
use std::time::{Duration, Instant};

use ctappearance::dock::MinimizeEffect;
use malaysia_scene::scene::WindowId;
use smithay::backend::renderer::Texture;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::element::TextureShaderElement;
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::utils::{Logical, Point, Rectangle, Scale, Size, Transform};

use crate::cursor::OutputRenderElement;
use crate::renderer::{MalaysiaRenderer, PrimaryGpu};
use crate::scene_elements::Placement;
use crate::state::State;

const DURATION: Duration = Duration::from_millis(ctdesign::DOCK_MINIMIZE_MS);
const HANDOFF: Duration = Duration::from_millis(ctdesign::DOCK_MINIMIZE_HANDOFF_MS);

impl Minimizing {
    fn lasts(&self) -> Duration {
        match (&self.thumbnail, self.hiding) {
            (Some(_), true) => DURATION + HANDOFF,
            _ => DURATION,
        }
    }
}

pub(crate) struct Minimizing {
    pub(crate) window: WindowId,
    texture: TextureBuffer<GlesTexture>,
    thumbnail: Option<Size<f64, Logical>>,
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
    hiding: bool,
    start: Instant,
}

pub(crate) fn compile(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    renderer
        .compile_custom_texture_shader(
            grade_graphics::shaders::MINIMIZE,
            &[
                UniformName::new("turn", UniformType::_1f),
                UniformName::new("flip", UniformType::_1f),
                UniformName::new("rows", UniformType::_2f),
                UniformName::new("cut", UniformType::_1f),
                UniformName::new("wide", UniformType::_2f),
                UniformName::new("mouth", UniformType::_2f),
                UniformName::new("taper", UniformType::_2f),
            ],
        )
        .inspect_err(|error| tracing::warn!(?error, "failed to compile the minimize shader"))
        .ok()
}

pub(crate) struct MinimizeRequest {
    pub(crate) window: WindowId,
    hiding: bool,
    was_focused: bool,
}

impl State {
    pub(crate) fn request_minimize_effect(
        &mut self,
        window: WindowId,
        hiding: bool,
        was_focused: bool,
    ) {
        if self.minimize_program.is_some() {
            self.minimizing.retain(|effect| effect.window != window);
            self.minimize_requests.push(MinimizeRequest {
                window,
                hiding,
                was_focused,
            });
        }
    }

    pub(crate) fn is_minimizing(&self, window: WindowId) -> bool {
        self.minimizing.iter().any(|effect| effect.window == window)
    }
}

pub(crate) fn start_requested(state: &mut State, renderer: &mut GlesRenderer, scale: f64) {
    for request in std::mem::take(&mut state.minimize_requests) {
        let MinimizeRequest { window, hiding, .. } = request;
        let Some((texture, from)) = snapshot(state, renderer, &request, scale) else {
            continue;
        };
        let output = state.logical_output_size();
        let size = texture.size();
        let (to, thumbnail) = match state.minimize_hints.get(&window) {
            Some(slot) if state.config.dock.minimize_to_application => (*slot, None),
            Some(slot) => (
                thumbnail_rect(*slot, from.size, nearest_side(*slot, output)),
                Some(Size::from((f64::from(size.w), f64::from(size.h)))),
            ),
            None => (bottom_centre(output), None),
        };
        state.minimizing.push(Minimizing {
            window,
            thumbnail,
            texture: TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None),
            from,
            to,
            hiding,
            start: Instant::now(),
        });
    }
}

fn bottom_centre(output: Size<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((output.w / 2 - 50, output.h)),
        Size::from((100, 50)),
    )
}

fn snapshot(
    state: &mut State,
    renderer: &mut GlesRenderer,
    request: &MinimizeRequest,
    scale: f64,
) -> Option<(GlesTexture, Rectangle<i32, Logical>)> {
    let window = request.window;
    let minimized = state.minimized.remove(&window);
    state.windows.set_hidden(window, false);
    let outer = state.windows.outer(window).map(crate::popup::smithay_rect);
    let mut elements = Vec::new();
    if let Some(outer) = outer {
        let focused = if request.hiding {
            request.was_focused.then_some(window)
        } else {
            state.focused_window()
        };
        crate::scene_elements::append_window_frame(
            state,
            renderer,
            window,
            focused,
            scale,
            Placement {
                offset: Point::from((-outer.loc.x, -outer.loc.y)),
                snapshot: Some(crate::session::physical_point(outer.loc, scale)),
            },
            &mut elements,
        );
    }
    if minimized {
        state.minimized.insert(window);
        state.windows.set_hidden(window, true);
    }
    let outer = outer.filter(|outer| outer.size.w > 0 && outer.size.h > 0)?;
    let size = outer.size.to_physical_precise_round(scale);
    let texture = crate::renderer::render_to_texture(
        renderer,
        size,
        Scale::from(scale),
        &elements,
        [0.0; 4],
    )?;
    Some((texture, outer))
}

pub(crate) fn append<R: MalaysiaRenderer>(
    state: &mut State,
    now: Instant,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    state
        .minimizing
        .retain(|effect| now.saturating_duration_since(effect.start) < effect.lasts());
    let Some(program) = state.minimize_program.clone() else {
        return;
    };
    let output = state.logical_output_size();
    let effect = state.config.dock.mineffect;
    for minimizing in &state.minimizing {
        let elapsed = now
            .saturating_duration_since(minimizing.start)
            .as_secs_f64();
        let linear = (elapsed / DURATION.as_secs_f64()).clamp(0.0, 1.0);
        let progress = if minimizing.hiding {
            linear
        } else {
            1.0 - linear
        };
        let course = Course::new(minimizing.from, minimizing.to, output);
        let shape = course.shape(effect, progress as f32);
        elements.extend(built_thumbnail(minimizing, (&course, &shape), scale).map(Into::into));
        elements
            .push(PrimaryGpu(bend(minimizing, (&course, &shape), program.clone(), scale)).into());
    }
}

fn thumbnail_rect(
    slot: Rectangle<i32, Logical>,
    window: Size<i32, Logical>,
    side: Side,
) -> Rectangle<i32, Logical> {
    let body = f64::from(slot.size.w.min(slot.size.h)) * f64::from(ctdesign::DOCK_ICON_BODY);
    let aspect = f64::from(window.w.max(1)) / f64::from(window.h.max(1));
    let (width, height) = match side {
        Side::Bottom | Side::Top => (body * aspect, body),
        Side::Left | Side::Right => (body, body / aspect),
    };
    let centre = slot.loc.to_f64() + slot.size.to_f64().downscale(2.0).to_point();
    Rectangle::new(
        Point::from((
            (centre.x - width / 2.0).round() as i32,
            (centre.y - height / 2.0).round() as i32,
        )),
        Size::from((width.round() as i32, height.round() as i32)),
    )
}

fn built_thumbnail(
    minimizing: &Minimizing,
    (course, shape): (&Course, &Shape),
    scale: f64,
) -> Option<PrimaryGpu<TextureRenderElement<GlesTexture>>> {
    let texture = minimizing.thumbnail?;
    let to = minimizing.to;
    let share = entered(shape);
    if share <= 0.0 {
        return None;
    }
    let [x0, y0, x1, y1] = built_part(course.side, share).map(f64::from);
    let to = to.to_f64();
    let part = Rectangle::<f64, Logical>::new(
        Point::from((to.loc.x + x0 * to.size.w, to.loc.y + y0 * to.size.h)),
        Size::from(((x1 - x0) * to.size.w, (y1 - y0) * to.size.h)),
    );
    let src = Rectangle::<f64, Logical>::new(
        Point::from((x0 * texture.w, y0 * texture.h)),
        Size::from(((x1 - x0) * texture.w, (y1 - y0) * texture.h)),
    );
    Some(PrimaryGpu(TextureRenderElement::from_texture_buffer(
        part.loc.to_physical(scale),
        &minimizing.texture,
        None,
        Some(src),
        Some(part.size.to_i32_round()),
        Kind::Unspecified,
    )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Bottom,
    Left,
    Right,
    Top,
}

impl Side {
    fn turned(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }

    fn flipped(self) -> bool {
        matches!(self, Side::Top | Side::Left)
    }

    fn to_lane(self, [x, y]: [f32; 2]) -> [f32; 2] {
        let [breadth, depth] = if self.turned() { [y, x] } else { [x, y] };
        [breadth, if self.flipped() { 1.0 - depth } else { depth }]
    }

    fn to_upright(self, [breadth, depth]: [f32; 2]) -> [f32; 2] {
        let depth = if self.flipped() { 1.0 - depth } else { depth };
        if self.turned() {
            [depth, breadth]
        } else {
            [breadth, depth]
        }
    }
}

fn nearest_side(target: Rectangle<i32, Logical>, output: Size<i32, Logical>) -> Side {
    let end = target.loc + target.size.to_point();
    [
        (Side::Bottom, output.h - end.y),
        (Side::Left, target.loc.x),
        (Side::Right, output.w - end.x),
        (Side::Top, target.loc.y),
    ]
    .into_iter()
    .min_by_key(|&(_, gap)| gap)
    .map_or(Side::Bottom, |(side, _)| side)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Footprint {
    breadth: [f32; 2],
    depth: [f32; 2],
}

fn footprint(
    rect: Rectangle<i32, Logical>,
    stage: Rectangle<i32, Logical>,
    side: Side,
) -> Footprint {
    let corner = |point: Point<i32, Logical>| {
        side.to_lane([
            (point.x - stage.loc.x) as f32 / stage.size.w.max(1) as f32,
            (point.y - stage.loc.y) as f32 / stage.size.h.max(1) as f32,
        ])
    };
    let [breadth_a, depth_a] = corner(rect.loc);
    let [breadth_b, depth_b] = corner(rect.loc + rect.size.to_point());
    Footprint {
        breadth: [breadth_a.min(breadth_b), breadth_a.max(breadth_b)],
        depth: [depth_a.min(depth_b), depth_a.max(depth_b)],
    }
}

struct Course {
    side: Side,
    stage: Rectangle<i32, Logical>,
    window: Footprint,
    target: Footprint,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Shape {
    rows: [f32; 2],
    cut: f32,
    wide: [f32; 2],
    mouth: [f32; 2],
    taper: [f32; 2],
}

const FORMING_SHARE: f32 = 0.4;
const POURING_RAMP: f32 = 0.35;

fn forming(progress: f32) -> f32 {
    (FRAC_PI_2 * (progress / FORMING_SHARE).clamp(0.0, 1.0)).sin()
}

fn pouring(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    let launch = 2.0 * POURING_RAMP / PI;
    let covered = if progress < POURING_RAMP {
        launch * (1.0 - (FRAC_PI_2 * progress / POURING_RAMP).cos())
    } else {
        launch + progress - POURING_RAMP
    };
    covered / (launch + 1.0 - POURING_RAMP)
}

fn half_wave(progress: f32) -> f32 {
    0.5 - 0.5 * (PI * progress).cos()
}

fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

impl Course {
    fn new(
        from: Rectangle<i32, Logical>,
        to: Rectangle<i32, Logical>,
        output: Size<i32, Logical>,
    ) -> Self {
        let side = nearest_side(to, output);
        let stage = from.merge(to);
        Course {
            side,
            stage,
            window: footprint(from, stage, side),
            target: footprint(to, stage, side),
        }
    }

    fn shape(&self, effect: MinimizeEffect, progress: f32) -> Shape {
        let (window, target) = (self.window, self.target);
        let far = target.depth[1];
        if effect == MinimizeEffect::Scale {
            let eased = half_wave(progress);
            let blend =
                |a: [f32; 2], b: [f32; 2]| [lerp(a[0], b[0], eased), lerp(a[1], b[1], eased)];
            let wide = blend(window.breadth, target.breadth);
            return Shape {
                rows: blend(window.depth, target.depth),
                cut: window.depth[1].max(far),
                wide,
                mouth: wide,
                taper: [far, far],
            };
        }
        let mouth = if effect == MinimizeEffect::Suck {
            let centre = (target.breadth[0] + target.breadth[1]) / 2.0;
            [centre, centre]
        } else {
            target.breadth
        };
        let formed = forming(progress);
        let recede = |edge: f32| lerp(edge.max(window.depth[1]), edge, formed);
        let neck = recede(target.depth[0]);
        let travelled = pouring(progress) * (far - window.depth[0]);
        Shape {
            rows: [window.depth[0] + travelled, window.depth[1] + travelled],
            cut: recede(far),
            wide: window.breadth,
            mouth,
            taper: [lerp(neck, window.depth[0].min(neck), formed), neck],
        }
    }
}

fn entered(shape: &Shape) -> f32 {
    let [first, last] = shape.rows;
    ((last - shape.cut) / (last - first).max(f32::EPSILON)).clamp(0.0, 1.0)
}

fn built_part(side: Side, share: f32) -> [f32; 4] {
    let [x0, y0] = side.to_upright([0.0, 1.0 - share]);
    let [x1, y1] = side.to_upright([1.0, 1.0]);
    [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
}

fn bend(
    minimizing: &Minimizing,
    (course, shape): (&Course, &Shape),
    program: GlesTexProgram,
    scale: f64,
) -> TextureShaderElement {
    let stage = course.stage;
    let pixels: Size<i32, _> = minimizing.from.size.to_physical_precise_round(scale);
    let drawn = TextureRenderElement::from_texture_buffer(
        stage.loc.to_f64().to_physical(scale),
        &minimizing.texture,
        None,
        Some(Rectangle::from_size(Size::from((
            f64::from(pixels.w),
            f64::from(pixels.h),
        )))),
        Some(stage.size),
        Kind::Unspecified,
    );
    let switch = |on: bool| if on { 1.0f32 } else { 0.0 };
    TextureShaderElement::new(
        drawn,
        program,
        vec![
            Uniform::new("turn", switch(course.side.turned())),
            Uniform::new("flip", switch(course.side.flipped())),
            Uniform::new("rows", shape.rows),
            Uniform::new("cut", shape.cut),
            Uniform::new("wide", shape.wide),
            Uniform::new("mouth", shape.mouth),
            Uniform::new("taper", shape.taper),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    use crate::test_support::try_test_renderer;
    use smithay::backend::allocator::Fourcc;
    use smithay::backend::renderer::damage::OutputDamageTracker;
    use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
    use smithay::utils::Buffer as BufferCoord;

    fn red_window(renderer: &mut GlesRenderer) -> GlesTexture {
        use smithay::backend::renderer::element::solid::{
            SolidColorBuffer, SolidColorRenderElement,
        };
        let red = SolidColorRenderElement::from_buffer(
            &SolidColorBuffer::new(Size::from((120, 80)), [1.0, 0.0, 0.0, 1.0]),
            Point::from((0, 0)),
            1.0,
            1.0,
            Kind::Unspecified,
        );
        let size = Size::<i32, BufferCoord>::from((120, 80));
        let mut snapshot =
            Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).unwrap();
        {
            let mut target = renderer.bind(&mut snapshot).unwrap();
            let elements: Vec<OutputRenderElement<GlesRenderer>> = vec![red.into()];
            OutputDamageTracker::new(
                Size::<i32, smithay::utils::Physical>::from((120, 80)),
                Scale::from(1.0),
                Transform::Normal,
            )
            .render_output(renderer, &mut target, 0, &elements, [0.0; 4])
            .unwrap()
            .sync
            .wait()
            .unwrap();
        }
        snapshot
    }

    fn draw(
        renderer: &mut GlesRenderer,
        elements: &[OutputRenderElement<GlesRenderer>],
    ) -> Vec<u8> {
        let size = Size::<i32, BufferCoord>::from((200, 200));
        let mut frame =
            Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).unwrap();
        let mut target = renderer.bind(&mut frame).unwrap();
        OutputDamageTracker::new(
            Size::<i32, smithay::utils::Physical>::from((200, 200)),
            Scale::from(1.0),
            Transform::Normal,
        )
        .render_output(renderer, &mut target, 0, elements, [0.0; 4])
        .unwrap()
        .sync
        .wait()
        .unwrap();
        let mapping = renderer
            .copy_framebuffer(&target, Rectangle::from_size(size), Fourcc::Abgr8888)
            .unwrap();
        renderer.map_texture(&mapping).unwrap().to_vec()
    }

    fn bent_pixels(renderer: &mut GlesRenderer, effect: MinimizeEffect, progress: f32) -> Vec<u8> {
        let program = compile(renderer).expect("the minimize shader compiles");
        let minimizing = red_minimizing(renderer, rect(90, 180, 20, 20));
        let course = Course::new(minimizing.from, minimizing.to, Size::from((200, 200)));
        let shape = course.shape(effect, progress);
        let element = bend(&minimizing, (&course, &shape), program, 1.0);
        draw(renderer, &[PrimaryGpu(element).into()])
    }

    fn red_minimizing(renderer: &mut GlesRenderer, to: Rectangle<i32, Logical>) -> Minimizing {
        let snapshot = red_window(renderer);
        Minimizing {
            window: WindowId::new(1),
            texture: TextureBuffer::from_texture(renderer, snapshot, 1, Transform::Normal, None),
            thumbnail: None,
            from: rect(40, 20, 120, 80),
            to,
            hiding: true,
            start: Instant::now(),
        }
    }

    fn red_at(pixels: &[u8], x: usize, y: usize) -> bool {
        let offset = (y * 200 + x) * 4;
        pixels[offset] > 200 && pixels[offset + 3] > 200
    }

    #[test]
    fn the_genie_starts_as_the_window_and_pours_into_the_target() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let start = bent_pixels(&mut renderer, MinimizeEffect::Genie, 0.0);
        assert!(red_at(&start, 45, 25) && red_at(&start, 155, 95));
        assert!(!red_at(&start, 100, 150));

        let middle = bent_pixels(&mut renderer, MinimizeEffect::Genie, 0.6);
        assert!(!red_at(&middle, 45, 25), "the top of the window has left");
        assert!(
            red_at(&middle, 100, 155),
            "the window flows toward the target"
        );
        assert!(!red_at(&middle, 45, 155), "it narrows toward the target");

        let end = bent_pixels(&mut renderer, MinimizeEffect::Genie, 1.0);
        assert!(end.chunks(4).all(|pixel| pixel[3] == 0), "nothing is left");
    }

    #[test]
    fn the_window_travels_toward_the_screen_edge_nearest_its_target() {
        let output = Size::from((1920, 1080));
        for (target, side) in [
            (rect(860, 1040, 60, 40), Side::Bottom),
            (rect(0, 480, 40, 60), Side::Left),
            (rect(1880, 520, 40, 60), Side::Right),
            (rect(940, 0, 60, 30), Side::Top),
            (rect(910, 1080, 100, 50), Side::Bottom),
        ] {
            assert_eq!(nearest_side(target, output), side, "{target:?}");
        }
    }

    #[test]
    fn in_the_travel_frame_the_target_always_lies_past_the_window() {
        let window = rect(300, 200, 900, 500);
        let output = Size::from((1920, 1080));
        for target in [
            rect(860, 1040, 60, 40),
            rect(0, 480, 40, 60),
            rect(1880, 520, 40, 60),
            rect(940, 0, 60, 30),
        ] {
            let course = Course::new(window, target, output);
            let (near, far) = (course.window.depth[0], course.target.depth[1]);
            assert!(far > near, "{:?}: {near} {far}", course.side);
            let ends = [course.window, course.target]
                .into_iter()
                .flat_map(|print| print.breadth.into_iter().chain(print.depth));
            assert!(ends.into_iter().all(|end| (0.0..=1.0).contains(&end)));
        }
    }

    #[test]
    fn the_thumbnail_is_as_tall_as_the_icon_body_and_keeps_the_window_shape() {
        let slot = rect(900, 1000, 56, 56);
        let body = (56.0 * ctdesign::DOCK_ICON_BODY).round() as i32;
        let wide = thumbnail_rect(slot, Size::from((1600, 1000)), Side::Bottom);
        assert_eq!(wide.size.h, body);
        assert_eq!(wide.size.w, (f64::from(body) * 1.6).round() as i32);
        assert_eq!(wide.loc.x + wide.size.w / 2, 928);
        let side = thumbnail_rect(slot, Size::from((1600, 1000)), Side::Left);
        assert_eq!(side.size.w, body);
    }

    #[test]
    fn the_share_that_has_entered_goes_from_nothing_to_the_whole_window() {
        let course = Course::new(
            rect(300, 100, 900, 500),
            rect(700, 980, 120, 80),
            Size::from((1920, 1080)),
        );
        for effect in [MinimizeEffect::Genie, MinimizeEffect::Suck] {
            let shares: Vec<f32> = (0..=10)
                .map(|tenth| entered(&course.shape(effect, tenth as f32 / 10.0)))
                .collect();
            assert_eq!(shares[0], 0.0);
            assert!((shares[10] - 1.0).abs() < 1e-5, "{shares:?}");
            assert!(
                shares.windows(2).all(|pair| pair[0] <= pair[1]),
                "{shares:?}"
            );
            assert!(shares[9] > 0.0 && shares[9] < 1.0, "{shares:?}");
        }
    }

    #[test]
    fn the_thumbnail_fills_from_the_dock_edge_and_has_square_corners() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let snapshot = red_window(&mut renderer);
        let minimizing = Minimizing {
            window: WindowId::new(1),
            texture: TextureBuffer::from_texture(&renderer, snapshot, 1, Transform::Normal, None),
            thumbnail: Some(Size::from((120.0, 80.0))),
            from: rect(40, 20, 120, 80),
            to: rect(60, 140, 60, 40),
            hiding: true,
            start: Instant::now(),
        };
        let course = Course::new(minimizing.from, minimizing.to, Size::from((200, 200)));
        let built = |renderer: &mut GlesRenderer, progress: f32| {
            let shape = course.shape(MinimizeEffect::Genie, progress);
            let element = built_thumbnail(&minimizing, (&course, &shape), 1.0);
            draw(
                renderer,
                &element.into_iter().map(Into::into).collect::<Vec<_>>(),
            )
        };
        assert!(
            built(&mut renderer, 0.0)
                .chunks(4)
                .all(|pixel| pixel[3] == 0),
            "nothing yet"
        );
        let half = built(&mut renderer, 0.9);
        assert!(!red_at(&half, 90, 142), "the top is still outside");
        assert!(red_at(&half, 90, 176), "the bottom came in first");
        let full = built(&mut renderer, 1.0);
        assert!(
            red_at(&full, 90, 142) && red_at(&full, 90, 176),
            "all of it is in"
        );
        assert!(
            red_at(&full, 60, 140) && red_at(&full, 119, 179),
            "the corners are square"
        );
    }

    #[test]
    fn the_genie_never_leaves_the_width_of_the_thumbnail_inside_the_dock() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let program = compile(&mut renderer).expect("the minimize shader compiles");
        let to = rect(80, 150, 40, 30);
        let minimizing = red_minimizing(&mut renderer, to);
        let course = Course::new(minimizing.from, to, Size::from((200, 200)));
        for step in 1..=19 {
            let shape = course.shape(MinimizeEffect::Genie, step as f32 / 20.0);
            let element = bend(&minimizing, (&course, &shape), program.clone(), 1.0);
            let pixels = draw(&mut renderer, &[PrimaryGpu(element).into()]);
            for y in to.loc.y..to.loc.y + to.size.h {
                for x in (0..to.loc.x - 1).chain(to.loc.x + to.size.w + 1..200) {
                    assert!(
                        !red_at(&pixels, x as usize, y as usize),
                        "step {step}: the genie left the thumbnail at ({x}, {y})"
                    );
                }
            }
        }
    }
}
