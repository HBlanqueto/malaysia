use malaysia_scene::scene::WindowId;
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::{
    zwlr_foreign_toplevel_handle_v1::{self, State as ToplevelState, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource};
use smithay::wayland::foreign_toplevel_list::{ForeignToplevelHandle, ForeignToplevelListState};

use crate::state::State;

pub struct ForeignState {
    pub list: ForeignToplevelListState,
    pub management: ForeignManagement,
}

impl ForeignState {
    pub fn new(display: &DisplayHandle) -> Self {
        Self {
            list: ForeignToplevelListState::new::<State>(display),
            management: ForeignManagement::new(display),
        }
    }
}

pub struct ForeignManagement {
    _global: GlobalId,
    instances: Vec<ZwlrForeignToplevelManagerV1>,
    handles: Vec<(WindowId, ZwlrForeignToplevelHandleV1)>,
}

impl ForeignManagement {
    fn new(display: &DisplayHandle) -> Self {
        let global = display.create_global::<State, ZwlrForeignToplevelManagerV1, ()>(3, ());
        Self {
            _global: global,
            instances: Vec::new(),
            handles: Vec::new(),
        }
    }

    fn prune(&mut self) {
        self.instances.retain(|instance| instance.is_alive());
        self.handles.retain(|(_, handle)| handle.is_alive());
    }

    pub(crate) fn handle_for_client(
        &self,
        id: WindowId,
        client: ClientId,
    ) -> Option<&ZwlrForeignToplevelHandleV1> {
        self.handles
            .iter()
            .find(|(wid, handle)| {
                *wid == id && handle.client().is_some_and(|owner| owner.id() == client)
            })
            .map(|(_, handle)| handle)
            .filter(|handle| handle.is_alive())
    }
}

pub(crate) fn toplevel_states(
    activated: bool,
    minimized: bool,
    maximized: bool,
    fullscreen: bool,
) -> Vec<u8> {
    let mut states = Vec::with_capacity(4);
    if maximized {
        states.push(ToplevelState::Maximized as u8);
    }
    if minimized {
        states.push(ToplevelState::Minimized as u8);
    }
    if activated {
        states.push(ToplevelState::Activated as u8);
    }
    if fullscreen {
        states.push(ToplevelState::Fullscreen as u8);
    }
    states
}

fn advertise_to_manager(state: &mut State, id: WindowId, manager: &ZwlrForeignToplevelManagerV1) {
    let Some((title, app_id)) = state.windows.active_info(id) else {
        return;
    };
    let Some(client) = manager.client() else {
        return;
    };
    let Ok(handle) = client.create_resource::<ZwlrForeignToplevelHandleV1, WindowId, State>(
        &state.display_handle,
        manager.version(),
        id,
    ) else {
        return;
    };
    manager.toplevel(&handle);
    handle.title(title);
    handle.app_id(app_id);
    handle.state(state.foreign_states_for(id));
    handle.done();
    state.foreign.management.handles.push((id, handle));
    state.send_menus_to_client(id, client.id());
}

impl smithay::reexports::wayland_server::GlobalDispatch<ZwlrForeignToplevelManagerV1, (), State>
    for ForeignManagement
{
    fn bind(
        state: &mut State,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        let manager = data_init.init(resource, ());
        state.foreign.management.prune();
        let ids: Vec<WindowId> = state.windows.iter().map(|window| window.id()).collect();
        for id in ids {
            advertise_to_manager(state, id, &manager);
        }
        state.foreign.management.instances.push(manager);
    }

    fn can_view(client: Client, _data: &()) -> bool {
        crate::security::unrestricted(&client)
    }
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrForeignToplevelManagerV1, (), State>
    for ForeignManagement
{
    fn request(
        _state: &mut State,
        _client: &Client,
        resource: &ZwlrForeignToplevelManagerV1,
        request: <ZwlrForeignToplevelManagerV1 as Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Request::Stop = request {
            resource.finished();
        }
    }
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrForeignToplevelHandleV1, WindowId, State>
    for ForeignManagement
{
    fn request(
        state: &mut State,
        _client: &Client,
        _resource: &ZwlrForeignToplevelHandleV1,
        request: <ZwlrForeignToplevelHandleV1 as Resource>::Request,
        data: &WindowId,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        if state.locked {
            return;
        }
        let id = *data;
        match request {
            zwlr_foreign_toplevel_handle_v1::Request::Activate { seat: _ } => {
                state.activate_window(id);
            }
            zwlr_foreign_toplevel_handle_v1::Request::Close => {
                state.windows.send_close(id);
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetMinimized => {
                state.set_minimized(id, true);
            }
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMinimized => {
                state.set_minimized(id, false);
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetMaximized => {
                state.set_maximized(id, true);
            }
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMaximized => {
                state.set_maximized(id, false);
            }
            _ => {}
        }
    }
}

impl State {
    pub(crate) fn foreign_window_mapped(&mut self, id: WindowId) {
        let Some((title, app_id)) = self.windows.active_info(id) else {
            return;
        };
        let handle = self.foreign.list.new_toplevel::<State>("", "");
        handle.user_data().insert_if_missing(|| id);
        handle.send_title(&title);
        handle.send_app_id(&app_id);
        handle.send_done();
        self.list_handles.push((id, handle));
        self.advertise_foreign(id);
    }

    fn advertise_foreign(&mut self, id: WindowId) {
        self.foreign.management.prune();
        let managers = self.foreign.management.instances.clone();
        for manager in &managers {
            advertise_to_manager(self, id, manager);
        }
    }

    pub(crate) fn foreign_window_destroyed(&mut self, id: WindowId) {
        self.list_handles.retain(|(wid, _)| *wid != id);
        self.foreign.management.handles.retain(|(wid, handle)| {
            if *wid == id {
                if handle.is_alive() {
                    handle.closed();
                }
                false
            } else {
                handle.is_alive()
            }
        });
        self.foreign
            .management
            .instances
            .retain(|instance| instance.is_alive());
    }

    pub(crate) fn foreign_state_changed(&mut self, id: WindowId) {
        let states = self.foreign_states_for(id);
        self.foreign.management.prune();
        for (wid, handle) in &self.foreign.management.handles {
            if *wid == id {
                handle.state(states.clone());
                handle.done();
            }
        }
    }

    pub(crate) fn foreign_identity_changed(&mut self, id: WindowId) {
        let Some((title, app_id)) = self.windows.active_info(id) else {
            return;
        };
        if let Some((_, handle)) = self.list_handles.iter().find(|(wid, _)| *wid == id) {
            handle.send_title(&title);
            handle.send_app_id(&app_id);
            handle.send_done();
        }
        self.foreign.management.prune();
        for (wid, handle) in &self.foreign.management.handles {
            if *wid == id {
                handle.title(title.clone());
                handle.app_id(app_id.clone());
                handle.done();
            }
        }
        self.publish_active_if_focused(id);
    }

    fn foreign_states_for(&self, id: WindowId) -> Vec<u8> {
        toplevel_states(
            self.shown_active() == Some(id),
            self.minimized.contains(&id),
            self.maximized.contains_key(&id),
            self.fullscreen.contains_key(&id),
        )
    }

    pub(crate) fn activate_window(&mut self, id: WindowId) {
        if self.locked {
            return;
        }
        self.set_minimized(id, false);
        self.windows.set_deactivated(id, false);
        self.raise_and_focus(id);
    }

    pub(crate) fn set_minimized(&mut self, id: WindowId, minimized: bool) {
        let was = self.minimized.contains(&id);
        if was == minimized {
            return;
        }
        if minimized {
            self.minimized.insert(id);
            self.windows.set_hidden(id, true);
            tracing::info!(window = id.get(), "window minimized");
            if self.focused_window() == Some(id) {
                match self.focus_fallback(Some(id)) {
                    Some(next) => self.focus_window(next),
                    None => self.clear_keyboard_focus(),
                }
            }
        } else {
            self.minimized.remove(&id);
            self.windows.set_hidden(id, false);
            tracing::info!(window = id.get(), "window restored");
        }
        self.foreign_state_changed(id);
        self.request_redraw();
    }
}

pub type ListHandles = Vec<(WindowId, ForeignToplevelHandle)>;

#[macro_export]
macro_rules! delegate_foreign_management {
    ($ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($ty: [
            smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1: ()
        ] => $crate::foreign::ForeignManagement);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1: ()
        ] => $crate::foreign::ForeignManagement);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1: malaysia_scene::scene::WindowId
        ] => $crate::foreign::ForeignManagement);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(value: ToplevelState) -> u8 {
        value as u8
    }

    #[test]
    fn toplevel_states_carries_the_full_state() {
        assert!(toplevel_states(false, false, false, false).is_empty());
        assert_eq!(
            toplevel_states(true, false, false, false),
            vec![state(ToplevelState::Activated)]
        );
        assert_eq!(
            toplevel_states(false, true, false, false),
            vec![state(ToplevelState::Minimized)]
        );
        assert_eq!(
            toplevel_states(true, true, true, true),
            vec![
                state(ToplevelState::Maximized),
                state(ToplevelState::Minimized),
                state(ToplevelState::Activated),
                state(ToplevelState::Fullscreen),
            ]
        );
    }
}
