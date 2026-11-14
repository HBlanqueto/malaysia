use std::collections::HashMap;
use std::sync::Mutex;

use malaysia_scene::scene::WindowId;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{Bind, Frame, Offscreen, Renderer, Texture};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{
    Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};
use smithay::wayland::background_effect::BackgroundEffectSurfaceCachedState;
use smithay::wayland::compositor::{RectangleKind, with_states};
use smithay::wayland::shell::wlr_layer::Layer;

use crate::cursor::OutputRenderElement;
use crate::effects::RoundedTextureElement;
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::{
    LayerSide, Placement, append_buffer, append_layers, append_window_frame, background_of,
    physical_rect,
};
use crate::state::State;

// ADR 0071: dual Kawase (Bjørge, SIGGRAPH 2015), blurred again only when what it samples changes.
// `wallpaper` is the Background layer; `glass` is the same, blurred wider for the sidebars (ADR 0078); `behind` adds the windows, for the Dock's glass.
pub(crate) struct Blur {
    down: GlesTexProgram,
    up: GlesTexProgram,
    wallpaper: Option<Blurred>,
    glass: Option<Blurred>,
    behind: Option<Blurred>,
}

struct Blurred {
    tracker: OutputDamageTracker,
    scale: f64,
    source: GlesTexture,
    levels: Vec<GlesTexture>,
    blurred: GlesTexture,
    commit: CommitCounter,
}

#[derive(Default)]
struct BackdropIds(Mutex<HashMap<(usize, usize), Id>>);

pub(crate) fn compile(renderer: &mut GlesRenderer) -> Option<Blur> {
    let uniforms = [
        UniformName::new("half_pixel", UniformType::_2f),
        UniformName::new("offset", UniformType::_1f),
    ];
    let mut program = |shader| {
        renderer
            .compile_custom_texture_shader(shader, &uniforms)
            .inspect_err(|error| tracing::warn!(?error, "failed to compile blur shader"))
            .ok()
    };
    Some(Blur {
        down: program(grade_graphics::shaders::BLUR_DOWN)?,
        up: program(grade_graphics::shaders::BLUR_UP)?,
        wallpaper: None,
        glass: None,
        behind: None,
    })
}

pub(crate) fn drawn(state: &State) -> bool {
    !state.config.appearance.transparency_reduced()
}

pub(crate) fn update(
    state: &mut State,
    renderer: &mut GlesRenderer,
    output_size: Size<i32, Physical>,
) {
    let Some(mut blur) = state.blur.take() else {
        return;
    };
    if !drawn(state) {
        blur.wallpaper = None;
        blur.glass = None;
    } else if (requested(state) || glass_requested(state))
        && blur.refresh(state, renderer, output_size).is_none()
    {
        tracing::warn!("failed to blur the wallpaper");
        blur.wallpaper = None;
        blur.glass = None;
    }
    if !drawn(state) || !behind_requested(state) {
        blur.behind = None;
    }
    state.blur = Some(blur);
    if drawn(state) && behind_requested(state) {
        refresh_behind(state, renderer, output_size);
    }
}

pub(crate) fn backdrop<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    frame: Rectangle<i32, Physical>,
    radius: i32,
    exponent: f32,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    sampled(
        state,
        surface,
        origin,
        scale,
        state.rounded_clip.clone(),
        Vec::new(),
        0,
        Detail::Blurred,
        |dst| Some((dst.intersection(frame)?, frame, radius, exponent)),
    )
}

// One pixel wider than the bar: the outline lies outside it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dock<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    frame: Rectangle<i32, Physical>,
    radius: i32,
    exponent: f32,
    scale: f64,
    uniforms: Vec<Uniform<'static>>,
    part: usize,
) -> Vec<OutputRenderElement<R>> {
    let ring = Rectangle::new(
        (frame.loc.x - 1, frame.loc.y - 1).into(),
        (frame.size.w + 2, frame.size.h + 2).into(),
    );
    sampled(
        state,
        surface,
        origin,
        scale,
        state.dock_glass.clone(),
        uniforms,
        part,
        Detail::Sharp,
        |dst| {
            dst.overlaps(frame)
                .then_some((ring, frame, radius, exponent))
        },
    )
}

