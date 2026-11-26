mod drm;
mod effects;
mod handlers;
mod input;
mod scale;
mod state;
mod window_style;
mod winit;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use limitless::server::style::limitless_window_style_v1::limitless_window_style_v1::LimitlessWindowStyleV1;
use smithay::desktop::{PopupManager, Space};
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{SeatState, keyboard::XkbConfig};
use smithay::reexports::calloop::signals::{Signal, Signals};
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::viewporter::ViewporterState;
use tracing_subscriber::{filter::Targets, fmt, prelude::*};

use crate::drm::Drm;
use crate::input::{REPEAT_DELAY, REPEAT_RATE};
use crate::state::{Backend, ClientState, State};
use crate::window_style::WindowStyle;
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
    let dark = std::env::var("MALAYSIA_SCHEME").is_ok_and(|scheme| scheme == "dark");
    let device = std::env::args_os().nth(1).map(PathBuf::from);
    let nested = std::env::var_os("WAYLAND_DISPLAY").is_some();

    let mut event_loop = EventLoop::<State>::try_new()?;
    let signals = Signals::new(&[Signal::SIGTERM, Signal::SIGINT, Signal::SIGHUP])?;
    let display = Display::<State>::new()?;
    let display_handle = display.handle();
    let loop_handle = event_loop.handle();
    let (backend, output) = if device.is_none() && nested {
        tracing::info!("winit backend");
        let (winit, output) =
            Winit::init(&display_handle, &loop_handle, event_loop.get_signal(), dark)?;
        (Backend::Winit(Box::new(winit)), output)
    } else {
        let (drm, output) = Drm::init(device, &display_handle, &loop_handle, dark)?;
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
    loop_handle.insert_source(signals, |event, _, state| {
        tracing::info!(signal = ?event.signal(), "stopping: signal");
        state.loop_signal.stop();
    })?;

    tracing::info!(socket = %socket_name.to_string_lossy(), "listening");

    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&display_handle, "seat0");
    seat.add_keyboard(XkbConfig::default(), REPEAT_DELAY, REPEAT_RATE)?;
    seat.add_pointer();

    FractionalScaleManagerState::new::<State>(&display_handle);
    ViewporterState::new::<State>(&display_handle);
    display_handle
        .create_global::<State, LimitlessWindowStyleV1, _>(window_style::VERSION, WindowStyle);

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
