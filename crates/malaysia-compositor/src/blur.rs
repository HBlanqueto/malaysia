mod gaussian;
mod look;
#[cfg(test)]
mod parity;
#[cfg(test)]
mod timing;
pub(crate) mod within;

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
use smithay::backend::renderer::{Bind, ExportMem, Offscreen, Texture};
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
use gaussian::{Backdrop, Blend, Programs, draw_pass};
use look::{Look, Sigmas};

pub(crate) struct Blur {
    programs: Programs,
    soften: GlesTexProgram,
    sigmas: Sigmas,
    wallpaper: Option<Wallpaper>,
    behind: [Option<Behind>; GLASSES.len()],
    veil: Option<Behind>,
    probe: Option<GlesTexture>,
    probed: CommitCounter,
    popups_panel: Option<ctdesign::Rgba>,
    edge_fade: Option<GlesTexProgram>,
    within: within::Within,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Windows,
    Layers,
    Glass,
}

const FAMILIES: [Family; 3] = [Family::Windows, Family::Layers, Family::Glass];

struct Wallpaper {
    tracker: OutputDamageTracker,
    scale: f64,
    source: GlesTexture,
    commit: CommitCounter,
    families: [Option<Shown>; FAMILIES.len()],
}

struct Shown {
    backdrop: Backdrop,
    look: Look,
    from: Option<CommitCounter>,
    commit: CommitCounter,
}

struct Behind {
    tracker: OutputDamageTracker,
    scale: f64,
    area: Rectangle<i32, Physical>,
    source: GlesTexture,
    finish: Finish,
    commit: CommitCounter,
}

enum Finish {
    Soften {
        across: GlesTexture,
        softened: GlesTexture,
        spread: f32,
    },
    Measured {
        backdrop: Box<Backdrop>,
        look: Look,
        region: Rectangle<i32, Physical>,
    },
}

type Placed = (Rectangle<i32, Physical>, Rectangle<i32, Physical>);

