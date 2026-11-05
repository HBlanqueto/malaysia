pub mod dmabuf;
pub mod harness;

use smithay_client_toolkit::reexports::{
    calloop,
    calloop_wayland_source::WaylandSource,
    client::{self as wayland_client, globals::GlobalList},
};

use calloop::EventLoop;
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::wl_registry::WlRegistry,
    Connection, Dispatch, QueueHandle,
};

pub fn init_connection<APP>() -> (EventLoop<'static, APP>, GlobalList, QueueHandle<APP>)
where
    APP: Dispatch<WlRegistry, GlobalListContents> + 'static,
{
    let conn =
        Connection::connect_to_env().expect("WAYLAND_DISPLAY is not set to a reachable socket");

    let (globals, event_queue) = registry_queue_init(&conn).expect("failed to read the registry");
    let qh = event_queue.handle();
    let event_loop: EventLoop<APP> = EventLoop::try_new().expect("failed to create the event loop");
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle)
        .expect("failed to insert the Wayland source");

    (event_loop, globals, qh)
}