// ADR 0075: a rectangle shorter than the menu bar is its selection pill, rounded like it.
pub(crate) fn layer<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    frame: Rectangle<i32, Physical>,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    let pill = (f64::from(ctdesign::MENU_LIGHT.bar_title_open_radius) * scale).round() as i32;
    sampled(
        state,
        surface,
        origin,
        scale,
        state.rounded_clip.clone(),
        Vec::new(),
        0,
        Detail::Blurred,
        |dst| {
            let dst = dst.intersection(frame)?;
            let radius = if dst.size.h < frame.size.h {
                pill.min(dst.size.h / 2)
            } else {
                0
            };
            Some((dst, dst, radius, CIRCLE_EXPONENT))
        },
    )
}

// ADR 0074: the lock screen's blur rectangles are its capsule-shaped fields.
pub(crate) fn capsules<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    sampled(
        state,
        surface,
        Point::default(),
        scale,
        state.rounded_clip.clone(),
        Vec::new(),
        0,
        Detail::Blurred,
        |dst| Some((dst, dst, dst.size.w.min(dst.size.h) / 2, CIRCLE_EXPONENT)),
    )
}

const CIRCLE_EXPONENT: f32 = 2.0;

#[derive(Clone, Copy)]
enum Detail {
    Blurred,
    Sharp,
}

#[allow(clippy::too_many_arguments)]
fn sampled<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    scale: f64,
    program: Option<GlesTexProgram>,
    uniforms: Vec<Uniform<'static>>,
    part: usize,
    detail: Detail,
    shape: impl Fn(
        Rectangle<i32, Physical>,
    ) -> Option<(Rectangle<i32, Physical>, Rectangle<i32, Physical>, i32, f32)>,
) -> Vec<OutputRenderElement<R>> {
    let (Some(wallpaper), Some(program)) = (
        state
            .blur
            .as_ref()
            .and_then(|blur| sampled_cache(blur, surface))
            .filter(|_| drawn(state)),
        program,
    ) else {
        return Vec::new();
    };
    let rects = blur_region(surface);
    let ids = with_states(surface, |states| {
        let ids = states
            .data_map
            .get_or_insert_threadsafe(BackdropIds::default);
        let mut ids = ids
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (0..rects.len())
            .map(|index| ids.entry((part, index)).or_insert_with(Id::new).clone())
            .collect::<Vec<_>>()
    });
    let texture = match detail {
        Detail::Blurred => &wallpaper.blurred,
        Detail::Sharp => &wallpaper.source,
    };
    let size = texture.size();
    let output = Rectangle::from_size(Size::<i32, Physical>::from((size.w, size.h)));
    rects
        .into_iter()
        .zip(ids)
        .filter_map(|(rect, id)| {
            let (dst, frame, radius, exponent) = shape(physical_rect(
                Rectangle::new(origin + rect.loc, rect.size),
                scale,
            ))?;
            let dst = dst.intersection(output)?;
            let src = Rectangle::<f64, BufferCoords>::new(
                (f64::from(dst.loc.x), f64::from(dst.loc.y)).into(),
                (f64::from(dst.size.w), f64::from(dst.size.h)).into(),
            );
            Some(
                RoundedTextureElement::from_texture(
                    texture.clone(),
                    id,
                    wallpaper.commit,
                    src,
                    dst,
                    frame,
                    radius,
                    exponent,
                    program.clone(),
                )
                .with_uniforms(uniforms.clone())
                .into(),
            )
        })
        .collect()
}

// ADR 0071: the Dock's glass shows the windows that pass behind it; if that cache failed, the wallpaper.
fn sampled_cache<'a>(blur: &'a Blur, surface: &WlSurface) -> Option<&'a Blurred> {
    let behind = crate::window_style::is_dock_layer(surface)
        .then_some(blur.behind.as_ref())
        .flatten();
    behind.or(blur.wallpaper.as_ref())
}

