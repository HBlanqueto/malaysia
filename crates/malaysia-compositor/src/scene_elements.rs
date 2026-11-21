use std::time::Instant;

use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::wlr_layer::Layer;

use crate::cursor::{Cursors, OutputRenderElement};
use crate::effects::Shape;
use crate::renderer::MalaysiaRenderer;
use crate::session::{physical_point, reset_dead_cursor};
use crate::state::State;
use crate::state::{ScreenshotMode, region_rect};

fn append_screenshot_overlay<R: MalaysiaRenderer>(
    state: &State,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let Some(mode) = state.screenshot_mode() else {
        return;
    };
    let scale = state.scale.fractional_scale();
    let output = state.logical_output_size();
    match mode {
        ScreenshotMode::Region {
            start: Some(start),
            current,
        } => {
            let rect = region_rect(*start, *current, output);
            append_dim_outside(rect, output, scale, elements);
            append_border(rect, scale, elements);
        }
        ScreenshotMode::Region { start: None, .. } => {
            append_solid(
                Point::from((0, 0)),
                output,
                ctdesign::SCREENSHOT_IDLE_DIM.premultiplied(),
                scale,
                elements,
            );
        }
        ScreenshotMode::Window { under: Some(id) } => {
            if let Some(outer) = state.windows.outer(*id) {
                let rect = crate::popup::smithay_rect(outer);
                append_dim_outside(rect, output, scale, elements);
                append_border(rect, scale, elements);
            }
        }
        ScreenshotMode::Window { under: None } => {}
    }
}

fn append_dim_outside<R: MalaysiaRenderer>(
    rect: Rectangle<i32, Logical>,
    output: Size<i32, Logical>,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let dim = ctdesign::SCREENSHOT_DIM.premultiplied();
    let left = rect.loc.x.clamp(0, output.w);
    let top = rect.loc.y.clamp(0, output.h);
    let right = (rect.loc.x + rect.size.w).clamp(left, output.w);
    let bottom = (rect.loc.y + rect.size.h).clamp(top, output.h);
    append_solid(
        Point::from((0, 0)),
        Size::from((output.w, top)),
        dim,
        scale,
        elements,
    );
    append_solid(
        Point::from((0, bottom)),
        Size::from((output.w, output.h - bottom)),
        dim,
        scale,
        elements,
    );
    append_solid(
        Point::from((0, top)),
        Size::from((left, bottom - top)),
        dim,
        scale,
        elements,
    );
    append_solid(
        Point::from((right, top)),
        Size::from((output.w - right, bottom - top)),
        dim,
        scale,
        elements,
    );
}

fn append_border<R: MalaysiaRenderer>(
    rect: Rectangle<i32, Logical>,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    const BORDER: i32 = ctdesign::SCREENSHOT_BORDER_WIDTH;
    let color = ctdesign::SCREENSHOT_BORDER.premultiplied();
    let (x, y) = (rect.loc.x, rect.loc.y);
    let (w, h) = (rect.size.w, rect.size.h);
    if w <= 0 || h <= 0 {
        return;
    }
    append_solid(
        Point::from((x, y)),
        Size::from((w, BORDER)),
        color,
        scale,
        elements,
    );
    append_solid(
        Point::from((x, y + h - BORDER)),
        Size::from((w, BORDER)),
        color,
        scale,
        elements,
    );
    append_solid(
        Point::from((x, y + BORDER)),
        Size::from((BORDER, h - 2 * BORDER)),
        color,
        scale,
        elements,
    );
    append_solid(
        Point::from((x + w - BORDER, y + BORDER)),
        Size::from((BORDER, h - 2 * BORDER)),
        color,
        scale,
        elements,
    );
}

fn append_theme_fade<R: MalaysiaRenderer>(
    state: &State,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let Some(fade) = state.theme_fade.as_ref() else {
        return;
    };
    let alpha = fade.alpha(Instant::now());
    elements.push(fade.element(alpha));
}

pub(crate) fn append_solid<R: MalaysiaRenderer>(
    location: Point<i32, Logical>,
    size: Size<i32, Logical>,
    color: [f32; 4],
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let mut buffer = SolidColorBuffer::default();
    append_buffer(&mut buffer, location, size, color, scale, elements);
}

pub(crate) fn append_buffer<R: MalaysiaRenderer>(
    buffer: &mut SolidColorBuffer,
    location: Point<i32, Logical>,
    size: Size<i32, Logical>,
    color: [f32; 4],
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    if size.w <= 0 || size.h <= 0 {
        return;
    }
    buffer.update(size, color);
    elements.push(
        SolidColorRenderElement::from_buffer(
            buffer,
            physical_point(location, scale),
            scale,
            1.0,
            Kind::Unspecified,
        )
        .into(),
    );
}

pub(crate) fn build_output_elements<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    cursor: &Cursors,
    include_cursor: bool,
) -> Vec<OutputRenderElement<R>> {
    let mut elements: Vec<OutputRenderElement<R>> = Vec::new();
    let scale = state.scale.fractional_scale();
    let now = Instant::now();
    state.update_lock_transition(now);
    state.windows.advance_expansions(now);
    if state.locked && state.lock_transition.shown() {
        reset_dead_cursor(state);
        if include_cursor {
            append_cursor(renderer, &mut elements, state, cursor);
        }
        append_layers(
            state,
            renderer,
            &mut elements,
            LayerSide::Front,
            Point::from((0, 0)),
            crate::lock_transition::is_menubar,
        );
        for lock_surface in &state.lock_surfaces {
            if !lock_surface.alive() {
                continue;
            }
            elements.extend(
                render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
                    renderer,
                    lock_surface.wl_surface(),
                    physical_point(Point::from((0, 0)), scale),
                    scale,
                    1.0,
                    Kind::Unspecified,
                )
                .into_iter()
                .map(|e| e.into()),
            );
            elements.extend(crate::blur::capsules(
                state,
                lock_surface.wl_surface(),
                scale,
            ));
        }
        crate::lock_transition::append_wallpaper(state, renderer, 1.0, &mut elements);
        state.lock_frame_built();
        return elements;
    }
    if let Some(cover) = state.lock_cover(now) {
        reset_dead_cursor(state);
        if include_cursor {
            append_cursor(renderer, &mut elements, state, cursor);
        }
        append_layers(
            state,
            renderer,
            &mut elements,
            LayerSide::Front,
            Point::from((0, 0)),
            crate::lock_transition::is_menubar,
        );
        crate::lock_transition::append_wallpaper(state, renderer, cover, &mut elements);
        append_desktop(state, renderer, cursor, false, false, &mut elements);
        return elements;
    }
    append_desktop(state, renderer, cursor, include_cursor, true, &mut elements);
    elements
}

