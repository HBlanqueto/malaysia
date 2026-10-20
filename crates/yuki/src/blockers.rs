//! The two commit-blocker sources, type-erased for either backend.
//!
//! A dmabuf commit is held until its implicit fence or explicit acquire point
//! becomes readable. Smithay hands back two concrete `EventSource` types; this
//! enum gives them one type, and [`BlockerSink`] lets `State` insert it into
//! the backend's loop without knowing whether that loop is winit or udev.

use std::io;

use calloop::{EventSource, Poll, PostAction, Readiness, Token, TokenFactory};
use smithay::backend::allocator::dmabuf::DmabufSource;
use smithay::reexports::wayland_server::Client;
use smithay::wayland::drm_syncobj::DrmSyncPointSource;

/// A blocker source from either the explicit-sync or the dmabuf path.
pub(crate) enum BlockerSource {
    /// An explicit `wp_linux_drm_syncobj` acquire point.
    Sync(DrmSyncPointSource),
    /// A dmabuf's implicit fence.
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

/// Inserts a blocker into the backend's loop. The implementation lives in the
/// backend because only it knows its loop's data type.
pub(crate) trait BlockerSink {
    /// Returns whether the source was inserted. A failed insertion leaves the
    /// commit unblocked rather than blocked forever.
    fn insert(&mut self, source: BlockerSource, client: Client) -> bool;
}