fn blur_region(surface: &WlSurface) -> Vec<Rectangle<i32, Logical>> {
    with_states(surface, |states| {
        if !states
            .cached_state
            .has::<BackgroundEffectSurfaceCachedState>()
        {
            return Vec::new();
        }
        let mut cached = states
            .cached_state
            .get::<BackgroundEffectSurfaceCachedState>();
        let Some(region) = cached.current().blur_region.as_ref() else {
            return Vec::new();
        };
        region
            .rects
            .iter()
            .fold(Vec::new(), |mut rects, (kind, rect)| match kind {
                RectangleKind::Add => {
                    rects.push(*rect);
                    rects
                }
                RectangleKind::Subtract => Rectangle::subtract_rects_many(rects, [*rect]),
            })
    })
}

pub(crate) fn glass_texture(state: &State) -> Option<(GlesTexture, CommitCounter)> {
    let glass = state.blur.as_ref()?.glass.as_ref()?;
    drawn(state).then(|| (glass.blurred.clone(), glass.commit))
}

fn wants_glass(state: &State, window: WindowId) -> bool {
    state
        .windows
        .surface_of(window)
        .is_some_and(|surface| crate::window_style::glass(surface).is_some())
}

// A window that is about to minimize is already hidden, but its snapshot still needs the glass.
fn glass_requested(state: &State) -> bool {
    state
        .windows
        .iter_visible(&state.minimized)
        .any(|window| wants_glass(state, window.id()))
        || state
            .minimize_requests
            .iter()
            .any(|request| wants_glass(state, request.window))
}

fn requested(state: &State) -> bool {
    let windows = state
        .windows
        .iter_visible(&state.minimized)
        .any(|window| !blur_region(window.surface()).is_empty());
    windows
        || [Layer::Background, Layer::Bottom, Layer::Top, Layer::Overlay]
            .into_iter()
            .flat_map(|layer| state.layers.on_layer(layer))
            .any(|entry| !blur_region(entry.surface.wl_surface()).is_empty())
        || state
            .lock_surfaces
            .iter()
            .any(|surface| !blur_region(surface.wl_surface()).is_empty())
}

fn behind_requested(state: &State) -> bool {
    let dock_hidden = state.space_slide.is_none()
        && state.layers.chrome_hidden()
        && !state.layers.dock_revealed();
    !state.locked
        && !dock_hidden
        && state.layers.on_layer(Layer::Top).any(|entry| {
            crate::window_style::is_dock_layer(entry.surface.wl_surface())
                && !blur_region(entry.surface.wl_surface()).is_empty()
        })
}

impl Blur {
    fn refresh(
        &mut self,
        state: &mut State,
        renderer: &mut GlesRenderer,
        output_size: Size<i32, Physical>,
    ) -> Option<()> {
        let scale = state.scale.fractional_scale();
        let elements = wallpaper_elements(state, renderer, scale);
        let programs = (&self.down, &self.up);
        render_blurred(
            &mut self.wallpaper,
            programs,
            renderer,
            (output_size, scale, ctdesign::BACKGROUND_BLUR_PASSES),
            &elements,
        )?;
        if !glass_requested(state) {
            self.glass = None;
            return Some(());
        }
        render_blurred(
            &mut self.glass,
            programs,
            renderer,
            (
                output_size,
                scale,
                ctappearance::glass_blur_passes(state.config.appearance.color_scheme),
            ),
            &elements,
        )
    }
}

fn refresh_behind(
    state: &mut State,
    renderer: &mut GlesRenderer,
    output_size: Size<i32, Physical>,
) {
    let scale = state.scale.fractional_scale();
    let elements = behind_elements(state, renderer, scale);
    let Some(mut blur) = state.blur.take() else {
        return;
    };
    if render_blurred(
        &mut blur.behind,
        (&blur.down, &blur.up),
        renderer,
        (output_size, scale, ctdesign::BACKGROUND_BLUR_PASSES),
        &elements,
    )
    .is_none()
    {
        tracing::warn!("failed to blur what is behind the Dock");
        blur.behind = None;
    }
    state.blur = Some(blur);
}

