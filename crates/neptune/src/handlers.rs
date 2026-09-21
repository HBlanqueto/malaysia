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
        // Every client is accepted with `ClientState` (see `session.rs`), and a
        // client in this display cannot be dispatched before it is inserted.
        &client
            .get_data::<ClientState>()
            .expect("clients are inserted with ClientState")
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

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {
        // Popups are out of scope for Block 1.
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