fn append_desktop<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    cursor: &Cursors,
    include_cursor: bool,
    with_bar: bool,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let scale = state.scale.fractional_scale();
    crate::popup_fade::start_requested(state, renderer.gles(), scale);
    crate::minimize::start_requested(state, renderer.gles(), scale);
    reset_dead_cursor(state);
    if include_cursor {
        append_cursor(renderer, elements, state, cursor);
    }
    append_theme_fade(state, elements);
    append_screenshot_overlay(state, elements);
    let now = Instant::now();
    let focus_frame = state.focus_transition_frame(now);
    let slide = state.space_frame(now);
    state.sync_window_buttons();
    let moving = focus_frame.map(|frame| frame.window);
    if let Some(frame) = focus_frame {
        append_focus_transition(state, renderer, frame, scale, elements);
    }
    let chrome_hidden = slide.is_none() && state.layers.chrome_hidden();
    let dock_shown = state.layers.dock_shown();
    let reveal = state.reveal_frame(now);
    let reveal_drop = reveal.map(|frame| {
        let travel = frame.bar
            + if frame.titlebar {
                ctdesign::TITLEBAR.thickness as i32
            } else {
                0
            };
        ((frame.progress - 1.0) * f64::from(travel)).round() as i32
    });
    if let (Some(frame), Some(drop)) = (reveal, reveal_drop) {
        append_layers(
            state,
            renderer,
            elements,
            LayerSide::Front,
            Point::from((0, drop)),
            |entry| entry.is_desktop_bar(),
        );
        append_solid(
            Point::from((0, drop)),
            Size::from((state.logical_output_size().w, frame.bar)),
            ctdesign::SPACE_GAP_COLOR.premultiplied(),
            scale,
            elements,
        );
    }
    let layer_popups: Vec<_> = state
        .windows
        .iter_popups()
        .map(|(id, _, _)| id)
        .filter(|id| state.windows.popup_on_layer(*id))
        .collect();
    append_popups(state, renderer, scale, elements, |id| {
        layer_popups.contains(&id)
    });
    if slide.is_none() {
        crate::minimize::append(state, now, scale, elements);
    }
    append_layers(
        state,
        renderer,
        elements,
        LayerSide::Front,
        Point::from((0, 0)),
        |entry| {
            (slide.is_none() || !entry.is_desktop_bar())
                && !(chrome_hidden && entry.is_shell_chrome() && !(dock_shown && entry.is_dock()))
                && (with_bar || !crate::lock_transition::is_menubar(entry))
        },
    );
    crate::popup_fade::append(state, Instant::now(), elements);
    append_ime_popups(state, renderer, scale, elements);
    append_popups(state, renderer, scale, elements, |id| {
        !layer_popups.contains(&id)
    });
    if let (Some(frame), Some(drop)) = (reveal, reveal_drop)
        && frame.titlebar
    {
        append_revealed_titlebar(
            state,
            renderer,
            frame.owner,
            drop + frame.bar,
            scale,
            elements,
        );
    }
    if let Some(position) = slide {
        append_sliding_spaces(state, renderer, position, scale, moving, elements);
    } else {
        let focused = state.focused_window();
        let window_ids: Vec<_> = state
            .windows
            .iter_visible(&state.minimized)
            .rev()
            .map(|window| window.id())
            .filter(|id| Some(*id) != moving && !state.is_minimizing(*id))
            .collect();
        for id in window_ids {
            let offset = match reveal {
                Some(frame) if frame.owner == id && frame.slides => {
                    let scene_y = state
                        .windows
                        .outer(id)
                        .map_or(0, |outer| outer.y().round() as i32);
                    let drawn = (frame.progress * f64::from(frame.bar)).round() as i32;
                    Point::from((0, drawn - scene_y))
                }
                _ => Point::from((0, 0)),
            };
            append_window_frame(
                state,
                renderer,
                id,
                focused,
                scale,
                Placement::at(offset),
                elements,
            );
        }
        let focus_space = matches!(
            state.spaces.current().kind,
            malaysia_scene::spaces::SpaceKind::Focus { .. }
        );
        if focus_space {
            let size = state.logical_output_size();
            append_buffer(
                &mut state.focus_backdrop,
                Point::from((0, 0)),
                size,
                ctdesign::SPACE_GAP_COLOR.premultiplied(),
                scale,
                elements,
            );
        } else {
            let desktop = state.spaces.desktop_number(state.spaces.current().id);
            let shows = background_of(state, desktop);
            append_layers(
                state,
                renderer,
                elements,
                LayerSide::Back,
                Point::from((0, 0)),
                shows,
            );
        }
    }
}

pub(crate) fn background_of(
    state: &State,
    desktop: Option<usize>,
) -> impl Fn(&crate::layers::LayerEntry) -> bool + use<> {
    let own = desktop.filter(|&number| {
        state
            .layers
            .on_layer(Layer::Background)
            .any(|entry| crate::window_style::desktop(entry.surface.wl_surface()) == Some(number))
    });
    move |entry| {
        entry.layer != Layer::Background
            || crate::window_style::desktop(entry.surface.wl_surface()) == own
    }
}

