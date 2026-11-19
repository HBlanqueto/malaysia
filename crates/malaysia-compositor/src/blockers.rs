use std::io;

use calloop::{EventSource, Poll, PostAction, Readiness, Token, TokenFactory};
use smithay::backend::allocator::dmabuf::DmabufSource;
use smithay::reexports::wayland_server::Client;
use smithay::wayland::drm_syncobj::DrmSyncPointSource;

pub(crate) enum BlockerSource {
    Sync(DrmSyncPointSource),
    Dmabuf(DmabufSource),
}

impl EventSource for BlockerSource {
    type Event = ();
    type Metadata = ();
    type Ret = Result<(), io::Error>;
    type Error = io::Error;

    fn process_events<F>(
        &mut self,
        readiness: Readiness,
        token: Token,
        mut callback: F,
    ) -> Result<PostAction, Self::Error>
    where
        F: FnMut(Self::Event, &mut Self::Metadata) -> Self::Ret,
    {
        match self {
            Self::Sync(source) => {
                source.process_events(readiness, token, |_, _| callback((), &mut ()))
            }
            Self::Dmabuf(source) => {
                source.process_events(readiness, token, |_, _| callback((), &mut ()))
            }
        }
    }

    fn register(
        &mut self,
        poll: &mut Poll,
        token_factory: &mut TokenFactory,
    ) -> calloop::Result<()> {
        match self {
            Self::Sync(source) => source.register(poll, token_factory),
            Self::Dmabuf(source) => source.register(poll, token_factory),
        }
    }

    fn reregister(
        &mut self,
        poll: &mut Poll,
        token_factory: &mut TokenFactory,
    ) -> calloop::Result<()> {
        match self {
            Self::Sync(source) => source.reregister(poll, token_factory),
            Self::Dmabuf(source) => source.reregister(poll, token_factory),
        }
    }

    fn unregister(&mut self, poll: &mut Poll) -> calloop::Result<()> {
        match self {
            Self::Sync(source) => source.unregister(poll),
            Self::Dmabuf(source) => source.unregister(poll),
        }
    }
}

pub(crate) trait BlockerSink {
    fn insert(&mut self, source: BlockerSource, client: Client) -> bool;
}

impl<D: crate::idle::IdleHost + 'static> BlockerSink for calloop::LoopHandle<'static, D> {
    fn insert(&mut self, source: BlockerSource, client: Client) -> bool {
        self.insert_source(source, move |_, _, host: &mut D| {
            if let Some(client_state) = client.get_data::<crate::state::ClientState>() {
                let state = host.idle_state();
                let dh = state.display_handle.clone();
                client_state.compositor_state.blocker_cleared(state, &dh);
            }
            Ok(())
        })
        .is_ok()
    }
}
