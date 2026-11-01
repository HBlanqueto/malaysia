//! `infinity_focus_v1`: focus-mode notices for applications (ADR 0051).
//!
//! Open to every client: it grants nothing and only says when the bar comes
//! down over a client-decorated window in focus mode (`bar(1)`) and when it
//! goes up again (`bar(0)`). A barrierkit app shows its traffic lights only
//! in between, like Finder.
//!
//! Version 1 objects get their window moved down under the bar. A version 2
//! object that sent `set_overlay` draws its titlebar over its content below
//! the bar itself, so the window stays put and `bar_height` precedes every
//! `bar(1)` (ADR 0054).

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

/// Marker data for the `infinity_focus_manager_v1` global and its objects.
#[derive(Debug, Clone)]
pub struct FocusNoticeGlobalData;

/// The toplevel surface an `infinity_focus_surface_v1` reports on.
#[derive(Debug)]
pub struct FocusNoticeSurfaceData {
    surface: WlSurface,
    /// The client draws its titlebar over its content (`set_overlay`).
    overlay: AtomicBool,
}

/// The global's version: 2 adds `set_overlay` and `bar_height`.
const VERSION: u32 = 2;

impl FocusNoticeGlobalData {
    /// Creates the global.
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
        // `destroy` is handled by `destroyed`.
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
    /// Whether the window's client draws its titlebar over its content in
    /// focus mode (`set_overlay`), so the bar must not move the window.
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

    /// Tells the window's client that the bar came down over it (`true`) or
    /// went up (`false`). An overlay client first gets the bar's height.
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