pub(crate) fn window_content_rect(window: &crate::window::Window<'_>) -> Rectangle<i32, Logical> {
    let geometry = window.window_geometry();
    let origin = window.location() + window.surface_location() + geometry.loc;
    Rectangle::new(origin, geometry.size)
}

fn append_revealed_titlebar<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    id: malaysia_scene::scene::WindowId,
    y: i32,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let Some(title) = state
        .windows
        .iter()
        .find(|window| window.id() == id)
        .map(|window| window.title())
    else {
        return;
    };
    let Some(program) = state.rounded_clip.clone() else {
        return;
    };
    let width = state.logical_output_size().w;
    let rect = Rectangle::new(
        Point::from((0, y)),
        Size::from((width, ctdesign::TITLEBAR.thickness as i32)),
    );
    let focused = state.windows.is_activated(id);
    let pointer = state
        .chrome_pointer
        .as_ref()
        .filter(|(held, _)| *held == id)
        .map(|(_, pointer)| pointer);
    let input = ctappearance::traffic::TrafficInput {
        focused,
        group_hovered: pointer.is_some_and(|pointer| pointer.hovered())
            || (focused && state.config.appearance.differentiate_without_color),
        pressed: pointer.and_then(|pointer| pointer.pressed_visual()),
        zoom_supported: state.green_button_admitted(id),
        fullscreen: false,
    };
    let theme = state.chrome_theme_of(id);
    if let Some(element) = state.chrome.element(
        renderer.gles(),
        crate::chrome::Titlebar {
            id,
            title: &title,
            input,
            theme: &theme,
        },
        rect,
        scale,
        program,
        Shape {
            rect: physical_rect(rect, scale),
            radius: 0,
            exponent: ctdesign::WINDOW_CORNER.exponent,
        },
    ) {
        elements.push(element.into());
    }
    let background = if focused {
        theme.palette.titlebar_bg.focused
    } else {
        theme.palette.titlebar_bg.unfocused
    };
    append_buffer(
        &mut state.revealed_titlebar_backdrop,
        rect.loc,
        rect.size,
        [background.r, background.g, background.b, 1.0],
        scale,
        elements,
    );
}

fn append_focus_transition<R: MalaysiaRenderer>(
    state: &State,
    renderer: &mut R,
    frame: crate::spaces::FocusFrame,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let Some(surface) = state
        .windows
        .iter()
        .find(|window| window.id() == frame.window && window.is_mapped() && window.has_buffer())
        .map(|window| (window.surface().clone(), window.window_geometry()))
    else {
        return;
    };
    let (surface, geometry) = surface;
    let geometry_origin = geometry.loc;
    let rect_physical = physical_rect(frame.rect, scale);
    let content_physical = physical_rect(Rectangle::new(frame.rect.loc, geometry.size), scale);
    let radius = (frame.radius * scale).round() as i32;
    let exponent = ctdesign::WINDOW_CORNER.exponent;
    let surface_elements = render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
        renderer,
        &surface,
        physical_point(frame.rect.loc - geometry_origin, scale),
        scale,
        1.0,
        Kind::Unspecified,
    );
    for element in surface_elements {
        let Some(program) = state.rounded_clip.clone() else {
            elements.push(element.into());
            continue;
        };
        match crate::effects::RoundedTextureElement::from_surface_element(
            element,
            Shape {
                rect: rect_physical,
                radius,
                exponent,
            },
            scale,
            program,
        ) {
            Ok(rounded) => elements.push(rounded.mapped(content_physical, rect_physical).into()),
            Err(element) => elements.push((*element).into()),
        }
    }
}

fn append_sliding_spaces<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    position: f64,
    scale: f64,
    moving: Option<malaysia_scene::scene::WindowId>,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let output = state.logical_output_size();
    let stride = f64::from(output.w + ctdesign::SPACE_GAP);
    let shown: Vec<_> = state
        .spaces
        .iter()
        .enumerate()
        .filter_map(|(index, space)| {
            let x = ((index as f64 - position) * stride).round() as i32;
            (x > -output.w && x < output.w).then_some((space.id, space.kind, x))
        })
        .collect();
    let focused = state.focused_window();
    for (space, kind, x) in shown {
        let desktop = kind == malaysia_scene::spaces::SpaceKind::Desktop;
        if desktop {
            append_layers(
                state,
                renderer,
                elements,
                LayerSide::Front,
                Point::from((x, 0)),
                |entry| entry.is_desktop_bar(),
            );
        }
        let window_ids: Vec<_> = state
            .windows
            .iter_drawable(&state.minimized)
            .rev()
            .map(|window| window.id())
            .filter(|id| state.spaces.space_of(*id) == Some(space) && Some(*id) != moving)
            .collect();
        for id in window_ids {
            append_window_frame(
                state,
                renderer,
                id,
                focused,
                scale,
                Placement::at(Point::from((x, 0))),
                elements,
            );
        }
        if desktop {
            let shows = background_of(state, state.spaces.desktop_number(space));
            append_layers(
                state,
                renderer,
                elements,
                LayerSide::Back,
                Point::from((x, 0)),
                shows,
            );
        }
    }
    append_solid(
        Point::from((0, 0)),
        output,
        ctdesign::SPACE_GAP_COLOR.premultiplied(),
        scale,
        elements,
    );
}

struct WindowFrameData {
    title: String,
    location: Point<i32, Logical>,
    size: Size<i32, Logical>,
    surface_location: Point<i32, Logical>,
    surface: WlSurface,
    insets: crate::decoration::Insets,
    is_client_decorated: bool,
    window_geometry: Rectangle<i32, Logical>,
    state: crate::decoration::WindowState,
    role: crate::decoration::Role,
    resizing: bool,
    expanding: bool,
}

