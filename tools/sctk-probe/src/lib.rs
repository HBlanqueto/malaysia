pub mod dmabuf;
pub mod harness;

use sctk::reexports::{
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
        impl ::sctk::compositor::CompositorHandler for $app {
            fn frame(
                &mut self,
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
                _: &::sctk::reexports::client::protocol::wl_surface::WlSurface,
                _: u32,
            ) {
            }
            fn surface_enter(
                &mut self,
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
                _: &::sctk::reexports::client::protocol::wl_surface::WlSurface,
                _: &::sctk::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
            fn surface_leave(
                &mut self,
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
                _: &::sctk::reexports::client::protocol::wl_surface::WlSurface,
                _: &::sctk::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
            fn scale_factor_changed(
                &mut self,
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
                _: &::sctk::reexports::client::protocol::wl_surface::WlSurface,
                _: i32,
            ) {
            }
            fn transform_changed(
                &mut self,
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
                _: &::sctk::reexports::client::protocol::wl_surface::WlSurface,
                _: ::sctk::reexports::client::protocol::wl_output::Transform,
            ) {
            }
        }
    };
}

#[macro_export]
macro_rules! noop_output_handler {
    ($app:ty) => {
        impl ::sctk::output::OutputHandler for $app {
            fn output_state(&mut self) -> &mut ::sctk::output::OutputState {
                &mut self.output_state
            }
            fn new_output(
                &mut self,
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
                _: ::sctk::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
            fn update_output(
                &mut self,
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
                _: ::sctk::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
            fn output_destroyed(
                &mut self,
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
                _: ::sctk::reexports::client::protocol::wl_output::WlOutput,
            ) {
            }
        }
    };
}

#[macro_export]
macro_rules! shm_and_registry {
    ($app:ty) => {
        impl ::sctk::shm::ShmHandler for $app {
            fn shm_state(&mut self) -> &mut ::sctk::shm::Shm {
                &mut self.shm
            }
        }
        impl ::sctk::registry::ProvidesRegistryState for $app {
            fn registry(&mut self) -> &mut ::sctk::registry::RegistryState {
                &mut self.registry_state
            }
            ::sctk::registry_handlers![::sctk::output::OutputState,];
        }
    };
}

pub fn is_linear_capture_format(format: u32) -> bool {
    matches!(format, 0x3432_5241 | 0x3432_5258)
}

#[macro_export]
macro_rules! capture_session_and_frame {
    ($app:ty, $probe:literal) => {
        impl ::sctk::reexports::client::Dispatch<
            ::sctk::reexports::protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
            (),
        > for $app
        {
            fn event(
                state: &mut Self,
                _: &::sctk::reexports::protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
                event: ::sctk::reexports::protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::Event,
                _: &(),
                _: &::sctk::reexports::client::Connection,
                qh: &::sctk::reexports::client::QueueHandle<Self>,
            ) {
                use ::sctk::reexports::client::WEnum;
                use ::sctk::reexports::protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::Event;
                match event {
                    Event::BufferSize { width, height } => {
                        state.size = Some((width as i32, height as i32));
                    }
                    Event::ShmFormat { format } => {
                        if state.shm_format.is_none() {
                            if let WEnum::Value(format) = format {
                                state.shm_format = Some(format);
                            }
                        }
                    }
                    Event::DmabufDevice { .. } => {
                        state.dma_offered = true;
                    }
                    Event::DmabufFormat { format, modifiers } => {
                        state.dma_offered = true;
                        if state.dma_format.is_none() && $crate::is_linear_capture_format(format) {
                            let linear = modifiers
                                .as_chunks::<8>()
                                .0
                                .iter()
                                .map(|chunk| u64::from_le_bytes(*chunk))
                                .find(|modifier| *modifier == 0);
                            if let Some(modifier) = linear {
                                state.dma_format = Some((format, modifier));
                            }
                        }
                    }
                    Event::Done => {
                        println!(
                            "{}: constraints done (dma-buf offered: {})",
                            $probe, state.dma_offered
                        );
                        state.done = true;
                        state.start_capture(qh);
                    }
                    Event::Stopped => {
                        eprintln!("{}: FAIL the compositor stopped the session", $probe);
                        state.failed = true;
                        state.loop_signal.stop();
                    }
                    _ => {}
                }
            }
        }

        impl ::sctk::reexports::client::Dispatch<
            ::sctk::reexports::protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
            (),
        > for $app
        {
            fn event(
                state: &mut Self,
                _: &::sctk::reexports::protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
                event: ::sctk::reexports::protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::Event,
                _: &(),
                _: &::sctk::reexports::client::Connection,
                _: &::sctk::reexports::client::QueueHandle<Self>,
            ) {
                use ::sctk::reexports::protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::Event;
                match event {
                    Event::Ready => {
                        println!("{}: frame ready", $probe);
                        state.check_pixels();
                        state.ready = true;
                        state.loop_signal.stop();
                    }
                    Event::Failed { reason } => {
                        eprintln!("{}: FAIL the frame failed: {reason:?}", $probe);
                        state.failed = true;
                        state.loop_signal.stop();
                    }
                    _ => {}
                }
            }
        }
    };
}
