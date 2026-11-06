use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use smithay::reexports::wayland_server::WEnum;
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New};
use smithay::wayland::compositor::with_states;
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use infinity_protocol::server::style::infinity_window_style_v1::infinity_window_style_v1::{
    self, InfinityWindowStyleV1,
};
pub(crate) use infinity_protocol::server::style::infinity_window_style_v1::infinity_window_style_v1::Bar;
use malaysia_scene::scene::WindowId;

use crate::decoration::Role;
use crate::state::State;

#[derive(Debug, Clone)]
pub struct WindowStyleGlobalData;

struct BarStyle(AtomicU32);

struct AlertLayer(u64);

static NEXT_ALERT: AtomicU64 = AtomicU64::new(0);

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
        if let infinity_window_style_v1::Request::SetBar {
            surface,
            bar: WEnum::Value(bar),
        } = request
        {
            with_states(&surface, |states| {
                states
                    .data_map
                    .insert_if_missing_threadsafe(|| BarStyle(AtomicU32::new(0)));
                if let Some(style) = states.data_map.get::<BarStyle>() {
                    style.0.store(bar.into(), Ordering::Relaxed);
                }
            });
            state.request_redraw();
        }
    }
}

pub(crate) fn bar(surface: &WlSurface) -> Option<Bar> {
    with_states(surface, |states| {
        let style = states.data_map.get::<BarStyle>()?;
        Bar::try_from(style.0.load(Ordering::Relaxed)).ok()
    })
}

pub(crate) fn mark_alert_layer(surface: &WlSurface) {
    with_states(surface, |states| {
        states.data_map.insert_if_missing_threadsafe(|| {
            AlertLayer(NEXT_ALERT.fetch_add(1, Ordering::Relaxed))
        });
    });
}

pub(crate) fn alert_layer(surface: &WlSurface) -> Option<u64> {
    with_states(surface, |states| {
        states.data_map.get::<AlertLayer>().map(|alert| alert.0)
    })
}

pub(crate) fn corner_radius(
    forced: Option<u32>,
    bar: Option<Bar>,
    role: Role,
    client_decorated: bool,
) -> u32 {
    let corner = vow_design::WINDOW_CORNER;
    if let Some(radius) = forced {
        return radius;
    }
    match (bar, role) {
        (Some(Bar::Compact), _) => corner.compact,
        (Some(Bar::Unified), _) | (None, Role::Dialog) => corner.unified,
        (Some(Bar::Utility), _) => corner.utility,
        (None, Role::Toplevel) if client_decorated => corner.unified,
        _ => corner.titlebar,
    }
}

// Apple's continuous corner starts 1.5287 R along each edge, not at R (ADR 0059).
pub(crate) fn corner_extent(radius: u32) -> f64 {
    f64::from(radius) * f64::from(vow_design::WINDOW_CORNER.expansion)
}

impl State {
    pub(crate) fn window_corner(&self, window: WindowId) -> f64 {
        self.windows
            .iter()
            .find(|candidate| candidate.id() == window)
            .map_or(0.0, |window| {
                corner_extent(corner_radius(
                    self.config.window.corner_radius,
                    bar(window.surface()),
                    window.role(),
                    window.is_client_decorated(),
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_bar_gets_the_tahoe_radius() {
        let radius = |bar| corner_radius(None, Some(bar), Role::Toplevel, true);
        assert_eq!(radius(Bar::Titlebar), 16);
        assert_eq!(radius(Bar::Compact), 20);
        assert_eq!(radius(Bar::Unified), 26);
        assert_eq!(radius(Bar::Utility), 15);
    }

    #[test]
    fn without_a_bar_ssd_gets_the_titlebar_and_csd_and_dialogs_the_toolbar() {
        assert_eq!(corner_radius(None, None, Role::Toplevel, false), 16);
        assert_eq!(corner_radius(None, None, Role::Toplevel, true), 26);
        assert_eq!(corner_radius(None, None, Role::Dialog, false), 26);
    }

    #[test]
    fn a_configured_radius_overrides_every_bar() {
        for bar in [None, Some(Bar::Titlebar), Some(Bar::Unified)] {
            assert_eq!(corner_radius(Some(8), bar, Role::Toplevel, true), 8);
        }
    }
}