pub(crate) fn ssd_frame_rect(
    location: Point<i32, Logical>,
    configured_size: Size<i32, Logical>,
    geometry: Rectangle<i32, Logical>,
    insets: crate::decoration::Insets,
    resizing: bool,
) -> Rectangle<i32, Logical> {
    let size = if resizing {
        configured_size
    } else {
        Size::from((
            (f64::from(geometry.size.w) + insets.horizontal()).round() as i32,
            (f64::from(geometry.size.h) + insets.vertical()).round() as i32,
        ))
    };
    Rectangle::new(location, size)
}

pub(crate) fn frame_radius(
    forced: Option<u32>,
    state: crate::decoration::WindowState,
    role: crate::decoration::Role,
    client_decorated: bool,
    bar: Option<crate::window_style::Bar>,
) -> u32 {
    if state == crate::decoration::WindowState::Fullscreen {
        return 0;
    }
    crate::window_style::corner_radius(forced, bar, role, client_decorated)
}

pub(crate) fn frame_has_border(state: crate::decoration::WindowState) -> bool {
    state != crate::decoration::WindowState::Fullscreen
}

pub(crate) struct Placement {
    pub(crate) offset: Point<i32, Logical>,
    pub(crate) snapshot: Option<Point<i32, Physical>>,
}

impl Placement {
    pub(crate) fn at(offset: Point<i32, Logical>) -> Self {
        Self {
            offset,
            snapshot: None,
        }
    }
}

pub(crate) fn append_window_frame<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    id: malaysia_scene::scene::WindowId,
    focused: Option<malaysia_scene::scene::WindowId>,
    scale: f64,
    placement: Placement,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let Placement { offset, snapshot } = placement;
    let data = {
        let Some(window) = state
            .windows
            .iter_drawable(&state.minimized)
            .find(|w| w.id() == id)
        else {
            return;
        };
        if !window.is_mapped() {
            return;
        }
        WindowFrameData {
            title: window.title(),
            location: window.location() + offset,
            size: window.size(),
            surface_location: window.surface_location(),
            surface: window.surface().clone(),
            insets: window.insets(),
            is_client_decorated: window.is_client_decorated(),
            window_geometry: window.window_geometry(),
            state: window.frame_state(),
            role: window.role(),
            resizing: window.is_resizing(),
            expanding: window.is_expanding(),
        }
    };

    let is_focused = focused == Some(id);

    let frame_rect_logical = if data.expanding {
        Rectangle::new(data.location, data.size)
    } else if data.is_client_decorated || matches!(data.role, crate::decoration::Role::Dialog) {
        let surface_origin = data.location + data.surface_location;
        Rectangle::new(
            Point::from((
                surface_origin.x + data.window_geometry.loc.x,
                surface_origin.y + data.window_geometry.loc.y,
            )),
            data.window_geometry.size,
        )
    } else {
        ssd_frame_rect(
            data.location,
            data.size,
            data.window_geometry,
            data.insets,
            data.resizing,
        )
    };
    let frame_rect_physical = physical_rect(frame_rect_logical, scale);
    let radius_logical = frame_radius(
        state.config.window.corner_radius,
        data.state,
        data.role,
        data.is_client_decorated,
        state.window_bar(&data.surface),
    );
    let radius_physical =
        (crate::window_style::corner_extent(radius_logical) * scale).round() as i32;
    let exponent = ctdesign::WINDOW_CORNER.exponent;
    let shape = Shape {
        rect: frame_rect_physical,
        radius: radius_physical,
        exponent,
    };

    let mut content: Vec<OutputRenderElement<R>> = Vec::new();
    if data.insets.top > 0.0 {
        let pointer = state
            .chrome_pointer
            .as_ref()
            .filter(|(held, _)| *held == id)
            .map(|(_, pointer)| pointer);
        let input = ctappearance::traffic::TrafficInput {
            focused: is_focused,
            group_hovered: pointer.is_some_and(|pointer| pointer.hovered())
                || (is_focused && state.config.appearance.differentiate_without_color),
            pressed: pointer.and_then(|pointer| pointer.pressed_visual()),
            zoom_supported: state.green_button_admitted(id),
            fullscreen: false,
        };
        let chrome_theme = state.chrome_theme_of(id);
        if let Some(program) = state.rounded_clip.clone()
            && let Some(element) = state.chrome.element(
                renderer.gles(),
                crate::chrome::Titlebar {
                    id,
                    title: &data.title,
                    input,
                    theme: &chrome_theme,
                },
                frame_rect_logical,
                scale,
                program,
                shape,
            )
        {
            content.push(element.into());
        }
    }
    let surface_location = physical_point(data.location + data.surface_location, scale);
    let kind = if frame_has_border(data.state) {
        Kind::Unspecified
    } else {
        Kind::ScanoutCandidate
    };
    let surface_elements = render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
        renderer,
        &data.surface,
        surface_location,
        scale,
        1.0,
        kind,
    );
    crate::knobs::append(
        state,
        renderer,
        id,
        data.location + data.surface_location,
        scale,
        elements,
    );
    let detail = if matches!(data.role, crate::decoration::Role::Dialog) {
        crate::blur::Detail::Dialogs
    } else {
        crate::blur::Detail::Blurred
    };
    let mut backdrop = crate::blur::backdrop(
        state,
        &data.surface,
        data.location + data.surface_location,
        shape,
        detail,
        scale,
    );
    let settle = state.chrome.fading_settle(
        id,
        if is_focused { 0.0 } else { 1.0 },
        std::time::Instant::now(),
    );
    let (material, rim) = crate::glass::panels(
        state,
        id,
        data.location + data.surface_location,
        &shape,
        settle,
        scale,
        snapshot.unwrap_or_default(),
    );
    backdrop.extend(material);
    if snapshot.is_some() {
        crate::glass::flat_underlay(
            &data.surface,
            data.location + data.surface_location,
            frame_rect_physical,
            scale,
            &mut backdrop,
        );
    }
    if data.expanding {
        let window = state.chrome_theme_of(id).resolved.surfaces.window;
        crate::backing::append(
            frame_rect_logical,
            crate::window_style::corner_extent(radius_logical).ceil() as i32,
            [window.r, window.g, window.b, 1.0],
            scale,
            &mut backdrop,
        );
    }
    content.extend(rim);
    append_frame(
        state,
        content,
        surface_elements,
        backdrop,
        Frame {
            owner: crate::chrome::FrameOwner::Window(id),
            shape,
            plate: false,
            focused: is_focused,
            panel: data.role == crate::decoration::Role::Dialog,
            bordered: frame_has_border(data.state),
            arrow: crate::effects::Arrow::default(),
        },
        scale,
        elements,
    );
}

