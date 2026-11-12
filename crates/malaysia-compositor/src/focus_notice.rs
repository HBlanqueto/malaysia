use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

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
    wallpaper: AtomicU32,
    desktop: AtomicU32,
}

const VERSION: u32 = 4;
const NO_DESKTOP: u32 = u32::MAX;
const NO_WALLPAPER: u32 = u32::MAX;

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
                    wallpaper: AtomicU32::new(NO_WALLPAPER),
                    desktop: AtomicU32::new(NO_DESKTOP),
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
    fn desktop_of(&self, window: WindowId) -> u32 {
        self.spaces.space_of(window).map_or(0, |space| {
            self.spaces.desktop_number(space).unwrap_or(0) as u32
        })
    }

    pub(crate) fn wallpaper_behind(&self, window: WindowId) -> Option<ctdesign::Rgba> {
        let desktop = self.desktop_of(window);
        let grid = self.wallpaper.grid_for((desktop > 0).then_some(desktop))?;
        let outer = self.windows.iter().find(|w| w.id() == window)?.outer();
        let output = self.logical_output_size();
        grid.behind(
            (f64::from(output.w), f64::from(output.h)),
            (outer.x(), outer.y(), outer.width(), outer.height()),
        )
    }

    pub(crate) fn chrome_theme_of(&mut self, window: WindowId) -> crate::chrome::ChromeTheme {
        let Some(target) = self.wallpaper_behind(window) else {
            return self.chrome_theme;
        };
        let wallpaper = self.chrome.fading_tint(window, target, std::time::Instant::now());
        self.chrome_theme
            .behind_window(wallpaper, &self.config.appearance)
    }

    pub(crate) fn send_wallpaper_colors(&mut self) {
        self.focus_notices.retain(|notice| notice.is_alive());
        for notice in &self.focus_notices {
            let Some(data) = notice.data::<FocusNoticeSurfaceData>() else {
                continue;
            };
            let Some(id) = self
                .windows
                .iter()
                .find(|window| *window.surface() == data.surface)
                .map(|window| window.id())
            else {
                continue;
            };
            let desktop = self.desktop_of(id);
            if notice.version() >= 4 && data.desktop.swap(desktop, Ordering::Relaxed) != desktop {
                notice.desktop(desktop);
            }
            let Some(color) = self.wallpaper_behind(id) else {
                continue;
            };
            let [r, g, b, _] = color.to_rgba8();
            let rgb = u32::from_be_bytes([0, r, g, b]);
            if notice.version() >= 3 && data.wallpaper.swap(rgb, Ordering::Relaxed) != rgb {
                notice.wallpaper(rgb);
            }
        }
    }

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
