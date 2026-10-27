//! The nested compositor session: event loop, winit window and GLES2 output.
//!
//! The nested session hosts MalaysiaDreams inside a winit window so a Wayland client
//! has somewhere to be shown. This module owns the calloop event loop, the single
//! winit window and the GLES2 renderer — the backend sits beside the shared
//! [`State`] rather than inside it, so replacing it later touches no protocol
//! handler (ADR 0012). It also owns the Wayland display, the listening socket
//! clients connect to, and the event source that dispatches their requests
//! (ADR 0013). It stops the loop when the window is closed or a termination
//! signal arrives.
//!
//! Rendering happens in the redraw path: mapped windows' surfaces are turned
//! into render elements, drawn through a damage tracker, and their frame
//! callbacks are sent afterwards. Frame callbacks are only sent here, after a
//! frame has actually been drawn, so a client that commits cannot make the
//! compositor draw faster than the output refreshes.

use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::backend::SwapBuffersError;
use smithay::backend::drm::{DrmDeviceFd, DrmNode};
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::damage::{Error as DamageError, OutputDamageTracker};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::{
    GlesError, GlesPixelProgram, GlesRenderer, GlesTexProgram, UniformName, UniformType,
};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::utils::{Buffer, RendererSurfaceStateUserData};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
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
    SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward,
};
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use smithay::wayland::drm_syncobj::supports_syncobj_eventfd;
use smithay::wayland::image_copy_capture::CaptureFailureReason;
use smithay::wayland::presentation::{PresentationFeedbackCachedState, Refresh};
use smithay::wayland::shell::wlr_layer::Layer;
use smithay::wayland::socket::ListeningSocketSource;

use crate::config::ScreenshotTarget;
use crate::cursor::{Cursors, OutputRenderElement};
use crate::state::{ClientState, State, StateError};
use crate::state::{ScreenshotMode, region_rect};
use vow_appearance::watch::AppearanceWatch;