fn render_blurred(
    cache: &mut Option<Blurred>,
    (down, up): (&GlesTexProgram, &GlesTexProgram),
    renderer: &mut GlesRenderer,
    (output_size, scale, passes): (Size<i32, Physical>, f64, u32),
    elements: &[OutputRenderElement<GlesRenderer>],
) -> Option<()> {
    let stale = cache.as_ref().is_none_or(|blurred| {
        blurred.scale != scale
            || blurred.levels.len() != passes as usize
            || blurred.source.size() != Size::from((output_size.w, output_size.h))
    });
    if stale {
        *cache = Some(Blurred::new(renderer, output_size, scale, passes)?);
    }
    let blurred = cache.as_mut()?;
    let changed = {
        let mut target = renderer.bind(&mut blurred.source).ok()?;
        blurred
            .tracker
            .render_output(
                renderer,
                &mut target,
                1,
                elements,
                ctdesign::DESKTOP_CLEAR.premultiplied(),
            )
            .ok()?
            .damage
            .is_some_and(|damage| !damage.is_empty())
    };
    if changed {
        blurred.blur(renderer, down, up)?;
        blurred.commit.increment();
    }
    Some(())
}

fn behind_elements(
    state: &mut State,
    renderer: &mut GlesRenderer,
    scale: f64,
) -> Vec<OutputRenderElement<GlesRenderer>> {
    let windows: Vec<_> = state
        .windows
        .iter_visible(&state.minimized)
        .rev()
        .map(|window| window.id())
        .filter(|id| !state.is_minimizing(*id))
        .collect();
    let focused = state.focused_window();
    let mut elements = Vec::new();
    for id in windows {
        append_window_frame(
            state,
            renderer,
            id,
            focused,
            scale,
            Placement::at(Point::default()),
            &mut elements,
        );
    }
    elements.extend(wallpaper_elements(state, renderer, scale));
    elements
}

fn wallpaper_elements(
    state: &mut State,
    renderer: &mut GlesRenderer,
    scale: f64,
) -> Vec<OutputRenderElement<GlesRenderer>> {
    let mut elements = Vec::new();
    if state.locked {
        crate::lock_transition::append_wallpaper(state, renderer, 1.0, &mut elements);
        return elements;
    }
    let focus_space = matches!(
        state.spaces.current().kind,
        malaysia_scene::spaces::SpaceKind::Focus { .. }
    );
    if focus_space {
        let size = state.logical_output_size();
        append_buffer(
            &mut state.focus_backdrop,
            Point::default(),
            size,
            ctdesign::SPACE_GAP_COLOR.premultiplied(),
            scale,
            &mut elements,
        );
    } else {
        let desktop = state.spaces.desktop_number(state.spaces.current().id);
        let shows = background_of(state, desktop);
        append_layers(
            state,
            renderer,
            &mut elements,
            LayerSide::Back,
            Point::default(),
            move |entry| entry.layer == Layer::Background && shows(entry),
        );
    }
    elements
}

impl Blurred {
    fn new(
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        scale: f64,
        passes: u32,
    ) -> Option<Self> {
        let full = Size::<i32, BufferCoords>::from((size.w, size.h));
        let mut texture =
            |size| Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).ok();
        let levels = (1..=passes)
            .map(|pass| {
                texture(Size::from((
                    (full.w >> pass).max(1),
                    (full.h >> pass).max(1),
                )))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            tracker: OutputDamageTracker::new(size, Scale::from(scale), Transform::Normal),
            scale,
            source: texture(full)?,
            levels,
            blurred: texture(full)?,
            commit: CommitCounter::default(),
        })
    }

    fn blur(
        &mut self,
        renderer: &mut GlesRenderer,
        down: &GlesTexProgram,
        up: &GlesTexProgram,
    ) -> Option<()> {
        for level in 0..self.levels.len() {
            let (done, rest) = self.levels.split_at_mut(level);
            pass(
                renderer,
                down,
                done.last().unwrap_or(&self.source),
                &mut rest[0],
            )?;
        }
        for level in (1..self.levels.len()).rev() {
            let (lower, upper) = self.levels.split_at_mut(level);
            pass(renderer, up, &upper[0], &mut lower[level - 1])?;
        }
        pass(renderer, up, self.levels.first()?, &mut self.blurred)
    }
}

