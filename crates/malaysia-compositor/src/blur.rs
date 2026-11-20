use std::collections::HashMap;
use std::sync::Mutex;

use malaysia_scene::geometry::{Logical as SceneLogical, Rect as SceneRect};
use malaysia_scene::scene::WindowId;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::{Id, RenderElement};
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{Bind, ExportMem, Frame, Offscreen, Renderer, Texture};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{
    Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};
use smithay::wayland::background_effect::BackgroundEffectSurfaceCachedState;
use smithay::wayland::compositor::{RectangleKind, with_states};
use smithay::wayland::shell::wlr_layer::Layer;

use crate::cursor::OutputRenderElement;
use crate::decoration::Role;
use crate::effects::{RoundedTextureElement, Shape};
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::{
    LayerSide, Placement, append_buffer, append_layers, append_window_frame, background_of,
    physical_rect,
};
use crate::state::State;

pub(crate) struct Blur {
    down: GlesTexProgram,
    up: GlesTexProgram,
    soften: GlesTexProgram,
    wallpaper: Option<Blurred>,
    glass: Option<Blurred>,
    dialogs: Option<Blurred>,
    behind: [Option<Behind>; GLASSES.len()],
    veil: Option<Behind>,
    probe: Option<GlesTexture>,
    probed: CommitCounter,
    popups_backdrop: Option<ctdesign::Rgba>,
}

const PROBE_SIDE: i32 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Glass {
    Dock,
    Notifications,
    Popups,
}

const GLASSES: [Glass; 3] = [Glass::Dock, Glass::Notifications, Glass::Popups];

pub(crate) fn material(resolved: &ctappearance::Resolved, glass: Glass) -> ctdesign::DockSurface {
    match glass {
        Glass::Dock => resolved.dock,
        Glass::Notifications => resolved.notification_glass,
        Glass::Popups => resolved.menu_glass,
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Kawase {
    passes: u32,
    offset: f32,
}

impl Kawase {
    fn reaching(passes: u32, reach: f32) -> Self {
        let deeper = reach.log2().round().max(0.0) as u32;
        Self {
            passes: passes + deeper,
            offset: ctdesign::BACKGROUND_BLUR_OFFSET * reach / (1u32 << deeper) as f32,
        }
    }
}

struct Blurred {
    tracker: OutputDamageTracker,
    scale: f64,
    kawase: Kawase,
    source: GlesTexture,
    levels: Vec<GlesTexture>,
    blurred: GlesTexture,
    commit: CommitCounter,
}

struct Behind {
    tracker: OutputDamageTracker,
    scale: f64,
    area: Rectangle<i32, Physical>,
    source: GlesTexture,
    across: GlesTexture,
    softened: GlesTexture,
    spread: f32,
    commit: CommitCounter,
}

#[derive(Default)]
struct BackdropIds(Mutex<HashMap<(usize, usize), Id>>);

pub(crate) fn compile(renderer: &mut GlesRenderer) -> Option<Blur> {
    let kawase = [
        UniformName::new("half_pixel", UniformType::_2f),
        UniformName::new("offset", UniformType::_1f),
    ];
    let along = [
        UniformName::new("texel", UniformType::_2f),
        UniformName::new("direction", UniformType::_2f),
        UniformName::new("spread", UniformType::_1f),
    ];
    let mut program = |shader, uniforms: &[UniformName]| {
        renderer
            .compile_custom_texture_shader(shader, uniforms)
            .inspect_err(|error| tracing::warn!(?error, "failed to compile blur shader"))
            .ok()
    };
    Some(Blur {
        down: program(grade_graphics::shaders::BLUR_DOWN, &kawase)?,
        up: program(grade_graphics::shaders::BLUR_UP, &kawase)?,
        soften: program(grade_graphics::shaders::DOCK_SOFTEN, &along)?,
        wallpaper: None,
        glass: None,
        dialogs: None,
        behind: std::array::from_fn(|_| None),
        veil: None,
        probe: None,
        probed: CommitCounter::default(),
        popups_backdrop: None,
    })
}

pub(crate) fn popups_backdrop(state: &State) -> Option<ctdesign::Rgba> {
    state.blur.as_ref()?.popups_backdrop
}

pub(crate) fn drawn(state: &State) -> bool {
    !state.config.appearance.transparency_reduced()
}

pub(crate) fn popups_drawn(state: &State) -> bool {
    !state.config.appearance.contrast
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
        blur.dialogs = None;
    } else if (requested(state) || glass_requested(state))
        && blur.refresh(state, renderer, output_size).is_none()
    {
        tracing::warn!("failed to blur the wallpaper");
        blur.wallpaper = None;
        blur.glass = None;
        blur.dialogs = None;
    }
    if !popups_drawn(state) {
        blur.behind = Default::default();
        blur.veil = None;
        blur.popups_backdrop = None;
    }
    state.blur = Some(blur);
    if popups_drawn(state) {
        refresh_behind(state, renderer, output_size);
    }
}

pub(crate) fn backdrop<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    frame: Shape,
    detail: Detail,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    sampled(
        state,
        surface,
        blur_region(surface),
        origin,
        scale,
        Sampling {
            detail,
            ..Sampling::blurred(state)
        },
        |dst| Some((dst.intersection(frame.rect)?, frame)),
    )
}

