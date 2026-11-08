use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::backend::SwapBuffersError;
use smithay::backend::drm::{DrmDeviceFd, DrmNode};
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::damage::{Error as DamageError, OutputDamageTracker};
use smithay::backend::renderer::gles::{GlesError, GlesRenderer};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::utils::Buffer;
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::input::pointer::CursorImageStatus;
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
use smithay::utils::{
    DeviceFd, IsAlive, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use smithay::wayland::drm_syncobj::supports_syncobj_eventfd;
use smithay::wayland::presentation::Refresh;
use smithay::wayland::socket::ListeningSocketSource;

use crate::capture::{log_screenshot, save_screenshot};
use crate::cursor::Cursors;
use crate::frame_clock::{
    collect_drawn_buffers, queue_estimated_vblank, refresh_interval, schedule_frame_release,
    send_frame_callbacks_for_state, take_presentation_feedback, update_primary_scanout_outputs,
    watch_overdue_frame_callbacks,
};
use crate::scene_elements::build_output_elements;
use crate::state::{ClientState, State, StateError};
use ctappearance::watch::AppearanceWatch;

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
    pub(crate) loop_handle: LoopHandle<'static, Session>,
    display: Display<State>,
    pub(crate) state: State,
    pub(crate) backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
    cursor: Cursors,
    track_objects: bool,
    last_objects: LiveObjects,
    capture_requested: bool,
    frame_seq: u64,
    pub(crate) last_frame_duration: Option<Duration>,
    pub(crate) estimated_vblank: Option<RegistrationToken>,
    appearance_watch: Option<AppearanceWatch>,
}

pub(crate) struct PendingFrameRelease {
    pub(crate) sync: SyncPoint,
    pub(crate) buffers: Vec<Buffer>,
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

pub(crate) fn physical_point(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
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
                Timer::from_duration(crate::test_input::test_pointer_delay()),
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
                ctdesign::DESKTOP_CLEAR.premultiplied(),
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

pub(crate) fn reset_dead_cursor(state: &mut State) {
    if let CursorImageStatus::Surface(surface) = &state.cursor_status
        && !surface.alive()
    {
        state.cursor_status = CursorImageStatus::default_named();
        state.client_cursor = CursorImageStatus::default_named();
    }
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
mod tests;
