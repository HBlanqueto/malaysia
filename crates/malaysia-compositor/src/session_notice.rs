use std::time::Duration;

use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource};
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use infinity_protocol::server::session::infinity_session_v1::infinity_session_v1::{
    self, InfinitySessionV1,
};

use crate::state::State;

#[derive(Debug, Clone)]
pub struct SessionNoticeGlobalData;

const VERSION: u32 = 1;

impl SessionNoticeGlobalData {
    pub fn create_global(display_handle: &DisplayHandle) -> GlobalId {
        display_handle
            .create_global::<State, InfinitySessionV1, _>(VERSION, SessionNoticeGlobalData)
    }
}

impl GlobalDispatch2<InfinitySessionV1, State> for SessionNoticeGlobalData {
    fn bind(
        &self,
        state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<InfinitySessionV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        let notice = data_init.init(resource, SessionNoticeGlobalData);
        state.session_notices.push(notice);
    }
}

impl Dispatch2<InfinitySessionV1, State> for SessionNoticeGlobalData {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        _resource: &InfinitySessionV1,
        _request: infinity_session_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
    }
}

impl State {
    pub(crate) fn notify_locking(&mut self, fade: Duration) {
        self.session_notices.retain(Resource::is_alive);
        let duration = u32::try_from(fade.as_millis()).unwrap_or(u32::MAX);
        for notice in &self.session_notices {
            notice.locking(duration);
        }
    }

    pub(crate) fn notify_unlocked(&mut self) {
        self.session_notices.retain(Resource::is_alive);
        for notice in &self.session_notices {
            notice.unlocked();
        }
    }
}