pub(crate) struct Frame {
    pub(crate) owner: crate::chrome::FrameOwner,
    pub(crate) shape: Shape,
    pub(crate) focused: bool,
    pub(crate) panel: bool,
    pub(crate) bordered: bool,
    pub(crate) plate: bool,
    pub(crate) arrow: crate::effects::Arrow,
}

fn append_ime_popups<R: MalaysiaRenderer>(
    state: &State,
    renderer: &mut R,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let output = Rectangle::from_size(state.logical_output_size());
    for popup in state.ime_popups.iter().filter(|popup| popup.alive()) {
        let Some(origin) = popup
            .get_parent()
            .and_then(|parent| state.windows.id_of(&parent.surface))
            .and_then(|id| state.windows.iter().find(|window| window.id() == id))
            .filter(|window| window.is_mapped())
            .map(|window| window.location() + window.surface_location())
        else {
            continue;
        };
        let size = smithay::desktop::utils::bbox_from_surface_tree(
            popup.wl_surface(),
            Point::from((0, 0)),
        )
        .size;
        let location = ime_popup_location(origin, popup.text_input_rectangle(), size, output);
        popup.set_location(location - origin);
        elements.extend(
            render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
                renderer,
                popup.wl_surface(),
                physical_point(location, scale),
                scale,
                1.0,
                Kind::Unspecified,
            )
            .into_iter()
            .map(OutputRenderElement::from),
        );
    }
}

pub(crate) fn ime_popup_location(
    surface_origin: Point<i32, Logical>,
    cursor: Rectangle<i32, Logical>,
    size: Size<i32, Logical>,
    output: Rectangle<i32, Logical>,
) -> Point<i32, Logical> {
    let cursor_x = surface_origin.x + cursor.loc.x;
    let cursor_y = surface_origin.y + cursor.loc.y;
    let right = output.loc.x + output.size.w;
    let x = (cursor_x.min(right - size.w)).max(output.loc.x);
    let below = cursor_y + cursor.size.h;
    let y = if below + size.h <= output.loc.y + output.size.h {
        below
    } else {
        cursor_y - size.h
    };
    Point::from((x, y))
}

pub(crate) fn append_popups<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
    shown: impl Fn(malaysia_scene::scene::PopupId) -> bool,
) {
    let popups: Vec<_> = state
        .windows
        .iter_popups()
        .rev()
        .filter(|(id, _, _)| shown(*id))
        .collect();
    for (id, _parent, rect) in popups {
        let Some(surface) = state.popups.surface_of(id).cloned() else {
            continue;
        };
        let frame = crate::popup::smithay_rect(rect);
        let geometry = crate::window::window_geometry(&surface).unwrap_or_default();
        let surface_elements = render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
            renderer,
            &surface,
            physical_point(frame.loc - geometry.loc, scale),
            scale,
            1.0,
            Kind::Unspecified,
        );
        if !state.popup_has_kit_frame(id) {
            elements.extend(surface_elements.into_iter().map(|e| e.into()));
            continue;
        }
        let popover = state.popups.popup_of(id).and_then(|popup| {
            let shape = crate::window_style::popover_shape(popup.wl_surface())?;
            let metrics = crate::window_style::popover_metrics(shape);
            let (edge, tip) =
                crate::window_style::popover_arrow(popup, frame.size.to_f64(), metrics);
            Some((
                crate::window_style::popover_corner(shape, metrics),
                crate::effects::Arrow::popover(edge, tip, metrics, scale),
            ))
        });
        let (extent, exponent) = popover.map_or(
            (
                crate::window_style::corner_extent(ctdesign::POPUP.corner_radius),
                ctdesign::POPUP.superellipse_exponent,
            ),
            |(corner, _)| corner,
        );
        let styled = Frame {
            owner: crate::chrome::FrameOwner::Popup(id),
            shape: Shape {
                rect: physical_rect(frame, scale),
                radius: (extent * scale).round() as i32,
                exponent,
            },
            plate: false,
            focused: true,
            panel: true,
            bordered: true,
            arrow: popover.map(|(_, arrow)| arrow).unwrap_or_default(),
        };
        if state.popup_has_glass(id) {
            let mut content = Vec::new();
            clip_to_frame(state, surface_elements, &styled, scale, &mut content);
            elements.extend(content);
            let region = vec![Rectangle::from_size(frame.size)];
            elements.extend(dock_glass(
                state,
                &surface,
                crate::blur::Glass::Popups,
                region,
                frame.loc,
                &styled,
                scale,
            ));
            continue;
        }
        append_frame(
            state,
            Vec::new(),
            surface_elements,
            Vec::new(),
            styled,
            scale,
            elements,
        );
    }
}

pub(crate) fn append_frame<R: MalaysiaRenderer>(
    state: &mut State,
    mut content: Vec<OutputRenderElement<R>>,
    surface_elements: Vec<WaylandSurfaceRenderElement<R>>,
    backdrop: Vec<OutputRenderElement<R>>,
    frame: Frame,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    clip_to_frame(state, surface_elements, &frame, scale, &mut content);
    content.extend(backdrop);

    let (outer, inner, shadow) = frame_decorations(state, &frame, scale);
    elements.extend(frame_order(outer, inner, content, shadow));
}

