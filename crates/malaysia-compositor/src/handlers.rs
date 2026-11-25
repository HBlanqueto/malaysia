use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::desktop::Window;
use smithay::reexports::wayland_server::Client;
use smithay::reexports::wayland_server::protocol::{
    wl_buffer::WlBuffer, wl_seat::WlSeat, wl_surface::WlSurface,
};
use smithay::utils::Serial;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface,
};
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
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.needs_redraw = true;
        if is_sync_subsurface(surface) {
            return;
        }
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        if let Some(window) = self
            .space
            .elements()
            .find(|window| window.toplevel().unwrap().wl_surface() == &root)
        {
            window.on_commit();
            let toplevel = window.toplevel().unwrap();
            if !toplevel.is_initial_configure_sent() {
                toplevel.send_configure();
            }
        }
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.space
            .map_element(Window::new_wayland_window(surface), (0, 0), false);
    }

    fn toplevel_destroyed(&mut self, _surface: ToplevelSurface) {
        self.needs_redraw = true;
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {}

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
    }
}

impl OutputHandler for State {}

smithay::delegate_dispatch2!(State);
