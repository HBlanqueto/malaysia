//! The compositor's shared state.
//!
//! [`State`] is the value every protocol handler receives as `&mut State`. It
//! is deliberately backend-agnostic: the winit window, the GLES renderer and
//! the damage tracker live *beside* it, in [`crate::session`], never inside it.
//! A handler therefore cannot reach the backend, and Block 2 can replace winit
//! with DRM/KMS without editing a handler. The reasoning is recorded in
//! ADR 0012.
//!
//! The session wraps `State` together with the Wayland [`Display`] and moves
//! that wrapper into the calloop event loop; `State` itself stays free of the
//! display so that its borrow is clean when clients are dispatched (ADR 0013).
//!
//! The protocol state structs are created here, with the globals they own:
//! `wl_compositor` and `wl_subcompositor` via `CompositorState`, `wl_shm` via
//! `ShmState`, `xdg_wm_base` via `XdgShellState`, the output globals via
//! `OutputManagerState` and `Output`, and the seat via `SeatState`. Smithay
//! will not create a global until the state type implements its dispatch
//! traits, so those impls live in crate::handlers beside this module.
//!
//! [`Display`]: smithay::reexports::wayland_server::Display

use smithay::input::keyboard::XkbConfig;
use smithay::input::{Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::LoopSignal;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::ClientData;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{Logical, Physical, Size, Transform};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;

use crate::window::Windows;

/// Failures while building the compositor state.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// The seat's keyboard could not be created.
    ///
    /// This is nearly always a missing or unreadable xkb keymap rather than
    /// anything the compositor did.
    #[error("failed to set up the seat keyboard")]
    Keyboard(#[from] smithay::input::keyboard::Error),
}

/// The shared compositor state.
///
/// One instance is created per session and held by the session beside the
/// Wayland display; every protocol handler borrows it as `&mut State`. The
/// fields are public because the handlers in crate::handlers are the
/// consumers.
pub struct State {
    /// Wakes and stops the event loop, and later triggers a redraw.
    loop_signal: LoopSignal,
    /// Set when state changed in a way that needs a new frame.
    ///
    /// Handlers cannot reach the backend, so they raise this and the session
    /// turns it into a redraw request (ADR 0012).
    needs_redraw: bool,
    /// The `wl_compositor` and `wl_subcompositor` state.
    pub compositor_state: CompositorState,
    /// The `wl_shm` state and its advertised pixel formats.
    pub shm_state: ShmState,
    /// The factory for `wl_seat` globals.
    pub seat_state: SeatState<State>,
    /// The one seat Neptune offers.
    pub seat: Seat<State>,
    /// The `xdg_wm_base` state.
    pub xdg_shell_state: XdgShellState,
    /// The `wl_data_device_manager` state.
    ///
    /// Advertising the global completes the seat's protocol surface; no
    /// clipboard behavior is implemented and none is intended here. Clients
    /// that assume a conformant seat (foot among them) tear down without it.
    pub data_device_state: DataDeviceState,
    /// The `wl_output` and `zxdg_output_manager_v1` state.
    pub output_manager_state: OutputManagerState,
    /// The one output, backed by the winit window.
    pub output: Output,
    /// The output's current size, in physical pixels, mirroring its current
    /// mode. New toplevels are sized from it; the session updates it on resize.
    pub output_size: Size<i32, Physical>,
    /// The tracked toplevels. All window geometry goes through this; Block 2
    /// replaces it with `neptune-scene` (see [`crate::window`]).
    pub windows: Windows,
}

impl State {
    /// The output scale, as an integer.
    ///
    /// This is the one literal that stands in for scale. Every physical/logical
    /// conversion names it rather than writing `1`, so when fractional scale
    /// lands there is a single value to replace and a single set of call sites
    /// to revisit.
    // TODO(scale): this must not stay a literal. On a HiDPI output the integer
    // scale is 2, not 1, and fractional scaling wants a per-surface value, not
    // one global integer. Both call sites change together: [`Self::logical_output_size`]
    // below, and the render-element placement in `session::draw` (the location
    // converted with `to_physical` and the element scale, both of which read
    // this constant).
    pub const OUTPUT_SCALE: i32 = 1;

    /// Creates the state for one session and publishes its globals.
    ///
    /// `size` is the initial size of the output, which the session takes from
    /// the backend's window and updates when that window is resized.
    pub fn new(
        loop_signal: LoopSignal,
        display_handle: &DisplayHandle,
        size: Size<i32, Physical>,
    ) -> Result<Self, StateError> {
        let compositor_state = CompositorState::new::<Self>(display_handle);
        let shm_state = ShmState::new::<Self>(
            display_handle,
            [wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888],
        );
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(display_handle);
        let data_device_state = DataDeviceState::new::<Self>(display_handle);

        let mut seat_state: SeatState<Self> = SeatState::new();
        let mut seat = seat_state.new_wl_seat(display_handle, "neptune");
        // A seat with no keymap is worse than no seat: clients that accepted
        // the capability then get no keymap and misbehave. The keyboard must be
        // added before the seat is useful, so its error is fatal here.
        seat.add_keyboard(XkbConfig::default(), 200, 25)?;
        let _ = seat.add_pointer();

        let output = Output::new(
            "neptune".to_string(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Neptune".to_string(),
                model: "Winit".to_string(),
            },
        );
        let mode = Mode {
            size,
            refresh: 60_000,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);
        let _ = output.create_global::<Self>(display_handle);

        Ok(Self {
            loop_signal,
            needs_redraw: false,
            compositor_state,
            shm_state,
            seat_state,
            seat,
            xdg_shell_state,
            data_device_state,
            output_manager_state,
            output,
            output_size: size,
            windows: Windows::new(),
        })
    }

    /// Asks the event loop to stop and return from its `run` call.
    ///
    /// Safe to call from any thread, which is why the signal thread can use it
    /// as well; the signal itself is cloned at startup.
    pub fn stop(&self) {
        self.loop_signal.stop();
    }

    /// Marks that a new frame is needed.
    ///
    /// Handlers call this instead of reaching for the backend, which they
    /// cannot see (ADR 0012); the session turns it into a redraw request once
    /// the current batch of events has been dispatched.
    pub fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    /// Takes the pending redraw request, clearing it.
    pub fn take_redraw_request(&mut self) -> bool {
        std::mem::take(&mut self.needs_redraw)
    }

    /// The output size in logical coordinates.
    ///
    /// Every conversion from the output's physical mode size to logical
    /// coordinates goes through here — there is deliberately one such call
    /// site, not one per user. The output runs at scale 1, so today this is the
    /// identity; when fractional scale lands, this method is what changes.
    pub fn logical_output_size(&self) -> Size<i32, Logical> {
        self.output_size.to_logical(Self::OUTPUT_SCALE)
    }
}

/// Data attached to every connected Wayland client.
///
/// Smithay stores per-client protocol state behind this type; a handler
/// retrieves it with `Client::get_data`. The compositor's `CompositorClientState`
/// holds the per-client surface bookkeeping that `wl_compositor` needs.
#[derive(Default)]
pub struct ClientState {
    /// Per-client compositor bookkeeping, created by Smithay on first use.
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {}
