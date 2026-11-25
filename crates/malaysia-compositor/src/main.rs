use std::sync::Arc;

use smithay::backend::renderer::{Color32F, Frame, Renderer, gles::GlesRenderer};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::reexports::calloop::{
    EventLoop, Interest, LoopSignal, Mode, PostAction, generic::Generic,
};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Rectangle, Transform};
use smithay::wayland::socket::ListeningSocketSource;

const CLEAR_COLOR: Color32F = Color32F::new(0.1, 0.1, 0.12, 1.0);

struct State {
    display_handle: DisplayHandle,
    loop_signal: LoopSignal,
    backend: WinitGraphicsBackend<GlesRenderer>,
}

impl State {
    fn draw(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let size = self.backend.window_size();
        {
            let (renderer, mut framebuffer) = self.backend.bind()?;
            let mut frame = renderer.render(&mut framebuffer, size, Transform::Flipped180)?;
            frame.clear(CLEAR_COLOR, &[Rectangle::from_size(size)])?;
            let _ = frame.finish()?;
        }
        self.backend.submit(None)?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = EventLoop::<State>::try_new()?;
    let display = Display::<State>::new()?;
    let (backend, winit_events) = winit::init::<GlesRenderer>()?;
    let socket = ListeningSocketSource::new_auto()?;
    let socket_name = socket.socket_name().to_os_string();

    let handle = event_loop.handle();
    handle.insert_source(socket, |stream, _, state| {
        let _ = state.display_handle.insert_client(stream, Arc::new(()));
    })?;
    let display_handle = display.handle();
    handle.insert_source(
        Generic::new(display, Interest::READ, Mode::Level),
        |_, display, state| {
            unsafe { display.get_mut().dispatch_clients(state)? };
            Ok(PostAction::Continue)
        },
    )?;
    handle.insert_source(winit_events, |event, _, state| match event {
        WinitEvent::Resized { .. } => state.backend.window().request_redraw(),
        WinitEvent::Redraw => {
            if let Err(error) = state.draw() {
                eprintln!("draw failed: {error}");
            }
        }
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        _ => {}
    })?;

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &socket_name) };
    println!("{}", socket_name.to_string_lossy());

    let mut state = State {
        display_handle,
        loop_signal: event_loop.get_signal(),
        backend,
    };
    state.backend.window().request_redraw();
    event_loop.run(None, &mut state, |state| {
        let _ = state.display_handle.flush_clients();
    })?;
    Ok(())
}
