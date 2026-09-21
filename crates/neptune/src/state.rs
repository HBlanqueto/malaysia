//! The compositor's shared state.
//!
//! [`State`] is the calloop loop data and the value every protocol handler
//! receives as `&mut State`. It is deliberately backend-agnostic: the winit
//! window, the GLES renderer and the damage tracker live *beside* it, in
//! [`crate::session`], never inside it. A handler therefore cannot reach the
//! backend, and Block 2 can replace winit with DRM/KMS without editing a
//! handler. The reasoning is recorded in ADR 0012.
//!
//! Block 1 builds the state up one part at a time. Today it carries the loop
//! signal. The protocol state structs — `CompositorState`, `ShmState`,
//! `SeatState`, `XdgShellState` and `OutputManagerState` — join it as the
//! parts that create their globals land, because Smithay refuses to create a
//! global until its `Dispatch`/`GlobalDispatch` traits are implemented.

use smithay::reexports::calloop::LoopSignal;

/// The shared compositor state.
///
/// One instance is created per session and moved into the calloop event loop;
/// every event source and protocol handler borrows it as `&mut State`.
pub struct State {
    /// Wakes and stops the event loop.
    ///
    /// The session's shutdown paths (window close, `SIGTERM`, `SIGINT`) use
    /// this, and later handlers that need a frame drawn will use it to reach
    /// the loop without holding a reference to the backend. See [`State::stop`].
    loop_signal: LoopSignal,
}

impl State {
    /// Creates the state for one session from the loop's shutdown signal.
    pub fn new(loop_signal: LoopSignal) -> Self {
        Self { loop_signal }
    }

    /// Asks the event loop to stop and return from its `run` call.
    ///
    /// Safe to call from any thread, which is why the signal thread can use it
    /// as well; the signal itself is cloned at startup.
    pub fn stop(&self) {
        self.loop_signal.stop();
    }
}
