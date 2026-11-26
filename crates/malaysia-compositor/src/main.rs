mod drm;
mod handlers;
mod input;
mod scale;
mod state;
mod winit;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use smithay::desktop::{PopupManager, Space};
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{SeatState, keyboard::XkbConfig};
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use tracing_subscriber::{filter::Targets, fmt, prelude::*};

use crate::drm::Drm;
use crate::input::{REPEAT_DELAY, REPEAT_RATE};
use crate::state::{Backend, ClientState, State};
use crate::winit::Winit;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|directives| {
            directives
                .parse()
                .map_err(|error| eprintln!("invalid RUST_LOG: {error}"))
                .ok()
        })
        .unwrap_or_else(|| Targets::new().with_default(tracing::Level::INFO));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
    let device = std::env::args_os().nth(1).map(PathBuf::from);
    let nested = std::env::var_os("WAYLAND_DISPLAY").is_some();

    let mut event_loop = EventLoop::<State>::try_new()?;
    let display = Display::<State>::new()?;
    let display_handle = display.handle();
    let loop_handle = event_loop.handle();
    let (backend, output) = if device.is_none() && nested {
        tracing::info!("winit backend");
        let (winit, output) = Winit::init(&display_handle, &loop_handle)?;
        (Backend::Winit(Box::new(winit)), output)
    } else {
        let (drm, output) = Drm::init(device, &display_handle, &loop_handle)?;
        (Backend::Drm(Box::new(drm)), output)
    };
    let mut space = Space::default();
    space.map_output(&output, (0, 0));

    let socket = ListeningSocketSource::new_auto()?;
    let socket_name = socket.socket_name().to_os_string();
    loop_handle.insert_source(socket, |stream, _, state| {
        if let Err(error) = state
            .display_handle
            .insert_client(stream, Arc::new(ClientState::default()))
        {
            tracing::warn!(%error, "client insert failed");
        }
    })?;
    loop_handle.insert_source(
        Generic::new(display, Interest::READ, Mode::Level),
        |_, display, state| {
            unsafe { display.get_mut().dispatch_clients(state)? };
            Ok(PostAction::Continue)
        },
    )?;

    tracing::info!(socket = %socket_name.to_string_lossy(), "listening");

    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&display_handle, "seat0");
    seat.add_keyboard(XkbConfig::default(), REPEAT_DELAY, REPEAT_RATE)?;
    seat.add_pointer();

    let mut state = State {
        compositor_state: CompositorState::new::<State>(&display_handle),
        xdg_shell_state: XdgShellState::new::<State>(&display_handle),
        shm_state: ShmState::new::<State>(&display_handle, []),
        data_device_state: DataDeviceState::new::<State>(&display_handle),
        seat_state,
        seat,
        display_handle,
        loop_handle,
        loop_signal: event_loop.get_signal(),
        start_time: Instant::now(),
        backend,
        output,
        space,
        popups: PopupManager::default(),
        needs_redraw: true,
        frame_pending: false,
        suppressed_keys: Vec::new(),
        cursor_status: CursorImageStatus::default_named(),
        frame_elements: Vec::new(),
    };
    event_loop.run(None, &mut state, |state| {
        state.space.refresh();
        state.popups.cleanup();
        state.redraw_if_needed();
        if let Err(error) = state.display_handle.flush_clients() {
            tracing::warn!(%error, "client flush failed");
        }
    })?;
    if let Backend::Drm(drm) = &mut state.backend {
        drm.pause();
    }
    Ok(())
}
