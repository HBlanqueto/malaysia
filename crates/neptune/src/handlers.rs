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

use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::delegate_compositor;
use smithay::delegate_data_device;
use smithay::delegate_kde_decoration;
use smithay::delegate_output;
use smithay::delegate_seat;
use smithay::delegate_shm;
use smithay::delegate_xdg_decoration;
use smithay::delegate_xdg_shell;
use smithay::input::Seat;
use smithay::input::SeatHandler;
use smithay::input::SeatState;
use smithay::input::pointer::CursorImageStatus;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
    Mode as KdeMode, OrgKdeKwinServerDecoration,
};
use smithay::reexports::wayland_server::Client;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_buffer;
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::WEnum;
use smithay::utils::Serial;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::{
    Configure, PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};

use neptune_scene::scene::Decoration;

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

    fn commit(&mut self, surface: &WlSurface) {
        // Let Smithay move the pending buffer into the surface's current state
        // first; the window tracker then reads that state to decide on mapping.
        on_commit_buffer_handler::<Self>(surface);
        if let Some(mapped) = self.windows.commit(surface) {
            self.focus_window(mapped);
            self.inject_test_press(mapped);
        }
        self.request_redraw();
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

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        // Smithay only reaches here for a `wl_pointer.set_cursor` whose serial
        // matches the pointer's last enter serial, which this compositor
        // generated from its single serial counter (see `cursor.rs`). No second
        // counter is involved. Store the image; the draw path renders it.
        self.cursor_status = image;
        self.request_redraw();
    }
}

delegate_seat!(State);

impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for State {}
impl ServerDndGrabHandler for State {}

delegate_data_device!(State);

impl OutputHandler for State {}

delegate_output!(State);

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // Send the initial configure now, before the client attaches a buffer:
        // a toplevel that never receives one waits forever, with no error.
        let size = self.logical_output_size();
        self.windows.add_toplevel(surface, size);
        self.request_redraw();
    }

    fn ack_configure(&mut self, surface: WlSurface, configure: Configure) {
        self.windows.ack_configure(&surface, configure);
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        // The serial must name the pointer's implicit grab: a button press
        // that is still held, made on this client's surface. A request with
        // any other serial is ignored rather than starting a drag.
        let Some(start) = self.held_grab(serial) else {
            tracing::info!(
                serial = ?serial,
                "rejected move request: no button press with this serial is held"
            );
            return;
        };
        if !start
            .focus
            .as_ref()
            .is_some_and(|(focus, _)| focus.id().same_client_as(&surface.wl_surface().id()))
        {
            tracing::info!(
                serial = ?serial,
                "rejected move request: the press was not on this client's surface"
            );
            return;
        }
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            tracing::info!(
                serial = ?serial,
                "rejected move request: the surface is not a tracked window"
            );
            return;
        };
        if let Some(location) = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
        {
            tracing::info!(serial = ?serial, "accepted move request");
            self.start_move(id, location);
            self.inject_test_drag(id);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let Some(start) = self.held_grab(serial) else {
            tracing::info!(
                serial = ?serial,
                "rejected resize request: no button press with this serial is held"
            );
            return;
        };
        if !start
            .focus
            .as_ref()
            .is_some_and(|(focus, _)| focus.id().same_client_as(&surface.wl_surface().id()))
        {
            tracing::info!(
                serial = ?serial,
                "rejected resize request: the press was not on this client's surface"
            );
            return;
        }
        let Some(edge) = crate::window::from_xdg_resize_edge(edges) else {
            return;
        };
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            tracing::info!(
                serial = ?serial,
                "rejected resize request: the surface is not a tracked window"
            );
            return;
        };
        if let Some(location) = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
        {
            tracing::info!(serial = ?serial, "accepted resize request");
            self.start_resize(id, edge, location);
            // No drag motion for a resize: the probe asserts the request was
            // accepted and the resizing configure, not a new size.
            self.inject_test_release();
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.forget_window(surface.wl_surface());
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        // Constraint adjustment and grabs are out of scope for the nested session. Sending
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
        // Popup grabs are out of scope for the nested session.
    }

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
        // Popups are out of scope for the nested session.
    }
}

