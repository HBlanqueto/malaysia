//! The window list and control protocols the dock uses.
//!
//! - **`ext-foreign-toplevel-list-v1`** (smithay): the compositor publishes its
//!   toplevels so the dock can list the running apps.
//! - **`zwlr_foreign_toplevel_management_v1`** (served here): the dock
//!   activates, closes and minimizes windows, and reports its icon rectangle
//!   (`set_rectangle`) — what the minimize animation needs.
//!
//! The management global is not in smithay 0.7, so it is served manually. Both
//! protocols are part of the shell model (ADR 0023).

use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource};
use smithay::utils::{Point, Rectangle, Size};
use smithay::wayland::foreign_toplevel_list::{ForeignToplevelHandle, ForeignToplevelListState};
use yuki_scene::scene::WindowId;

use crate::state::State;

/// The foreign-toplevel state the compositor holds.
pub struct ForeignState {
    /// The `ext-foreign-toplevel-list-v1` global (smithay).
    pub list: ForeignToplevelListState,
    /// The `zwlr_foreign_toplevel_management_v1` global (served manually).
    pub management: ForeignManagement,
}

impl ForeignState {
    /// Creates both globals.
    pub fn new(display: &DisplayHandle) -> Self {
        Self {
            list: ForeignToplevelListState::new::<State>(display),
            management: ForeignManagement::new(display),
        }
    }
}

/// The `zwlr_foreign_toplevel_management_v1` global.
pub struct ForeignManagement {
    _global: GlobalId,
    instances: Vec<ZwlrForeignToplevelManagerV1>,
}

impl ForeignManagement {
    fn new(display: &DisplayHandle) -> Self {
        let global = display.create_global::<State, ZwlrForeignToplevelManagerV1, ()>(3, ());
        Self {
            _global: global,
            instances: Vec::new(),
        }
    }

    /// Removes dead manager instances.
    fn prune(&mut self) {
        self.instances.retain(|instance| instance.is_alive());
    }
}

/// Creates a management handle for `id` on `manager` and sends its identity.
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
    handle.done();
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
        // Advertise every current toplevel to the new manager.
        let ids: Vec<WindowId> = state.windows.iter().map(|window| window.id()).collect();
        for id in ids {
            advertise_to_manager(state, id, &manager);
        }
        state.foreign.management.instances.push(manager);
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
        let id = *data;
        match request {
            zwlr_foreign_toplevel_handle_v1::Request::Activate { seat: _ } => {
                // The dock asks to activate this handle's window. The serial
                // validated by the seat belongs to the dock's own press; the
                // wlr protocol only names the seat here.
                state.activate_window(id);
            }
            zwlr_foreign_toplevel_handle_v1::Request::Close => {
                state.windows.send_close(id);
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetRectangle {
                surface: _,
                x,
                y,
                width,
                height,
            } => {
                state.dock_rectangle = Some((
                    id,
                    Rectangle::new(Point::from((x, y)), Size::from((width, height))),
                ));
                tracing::info!(window = id.get(), rect = ?state.dock_rectangle, "dock icon rectangle set");
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetMinimized => {
                state.set_minimized(id, true);
            }
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMinimized => {
                state.set_minimized(id, false);
            }
            _ => {
                // set_maximized/unset_maximized/set_fullscreen/unset_fullscreen
                // and destroy are acknowledged but not implemented yet.
            }
        }
    }
}

impl State {
    /// The window's foreign handles were needed: creates the list handle and
    /// advertises the management handle when a window maps.
    pub(crate) fn foreign_window_mapped(&mut self, id: WindowId) {
        let Some((title, app_id)) = self.windows.active_info(id) else {
            return;
        };
        // The ext list handle: `new_toplevel` stores the identity, so it must
        // be created empty and the identity sent explicitly (smithay's
        // `send_title`/`send_app_id` dedupe against the stored value).
        let handle = self.foreign.list.new_toplevel::<State>("", "");
        // The window id rides on the handle so a capture source created from it
        // can resolve the window without a reverse lookup (image_capture_source).
        handle.user_data().insert_if_missing(|| id);
        handle.send_title(&title);
        handle.send_app_id(&app_id);
        handle.send_done();
        self.list_handles.push((id, handle));
        // The management handles.
        self.advertise_foreign(id);
    }

    /// Advertises `id` to every bound management manager.
    fn advertise_foreign(&mut self, id: WindowId) {
        self.foreign.management.prune();
        let managers = self.foreign.management.instances.clone();
        for manager in &managers {
            advertise_to_manager(self, id, manager);
        }
    }

    /// Drops a window's foreign handles on destroy (`closed` is sent by the
    /// handle drop; management handles close when the resource is gone).
    pub(crate) fn foreign_window_destroyed(&mut self, id: WindowId) {
        self.list_handles.retain(|(wid, _)| *wid != id);
        self.foreign.management.prune();
    }

    /// Activates a window: restore it if minimized, then raise and focus.
    pub(crate) fn activate_window(&mut self, id: WindowId) {
        // While the session is locked, no window may take the screen or the
        // focus from the lock surface, even if the dock still asks (lock.rs).
        if self.locked {
            return;
        }
        self.set_minimized(id, false);
        self.raise_and_focus(id);
    }

    /// Minimizes or restores a window. A minimized window leaves the scene and
    /// stops rendering (it keeps its state; the dock icon becomes its handle).
    pub(crate) fn set_minimized(&mut self, id: WindowId, minimized: bool) {
        let was = self.minimized.contains(&id);
        if was == minimized {
            return;
        }
        if minimized {
            self.minimized.insert(id);
            self.windows.set_hidden(id, true);
            tracing::info!(window = id.get(), "window minimized");
            let focused = self
                .seat
                .get_keyboard()
                .and_then(|keyboard| keyboard.current_focus())
                .and_then(|focused| self.windows.id_of(&focused));
            if focused == Some(id) {
                let next = self
                    .windows
                    .iter()
                    .rev()
                    .find(|window| window.id() != id && !self.minimized.contains(&window.id()))
                    .map(|window| window.id());
                match next {
                    Some(next) => self.focus_window(next),
                    None => self.clear_keyboard_focus(),
                }
            }
        } else {
            self.minimized.remove(&id);
            self.windows.set_hidden(id, false);
            tracing::info!(window = id.get(), "window restored");
        }
        self.request_redraw();
    }
}

/// The list handles tracked by [`State::list_handles`].
pub type ListHandles = Vec<(WindowId, ForeignToplevelHandle)>;

/// Delegates the management global to [`ForeignManagement`].
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
            smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1: yuki_scene::scene::WindowId
        ] => $crate::foreign::ForeignManagement);
    };
}
