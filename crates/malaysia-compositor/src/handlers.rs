use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::desktop::{
    PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy, Space, Window,
    find_popup_root_surface, get_popup_toplevel_coords,
};
use smithay::input::pointer::{ClickGrab, CursorImageStatus, Focus, GrabStartData};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;
use smithay::reexports::wayland_server::protocol::{
    wl_buffer::WlBuffer, wl_seat::WlSeat, wl_surface::WlSurface,
};
use smithay::reexports::wayland_server::{Client, Resource};
use smithay::utils::{IsAlive, Logical, Point, SERIAL_COUNTER, Serial};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface,
    with_states,
};
use smithay::wayland::fractional_scale::{FractionalScaleHandler, with_fractional_scale};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::pointer_constraints::PointerConstraintsHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};

use crate::state::{ClientState, State};
use crate::window_grab::{Drag, WindowGrab};

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
        self.popups.commit(surface);
        if is_sync_subsurface(surface) {
            return;
        }
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        if let Some(window) = self.window_for_surface(&root).cloned() {
            window.on_commit();
            self.place_new_window(&window);
            let toplevel = window.toplevel().unwrap();
            if !toplevel.is_initial_configure_sent() {
                toplevel.send_configure();
            }
            self.anchor_resized(&window);
            self.finish_move_resize(&window);
        } else if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface)
            && !popup.is_initial_configure_sent()
            && let Err(error) = popup.send_configure()
        {
            tracing::warn!(%error, "popup configure failed");
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
        let window = Window::new_wayland_window(surface);
        self.desktops[self.current_desktop].map_element(window.clone(), (0, 0), false);
        self.focus_window(&window, SERIAL_COUNTER.next_serial());
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.needs_redraw = true;
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard
            .current_focus()
            .is_none_or(|focus| focus.alive() && self.root_surface(&focus) != *surface.wl_surface())
        {
            return;
        }
        keyboard.unset_grab(self);
        let serial = SERIAL_COUNTER.next_serial();
        let next = self.desktops[self.current_desktop]
            .elements()
            .rev()
            .find(|window| window.alive() && window.toplevel().unwrap() != &surface)
            .cloned();
        match next {
            Some(window) => self.focus_window(&window, serial),
            None => keyboard.set_focus(self, None, serial),
        }
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: WlSeat, serial: Serial) {
        if let Some((start_data, window, initial_location)) = self.grab_start(&surface, serial) {
            let grab = WindowGrab {
                start_data,
                window,
                drag: Drag::Move { initial_location },
            };
            let pointer = self.seat.get_pointer().unwrap();
            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: WlSeat,
        serial: Serial,
        edges: ResizeEdge,
    ) {
        if let Some((start_data, window, _)) = self.grab_start(&surface, serial)
            && let Some(drag) = Drag::resize(&window, edges)
        {
            let grab = WindowGrab {
                start_data,
                window,
                drag,
            };
            let pointer = self.seat.get_pointer().unwrap();
            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        if let Err(error) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            tracing::warn!(%error, "popup tracking failed");
        }
    }

    fn popup_destroyed(&mut self, _surface: PopupSurface) {
        self.needs_redraw = true;
    }

    fn grab(&mut self, surface: PopupSurface, _seat: WlSeat, serial: Serial) {
        let keyboard = self.seat.get_keyboard().unwrap();
        let pointer = self.seat.get_pointer().unwrap();
        let focused_root = keyboard
            .current_focus()
            .map(|focus| self.root_surface(&focus));
        let kind = PopupKind::Xdg(surface.clone());
        let Some(root) = find_popup_root_surface(&kind).ok().filter(|root| {
            self.window_for_surface(root).is_some() && focused_root.as_ref() == Some(root)
        }) else {
            surface.send_popup_done();
            return;
        };
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &self.seat, serial) else {
            return;
        };
        let previous_serial = grab.previous_serial().unwrap_or(serial);
        if keyboard.is_grabbed()
            && !(keyboard.has_grab(serial) || keyboard.has_grab(previous_serial))
            || pointer.is_grabbed()
                && !(pointer.has_grab(serial) || pointer.has_grab(previous_serial))
        {
            grab.ungrab(PopupUngrabStrategy::All);
            return;
        }
        pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        keyboard.set_focus(self, grab.current_grab(), serial);
        keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }
}

impl State {
    fn root_surface(&self, surface: &WlSurface) -> WlSurface {
        self.popups
            .find_popup(surface)
            .and_then(|popup| find_popup_root_surface(&popup).ok())
            .unwrap_or_else(|| surface.clone())
    }

    fn grab_start(
        &self,
        surface: &ToplevelSurface,
        serial: Serial,
    ) -> Option<(GrabStartData<State>, Window, Point<i32, Logical>)> {
        let start_data =
            self.seat
                .get_pointer()
                .unwrap()
                .with_grab(|grab_serial, grab| {
                    (grab_serial == serial && grab.is::<ClickGrab<State>>())
                        .then(|| grab.start_data().clone())
                })
                .flatten()
                .filter(|start_data| {
                    start_data.focus.as_ref().is_some_and(|(focus, _)| {
                        focus.id().same_client_as(&surface.wl_surface().id())
                    })
                })?;
        let window = self.window_for_surface(surface.wl_surface())?.clone();
        let location = self.desktops[self.current_desktop].element_location(&window)?;
        Some((start_data, window, location))
    }

    fn window_for_surface(&self, surface: &WlSurface) -> Option<&Window> {
        self.desktops
            .iter()
            .flat_map(Space::elements)
            .find(|window| window.toplevel().unwrap().wl_surface() == surface)
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        let Some(window) = find_popup_root_surface(&kind)
            .ok()
            .and_then(|root| self.window_for_surface(&root))
        else {
            return;
        };
        let (Some(mut target), Some(window_geometry)) = (
            self.desktops[self.current_desktop].output_geometry(&self.output),
            self.desktops
                .iter()
                .find_map(|desktop| desktop.element_geometry(window)),
        ) else {
            return;
        };
        target.loc -= get_popup_toplevel_coords(&kind) + window_geometry.loc;
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<State> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_data_device_focus(&self.display_handle, seat, client);
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.cursor_status = image;
        self.needs_redraw = true;
    }
}

impl PointerConstraintsHandler for State {}

impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl WaylandDndGrabHandler for State {}

impl OutputHandler for State {}

impl FractionalScaleHandler for State {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.output.current_scale().fractional_scale();
        with_states(&surface, |states| {
            with_fractional_scale(states, |fractional| fractional.set_preferred_scale(scale))
        });
    }
}

smithay::delegate_dispatch2!(State);