delegate_xdg_shell!(State);

/// The decoration a client asks for with an xdg-decoration mode.
///
/// The protocol has no "unset" value here: `request_mode` carries one of the
/// two, and the compositor answers with the one it chose.
fn decoration_for_xdg_mode(mode: Mode) -> Decoration {
    match mode {
        Mode::ClientSide => Decoration::Client,
        // Server-side, and any value this build does not know: the compositor
        // chooses, and it chooses server-side.
        _ => Decoration::Server,
    }
}

/// The decoration a client asks for with a KDE decoration mode.
///
/// `None` means "do not decorate me at all", which for Neptune is the same as
/// self-decorating: no chrome. `Server` and `Client` map directly.
fn decoration_for_kde_mode(mode: KdeMode) -> Decoration {
    match mode {
        KdeMode::Server => Decoration::Server,
        // Client-side, undecorated, and any unknown value: no compositor
        // chrome, so an odd client cannot end up with a second titlebar.
        _ => Decoration::Client,
    }
}

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        // The client created the decoration object but has not chosen a mode;
        // the protocol leaves the choice to the compositor, which chooses
        // server-side. The configure goes out before the client attaches, so it
        // never draws a titlebar of its own.
        self.windows
            .set_decoration(toplevel.wl_surface(), Decoration::Server);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        // An explicit choice is honoured: a client that asks for client-side
        // chrome keeps it, and one that accepts server-side gets Neptune's.
        self.windows
            .set_decoration(toplevel.wl_surface(), decoration_for_xdg_mode(mode));
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        // The client withdrew its choice, so the compositor chooses again:
        // server-side, as for a fresh decoration object.
        self.windows
            .set_decoration(toplevel.wl_surface(), Decoration::Server);
    }
}

delegate_xdg_decoration!(State);

impl KdeDecorationHandler for State {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.kde_decoration_state
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        mode: WEnum<KdeMode>,
    ) {
        let WEnum::Value(mode) = mode else {
            return;
        };
        // GTK3 and GTK4 ask through this protocol; an explicit client-side
        // request means the window has its own headerbar and must keep it. The
        // reply repeats the client's mode, because that is the mode chosen.
        decoration.mode(mode);
        self.windows
            .set_decoration(surface, decoration_for_kde_mode(mode));
    }
}

delegate_kde_decoration!(State);

#[cfg(test)]
mod tests {
    use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
    use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::Mode as KdeMode;

    use super::{decoration_for_kde_mode, decoration_for_xdg_mode};
    use neptune_scene::scene::Decoration;

    #[test]
    fn an_xdg_client_that_accepts_server_side_gets_server_chrome() {
        assert_eq!(
            decoration_for_xdg_mode(Mode::ServerSide),
            Decoration::Server
        );
    }

    #[test]
    fn an_xdg_client_that_requests_client_side_keeps_its_own_chrome() {
        assert_eq!(
            decoration_for_xdg_mode(Mode::ClientSide),
            Decoration::Client
        );
    }

    #[test]
    fn a_kde_client_that_requests_server_side_gets_server_chrome() {
        assert_eq!(decoration_for_kde_mode(KdeMode::Server), Decoration::Server);
    }

    #[test]
    fn a_kde_client_that_requests_client_side_keeps_its_own_chrome() {
        assert_eq!(decoration_for_kde_mode(KdeMode::Client), Decoration::Client);
    }

    #[test]
    fn a_kde_client_that_asks_for_no_decoration_gets_none_from_neptune() {
        assert_eq!(decoration_for_kde_mode(KdeMode::None), Decoration::Client);
    }
}
