use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New};
use smithay::wayland::compositor::with_states;
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use infinity_protocol::server::style::infinity_window_style_v1::infinity_window_style_v1::{
    self, InfinityWindowStyleV1,
};
use malaysia_scene::scene::WindowId;

use crate::state::State;

#[derive(Debug, Clone)]
pub struct WindowStyleGlobalData;

struct HasToolbar;

const VERSION: u32 = 1;

impl WindowStyleGlobalData {
    pub fn create_global(display_handle: &DisplayHandle) -> GlobalId {
        display_handle
            .create_global::<State, InfinityWindowStyleV1, _>(VERSION, WindowStyleGlobalData)
    }
}

impl GlobalDispatch2<InfinityWindowStyleV1, State> for WindowStyleGlobalData {
    fn bind(
        &self,
        _state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<InfinityWindowStyleV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, WindowStyleGlobalData);
    }
}

impl Dispatch2<InfinityWindowStyleV1, State> for WindowStyleGlobalData {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _resource: &InfinityWindowStyleV1,
        request: infinity_window_style_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        if let infinity_window_style_v1::Request::SetToolbar { surface } = request {
            with_states(&surface, |states| {
                states.data_map.insert_if_missing_threadsafe(|| HasToolbar);
            });
            state.request_redraw();
        }
    }
}

pub(crate) fn has_toolbar(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states.data_map.get::<HasToolbar>().is_some()
    })
}

pub(crate) fn toplevel_radius(toolbar: bool) -> u32 {
    if toolbar {
        vow_design::WINDOW_TOOLBAR.corner_radius
    } else {
        vow_design::WINDOW.corner_radius
    }
}

impl State {
    pub(crate) fn window_radius(&self, window: WindowId) -> u32 {
        toplevel_radius(self.windows.surface_of(window).is_some_and(has_toolbar))
    }
}