pub(crate) fn veil<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    veil: Rectangle<i32, Logical>,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    let feather = ctdesign::NOTIFICATION_SHADOW_BLUR.round() as i32;
    let reach = Rectangle::new(
        veil.loc - Point::from((feather, feather)),
        (veil.size.w + 2 * feather, veil.size.h + 2 * feather).into(),
    );
    let shape = Shape {
        rect: physical_rect(Rectangle::new(origin + veil.loc, veil.size), scale),
        radius: (f64::from(feather) * scale).round() as i32,
        exponent: ctdesign::CIRCLE_EXPONENT,
    };
    sampled(
        state,
        surface,
        vec![reach],
        origin,
        scale,
        Sampling {
            program: state.veil.clone(),
            uniforms: Vec::new(),
            part: VEIL_PART,
            detail: Detail::Veil,
            arrow: crate::effects::Arrow::default(),
        },
        |dst| Some((dst, shape)),
    )
}

const VEIL_PART: usize = usize::MAX;

pub(crate) fn outlined(frame: Rectangle<i32, Physical>) -> Rectangle<i32, Physical> {
    Rectangle::new(
        (frame.loc.x - 1, frame.loc.y - 1).into(),
        (frame.size.w + 2, frame.size.h + 2).into(),
    )
}

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
        blur_region(surface),
        origin,
        scale,
        Sampling::blurred(state),
        |dst| {
            let dst = dst.intersection(frame)?;
            let radius = if dst.size.h < frame.size.h {
                pill.min(dst.size.h / 2)
            } else {
                0
            };
            Some((dst, circle(dst, radius)))
        },
    )
}

pub(crate) fn capsules<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    sampled(
        state,
        surface,
        blur_region(surface),
        Point::default(),
        scale,
        Sampling::blurred(state),
        |dst| Some((dst, circle(dst, dst.size.w.min(dst.size.h) / 2))),
    )
}

