//! Shared setup for Neptune's dev-only probe clients.
//!
//! `tools/sctk-probe` is **not** part of the compositor. It is a workspace
//! member excluded from the default build, used to drive Neptune as a Wayland
//! client while developing. Its `parent_probe` binary maps two toplevels and
//! toggles a parent on the child to check that the server recomputes decoration
//! insets and reconfigures. See the crate README for the command.

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

/// Opens the Wayland connection named by the environment and returns an event
/// loop with the connection already inserted as a source.
///
/// Neither the display nor the registry is chosen here: the probe connects to
/// whatever `WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR` point at, so the caller can run
/// it against Neptune or against the host session without this crate knowing.
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
