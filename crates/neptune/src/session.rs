//! The nested compositor session: event loop, winit window and GLES2 output.
//!
//! Block 1 hosts Neptune inside a winit window so a Wayland client (later in
//! the block) has somewhere to be shown. This module owns the calloop event
//! loop, the single winit window and the GLES2 renderer — the backend sits
//! beside the shared [`State`] rather than inside it, so replacing it later
//! touches no protocol handler (ADR 0012). It stops the loop when the window
//! is closed or a termination signal arrives. Protocol handlers and client
//! rendering are added by later parts of the block; for now the loop draws a
//! solid background.

use smithay::backend::SwapBuffersError;
use smithay::backend::renderer::gles::{GlesError, GlesRenderer};
use smithay::backend::renderer::{Frame as _, Renderer as _};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::reexports::calloop::{EventLoop, LoopSignal};
use smithay::utils::{Rectangle, Transform};

use crate::state::State;

/// Failures that can stop the session from starting.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The calloop event loop could not be created or driven.
    #[error("event loop failure")]
    EventLoop(#[from] smithay::reexports::calloop::Error),
    /// The winit backend could not open a window or create its EGL context.
    ///
    /// The message is flattened to a `String` because winit's error type is
    /// not `Sync`, and this error reaches `main` through `anyhow`.
    #[error("failed to initialise the winit backend: {0}")]
    Backend(String),
    /// Signal handling could not be installed.
    #[error("failed to install signal handling")]
    Signals(#[source] std::io::Error),
}

/// Runs the nested session until the window is closed or a signal arrives.
///
/// This blocks the calling thread, driving the calloop event loop. It returns
/// once the loop has stopped and is safe to call after the logger is set up.
pub fn run() -> Result<(), SessionError> {
    let mut event_loop: EventLoop<State> = EventLoop::try_new()?;
    let loop_signal = event_loop.get_signal();
    let mut state = State::new(loop_signal.clone());

    let (mut backend, winit_events) =
        winit::init::<GlesRenderer>().map_err(|error| SessionError::Backend(error.to_string()))?;
    backend.window().request_redraw();
    watch_for_shutdown(loop_signal)?;

    event_loop
        .handle()
        .insert_source(winit_events, move |event, _, state: &mut State| {
            handle_event(event, &mut backend, state);
        })
        .map_err(|error| SessionError::EventLoop(error.into()))?;

    tracing::info!("winit window open; close it or send SIGTERM to exit");
    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
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
fn handle_event(
    event: WinitEvent,
    backend: &mut WinitGraphicsBackend<GlesRenderer>,
    state: &mut State,
) {
    match event {
        WinitEvent::Redraw => redraw(backend),
        WinitEvent::Resized { .. } => backend.window().request_redraw(),
        WinitEvent::CloseRequested => {
            tracing::info!("window close requested");
            state.stop();
        }
        WinitEvent::Input(_) | WinitEvent::Focus(_) => {}
    }
}

/// Draws one frame, logging rather than propagating a failure.
///
/// A lost EGL context cannot be recovered from here; the loop keeps running so
/// a later event can try again. Frames are drawn on demand — once at startup
/// and after each resize — rather than in a continuous loop.
fn redraw(backend: &mut WinitGraphicsBackend<GlesRenderer>) {
    if let Err(error) = draw(backend) {
        tracing::error!(%error, "failed to draw a frame");
    }
}

/// Clears the window to the session background colour.
fn draw(backend: &mut WinitGraphicsBackend<GlesRenderer>) -> Result<(), DrawError> {
    let size = backend.window_size();
    let damage = [Rectangle::from_size(size)];

    {
        let (renderer, mut framebuffer) = backend.bind().map_err(DrawError::Bind)?;
        let mut frame = renderer
            .render(&mut framebuffer, size, Transform::Normal)
            .map_err(DrawError::Render)?;
        frame
            .clear([0.06, 0.06, 0.09, 1.0].into(), &damage)
            .map_err(DrawError::Render)?;
        let _ = frame.finish().map_err(DrawError::Render)?;
    }
    backend.submit(Some(&damage)).map_err(DrawError::Submit)?;
    Ok(())
}

/// Failures while drawing a single frame.
#[derive(Debug, thiserror::Error)]
enum DrawError {
    /// Binding the renderer to the window failed.
    #[error("could not bind the renderer to the window")]
    Bind(#[source] SwapBuffersError),
    /// A GLES2 draw call failed.
    #[error("GLES2 rendering failed")]
    Render(#[source] GlesError),
    /// Presenting the frame to the window failed.
    #[error("could not present the frame")]
    Submit(#[source] SwapBuffersError),
}