fn circle(rect: Rectangle<i32, Physical>, radius: i32) -> Shape {
    Shape {
        rect,
        radius,
        exponent: ctdesign::CIRCLE_EXPONENT,
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Detail {
    Blurred,
    Dialogs,
    Behind(Glass),
    Veil,
}

pub(crate) struct Sampling {
    pub(crate) program: Option<GlesTexProgram>,
    pub(crate) uniforms: Vec<Uniform<'static>>,
    pub(crate) part: usize,
    pub(crate) detail: Detail,
    pub(crate) arrow: crate::effects::Arrow,
}

impl Sampling {
    fn blurred(state: &State) -> Self {
        Self {
            program: state.rounded_clip.clone(),
            uniforms: Vec::new(),
            part: 0,
            detail: Detail::Blurred,
            arrow: crate::effects::Arrow::default(),
        }
    }
}

pub(crate) fn backdrop_ids(surface: &WlSurface, part: usize, count: usize) -> Vec<Id> {
    with_states(surface, |states| {
        let ids = states
            .data_map
            .get_or_insert_threadsafe(BackdropIds::default);
        let mut ids = ids
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (0..count)
            .map(|index| ids.entry((part, index)).or_insert_with(Id::new).clone())
            .collect()
    })
}

pub(crate) fn sampled<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    rects: Vec<Rectangle<i32, Logical>>,
    origin: Point<i32, Logical>,
    scale: f64,
    sampling: Sampling,
    shape: impl Fn(Rectangle<i32, Physical>) -> Option<(Rectangle<i32, Physical>, Shape)>,
) -> Vec<OutputRenderElement<R>> {
    let (Some((texture, commit, corner)), Some(program)) = (
        state
            .blur
            .as_ref()
            .and_then(|blur| sampled_cache(blur, sampling.detail))
            .filter(|_| match sampling.detail {
                Detail::Behind(Glass::Popups) => popups_drawn(state),
                _ => drawn(state),
            }),
        sampling.program,
    ) else {
        return Vec::new();
    };
    let ids = backdrop_ids(surface, sampling.part, rects.len());
    let size = texture.size();
    let output = Rectangle::new(corner, Size::<i32, Physical>::from((size.w, size.h)));
    rects
        .into_iter()
        .zip(ids)
        .filter_map(|(rect, id)| {
            let (dst, frame) = shape(physical_rect(
                Rectangle::new(origin + rect.loc, rect.size),
                scale,
            ))?;
            let dst = dst.intersection(output)?;
            let src = Rectangle::<f64, BufferCoords>::new(
                (
                    f64::from(dst.loc.x - corner.x),
                    f64::from(dst.loc.y - corner.y),
                )
                    .into(),
                (f64::from(dst.size.w), f64::from(dst.size.h)).into(),
            );
            Some(
                RoundedTextureElement::from_texture(
                    texture.clone(),
                    id,
                    commit,
                    src,
                    dst,
                    frame,
                    program.clone(),
                )
                .with_arrow(sampling.arrow)
                .with_uniforms(sampling.uniforms.clone())
                .into(),
            )
        })
        .collect()
}

fn sampled_cache(
    blur: &Blur,
    detail: Detail,
) -> Option<(&GlesTexture, CommitCounter, Point<i32, Physical>)> {
    match detail {
        Detail::Blurred => blur
            .wallpaper
            .as_ref()
            .map(|wallpaper| (&wallpaper.blurred, wallpaper.commit, Point::default())),
        Detail::Dialogs => blur
            .dialogs
            .as_ref()
            .or(blur.wallpaper.as_ref())
            .map(|blurred| (&blurred.blurred, blurred.commit, Point::default())),
        Detail::Behind(glass) => blur.behind[glass as usize]
            .as_ref()
            .map(|behind| (&behind.softened, behind.commit, behind.area.loc)),
        Detail::Veil => blur
            .veil
            .as_ref()
            .map(|veil| (&veil.softened, veil.commit, veil.area.loc)),
    }
}

pub(crate) fn blur_region(surface: &WlSurface) -> Vec<Rectangle<i32, Logical>> {
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

fn glass_requested(state: &State) -> bool {
    state
        .windows
        .iter_visible(&state.minimized)
        .any(|window| wants_glass(state, window.id()))
        || state
            .minimize_requests
            .iter()
            .any(|request| wants_glass(state, request.window))
        || state
            .leaving_focus_window()
            .is_some_and(|window| wants_glass(state, window))
}

fn dialogs_requested(state: &State) -> bool {
    state
        .windows
        .iter_visible(&state.minimized)
        .any(|window| window.role() == Role::Dialog && !blur_region(window.surface()).is_empty())
        || state.layers.mapped().iter().any(|entry| {
            let surface = entry.surface.wl_surface();
            crate::window_style::alert_layer(surface).is_some() && !blur_region(surface).is_empty()
        })
}

fn requested(state: &State) -> bool {
    let windows = state
        .windows
        .iter_visible(&state.minimized)
        .any(|window| !blur_region(window.surface()).is_empty());
    windows
        || state
            .leaving_focus_window()
            .and_then(|window| state.windows.surface_of(window))
            .is_some_and(|surface| !blur_region(surface).is_empty())
        || [Layer::Background, Layer::Bottom, Layer::Top, Layer::Overlay]
            .into_iter()
            .flat_map(|layer| state.layers.on_layer(layer))
            .any(|entry| !blur_region(entry.surface.wl_surface()).is_empty())
        || state
            .lock_surfaces
            .iter()
            .any(|surface| !blur_region(surface.wl_surface()).is_empty())
}

pub(crate) fn glass_of(surface: &WlSurface) -> Option<Glass> {
    if crate::window_style::is_dock_layer(surface) {
        Some(Glass::Dock)
    } else if crate::notification_panel::marked(surface) {
        Some(Glass::Notifications)
    } else {
        None
    }
}

fn glass_popups(state: &State) -> Vec<Rectangle<i32, Logical>> {
    state
        .windows
        .iter_popups()
        .filter(|(id, _, _)| state.popup_has_glass(*id))
        .map(|(_, _, rect)| crate::popup::smithay_rect(rect))
        .collect()
}

fn behind_requested(state: &State, glass: Glass) -> bool {
    let dock_hidden = glass == Glass::Dock
        && state.space_slide.is_none()
        && state.layers.chrome_hidden()
        && !state.layers.dock_shown();
    if state.locked || dock_hidden {
        return false;
    }
    let layer = match glass {
        Glass::Dock => Layer::Top,
        Glass::Notifications => Layer::Overlay,
        Glass::Popups => return !glass_popups(state).is_empty(),
    };
    drawn(state)
        && state.layers.on_layer(layer).any(|entry| {
            glass_of(entry.surface.wl_surface()) == Some(glass)
                && !blur_region(entry.surface.wl_surface()).is_empty()
        })
}

impl Blur {
    fn probe_popups(&mut self, renderer: &mut GlesRenderer) {
        let Some(behind) = self.behind[Glass::Popups as usize].as_ref() else {
            self.popups_backdrop = None;
            return;
        };
        if self.popups_backdrop.is_some() && behind.commit == self.probed {
            return;
        }
        self.probed = behind.commit;
        self.popups_backdrop = average_color(renderer, &behind.softened, &mut self.probe);
    }

    fn refresh(
        &mut self,
        state: &mut State,
        renderer: &mut GlesRenderer,
        output_size: Size<i32, Physical>,
    ) -> Option<()> {
        let scale = state.scale.fractional_scale();
        let elements = wallpaper_elements(state, renderer, scale);
        let programs = (&self.down, &self.up);
        let reach = state.chrome_theme.resolved.dock.soften;
        render_blurred(
            &mut self.wallpaper,
            programs,
            renderer,
            (
                output_size,
                scale,
                Kawase::reaching(ctdesign::BACKGROUND_BLUR_PASSES, 1.0),
            ),
            &elements,
        )?;
        if glass_requested(state) {
            render_blurred(
                &mut self.glass,
                programs,
                renderer,
                (
                    output_size,
                    scale,
                    Kawase::reaching(
                        ctappearance::glass_blur_passes(state.config.appearance.color_scheme),
                        reach,
                    ),
                ),
                &elements,
            )?;
        } else {
            self.glass = None;
        }
        if reach > 1.0 && dialogs_requested(state) {
            render_blurred(
                &mut self.dialogs,
                programs,
                renderer,
                (
                    output_size,
                    scale,
                    Kawase::reaching(ctdesign::BACKGROUND_BLUR_PASSES, reach),
                ),
                &elements,
            )
        } else {
            self.dialogs = None;
            Some(())
        }
    }
}

fn refresh_behind(
    state: &mut State,
    renderer: &mut GlesRenderer,
    output_size: Size<i32, Physical>,
) {
    let scale = state.scale.fractional_scale();
    let areas = GLASSES.map(|glass| {
        if behind_requested(state, glass) {
            behind_area(state, glass, scale, output_size)
        } else {
            None
        }
    });
    let veil_area = areas[Glass::Notifications as usize]
        .filter(|_| crate::notification_panel::open_panel(&state.layers).is_some());
    let glass = areas
        .iter()
        .flatten()
        .copied()
        .reduce(Rectangle::merge)
        .map(|area| area.to_f64().to_logical(scale));
    let elements = if areas.iter().any(Option::is_some) {
        behind_elements(state, renderer, scale, glass)
    } else {
        Vec::new()
    };
    let Some(mut blur) = state.blur.take() else {
        return;
    };
    for ((cache, area), glass) in blur.behind.iter_mut().zip(areas).zip(GLASSES) {
        let Some(area) = area else {
            *cache = None;
            continue;
        };
        if render_behind(
            cache,
            &blur.soften,
            material(&state.chrome_theme.resolved, glass).soften,
            renderer,
            (area, scale),
            &elements,
        )
        .is_none()
        {
            tracing::warn!("failed to draw what is behind the glass");
            *cache = None;
        }
    }
    blur.probe_popups(renderer);
    let veiled = veil_area.and_then(|area| {
        render_behind(
            &mut blur.veil,
            &blur.soften,
            ctdesign::NOTIFICATION_VEIL_SPREAD,
            renderer,
            (area, scale),
            &elements,
        )
    });
    if veiled.is_none() {
        blur.veil = None;
    }
    state.blur = Some(blur);
}

fn behind_area(
    state: &State,
    glass: Glass,
    scale: f64,
    output_size: Size<i32, Physical>,
) -> Option<Rectangle<i32, Physical>> {
    let area = match glass {
        Glass::Popups => {
            let menus = glass_popups(state).into_iter().reduce(Rectangle::merge)?;
            outlined(physical_rect(menus, scale))
        }
        _ => physical_rect(
            state
                .layers
                .mapped()
                .iter()
                .find(|entry| glass_of(entry.surface.wl_surface()) == Some(glass))?
                .geometry,
            scale,
        ),
    };
    area.intersection(Rectangle::from_size(output_size))
        .filter(|area| !area.is_empty())
}

fn render_blurred(
    cache: &mut Option<Blurred>,
    (down, up): (&GlesTexProgram, &GlesTexProgram),
    renderer: &mut GlesRenderer,
    (output_size, scale, kawase): (Size<i32, Physical>, f64, Kawase),
    elements: &[OutputRenderElement<GlesRenderer>],
) -> Option<()> {
    let stale = cache.as_ref().is_none_or(|blurred| {
        blurred.scale != scale
            || blurred.kawase != kawase
            || blurred.source.size() != Size::from((output_size.w, output_size.h))
    });
    if stale {
        *cache = Some(Blurred::new(renderer, output_size, scale, kawase)?);
    }
    let blurred = cache.as_mut()?;
    if redraw(
        &mut blurred.tracker,
        &mut blurred.source,
        renderer,
        elements,
    )? {
        blurred.blur(renderer, down, up)?;
        blurred.commit.increment();
    }
    Some(())
}

fn render_behind(
    cache: &mut Option<Behind>,
    soften: &GlesTexProgram,
    spread: f32,
    renderer: &mut GlesRenderer,
    (area, scale): (Rectangle<i32, Physical>, f64),
    elements: &[OutputRenderElement<GlesRenderer>],
) -> Option<()> {
    let stale = cache.as_ref().is_none_or(|behind| {
        behind.scale != scale || behind.area.size != area.size || behind.spread != spread
    });
    if stale {
        *cache = Some(Behind::new(renderer, area, scale, spread)?);
    }
    let behind = cache.as_mut()?;
    behind.area = area;
    let corner = Point::from((-area.loc.x, -area.loc.y));
    let relocated: Vec<_> = elements
        .iter()
        .map(|element| RelocateRenderElement::from_element(element, corner, Relocate::Relative))
        .collect();
    if redraw(
        &mut behind.tracker,
        &mut behind.source,
        renderer,
        &relocated,
    )? {
        behind.soften(renderer, soften)?;
        behind.commit.increment();
    }
    Some(())
}

fn redraw<E: RenderElement<GlesRenderer>>(
    tracker: &mut OutputDamageTracker,
    source: &mut GlesTexture,
    renderer: &mut GlesRenderer,
    elements: &[E],
) -> Option<bool> {
    let mut target = renderer.bind(source).ok()?;
    let rendered = tracker
        .render_output(
            renderer,
            &mut target,
            1,
            elements,
            ctdesign::DESKTOP_CLEAR.premultiplied(),
        )
        .ok()?;
    Some(rendered.damage.is_some_and(|damage| !damage.is_empty()))
}

fn behind_elements(
    state: &mut State,
    renderer: &mut GlesRenderer,
    scale: f64,
    glass: Option<Rectangle<f64, Logical>>,
) -> Vec<OutputRenderElement<GlesRenderer>> {
    let windows: Vec<_> = state
        .windows
        .iter_visible(&state.minimized)
        .rev()
        .map(|window| window.id())
        .filter(|id| !state.is_minimizing(*id))
        .filter(|id| {
            glass.is_none_or(|area| {
                state
                    .windows
                    .outer(*id)
                    .is_some_and(|outer| behind_the_glass(&outer, area))
            })
        })
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

fn behind_the_glass(outer: &SceneRect<f64, SceneLogical>, area: Rectangle<f64, Logical>) -> bool {
    outer.x() < area.loc.x + area.size.w
        && outer.x() + outer.width() > area.loc.x
        && outer.y() < area.loc.y + area.size.h
        && outer.y() + outer.height() > area.loc.y
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

impl Behind {
    fn new(
        renderer: &mut GlesRenderer,
        area: Rectangle<i32, Physical>,
        scale: f64,
        spread: f32,
    ) -> Option<Self> {
        let size = Size::<i32, BufferCoords>::from((area.size.w, area.size.h));
        let mut texture =
            || Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).ok();
        Some(Self {
            tracker: OutputDamageTracker::new(area.size, Scale::from(scale), Transform::Normal),
            scale,
            area,
            source: texture()?,
            across: texture()?,
            softened: texture()?,
            spread,
            commit: CommitCounter::default(),
        })
    }

    fn soften(&mut self, renderer: &mut GlesRenderer, program: &GlesTexProgram) -> Option<()> {
        let size = self.source.size();
        let texel = [1.0 / size.w as f32, 1.0 / size.h as f32];
        let spread = self.spread;
        let along = |direction: [f32; 2]| {
            [
                Uniform::new("texel", texel),
                Uniform::new("direction", direction),
                Uniform::new("spread", spread),
            ]
        };
        let (source, across) = (&self.source, &mut self.across);
        draw_pass(renderer, Some(program), source, across, &along([1.0, 0.0]))?;
        draw_pass(
            renderer,
            Some(program),
            &self.across,
            &mut self.softened,
            &along([0.0, 1.0]),
        )
    }
}

impl Blurred {
    fn new(
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        scale: f64,
        kawase: Kawase,
    ) -> Option<Self> {
        let full = Size::<i32, BufferCoords>::from((size.w, size.h));
        let mut texture =
            |size| Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).ok();
        let levels = (1..=kawase.passes)
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
            kawase,
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
                self.kawase.offset,
            )?;
        }
        for level in (1..self.levels.len()).rev() {
            let (lower, upper) = self.levels.split_at_mut(level);
            pass(
                renderer,
                up,
                &upper[0],
                &mut lower[level - 1],
                self.kawase.offset,
            )?;
        }
        pass(
            renderer,
            up,
            self.levels.first()?,
            &mut self.blurred,
            self.kawase.offset,
        )
    }
}

