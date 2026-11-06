use std::cell::Cell;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::backend::SwapBuffersError;
use smithay::backend::drm::{DrmDeviceFd, DrmNode};
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::damage::{Error as DamageError, OutputDamageTracker};
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::{
    Kind, RenderElementStates, default_primary_scanout_output_compare,
};
use smithay::backend::renderer::gles::{GlesError, GlesRenderer};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::utils::{Buffer, RendererSurfaceStateUserData};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::desktop::utils::{
    OutputPresentationFeedback, send_frames_surface_tree,
    surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
    take_presentation_feedback_surface_tree, update_surface_primary_scanout_output,
    with_surfaces_surface_tree,
};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::{Mode as OutputMode, OutputModeSource};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{
    EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction, RegistrationToken,
};
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::wayland_server::BindError;
use smithay::reexports::wayland_server::Display;
use smithay::reexports::wayland_server::backend::InitError;
use smithay::reexports::wayland_server::backend::{ClientId, ObjectId};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{
    DeviceFd, IsAlive, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};
use smithay::wayland::compositor::{
    SurfaceAttributes, SurfaceData, TraversalAction, with_states, with_surface_tree_downward,
};
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use smithay::wayland::drm_syncobj::supports_syncobj_eventfd;
use smithay::wayland::presentation::Refresh;
use smithay::wayland::shell::wlr_layer::Layer;
use smithay::wayland::socket::ListeningSocketSource;