fn pass(
    renderer: &mut GlesRenderer,
    program: &GlesTexProgram,
    from: &GlesTexture,
    to: &mut GlesTexture,
) -> Option<()> {
    let from_size = from.size();
    let to_size = to.size();
    let dst = Rectangle::from_size(Size::<i32, Physical>::from((to_size.w, to_size.h)));
    let half_pixel = [0.5 / from_size.w as f32, 0.5 / from_size.h as f32];
    let mut target = renderer.bind(to).ok()?;
    let mut frame = renderer
        .render(&mut target, dst.size, Transform::Normal)
        .ok()?;
    frame.clear([0.0; 4].into(), &[dst]).ok()?;
    frame
        .render_texture_from_to(
            from,
            Rectangle::from_size(from_size.to_f64()),
            dst,
            &[dst],
            &[],
            Transform::Normal,
            1.0,
            Some(program),
            &[
                Uniform::new("half_pixel", half_pixel),
                Uniform::new("offset", ctdesign::BACKGROUND_BLUR_OFFSET),
            ],
        )
        .ok()?;
    // The passes and the draw that samples them share one GL context, which orders them.
    frame.finish().ok().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::try_test_renderer;
    use smithay::backend::renderer::ExportMem;
    use smithay::backend::renderer::element::Kind;
    use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};

    const SIDE: i32 = 128;
    fn output() -> Size<i32, Physical> {
        Size::from((SIDE, SIDE))
    }

    fn solid(x: i32, size: i32, color: [f32; 4]) -> OutputRenderElement<GlesRenderer> {
        SolidColorRenderElement::from_buffer(
            &SolidColorBuffer::new(Size::from((size, size)), color),
            Point::from((x, x)),
            1.0,
            1.0,
            Kind::Unspecified,
        )
        .into()
    }

    fn center_pixel(renderer: &mut GlesRenderer, blurred: &mut Blurred) -> [u8; 4] {
        let size = Size::<i32, BufferCoords>::from((SIDE, SIDE));
        let target = renderer.bind(&mut blurred.blurred).unwrap();
        let mapping = renderer
            .copy_framebuffer(&target, Rectangle::from_size(size), Fourcc::Abgr8888)
            .unwrap();
        let bytes = renderer.map_texture(&mapping).unwrap();
        let at = ((SIDE / 2 * SIDE + SIDE / 2) * 4) as usize;
        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
    }

    #[test]
    fn what_is_behind_the_dock_shows_a_window_and_blurs_again_only_when_it_changes() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let blur = compile(&mut renderer).expect("the blur shaders compile");
        let wallpaper = || solid(0, 128, [0.0, 0.0, 1.0, 1.0]);
        let programs = (&blur.down, &blur.up);
        let mut cache = None;

        let with_window = [solid(32, 64, [1.0, 0.0, 0.0, 1.0]), wallpaper()];
        render_blurred(
            &mut cache,
            programs,
            &mut renderer,
            (output(), 1.0, ctdesign::BACKGROUND_BLUR_PASSES),
            &with_window,
        )
        .unwrap();
        let blurred = cache.as_mut().unwrap();
        let commit = blurred.commit;
        let [red, _, blue, _] = center_pixel(&mut renderer, blurred);
        assert!(red > blue, "the window is under the glass: {red} {blue}");

        render_blurred(
            &mut cache,
            programs,
            &mut renderer,
            (output(), 1.0, ctdesign::BACKGROUND_BLUR_PASSES),
            &with_window,
        )
        .unwrap();
        assert_eq!(cache.as_ref().unwrap().commit, commit, "nothing changed");

        render_blurred(
            &mut cache,
            programs,
            &mut renderer,
            (output(), 1.0, ctdesign::BACKGROUND_BLUR_PASSES),
            &[wallpaper()],
        )
        .unwrap();
        let blurred = cache.as_mut().unwrap();
        assert_ne!(blurred.commit, commit, "the window left");
        let [red, _, blue, _] = center_pixel(&mut renderer, blurred);
        assert!(blue > red, "only the wallpaper is left: {red} {blue}");
    }
}