#[derive(Clone, Copy)]
enum Method<'a> {
    Soften(&'a GlesTexProgram, f32),
    Measured(&'a Programs, Look, Rectangle<i32, Physical>),
}

#[derive(Default)]
struct BackdropIds(Mutex<HashMap<(usize, usize), Id>>);

pub(crate) fn compile(renderer: &mut GlesRenderer) -> Option<Blur> {
    let along = [
        UniformName::new("texel", UniformType::_2f),
        UniformName::new("direction", UniformType::_2f),
        UniformName::new("spread", UniformType::_1f),
    ];
    let soften = renderer
        .compile_custom_texture_shader(grade_graphics::shaders::DOCK_SOFTEN, &along)
        .inspect_err(|error| tracing::warn!(?error, "failed to compile blur shader"))
        .ok()?;
    Some(Blur {
        programs: gaussian::compile(renderer)?,
        soften,
        sigmas: Sigmas::measure(),
        wallpaper: None,
        behind: std::array::from_fn(|_| None),
        veil: None,
        probe: None,
        probed: CommitCounter::default(),
        popups_panel: None,
        edge_fade: crate::effects::compile_edge_fade(renderer),
        within: within::Within::default(),
    })
}

pub(crate) fn popups_panel(state: &State) -> Option<ctdesign::Rgba> {
    state.blur.as_ref()?.popups_panel
}

pub(crate) fn measured(state: &State, glass: Glass) -> bool {
    state
        .blur
        .as_ref()
        .and_then(|blur| blur.behind[glass as usize].as_ref())
        .is_some_and(|behind| matches!(behind.finish, Finish::Measured { .. }))
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
    blur.within.sweep();
    let wanted = families_requested(state);
    if !wanted.contains(&true) {
        blur.wallpaper = None;
    } else if blur.refresh(state, renderer, output_size, wanted).is_none() {
        tracing::warn!("failed to blur the wallpaper");
        blur.wallpaper = None;
    }
    if !popups_drawn(state) {
        blur.behind = Default::default();
        blur.veil = None;
        blur.popups_panel = None;
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
        Sampling::over(state, detail),
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
        Sampling::over(state, Detail::Layers),
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
        Sampling::over(state, Detail::Layers),
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
    Windows,
    Layers,
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
    fn over(state: &State, detail: Detail) -> Self {
        Self {
            program: state.rounded_clip.clone(),
            uniforms: Vec::new(),
            part: 0,
            detail,
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
                Detail::Windows => true,
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
    let family = |family: Family| {
        blur.wallpaper
            .as_ref()
            .and_then(|wallpaper| wallpaper.families[family as usize].as_ref())
            .map(|shown| (shown.backdrop.result(), shown.commit, Point::default()))
    };
    match detail {
        Detail::Windows => family(Family::Windows),
        Detail::Layers => family(Family::Layers),
        Detail::Behind(glass) => blur.behind[glass as usize]
            .as_ref()
            .map(|behind| (behind.shown(), behind.commit, behind.area.loc)),
        Detail::Veil => blur
            .veil
            .as_ref()
            .map(|veil| (veil.shown(), veil.commit, veil.area.loc)),
    }
}

pub(crate) fn blur_region(surface: &WlSurface) -> Vec<Rectangle<i32, Logical>> {
    if crate::window_style::backdrop(surface).is_some() {
        return Vec::new();
    }
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
    let glass =
        state.blur.as_ref()?.wallpaper.as_ref()?.families[Family::Glass as usize].as_ref()?;
    drawn(state).then(|| (glass.backdrop.result().clone(), glass.commit))
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

fn windows_requested(state: &State) -> bool {
    state
        .windows
        .iter_visible(&state.minimized)
        .any(|window| !blur_region(window.surface()).is_empty())
        || state
            .leaving_focus_window()
            .and_then(|window| state.windows.surface_of(window))
            .is_some_and(|surface| !blur_region(surface).is_empty())
}

fn layers_requested(state: &State) -> bool {
    [Layer::Background, Layer::Bottom, Layer::Top, Layer::Overlay]
        .into_iter()
        .flat_map(|layer| state.layers.on_layer(layer))
        .map(|entry| entry.surface.wl_surface())
        .any(|surface| {
            glass_of(surface).is_none()
                && crate::window_style::alert_layer(surface).is_none()
                && !blur_region(surface).is_empty()
        })
        || state
            .lock_surfaces
            .iter()
            .any(|surface| !blur_region(surface.wl_surface()).is_empty())
}

fn families_requested(state: &State) -> [bool; FAMILIES.len()] {
    [
        windows_requested(state) || dialogs_requested(state),
        drawn(state) && layers_requested(state),
        drawn(state) && glass_requested(state),
    ]
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
    fn probe_popups(&mut self, renderer: &mut GlesRenderer, menu_glass: &ctdesign::DockSurface) {
        let Some(behind) = self.behind[Glass::Popups as usize].as_ref() else {
            self.popups_panel = None;
            return;
        };
        if self.popups_panel.is_some() && behind.commit == self.probed {
            return;
        }
        self.probed = behind.commit;
        self.popups_panel =
            average_color(renderer, behind.shown(), &mut self.probe).map(|average| {
                if behind.measured() {
                    average
                } else {
                    ctappearance::menu::panel_over(menu_glass, average)
                }
            });
    }

    fn refresh(
        &mut self,
        state: &mut State,
        renderer: &mut GlesRenderer,
        output_size: Size<i32, Physical>,
        wanted: [bool; FAMILIES.len()],
    ) -> Option<()> {
        let scale = state.scale.fractional_scale();
        let elements = wallpaper_elements(state, renderer, scale);
        let stale = self.wallpaper.as_ref().is_none_or(|wallpaper| {
            wallpaper.scale != scale
                || wallpaper.source.size() != Size::from((output_size.w, output_size.h))
        });
        if stale {
            self.wallpaper = Some(Wallpaper::new(renderer, output_size, scale)?);
        }
        let wallpaper = self.wallpaper.as_mut()?;
        if redraw(
            &mut wallpaper.tracker,
            &mut wallpaper.source,
            renderer,
            &elements,
        )? {
            wallpaper.commit.increment();
        }
        let looks = [
            look::windows(state),
            look::layers(&self.sigmas),
            look::glass(&self.sigmas, state),
        ];
        for ((slot, look), wanted) in wallpaper.families.iter_mut().zip(looks).zip(wanted) {
            if !wanted {
                *slot = None;
                continue;
            }
            show(
                slot,
                renderer,
                &self.programs,
                (&wallpaper.source, wallpaper.commit),
                (look, scale),
            )?;
        }
        Some(())
    }
}

fn show(
    slot: &mut Option<Shown>,
    renderer: &mut GlesRenderer,
    programs: &Programs,
    (source, from): (&GlesTexture, CommitCounter),
    (look, scale): (Look, f64),
) -> Option<()> {
    let size = source.size();
    if slot
        .as_ref()
        .is_none_or(|shown| !shown.backdrop.fits(size, &look, scale))
    {
        *slot = Some(Shown {
            backdrop: Backdrop::new(renderer, size, &look, scale)?,
            look,
            from: None,
            commit: CommitCounter::default(),
        });
    }
    let shown = slot.as_mut()?;
    if shown.look != look || shown.from != Some(from) {
        let whole = Rectangle::from_size(Size::<i32, Physical>::from((size.w, size.h)));
        shown
            .backdrop
            .run(renderer, programs, source, whole, &look, scale)?;
        shown.look = look;
        shown.from = Some(from);
        shown.commit.increment();
    }
    Some(())
}

fn refresh_behind(
    state: &mut State,
    renderer: &mut GlesRenderer,
    output_size: Size<i32, Physical>,
) {
    let scale = state.scale.fractional_scale();
    let looks = GLASSES.map(|glass| look::shell(state, glass));
    let areas: [Option<Placed>; GLASSES.len()] = std::array::from_fn(|index| {
        let glass = GLASSES[index];
        behind_requested(state, glass)
            .then(|| behind_area(state, glass, scale, output_size, looks[index]))
            .flatten()
    });
    let veil_area = areas[Glass::Notifications as usize]
        .map(|(area, _)| area)
        .filter(|_| crate::notification_panel::open_panel(&state.layers).is_some());
    let glass = areas
        .iter()
        .flatten()
        .map(|(area, _)| *area)
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
    for (((cache, area), glass), look) in blur.behind.iter_mut().zip(areas).zip(GLASSES).zip(looks)
    {
        let Some((area, region)) = area else {
            *cache = None;
            continue;
        };
        let method = match look {
            Some(look) => Method::Measured(&blur.programs, look, region),
            None => Method::Soften(
                &blur.soften,
                material(&state.chrome_theme.resolved, glass).soften,
            ),
        };
        if render_behind(cache, method, renderer, (area, scale), &elements).is_none() {
            tracing::warn!("failed to draw what is behind the glass");
            *cache = None;
        }
    }
    blur.probe_popups(renderer, &state.chrome_theme.resolved.menu_glass);
    let veiled = veil_area.and_then(|area| {
        render_behind(
            &mut blur.veil,
            Method::Soften(&blur.soften, ctdesign::NOTIFICATION_VEIL_SPREAD),
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
    look: Option<Look>,
) -> Option<Placed> {
    let output = Rectangle::from_size(output_size);
    let (shown, region) = match glass {
        Glass::Popups => {
            let menus = glass_popups(state).into_iter().reduce(Rectangle::merge)?;
            let area = outlined(physical_rect(menus, scale));
            (area, area)
        }
        _ => {
            let entry = state
                .layers
                .mapped()
                .iter()
                .find(|entry| glass_of(entry.surface.wl_surface()) == Some(glass))?;
            let area = physical_rect(entry.geometry, scale);
            let region = blur_region(entry.surface.wl_surface())
                .into_iter()
                .reduce(Rectangle::merge)
                .map_or(area, |rect| {
                    physical_rect(
                        Rectangle::new(entry.geometry.loc + rect.loc, rect.size),
                        scale,
                    )
                });
            (area, region)
        }
    };
    let reach = look
        .and_then(|look| look.halo)
        .map_or(0, |(sigma, _)| gaussian::reach_in_pixels(sigma, scale));
    let area = Rectangle::new(
        shown.loc - Point::from((reach, reach)),
        (shown.size.w + 2 * reach, shown.size.h + 2 * reach).into(),
    )
    .intersection(output)
    .filter(|area| !area.is_empty())?;
    let region = region.intersection(area)?;
    Some((area, Rectangle::new(region.loc - area.loc, region.size)))
}

fn render_behind(
    cache: &mut Option<Behind>,
    method: Method<'_>,
    renderer: &mut GlesRenderer,
    (area, scale): (Rectangle<i32, Physical>, f64),
    elements: &[impl RenderElement<GlesRenderer>],
) -> Option<()> {
    let stale = cache.as_ref().is_none_or(|behind| {
        behind.scale != scale || behind.area.size != area.size || !behind.fits(&method)
    });
    if stale {
        *cache = Some(Behind::new(renderer, area, scale, &method)?);
    }
    let behind = cache.as_mut()?;
    behind.area = area;
    let corner = Point::from((-area.loc.x, -area.loc.y));
    let relocated: Vec<_> = elements
        .iter()
        .map(|element| RelocateRenderElement::from_element(element, corner, Relocate::Relative))
        .collect();
    let retargeted = behind.retarget(&method);
    if redraw(
        &mut behind.tracker,
        &mut behind.source,
        renderer,
        &relocated,
    )? || retargeted
    {
        behind.finish(renderer, &method)?;
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
        method: &Method<'_>,
    ) -> Option<Self> {
        let size = Size::<i32, BufferCoords>::from((area.size.w, area.size.h));
        let mut texture =
            || Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).ok();
        let source = texture()?;
        let finish = match method {
            Method::Soften(_, spread) => Finish::Soften {
                across: texture()?,
                softened: texture()?,
                spread: *spread,
            },
            Method::Measured(_, look, region) => Finish::Measured {
                backdrop: Box::new(Backdrop::new(renderer, size, look, scale)?),
                look: *look,
                region: *region,
            },
        };
        Some(Self {
            tracker: OutputDamageTracker::new(area.size, Scale::from(scale), Transform::Normal),
            scale,
            area,
            source,
            finish,
            commit: CommitCounter::default(),
        })
    }

    fn measured(&self) -> bool {
        matches!(self.finish, Finish::Measured { .. })
    }

    fn shown(&self) -> &GlesTexture {
        match &self.finish {
            Finish::Soften { softened, .. } => softened,
            Finish::Measured { backdrop, .. } => backdrop.result(),
        }
    }

    fn fits(&self, method: &Method<'_>) -> bool {
        match (&self.finish, method) {
            (Finish::Soften { .. }, Method::Soften(..)) => true,
            (Finish::Measured { backdrop, .. }, Method::Measured(_, look, _)) => {
                let size = self.source.size();
                backdrop.fits(size, look, self.scale)
            }
            _ => false,
        }
    }

    fn retarget(&mut self, method: &Method<'_>) -> bool {
        match (&mut self.finish, method) {
            (Finish::Soften { spread, .. }, Method::Soften(_, wanted)) => {
                std::mem::replace(spread, *wanted) != *wanted
            }
            (
                Finish::Measured { look, region, .. },
                Method::Measured(_, wanted_look, wanted_region),
            ) => {
                let changed = look != wanted_look || region != wanted_region;
                *look = *wanted_look;
                *region = *wanted_region;
                changed
            }
            _ => false,
        }
    }

    fn finish(&mut self, renderer: &mut GlesRenderer, method: &Method<'_>) -> Option<()> {
        let Self {
            source,
            finish,
            scale,
            ..
        } = self;
        match (finish, method) {
            (
                Finish::Soften {
                    across, softened, ..
                },
                Method::Soften(program, spread),
            ) => {
                let size = source.size();
                let texel = [1.0 / size.w as f32, 1.0 / size.h as f32];
                let along = |direction: [f32; 2]| {
                    [
                        Uniform::new("texel", texel),
                        Uniform::new("direction", direction),
                        Uniform::new("spread", *spread),
                    ]
                };
                draw_pass(
                    renderer,
                    Some(program),
                    source,
                    across,
                    &along([1.0, 0.0]),
                    Blend::Replace,
                )?;
                draw_pass(
                    renderer,
                    Some(program),
                    across,
                    softened,
                    &along([0.0, 1.0]),
                    Blend::Replace,
                )
            }
            (
                Finish::Measured {
                    backdrop,
                    look,
                    region,
                },
                Method::Measured(programs, ..),
            ) => backdrop.run(renderer, programs, source, *region, look, *scale),
            _ => None,
        }
    }
}

impl Wallpaper {
    fn new(renderer: &mut GlesRenderer, size: Size<i32, Physical>, scale: f64) -> Option<Self> {
        let full = Size::<i32, BufferCoords>::from((size.w, size.h));
        Some(Self {
            tracker: OutputDamageTracker::new(size, Scale::from(scale), Transform::Normal),
            scale,
            source: Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, full)
                .ok()?,
            commit: CommitCounter::default(),
            families: std::array::from_fn(|_| None),
        })
    }
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
    draw_pass(renderer, None, from, probe, &[], Blend::Replace)?;
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

    fn pixel_at(renderer: &mut GlesRenderer, texture: &GlesTexture, (x, y): (i32, i32)) -> [u8; 4] {
        let size = Size::<i32, BufferCoords>::from((SIDE, SIDE));
        let mut texture = texture.clone();
        let target = renderer.bind(&mut texture).unwrap();
        let mapping = renderer
            .copy_framebuffer(&target, Rectangle::from_size(size), Fourcc::Abgr8888)
            .unwrap();
        let bytes = renderer.map_texture(&mapping).unwrap();
        let at = ((y * SIDE + x) * 4) as usize;
        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
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
    fn the_wallpaper_blur_shows_a_window_and_runs_again_only_when_it_changes() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let blur = compile(&mut renderer).expect("the blur shaders compile");
        let wallpaper = || solid(0, 128, [0.0, 0.0, 1.0, 1.0]);
        let mut cache = Wallpaper::new(&mut renderer, output(), 1.0).unwrap();
        let look = look::Look {
            sigma: 4.0,
            halo: None,
            post: look::Post::Plain,
        };
        let run = |renderer: &mut GlesRenderer, cache: &mut Wallpaper, scene: &[_]| {
            if redraw(&mut cache.tracker, &mut cache.source, renderer, scene).unwrap() {
                cache.commit.increment();
            }
            show(
                &mut cache.families[Family::Layers as usize],
                renderer,
                &blur.programs,
                (&cache.source, cache.commit),
                (look, 1.0),
            )
            .unwrap();
        };

        let with_window = [solid(32, 64, [1.0, 0.0, 0.0, 1.0]), wallpaper()];
        run(&mut renderer, &mut cache, &with_window);
        let shown = cache.families[Family::Layers as usize].as_ref().unwrap();
        let commit = shown.commit;
        let [red, _, blue, _] =
            pixel_at(&mut renderer, shown.backdrop.result(), (SIDE / 2, SIDE / 2));
        assert!(red > blue, "the window is under the glass: {red} {blue}");

        run(&mut renderer, &mut cache, &with_window);
        let shown = cache.families[Family::Layers as usize].as_ref().unwrap();
        assert_eq!(shown.commit, commit, "nothing changed");

        run(&mut renderer, &mut cache, &[wallpaper()]);
        let shown = cache.families[Family::Layers as usize].as_ref().unwrap();
        assert_ne!(shown.commit, commit, "the window left");
        let [red, _, blue, _] =
            pixel_at(&mut renderer, shown.backdrop.result(), (SIDE / 2, SIDE / 2));
        assert!(blue > red, "only the wallpaper is left: {red} {blue}");
    }

    fn dark_state() -> (
        smithay::reexports::calloop::EventLoop<'static, crate::session::Session>,
        smithay::reexports::wayland_server::Display<State>,
        State,
    ) {
        let (event_loop, display, mut state) = crate::test_support::test_state((1280, 720));
        state.chrome_theme.resolved.scheme = ctappearance::ColorScheme::Dark;
        (event_loop, display, state)
    }

    #[test]
    fn reduce_transparency_makes_the_window_blur_a_solid_and_increase_contrast_alone_keeps_it() {
        let (_loop, _display, mut state) = dark_state();
        let set = ctappearance::material::looks(
            ctappearance::material::Material::UnderWindowBackground,
            state.chrome_theme.resolved.scheme,
        );
        for (reduce_transparency, contrast, expected) in [
            (false, false, set.active),
            (false, true, set.active),
            (true, false, set.reduced),
            (true, true, set.reduced_contrast),
        ] {
            state.config.appearance.reduce_transparency = reduce_transparency;
            state.config.appearance.contrast = contrast;
            let look = look::windows(&state);
            assert!(look.post == look::Post::Material(expected));
            assert_eq!(look.solid(), reduce_transparency);
            assert_eq!(look.sigma > 0.0, !reduce_transparency);
        }
    }

    #[test]
    fn the_liquid_glass_slider_mixes_the_blur_fill_into_the_glass_a_window_requests() {
        use crate::window_style::Material as Requested;
        let (_loop, _display, mut state) = dark_state();
        let glass =
            |state: &State, material| look::requested(state, material, true, 400.0).unwrap();
        let regular = glass(&state, Requested::GlassRegular);
        let clear = glass(&state, Requested::GlassClear);
        assert!(regular.halo.is_none() && clear.halo.is_none());
        state.config.appearance.liquid_glass_tint = 1.0;
        let frosted = glass(&state, Requested::GlassRegular);
        let frosted_clear = glass(&state, Requested::GlassClear);
        let (halo_sigma, halo_weight) = frosted.halo.expect("the blur fill is the halo");
        assert!(halo_sigma > frosted.sigma);
        assert_eq!(
            halo_weight,
            ctdesign::LIQUID_GLASS_TINT_REGULAR_DARK.blur_fill_opacity[2]
        );
        assert_eq!(frosted.sigma, regular.sigma);
        assert_eq!(frosted_clear.halo, frosted.halo);
        assert!(frosted_clear.sigma > clear.sigma);
        let look::Post::Glass(face) = frosted.post else {
            panic!("requested glass draws the face of the model");
        };
        assert_eq!(
            face.look.fill.a,
            ctdesign::LIQUID_GLASS_TINT_REGULAR_DARK.fill_alpha[2]
        );
        state.config.appearance.liquid_glass_tint = 0.0;
        assert!(glass(&state, Requested::GlassRegular) == regular);
    }

    #[test]
    fn only_a_dark_scheme_has_measured_shell_glass_and_the_soften_factor_widens_it() {
        let (_loop, _display, mut state) = dark_state();
        for glass in GLASSES {
            let look = look::shell(&state, glass).expect("dark glass is measured");
            assert!(matches!(look.post, look::Post::Shell(_)));
            assert!(look.sigma > 0.0);
        }
        let rest = look::shell(&state, Glass::Dock).unwrap().sigma;
        state.chrome_theme.resolved.dock.soften *= 2.0;
        assert_eq!(look::shell(&state, Glass::Dock).unwrap().sigma, 2.0 * rest);
        let menu = look::shell(&state, Glass::Popups).unwrap();
        assert!(menu.halo.is_some());
        assert!(look::shell(&state, Glass::Dock).unwrap().halo.is_none());
        state.chrome_theme.resolved.scheme = ctappearance::ColorScheme::Light;
        assert!(
            GLASSES
                .iter()
                .all(|glass| look::shell(&state, *glass).is_none())
        );
    }

    #[test]
    fn the_measured_dock_glass_sees_only_its_region_and_shows_the_measured_tone() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let (_loop, _display, state) = dark_state();
        let blur = compile(&mut renderer).expect("the blur shaders compile");
        let look = look::shell(&state, Glass::Dock).unwrap();
        let white = SolidColorBuffer::new(Size::from((SIDE, SIDE)), [1.0, 1.0, 1.0, 1.0]);
        let black = SolidColorBuffer::new(Size::from((SIDE / 2, SIDE)), [0.0, 0.0, 0.0, 1.0]);
        let right_half: OutputRenderElement<GlesRenderer> = SolidColorRenderElement::from_buffer(
            &black,
            Point::from((SIDE / 2, 0)),
            1.0,
            1.0,
            Kind::Unspecified,
        )
        .into();
        let scene = [right_half, placed(&white, 0)];
        let left = Rectangle::new(Point::from((0, 0)), Size::from((SIDE / 2, SIDE)));
        let mut cache = None;
        render_behind(
            &mut cache,
            Method::Measured(&blur.programs, look, left),
            &mut renderer,
            (whole(), 1.0),
            &scene,
        )
        .unwrap();
        let shown = cache.as_ref().unwrap().shown().clone();
        let mut ink = |x: i32| pixel_at(&mut renderer, &shown, (x, SIDE / 2))[0];
        let ink_at_the_border = ink(SIDE / 2 - 1);
        let ink_far_inside = ink(8);
        assert_eq!(
            ink_at_the_border, ink_far_inside,
            "the border sees no black"
        );
        let ctdesign::ShellGlass { tone, .. } = ctdesign::SHELL_GLASS_DOCK_DARK;
        let white_tone = (tone[11] * 255.0).round() as u8;
        assert!(
            ink_far_inside.abs_diff(white_tone) <= 1,
            "{ink_far_inside} {white_tone}"
        );
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
            Method::Soften(&blur.soften, 1.0),
            &mut renderer,
            (whole(), 1.0),
            &with_window,
        )
        .unwrap();
        let behind = cache.as_mut().unwrap();
        let commit = behind.commit;
        let [red, _, blue, _] = pixel_at(&mut renderer, &behind.source, (32, 64));
        assert!(
            red > 250 && blue < 5,
            "the window edge is sharp: {red} {blue}"
        );
        let [red, _, blue, _] = pixel_at(&mut renderer, &behind.source, (31, 64));
        assert!(
            blue > 250 && red < 5,
            "the wallpaper is right next to it: {red} {blue}"
        );

        render_behind(
            &mut cache,
            Method::Soften(&blur.soften, 1.0),
            &mut renderer,
            (whole(), 1.0),
            &with_window,
        )
        .unwrap();
        assert_eq!(cache.as_ref().unwrap().commit, commit, "nothing changed");

        render_behind(
            &mut cache,
            Method::Soften(&blur.soften, 1.0),
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
            Method::Soften(&blur.soften, 1.0),
            &mut renderer,
            (strip, 1.0),
            &scene(32),
        )
        .unwrap();
        let behind = cache.as_mut().unwrap();
        assert_eq!(behind.source.size(), Size::from((64, 64)));
        let [red, _, blue, _] = pixel_at(&mut renderer, &behind.source, (10, 10));
        assert!(
            red > 250 && blue < 5,
            "the window covers the corner: {red} {blue}"
        );
        let [red, _, blue, _] = pixel_at(&mut renderer, &behind.source, (50, 50));
        assert!(
            blue > 250 && red < 5,
            "the wallpaper covers the rest: {red} {blue}"
        );

        let commit = behind.commit;
        render_behind(
            &mut cache,
            Method::Soften(&blur.soften, 1.0),
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
            Method::Soften(&blur.soften, 1.0),
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
            Method::Soften(&blur.soften, 1.0),
            &mut renderer,
            (whole(), 1.0),
            &scene,
        )
        .unwrap();
        let softened = &cache.as_ref().unwrap().shown().clone();

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
