//! `infinity_focus_v1`: focus-mode notices for applications (ADR 0051).
//!
//! Open to every client: it grants nothing and only says when the bar comes
//! down over a client-decorated window in focus mode (`bar(1)`, the window
//! is moved down under it) and when it goes up again (`bar(0)`). A barrierkit
//! app shows its traffic lights only in between, like Finder.

use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource};
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use infinity_protocol::server::focus::infinity_focus_v1::infinity_focus_manager_v1::{
    self, InfinityFocusManagerV1,
};
use infinity_protocol::server::focus::infinity_focus_v1::infinity_focus_surface_v1::{
    self, InfinityFocusSurfaceV1,
};
use malaysia_scene::scene::WindowId;

use crate::state::State;

/// Marker data for the `infinity_focus_manager_v1` global and its objects.
#[derive(Debug, Clone)]
pub struct FocusNoticeGlobalData;

/// The toplevel surface an `infinity_focus_surface_v1` reports on.
#[derive(Debug)]
pub struct FocusNoticeSurfaceData {
    surface: WlSurface,
}

impl FocusNoticeGlobalData {
    /// Creates the global (version 1).
    pub fn create_global(display_handle: &DisplayHandle) -> GlobalId {
        display_handle.create_global::<State, InfinityFocusManagerV1, _>(1, FocusNoticeGlobalData)
    }
}

impl GlobalDispatch2<InfinityFocusManagerV1, State> for FocusNoticeGlobalData {
    fn bind(
        &self,
        _state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<InfinityFocusManagerV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, FocusNoticeGlobalData);
    }
}

impl Dispatch2<InfinityFocusManagerV1, State> for FocusNoticeGlobalData {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _resource: &InfinityFocusManagerV1,
        request: infinity_focus_manager_v1::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        if let infinity_focus_manager_v1::Request::GetFocusSurface { id, surface } = request {
            let notice = data_init.init(
                id,
                FocusNoticeSurfaceData {
                    surface: surface.clone(),
                },
            );
            state.focus_notices.push(notice);
        }
    }
}

impl Dispatch2<InfinityFocusSurfaceV1, State> for FocusNoticeSurfaceData {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        _resource: &InfinityFocusSurfaceV1,
        _request: infinity_focus_surface_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        // Only `destroy`, handled by `destroyed`.
    }

    fn destroyed(&self, state: &mut State, _client: ClientId, resource: &InfinityFocusSurfaceV1) {
        let id = resource.id();
        state.focus_notices.retain(|notice| notice.id() != id);
    }
}

impl State {
    /// Tells the window's client that the bar came down over it (`true`) or
    /// went up (`false`).
    pub(crate) fn send_focus_bar(&mut self, window: WindowId, revealed: bool) {
        let Some(surface) = self.windows.surface_of(window).cloned() else {
            return;
        };
        self.focus_notices.retain(|notice| notice.is_alive());
        for notice in &self.focus_notices {
            let ours = notice
                .data::<FocusNoticeSurfaceData>()
                .is_some_and(|data| data.surface == surface);
            if ours {
                notice.bar(u32::from(revealed));
            }
        }
    }
}
