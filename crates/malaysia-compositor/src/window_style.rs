use std::cell::Cell;

use ctdesign::{CORNER_WINDOW, WINDOW_CORNER};
use limitless::server::style::limitless_window_style_v1::limitless_window_style_v1::{
    self, LimitlessWindowStyleV1,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New};
use smithay::wayland::compositor::with_states;
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use crate::state::State;

pub const VERSION: u32 = 1;

const FITTED_PER_RADIUS: f32 = CORNER_WINDOW.fitted / CORNER_WINDOW.radius;
const BAR_CORNERS: [f32; 4] = [
    WINDOW_CORNER.titlebar as f32 * FITTED_PER_RADIUS,
    WINDOW_CORNER.unified_compact as f32 * FITTED_PER_RADIUS,
    WINDOW_CORNER.unified as f32 * FITTED_PER_RADIUS,
    WINDOW_CORNER.utility as f32 * FITTED_PER_RADIUS,
];

struct BarCorner(Cell<f32>);

pub fn corner_radius(surface: &WlSurface) -> f32 {
    with_states(surface, |states| {
        states
            .data_map
            .get::<BarCorner>()
            .map_or(CORNER_WINDOW.fitted, |corner| corner.0.get())
    })
}

pub struct WindowStyle;

impl GlobalDispatch2<LimitlessWindowStyleV1, State> for WindowStyle {
    fn bind(
        &self,
        _state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<LimitlessWindowStyleV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, WindowStyle);
    }
}

impl Dispatch2<LimitlessWindowStyleV1, State> for WindowStyle {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _resource: &LimitlessWindowStyleV1,
        request: limitless_window_style_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        if let limitless_window_style_v1::Request::SetBar { surface, bar } = request
            && let Some(&radius) = BAR_CORNERS.get(u32::from(bar) as usize)
        {
            with_states(&surface, |states| {
                states
                    .data_map
                    .insert_if_missing(|| BarCorner(Cell::new(radius)));
                states.data_map.get::<BarCorner>().unwrap().0.set(radius);
            });
            state.needs_redraw = true;
        }
    }
}
