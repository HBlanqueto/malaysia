use smithay::backend::input::InputTime;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New};
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use limitless::server::expansion::limitless_expansion_v1::limitless_expansion_v1::{
    self, LimitlessExpansionV1,
};

use crate::state::State;

pub const VERSION: u32 = 1;

pub struct ExpansionToggle;

impl GlobalDispatch2<LimitlessExpansionV1, State> for ExpansionToggle {
    fn bind(
        &self,
        _state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<LimitlessExpansionV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, ExpansionToggle);
    }
}

impl Dispatch2<LimitlessExpansionV1, State> for ExpansionToggle {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _resource: &LimitlessExpansionV1,
        request: limitless_expansion_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        if let limitless_expansion_v1::Request::Toggle = request {
            let time = InputTime::now();
            state.end_swipe(true, time);
            if state.expansion.open {
                state.close_expansion(None, time);
            } else {
                state.open_expansion(None, time);
            }
        }
    }
}
