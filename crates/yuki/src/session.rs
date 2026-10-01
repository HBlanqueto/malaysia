//! The nested compositor session: event loop, winit window and GLES2 output.
//!
//! The nested session hosts Yuki inside a winit window so a Wayland client
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

use std::os::fd::AsFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::backend::SwapBuffersError;
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::damage::{Error as DamageError, OutputDamageTracker};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::{GlesError, GlesRenderer};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::{Mode as OutputMode, OutputModeSource};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_server::BindError;
use smithay::reexports::wayland_server::Display;
use smithay::reexports::wayland_server::backend::InitError;
use smithay::reexports::wayland_server::backend::{ClientId, ObjectId};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Logical, Physical, Point, Scale, Size, Transform};
use smithay::wayland::compositor::{
    SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward,
};
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use smithay::wayland::shell::wlr_layer::Layer;
use smithay::wayland::socket::ListeningSocketSource;

use crate::cursor::{Cursors, OutputRenderElement};
use crate::state::{ClientState, State, StateError};

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
}

/// Everything the event loop's sources share.
///
/// The Wayland display is held here, next to `State`, rather than inside it.
/// Dispatching clients needs a mutable borrow of both, and two disjoint fields
/// of one value can be borrowed at once while a display stored inside `State`
/// could not (ADR 0013). The damage tracker is backend state, so it lives here
/// too rather than in `State` (ADR 0012).
struct Session {
    /// The Wayland display: owns the clients and their protocol objects.
    display: Display<State>,
    /// The compositor state handlers mutate.
    state: State,
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
    /// Off unless `YUKI_TEST_OBJECTS` is set, so a normal run pays nothing;
    /// the Block 4 harness turns it on to assert teardown.
    track_objects: bool,
    /// The last live-object counts logged, so only changes are emitted.
    last_objects: LiveObjects,
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

/// The damage tracker mode source for the winit output.
///
/// A static source, because the render transform is not the output's: it pins
/// the size the session already tracks and the transform above. A resize builds
/// a new tracker with the new size.
pub(crate) fn winit_mode_source(size: Size<i32, Physical>) -> OutputModeSource {
    OutputModeSource::Static {
        size,
        scale: Scale::from(f64::from(State::OUTPUT_SCALE)),
        transform: winit_render_transform(),
    }
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
    let dmabuf_feedback = DmabufFeedbackBuilder::new(render_node.dev_id(), backend.renderer().dmabuf_formats())
        .build()
        .map_err(|error| {
            SessionError::DmabufFeedback(format!("the feedback could not be built: {error}"))
        })?;

    let state = State::new(
        loop_signal.clone(),
        &display.handle(),
        backend.window_size(),
        dmabuf_feedback,
    )?;
    let damage_tracker =
        OutputDamageTracker::from_mode_source(winit_mode_source(backend.window_size()));
    let cursor = Cursors::load(State::OUTPUT_SCALE);
    let track_objects = std::env::var_os("YUKI_TEST_OBJECTS").is_some();
    let mut session = Session {
        display,
        state,
        backend,
        damage_tracker,
        cursor,
        start_time: Instant::now(),
        track_objects,
        last_objects: LiveObjects::default(),
    };
    if session.track_objects {
        session.last_objects = live_objects(&session.display, &session.state);
        session.last_objects.log();
    }
    session.backend.window().request_redraw();

    watch_for_shutdown(loop_signal)?;

    let loop_handle = event_loop.handle();

    loop_handle
        .insert_source(winit_events, move |event, _, session: &mut Session| {
            handle_event(event, session);
        })
        .map_err(|error| SessionError::EventLoop(error.into()))?;

    let listening_socket = ListeningSocketSource::new_auto()?;
    tracing::info!(
        socket = %listening_socket.socket_name().to_string_lossy(),
        "listening on the Wayland socket"
    );
    loop_handle
        .insert_source(listening_socket, |stream, _, session: &mut Session| {
            let data = Arc::new(ClientState::default());
            let mut handle = session.display.handle();
            match handle.insert_client(stream, data) {
                Ok(_) => tracing::info!("client connected"),
                Err(error) => tracing::warn!(%error, "rejected a client"),
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
fn import_pending_dmabufs(state: &mut State, renderer: &mut GlesRenderer) {
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

/// Stops the loop when `SIGTERM` or `SIGINT` is delivered.
///
/// The signal iterator blocks, so it lives on its own thread; `LoopSignal`
/// is safe to use from another thread and wakes the loop.
fn watch_for_shutdown(loop_signal: LoopSignal) -> Result<(), SessionError> {
    use signal_hook::consts::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    let mut signals = Signals::new([SIGTERM, SIGINT]).map_err(SessionError::Signals)?;
    std::thread::spawn(move || {
        if signals.forever().next().is_some() {
            tracing::info!("shutdown signal received");
            loop_signal.stop();
        }
    });
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
            session.damage_tracker = OutputDamageTracker::from_mode_source(winit_mode_source(size));
            session.backend.window().request_redraw();
        }
        WinitEvent::CloseRequested => {
            tracing::info!("window close requested");
            session.state.stop();
        }
        WinitEvent::Input(event) => {
            session.state.process_input_event(event);
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

/// Renders every mapped window over the session background.
fn draw(session: &mut Session) -> Result<(), DrawError> {
    let Session {
        backend,
        state,
        damage_tracker,
        display,
        cursor,
        start_time,
        ..
    } = session;

    // Everything that borrows the renderer or the framebuffer is scoped so the
    // frame is submitted after the framebuffer is released.
    let damage = {
        let (renderer, mut framebuffer) = backend.bind().map_err(DrawError::Bind)?;

        let mut elements: Vec<OutputRenderElement<GlesRenderer>> = Vec::new();
        if state.locked {
            // A locked session shows only the lock surfaces (plus the cursor):
            // no windows, no shell panels, no popups. If the lock client died,
            // its surfaces are gone and the clear color behind them stands in
            // — the session stays locked (lock.rs).
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
                        Point::from((0, 0)).to_physical(State::OUTPUT_SCALE),
                        f64::from(State::OUTPUT_SCALE),
                        1.0,
                        Kind::Unspecified,
                    )
                    .into_iter()
                    .map(OutputRenderElement::from),
                );
            }
            reset_dead_cursor(state);
            append_cursor(renderer, &mut elements, state, cursor);
        } else {
            // The layer shell's Background and Bottom layers sit behind every
            // window (the wallpaper is on Background, as on macOS); Top and
            // Overlay are appended after the windows. See `layers.rs` and ADR 0023.
            append_layers(state, renderer, &mut elements, LayerSide::Back);
            for window in state.windows.iter() {
                if !window.is_mapped() {
                    continue;
                }
                let location = (window.location() + window.surface_location())
                    .to_physical(State::OUTPUT_SCALE);
                elements.extend(
                    render_elements_from_surface_tree::<
                        GlesRenderer,
                        WaylandSurfaceRenderElement<GlesRenderer>,
                    >(
                        renderer,
                        window.surface(),
                        location,
                        f64::from(State::OUTPUT_SCALE),
                        1.0,
                        Kind::Unspecified,
                    )
                    .into_iter()
                    .map(OutputRenderElement::from),
                );
            }
            // Popups (Bloque 5) sit above their parent window and above every
            // window in the stack; the Top/Overlay layers come after them.
            for (id, _parent, rect) in state.windows.iter_popups() {
                let Some(surface) = state.popups.surface_of(id) else {
                    continue;
                };
                let location = Point::from((rect.x().round() as i32, rect.y().round() as i32))
                    .to_physical(State::OUTPUT_SCALE);
                elements.extend(
                    render_elements_from_surface_tree::<
                        GlesRenderer,
                        WaylandSurfaceRenderElement<GlesRenderer>,
                    >(
                        renderer,
                        surface,
                        location,
                        f64::from(State::OUTPUT_SCALE),
                        1.0,
                        Kind::Unspecified,
                    )
                    .into_iter()
                    .map(OutputRenderElement::from),
                );
            }
            append_layers(state, renderer, &mut elements, LayerSide::Front);
            reset_dead_cursor(state);
            append_cursor(renderer, &mut elements, state, cursor);
        }

        let result = damage_tracker
            .render_output(
                renderer,
                &mut framebuffer,
                0,
                &elements,
                [0.06, 0.06, 0.09, 1.0],
            )
            .map_err(DrawError::Output)?;
        result.damage.cloned()
    };

    // Send frame callbacks only once the frame has been drawn, so a client
    // cannot drive the compositor faster than the output refreshes. While the
    // session is locked, only the lock surfaces receive them; no normal client
    // may render anything under the lock.
    let now = start_time.elapsed();
    if state.locked {
        for surface in &state.lock_surfaces {
            if surface.alive() {
                send_frame_callbacks(surface.wl_surface(), now);
            }
        }
    } else {
        for window in state.windows.iter() {
            if window.is_mapped() {
                send_frame_callbacks(window.surface(), now);
            }
        }
        for entry in state.layers.all() {
            send_frame_callbacks(entry.surface.wl_surface(), now);
        }
        for (id, _parent, _rect) in state.windows.iter_popups() {
            if let Some(surface) = state.popups.surface_of(id) {
                send_frame_callbacks(surface, now);
            }
        }
    }
    if let Err(error) = display.flush_clients() {
        tracing::warn!(%error, "failed to flush clients after a frame");
    }

    backend
        .submit(damage.as_deref())
        .map_err(DrawError::Submit)?;
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
    elements: &mut Vec<OutputRenderElement<GlesRenderer>>,
    side: LayerSide,
) {
    let layers = match side {
        LayerSide::Back => [Layer::Background, Layer::Bottom],
        LayerSide::Front => [Layer::Top, Layer::Overlay],
    };
    for layer in layers {
        for entry in state.layers.on_layer(layer) {
            let location = entry.geometry.loc.to_physical(State::OUTPUT_SCALE);
            elements.extend(
                render_elements_from_surface_tree::<
                    GlesRenderer,
                    WaylandSurfaceRenderElement<GlesRenderer>,
                >(
                    renderer,
                    entry.surface.wl_surface(),
                    location,
                    f64::from(State::OUTPUT_SCALE),
                    1.0,
                    Kind::Unspecified,
                )
                .into_iter()
                .map(OutputRenderElement::from),
            );
        }
    }
}

/// Appends the pointer to the frame: the client's cursor surface, or the
/// default arrow, at the pointer position minus its hotspot.
fn append_cursor(
    renderer: &mut GlesRenderer,
    elements: &mut Vec<OutputRenderElement<GlesRenderer>>,
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

    match &state.cursor_status {
        CursorImageStatus::Hidden => {}
        CursorImageStatus::Named(icon) => {
            let shape = cursor.get(*icon);
            let position = cursor_origin(
                location,
                shape.hotspot().to_f64(),
                f64::from(State::OUTPUT_SCALE),
            );
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                position,
                shape.buffer(),
                None,
                None,
                None,
                Kind::Cursor,
            ) {
                Ok(element) => {
                    tracing::debug!("drawing a pointer cursor");
                    elements.push(OutputRenderElement::from(element));
                }
                Err(error) => tracing::warn!(?error, "could not draw a pointer cursor"),
            }
        }
        CursorImageStatus::Surface(surface) => {
            let position = cursor_origin(
                location,
                cursor_surface_hotspot(surface),
                f64::from(State::OUTPUT_SCALE),
            )
            .to_i32_round();
            elements.extend(
                render_elements_from_surface_tree::<
                    GlesRenderer,
                    WaylandSurfaceRenderElement<GlesRenderer>,
                >(
                    renderer,
                    surface,
                    position,
                    f64::from(State::OUTPUT_SCALE),
                    1.0,
                    Kind::Cursor,
                )
                .into_iter()
                .map(OutputRenderElement::from),
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
fn send_frame_callbacks(surface: &WlSurface, time: Duration) {
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
    use smithay::utils::{Buffer as BufferCoord, Physical, Point, Rectangle, Size, Transform};

    use super::{cursor_origin, describe_chain, import_pending_dmabufs, winit_mode_source};
    use crate::cursor::OutputRenderElement;
    use crate::state::State;

    // Client side for the dmabuf import-queue test: sctk, like the shell.
    use sctk::output::{OutputHandler, OutputState};
    use sctk::reexports::client::protocol::{wl_buffer::WlBuffer, wl_output, wl_surface};
    use sctk::reexports::client::{Connection, QueueHandle};
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
        elements: &[OutputRenderElement<GlesRenderer>],
    ) -> Vec<u8> {
        let buffer_size: Size<i32, BufferCoord> = Size::from((size.w, size.h));
        let mut target =
            Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Argb8888, buffer_size)
                .expect("the test needs an offscreen buffer");
        let mut framebuffer = renderer.bind(&mut target).expect("the buffer should bind");

        let mut tracker = OutputDamageTracker::from_mode_source(winit_mode_source(size));
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
        OutputRenderElement::from(element)
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
        let pixels = render_and_read(&mut renderer, size, &[OutputRenderElement::from(element)]);

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

        let event_loop = smithay::reexports::calloop::EventLoop::<()>::try_new()
            .expect("the test needs an event loop");
        let mut display = Display::<State>::new().expect("the test needs a display");
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
                std::sync::Arc::new(crate::state::ClientState::default()),
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
}
