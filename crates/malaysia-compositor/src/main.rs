mod drm;
mod handlers;
mod state;
mod winit;

use std::sync::Arc;
use std::time::Instant;

use smithay::desktop::Space;
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use tracing_subscriber::{filter::Targets, fmt, prelude::*};

use crate::drm::Drm;
use crate::state::{Backend, ClientState, State};
use crate::winit::Winit;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|directives| directives.parse().ok())
        .unwrap_or_else(|| Targets::new().with_default(tracing::Level::INFO));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
    let device = std::env::args().nth(1);
    let nested = ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some());

    let mut event_loop = EventLoop::<State>::try_new()?;
    let display = Display::<State>::new()?;
    let display_handle = display.handle();
    let loop_handle = event_loop.handle();
    let (backend, output) = match device {
        Some(path) => {
            tracing::info!(%path, "drm backend");
            let (drm, output) = Drm::init(&path, &display_handle, &loop_handle)?;
            (Backend::Drm(Box::new(drm)), output)
        }
        None if nested => {
            tracing::info!("winit backend");
            let (winit, output) =
                Winit::init(&display_handle, &loop_handle, event_loop.get_signal())?;
            (Backend::Winit(Box::new(winit)), output)
        }
        None => return Err("usage: malaysia-compositor [/dev/dri/cardN]".into()),
    };
    let mut space = Space::default();
    space.map_output(&output, (0, 0));

    let socket = ListeningSocketSource::new_auto()?;
    let socket_name = socket.socket_name().to_os_string();
    loop_handle.insert_source(socket, |stream, _, state| {
        let _ = state
            .display_handle
            .insert_client(stream, Arc::new(ClientState::default()));
    })?;
    loop_handle.insert_source(
        Generic::new(display, Interest::READ, Mode::Level),
        |_, display, state| {
            unsafe { display.get_mut().dispatch_clients(state)? };
            Ok(PostAction::Continue)
        },
    )?;

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &socket_name) };
    tracing::info!(socket = %socket_name.to_string_lossy(), "listening");

    let mut state = State {
        compositor_state: CompositorState::new::<State>(&display_handle),
        xdg_shell_state: XdgShellState::new::<State>(&display_handle),
        shm_state: ShmState::new::<State>(&display_handle, []),
        display_handle,
        loop_handle,
        start_time: Instant::now(),
        backend,
        output,
        space,
        needs_redraw: true,
        frame_pending: false,
    };
    event_loop.run(None, &mut state, |state| {
        state.space.refresh();
        state.redraw_if_needed();
        let _ = state.display_handle.flush_clients();
    })?;
    Ok(())
}
