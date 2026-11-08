use std::os::unix::net::UnixStream;
use std::sync::Arc;

use smithay::backend::allocator::{Format, Fourcc, Modifier};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::calloop::ping::make_ping;
use smithay::reexports::wayland_server::{Client, Display};
use smithay::utils::Size;
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;

use crate::session::Session;
use crate::state::{ClientDisconnectQueue, ClientState, State};

pub(crate) fn test_state(size: (i32, i32)) -> (EventLoop<'static, Session>, Display<State>, State) {
    let event_loop = EventLoop::<Session>::try_new().expect("the test needs an event loop");
    let display = Display::<State>::new().expect("the test needs a display");
    let feedback = DmabufFeedbackBuilder::new(
        0,
        [Format {
            code: Fourcc::Xrgb8888,
            modifier: Modifier::Invalid,
        }],
    )
    .build()
    .expect("the feedback should build");
    let state = State::new(
        event_loop.get_signal(),
        &display.handle(),
        Size::from(size),
        feedback,
        None,
        Box::new(event_loop.handle()),
        Box::new(()),
    )
    .expect("the state should build");
    (event_loop, display, state)
}

pub(crate) fn insert_client(display: &Display<State>, server_side: UnixStream) -> Client {
    let (disconnect_ping, _) = make_ping().expect("the test needs a ping");
    display
        .handle()
        .insert_client(
            server_side,
            Arc::new(ClientState::new(
                ClientDisconnectQueue::default(),
                disconnect_ping,
                false,
            )),
        )
        .expect("the server should accept the client")
}
