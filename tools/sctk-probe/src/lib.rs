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

#[macro_export]
macro_rules! noop_compositor_handler {
    ($app:ty) => {
        impl ::smithay_client_toolkit::compositor::CompositorHandler for $app {
            fn frame(
                &mut self,
                _: &::smithay_client_toolkit::reexports::client::Connection,
                _: &::smithay_client_toolkit::reexports::client::QueueHandle<Self>,
                _: &::smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
                _: u32,
            ) {
            }
            fn surface_enter(
                &mut self,
                _: &::smithay_client_toolkit::reexports::client::Connection,
                _: &::smithay_client_toolkit::reexports::client::QueueHandle<Self>,
                _: &::smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
                _: &::smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
            fn surface_leave(
                &mut self,
                _: &::smithay_client_toolkit::reexports::client::Connection,
                _: &::smithay_client_toolkit::reexports::client::QueueHandle<Self>,
                _: &::smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
                _: &::smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
            fn scale_factor_changed(
                &mut self,
                _: &::smithay_client_toolkit::reexports::client::Connection,
                _: &::smithay_client_toolkit::reexports::client::QueueHandle<Self>,
                _: &::smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
                _: i32,
            ) {
            }
            fn transform_changed(
                &mut self,
                _: &::smithay_client_toolkit::reexports::client::Connection,
                _: &::smithay_client_toolkit::reexports::client::QueueHandle<Self>,
                _: &::smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
                _: ::smithay_client_toolkit::reexports::client::protocol::wl_output::Transform,
            ) {
            }
        }
    };
}

#[macro_export]
macro_rules! noop_output_handler {
    ($app:ty) => {
        impl ::smithay_client_toolkit::output::OutputHandler for $app {
            fn output_state(&mut self) -> &mut ::smithay_client_toolkit::output::OutputState {
                &mut self.output_state
            }
            fn new_output(
                &mut self,
                _: &::smithay_client_toolkit::reexports::client::Connection,
                _: &::smithay_client_toolkit::reexports::client::QueueHandle<Self>,
                _: ::smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
            fn update_output(
                &mut self,
                _: &::smithay_client_toolkit::reexports::client::Connection,
                _: &::smithay_client_toolkit::reexports::client::QueueHandle<Self>,
                _: ::smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
            fn output_destroyed(
                &mut self,
                _: &::smithay_client_toolkit::reexports::client::Connection,
                _: &::smithay_client_toolkit::reexports::client::QueueHandle<Self>,
                _: ::smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
        }
    };
}

#[macro_export]
macro_rules! shm_and_registry {
    ($app:ty) => {
        impl ::smithay_client_toolkit::shm::ShmHandler for $app {
            fn shm_state(&mut self) -> &mut ::smithay_client_toolkit::shm::Shm {
                &mut self.shm
            }
        }
        impl ::smithay_client_toolkit::registry::ProvidesRegistryState for $app {
            fn registry(&mut self) -> &mut ::smithay_client_toolkit::registry::RegistryState {
                &mut self.registry_state
            }
            ::smithay_client_toolkit::registry_handlers![
                ::smithay_client_toolkit::output::OutputState,
            ];
        }
    };
}