fn clip_to_frame<R: MalaysiaRenderer>(
    state: &State,
    surface_elements: Vec<WaylandSurfaceRenderElement<R>>,
    frame: &Frame,
    scale: f64,
    content: &mut Vec<OutputRenderElement<R>>,
) {
    if let Some(program) = state.rounded_clip.clone().filter(|_| frame.bordered) {
        for element in surface_elements {
            match crate::effects::RoundedTextureElement::from_surface_element(
                element,
                frame.shape,
                scale,
                program.clone(),
            ) {
                Ok(rounded) => content.push(rounded.with_arrow(frame.arrow).into()),
                Err(element) => content.push((*element).into()),
            }
        }
    } else {
        content.extend(surface_elements.into_iter().map(|e| e.into()));
    }
}

type Decorations<R> = (
    Option<OutputRenderElement<R>>,
    Option<OutputRenderElement<R>>,
    Vec<OutputRenderElement<R>>,
);

fn frame_decorations<R: MalaysiaRenderer>(
    state: &mut State,
    frame: &Frame,
    scale: f64,
) -> Decorations<R> {
    let frame_rect_physical = frame.shape.rect;
    let radius_physical = frame.shape.radius;
    let exponent = frame.shape.exponent;
    let mut outer = None;
    let mut inner = None;
    let mut shadow = Vec::new();
    if frame.bordered {
        let palette = &state.chrome_theme.palette;
        let slot = |slot| (frame.owner, slot);
        if let Some(program) = state.border.as_ref() {
            let panel_line = palette
                .panel_inner_line
                .filter(|_| frame.panel && frame.focused);
            let inner_line = panel_line
                .map(|line| (line.sides, line.edges, line.glow))
                .or(palette
                    .inner_line
                    .map(|color| (color, color, ctdesign::Rgba::TRANSPARENT)));
            if let Some(colors) = inner_line {
                let (area, uniforms, holes) = border_shader(
                    frame_rect_physical,
                    radius_physical,
                    scale,
                    exponent,
                    false,
                    colors,
                    frame.arrow,
                );
                inner = Some(
                    crate::renderer::PrimaryGpu(state.chrome.shader(
                        slot(crate::chrome::INNER_BORDER_SLOT),
                        program,
                        area,
                        1.0,
                        uniforms,
                        holes,
                    ))
                    .into(),
                );
            }
            let (area, uniforms, holes) = border_shader(
                frame_rect_physical,
                radius_physical,
                scale,
                exponent,
                true,
                (
                    palette.outer_border,
                    palette.outer_border,
                    ctdesign::Rgba::TRANSPARENT,
                ),
                frame.arrow,
            );
            outer = Some(
                crate::renderer::PrimaryGpu(state.chrome.shader(
                    slot(crate::chrome::OUTER_BORDER_SLOT),
                    program,
                    area,
                    1.0,
                    uniforms,
                    holes,
                ))
                .into(),
            );
        }
        if let Some(program) = state.shadow.as_ref() {
            shadow = state
                .chrome
                .shadow_element(frame, program, scale, &state.chrome_theme);
        }
    }

    (outer, inner, shadow)
}

pub(crate) fn frame_order<T>(
    outer: Option<T>,
    inner: Option<T>,
    content: Vec<T>,
    shadow: Vec<T>,
) -> Vec<T> {
    let mut frame = Vec::with_capacity(content.len() + shadow.len() + 2);
    frame.extend(outer);
    frame.extend(inner);
    frame.extend(content);
    frame.extend(shadow);
    frame
}

pub(crate) fn physical_rect(rect: Rectangle<i32, Logical>, scale: f64) -> Rectangle<i32, Physical> {
    rect.to_f64().to_physical_precise_up(scale)
}

fn shader_rect(
    frame_rect: Rectangle<i32, Physical>,
    scale: f64,
) -> (Rectangle<i32, Logical>, Rectangle<i32, Physical>) {
    let area = crate::effects::shader_area(frame_rect, 1, scale);
    let area_physical: Rectangle<i32, Physical> = area.to_physical_precise_round(scale);
    (
        area,
        Rectangle::new(frame_rect.loc - area_physical.loc, frame_rect.size),
    )
}

pub(crate) fn border_shader(
    frame_rect: Rectangle<i32, Physical>,
    radius: i32,
    scale: f64,
    exponent: f32,
    outer: bool,
    (color, edge_color, glow): (ctdesign::Rgba, ctdesign::Rgba, ctdesign::Rgba),
    arrow: crate::effects::Arrow,
) -> crate::effects::ShaderQuad {
    use smithay::backend::renderer::gles::Uniform;
    let (area, local) = shader_rect(frame_rect, scale);
    let glow_depth = ctdesign::PANEL_GLOW_DEPTH * scale as f32;
    let mut uniforms = vec![
        Uniform::new("rect", crate::effects::uniform_rect(local)),
        Uniform::new("radius", radius as f32),
        Uniform::new("width", 1.0f32),
        Uniform::new("side", if outer { 1.0f32 } else { -1.0f32 }),
        Uniform::new("scale", scale as f32),
        Uniform::new("exponent", exponent),
        Uniform::new("color", color.premultiplied()),
        Uniform::new("edge_color", edge_color.premultiplied()),
        Uniform::new("glow", glow.premultiplied()),
        Uniform::new("glow_depth", glow_depth),
    ];
    uniforms.extend(arrow.uniforms(1.0));
    let depth = if glow.a > 0.0 {
        glow_depth.ceil() as i32
    } else {
        0
    };
    (
        area,
        uniforms,
        crate::effects::beyond_reach(local, radius, arrow, depth),
    )
}

pub(crate) fn build_window_elements<R: MalaysiaRenderer>(
    renderer: &mut R,
    surface: &WlSurface,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    let geometry = crate::window::window_geometry(surface).unwrap_or_default();
    render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
        renderer,
        surface,
        physical_point(Point::from((0, 0)) - geometry.loc, scale),
        scale,
        1.0,
        Kind::Unspecified,
    )
    .into_iter()
    .map(|e| e.into())
    .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerSide {
    Back,
    Front,
}

