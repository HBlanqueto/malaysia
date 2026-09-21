//! The nested compositor session: event loop, winit window and GLES2 output.
//!
//! The nested session hosts Neptune inside a winit window so a Wayland client
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
use smithay::backend::renderer::damage::{Error as DamageError, OutputDamageTracker};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::{GlesError, GlesRenderer};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::Mode as OutputMode;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_server::BindError;
use smithay::reexports::wayland_server::Display;
use smithay::reexports::wayland_server::backend::InitError;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Logical, Point};
use smithay::wayland::compositor::{
    SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward,
};
use smithay::wayland::socket::ListeningSocketSource;

use crate::cursor::{DefaultCursor, OutputRenderElement};
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
    /// The default arrow, drawn when no client cursor is active.
    cursor: DefaultCursor,
    /// When the session started, used as the frame callback timestamp base.
    start_time: Instant,
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

/// Runs the nested session until the window is closed or a signal arrives.
///
/// This blocks the calling thread, driving the calloop event loop. It returns
/// once the loop has stopped and is safe to call after the logger is set up.
pub fn run() -> Result<(), SessionError> {
    let mut event_loop: EventLoop<Session> = EventLoop::try_new()?;
    let loop_signal = event_loop.get_signal();

    let display = Display::<State>::new()?;

    let (backend, winit_events) = winit::init::<GlesRenderer>()
        .map_err(|error| SessionError::Backend(describe_chain(&error)))?;

    let state = State::new(
        loop_signal.clone(),
        &display.handle(),
        backend.window_size(),
    )?;
    let damage_tracker = OutputDamageTracker::from_output(&state.output);
    let cursor = DefaultCursor::load(State::OUTPUT_SCALE);
    let mut session = Session {
        display,
        state,
        backend,
        damage_tracker,
        cursor,
        start_time: Instant::now(),
    };
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
    })?;
    Ok(())
}

/// Reads and handles whatever the connected clients have sent.
///
/// Both calls are non-blocking, so a client that is slow to read or write
/// cannot hold up the loop. A client error is logged and the loop continues:
/// one misbehaving client should not take the compositor down.
fn dispatch_clients(session: &mut Session) {
    let Session { display, state, .. } = session;

    if let Err(error) = display.dispatch_clients(state) {
        tracing::error!(%error, "failed to dispatch client requests");
        return;
    }
    if let Err(error) = display.flush_clients() {
        tracing::error!(%error, "failed to flush events to clients");
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
    } = session;

    // Everything that borrows the renderer or the framebuffer is scoped so the
    // frame is submitted after the framebuffer is released.
    let damage = {
        let (renderer, mut framebuffer) = backend.bind().map_err(DrawError::Bind)?;

        let mut elements: Vec<OutputRenderElement<GlesRenderer>> = Vec::new();
        for window in state.windows.iter() {
            if !window.is_mapped() {
                continue;
            }
            let location =
                (window.location() + window.surface_location()).to_physical(State::OUTPUT_SCALE);
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
        reset_dead_cursor(state);
        append_cursor(renderer, &mut elements, state, cursor);

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
    // cannot drive the compositor faster than the output refreshes.
    let now = start_time.elapsed();
    for window in state.windows.iter() {
        if window.is_mapped() {
            send_frame_callbacks(window.surface(), now);
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

/// Appends the pointer to the frame: the client's cursor surface, or the
/// default arrow, at the pointer position minus its hotspot.
fn append_cursor(
    renderer: &mut GlesRenderer,
    elements: &mut Vec<OutputRenderElement<GlesRenderer>>,
    state: &State,
    cursor: &DefaultCursor,
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
        CursorImageStatus::Named(_) => {
            let position =
                (location - cursor.hotspot().to_f64()).to_physical(f64::from(State::OUTPUT_SCALE));
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                position.to_f64(),
                cursor.buffer(),
                None,
                None,
                None,
                Kind::Cursor,
            ) {
                Ok(element) => {
                    tracing::debug!("drawing the default pointer");
                    elements.push(OutputRenderElement::from(element));
                }
                Err(error) => tracing::warn!(?error, "could not draw the default cursor"),
            }
        }
        CursorImageStatus::Surface(surface) => {
            let position = (location - cursor_surface_hotspot(surface))
                .to_physical(f64::from(State::OUTPUT_SCALE))
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

    use super::describe_chain;

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
}
