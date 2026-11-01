//! `infinity_focus_v1`: tells applications when focus mode hides or shows the bar.

use std::sync::atomic::{AtomicBool, Ordering};

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

#[derive(Debug, Clone)]
pub struct FocusNoticeGlobalData;

#[derive(Debug)]
pub struct FocusNoticeSurfaceData {
    surface: WlSurface,
    overlay: AtomicBool,
}

const VERSION: u32 = 2;

impl FocusNoticeGlobalData {
    pub fn create_global(display_handle: &DisplayHandle) -> GlobalId {
        display_handle
            .create_global::<State, InfinityFocusManagerV1, _>(VERSION, FocusNoticeGlobalData)
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
                    overlay: AtomicBool::new(false),
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
        request: infinity_focus_surface_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        if let infinity_focus_surface_v1::Request::SetOverlay = request {
            self.overlay.store(true, Ordering::Relaxed);
        }
    }

    fn destroyed(&self, state: &mut State, _client: ClientId, resource: &InfinityFocusSurfaceV1) {
        let id = resource.id();
        state.focus_notices.retain(|notice| notice.id() != id);
    }
}

impl State {
    pub(crate) fn focus_overlay(&self, window: WindowId) -> bool {
        let Some(surface) = self.windows.surface_of(window) else {
            return false;
        };
        self.focus_notices.iter().any(|notice| {
            notice.is_alive()
                && notice.data::<FocusNoticeSurfaceData>().is_some_and(|data| {
                    data.surface == *surface && data.overlay.load(Ordering::Relaxed)
                })
        })
    }

    pub(crate) fn send_focus_bar(&mut self, window: WindowId, revealed: bool) {
        let Some(surface) = self.windows.surface_of(window).cloned() else {
            return;
        };
        let height = u32::try_from(self.desktop_bar_height()).unwrap_or(0);
        self.focus_notices.retain(|notice| notice.is_alive());
        for notice in &self.focus_notices {
            let Some(data) = notice.data::<FocusNoticeSurfaceData>() else {
                continue;
            };
            if data.surface != surface {
                continue;
            }
            if revealed && data.overlay.load(Ordering::Relaxed) {
                notice.bar_height(height);
            }
            notice.bar(u32::from(revealed));
        }
    }
}