/// Failures that can stop the session from starting.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The calloop event loop could not be created or driven.
    #[error("event loop failure")]
    EventLoop(#[from] smithay::reexports::calloop::Error),
    /// The winit backend could not open a window or create its EGL context.
    ///
    /// The message carries the whole `source()` chain, flattened into a
    /// `String`: the outer "Failed to initialize an event loop" is useless on
    /// its own, while the winit error underneath names the real reason. It is a
    /// `String` because winit's error type is not `Sync`, and this error reaches
    /// `main` through `anyhow`.
    #[error("failed to initialise the winit backend: {0}")]
    Backend(String),
    /// Signal handling could not be installed.
    #[error("failed to install signal handling")]
    Signals(#[source] std::io::Error),
    /// The Wayland display could not be created.
    #[error("failed to create the Wayland display")]
    Display(#[from] InitError),
    /// The listening socket could not be created.
    #[error("failed to create the Wayland listening socket")]
    Socket(#[from] BindError),
    /// The display's file descriptor could not be duplicated for polling.
    #[error("failed to duplicate the display file descriptor")]
    Fd(#[source] std::io::Error),
    /// The compositor state could not be built: its globals could not be
    /// published, or the seat has no usable keymap.
    #[error("failed to build the compositor state")]
    State(#[from] StateError),
    /// The device behind the winit renderer has no usable render node, or the
    /// dmabuf feedback could not be built from it.
    ///
    /// The linux-dmabuf global needs a main device and formats to start
    /// (Bloque 7a); without a render node there is no v3 fallback, so startup
    /// fails loudly instead of silently degrading.
    #[error("cannot set up the dmabuf feedback: {0}")]
    DmabufFeedback(String),
    /// The EGL display behind the winit renderer lacks
    /// `EGL_ANDROID_native_fence_sync`.
    ///
    /// The deferred buffer release of Bloque 7b waits on the frame's EGL fence
    /// through its native fd (ADR 0026). Without the extension there is no
    /// fd-based wait that keeps the loop unblocked, and there is deliberately
    /// no alternative path, so startup fails loudly.
    #[error("cannot wait on the GPU without EGL_ANDROID_native_fence_sync: {0}")]
    NativeFence(String),
}

/// Everything the event loop's sources share.
///
/// The Wayland display is held here, next to `State`, rather than inside it.
/// Dispatching clients needs a mutable borrow of both, and two disjoint fields
/// of one value can be borrowed at once while a display stored inside `State`
/// could not (ADR 0013). The damage tracker is backend state, so it lives here
/// too rather than in `State` (ADR 0012).
pub(crate) struct Session {
    /// The loop handle, for the deferred frame-fence source.
    loop_handle: LoopHandle<'static, Session>,
    /// The Wayland display: owns the clients and their protocol objects.
    display: Display<State>,
    /// The compositor state handlers mutate.
    ///
    /// `pub(crate)` so the blocker callbacks (handlers.rs) can resume commits
    /// through it; everything else stays private to the session.
    pub(crate) state: State,
    /// The winit window and its GLES2 renderer.
    backend: WinitGraphicsBackend<GlesRenderer>,
    /// Tracks damage between frames for the one output.
    damage_tracker: OutputDamageTracker,
    /// The compositor-drawn cursors, shown when no client cursor is active.
    cursor: Cursors,
    /// When the session started, used as the frame callback timestamp base.
    start_time: Instant,
    /// Whether live-object counts are tracked and logged.
    ///
    /// Off unless `MALAYSIA_TEST_OBJECTS` is set, so a normal run pays nothing;
    /// the Block 4 harness turns it on to assert teardown.
    track_objects: bool,
    /// The last live-object counts logged, so only changes are emitted.
    last_objects: LiveObjects,
    /// Set by `SIGUSR1`; the next frame writes a debug PNG (Bloque 7.1).
    capture_requested: bool,
    /// Sequence number for `wp_presentation` feedback.
    frame_seq: u64,
    /// When the previous presented frame happened, for per-frame timing logs.
    last_frame_duration: Option<Duration>,
    /// One-refresh timer for empty frames: winit only emits `Redraw` events
    /// when the window is explicitly requested, so an idle output that only
    /// needs to answer frame callbacks needs this fallback.
    repaint_timer: Option<RegistrationToken>,
    /// Watches `appearance.domain` and `wallpaper.state` for changes.
    appearance_watch: Option<AppearanceWatch>,
}

/// The buffers a finished frame read, held until their GPU work completes.
///
/// Smithay 0.7.0 replaces the buffer of a surface at the *next* commit, which
/// drops its only reference and signals `wl_buffer.release` (and the
/// explicit-sync release point) *before* the render that read it has finished
/// (`backend/renderer/utils/wayland.rs:145-181`, `on_commit_buffer_handler`
/// called first from `CompositorHandler::commit`). The clone this struct holds
/// keeps each such buffer alive until the frame's own EGL fence signals; only
/// then is it dropped, so a client never reuses a buffer the GPU still reads.
///
/// The fence is waited on through its native fd (exportable thanks to
/// `EGL_ANDROID_native_fence_sync`), registered as a one-shot event source,
/// so the compositor never blocks on the GPU (ADR 0026).
struct PendingFrameRelease {
    /// The EGL fence of the render that sampled the buffers.
    sync: SyncPoint,
    /// Clones of every buffer that frame drew, in no particular order.
    buffers: Vec<Buffer>,
}

/// How many live Wayland objects the compositor is holding.
///
/// Counted from the backend's own client and object tables, not from a
/// hand-rolled counter, so it cannot drift from reality: a client or surface
/// that is still alive is still counted, and one that is gone is not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LiveObjects {
    /// Connected clients.
    clients: usize,
    /// `wl_surface` objects across every client.
    surfaces: usize,
    /// Toplevels tracked by the scene.
    toplevels: usize,
}

impl LiveObjects {
    /// Emits the counts as one log line the harness can parse.
    fn log(self) {
        tracing::info!(
            clients = self.clients,
            surfaces = self.surfaces,
            toplevels = self.toplevels,
            "live objects"
        );
    }
}

/// Counts the live clients, `wl_surface` objects and tracked toplevels.
///
/// The backend holds a lock while enumerating, so the ids are collected first
/// and inspected afterwards; calling back into the handle inside the closures
/// would deadlock.
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

/// Flattens an error and every `source()` below it into one line.
///
/// Startup failures are usually two errors deep — Smithay's wrapper plus the
/// winit error it wraps — and the outer message alone hides the real reason.
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

/// The transform that makes Smithay's render space land upright in the winit
/// framebuffer.
///
/// OpenGL's framebuffer origin is bottom-left, so Smithay's y-down render space
/// arrives inverted unless the render transform flips it. The damage tracker
/// inverts whatever transform it is given before handing it to the renderer
/// (`smithay-0.7.0/src/backend/renderer/damage/mod.rs:333-335`), and the GLES
/// renderer composes that with its own OpenGL correction
/// (`smithay-0.7.0/src/backend/renderer/gles/mod.rs:2087-2089`): with
/// `Transform::Normal` the frame lands upside down, with `Flipped180` render
/// y = 0 is the top of the window.
///
/// It is deliberately **not** set on the `Output`. `Output::change_current_state`
/// sends the output's transform to clients in `wl_output.geometry`
/// (`smithay-0.7.0/src/wayland/output/mod.rs:164-177`), and `flipped_180` there
/// would tell a client to pre-rotate its buffers. The tracker's
/// [`OutputModeSource::Static`] carries the render transform on its own, so the
/// flip stays at render time (`smithay-0.7.0/src/output.rs:496-536`).
pub(crate) const fn winit_render_transform() -> Transform {
    Transform::Flipped180
}

/// Converts a logical point to the physical pixel the renderer draws at.
///
/// The position is floored (ADR 0004).
fn physical_point(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
    point.to_f64().to_physical(scale).to_i32_floor()
}

/// The damage tracker mode source for the winit output.
///
/// A static source, because the render transform is not the output's: it pins
/// the size the session already tracks and the transform above. A resize builds
/// a new tracker with the new size.
pub(crate) fn winit_mode_source(size: Size<i32, Physical>, scale: f64) -> OutputModeSource {
    OutputModeSource::Static {
        size,
        scale: Scale::from(scale),
        transform: winit_render_transform(),
    }
}

/// Opens a render node as a `DrmDeviceFd`, for the explicit-sync protocol.
///
/// The `wp_linux_drm_syncobj_manager_v1` global imports the client's syncobj
/// timelines and waits on them through this device fd (Bloque 7b). The render
/// node is what the EGL device reported; its path is opened read-write, which
/// is all a render node needs.
fn open_render_node(node: &DrmNode) -> std::io::Result<DrmDeviceFd> {
    use std::fs::OpenOptions;

    let path = node
        .dev_path()
        .ok_or_else(|| std::io::Error::other("the render node has no device path"))?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    let fd = OwnedFd::from(file);
    Ok(DrmDeviceFd::new(DeviceFd::from(fd)))
}

/// Runs the nested session until the window is closed or a signal arrives.
///
/// This blocks the calling thread, driving the calloop event loop. It returns
/// once the loop has stopped and is safe to call after the logger is set up.
pub fn run() -> Result<(), SessionError> {
    let mut event_loop: EventLoop<Session> = EventLoop::try_new()?;
    let loop_signal = event_loop.get_signal();

    let display = Display::<State>::new()?;

    let (mut backend, winit_events) = winit::init::<GlesRenderer>()
        .map_err(|error| SessionError::Backend(describe_chain(&error)))?;

    // The deferred buffer release of Bloque 7b waits on the frame's EGL fence
    // through its native fd (ADR 0026). Without `EGL_ANDROID_native_fence_sync`
    // that wait is impossible without blocking the loop, so — like the render
    // node — this fails startup with a clear message. There is no alternative
    // path and none is added.
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

    // The render node of the device the winit renderer runs on, derived from
    // the EGL device behind that renderer's display (anvil's winit backend does
    // exactly this). It becomes the `main_device` of the dmabuf feedback, so a
    // GPU client allocates buffers the compositor can texture from. There is
    // deliberately no fallback: without a render node the global cannot start,
    // so this fails startup with a clear message (see [`SessionError::DmabufFeedback`]).
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

    // Open the render node as a DRM device fd and, if it supports syncobj
    // eventfds, announce the explicit-sync global (Bloque 7b). Otherwise the
    // global is simply not advertised and clients fall back to the implicit
    // sync blocker; the loop's handle rides into `State` for the blockers.
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
        start_time: Instant::now(),
        track_objects,
        last_objects: LiveObjects::default(),
        capture_requested: false,
        frame_seq: 0,
        last_frame_duration: None,
        repaint_timer: None,
        appearance_watch: None,
    };
    if session.track_objects {
        session.last_objects = live_objects(&session.display, &session.state);
        session.last_objects.log();
    }

    // Compile the shared shaders once; failures are non-fatal and leave the
    // optional fields as None.
    let renderer = session.backend.renderer();
    session.state.rounded_clip = compile_rounded_clip(renderer);
    session.state.shadow = compile_shadow(renderer);
    session.state.border = compile_border(renderer);

    session.backend.window().request_redraw();

    watch_for_shutdown(&loop_handle, loop_signal)?;

    // When a client dies, its ClientData pushes the id into this queue and
    // pings the loop; the ping callback drains the queue and removes all of
    // the client's surfaces in one batch.
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

    // A scripted `MALAYSIA_TEST_INPUT` sequence drives the same motion/button path
    // as the host pointer. The timer starts after the compositor is up; the
    // script itself waits for its target surface to map.
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

    // Rebuild the chrome theme when appearance.domain or wallpaper.state
    // changes, without a restart.
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
                    if let Ok((renderer, _)) = session.backend.bind() {
                        let fade = crate::theme_fade::start_if_allowed(
                            &mut session.state,
                            renderer,
                            &session.cursor,
                            output_size,
                            scale,
                            now,
                        );
                        session.state.theme_fade = fade;
                    } else {
                        crate::theme_fade::log_snapshot_failure(
                            "could not bind the renderer for the theme fade snapshot",
                        );
                    }
                    session.state.refresh_theme();
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

    // The display's own file descriptor is moved into the source; poll a
    // duplicate so the display keeps the original (ADR 0013). Client requests
    // only arrive when this is readable, and `dispatch_clients` never blocks,
    // so a slow client cannot stall the loop.
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
        if session.state.take_redraw_request() {
            session.backend.window().request_redraw();
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

/// Reads and handles whatever the connected clients have sent.
///
/// Both calls are non-blocking, so a client that is slow to read or write
/// cannot hold up the loop. A client error is logged and the loop continues:
/// one misbehaving client should not take the compositor down.
///
/// The dmabuf import queue is drained here, in the same call that dispatched
/// the clients. That ordering is the guarantee that no frame ever renders an
/// unimported buffer: a client's `create`/`create_immed` + `attach` + `commit`
/// can arrive in one write, and dispatch processes all three in this one call
/// — the import is queued by the handler during dispatch and drained before
/// this call returns. A frame can only be drawn from a separate winit `Redraw`
/// event that the loop processes after this source's batch, so the drain
/// always precedes the next render.
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
    // The handlers cannot reach the renderer (ADR 0012), so the session — the
    // renderer's owner — performs the import here, right after dispatching. A
    // buffer that cannot be imported is rejected at this point, never at
    // render time (see [`import_pending_dmabufs`]).
    import_pending_dmabufs(state, backend.renderer());
    if let Err(error) = display.flush_clients() {
        tracing::error!(%error, "failed to flush events to clients");
    }
}

/// Imports every dmabuf the handlers queued during dispatch, answering each
/// client's request through its notifier.
///
/// A `Dmabuf` that the renderer cannot import is rejected here, at commit
/// time, never at render time. For a `create` request that is a `failed` event
/// on the params object; for `create_immed` (whose buffer the client already
/// holds) `ImportNotifier::failed` posts the protocol's
/// `InvalidWlBuffer` error, which kills the client — the compositor never
/// silently accepts a buffer it cannot draw.
///
/// The renderer lives in the session (ADR 0012); this function is a free
/// function rather than a `State` method so `State` stays free of renderer
/// types, and so the tests can drive it directly.
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

/// Stops the loop on `SIGTERM`/`SIGINT` and requests a debug capture on
/// `SIGUSR1`, through calloop's signal source (Bloque 7.1).
///
/// The signal source lives in the loop, so the callback runs on the main
/// thread with `&mut Session`: `SIGUSR1` sets `capture_requested` and the next
/// frame writes a PNG; the shutdown signals stop the loop.
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
                    // The compositor draws on demand; ask for a frame so the
                    // capture happens now rather than at the next unrelated
                    // redraw.
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

/// Reacts to one event from the host window.
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
            // The tracker's mode source is static, so the new size is carried
            // by a fresh tracker. Resetting the damage history on a resize is
            // what the whole surface needs anyway.
            session.damage_tracker = OutputDamageTracker::from_mode_source(winit_mode_source(
                size,
                session.state.scale.fractional_scale(),
            ));
            session.backend.window().request_redraw();
        }
        WinitEvent::CloseRequested => {
            tracing::info!("window close requested");
            session.state.stop();
        }
        WinitEvent::Input(event) => {
            // A scripted `MALAYSIA_TEST_INPUT` sequence is the only pointer in the
            // test session. Dropping host events keeps a real mouse or touchpad
            // from interleaving with it; the script still goes through the same
            // motion/button path.
            if session.state.test_pointer.is_some() {
                return;
            }
            session.state.process_input_event(event);
            // Input sends keyboard and pointer events to clients; flush them
            // now instead of waiting for the next client request, which may
            // never come (an idle popup cannot answer a frame callback it has
            // not received yet).
            if let Err(error) = session.display.flush_clients() {
                tracing::warn!(%error, "flushing clients after input");
            }
            // Motion moves the cursor and any dragged window; a button changes
            // focus and stacking. Both are visible, so a frame is needed.
            session.state.request_redraw();
        }
        WinitEvent::Focus(_) => {}
    }
}

/// Draws one frame, logging rather than propagating a failure.
///
/// A lost EGL context cannot be recovered from here; the loop keeps running so
/// a later event can try again. Frames are drawn on demand — after a commit, a
/// resize or input — rather than in a continuous loop, and the frame callbacks
/// are sent only after the frame has been submitted.
fn redraw(session: &mut Session) {
    if let Err(error) = draw(session) {
        tracing::error!(%error, "failed to draw a frame");
    }
}

/// Draws the screenshot selection: a dimmed outside, a rectangle border, or a
/// window highlight while picking one.
fn append_screenshot_overlay(state: &State, elements: &mut Vec<OutputRenderElement>) {
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
            // A light dim says "the next drag selects"; the crosshair points.
            append_solid(
                Point::from((0, 0)),
                output,
                vow_design::SCREENSHOT_IDLE_DIM,
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

/// Dims everything outside `rect`.
fn append_dim_outside(
    rect: Rectangle<i32, Logical>,
    output: Size<i32, Logical>,
    scale: f64,
    elements: &mut Vec<OutputRenderElement>,
) {
    let dim = vow_design::SCREENSHOT_DIM;
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

/// Draws a thin white border around `rect`.
fn append_border(
    rect: Rectangle<i32, Logical>,
    scale: f64,
    elements: &mut Vec<OutputRenderElement>,
) {
    const BORDER: i32 = vow_design::SCREENSHOT_BORDER_WIDTH;
    let color = vow_design::SCREENSHOT_BORDER;
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

/// Pushes the theme-fade overlay between the cursor and everything else.
fn append_theme_fade(state: &State, elements: &mut Vec<OutputRenderElement>) {
    let Some(fade) = state.theme_fade.as_ref() else {
        return;
    };
    let alpha = fade.alpha(Instant::now());
    elements.push(fade.element(alpha));
}

/// Pushes one solid-colour rectangle as a render element.
fn append_solid(
    location: Point<i32, Logical>,
    size: Size<i32, Logical>,
    color: [f32; 4],
    scale: f64,
    elements: &mut Vec<OutputRenderElement>,
) {
    if size.w <= 0 || size.h <= 0 {
        return;
    }
    let buffer = SolidColorBuffer::new(size, color);
    elements.push(
        SolidColorRenderElement::from_buffer(
            &buffer,
            physical_point(location, scale),
            scale,
            1.0,
            Kind::Unspecified,
        )
        .into(),
    );
}

/// Renders every mapped window over the session background.
/// Builds the frame's elements in Smithay's front-to-back order.
///
/// Shared by the winit and DRM backends: the scene, the lock rule and the
/// cursor are the same, only presentation differs.
pub(crate) fn build_output_elements(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    include_cursor: bool,
) -> Vec<OutputRenderElement> {
    let mut elements: Vec<OutputRenderElement> = Vec::new();
    let scale = state.scale.fractional_scale();
    if state.locked {
        // A locked session shows only the lock surfaces (plus the cursor):
        // no windows, no shell panels, no popups. If the lock client died,
        // its surfaces are gone and the clear color behind them stands in
        // — the session stays locked (lock.rs).
        reset_dead_cursor(state);
        // Front-to-back (smithay's `render_output` order): the cursor is on
        // top, the lock surfaces behind it.
        if include_cursor {
            append_cursor(renderer, &mut elements, state, cursor);
        }
        for lock_surface in &state.lock_surfaces {
            if !lock_surface.alive() {
                continue;
            }
            elements.extend(
                render_elements_from_surface_tree::<
                    GlesRenderer,
                    WaylandSurfaceRenderElement<GlesRenderer>,
                >(
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
        // `render_output` takes elements in front-to-back order (the first
        // is drawn on top). So: cursor, then the Top/Overlay panels (above
        // every window), then popups, then windows, then the Background /
        // Bottom layers (the wallpaper). See `layers.rs` and ADR 0023.
        reset_dead_cursor(state);
        if include_cursor {
            append_cursor(renderer, &mut elements, state, cursor);
        }
        append_theme_fade(state, &mut elements);
        append_screenshot_overlay(state, &mut elements);
        append_layers(state, renderer, &mut elements, LayerSide::Front);
        for (id, _parent, rect) in state.windows.iter_popups().rev() {
            let Some(surface) = state.popups.surface_of(id) else {
                continue;
            };
            let location = physical_point(
                Point::from((rect.x().round() as i32, rect.y().round() as i32)),
                scale,
            );
            elements.extend(
                render_elements_from_surface_tree::<
                    GlesRenderer,
                    WaylandSurfaceRenderElement<GlesRenderer>,
                >(renderer, surface, location, scale, 1.0, Kind::Unspecified)
                .into_iter()
                .map(|e| e.into()),
            );
        }
        let focused = state.focused_window();
        let window_ids: Vec<_> = state
            .windows
            .iter_visible(&state.minimized)
            .rev()
            .map(|window| window.id())
            .collect();
        for id in window_ids {
            append_window_frame(state, renderer, id, focused, scale, &mut elements);
        }
        append_layers(state, renderer, &mut elements, LayerSide::Back);
    }
    elements
}

/// Window data needed to build its frame; copied out so the mutable borrow of
/// `State` for `chrome.element` does not conflict with the window iterator.
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

/// Computes the logical rectangle for the SSD frame.
///
/// For server-decorated windows the frame hugs the committed client geometry
/// (window geometry or buffer) expanded by the insets. During an interactive
/// resize the scene outer already follows the pointer and anchors the opposite
/// edge, so the configured outer size is used until the client commits.
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

/// The corner radius of a window frame. Only fullscreen drops the frame;
/// a maximized window keeps its rounded corners, border and shadow.
fn frame_radius(state: crate::decoration::WindowState, role: crate::decoration::Role) -> u32 {
    if state == crate::decoration::WindowState::Fullscreen {
        return 0;
    }
    match role {
        crate::decoration::Role::Dialog => vow_design::DIALOG.corner_radius,
        _ => vow_design::WINDOW.corner_radius,
    }
}

/// Whether the frame draws its outer border, inner line and clipped content.
fn frame_has_border(state: crate::decoration::WindowState) -> bool {
    state != crate::decoration::WindowState::Fullscreen
}

/// Appends one window's frame elements in front-to-back order: outer border,
/// inner line, clipped content, then shadow.
fn append_window_frame(
    state: &mut State,
    renderer: &mut GlesRenderer,
    id: malaysia_scene::scene::WindowId,
    focused: Option<malaysia_scene::scene::WindowId>,
    scale: f64,
    elements: &mut Vec<OutputRenderElement>,
) {
    let data = {
        let Some(window) = state
            .windows
            .iter_visible(&state.minimized)
            .find(|w| w.id() == id)
        else {
            return;
        };
        if !window.is_mapped() {
            return;
        }
        WindowFrameData {
            title: window.title(),
            location: window.location(),
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
    let radius_logical = frame_radius(data.state, data.role);
    let radius_physical = (f64::from(radius_logical) * scale).round() as i32;
    let radius = radius_logical;

    // Content, clipped to the rounded frame.
    let rounded_program = state.rounded_clip.clone();
    let mut content: Vec<OutputRenderElement> = Vec::new();
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
        if let Some(program) = rounded_program.clone()
            && let Some(element) = state.chrome.element(
                renderer,
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
            )
        {
            content.push(element.into());
        }
    }
    let surface_location = physical_point(data.location + data.surface_location, scale);
    let surface_elements = render_elements_from_surface_tree::<
        GlesRenderer,
        WaylandSurfaceRenderElement<GlesRenderer>,
    >(
        renderer,
        &data.surface,
        surface_location,
        scale,
        1.0,
        Kind::Unspecified,
    );

    if let Some(program) = rounded_program.filter(|_| frame_has_border(data.state)) {
        for element in surface_elements {
            if let Some(rounded) = crate::effects::RoundedTextureElement::from_surface_element(
                &element,
                frame_rect_physical,
                radius_physical,
                program.clone(),
            ) {
                content.push(rounded.into());
            } else {
                // Solid-colour surfaces cannot be clipped with the texture
                // shader; fall back to the normal surface draw.
                content.push(element.into());
            }
        }
    } else {
        content.extend(surface_elements.into_iter().map(|e| e.into()));
    }

    let mut outer = None;
    let mut inner = None;
    let mut shadow = Vec::new();
    if frame_has_border(data.state) {
        // The compositor draws the outer border over the inner line, both over
        // the content, and the shadow falls behind everything.
        let palette = &state.chrome_theme.palette;
        if let Some(program) = state.border.as_ref() {
            if let Some(color) = palette.inner_line
                && let Some(element) =
                    border_element(program, frame_rect_logical, radius, scale, false, color)
            {
                inner = Some(element.into());
            }
            if let Some(element) = border_element(
                program,
                frame_rect_logical,
                radius,
                scale,
                true,
                palette.outer_border,
            ) {
                outer = Some(element.into());
            }
        }
        if let Some(program) = state.shadow.as_ref() {
            shadow = state.chrome.shadow_element(
                program,
                frame_rect_logical,
                &state.chrome_theme,
                is_focused,
            );
        }
    }

    elements.extend(frame_order(outer, inner, content, shadow));
}

/// Assembles a window frame in Smithay's front-to-back order: outer border,
/// inner line, content (SSD bar + surface), then the shadow behind it all.
fn frame_order<T>(outer: Option<T>, inner: Option<T>, content: Vec<T>, shadow: Vec<T>) -> Vec<T> {
    let mut frame = Vec::with_capacity(content.len() + shadow.len() + 2);
    frame.extend(outer);
    frame.extend(inner);
    frame.extend(content);
    frame.extend(shadow);
    frame
}

/// Converts a logical rectangle to a physical one at `scale`.
///
/// Uses Smithay's `Rectangle::to_physical_precise_up`, which floors the
/// location and ceils the size (ADR 0004).
fn physical_rect(rect: Rectangle<i32, Logical>, scale: f64) -> Rectangle<i32, Physical> {
    rect.to_f64().to_physical_precise_up(scale)
}

/// Builds one [`PixelShaderElement`] for an inner line or outer border ring.
fn border_element(
    program: &smithay::backend::renderer::gles::GlesPixelProgram,
    frame_rect: Rectangle<i32, Logical>,
    radius: u32,
    scale: f64,
    outer: bool,
    color: vow_design::Rgba,
) -> Option<smithay::backend::renderer::gles::element::PixelShaderElement> {
    use smithay::backend::renderer::gles::Uniform;
    use smithay::backend::renderer::gles::element::PixelShaderElement;
    // One physical pixel expressed in logical coordinates.
    let width_logical = (1.0 / scale) as f32;
    let margin = width_logical.ceil().max(1.0) as i32;
    let area = Rectangle::new(
        Point::from((frame_rect.loc.x - margin, frame_rect.loc.y - margin)),
        Size::from((
            frame_rect.size.w + margin * 2,
            frame_rect.size.h + margin * 2,
        )),
    );
    let rect_local = [
        (frame_rect.loc.x - area.loc.x) as f32,
        (frame_rect.loc.y - area.loc.y) as f32,
        frame_rect.size.w as f32,
        frame_rect.size.h as f32,
    ];
    let uniforms = vec![
        Uniform::new("size", [area.size.w as f32, area.size.h as f32]),
        Uniform::new("rect", rect_local),
        Uniform::new("radius", radius as f32),
        Uniform::new("width", width_logical),
        Uniform::new("side", if outer { 1.0f32 } else { -1.0f32 }),
        Uniform::new("color", color.premultiplied()),
    ];
    Some(PixelShaderElement::new(
        program.clone(),
        area,
        None,
        1.0,
        uniforms,
        Kind::Unspecified,
    ))
}

/// Renders a single window's surface tree at the origin, for window capture.
///
/// The window's client surface only; the compositor's chrome and effects are
/// not drawn yet, so a captured window is its client content.
pub(crate) fn build_window_elements(
    renderer: &mut GlesRenderer,
    surface: &WlSurface,
    scale: f64,
) -> Vec<OutputRenderElement> {
    render_elements_from_surface_tree::<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>(
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

/// Saves a PrintScreen screenshot to the Desktop.
///
/// The focused window when there is one (its client content), otherwise the
/// whole output. Shared by the winit and DRM backends; `transform` is the
/// backend's render transform.
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

/// Where a screenshot went.
pub(crate) enum ScreenshotOutcome {
    /// Written to the screenshot path (already logged).
    Saved,
    /// Offered as `image/png` on the clipboard.
    Copied,
}

/// The output elements a screenshot should draw: the same scene as the frame,
/// with the pointer following `screenshot.show_pointer` (macOS hides it).
fn screenshot_elements(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
) -> Vec<OutputRenderElement> {
    let include_cursor = state.config.screenshot.show_pointer;
    build_output_elements(state, renderer, cursor, include_cursor)
}

/// Renders one window (its client content) as PNG bytes.
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

/// Renders the whole output as PNG bytes.
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
        [0.06, 0.06, 0.09, 1.0],
    )
}

/// Logs a screenshot result. The save path already logs itself.
pub(crate) fn log_screenshot(result: std::io::Result<ScreenshotOutcome>) {
    match result {
        Ok(ScreenshotOutcome::Copied) => tracing::info!("screenshot copied to the clipboard"),
        Ok(ScreenshotOutcome::Saved) => {}
        Err(error) => tracing::warn!(%error, "screenshot failed"),
    }
}

fn draw(session: &mut Session) -> Result<(), DrawError> {
    // Everything that borrows the renderer or the framebuffer is scoped so the
    // frame is submitted after the framebuffer is released.
    let capture_size = session.backend.window_size();
    let (damage, sync, request_redraw) = {
        let (renderer, mut framebuffer) = session.backend.bind().map_err(DrawError::Bind)?;

        let elements = build_output_elements(&mut session.state, renderer, &session.cursor, true);

        session.state.init_capture_dma(renderer);

        // Debug capture (Bloque 7.1): `SIGUSR1` asked for it. The offscreen readback
        // makes the GL context surfaceless (`eglMakeCurrent(EGL_NO_SURFACE, ...)`
        // in smithay's `bind_texture`), so it runs *before* the window pass: the
        // `render_output` that follows re-binds the window's EGL surface and the
        // swap in `submit` still targets a current surface (Bloque 7.3).
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

        // PrintScreen: save the focused window (or the output when none) to the
        // Desktop. Runs before the window pass like the other offscreen reads.
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

        // `ext-image-copy-capture-v1` frames queued by the handler. Like the
        // debug capture they run before the window pass, so the offscreen
        // render does not leave the GL context without the window surface.
        let capture_scale = session.state.scale.fractional_scale();
        for pending in std::mem::take(&mut session.state.pending_captures) {
            let (size, window_elements) = match &pending.target {
                crate::capture::CaptureTarget::Output { size } => (*size, None),
                crate::capture::CaptureTarget::Window { id, size } => {
                    let Some(surface) = session.state.windows.surface_of(*id).cloned() else {
                        pending.frame.fail(CaptureFailureReason::Unknown);
                        continue;
                    };
                    (
                        *size,
                        Some(build_window_elements(renderer, &surface, capture_scale)),
                    )
                }
            };
            let without_cursor: Vec<OutputRenderElement>;
            let drawn: &[OutputRenderElement] = match &pending.target {
                crate::capture::CaptureTarget::Output { .. } if !pending.draw_cursor => {
                    without_cursor =
                        build_output_elements(&mut session.state, renderer, &session.cursor, false);
                    &without_cursor
                }
                _ => window_elements.as_deref().unwrap_or(&elements),
            };
            let damage = Rectangle::from_size(Size::from((size.w, size.h)));
            match crate::capture::capture_into_buffer(
                renderer,
                drawn,
                size,
                capture_scale,
                [0.06, 0.06, 0.09, 1.0],
                &pending.frame.buffer(),
            ) {
                Ok(()) => {
                    pending
                        .frame
                        .success(Transform::Normal, Some(vec![damage]), Duration::ZERO)
                }
                Err(error) => {
                    tracing::warn!(%error, "screencopy frame failed");
                    pending.frame.fail(CaptureFailureReason::Unknown);
                }
            }
        }

        let result = session
            .damage_tracker
            .render_output(
                renderer,
                &mut framebuffer,
                0,
                &elements,
                [0.06, 0.06, 0.09, 1.0],
            )
            .map_err(DrawError::Output)?;

        let mut damage = result.damage.cloned();
        let sync = result.sync.clone();
        drop(result);
        let mut request_redraw = false;
        let now = Instant::now();
        if let Some(fade) = session.state.theme_fade.as_mut() {
            match fade.tick(now) {
                crate::theme_fade::FadeTick::Done => session.state.theme_fade = None,
                _ => {
                    // Force a full redraw on every frame of the fade and ask
                    // for the next redraw event.
                    damage = Some(vec![Rectangle::from_size(capture_size)]);
                    session.damage_tracker = OutputDamageTracker::from_mode_source(
                        winit_mode_source(capture_size, session.state.scale.fractional_scale()),
                    );
                    request_redraw = true;
                }
            }
        }

        (damage, sync, request_redraw)
    };

    if request_redraw {
        session.backend.window().request_redraw();
    }

    let now = session.start_time.elapsed();
    let presented = damage.is_some();
    if !presented {
        // No damage: there is nothing to submit, but surfaces that asked for a
        // frame callback still need an answer. Schedule a repaint so the next
        // batch of callbacks does not stall (same behaviour as DRM).
        let refresh = session
            .state
            .output
            .current_mode()
            .map(|mode| mode.refresh)
            .unwrap_or(60_000)
            .max(1) as u64;
        let delay = Duration::from_micros(1_000_000 / refresh);
        let loop_handle = &session.loop_handle;
        let repaint_timer = &mut session.repaint_timer;
        let mut schedule_repaint = move || {
            if repaint_timer.is_some() {
                return;
            }
            match loop_handle.insert_source(
                Timer::from_duration(delay),
                |_, _, session: &mut Session| {
                    session.repaint_timer = None;
                    session.backend.window().request_redraw();
                    TimeoutAction::Drop
                },
            ) {
                Ok(token) => *repaint_timer = Some(token),
                Err(error) => tracing::warn!(%error, "scheduling an empty-frame repaint"),
            }
        };
        finish_empty_frame(
            &mut session.state,
            now,
            session.frame_seq,
            &mut schedule_repaint,
        );
        return Ok(());
    }

    // The frame just submitted read these buffers; hold them so their release
    // waits for its fence (ADR 0026, see `PendingFrameRelease`).
    let buffers = collect_drawn_buffers(&session.state);
    if !buffers.is_empty() {
        schedule_frame_release(&session.loop_handle, PendingFrameRelease { sync, buffers });
    }

    // Send frame callbacks only once the frame has been drawn, so a client
    // cannot drive the compositor faster than the output refreshes. While the
    // session is locked, only the lock surfaces receive them; no normal client
    // may render anything under the lock.
    if session.state.locked {
        for surface in &session.state.lock_surfaces {
            if surface.alive() {
                send_frame_callbacks(surface.wl_surface(), now);
            }
        }
    } else {
        for surface in session.state.windows.surfaces() {
            // Unplaced toplevels have not committed a buffer yet; they still need
            // frame callbacks so the client can render its first frame and map.
            if session.state.windows.is_mapped(surface) || !session.state.windows.is_placed(surface)
            {
                send_frame_callbacks(surface, now);
            }
        }
        for entry in session.state.layers.all() {
            send_frame_callbacks(entry.surface.wl_surface(), now);
        }
        for (id, _parent, _rect) in session.state.windows.iter_popups() {
            if let Some(surface) = session.state.popups.surface_of(id) {
                send_frame_callbacks(surface, now);
            }
        }
    }
    if let Err(error) = session.display.flush_clients() {
        tracing::warn!(%error, "failed to flush clients after a frame");
    }

    session
        .backend
        .submit(damage.as_deref())
        .map_err(DrawError::Submit)?;
    let now = session.start_time.elapsed();
    if let Some(previous) = session.last_frame_duration {
        let delta = now - previous;
        tracing::debug!(frame = session.frame_seq, ?delta, "frame presented");
    }
    session.last_frame_duration = Some(now);
    session.frame_seq = session.frame_seq.wrapping_add(1);
    send_presentation_feedbacks_for_state(
        &session.state,
        session.start_time.elapsed(),
        session.frame_seq,
        true,
    );
    if let Err(error) = session.display.flush_clients() {
        tracing::warn!(%error, "failed to flush presentation feedback");
    }
    Ok(())
}

/// Returns the pointer to the default arrow once a client cursor surface dies.
///
/// A `wl_pointer.set_cursor` surface can be destroyed while it is still the
/// pointer's image; the protocol then says the cursor is gone, so the compositor
/// falls back to its own arrow rather than drawing a dead surface.
fn reset_dead_cursor(state: &mut State) {
    if let CursorImageStatus::Surface(surface) = &state.cursor_status
        && !surface.alive()
    {
        state.cursor_status = CursorImageStatus::default_named();
        state.client_cursor = CursorImageStatus::default_named();
    }
}

/// Clones the buffer of every surface a frame will draw, so the release of
/// each waits for the frame's fence (ADR 0026).
///
/// Mirrors the surface enumeration of [`draw`]: the lock surfaces when locked,
/// otherwise the windows, popups and layer-shell surfaces. Subsurfaces are
/// walked too, since they carry buffers of their own.
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

/// Clones the current buffer of `surface` and every subsurface below it.
///
/// The renderer's surface state is read through the `states` argument of the
/// traversal, exactly as `import_surface` does: re-locking it via
/// `with_renderer_surface_state` inside the walk would deadlock on the same
/// `SurfaceData` mutex (`backend/renderer/utils/wayland.rs:474` re-locks what
/// `with_surface_tree_downward` already holds).
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

/// Holds the buffers a finished frame read until its fence signals (ADR 0026).
///
/// The frame's EGL fence is exported as a native fence fd (an eventfd that
/// becomes readable when the GPU completes the render) and registered as a
/// one-shot event source: when it fires the release is dropped — signalling
/// `wl_buffer.release` / the explicit-sync release point — and the source
/// removes itself. The loop keeps running, so the compositor never blocks on
/// the GPU and a client is never signalled to reuse a buffer still being read.
fn schedule_frame_release(
    loop_handle: &LoopHandle<'static, Session>,
    release: PendingFrameRelease,
) {
    // `EGL_ANDROID_native_fence_sync` is guaranteed at startup, so export only
    // returns `None` when the fence is already signalled — the GPU finished
    // this frame before it was exported — and releasing right away is exactly
    // right. There is no alternative path (ADR 0026).
    let Some(fd) = release.sync.export() else {
        if !release.sync.is_reached() {
            tracing::error!("frame fence is neither exportable nor signalled");
        }
        return;
    };
    // The source takes a duplicate; the release keeps the original until the
    // GPU signals.
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

/// Which side of the window stack a set of layer-shell layers renders on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LayerSide {
    /// Behind the windows: `Background` (wallpaper) and `Bottom`.
    Back,
    /// In front of the windows: `Top` and `Overlay`.
    Front,
}

/// Appends the layer-shell surfaces on `side` to the frame, placed at their
/// computed geometry. The wallpaper (Background) is behind every window, the
/// panels (Top/Overlay) in front, as on macOS.
fn append_layers(
    state: &State,
    renderer: &mut GlesRenderer,
    elements: &mut Vec<OutputRenderElement>,
    side: LayerSide,
) {
    let layers = match side {
        LayerSide::Back => [Layer::Background, Layer::Bottom],
        LayerSide::Front => [Layer::Top, Layer::Overlay],
    };
    let scale = state.scale.fractional_scale();
    for layer in layers {
        for entry in state.layers.on_layer(layer) {
            let location = physical_point(entry.geometry.loc, scale);
            elements.extend(
                render_elements_from_surface_tree::<
                    GlesRenderer,
                    WaylandSurfaceRenderElement<GlesRenderer>,
                >(
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

/// Appends the pointer to the frame: the client's cursor surface, or the
/// default arrow, at the pointer position minus its hotspot.
fn append_cursor(
    renderer: &mut GlesRenderer,
    elements: &mut Vec<OutputRenderElement>,
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
            // While the shake enlarges the pointer, sample the set loaded at
            // the enlarged size; magnifying the normal image looks pixelated.
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
                    tracing::debug!(boost, "drawing a pointer cursor");
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
                render_elements_from_surface_tree::<
                    GlesRenderer,
                    WaylandSurfaceRenderElement<GlesRenderer>,
                >(
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

/// The physical position of a cursor image's top-left.
///
/// The pointer sits on the image's hotspot, so the image's top-left is the
/// pointer position minus the hotspot. One helper for both the client's cursor
/// surface and the compositor's own shapes keeps the two in step.
fn cursor_origin(
    location: Point<f64, Logical>,
    hotspot: Point<f64, Logical>,
    scale: f64,
) -> Point<f64, Physical> {
    (location - hotspot).to_physical(scale)
}

/// The hotspot of a client cursor surface, in logical coordinates.
///
/// Smithay stores it on the surface when it accepts `set_cursor`, so it is
/// present whenever the status is `Surface`; a missing value is treated as the
/// top-left rather than panicking.
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

/// Completes every pending frame callback in a surface tree.
///
/// There is one output, so every mapped surface is on its scan-out path and no
/// throttling is needed; with more outputs this is where visibility would be
/// decided.
pub(crate) fn send_frame_callbacks(surface: &WlSurface, time: Duration) {
    let time_ms = time.as_millis() as u32;
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_, states, &()| {
            let mut cached = states.cached_state.get::<SurfaceAttributes>();
            for callback in cached.current().frame_callbacks.drain(..) {
                callback.done(time_ms);
            }
        },
        |_, _, &()| true,
    );
}

/// Completes the frame callbacks of everything a frame drew.
pub(crate) fn send_frame_callbacks_for_state(state: &State, time: Duration) {
    if state.locked {
        for surface in &state.lock_surfaces {
            if surface.alive() {
                send_frame_callbacks(surface.wl_surface(), time);
            }
        }
        return;
    }
    for surface in state.windows.surfaces() {
        // Unplaced toplevels have not committed a buffer yet; they still need
        // frame callbacks so the client can render its first frame and map.
        if state.windows.is_mapped(surface) || !state.windows.is_placed(surface) {
            send_frame_callbacks(surface, time);
        }
    }
    for entry in state.layers.all() {
        send_frame_callbacks(entry.surface.wl_surface(), time);
    }
    for (id, _parent, _rect) in state.windows.iter_popups() {
        if let Some(surface) = state.popups.surface_of(id) {
            send_frame_callbacks(surface, time);
        }
    }
}

/// Sends `wp_presentation` feedback for the surfaces a frame drew.
///
/// `presented` is false when the frame was discarded (no damage); the protocol
/// requires `discarded` for a frame that never reached the output.
pub(crate) fn send_presentation_feedbacks_for_state(
    state: &State,
    time: Duration,
    seq: u64,
    presented: bool,
) {
    let refresh = Refresh::fixed(Duration::from_micros(16_666));
    let send = |surface: &WlSurface| {
        with_surface_tree_downward(
            surface,
            (),
            |_, _, &()| TraversalAction::DoChildren(()),
            |_, states, &()| {
                let callbacks = std::mem::take(
                    &mut states
                        .cached_state
                        .get::<PresentationFeedbackCachedState>()
                        .current()
                        .callbacks,
                );
                for callback in callbacks {
                    if presented {
                        callback.presented(
                            &state.output,
                            time,
                            refresh,
                            seq,
                            wp_presentation_feedback::Kind::Vsync,
                        );
                    } else {
                        callback.discarded();
                    }
                }
            },
            |_, _, &()| true,
        );
    };
    if state.locked {
        for surface in &state.lock_surfaces {
            if surface.alive() {
                send(surface.wl_surface());
            }
        }
        return;
    }
    for window in state.windows.iter() {
        if window.is_mapped() {
            send(window.surface());
        }
    }
    for entry in state.layers.all() {
        send(entry.surface.wl_surface());
    }
    for (id, _parent, _rect) in state.windows.iter_popups() {
        if let Some(surface) = state.popups.surface_of(id) {
            send(surface);
        }
    }
}

/// Sends frame callbacks and discarded presentation feedback for a frame that
/// had no damage to present, then asks the backend to schedule another output
/// cycle so the next pending callbacks are not left waiting.
///
/// Shared by the winit and DRM backends so neither has its own copy of the
/// "answer callbacks even when there is nothing to draw" logic.
pub(crate) fn finish_empty_frame(
    state: &mut State,
    time: Duration,
    seq: u64,
    schedule_repaint: &mut dyn FnMut(),
) {
    send_frame_callbacks_for_state(state, time);
    send_presentation_feedbacks_for_state(state, time, seq, false);
    schedule_repaint();
}

/// Failures while drawing a single frame.
#[derive(Debug, thiserror::Error)]
enum DrawError {
    /// Binding the renderer to the window failed.
    #[error("could not bind the renderer to the window")]
    Bind(#[source] SwapBuffersError),
    /// Rendering the output failed.
    #[error("failed to render the output")]
    Output(#[source] DamageError<GlesError>),
    /// Presenting the frame to the window failed.
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
            // The client may have died while the commit was blocked.
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

/// Compiles the rounded-clip texture shader. Returns `None` if the renderer
/// fails to compile it; the caller can then fall back to `ClippedSurface`.
fn compile_rounded_clip(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    let program = renderer.compile_custom_texture_shader(
        grade_graphics::shaders::ROUNDED_CLIP,
        &[
            UniformName::new("texture_size", UniformType::_2f),
            UniformName::new("geometry", UniformType::_4f),
            UniformName::new("radius", UniformType::_1f),
        ],
    );
    match program {
        Ok(program) => Some(program),
        Err(error) => {
            tracing::warn!(?error, "failed to compile rounded-clip shader");
            None
        }
    }
}

/// Compiles the shadow pixel shader. Returns `None` on failure.
fn compile_shadow(renderer: &mut GlesRenderer) -> Option<GlesPixelProgram> {
    let program = renderer.compile_custom_pixel_shader(
        grade_graphics::shaders::SHADOW,
        &[
            UniformName::new("size", UniformType::_2f),
            UniformName::new("rect", UniformType::_4f),
            UniformName::new("window_rect", UniformType::_4f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("blur", UniformType::_1f),
            UniformName::new("offset", UniformType::_2f),
            UniformName::new("color", UniformType::_4f),
        ],
    );
    match program {
        Ok(program) => Some(program),
        Err(error) => {
            tracing::warn!(?error, "failed to compile shadow shader");
            None
        }
    }
}

/// Compiles the border pixel shader. Returns `None` on failure.
fn compile_border(renderer: &mut GlesRenderer) -> Option<GlesPixelProgram> {
    let program = renderer.compile_custom_pixel_shader(
        grade_graphics::shaders::BORDER,
        &[
            UniformName::new("size", UniformType::_2f),
            UniformName::new("rect", UniformType::_4f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("width", UniformType::_1f),
            UniformName::new("side", UniformType::_1f),
            UniformName::new("color", UniformType::_4f),
        ],
    );
    match program {
        Ok(program) => Some(program),
        Err(error) => {
            tracing::warn!(?error, "failed to compile border shader");
            None
        }
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

    // Client side for the dmabuf import-queue test: sctk, like the shell.
    use sctk::output::{OutputHandler, OutputState};
    use sctk::reexports::client::protocol::{
        wl_buffer, wl_buffer::WlBuffer, wl_output, wl_shm, wl_shm_pool, wl_surface,
    };
    use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
    use sctk::reexports::protocols::wp::linux_dmabuf::zv1::client::{
        zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1,
    };

    /// The innermost error, with no cause of its own.
    #[derive(Debug)]
    struct Leaf;

    impl fmt::Display for Leaf {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("the inner cause")
        }
    }

    impl Error for Leaf {}

    /// An error that wraps [`Leaf`], like Smithay wraps winit.
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
                frame_radius(state, Role::Toplevel),
                vow_design::WINDOW.corner_radius
            );
            assert!(frame_has_border(state));
        }
        assert_eq!(frame_radius(WindowState::Fullscreen, Role::Toplevel), 0);
        assert!(!frame_has_border(WindowState::Fullscreen));
        assert_eq!(
            frame_radius(WindowState::Maximized, Role::Dialog),
            vow_design::DIALOG.corner_radius
        );
    }

    /// A logical point is floored when entering physical space.
    ///
    /// With `round()` a logical point at x=2 under scale 1.25 would become
    /// physical 3 instead of 2, and a negative point at -2 would become -2
    /// instead of -3, shifting drawn surfaces by one pixel.
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

    /// Two adjacent logical rectangles must share the physical boundary pixel.
    ///
    /// Rounding the position and size independently with `round()` can leave a
    /// one-pixel gap (or a one-pixel overlap) under fractional scales; ADR 0004
    /// requires flooring the position and ceiling the size so the edge is always
    /// drawn by both rectangles.
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

    /// Red, in `Argb8888`'s in-memory byte order (B, G, R, A).
    const RED: [u8; 4] = [0, 0, 255, 255];
    /// Green, in `Argb8888`'s in-memory byte order.
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    /// The frame's clear color, in `Argb8888`'s in-memory byte order.
    const CLEAR: [u8; 4] = [0, 0, 0, 255];

    /// A GLES renderer on the first EGL device, with no window.
    ///
    /// EGL context creation is unsafe by API: `EGLDisplay::new` and
    /// `GlesRenderer::new` both require the caller to uphold that the display
    /// and context are used on one thread and not shared behind its back. This
    /// is the only place the crate allows it, it is compiled only for tests,
    /// and it uses a render node rather than the host session, so the pixel
    /// test needs no display.
    #[allow(unsafe_code)]
    fn offscreen_renderer() -> GlesRenderer {
        let device = EGLDevice::enumerate()
            .expect("EGL device enumeration should work")
            .next()
            .expect("the pixel test needs an EGL device");
        let display = unsafe { EGLDisplay::new(device) }.expect("the test needs an EGL display");
        let context = EGLContext::new(&display).expect("the test needs an EGL context");
        unsafe { GlesRenderer::new(context) }.expect("the test needs a GLES renderer")
    }

    /// Renders `elements` through the session's own damage tracker setup and
    /// returns the pixels in window order: row 0 is the top.
    ///
    /// The tracker is built from [`winit_mode_source`], the same call the
    /// session makes, so a wrong render transform fails here exactly as it
    /// would on screen. `copy_framebuffer` returns OpenGL order (row 0 at the
    /// bottom), so the rows are flipped back before asserting.
    fn render_and_read(
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        elements: &[OutputRenderElement],
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

    /// The pixel at window coordinates, top-left origin.
    fn pixel(pixels: &[u8], size: Size<i32, Physical>, x: i32, y: i32) -> [u8; 4] {
        let stride = (size.w * 4) as usize;
        let offset = (y as usize) * stride + (x as usize) * 4;
        pixels[offset..offset + 4]
            .try_into()
            .expect("the readback should cover the whole frame")
    }

    /// A solid-color element of `side` by `side` pixels at the origin.
    fn solid(renderer: &mut GlesRenderer, color: [u8; 4], side: i32) -> OutputRenderElement {
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
        // A 4x4 cursor scaled to 12x12 must cover the whole destination: the
        // source rectangle stays 4x4, so the sample cannot run past it.
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

        // A 4x4 cursor whose hotspot is (3, 4), pointed at logical (10, 10):
        // its top-left belongs at physical (7, 6).
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

    // -----------------------------------------------------------------------
    // The dmabuf import queue: create_immed + attach + commit in one batch.
    //
    // `dispatch_clients` must queue the import while processing the batch and
    // `import_pending_dmabufs` must drain it before any render, and a buffer
    // the renderer cannot import must end in the protocol's error for
    // create_immed — a dead client, never a silently accepted black window.
    // -----------------------------------------------------------------------

    /// A minimal sctk client that binds `wl_compositor` and
    /// `zwp_linux_dmabuf_v1`, commits one dmabuf, and reports what happened.
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

    /// The global visibility filter must not stall dispatching: it used to
    /// re-lock the backend state to read the peer credentials. A second client
    /// that has to receive its globals turns a stall into a missed deadline.
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

    /// A real client commits `create_immed` + `attach` + `commit` in one batch
    /// with an fd that is not a dma-buf, while the compositor imports and
    /// renders nothing. The import must be queued during the same dispatch and
    /// drained before a frame, and the failed import must kill the client.
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

        // A v5 global advertising exactly one format, XRGB8888 linear. The
        // client uses it, so smithay's parameter validation accepts the params
        // and the failure happens at the renderer import, which is the path the
        // compositor owns.
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

            // A socket is not seekable, so smithay's plane-size checks are
            // skipped, but it is not a dma-buf either: the renderer's import
            // fails. This exercises the compositor's import path, not the
            // client's parameter validation.
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

            // create_immed + attach + commit in the same roundtrip: one write
            // to the compositor.
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

            // The compositor rejects the import, which for create_immed is a
            // protocol error: the connection dies on the next roundtrip. The
            // client reports whether it observed the death.
            let dead = event_queue.roundtrip(&mut client).is_err();
            sent_tx
                .send(dead)
                .expect("the main thread should read the verdict");
        });

        // Accept the client into the display.
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

        // The client's roundtrips (registry, bind, then the batch) need the
        // compositor to dispatch, so service the connection while waiting for
        // the signal that the batch reached the compositor.
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

        // The client's create_immed must have queued exactly one import, in
        // the same dispatch that also handled the attach and the commit.
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

        // Drain before any frame is drawn: the queue empties, the import
        // fails, and for create_immed that failure is a protocol error.
        import_pending_dmabufs(&mut state, &mut renderer);
        assert!(
            state.pending_dmabuf_imports.is_empty(),
            "the queue must be drained before a frame can be drawn"
        );
        display
            .flush_clients()
            .expect("the error must reach the client");

        // The client's next roundtrip observes the error (the connection is
        // dead); it reports the verdict through the channel.
        let dead = sent_rx
            .recv()
            .expect("the client should report its verdict");
        assert!(
            dead,
            "the failed create_immed must kill the client, never leave a black window"
        );
    }

    /// Without a syncobj-capable device, `State` must not announce the
    /// explicit-sync global (Bloque 7b): the session tests
    /// `supports_syncobj_eventfd` and only then passes the device in.
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

    /// A surface that has never attached a buffer has no current buffer; after
    /// attaching one, the compositor sees it. This is the building block of the
    /// visibility rule that keeps uncommitted windows out of render, hit-test
    /// and focus.
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
        // Configured 1280x708, but the client only committed 640x420.
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

    /// An xdg_toplevel that has not committed its first buffer is still tracked
    /// by the compositor but is not placed yet. It must receive frame callbacks
    /// so the client can render and map.
    #[test]
    fn unplaced_toplevel_gets_frame_callbacks() {
        use std::sync::mpsc;
        use std::time::Duration;

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

        std::thread::spawn(move || {
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

        let mut spins = 0;
        loop {
            display.dispatch_clients(&mut state).expect("dispatch");
            display.flush_clients().expect("flush");
            if state.windows.len() == 1 {
                break;
            }
            spins += 1;
            assert!(spins < 1000, "the toplevel was never tracked");
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
    }
}