use crate::config::ScreenshotTarget;
use crate::cursor::{Cursors, OutputRenderElement};
use crate::renderer::MalaysiaRenderer;
use crate::state::{ClientState, State, StateError};
use crate::state::{ScreenshotMode, region_rect};
use vow_appearance::watch::AppearanceWatch;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("event loop failure")]
    EventLoop(#[from] smithay::reexports::calloop::Error),
    #[error("failed to initialise the winit backend: {0}")]
    Backend(String),
    #[error("failed to install signal handling")]
    Signals(#[source] std::io::Error),
    #[error("failed to create the Wayland display")]
    Display(#[from] InitError),
    #[error("failed to create the Wayland listening socket")]
    Socket(#[from] BindError),
    #[error("failed to duplicate the display file descriptor")]
    Fd(#[source] std::io::Error),
    #[error("failed to build the compositor state")]
    State(#[from] StateError),
    #[error("cannot set up the dmabuf feedback: {0}")]
    DmabufFeedback(String),
    #[error("cannot wait on the GPU without EGL_ANDROID_native_fence_sync: {0}")]
    NativeFence(String),
}

pub(crate) struct Session {
    loop_handle: LoopHandle<'static, Session>,
    display: Display<State>,
    pub(crate) state: State,
    backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
    cursor: Cursors,
    track_objects: bool,
    last_objects: LiveObjects,
    capture_requested: bool,
    frame_seq: u64,
    last_frame_duration: Option<Duration>,
    estimated_vblank: Option<RegistrationToken>,
    appearance_watch: Option<AppearanceWatch>,
}

struct PendingFrameRelease {
    sync: SyncPoint,
    buffers: Vec<Buffer>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LiveObjects {
    clients: usize,
    surfaces: usize,
    toplevels: usize,
}

impl LiveObjects {
    fn log(self) {
        tracing::info!(
            clients = self.clients,
            surfaces = self.surfaces,
            toplevels = self.toplevels,
            "live objects"
        );
    }
}

fn live_objects(display: &Display<State>, state: &State) -> LiveObjects {
    let handle = display.handle().backend_handle();

    let mut clients: Vec<ClientId> = Vec::new();
    handle.with_all_clients(|client| clients.push(client));

    let mut surfaces = 0;
    for client in &clients {
        let mut objects: Vec<ObjectId> = Vec::new();
        let _ = handle.with_all_objects_for(client.clone(), |object| objects.push(object));
        for object in objects {
            if let Ok(info) = handle.object_info(object)
                && info.interface.name == "wl_surface"
            {
                surfaces += 1;
            }
        }
    }

    LiveObjects {
        clients: clients.len(),
        surfaces,
        toplevels: state.windows.len(),
    }
}

fn describe_chain(error: &dyn std::error::Error) -> String {
    let mut description = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        description.push_str(": ");
        description.push_str(&error.to_string());
        cause = error.source();
    }
    description
}

pub(crate) const fn winit_render_transform() -> Transform {
    Transform::Flipped180
}

fn physical_point(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
    point.to_f64().to_physical(scale).to_i32_floor()
}

pub(crate) fn winit_mode_source(size: Size<i32, Physical>, scale: f64) -> OutputModeSource {
    OutputModeSource::Static {
        size,
        scale: Scale::from(scale),
        transform: winit_render_transform(),
    }
}

fn open_render_node(node: &DrmNode) -> std::io::Result<DrmDeviceFd> {
    use std::fs::OpenOptions;

    let path = node
        .dev_path()
        .ok_or_else(|| std::io::Error::other("the render node has no device path"))?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    let fd = OwnedFd::from(file);
    Ok(DrmDeviceFd::new(DeviceFd::from(fd)))
}

pub fn run() -> Result<(), SessionError> {
    crate::spawn::mark_nested_session();
    let mut event_loop: EventLoop<Session> = EventLoop::try_new()?;
    let loop_signal = event_loop.get_signal();

    let display = Display::<State>::new()?;

    let (mut backend, winit_events) = winit::init::<GlesRenderer>()
        .map_err(|error| SessionError::Backend(describe_chain(&error)))?;

    if !backend
        .renderer()
        .egl_context()
        .display()
        .extensions()
        .iter()
        .any(|ext| ext == "EGL_ANDROID_native_fence_sync")
    {
        return Err(SessionError::NativeFence(
            "the EGL display lacks the extension".to_string(),
        ));
    }

    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node())
        .map_err(|error| {
            SessionError::DmabufFeedback(format!("the EGL device could not be queried: {error}"))
        })?
        .ok_or_else(|| {
            SessionError::DmabufFeedback("the EGL device has no render node".to_string())
        })?;
    let dmabuf_feedback =
        DmabufFeedbackBuilder::new(render_node.dev_id(), backend.renderer().dmabuf_formats())
            .build()
            .map_err(|error| {
                SessionError::DmabufFeedback(format!("the feedback could not be built: {error}"))
            })?;

    let import_device = open_render_node(&render_node).map_err(|error| {
        SessionError::DmabufFeedback(format!("the render node could not be opened: {error}"))
    })?;
    let syncobj_device = supports_syncobj_eventfd(&import_device).then_some(import_device);
    let loop_handle = event_loop.handle();

    let state = State::new(
        loop_signal.clone(),
        &display.handle(),
        backend.window_size(),
        dmabuf_feedback,
        syncobj_device,
        Box::new(loop_handle.clone()),
        Box::new(crate::idle::IdleTimers::new(loop_handle.clone())),
    )?;
    let damage_tracker = OutputDamageTracker::from_mode_source(winit_mode_source(
        backend.window_size(),
        state.scale.fractional_scale(),
    ));
    let cursor = Cursors::load(state.scale.fractional_scale().ceil() as i32);
    let track_objects = std::env::var_os("MALAYSIA_TEST_OBJECTS").is_some();
    let mut session = Session {
        loop_handle: loop_handle.clone(),
        display,
        state,
        backend,
        damage_tracker,
        cursor,
        track_objects,
        last_objects: LiveObjects::default(),
        capture_requested: false,
        frame_seq: 0,
        last_frame_duration: None,
        estimated_vblank: None,
        appearance_watch: None,
    };
    if session.track_objects {
        session.last_objects = live_objects(&session.display, &session.state);
        session.last_objects.log();
    }

    let renderer = session.backend.renderer();
    session.state.rounded_clip = crate::effects::compile_rounded_clip(renderer);
    session.state.shadow = crate::effects::compile_shadow(renderer);
    session.state.border = crate::effects::compile_border(renderer);

    session.backend.window().request_redraw();

    watch_for_shutdown(&loop_handle, loop_signal)?;

    let disconnect_queue = crate::state::ClientDisconnectQueue::default();
    let (disconnect_ping, disconnect_source) = smithay::reexports::calloop::ping::make_ping()
        .map_err(|error| SessionError::EventLoop(error.into()))?;
    loop_handle
        .insert_source(disconnect_source, {
            let queue = disconnect_queue.clone();
            move |_, _, session: &mut Session| {
                for client_id in queue.drain() {
                    session.state.client_disconnected(client_id);
                }
            }
        })
        .map_err(|error| SessionError::EventLoop(error.into()))?;
    watch_overdue_frame_callbacks(&loop_handle);

    if session.state.test_pointer.is_some() {
        loop_handle
            .insert_source(
                Timer::from_duration(crate::input::test_pointer_delay()),
                |_, _, session: &mut Session| {
                    let action = session.state.test_pointer_step();
                    if let Err(error) = session.display.flush_clients() {
                        tracing::warn!(%error, "flushing clients after scripted input");
                    }
                    match action {
                        Some(delay) => TimeoutAction::ToDuration(delay),
                        None => TimeoutAction::Drop,
                    }
                },
            )
            .map_err(|error| SessionError::EventLoop(error.into()))?;
    }

    let appearance_watch = AppearanceWatch::new().map_err(SessionError::Fd)?;
    let appearance_fd = appearance_watch
        .as_fd()
        .try_clone_to_owned()
        .map_err(SessionError::Fd)?;
    session.appearance_watch = Some(appearance_watch);
    loop_handle
        .insert_source(
            Generic::new(appearance_fd, Interest::READ, Mode::Level),
            |_, _, session: &mut Session| {
                if let Some(watch) = session.appearance_watch.as_ref()
                    && let Ok(events) = watch.drain()
                    && !events.is_empty()
                {
                    let now = Instant::now();
                    let output_size = session.state.output_size;
                    let scale = session.state.scale.fractional_scale();
                    let restyled = events.iter().any(|event| event.restyles());
                    if restyled {
                        match session.backend.bind() {
                            Ok((renderer, _)) => {
                                session.state.theme_fade = crate::theme_fade::start_if_allowed(
                                    &mut session.state,
                                    renderer,
                                    &session.cursor,
                                    output_size,
                                    scale,
                                    now,
                                );
                            }
                            Err(_) => crate::theme_fade::log_snapshot_failure(
                                "could not bind the renderer for the theme fade snapshot",
                            ),
                        }
                    }
                    session.state.refresh_theme(restyled);
                    if session.state.take_redraw_request() {
                        session.backend.window().request_redraw();
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| SessionError::EventLoop(error.into()))?;

    loop_handle
        .insert_source(winit_events, move |event, _, session: &mut Session| {
            handle_event(event, session);
        })
        .map_err(|error| SessionError::EventLoop(error.into()))?;

    let listening_socket = ListeningSocketSource::new_auto()?;
    let socket_name = listening_socket
        .socket_name()
        .to_string_lossy()
        .into_owned();
    tracing::info!(socket = %socket_name, "listening on the Wayland socket");
    let _xwayland = crate::xwayland::start(&socket_name);
    session.state.set_wayland_display(socket_name);
    loop_handle
        .insert_source(listening_socket, {
            let queue = disconnect_queue.clone();
            let ping = disconnect_ping.clone();
            move |stream, _, session: &mut Session| {
                let trusted_shell = crate::infinity::is_trusted_client(&stream);
                let data = Arc::new(ClientState::new(queue.clone(), ping.clone(), trusted_shell));
                let mut handle = session.display.handle();
                match handle.insert_client(stream, data) {
                    Ok(_) => tracing::info!("client connected"),
                    Err(error) => tracing::warn!(%error, "rejected a client"),
                }
            }
        })
        .map_err(|error| SessionError::EventLoop(error.into()))?;

    let display_fd = session
        .display
        .as_fd()
        .try_clone_to_owned()
        .map_err(SessionError::Fd)?;
    loop_handle
        .insert_source(
            Generic::new(display_fd, Interest::READ, Mode::Level),
            |_, _, session: &mut Session| {
                dispatch_clients(session);
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| SessionError::EventLoop(error.into()))?;

    tracing::info!("winit window open; close it or send SIGTERM to exit");
    event_loop.run(None, &mut session, |session| {
        session.state.activity_noted = false;
        let listeners = std::mem::take(&mut session.state.security_listeners);
        crate::security::listen(
            &session.loop_handle,
            listeners,
            &disconnect_queue,
            &disconnect_ping,
        );
        if session.state.take_redraw_request() {
            session.backend.window().request_redraw();
        }
        if let Err(error) = session.display.flush_clients() {
            tracing::warn!(%error, "failed to flush clients");
        }
        if session.track_objects {
            let now = live_objects(&session.display, &session.state);
            if now != session.last_objects {
                session.last_objects = now;
                now.log();
            }
        }
    })?;
    Ok(())
}

fn dispatch_clients(session: &mut Session) {
    let Session {
        display,
        state,
        backend,
        ..
    } = session;

    if let Err(error) = display.dispatch_clients(state) {
        tracing::error!(%error, "failed to dispatch client requests");
        return;
    }
    import_pending_dmabufs(state, backend.renderer());
    if let Err(error) = display.flush_clients() {
        tracing::error!(%error, "failed to flush events to clients");
    }
}

pub(crate) fn import_pending_dmabufs(state: &mut State, renderer: &mut GlesRenderer) {
    for pending in state.pending_dmabuf_imports.drain(..) {
        match renderer.import_dmabuf(&pending.dmabuf, None) {
            Ok(_) => {
                if let Err(error) = pending.notifier.successful::<State>() {
                    tracing::warn!(
                        ?error,
                        "dmabuf import: the client died before the buffer was created"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(%error, "rejecting a dmabuf the renderer cannot import");
                pending.notifier.failed();
            }
        }
    }
}

fn watch_for_shutdown(
    loop_handle: &LoopHandle<'static, Session>,
    loop_signal: LoopSignal,
) -> Result<(), SessionError> {
    use calloop::signals::{Signal, Signals};
    let signals = Signals::new(&[Signal::SIGUSR1, Signal::SIGTERM, Signal::SIGINT])
        .map_err(|error| SessionError::Signals(std::io::Error::other(error.to_string())))?;
    loop_handle
        .insert_source(
            signals,
            move |signal, _, session: &mut Session| match signal.signal() {
                Signal::SIGUSR1 => {
                    session.capture_requested = true;
                    session.backend.window().request_redraw();
                    tracing::info!("SIGUSR1: debug capture requested");
                }
                Signal::SIGTERM | Signal::SIGINT => {
                    tracing::info!("shutdown signal received");
                    loop_signal.stop();
                }
                _ => {}
            },
        )
        .map_err(|error| SessionError::EventLoop(error.into()))?;
    Ok(())
}

fn handle_event(event: WinitEvent, session: &mut Session) {
    match event {
        WinitEvent::Redraw => redraw(session),
        WinitEvent::Resized { size, .. } => {
            session.state.output_size = size;
            session.state.output.change_current_state(
                Some(OutputMode {
                    size,
                    refresh: 60_000,
                }),
                None,
                None,
                None,
            );
            session.damage_tracker = OutputDamageTracker::from_mode_source(winit_mode_source(
                size,
                session.state.scale.fractional_scale(),
            ));
            session.state.refresh_capture_constraints();
            session.backend.window().request_redraw();
        }
        WinitEvent::CloseRequested => {
            tracing::info!("window close requested");
            session.state.stop();
        }
        WinitEvent::Input(event) => {
            if session.state.test_pointer.is_some() {
                return;
            }
            session.state.process_input_event(event);
            if let Err(error) = session.display.flush_clients() {
                tracing::warn!(%error, "flushing clients after input");
            }
            session.state.request_redraw();
        }
        WinitEvent::Focus(_) => {}
    }
}

fn redraw(session: &mut Session) {
    if let Err(error) = draw(session) {
        tracing::error!(%error, "failed to draw a frame");
    }
}

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
                vow_design::SCREENSHOT_IDLE_DIM.premultiplied(),
                scale,
                elements,
            );
        }
        ScreenshotMode::Window { under: Some(id) } => {
            if let Some(outer) = state.windows.outer(*id) {
                let rect = Rectangle::new(
                    Point::from((outer.x().round() as i32, outer.y().round() as i32)),
                    Size::from((outer.width().round() as i32, outer.height().round() as i32)),
                );
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
    let dim = vow_design::SCREENSHOT_DIM.premultiplied();
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
    const BORDER: i32 = vow_design::SCREENSHOT_BORDER_WIDTH;
    let color = vow_design::SCREENSHOT_BORDER.premultiplied();
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

fn append_solid<R: MalaysiaRenderer>(
    location: Point<i32, Logical>,
    size: Size<i32, Logical>,
    color: [f32; 4],
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let mut buffer = SolidColorBuffer::default();
    append_buffer(&mut buffer, location, size, color, scale, elements);
}

fn append_buffer<R: MalaysiaRenderer>(
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
    if state.locked {
        reset_dead_cursor(state);
        if include_cursor {
            append_cursor(renderer, &mut elements, state, cursor);
        }
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
        }
    } else {
        crate::popup_fade::start_requested(state, renderer.gles(), scale);
        reset_dead_cursor(state);
        if include_cursor {
            append_cursor(renderer, &mut elements, state, cursor);
        }
        append_theme_fade(state, &mut elements);
        append_screenshot_overlay(state, &mut elements);
        let now = Instant::now();
        let focus_frame = state.focus_transition_frame(now);
        let slide = state.space_frame(now);
        let moving = focus_frame.map(|frame| frame.window);
        if let Some(frame) = focus_frame {
            append_focus_transition(state, renderer, frame, scale, &mut elements);
        }
        let chrome_hidden = slide.is_none() && state.layers.chrome_hidden();
        let reveal = state.reveal_frame(now);
        let reveal_drop = reveal.map(|frame| {
            let travel = frame.bar
                + if frame.titlebar {
                    vow_design::TITLEBAR.thickness as i32
                } else {
                    0
                };
            ((frame.progress - 1.0) * f64::from(travel)).round() as i32
        });
        if let (Some(frame), Some(drop)) = (reveal, reveal_drop) {
            append_layers(
                state,
                renderer,
                &mut elements,
                LayerSide::Front,
                Point::from((0, drop)),
                |entry| entry.is_desktop_bar(),
            );
            append_solid(
                Point::from((0, drop)),
                Size::from((state.logical_output_size().w, frame.bar)),
                vow_design::SPACE_GAP_COLOR.premultiplied(),
                scale,
                &mut elements,
            );
        }
        append_layers(
            state,
            renderer,
            &mut elements,
            LayerSide::Front,
            Point::from((0, 0)),
            |entry| {
                (slide.is_none() || !entry.is_desktop_bar())
                    && !(chrome_hidden && entry.is_shell_chrome())
            },
        );
        crate::popup_fade::append(state, Instant::now(), &mut elements);
        append_ime_popups(state, renderer, scale, &mut elements);
        append_popups(state, renderer, scale, &mut elements, |_| true);
        if let (Some(frame), Some(drop)) = (reveal, reveal_drop)
            && frame.titlebar
        {
            append_revealed_titlebar(
                state,
                renderer,
                frame.owner,
                drop + frame.bar,
                scale,
                &mut elements,
            );
        }
        if let Some(position) = slide {
            append_sliding_spaces(state, renderer, position, scale, moving, &mut elements);
        } else {
            let focused = state.focused_window();
            let window_ids: Vec<_> = state
                .windows
                .iter_visible(&state.minimized)
                .rev()
                .map(|window| window.id())
                .filter(|id| Some(*id) != moving)
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
                append_window_frame(state, renderer, id, focused, scale, offset, &mut elements);
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
                    vow_design::SPACE_GAP_COLOR.premultiplied(),
                    scale,
                    &mut elements,
                );
            } else {
                append_layers(
                    state,
                    renderer,
                    &mut elements,
                    LayerSide::Back,
                    Point::from((0, 0)),
                    |_| true,
                );
            }
        }
    }
    elements
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
        Size::from((width, vow_design::TITLEBAR.thickness as i32)),
    );
    let focused = state.focused_window() == Some(id);
    let pointer = state
        .chrome_pointer
        .as_ref()
        .filter(|(held, _)| *held == id)
        .map(|(_, pointer)| pointer);
    let input = vow_appearance::traffic::TrafficInput {
        focused,
        group_hovered: pointer.is_some_and(|pointer| pointer.hovered()),
        pressed: pointer.and_then(|pointer| pointer.pressed_visual()),
        zoom_supported: state.green_button_admitted(id),
    };
    let theme = state.chrome_theme;
    if let Some(element) = state.chrome.element(
        renderer.gles(),
        id,
        &title,
        focused,
        input,
        width,
        rect.loc,
        scale,
        &theme,
        program,
        physical_rect(rect, scale),
        0,
        vow_design::WINDOW.superellipse_exponent,
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
    let exponent = vow_design::WINDOW.superellipse_exponent;
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
            rect_physical,
            radius,
            exponent,
            scale,
            program,
        ) {
            Ok(rounded) => elements.push(rounded.mapped(content_physical, rect_physical).into()),
            Err(element) => elements.push((*element).into()),
        }
    }
    append_solid(
        frame.rect.loc,
        frame.rect.size,
        vow_design::SPACE_GAP_COLOR.premultiplied(),
        scale,
        elements,
    );
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
    let stride = f64::from(output.w + vow_design::SPACE_GAP);
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
                Point::from((x, 0)),
                elements,
            );
        }
        if desktop {
            append_layers(
                state,
                renderer,
                elements,
                LayerSide::Back,
                Point::from((x, 0)),
                |_| true,
            );
        }
    }
    append_solid(
        Point::from((0, 0)),
        output,
        vow_design::SPACE_GAP_COLOR.premultiplied(),
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
}

fn ssd_frame_rect(
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

fn frame_radius(
    state: crate::decoration::WindowState,
    role: crate::decoration::Role,
    client_decorated: bool,
    bar: Option<crate::window_style::Bar>,
) -> u32 {
    if state == crate::decoration::WindowState::Fullscreen {
        return 0;
    }
    crate::window_style::corner_radius(bar, role, client_decorated)
}

fn frame_has_border(state: crate::decoration::WindowState) -> bool {
    state != crate::decoration::WindowState::Fullscreen
}

fn append_window_frame<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    id: malaysia_scene::scene::WindowId,
    focused: Option<malaysia_scene::scene::WindowId>,
    scale: f64,
    offset: Point<i32, Logical>,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
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
            state: window.state(),
            role: window.role(),
            resizing: window.is_resizing(),
        }
    };

    let is_focused = focused == Some(id);

    let frame_rect_logical =
        if data.is_client_decorated || matches!(data.role, crate::decoration::Role::Dialog) {
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
        data.state,
        data.role,
        data.is_client_decorated,
        crate::window_style::bar(&data.surface),
    );
    let radius_physical =
        (crate::window_style::corner_extent(radius_logical) * scale).round() as i32;
    let exponent = vow_design::WINDOW.superellipse_exponent;

    let mut content: Vec<OutputRenderElement<R>> = Vec::new();
    if data.insets.top > 0.0 {
        let pointer = state
            .chrome_pointer
            .as_ref()
            .filter(|(held, _)| *held == id)
            .map(|(_, pointer)| pointer);
        let input = vow_appearance::traffic::TrafficInput {
            focused: is_focused,
            group_hovered: pointer.is_some_and(|pointer| pointer.hovered()),
            pressed: pointer.and_then(|pointer| pointer.pressed_visual()),
            zoom_supported: state.green_button_admitted(id),
        };
        if let Some(program) = state.rounded_clip.clone()
            && let Some(element) = state.chrome.element(
                renderer.gles(),
                id,
                &data.title,
                is_focused,
                input,
                frame_rect_logical.size.w,
                frame_rect_logical.loc,
                scale,
                &state.chrome_theme,
                program,
                frame_rect_physical,
                radius_physical,
                exponent,
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
    append_frame(
        state,
        content,
        surface_elements,
        Frame {
            owner: crate::chrome::FrameOwner::Window(id),
            rect: frame_rect_physical,
            radius: radius_physical,
            exponent,
            focused: is_focused,
            bordered: frame_has_border(data.state),
        },
        scale,
        elements,
    );
}

struct Frame {
    owner: crate::chrome::FrameOwner,
    rect: Rectangle<i32, Physical>,
    radius: i32,
    exponent: f32,
    focused: bool,
    bordered: bool,
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

fn ime_popup_location(
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
        let frame = Rectangle::new(
            Point::from((rect.x().round() as i32, rect.y().round() as i32)),
            Size::from((rect.width().round() as i32, rect.height().round() as i32)),
        );
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
        append_frame(
            state,
            Vec::new(),
            surface_elements,
            Frame {
                owner: crate::chrome::FrameOwner::Popup(id),
                rect: physical_rect(frame, scale),
                radius: (crate::window_style::corner_extent(vow_design::POPUP.corner_radius)
                    * scale)
                    .round() as i32,
                exponent: vow_design::POPUP.superellipse_exponent,
                focused: true,
                bordered: true,
            },
            scale,
            elements,
        );
    }
}

fn append_frame<R: MalaysiaRenderer>(
    state: &mut State,
    mut content: Vec<OutputRenderElement<R>>,
    surface_elements: Vec<WaylandSurfaceRenderElement<R>>,
    frame: Frame,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let frame_rect_physical = frame.rect;
    let radius_physical = frame.radius;
    let exponent = frame.exponent;
    let is_focused = frame.focused;
    if let Some(program) = state.rounded_clip.clone().filter(|_| frame.bordered) {
        for element in surface_elements {
            match crate::effects::RoundedTextureElement::from_surface_element(
                element,
                frame_rect_physical,
                radius_physical,
                exponent,
                scale,
                program.clone(),
            ) {
                Ok(rounded) => content.push(rounded.into()),
                Err(element) => content.push((*element).into()),
            }
        }
    } else {
        content.extend(surface_elements.into_iter().map(|e| e.into()));
    }

    let mut outer = None;
    let mut inner = None;
    let mut shadow = Vec::new();
    if frame.bordered {
        let palette = &state.chrome_theme.palette;
        let slot = |slot| (frame.owner, slot);
        if let Some(program) = state.border.as_ref() {
            let inner_line = palette
                .panel_inner_line
                .map(|line| (line.sides, line.edges))
                .or(palette.inner_line.map(|color| (color, color)));
            if let Some((color, edge_color)) = inner_line {
                let (area, uniforms) = border_shader(
                    frame_rect_physical,
                    radius_physical,
                    scale,
                    exponent,
                    false,
                    color,
                    edge_color,
                );
                inner = Some(
                    crate::renderer::PrimaryGpu(state.chrome.shader(
                        slot(crate::chrome::INNER_BORDER_SLOT),
                        program,
                        area,
                        1.0,
                        uniforms,
                    ))
                    .into(),
                );
            }
            let (area, uniforms) = border_shader(
                frame_rect_physical,
                radius_physical,
                scale,
                exponent,
                true,
                palette.outer_border,
                palette.outer_border,
            );
            outer = Some(
                crate::renderer::PrimaryGpu(state.chrome.shader(
                    slot(crate::chrome::OUTER_BORDER_SLOT),
                    program,
                    area,
                    1.0,
                    uniforms,
                ))
                .into(),
            );
        }
        if let Some(program) = state.shadow.as_ref() {
            shadow = state.chrome.shadow_element(
                frame.owner,
                program,
                frame_rect_physical,
                radius_physical,
                exponent,
                scale,
                &state.chrome_theme,
                is_focused,
            );
        }
    }

    elements.extend(frame_order(outer, inner, content, shadow));
}

fn frame_order<T>(outer: Option<T>, inner: Option<T>, content: Vec<T>, shadow: Vec<T>) -> Vec<T> {
    let mut frame = Vec::with_capacity(content.len() + shadow.len() + 2);
    frame.extend(outer);
    frame.extend(inner);
    frame.extend(content);
    frame.extend(shadow);
    frame
}

fn physical_rect(rect: Rectangle<i32, Logical>, scale: f64) -> Rectangle<i32, Physical> {
    rect.to_f64().to_physical_precise_up(scale)
}

fn border_shader(
    frame_rect: Rectangle<i32, Physical>,
    radius: i32,
    scale: f64,
    exponent: f32,
    outer: bool,
    color: vow_design::Rgba,
    edge_color: vow_design::Rgba,
) -> (
    Rectangle<i32, Logical>,
    Vec<smithay::backend::renderer::gles::Uniform<'static>>,
) {
    use smithay::backend::renderer::gles::Uniform;
    let area = crate::effects::shader_area(frame_rect, 1, scale);
    let area_physical: Rectangle<i32, Physical> = area.to_physical_precise_round(scale);
    let rect_local = [
        (frame_rect.loc.x - area_physical.loc.x) as f32,
        (frame_rect.loc.y - area_physical.loc.y) as f32,
        frame_rect.size.w as f32,
        frame_rect.size.h as f32,
    ];
    let uniforms = vec![
        Uniform::new("rect", rect_local),
        Uniform::new("radius", radius as f32),
        Uniform::new("width", 1.0f32),
        Uniform::new("side", if outer { 1.0f32 } else { -1.0f32 }),
        Uniform::new("scale", scale as f32),
        Uniform::new("exponent", exponent),
        Uniform::new("color", color.premultiplied()),
        Uniform::new("edge_color", edge_color.premultiplied()),
    ];
    (area, uniforms)
}

pub(crate) fn build_window_elements<R: MalaysiaRenderer>(
    renderer: &mut R,
    surface: &WlSurface,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
        renderer,
        surface,
        Point::from((0, 0)),
        scale,
        1.0,
        Kind::Unspecified,
    )
    .into_iter()
    .map(|e| e.into())
    .collect()
}

pub(crate) fn save_screenshot(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    output_size: Size<i32, Physical>,
    transform: Transform,
    request: crate::state::ScreenshotRequest,
) -> std::io::Result<ScreenshotOutcome> {
    let scale = state.scale.fractional_scale();
    let png = match request {
        crate::state::ScreenshotRequest::Focused => match state.focused_window() {
            Some(id) => capture_window_png(state, renderer, id, transform),
            None => capture_output_png(state, renderer, cursor, output_size, transform, scale),
        },
        crate::state::ScreenshotRequest::Output => {
            capture_output_png(state, renderer, cursor, output_size, transform, scale)
        }
        crate::state::ScreenshotRequest::Window(id) => {
            capture_window_png(state, renderer, id, transform)
        }
        crate::state::ScreenshotRequest::Region(region) => {
            let elements = screenshot_elements(state, renderer, cursor);
            crate::capture::screenshot_region_png(
                renderer,
                &elements,
                output_size,
                transform,
                region,
                scale,
            )
        }
    }?;

    if matches!(state.config.screenshot.target, ScreenshotTarget::Clipboard) {
        state.set_clipboard_png(png);
        Ok(ScreenshotOutcome::Copied)
    } else {
        crate::capture::save_png(&png)?;
        Ok(ScreenshotOutcome::Saved)
    }
}

pub(crate) enum ScreenshotOutcome {
    Saved,
    Copied,
}

fn screenshot_elements(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
) -> Vec<OutputRenderElement<GlesRenderer>> {
    let include_cursor = state.config.screenshot.show_pointer;
    build_output_elements(state, renderer, cursor, include_cursor)
}

fn capture_window_png(
    state: &State,
    renderer: &mut GlesRenderer,
    id: malaysia_scene::scene::WindowId,
    transform: Transform,
) -> std::io::Result<Vec<u8>> {
    let scale = state.scale.fractional_scale();
    let surface = state
        .windows
        .surface_of(id)
        .cloned()
        .ok_or_else(|| std::io::Error::other("screenshot: the window is gone"))?;
    let rect = state
        .windows
        .usable_rect(id)
        .ok_or_else(|| std::io::Error::other("screenshot: the window has no rect"))?;
    let size = rect.size.to_f64().to_physical(scale).to_i32_round();
    let elements = build_window_elements(renderer, &surface, scale);
    crate::capture::screenshot_png(
        renderer,
        &elements,
        size,
        transform,
        scale,
        [0.0, 0.0, 0.0, 0.0],
    )
}

fn capture_output_png(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    size: Size<i32, Physical>,
    transform: Transform,
    scale: f64,
) -> std::io::Result<Vec<u8>> {
    let elements = screenshot_elements(state, renderer, cursor);
    crate::capture::screenshot_png(
        renderer,
        &elements,
        size,
        transform,
        scale,
        vow_design::DESKTOP_CLEAR.premultiplied(),
    )
}

pub(crate) fn log_screenshot(result: std::io::Result<ScreenshotOutcome>) {
    match result {
        Ok(ScreenshotOutcome::Copied) => tracing::info!("screenshot copied to the clipboard"),
        Ok(ScreenshotOutcome::Saved) => {}
        Err(error) => tracing::warn!(%error, "screenshot failed"),
    }
}

fn draw(session: &mut Session) -> Result<(), DrawError> {
    let capture_size = session.backend.window_size();
    let age = session.backend.buffer_age().unwrap_or(0);
    let (damage, sync, request_redraw, mut feedback) = {
        let (renderer, mut framebuffer) = session.backend.bind().map_err(DrawError::Bind)?;

        let elements = build_output_elements(&mut session.state, renderer, &session.cursor, true);

        session.state.init_capture_dma(renderer);

        if std::mem::take(&mut session.capture_requested)
            && let Err(error) = crate::capture::capture_output(
                renderer,
                &elements,
                capture_size,
                winit_render_transform(),
                session.state.scale.fractional_scale(),
            )
        {
            tracing::warn!(%error, "debug capture failed");
        }

        if let Some(request) = session.state.take_screenshot_request() {
            log_screenshot(save_screenshot(
                &mut session.state,
                renderer,
                &session.cursor,
                capture_size,
                winit_render_transform(),
                request,
            ));
        }

        crate::capture::serve_pending(&mut session.state, renderer, &session.cursor, &elements);

        let result = session
            .damage_tracker
            .render_output(
                renderer,
                &mut framebuffer,
                age,
                &elements,
                vow_design::DESKTOP_CLEAR.premultiplied(),
            )
            .map_err(DrawError::Output)?;

        update_primary_scanout_outputs(&session.state, &result.states);
        let feedback = take_presentation_feedback(&session.state, &result.states);
        let mut damage = result.damage.cloned();
        let sync = result.sync.clone();
        drop(result);
        let mut request_redraw = false;
        let now = Instant::now();
        if session.state.animating(now) {
            request_redraw = true;
        }
        if let Some(fade) = session.state.theme_fade.as_mut() {
            match fade.tick(now) {
                crate::theme_fade::FadeTick::Done => session.state.theme_fade = None,
                _ => {
                    damage = Some(vec![Rectangle::from_size(capture_size)]);
                    session.damage_tracker = OutputDamageTracker::from_mode_source(
                        winit_mode_source(capture_size, session.state.scale.fractional_scale()),
                    );
                    request_redraw = true;
                }
            }
        }

        (damage, sync, request_redraw, feedback)
    };

    if request_redraw {
        session.backend.window().request_redraw();
    }

    if damage.is_none() {
        feedback.discarded();
        send_frame_callbacks_for_state(&session.state, Duration::from(session.state.clock.now()));
        queue_estimated_vblank(session);
        return Ok(());
    }

    let buffers = collect_drawn_buffers(&session.state);
    if !buffers.is_empty() {
        schedule_frame_release(&session.loop_handle, PendingFrameRelease { sync, buffers });
    }

    session
        .backend
        .submit(damage.as_deref())
        .map_err(DrawError::Submit)?;
    let now = Duration::from(session.state.clock.now());
    if let Some(previous) = session.last_frame_duration {
        let delta = now - previous;
        tracing::debug!(frame = session.frame_seq, ?delta, "frame presented");
    }
    session.last_frame_duration = Some(now);
    session.frame_seq = session.frame_seq.wrapping_add(1);
    feedback.presented(
        session.state.clock.now(),
        Refresh::fixed(refresh_interval(&session.state)),
        session.frame_seq,
        wp_presentation_feedback::Kind::Vsync,
    );
    if let Some(token) = session.estimated_vblank.take() {
        session.loop_handle.remove(token);
    }
    session.state.frame_callback_cycle = session.state.frame_callback_cycle.wrapping_add(1);
    send_frame_callbacks_for_state(&session.state, now);
    Ok(())
}

fn queue_estimated_vblank(session: &mut Session) {
    if session.estimated_vblank.is_some() {
        return;
    }
    let delay = next_vblank_delay(
        session.last_frame_duration,
        Duration::from(session.state.clock.now()),
        refresh_interval(&session.state),
    );
    match session.loop_handle.insert_source(
        Timer::from_duration(delay),
        |_, _, session: &mut Session| {
            session.estimated_vblank = None;
            session.state.frame_callback_cycle = session.state.frame_callback_cycle.wrapping_add(1);
            if session.state.animating(Instant::now()) {
                session.backend.window().request_redraw();
            } else {
                let now = Duration::from(session.state.clock.now());
                send_frame_callbacks_for_state(&session.state, now);
            }
            TimeoutAction::Drop
        },
    ) {
        Ok(token) => session.estimated_vblank = Some(token),
        Err(error) => tracing::warn!(%error, "scheduling the estimated vblank"),
    }
}

fn reset_dead_cursor(state: &mut State) {
    if let CursorImageStatus::Surface(surface) = &state.cursor_status
        && !surface.alive()
    {
        state.cursor_status = CursorImageStatus::default_named();
        state.client_cursor = CursorImageStatus::default_named();
    }
}

fn collect_drawn_buffers(state: &State) -> Vec<Buffer> {
    let mut buffers = Vec::new();
    if state.locked {
        for lock_surface in &state.lock_surfaces {
            if lock_surface.alive() {
                collect_tree_buffers(&mut buffers, lock_surface.wl_surface());
            }
        }
        return buffers;
    }
    for window in state.windows.iter() {
        if window.is_mapped() {
            collect_tree_buffers(&mut buffers, window.surface());
        }
    }
    for (id, _parent, _rect) in state.windows.iter_popups() {
        if let Some(surface) = state.popups.surface_of(id) {
            collect_tree_buffers(&mut buffers, surface);
        }
    }
    for entry in state.layers.all() {
        collect_tree_buffers(&mut buffers, entry.surface.wl_surface());
    }
    buffers
}

fn collect_tree_buffers(buffers: &mut Vec<Buffer>, surface: &WlSurface) {
    with_surface_tree_downward(
        surface,
        (),
        |_, states, _| {
            if let Some(state) = states.data_map.get::<RendererSurfaceStateUserData>()
                && let Some(buffer) = state.lock().unwrap().buffer().cloned()
            {
                buffers.push(buffer);
            }
            TraversalAction::DoChildren(())
        },
        |_, _, _| {},
        |_, _, _| true,
    );
}

fn schedule_frame_release(
    loop_handle: &LoopHandle<'static, Session>,
    release: PendingFrameRelease,
) {
    let Some(fd) = release.sync.export() else {
        if !release.sync.is_reached() {
            tracing::error!("frame fence is neither exportable nor signalled");
        }
        return;
    };
    let Ok(fd) = fd.try_clone() else {
        tracing::error!("could not duplicate the frame fence fd");
        return;
    };
    let source = Generic::new(fd, Interest::READ, Mode::Level);
    let mut release = Some(release);
    if let Err(error) = loop_handle.insert_source(source, move |_, _, _: &mut Session| {
        if let Some(release) = release.take() {
            tracing::trace!(count = release.buffers.len(), "released a frame's buffers");
            drop(release);
        }
        Ok(PostAction::Remove)
    }) {
        tracing::error!(%error, "could not arm the frame fence source");
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LayerSide {
    Back,
    Front,
}

fn append_layers<R: MalaysiaRenderer>(
    state: &State,
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
        for entry in state.layers.on_layer(layer).filter(|entry| include(entry)) {
            let location = physical_point(entry.geometry.loc + offset, scale);
            elements.extend(
                render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
                    renderer,
                    entry.surface.wl_surface(),
                    location,
                    scale,
                    1.0,
                    Kind::Unspecified,
                )
                .into_iter()
                .map(|e| e.into()),
            );
        }
    }
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

fn cursor_origin(
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

#[derive(Default)]
struct LastFrameCallbackCycle(Cell<Option<u32>>);

// niri a8562a1 src/niri.rs:205: hidden surfaces still get a callback about once a second.
const FRAME_CALLBACK_THROTTLE: Duration = Duration::from_millis(995);

fn for_each_frame_surface(state: &State, mut visit: impl FnMut(&WlSurface, bool)) {
    if let CursorImageStatus::Surface(surface) = &state.cursor_status {
        visit(surface, true);
    }
    if state.locked {
        for surface in state.lock_surfaces.iter().filter(|surface| surface.alive()) {
            visit(surface.wl_surface(), true);
        }
        return;
    }
    for surface in state.windows.surfaces() {
        visit(surface, !state.windows.is_placed(surface));
    }
    for entry in state.layers.all() {
        visit(entry.surface.wl_surface(), false);
    }
    for (id, _parent, _rect) in state.windows.iter_popups() {
        if let Some(surface) = state.popups.surface_of(id) {
            visit(surface, false);
        }
    }
    for popup in &state.ime_popups {
        visit(popup.wl_surface(), false);
    }
}

pub(crate) fn update_primary_scanout_outputs(state: &State, render_states: &RenderElementStates) {
    for_each_frame_surface(state, |surface, _| {
        with_surfaces_surface_tree(surface, |surface, states| {
            update_surface_primary_scanout_output(
                surface,
                &state.output,
                states,
                None,
                render_states,
                default_primary_scanout_output_compare,
            );
        });
    });
}

// niri a8562a1 src/niri.rs:5342: one callback per surface and cycle, or a frame-only client busy-loops.
pub(crate) fn send_frame_callbacks_for_state(state: &State, time: Duration) {
    let cycle = state.frame_callback_cycle;
    let output = &state.output;
    for_each_frame_surface(state, |surface, always| {
        if always {
            send_frame_callbacks(surface, time, cycle);
            return;
        }
        send_frames_surface_tree(
            surface,
            output,
            time,
            Some(FRAME_CALLBACK_THROTTLE),
            |surface, states| {
                let shown =
                    surface_primary_scanout_output(surface, states).as_ref() == Some(output);
                (shown && first_in_cycle(states, cycle)).then(|| output.clone())
            },
        );
    });
}

fn first_in_cycle(states: &SurfaceData, cycle: u32) -> bool {
    let last = states
        .data_map
        .get_or_insert(LastFrameCallbackCycle::default);
    last.0.replace(Some(cycle)) != Some(cycle)
}

// send_frames_surface_tree skips bufferless surfaces; a toplevel awaiting its first configure needs one.
fn send_frame_callbacks(surface: &WlSurface, time: Duration, cycle: u32) {
    let time_ms = time.as_millis() as u32;
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_, states, &()| {
            if !first_in_cycle(states, cycle) {
                return;
            }
            let mut cached = states.cached_state.get::<SurfaceAttributes>();
            for callback in cached.current().frame_callbacks.drain(..) {
                callback.done(time_ms);
            }
        },
        |_, _, &()| true,
    );
}

fn send_overdue_frame_callbacks(state: &State) {
    let time = Duration::from(state.clock.now());
    for_each_frame_surface(state, |surface, _| {
        send_frames_surface_tree(
            surface,
            &state.output,
            time,
            Some(FRAME_CALLBACK_THROTTLE),
            |_, _| None,
        );
    });
}

pub(crate) fn watch_overdue_frame_callbacks<D: crate::idle::IdleHost + 'static>(
    handle: &LoopHandle<'static, D>,
) {
    let timer = Timer::from_duration(FRAME_CALLBACK_THROTTLE);
    if let Err(error) = handle.insert_source(timer, |_, _, data: &mut D| {
        send_overdue_frame_callbacks(data.idle_state());
        TimeoutAction::ToDuration(FRAME_CALLBACK_THROTTLE)
    }) {
        tracing::warn!(%error, "arming the frame callback fallback timer");
    }
}

pub(crate) fn take_presentation_feedback(
    state: &State,
    render_states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut feedback = OutputPresentationFeedback::new(&state.output);
    for_each_frame_surface(state, |surface, _| {
        take_presentation_feedback_surface_tree(
            surface,
            &mut feedback,
            surface_primary_scanout_output,
            |surface, _| {
                surface_presentation_feedback_flags_from_states(surface, None, render_states)
            },
        );
    });
    feedback
}

pub(crate) fn refresh_interval(state: &State) -> Duration {
    let millihertz = state
        .output
        .current_mode()
        .map_or(60_000, |mode| mode.refresh)
        .max(1);
    Duration::from_micros(1_000_000_000 / millihertz as u64)
}

pub(crate) fn next_vblank_delay(
    last_vblank: Option<Duration>,
    now: Duration,
    refresh: Duration,
) -> Duration {
    let Some(last) = last_vblank.filter(|last| *last <= now) else {
        return refresh;
    };
    let since = (now - last).as_nanos() % refresh.as_nanos();
    refresh - Duration::from_nanos(since as u64)
}

#[derive(Debug, thiserror::Error)]
enum DrawError {
    #[error("could not bind the renderer to the window")]
    Bind(#[source] SwapBuffersError),
    #[error("failed to render the output")]
    Output(#[source] DamageError<GlesError>),
    #[error("could not present the frame")]
    Submit(#[source] SwapBuffersError),
}

impl crate::idle::IdleHost for Session {
    fn idle_state(&mut self) -> &mut State {
        &mut self.state
    }

    fn request_redraw(&mut self) {
        self.backend.window().request_redraw();
    }
}

impl crate::blockers::BlockerSink for LoopHandle<'static, Session> {
    fn insert(
        &mut self,
        source: crate::blockers::BlockerSource,
        client: smithay::reexports::wayland_server::Client,
    ) -> bool {
        self.insert_source(source, move |_, _, session: &mut Session| {
            if let Some(client_state) = client.get_data::<ClientState>() {
                let dh = session.state.display_handle.clone();
                client_state
                    .compositor_state
                    .blocker_cleared(&mut session.state, &dh);
            }
            Ok(())
        })
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fmt;

    use smithay::backend::allocator::{Format, Fourcc, Modifier};
    use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
    use smithay::backend::renderer::damage::OutputDamageTracker;
    use smithay::backend::renderer::element::Kind;
    use smithay::backend::renderer::element::memory::{
        MemoryRenderBuffer, MemoryRenderBufferRenderElement,
    };
    use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
    use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
    use smithay::utils::{
        Buffer as BufferCoord, Logical, Physical, Point, Rectangle, Size, Transform,
    };

    use super::{
        cursor_origin, describe_chain, frame_has_border, frame_order, frame_radius,
        import_pending_dmabufs, physical_point, physical_rect, winit_mode_source,
    };
    use crate::cursor::OutputRenderElement;
    use crate::state::State;

    use sctk::output::{OutputHandler, OutputState};
    use sctk::reexports::client::protocol::{
        wl_buffer, wl_buffer::WlBuffer, wl_output, wl_shm, wl_shm_pool, wl_surface,
    };
    use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
    use sctk::reexports::protocols::wp::linux_dmabuf::zv1::client::{
        zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1,
    };

    #[derive(Debug)]
    struct Leaf;

    impl fmt::Display for Leaf {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("the inner cause")
        }
    }

    impl Error for Leaf {}

    #[derive(Debug)]
    struct Wrapper(Leaf);

    impl fmt::Display for Wrapper {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("the outer message")
        }
    }

    impl Error for Wrapper {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn an_ime_popup_sits_below_the_text_cursor() {
        let output = Rectangle::from_size(Size::from((1000, 800)));
        let cursor = Rectangle::new(Point::from((40, 30)), Size::from((2, 20)));
        let location = super::ime_popup_location(
            Point::from((100, 100)),
            cursor,
            Size::from((200, 60)),
            output,
        );
        assert_eq!(location, Point::from((140, 150)));
    }

    #[test]
    fn an_ime_popup_flips_above_the_cursor_at_the_bottom_and_stays_inside_the_output() {
        let output = Rectangle::from_size(Size::from((1000, 800)));
        let cursor = Rectangle::new(Point::from((880, 700)), Size::from((2, 20)));
        let location =
            super::ime_popup_location(Point::from((50, 50)), cursor, Size::from((200, 60)), output);
        assert_eq!(location, Point::from((800, 690)));
    }

    #[test]
    fn the_chain_reports_every_cause() {
        assert_eq!(
            describe_chain(&Wrapper(Leaf)),
            "the outer message: the inner cause"
        );
    }

    #[test]
    fn a_lone_error_is_its_own_description() {
        assert_eq!(describe_chain(&Leaf), "the inner cause");
    }

    #[test]
    fn only_fullscreen_drops_the_frame() {
        use crate::decoration::{Role, WindowState};

        for state in [WindowState::Normal, WindowState::Maximized] {
            assert_eq!(
                frame_radius(state, Role::Toplevel, false, None),
                vow_design::WINDOW.corner_radius
            );
            assert!(frame_has_border(state));
        }
        assert_eq!(
            frame_radius(WindowState::Fullscreen, Role::Toplevel, true, None),
            0
        );
        assert!(!frame_has_border(WindowState::Fullscreen));
    }

    #[test]
    fn physical_point_floors_position() {
        assert_eq!(
            physical_point(Point::<i32, Logical>::from((2, 2)), 1.25),
            Point::<i32, Physical>::from((2, 2))
        );
        assert_eq!(
            physical_point(Point::<i32, Logical>::from((-2, -2)), 1.25),
            Point::<i32, Physical>::from((-3, -3))
        );
    }

    #[test]
    fn adjacent_logical_rectangles_share_physical_edge() {
        for scale in [1.25, 1.5] {
            let left = Rectangle::<i32, Logical>::new(Point::from((0, 0)), Size::from((101, 100)));
            let right =
                Rectangle::<i32, Logical>::new(Point::from((101, 0)), Size::from((100, 100)));
            let left_physical = physical_rect(left, scale);
            let right_physical = physical_rect(right, scale);
            assert!(
                left_physical.loc.x + left_physical.size.w > right_physical.loc.x,
                "adjacent rectangles must share the physical boundary pixel at scale {scale}; \
                 left ends at {}, right starts at {}",
                left_physical.loc.x + left_physical.size.w,
                right_physical.loc.x,
            );
        }
    }

    const RED: [u8; 4] = [0, 0, 255, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 255];

    #[allow(unsafe_code)]
    fn offscreen_renderer() -> GlesRenderer {
        let device = EGLDevice::enumerate()
            .expect("EGL device enumeration should work")
            .next()
            .expect("the pixel test needs an EGL device");
        // SAFETY: the display owns `device`, so the native handle lives as long as the display.
        let display = unsafe { EGLDisplay::new(device) }.expect("the test needs an EGL display");
        let context = EGLContext::new(&display).expect("the test needs an EGL context");
        // SAFETY: `context` was just created on this thread and is not shared with another.
        unsafe { GlesRenderer::new(context) }.expect("the test needs a GLES renderer")
    }

    fn render_and_read(
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        elements: &[OutputRenderElement<GlesRenderer>],
    ) -> Vec<u8> {
        let buffer_size: Size<i32, BufferCoord> = Size::from((size.w, size.h));
        let mut target =
            Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Argb8888, buffer_size)
                .expect("the test needs an offscreen buffer");
        let mut framebuffer = renderer.bind(&mut target).expect("the buffer should bind");

        let mut tracker = OutputDamageTracker::from_mode_source(winit_mode_source(size, 1.0));
        tracker
            .render_output(
                renderer,
                &mut framebuffer,
                0,
                elements,
                [0.0, 0.0, 0.0, 1.0],
            )
            .expect("the frame should render");

        let mapping = renderer
            .copy_framebuffer(
                &framebuffer,
                Rectangle::from_size(buffer_size),
                Fourcc::Argb8888,
            )
            .expect("the framebuffer should be readable");
        let pixels = renderer
            .map_texture(&mapping)
            .expect("the mapping should read");
        let mut window_order = pixels.to_vec();
        let stride = (size.w * 4) as usize;
        for row in 0..(size.h as usize) / 2 {
            let top = row * stride;
            let bottom = (size.h as usize - 1 - row) * stride;
            for byte in 0..stride {
                window_order.swap(top + byte, bottom + byte);
            }
        }
        window_order
    }

    fn pixel(pixels: &[u8], size: Size<i32, Physical>, x: i32, y: i32) -> [u8; 4] {
        let stride = (size.w * 4) as usize;
        let offset = (y as usize) * stride + (x as usize) * 4;
        pixels[offset..offset + 4]
            .try_into()
            .expect("the readback should cover the whole frame")
    }

    fn solid(
        renderer: &mut GlesRenderer,
        color: [u8; 4],
        side: i32,
    ) -> OutputRenderElement<GlesRenderer> {
        let buffer = MemoryRenderBuffer::from_slice(
            &color.repeat((side * side) as usize),
            Fourcc::Argb8888,
            (side, side),
            1,
            Transform::Normal,
            None,
        );
        let element = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (0.0, 0.0),
            &buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .expect("the element should build");
        element.into()
    }

    #[test]
    fn the_frame_is_outer_border_inner_line_content_then_shadow() {
        let frame = frame_order(
            Some("outer"),
            Some("inner"),
            vec!["content"],
            vec!["shadow"],
        );
        assert_eq!(frame, ["outer", "inner", "content", "shadow"]);
    }

    #[test]
    fn a_top_left_quadrant_stays_in_the_top_left() {
        let size = Size::from((64, 64));
        let mut renderer = offscreen_renderer();
        let quadrant = solid(&mut renderer, RED, 32);
        let pixels = render_and_read(&mut renderer, size, &[quadrant]);

        assert_eq!(
            pixel(&pixels, size, 4, 4),
            RED,
            "the element is at render (0, 0), so its pixels must appear at the window's top-left"
        );
        assert_eq!(
            pixel(&pixels, size, 4, 60),
            CLEAR,
            "the bottom-left must stay clear: an inverted frame puts the element there"
        );
        assert_eq!(
            pixel(&pixels, size, 60, 4),
            CLEAR,
            "the top-right must stay clear"
        );
    }

    #[test]
    fn an_enlarged_cursor_fills_its_scaled_rectangle() {
        let size = Size::from((64, 64));
        let mut renderer = offscreen_renderer();
        let cursor = MemoryRenderBuffer::from_slice(
            &GREEN.repeat(16),
            Fourcc::Argb8888,
            (4, 4),
            1,
            Transform::Normal,
            None,
        );
        let source = Rectangle::from_size(Size::from((4.0, 4.0)));
        let element = MemoryRenderBufferRenderElement::from_buffer(
            &mut renderer,
            Point::from((0.0, 0.0)),
            &cursor,
            None,
            Some(source),
            Some(Size::from((12, 12))),
            Kind::Cursor,
        )
        .expect("the enlarged cursor element should build");
        let pixels = render_and_read(&mut renderer, size, &[element.into()]);

        assert_eq!(
            pixel(&pixels, size, 10, 10),
            GREEN,
            "an enlarged cursor must paint the far corner of its destination"
        );
    }

    #[test]
    fn the_cursor_lands_at_the_hotspot_adjusted_position() {
        let size = Size::from((64, 64));
        let mut renderer = offscreen_renderer();

        let cursor = MemoryRenderBuffer::from_slice(
            &GREEN.repeat(16),
            Fourcc::Argb8888,
            (4, 4),
            1,
            Transform::Normal,
            None,
        );
        let origin = cursor_origin(Point::from((10.0, 10.0)), Point::from((3.0, 4.0)), 1.0);
        assert_eq!(origin, Point::from((7.0, 6.0)));
        let element = MemoryRenderBufferRenderElement::from_buffer(
            &mut renderer,
            origin,
            &cursor,
            None,
            None,
            None,
            Kind::Cursor,
        )
        .expect("the cursor element should build");
        let pixels = render_and_read(&mut renderer, size, &[element.into()]);

        assert_eq!(
            pixel(&pixels, size, 7, 6),
            GREEN,
            "the cursor image's top-left must land at the pointer minus the hotspot"
        );
        assert_eq!(
            pixel(&pixels, size, 6, 6),
            CLEAR,
            "nothing may be drawn left of the hotspot-adjusted origin"
        );
        assert_eq!(
            pixel(&pixels, size, 7, 5),
            CLEAR,
            "nothing may be drawn above the hotspot-adjusted origin"
        );
    }

    struct TestClient {
        registry: sctk::registry::RegistryState,
        compositor: Option<sctk::compositor::CompositorState>,
        output: OutputState,
        dmabuf: sctk::dmabuf::DmabufState,
    }

    impl TestClient {
        fn new(
            globals: &sctk::reexports::client::globals::GlobalList,
            qh: &QueueHandle<TestClient>,
        ) -> Self {
            Self {
                registry: sctk::registry::RegistryState::new(globals),
                compositor: sctk::compositor::CompositorState::bind(globals, qh).ok(),
                output: OutputState::new(globals, qh),
                dmabuf: sctk::dmabuf::DmabufState::new(globals, qh),
            }
        }
    }

    impl sctk::registry::ProvidesRegistryState for TestClient {
        fn registry(&mut self) -> &mut sctk::registry::RegistryState {
            &mut self.registry
        }

        fn runtime_add_global(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: u32,
            _: &str,
            _: u32,
        ) {
        }

        fn runtime_remove_global(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: u32,
            _: &str,
        ) {
        }
    }

    sctk::delegate_registry!(TestClient);

    impl OutputHandler for TestClient {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.output
        }

        fn new_output(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: wl_output::WlOutput,
        ) {
        }

        fn update_output(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: wl_output::WlOutput,
        ) {
        }

        fn output_destroyed(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: wl_output::WlOutput,
        ) {
        }
    }

    sctk::delegate_output!(TestClient);

    impl Dispatch<wl_shm::WlShm, ()> for TestClient {
        fn event(
            _state: &mut TestClient,
            _: &wl_shm::WlShm,
            _: wl_shm::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<TestClient>,
        ) {
        }
    }

    impl Dispatch<wl_shm_pool::WlShmPool, ()> for TestClient {
        fn event(
            _state: &mut TestClient,
            _: &wl_shm_pool::WlShmPool,
            _: wl_shm_pool::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<TestClient>,
        ) {
        }
    }

    impl Dispatch<wl_buffer::WlBuffer, ()> for TestClient {
        fn event(
            _state: &mut TestClient,
            _: &wl_buffer::WlBuffer,
            _: wl_buffer::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<TestClient>,
        ) {
        }
    }

    impl sctk::compositor::CompositorHandler for TestClient {
        fn scale_factor_changed(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: &wl_surface::WlSurface,
            _: i32,
        ) {
        }

        fn transform_changed(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: &wl_surface::WlSurface,
            _: wl_output::Transform,
        ) {
        }

        fn frame(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: &wl_surface::WlSurface,
            _: u32,
        ) {
        }

        fn surface_enter(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: &wl_surface::WlSurface,
            _: &wl_output::WlOutput,
        ) {
        }

        fn surface_leave(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: &wl_surface::WlSurface,
            _: &wl_output::WlOutput,
        ) {
        }
    }

    sctk::delegate_compositor!(TestClient);

    impl sctk::dmabuf::DmabufHandler for TestClient {
        fn dmabuf_state(&mut self) -> &mut sctk::dmabuf::DmabufState {
            &mut self.dmabuf
        }

        fn dmabuf_feedback(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
            _: sctk::dmabuf::DmabufFeedback,
        ) {
        }

        fn created(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
            _: WlBuffer,
        ) {
        }

        fn failed(
            &mut self,
            _: &Connection,
            _: &QueueHandle<TestClient>,
            _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        ) {
        }

        fn released(&mut self, _: &Connection, _: &QueueHandle<TestClient>, _: &WlBuffer) {}
    }

    sctk::delegate_dmabuf!(TestClient);

    #[test]
    fn the_global_filter_does_not_stall_the_next_client() {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        use sctk::reexports::client::Connection;
        use sctk::reexports::client::globals::registry_queue_init;
        use smithay::reexports::wayland_server::Display;

        let event_loop =
            smithay::reexports::calloop::EventLoop::<crate::session::Session>::try_new()
                .expect("the test needs an event loop");
        let mut display = Display::<State>::new().expect("the test needs a display");
        let disconnect_queue = crate::state::ClientDisconnectQueue::default();
        let (disconnect_ping, _disconnect_source) =
            smithay::reexports::calloop::ping::make_ping().expect("the test needs a ping");
        let feedback = smithay::wayland::dmabuf::DmabufFeedbackBuilder::new(
            0,
            [Format {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::Invalid,
            }],
        )
        .build()
        .expect("the feedback should build");
        let mut state = State::new(
            event_loop.get_signal(),
            &display.handle(),
            Size::from((1280, 720)),
            feedback,
            None,
            Box::new(event_loop.handle()),
            Box::new(()),
        )
        .expect("the state should build");

        fn registry_globals(stream: std::os::unix::net::UnixStream) -> Vec<String> {
            let conn = Connection::from_socket(stream).expect("the client should connect");
            let (globals, _queue) =
                registry_queue_init::<TestClient>(&conn).expect("the registry should init");
            globals
                .contents()
                .clone_list()
                .into_iter()
                .map(|global| global.interface)
                .collect()
        }

        let mut handle = display.handle();

        let (first_side, first_server) =
            std::os::unix::net::UnixStream::pair().expect("a socket pair");
        let (first_tx, first_rx) = mpsc::channel();
        std::thread::spawn(move || {
            first_tx
                .send(registry_globals(first_side))
                .expect("the main thread should read the globals");
        });
        handle
            .insert_client(
                first_server,
                std::sync::Arc::new(crate::state::ClientState::new(
                    disconnect_queue.clone(),
                    disconnect_ping.clone(),
                    false,
                )),
            )
            .expect("the server should accept the first client");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            display.dispatch_clients(&mut state).expect("dispatch");
            display.flush_clients().expect("flush");
            if let Ok(globals) = first_rx.try_recv() {
                assert!(globals.iter().any(|name| name == "wl_compositor"));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the first client never received its globals"
            );
        }

        let (second_side, second_server) =
            std::os::unix::net::UnixStream::pair().expect("a socket pair");
        let (second_tx, second_rx) = mpsc::channel();
        std::thread::spawn(move || {
            second_tx
                .send(registry_globals(second_side))
                .expect("the main thread should read the globals");
        });
        handle
            .insert_client(
                second_server,
                std::sync::Arc::new(crate::state::ClientState::new(
                    disconnect_queue.clone(),
                    disconnect_ping.clone(),
                    false,
                )),
            )
            .expect("the server should accept the second client");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            display.dispatch_clients(&mut state).expect("dispatch");
            display.flush_clients().expect("flush");
            if let Ok(globals) = second_rx.try_recv() {
                assert!(
                    globals.iter().any(|name| name == "wl_compositor"),
                    "the second client must receive the globals"
                );
                assert!(
                    !globals
                        .iter()
                        .any(|name| name == "infinity_window_manager_v1"),
                    "an untrusted client must not see the Infinity global"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "a client connected and the compositor stopped dispatching"
            );
        }
    }

    #[test]
    fn a_failed_create_immed_is_drained_and_posts_the_protocol_error() {
        use std::os::unix::io::AsFd;
        use std::sync::mpsc;

        use sctk::reexports::client::Connection;
        use sctk::reexports::client::globals::registry_queue_init;
        use smithay::reexports::wayland_server::Display;

        let event_loop =
            smithay::reexports::calloop::EventLoop::<crate::session::Session>::try_new()
                .expect("the test needs an event loop");
        let mut display = Display::<State>::new().expect("the test needs a display");
        let disconnect_queue = crate::state::ClientDisconnectQueue::default();
        let (disconnect_ping, _disconnect_source) =
            smithay::reexports::calloop::ping::make_ping().expect("the test needs a ping");
        let mut renderer = offscreen_renderer();

        let feedback = smithay::wayland::dmabuf::DmabufFeedbackBuilder::new(
            0,
            [Format {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::Invalid,
            }],
        )
        .build()
        .expect("the feedback should build");
        let mut state = State::new(
            event_loop.get_signal(),
            &display.handle(),
            Size::from((1280, 720)),
            feedback,
            None,
            Box::new(event_loop.handle()),
            Box::new(()),
        )
        .expect("the state should build");

        let (client_side, server_side) =
            std::os::unix::net::UnixStream::pair().expect("a socket pair");
        let (sent_tx, sent_rx) = mpsc::channel::<bool>();
        let mut handle = display.handle();

        std::thread::spawn(move || {
            let conn = Connection::from_socket(client_side).expect("the client should connect");
            let (globals, mut event_queue) =
                registry_queue_init(&conn).expect("the registry should init");
            let qh = event_queue.handle();
            let mut client = TestClient::new(&globals, &qh);
            event_queue
                .roundtrip(&mut client)
                .expect("the bind should roundtrip");

            let (fake_fd, _sink) = std::os::unix::net::UnixStream::pair().expect("a fake plane fd");
            let params = client
                .dmabuf
                .create_params(&qh)
                .expect("the compositor advertises zwp_linux_dmabuf_v1");
            params.add(fake_fd.as_fd(), 0, 0, 4 * 16, u64::from(Modifier::Invalid));
            let (buffer, _params) = params.create_immed(
                16,
                16,
                Fourcc::Xrgb8888 as u32,
                zwp_linux_buffer_params_v1::Flags::empty(),
                &qh,
            );

            let surface = client
                .compositor
                .as_ref()
                .expect("compositor bound")
                .create_surface(&qh);
            surface.attach(Some(&buffer), 0, 0);
            surface.commit();
            event_queue
                .roundtrip(&mut client)
                .expect("the batch should reach the compositor");
            sent_tx
                .send(false)
                .expect("the main thread should read the signal");

            let dead = event_queue.roundtrip(&mut client).is_err();
            sent_tx
                .send(dead)
                .expect("the main thread should read the verdict");
        });

        handle
            .insert_client(
                server_side,
                std::sync::Arc::new(crate::state::ClientState::new(
                    disconnect_queue.clone(),
                    disconnect_ping.clone(),
                    false,
                )),
            )
            .expect("the server should accept the client");

        let mut spins = 0;
        loop {
            display.dispatch_clients(&mut state).expect("dispatch");
            display.flush_clients().expect("flush");
            if matches!(sent_rx.try_recv(), Ok(false)) {
                break;
            }
            spins += 1;
            assert!(spins < 100_000, "the client never signalled its batch");
        }

        while state.pending_dmabuf_imports.is_empty() {
            display.dispatch_clients(&mut state).expect("dispatch");
            display.flush_clients().expect("flush");
            spins += 1;
            assert!(spins < 1000, "the import was never queued");
        }
        assert_eq!(
            state.pending_dmabuf_imports.len(),
            1,
            "the single create_immed queues exactly one import"
        );

        import_pending_dmabufs(&mut state, &mut renderer);
        assert!(
            state.pending_dmabuf_imports.is_empty(),
            "the queue must be drained before a frame can be drawn"
        );
        display
            .flush_clients()
            .expect("the error must reach the client");

        let dead = sent_rx
            .recv()
            .expect("the client should report its verdict");
        assert!(
            dead,
            "the failed create_immed must kill the client, never leave a black window"
        );
    }

    #[test]
    fn without_a_syncobj_device_no_explicit_sync_global_is_announced() {
        use smithay::backend::allocator::{Format, Fourcc, Modifier};
        use smithay::reexports::wayland_server::Display;
        use smithay::wayland::dmabuf::DmabufFeedbackBuilder;

        let event_loop =
            smithay::reexports::calloop::EventLoop::<crate::session::Session>::try_new()
                .expect("the test needs an event loop");
        let display = Display::<State>::new().expect("the test needs a display");
        let feedback = DmabufFeedbackBuilder::new(
            0,
            [Format {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::Invalid,
            }],
        )
        .build()
        .expect("the feedback should build");
        let state = State::new(
            event_loop.get_signal(),
            &display.handle(),
            Size::from((1280, 720)),
            feedback,
            None,
            Box::new(event_loop.handle()),
            Box::new(()),
        )
        .expect("the state should build");
        assert!(
            state.syncobj_state.is_none(),
            "without a device there must be no wp_linux_drm_syncobj_manager_v1 global"
        );
    }

    #[test]
    fn a_surface_without_a_current_buffer_is_not_visible() {
        use std::os::unix::io::AsFd;
        use std::sync::mpsc;

        use sctk::reexports::client::Connection;
        use sctk::reexports::client::globals::registry_queue_init;
        use smithay::backend::allocator::{Format, Fourcc, Modifier};
        use smithay::reexports::wayland_server::Display;
        use smithay::reexports::wayland_server::Resource;
        use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

        let event_loop =
            smithay::reexports::calloop::EventLoop::<crate::session::Session>::try_new()
                .expect("the test needs an event loop");
        let mut display = Display::<State>::new().expect("the test needs a display");
        let disconnect_queue = crate::state::ClientDisconnectQueue::default();
        let (disconnect_ping, _disconnect_source) =
            smithay::reexports::calloop::ping::make_ping().expect("the test needs a ping");

        let feedback = smithay::wayland::dmabuf::DmabufFeedbackBuilder::new(
            0,
            [Format {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::Invalid,
            }],
        )
        .build()
        .expect("the feedback should build");
        let mut state = State::new(
            event_loop.get_signal(),
            &display.handle(),
            Size::from((1280, 720)),
            feedback,
            None,
            Box::new(event_loop.handle()),
            Box::new(()),
        )
        .expect("the state should build");

        let (client_side, server_side) =
            std::os::unix::net::UnixStream::pair().expect("a socket pair");
        let (sent_tx, sent_rx) = mpsc::channel::<bool>();

        std::thread::spawn(move || {
            let conn = Connection::from_socket(client_side).expect("the client should connect");
            let (globals, mut event_queue) =
                registry_queue_init(&conn).expect("the registry should init");
            let qh = event_queue.handle();
            let mut client = TestClient::new(&globals, &qh);
            event_queue
                .roundtrip(&mut client)
                .expect("the bind should roundtrip");

            let surface = client
                .compositor
                .as_ref()
                .expect("compositor bound")
                .create_surface(&qh);
            surface.commit();
            event_queue
                .roundtrip(&mut client)
                .expect("the no-buffer commit should reach the compositor");
            sent_tx
                .send(false)
                .expect("the main thread should read the first signal");

            let shm = globals
                .bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())
                .expect("the compositor advertises wl_shm");
            let path =
                std::env::temp_dir().join(format!("malaysia-shm-test-{}", std::process::id()));
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)
                .expect("a temp file for the shm pool");
            file.set_len(64 * 64 * 4)
                .expect("the pool fits a 64x64 buffer");
            let pool = shm.create_pool(file.as_fd(), 64 * 64 * 4, &qh, ());
            let buffer = pool.create_buffer(0, 64, 64, 64 * 4, wl_shm::Format::Argb8888, &qh, ());
            surface.attach(Some(&buffer), 0, 0);
            surface.commit();
            std::fs::remove_file(&path).ok();
            event_queue
                .roundtrip(&mut client)
                .expect("the buffered commit should reach the compositor");
            sent_tx
                .send(true)
                .expect("the main thread should read the second signal");
        });

        let mut handle = display.handle();
        let client = handle
            .insert_client(
                server_side,
                std::sync::Arc::new(crate::state::ClientState::new(
                    disconnect_queue.clone(),
                    disconnect_ping.clone(),
                    false,
                )),
            )
            .expect("the server should accept the client");

        fn surface_of_client(
            handle: &smithay::reexports::wayland_server::DisplayHandle,
            client: &smithay::reexports::wayland_server::Client,
        ) -> Option<WlSurface> {
            let backend = handle.backend_handle();
            let mut ids = Vec::new();
            let _ = backend.with_all_objects_for(client.id(), |object_id| ids.push(object_id));
            let surface_id = ids.into_iter().find(|id| {
                handle
                    .object_info(id.clone())
                    .map(|info| info.interface.name == "wl_surface")
                    .unwrap_or(false)
            })?;
            WlSurface::from_id(handle, surface_id).ok()
        }

        let mut spins = 0;
        loop {
            display.dispatch_clients(&mut state).expect("dispatch");
            display.flush_clients().expect("flush");
            if matches!(sent_rx.try_recv(), Ok(false)) {
                break;
            }
            spins += 1;
            assert!(
                spins < 100_000,
                "the client never committed without a buffer"
            );
        }
        let surface = surface_of_client(&handle, &client)
            .expect("the client should have created a wl_surface");
        assert!(
            !crate::window::surface_has_buffer(&surface),
            "a surface that never attached a buffer has no current buffer"
        );

        loop {
            display.dispatch_clients(&mut state).expect("dispatch");
            display.flush_clients().expect("flush");
            if matches!(sent_rx.try_recv(), Ok(true)) {
                break;
            }
            spins += 1;
            assert!(spins < 100_000, "the client never committed with a buffer");
        }
        assert!(
            crate::window::surface_has_buffer(&surface),
            "a surface with an attached buffer has a current buffer"
        );
    }

    #[test]
    fn ssd_frame_rect_uses_committed_geometry_plus_insets() {
        use smithay::utils::{Point, Rectangle, Size};

        let insets = crate::decoration::Insets {
            top: 32.0,
            left: 0.0,
            right: 0.0,
            bottom: 0.0,
        };
        let configured = Size::from((1280, 708));
        let geometry = Rectangle::from_size(Size::from((640, 420)));
        let rect = super::ssd_frame_rect(Point::from((0, 0)), configured, geometry, insets, false);
        assert_eq!(
            rect.size,
            Size::from((640, 452)),
            "frame size is committed geometry + insets"
        );
    }

    #[test]
    fn ssd_frame_rect_keeps_configured_size_while_resizing() {
        use smithay::utils::{Point, Rectangle, Size};

        let insets = crate::decoration::Insets::ZERO;
        let configured = Size::from((800, 600));
        let geometry = Rectangle::from_size(Size::from((640, 420)));
        let rect =
            super::ssd_frame_rect(Point::from((100, 50)), configured, geometry, insets, true);
        assert_eq!(
            rect.size, configured,
            "during resize the frame follows the configured/pointer size"
        );
        assert_eq!(
            rect.loc,
            Point::from((100, 50)),
            "location anchors the dragged edge"
        );
    }

    #[test]
    fn unplaced_toplevel_gets_frame_callbacks() {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        use nix::poll::{PollFd, PollFlags, poll};

        use sctk::reexports::client::globals::GlobalListContents;
        use sctk::reexports::client::globals::registry_queue_init;
        use sctk::reexports::client::protocol::{
            wl_callback, wl_compositor, wl_registry, wl_surface,
        };
        use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
        use sctk::reexports::protocols::xdg::shell::client::{
            xdg_surface, xdg_toplevel, xdg_wm_base,
        };
        use smithay::reexports::wayland_server::Display;
        use smithay::utils::Size;

        let event_loop =
            smithay::reexports::calloop::EventLoop::<crate::session::Session>::try_new()
                .expect("the test needs an event loop");
        let mut display = Display::<State>::new().expect("the test needs a display");
        let disconnect_queue = crate::state::ClientDisconnectQueue::default();
        let (disconnect_ping, _disconnect_source) =
            smithay::reexports::calloop::ping::make_ping().expect("the test needs a ping");
        let feedback = smithay::wayland::dmabuf::DmabufFeedbackBuilder::new(
            0,
            [Format {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::Invalid,
            }],
        )
        .build()
        .expect("the feedback should build");
        let mut state = State::new(
            event_loop.get_signal(),
            &display.handle(),
            Size::from((1280, 720)),
            feedback,
            None,
            Box::new(event_loop.handle()),
            Box::new(()),
        )
        .expect("the state should build");

        let (client_side, server_side) =
            std::os::unix::net::UnixStream::pair().expect("a socket pair");
        let (done_tx, done_rx) = mpsc::channel::<()>();

        let client_handle = std::thread::spawn(move || {
            struct Client {
                frame_done: bool,
                done_tx: mpsc::Sender<()>,
            }

            impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Client {
                fn event(
                    _: &mut Self,
                    _: &wl_registry::WlRegistry,
                    _: wl_registry::Event,
                    _: &GlobalListContents,
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                }
            }

            impl Dispatch<wl_compositor::WlCompositor, ()> for Client {
                fn event(
                    _: &mut Self,
                    _: &wl_compositor::WlCompositor,
                    _: wl_compositor::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                }
            }

            impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
                fn event(
                    _: &mut Self,
                    wm_base: &xdg_wm_base::XdgWmBase,
                    event: xdg_wm_base::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                    if let xdg_wm_base::Event::Ping { serial } = event {
                        wm_base.pong(serial);
                    }
                }
            }

            impl Dispatch<xdg_surface::XdgSurface, ()> for Client {
                fn event(
                    _: &mut Self,
                    surface: &xdg_surface::XdgSurface,
                    event: xdg_surface::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                    if let xdg_surface::Event::Configure { serial } = event {
                        surface.ack_configure(serial);
                    }
                }
            }

            impl Dispatch<xdg_toplevel::XdgToplevel, ()> for Client {
                fn event(
                    _: &mut Self,
                    _: &xdg_toplevel::XdgToplevel,
                    _: xdg_toplevel::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                }
            }

            impl Dispatch<wl_callback::WlCallback, ()> for Client {
                fn event(
                    state: &mut Self,
                    _: &wl_callback::WlCallback,
                    event: wl_callback::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                    if let wl_callback::Event::Done { .. } = event {
                        state.frame_done = true;
                        let _ = state.done_tx.send(());
                    }
                }
            }

            impl Dispatch<wl_surface::WlSurface, ()> for Client {
                fn event(
                    _: &mut Self,
                    _: &wl_surface::WlSurface,
                    _: wl_surface::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                }
            }

            let conn = Connection::from_socket(client_side).expect("client connects");
            let (globals, mut event_queue) =
                registry_queue_init(&conn).expect("registry initializes");
            let qh = event_queue.handle();
            let mut client = Client {
                frame_done: false,
                done_tx,
            };

            let compositor = globals
                .bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=5, ())
                .expect("compositor global exists");
            let xdg_wm = globals
                .bind::<xdg_wm_base::XdgWmBase, _, _>(&qh, 1..=6, ())
                .expect("xdg_wm_base global exists");

            let surface = compositor.create_surface(&qh, ());
            let xdg_surface = xdg_wm.get_xdg_surface(&surface, &qh, ());
            let _toplevel = xdg_surface.get_toplevel(&qh, ());
            surface.frame(&qh, ());
            surface.commit();

            event_queue
                .roundtrip(&mut client)
                .expect("initial configure reaches the client");

            let mut spins = 0;
            while !client.frame_done {
                event_queue
                    .blocking_dispatch(&mut client)
                    .expect("frame callback is delivered");
                spins += 1;
                assert!(spins < 1000, "frame callback never arrived");
            }
        });

        let mut handle = display.handle();
        let _client = handle
            .insert_client(
                server_side,
                std::sync::Arc::new(crate::state::ClientState::new(
                    disconnect_queue.clone(),
                    disconnect_ping.clone(),
                    false,
                )),
            )
            .expect("the server accepts the client");

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            display.dispatch_clients(&mut state).expect("dispatch");
            display.flush_clients().expect("flush");
            if state.windows.len() == 1 {
                break;
            }
            if Instant::now() >= deadline {
                panic!("the toplevel was never tracked");
            }
            let fd = display.backend().poll_fd();
            let poll_fd = PollFd::new(fd, PollFlags::POLLIN);
            let _ = poll(&mut [poll_fd], 1u8);
        }

        let unplaced = state
            .windows
            .surfaces()
            .next()
            .map(|surface| !state.windows.is_placed(surface))
            .unwrap_or(false);
        assert!(
            unplaced,
            "the toplevel must not be placed before its first buffer"
        );

        super::send_frame_callbacks_for_state(&state, Duration::from_millis(1));
        display.flush_clients().expect("flush callbacks");

        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the client should receive the frame callback");

        if let Err(error) = client_handle.join() {
            std::panic::resume_unwind(error);
        }
    }

    #[test]
    fn the_estimated_vblank_lands_on_the_refresh_grid() {
        let refresh = std::time::Duration::from_millis(16);
        let delay = super::next_vblank_delay(
            Some(std::time::Duration::from_millis(10)),
            std::time::Duration::from_millis(35),
            refresh,
        );
        assert_eq!(delay, std::time::Duration::from_millis(7));
    }

    #[test]
    fn without_a_past_vblank_the_estimate_waits_one_refresh() {
        let refresh = std::time::Duration::from_millis(16);
        let now = std::time::Duration::from_millis(35);
        assert_eq!(super::next_vblank_delay(None, now, refresh), refresh);
        assert_eq!(
            super::next_vblank_delay(Some(std::time::Duration::from_millis(50)), now, refresh),
            refresh
        );
    }
}
