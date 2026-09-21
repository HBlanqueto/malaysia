//! Protocol handler implementations and their delegation glue.
//!
//! Smithay's `delegate_*` macros wire a state type to the protocol objects it
//! serves, but they only work if the matching `*Handler` trait is implemented.
//! The impls live here, next to the macros, so `state.rs` stays a plain data
//! description and every handler is in one place.
//!
//! The globals split across two traits each: the handler trait is the
//! compositor's behaviour, the `delegate_*` macro is the plumbing that turns
//! client requests into calls on that behaviour.

use smithay::delegate_compositor;
use smithay::delegate_output;
use smithay::delegate_seat;
use smithay::delegate_shm;
use smithay::delegate_xdg_shell;
use smithay::input::SeatHandler;
use smithay::input::SeatState;
use smithay::reexports::wayland_server::Client;
use smithay::reexports::wayland_server::protocol::wl_buffer;
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};

use crate::state::{ClientState, State};

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // The invariant: session.rs inserts every accepted client with
        // ClientState, so a client being dispatched cannot lack it. If this
        // fires, look at the socket handler, not at this call site.
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted with ClientState in the socket handler")
            .compositor_state
    }

    fn commit(&mut self, _surface: &WlSurface) {
        // Surface commits are handled when surfaces are rendered (Part 6).
    }
}

delegate_compositor!(State);

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {
        // Buffers are not retained yet; nothing to release.
    }
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_shm!(State);

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }
}

delegate_seat!(State);

impl OutputHandler for State {}

delegate_output!(State);

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, _surface: ToplevelSurface) {
        // The initial configure and mapping arrive in Part 5.
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        // Constraint adjustment and grabs are out of scope for Block 1. Sending
        // a configure with the positioner's raw geometry keeps the client from
        // waiting forever; the popup lands in the wrong place, which is visible
        // and debuggable rather than a silent stall.
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        if let Err(error) = surface.send_configure() {
            tracing::warn!(%error, "failed to configure a popup");
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // Popup grabs are out of scope for Block 1.
    }

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
        // Popups are out of scope for Block 1.
    }
}

delegate_xdg_shell!(State);