pub(crate) fn append_layers<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    elements: &mut Vec<OutputRenderElement<R>>,
    side: LayerSide,
    offset: Point<i32, Logical>,
    include: impl Fn(&crate::layers::LayerEntry) -> bool,
) {
    let layers = match side {
        LayerSide::Back => [Layer::Background, Layer::Bottom],
        LayerSide::Front => [Layer::Top, Layer::Overlay],
    };
    let scale = state.scale.fractional_scale();
    for layer in layers {
        let entries: Vec<_> = state
            .layers
            .on_layer(layer)
            .filter(|entry| include(entry))
            .map(|entry| (entry.surface.wl_surface().clone(), entry.geometry))
            .collect();
        for (surface, geometry) in entries {
            let rect = Rectangle::new(geometry.loc + offset, geometry.size);
            let surface_elements =
                render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
                    renderer,
                    &surface,
                    physical_point(rect.loc, scale),
                    scale,
                    1.0,
                    Kind::Unspecified,
                );
            if let Some(cards) = crate::window_style::cards(&surface) {
                append_cards(
                    state,
                    &surface,
                    &cards,
                    rect,
                    surface_elements,
                    scale,
                    elements,
                );
                continue;
            }
            let Some(alert) = crate::window_style::alert_layer(&surface) else {
                elements.extend(surface_elements.into_iter().map(|e| e.into()));
                elements.extend(crate::blur::layer(
                    state,
                    &surface,
                    rect.loc,
                    physical_rect(rect, scale),
                    scale,
                ));
                continue;
            };
            let radius = (crate::window_style::corner_extent(ctdesign::DIALOG.corner_radius)
                * scale)
                .round() as i32;
            let backdrop = crate::blur::backdrop(
                state,
                &surface,
                rect.loc,
                Shape {
                    rect: physical_rect(rect, scale),
                    radius,
                    exponent: ctdesign::DIALOG.superellipse_exponent,
                },
                crate::blur::Detail::Dialogs,
                scale,
            );
            append_frame(
                state,
                Vec::new(),
                surface_elements,
                backdrop,
                Frame {
                    owner: crate::chrome::FrameOwner::Alert(alert),
                    shape: Shape {
                        rect: physical_rect(rect, scale),
                        radius,
                        exponent: ctdesign::DIALOG.superellipse_exponent,
                    },
                    plate: false,
                    focused: true,
                    panel: true,
                    bordered: true,
                    arrow: crate::effects::Arrow::default(),
                },
                scale,
                elements,
            );
        }
    }
}

fn append_cards<R: MalaysiaRenderer>(
    state: &mut State,
    surface: &WlSurface,
    cards: &crate::window_style::Cards,
    surface_rect: Rectangle<i32, Logical>,
    surface_elements: Vec<WaylandSurfaceRenderElement<R>>,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let mut backdrops = Vec::new();
    let glass = crate::blur::glass_of(surface);
    let plates = cards
        .plates
        .iter()
        .filter(|_| glass == Some(crate::blur::Glass::Dock))
        .map(|plate| {
            let radius = crate::window_style::plate_radius(
                cards.plate_corner,
                plate.size.w.min(plate.size.h),
            );
            (plate, radius, true)
        });
    let rects = cards.rects.iter().map(|card| (card, cards.radius, false));
    for (index, (card, radius, plate)) in plates.chain(rects).enumerate() {
        let card = Rectangle::new(surface_rect.loc + card.loc, card.size);
        let (extent, exponent) =
            crate::window_style::card_corner(radius, card.size.w.min(card.size.h));
        let frame = Frame {
            owner: crate::chrome::FrameOwner::Card(cards.id, index),
            shape: Shape {
                rect: physical_rect(card, scale),
                radius: (extent * scale).round() as i32,
                exponent,
            },
            plate,
            focused: true,
            panel: false,
            bordered: true,
            arrow: crate::effects::Arrow::default(),
        };
        backdrops.extend(glass.into_iter().flat_map(|glass| {
            dock_glass(
                state,
                surface,
                glass,
                crate::blur::blur_region(surface),
                surface_rect.loc,
                &frame,
                scale,
            )
        }));
    }
    if glass == Some(crate::blur::Glass::Notifications)
        && crate::notification_panel::open_panel(&state.layers).is_some()
        && let Some(veil) = crate::notification_panel::veil(
            &cards.rects,
            surface_rect.size.w,
            state.layers.exclusive_top(),
        )
    {
        backdrops.extend(crate::blur::veil(
            state,
            surface,
            surface_rect.loc,
            veil,
            scale,
        ));
    }
    elements.extend(surface_elements.into_iter().map(|e| e.into()));
    elements.extend(backdrops);
}

fn dock_glass<R: MalaysiaRenderer>(
    state: &mut State,
    surface: &WlSurface,
    kind: crate::blur::Glass,
    region: Vec<Rectangle<i32, Logical>>,
    origin: Point<i32, Logical>,
    frame: &Frame,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    use smithay::backend::renderer::gles::Uniform;
    let shape = frame.shape;
    let plate_side = frame.plate.then_some(shape.rect.size.w as f32);
    let part = match frame.owner {
        crate::chrome::FrameOwner::Card(_, index) => index,
        _ => 0,
    };
    let resolved = &state.chrome_theme.resolved;
    let material = crate::blur::material(resolved, kind);
    let uniforms = dock_frame_uniforms(resolved, material, plate_side);
    let mut glass_uniforms = uniforms.clone();
    glass_uniforms.push(Uniform::new("lens", material.lens));
    let ring = crate::blur::outlined(shape.rect);
    let glass = crate::blur::sampled(
        state,
        surface,
        region,
        origin,
        scale,
        crate::blur::Sampling {
            program: state.dock_glass.clone(),
            uniforms: glass_uniforms,
            part,
            detail: crate::blur::Detail::Behind(kind),
            arrow: frame.arrow,
        },
        |dst| dst.overlaps(shape.rect).then_some((ring, shape)),
    );
    if !glass.is_empty() {
        return glass;
    }
    let Some(program) = state.dock_solid.clone() else {
        return Vec::new();
    };
    let (area, local) = shader_rect(shape.rect, scale);
    let mut all = vec![
        Uniform::new("rect", crate::effects::uniform_rect(local)),
        Uniform::new("radius", shape.radius as f32),
        Uniform::new("scale", scale as f32),
        Uniform::new("exponent", shape.exponent),
    ];
    all.extend(frame.arrow.uniforms(1.0));
    all.extend(uniforms);
    vec![
        crate::renderer::PrimaryGpu(state.chrome.shader(
            (frame.owner, crate::chrome::DOCK_SLOT),
            &program,
            area,
            1.0,
            all,
            [None, None],
        ))
        .into(),
    ]
}

