use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use smithay::reexports::wayland_server::WEnum;
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::PopupSurface;
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use infinity_protocol::server::style::infinity_window_style_v1::infinity_window_style_v1::{
    self, InfinityWindowStyleV1,
};
pub(crate) use infinity_protocol::server::style::infinity_window_style_v1::infinity_window_style_v1::{
    Bar, Panel,
};
use malaysia_scene::scene::WindowId;

use crate::decoration::Role;
use crate::state::{ClientState, State};

#[derive(Debug, Clone)]
pub struct WindowStyleGlobalData;

struct BarStyle(AtomicU32);

struct AlertLayer(u64);

struct Popover;

struct PanelStyle(AtomicU32);

static NEXT_ALERT: AtomicU64 = AtomicU64::new(0);

const VERSION: u32 = 3;

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
        client: &Client,
        resource: New<InfinityWindowStyleV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        if let Some(client_state) = client.get_data::<ClientState>() {
            client_state.kit_client.store(true, Ordering::Relaxed);
        }
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
        match request {
            infinity_window_style_v1::Request::SetBar {
                surface,
                bar: WEnum::Value(bar),
            } => {
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
            infinity_window_style_v1::Request::SetPanel { surface, flags } => {
                let bits = match flags {
                    WEnum::Value(flags) => flags.bits(),
                    WEnum::Unknown(bits) => bits,
                };
                with_states(&surface, |states| {
                    states
                        .data_map
                        .insert_if_missing_threadsafe(|| PanelStyle(AtomicU32::new(0)));
                    if let Some(style) = states.data_map.get::<PanelStyle>() {
                        style.0.store(bits, Ordering::Relaxed);
                    }
                });
                state.sync_panels();
            }
            infinity_window_style_v1::Request::SetPopover { surface } => {
                with_states(&surface, |states| {
                    states.data_map.insert_if_missing_threadsafe(|| Popover);
                });
                state.request_redraw();
            }
            _ => {}
        }
    }
}

pub(crate) fn bar(surface: &WlSurface) -> Option<Bar> {
    with_states(surface, |states| {
        let style = states.data_map.get::<BarStyle>()?;
        Bar::try_from(style.0.load(Ordering::Relaxed)).ok()
    })
}

pub(crate) fn panel(surface: &WlSurface) -> Option<Panel> {
    with_states(surface, |states| {
        let style = states.data_map.get::<PanelStyle>()?;
        Some(Panel::from_bits_truncate(style.0.load(Ordering::Relaxed)))
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

// ADR 0035: binding Infinity marks a barrier-kit client, whatever its app_id.
pub(crate) fn is_kit_client(surface: &WlSurface) -> bool {
    surface.client().is_some_and(|client| {
        client
            .get_data::<ClientState>()
            .is_some_and(|state| state.kit_client.load(Ordering::Relaxed))
    })
}

pub(crate) fn is_popover(surface: &WlSurface) -> bool {
    with_states(surface, |states| states.data_map.get::<Popover>().is_some())
}

pub(crate) fn popover_tip(popup: &PopupSurface, width: f64) -> f64 {
    let anchor_center = popup.with_pending_state(|state| {
        let anchor = state.positioner.anchor_rect;
        f64::from(anchor.loc.x) + f64::from(anchor.size.w) / 2.0 - f64::from(state.geometry.loc.x)
    });
    clamped_tip(anchor_center, width)
}

fn clamped_tip(anchor_center: f64, width: f64) -> f64 {
    let popover = ctdesign::POPOVER;
    let edge = f64::from(popover.radius) + f64::from(popover.arrow_width) / 2.0;
    if width < 2.0 * edge {
        width / 2.0
    } else {
        anchor_center.clamp(edge, width - edge)
    }
}

pub(crate) fn corner_radius(
    forced: Option<u32>,
    bar: Option<Bar>,
    role: Role,
    client_decorated: bool,
) -> u32 {
    let corner = ctdesign::WINDOW_CORNER;
    if let Some(radius) = forced {
        return radius;
    }
    match (bar, role) {
        (Some(Bar::UnifiedCompact), _) => corner.unified_compact,
        (Some(Bar::Unified), _) | (None, Role::Dialog) => corner.unified,
        (Some(Bar::Utility), _) => corner.utility,
        (None, Role::Toplevel) if client_decorated => corner.unified,
        _ => corner.titlebar,
    }
}

// Apple's continuous corner starts 1.5287 R along each edge, not at R (ADR 0059).
pub(crate) fn corner_extent(radius: u32) -> f64 {
    f64::from(radius) * f64::from(ctdesign::WINDOW_CORNER.expansion)
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
        assert_eq!(radius(Bar::UnifiedCompact), 20);
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
    fn the_popover_arrow_points_at_the_anchor_but_stays_clear_of_the_corners() {
        let edge =
            f64::from(ctdesign::POPOVER.radius) + f64::from(ctdesign::POPOVER.arrow_width) / 2.0;
        assert_eq!(clamped_tip(120.0, 300.0), 120.0);
        assert_eq!(clamped_tip(-40.0, 300.0), edge);
        assert_eq!(clamped_tip(400.0, 300.0), 300.0 - edge);
        assert_eq!(clamped_tip(10.0, 40.0), 20.0);
    }

    #[test]
    fn a_configured_radius_overrides_every_bar() {
        for bar in [None, Some(Bar::Titlebar), Some(Bar::Unified)] {
            assert_eq!(corner_radius(Some(8), bar, Role::Toplevel, true), 8);
        }
    }
}