fn pass(
    renderer: &mut GlesRenderer,
    program: &GlesTexProgram,
    from: &GlesTexture,
    to: &mut GlesTexture,
    offset: f32,
) -> Option<()> {
    let from_size = from.size();
    let uniforms = [
        Uniform::new(
            "half_pixel",
            [0.5 / from_size.w as f32, 0.5 / from_size.h as f32],
        ),
        Uniform::new("offset", offset),
    ];
    draw_pass(renderer, Some(program), from, to, &uniforms)
}

fn average_color(
    renderer: &mut GlesRenderer,
    from: &GlesTexture,
    probe: &mut Option<GlesTexture>,
) -> Option<ctdesign::Rgba> {
    let side = Size::<i32, BufferCoords>::from((PROBE_SIDE, PROBE_SIDE));
    if probe.is_none() {
        *probe = Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, side).ok();
    }
    let probe = probe.as_mut()?;
    draw_pass(renderer, None, from, probe, &[])?;
    let target = renderer.bind(probe).ok()?;
    let mapping = renderer
        .copy_framebuffer(&target, Rectangle::from_size(side), Fourcc::Abgr8888)
        .ok()?;
    let bytes = renderer.map_texture(&mapping).ok()?;
    let (pixels, _) = bytes.as_chunks::<4>();
    let count = pixels.len() as f32 * 255.0;
    let [red, green, blue] = pixels.iter().fold([0.0; 3], |sum, [r, g, b, _]| {
        [
            sum[0] + f32::from(*r),
            sum[1] + f32::from(*g),
            sum[2] + f32::from(*b),
        ]
    });
    Some(ctdesign::Rgba {
        r: red / count,
        g: green / count,
        b: blue / count,
        a: 1.0,
    })
}