fn dock_frame_uniforms(
    resolved: &ctappearance::Resolved,
    dock: ctdesign::DockSurface,
    plate_side: Option<f32>,
) -> Vec<smithay::backend::renderer::gles::Uniform<'static>> {
    use ctdesign::{
        ICON_PLATE, ICON_PLATE_CLEAR, ICON_PLATE_CLEAR_DARK, ICON_PLATE_GLASS,
        ICON_PLATE_GLASS_DARK, ICON_PLATE_GLASS_DARK_ON_DARK,
    };
    use smithay::backend::renderer::gles::Uniform;
    let (look, plate_glass) = match (resolved.icon_dark, resolved.scheme.is_dark()) {
        (true, true) => (&ICON_PLATE_CLEAR_DARK, ICON_PLATE_GLASS_DARK_ON_DARK),
        (true, false) => (&ICON_PLATE_CLEAR_DARK, ICON_PLATE_GLASS_DARK),
        (false, _) => (&ICON_PLATE_CLEAR, ICON_PLATE_GLASS),
    };
    let glass = resolved
        .bar_tint
        .map_or(dock.glass, |(tint, factor)| dock.glass.mix(tint, factor));
    let outline = dock.outline;
    let glow = dock.glow;
    let (tone, tone_outline) = dock.tone.map_or(([0.0; 4], [0.0; 2]), |tone| {
        ([tone.chroma, tone.lift, tone.cap, tone.gain], tone.outline)
    });
    vec![
        Uniform::new("glass", [glass.r, glass.g, glass.b, glass.a]),
        Uniform::new("rim", dock.rim),
        Uniform::new("corner", dock.corner),
        Uniform::new("rim_side", dock.rim_side),
        Uniform::new("outline", [outline.r, outline.g, outline.b, outline.a]),
        Uniform::new("glow_near", [glow[0], glow[1], glow[2]]),
        Uniform::new("glow_far", [glow[3], glow[4], glow[5]]),
        Uniform::new("tone", tone),
        Uniform::new("tone_outline", tone_outline),
        Uniform::new(
            "plate",
            [
                plate_glass[0],
                plate_glass[1],
                plate_glass[2],
                f32::from(u8::from(plate_side.is_some())),
            ],
        ),
        Uniform::new(
            "plate_edge",
            [
                ICON_PLATE.light_reach,
                ICON_PLATE.light_sharpness,
                ICON_PLATE.outline_ends,
                look.outline_lightness,
            ],
        ),
        Uniform::new(
            "plate_ramp",
            [
                ICON_PLATE.outline_from,
                ICON_PLATE.outline_to,
                plate_side.map_or(0.0, |side| ICON_PLATE.light_side / side.max(1.0)),
                plate_glass[3],
            ],
        ),
    ]
}

fn append_cursor<R: MalaysiaRenderer>(
    renderer: &mut R,
    elements: &mut Vec<OutputRenderElement<R>>,
    state: &State,
    cursor: &Cursors,
) {
    let Some(location) = state
        .seat
        .get_pointer()
        .map(|pointer| pointer.current_location())
    else {
        return;
    };
    let scale = state.scale.fractional_scale();
    let boost = f64::from(state.cursor_boost_scale());

    match &state.cursor_status {
        CursorImageStatus::Hidden => {}
        CursorImageStatus::Named(icon) => {
            let shape = if boost > 1.0 {
                cursor.get_boosted(*icon)
            } else {
                cursor.get(*icon)
            };
            let (source, size, hotspot) = shape.geometry(boost);
            let position = cursor_origin(location, hotspot, scale);
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                position,
                shape.buffer(),
                None,
                Some(source),
                Some(size),
                Kind::Cursor,
            ) {
                Ok(element) => {
                    tracing::trace!(boost, "drawing a pointer cursor");
                    elements.push(element.into());
                }
                Err(error) => tracing::warn!(?error, "could not draw a pointer cursor"),
            }
        }
        CursorImageStatus::Surface(surface) => {
            let hotspot = cursor_surface_hotspot(surface).to_f64();
            let hotspot = Point::from((hotspot.x * boost, hotspot.y * boost));
            let position = cursor_origin(location, hotspot, scale).to_i32_round();
            elements.extend(
                render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
                    renderer,
                    surface,
                    position,
                    scale * boost,
                    1.0,
                    Kind::Cursor,
                )
                .into_iter()
                .map(|e| e.into()),
            );
        }
    }
}

pub(crate) fn cursor_origin(
    location: Point<f64, Logical>,
    hotspot: Point<f64, Logical>,
    scale: f64,
) -> Point<f64, Physical> {
    (location - hotspot).to_physical(scale)
}

fn cursor_surface_hotspot(surface: &WlSurface) -> Point<f64, Logical> {
    with_states(surface, |states| {
        states
            .data_map
            .get::<CursorImageSurfaceData>()
            .and_then(|data| data.lock().ok().map(|attributes| attributes.hotspot))
    })
    .map(|hotspot| hotspot.to_f64())
    .unwrap_or_else(|| Point::from((0.0, 0.0)))
}
