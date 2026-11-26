use limitless::server::style::limitless_window_style_v1::limitless_window_style_v1::{
    self, LimitlessWindowStyleV1,
};
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New};
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use crate::state::State;

pub const VERSION: u32 = 1;

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
        _state: &mut State,
        _client: &Client,
        _resource: &LimitlessWindowStyleV1,
        _request: limitless_window_style_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
    }
}