fn draw_pass(
    renderer: &mut GlesRenderer,
    program: Option<&GlesTexProgram>,
    from: &GlesTexture,
    to: &mut GlesTexture,
    uniforms: &[Uniform<'_>],
) -> Option<()> {
    let from_size = from.size();
    let to_size = to.size();
    let dst = Rectangle::from_size(Size::<i32, Physical>::from((to_size.w, to_size.h)));
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
            program,
            uniforms,
        )
        .ok()?;
    frame.finish().ok().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::try_test_renderer;
    use smithay::backend::renderer::element::Kind;
    use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};

    const SIDE: i32 = 128;
    fn output() -> Size<i32, Physical> {
        Size::from((SIDE, SIDE))
    }

    fn whole() -> Rectangle<i32, Physical> {
        Rectangle::from_size(output())
    }

    fn placed(buffer: &SolidColorBuffer, x: i32) -> OutputRenderElement<GlesRenderer> {
        SolidColorRenderElement::from_buffer(
            buffer,
            Point::from((x, x)),
            1.0,
            1.0,
            Kind::Unspecified,
        )
        .into()
    }

    fn solid(x: i32, size: i32, color: [f32; 4]) -> OutputRenderElement<GlesRenderer> {
        placed(&SolidColorBuffer::new(Size::from((size, size)), color), x)
    }

    fn pixel_at(
        renderer: &mut GlesRenderer,
        texture: &mut GlesTexture,
        (x, y): (i32, i32),
    ) -> [u8; 4] {
        let size = Size::<i32, BufferCoords>::from((SIDE, SIDE));
        let target = renderer.bind(texture).unwrap();
        let mapping = renderer
            .copy_framebuffer(&target, Rectangle::from_size(size), Fourcc::Abgr8888)
            .unwrap();
        let bytes = renderer.map_texture(&mapping).unwrap();
        let at = ((y * SIDE + x) * 4) as usize;
        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
    }

    fn center_pixel(renderer: &mut GlesRenderer, blurred: &mut Blurred) -> [u8; 4] {
        pixel_at(renderer, &mut blurred.blurred, (SIDE / 2, SIDE / 2))
    }

    #[test]
    fn reduce_transparency_keeps_the_menu_glass_and_only_increase_contrast_drops_it() {
        use smithay::wayland::background_effect::{Capability, ExtBackgroundEffectHandler};
        let (_loop, _display, mut state) = crate::test_support::test_state((1280, 720));
        for (reduce_transparency, contrast, other_glass, menu_glass) in [
            (false, false, true, true),
            (true, false, false, true),
            (false, true, false, false),
            (true, true, false, false),
        ] {
            state.config.appearance.reduce_transparency = reduce_transparency;
            state.config.appearance.contrast = contrast;
            assert_eq!(drawn(&state), other_glass);
            assert_eq!(popups_drawn(&state), menu_glass);
            assert_eq!(state.capabilities().contains(Capability::Blur), menu_glass);
        }
    }

    #[test]
    fn a_window_is_behind_the_glass_only_when_its_rect_touches_the_area() {
        use malaysia_scene::geometry::{Point as ScenePoint, Size as SceneSize};
        let area = Rectangle::new(Point::from((50.0, 50.0)), Size::from((100.0, 100.0)));
        let touching = SceneRect::new(ScenePoint::new(40.0, 40.0), SceneSize::new(20.0, 20.0));
        assert!(behind_the_glass(&touching, area));
        let left = SceneRect::new(ScenePoint::new(0.0, 60.0), SceneSize::new(40.0, 40.0));
        assert!(!behind_the_glass(&left, area));
        let above = SceneRect::new(ScenePoint::new(60.0, 0.0), SceneSize::new(40.0, 40.0));
        assert!(!behind_the_glass(&above, area));
    }

    #[test]
    fn the_probe_averages_the_color_behind_the_glass() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let side = Size::<i32, BufferCoords>::from((SIDE, SIDE));
        let mut source =
            Offscreen::<GlesTexture>::create_buffer(&mut renderer, Fourcc::Abgr8888, side).unwrap();
        let mut tracker = OutputDamageTracker::new(output(), Scale::from(1.0), Transform::Normal);
        let quarter_white = [
            solid(64, 64, [1.0, 1.0, 1.0, 1.0]),
            solid(0, SIDE, [0.0, 0.0, 0.0, 1.0]),
        ];
        redraw(&mut tracker, &mut source, &mut renderer, &quarter_white).unwrap();
        let average = average_color(&mut renderer, &source, &mut None).unwrap();
        assert!((average.r - 0.25).abs() < 0.02, "{average:?}");
    }

    #[test]
    fn the_kawase_radius_grows_with_the_reach_whether_it_deepens_a_pass_or_widens_the_taps() {
        let passes = ctdesign::BACKGROUND_BLUR_PASSES;
        let radius = |kawase: Kawase| kawase.offset * (1u32 << kawase.passes) as f32;
        let clear = radius(Kawase::reaching(passes, 1.0));
        assert_eq!(Kawase::reaching(passes, 1.0).passes, passes);
        assert_eq!(Kawase::reaching(passes, 1.4).passes, passes);
        assert_eq!(Kawase::reaching(passes, 1.5).passes, passes + 1);
        for reach in [1.0, 1.2, 1.4, 1.5, 2.0, 2.08] {
            let kawase = Kawase::reaching(passes, reach);
            assert!(
                (radius(kawase) - clear * reach).abs() < 1e-3,
                "reach {reach}"
            );
            assert!((0.7..=1.45).contains(&(kawase.offset / ctdesign::BACKGROUND_BLUR_OFFSET)));
        }
    }

    #[test]
    fn the_blur_chain_shows_a_window_and_runs_again_only_when_it_changes() {
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
            (
                output(),
                1.0,
                Kawase::reaching(ctdesign::BACKGROUND_BLUR_PASSES, 1.0),
            ),
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
            (
                output(),
                1.0,
                Kawase::reaching(ctdesign::BACKGROUND_BLUR_PASSES, 1.0),
            ),
            &with_window,
        )
        .unwrap();
        assert_eq!(cache.as_ref().unwrap().commit, commit, "nothing changed");

        render_blurred(
            &mut cache,
            programs,
            &mut renderer,
            (
                output(),
                1.0,
                Kawase::reaching(ctdesign::BACKGROUND_BLUR_PASSES, 1.0),
            ),
            &[wallpaper()],
        )
        .unwrap();
        let blurred = cache.as_mut().unwrap();
        assert_ne!(blurred.commit, commit, "the window left");
        let [red, _, blue, _] = center_pixel(&mut renderer, blurred);
        assert!(blue > red, "only the wallpaper is left: {red} {blue}");
    }

    #[test]
    fn what_is_behind_the_dock_is_drawn_sharp_and_changes_only_when_it_does() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let blur = compile(&mut renderer).expect("the blur shaders compile");
        let wallpaper = || solid(0, 128, [0.0, 0.0, 1.0, 1.0]);
        let mut cache = None;

        let with_window = [solid(32, 64, [1.0, 0.0, 0.0, 1.0]), wallpaper()];
        render_behind(
            &mut cache,
            &blur.soften,
            1.0,
            &mut renderer,
            (whole(), 1.0),
            &with_window,
        )
        .unwrap();
        let behind = cache.as_mut().unwrap();
        let commit = behind.commit;
        let [red, _, blue, _] = pixel_at(&mut renderer, &mut behind.source, (32, 64));
        assert!(
            red > 250 && blue < 5,
            "the window edge is sharp: {red} {blue}"
        );
        let [red, _, blue, _] = pixel_at(&mut renderer, &mut behind.source, (31, 64));
        assert!(
            blue > 250 && red < 5,
            "the wallpaper is right next to it: {red} {blue}"
        );

        render_behind(
            &mut cache,
            &blur.soften,
            1.0,
            &mut renderer,
            (whole(), 1.0),
            &with_window,
        )
        .unwrap();
        assert_eq!(cache.as_ref().unwrap().commit, commit, "nothing changed");

        render_behind(
            &mut cache,
            &blur.soften,
            1.0,
            &mut renderer,
            (whole(), 1.0),
            &[wallpaper()],
        )
        .unwrap();
        assert_ne!(cache.as_ref().unwrap().commit, commit, "the window left");
    }

    #[test]
    fn what_is_behind_the_dock_covers_only_its_area_and_keeps_the_screen_coordinates() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let blur = compile(&mut renderer).expect("the blur shaders compile");
        let strip = Rectangle::new(Point::from((64, 64)), Size::from((64, 64)));
        let window = SolidColorBuffer::new(Size::from((64, 64)), [1.0, 0.0, 0.0, 1.0]);
        let wallpaper = SolidColorBuffer::new(Size::from((128, 128)), [0.0, 0.0, 1.0, 1.0]);
        let scene = |window_at| [placed(&window, window_at), placed(&wallpaper, 0)];
        let mut cache = None;

        render_behind(
            &mut cache,
            &blur.soften,
            1.0,
            &mut renderer,
            (strip, 1.0),
            &scene(32),
        )
        .unwrap();
        let behind = cache.as_mut().unwrap();
        assert_eq!(behind.source.size(), Size::from((64, 64)));
        let [red, _, blue, _] = pixel_at(&mut renderer, &mut behind.source, (10, 10));
        assert!(
            red > 250 && blue < 5,
            "the window covers the corner: {red} {blue}"
        );
        let [red, _, blue, _] = pixel_at(&mut renderer, &mut behind.source, (50, 50));
        assert!(
            blue > 250 && red < 5,
            "the wallpaper covers the rest: {red} {blue}"
        );

        let commit = behind.commit;
        render_behind(
            &mut cache,
            &blur.soften,
            1.0,
            &mut renderer,
            (strip, 1.0),
            &scene(0),
        )
        .unwrap();
        let commit_after_leaving = cache.as_ref().unwrap().commit;
        assert_ne!(
            commit_after_leaving, commit,
            "the window left the strip's corner"
        );

        render_behind(
            &mut cache,
            &blur.soften,
            1.0,
            &mut renderer,
            (strip, 1.0),
            &scene(-8),
        )
        .unwrap();
        assert_eq!(
            cache.as_ref().unwrap().commit,
            commit_after_leaving,
            "a change off the strip is not a change"
        );
    }

    #[test]
    fn what_is_behind_the_dock_is_softened_along_both_axes_over_a_hard_edge() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let blur = compile(&mut renderer).expect("the blur shaders compile");
        let window = SolidColorBuffer::new(Size::from((64, 64)), [1.0, 0.0, 0.0, 1.0]);
        let wallpaper = SolidColorBuffer::new(Size::from((128, 128)), [0.0, 0.0, 1.0, 1.0]);
        let scene = [placed(&window, 32), placed(&wallpaper, 0)];
        let mut cache = None;
        render_behind(
            &mut cache,
            &blur.soften,
            1.0,
            &mut renderer,
            (whole(), 1.0),
            &scene,
        )
        .unwrap();
        let softened = &mut cache.as_mut().unwrap().softened;

        let [red, _, blue, _] = pixel_at(&mut renderer, softened, (64, 64));
        assert!(
            red > 250 && blue < 5,
            "far from the edge it keeps its color: {red} {blue}"
        );
        let [red, _, blue, _] = pixel_at(&mut renderer, softened, (32, 64));
        assert!(
            (132..157).contains(&red) && (98..123).contains(&blue),
            "on the edge, its own pixel and the nine inside it are the window's: {red} {blue}"
        );
        let [red, _, blue, _] = pixel_at(&mut renderer, softened, (30, 64));
        assert!(
            (66..91).contains(&red) && (164..189).contains(&blue),
            "two pixels outside it, eight of the window's pixels reach: {red} {blue}"
        );
        let [red, _, _, _] = pixel_at(&mut renderer, softened, (22, 64));
        assert!(
            red < 3,
            "more than nine pixels outside it does not reach: {red}"
        );
        let [red, _, blue, _] = pixel_at(&mut renderer, softened, (32, 32));
        assert!(
            (70..95).contains(&red) && (160..185).contains(&blue),
            "at a corner both axes mix: {red} {blue}"
        );
    }
}
