//! The compositor's shared state.
//!
//! [`State`] is the value every protocol handler receives as `&mut State`. It
//! is deliberately backend-agnostic: the winit window, the GLES renderer and
//! the damage tracker live *beside* it, in [`crate::session`], never inside it.
//! A handler therefore cannot reach the backend, and the Direct Session
//! milestone can replace winit with DRM/KMS without editing a handler. The
//! reasoning is recorded in ADR 0012.
//!
//! The session wraps `State` together with the Wayland [`Display`] and moves
//! that wrapper into the calloop event loop; `State` itself stays free of the
//! display so that its borrow is clean when clients are dispatched (ADR 0013).
//! It holds only a [`DisplayHandle`], for server-side helpers that need one.
//!
//! The protocol state structs are created here, with the globals they own:
//! `wl_compositor` and `wl_subcompositor` via `CompositorState`, `wl_shm` via
//! `ShmState`, `xdg_wm_base` via `XdgShellState`, the output globals via
//! `OutputManagerState` and `Output`, and the seat via `SeatState`. Smithay
//! will not create a global until the state type implements its dispatch
//! traits, so those impls live in crate::handlers beside this module.
//!
//! [`Display`]: smithay::reexports::wayland_server::Display

use yuki_scene::geometry::{Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect};
use yuki_scene::scene::{Edge, WindowId};
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::drm::DrmDeviceFd;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::LoopSignal;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::ClientData;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size, Transform};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::dmabuf::{DmabufFeedback, DmabufState, ImportNotifier};
use smithay::wayland::drm_syncobj::DrmSyncobjState;
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::session_lock::{LockSurface, SessionLockManagerState};
use smithay::wayland::viewporter::ViewporterState;
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDefaultMode;
use smithay::wayland::shell::kde::decoration::KdeDecorationState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;

use crate::config::Config;
use crate::foreign::{ForeignState, ListHandles};
use crate::layers::Layers;
use crate::popup::Popups;
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
    /// A VT switch requested by the keyboard, for the DRM backend to perform.
    ///
    /// The nested backend never acts on it; input is shared, the action is not.
    pending_vt_switch: Option<u8>,
    /// The `wl_compositor` and `wl_subcompositor` state.
    pub compositor_state: CompositorState,
    /// The `wl_shm` state and its advertised pixel formats.
    pub shm_state: ShmState,
    /// The `zwp_linux_dmabuf_v1` state (Bloque 7a).
    ///
    /// The global is created with a default feedback carrying the render node
    /// of the device the winit renderer uses and that renderer's dmabuf
    /// formats, so GPU clients (Mesa EGL/Vulkan) can allocate buffers the
    /// compositor can texture from.
    pub dmabuf_state: DmabufState,
    /// Dmabufs a client committed, waiting for the session to import them.
    ///
    /// `DmabufHandler` cannot reach the renderer to test the import — ADR 0012
    /// keeps every backend type out of [`State`] — so the handler queues the
    /// buffer here and the session performs the import right after dispatching
    /// clients (see `session::dispatch_clients`). A buffer that cannot be
    /// imported is rejected there, never at render time.
    pub pending_dmabuf_imports: Vec<PendingDmabufImport>,
    /// The `wp_linux_drm_syncobj_manager_v1` state (Bloque 7b).
    ///
    /// `Some` only when the render node supports `syncobj_eventfd`; without
    /// that the global is not announced and clients fall back to the implicit
    /// sync blocker.
    pub syncobj_state: Option<DrmSyncobjState>,
    /// Inserts the commit-blocker source into the backend's loop (Bloque 7b).
    pub(crate) blocker_sink: Box<dyn crate::blockers::BlockerSink>,
    /// The factory for `wl_seat` globals.
    pub seat_state: SeatState<State>,
    /// The one seat Yuki offers.
    pub seat: Seat<State>,
    /// The cursor image the pointer should draw.
    ///
    /// Smithay updates this through `SeatHandler::cursor_image`, and only for a
    /// `wl_pointer.set_cursor` whose serial matches the pointer's last enter
    /// serial — the one generated from the single serial counter in
    /// `input.rs`. `Named` means no client cursor is active, so the
    /// compositor's own default arrow is drawn; `Hidden` means the client
    /// cleared the cursor and nothing is drawn.
    pub cursor_status: CursorImageStatus,
    /// The `xdg_wm_base` state.
    pub xdg_shell_state: XdgShellState,
    /// The `zxdg_decoration_manager_v1` state.
    ///
    /// The per-window mode lives in the scene; this only serves the protocol
    /// (ADR 0019).
    pub decoration_state: XdgDecorationState,
    /// The `org_kde_kwin_server_decoration_manager` state.
    ///
    /// GTK3 and GTK4 negotiate decorations only through this protocol; the
    /// advertised default mode is server-side so a plain GTK window accepts
    /// Yuki's chrome (ADR 0019).
    pub kde_decoration_state: KdeDecorationState,
    /// The `zwlr_layer_shell_v1` state (ADR 0023).
    ///
    /// Layer-shell is what the shell draws its surfaces on: the wallpaper lives
    /// on the `Background` layer, the panels on `Top`/`Overlay`.
    pub layer_shell_state: WlrLayerShellState,
    /// The `wp_viewporter` state: lets a client render at one resolution and
    /// display at another. The shell renders its buffers at the fractional
    /// scale and uses a viewport to land on the logical size.
    pub viewporter_state: ViewporterState,
    /// The `wp_fractional_scale_manager_v1` state (HiDPI fractional scales).
    pub fractional_scale_state: FractionalScaleManagerState,
    /// The placed layer surfaces.
    pub layers: Layers,
    /// The `wl_data_device_manager` state.
    ///
    /// Advertising the global completes the seat's protocol surface; the
    /// clipboard behaviour follows the seat's keyboard focus through
    /// `set_data_device_focus` (see `input.rs`). Clients that assume a
    /// conformant seat (foot among them) tear down without it.
    pub data_device_state: DataDeviceState,
    /// The `zwp_primary_selection_device_manager_v1` state.
    ///
    /// The X11-style primary selection: selecting text makes it available
    /// without an explicit copy. Like the clipboard it follows the keyboard
    /// focus through `set_primary_focus` (see `input.rs`).
    pub primary_selection_state: PrimarySelectionState,
    /// The `ext-session-lock-v1` state.
    ///
    /// The lock client binds the manager and asks to lock the session; while
    /// locked, Yuki renders only the lock surfaces (see `lock.rs` and ADR 0023).
    pub lock_state: SessionLockManagerState,
    /// Whether the session is locked: only the lock surfaces are rendered and
    /// fed input.
    pub locked: bool,
    /// The lock surfaces the lock client created, one per output.
    pub lock_surfaces: Vec<LockSurface>,
    /// The idle clock, `ext-idle-notify-v1` clients and the auto-lock timer.
    pub idle: crate::idle::IdleState,
    /// Surfaces holding `zwp_idle_inhibitor_v1`.
    pub idle_inhibitors: Vec<WlSurface>,
    /// Failed lock-recovery attempts since the current lock client was seen.
    pub(crate) lock_recovery_attempts: u32,
    /// Whether a lock-recovery timer is already scheduled.
    pub(crate) lock_recovery_pending: bool,
    /// The Wayland display handle.
    ///
    /// Held as a *handle*, never as a borrow of the [`Display`], so the module
    /// stays free of the display for dispatch (ADR 0013). Server-side helpers
    /// that need the display — clipboard and primary-selection focus tracking —
    /// reach it here.
    pub display_handle: DisplayHandle,
    /// The `wl_output` and `zxdg_output_manager_v1` state.
    pub output_manager_state: OutputManagerState,
    /// The one output, backed by the winit window.
    pub output: Output,
    /// The output's current size, in physical pixels, mirroring its current
    /// mode. New toplevels are sized from it; the session updates it on resize.
    pub output_size: Size<i32, Physical>,
    /// The tracked toplevels. All window geometry goes through this; the Direct
    /// Session milestone replaces it with `yuki-scene` (see [`crate::window`]).
    pub windows: Windows,
    /// The tracked popups (Bloque 5): positioned, configured and dismissed here.
    pub popups: Popups,
    /// The window list and control protocols for the dock (ADR 0023).
    pub foreign: ForeignState,
    /// The `ext-foreign-toplevel-list` handles, by window.
    pub list_handles: ListHandles,
    /// Windows minimized to the dock (kept out of the scene).
    pub minimized: std::collections::HashSet<WindowId>,
    /// The last dock icon rectangle (`set_rectangle`), for the minimize anim.
    pub dock_rectangle: Option<(WindowId, Rectangle<i32, Logical>)>,
    /// The runtime configuration, loaded from the per-domain files in
    /// `~/.config/yuki/` (ADR 0021), one TOML per section like macOS.
    pub config: Config,
    /// An interactive move or resize, if one is in progress.
    pub(crate) grab: Option<Grab>,
    /// Test-only: the press the scripted input is holding, if any.
    ///
    /// Only when `YUKI_TEST_INPUT` is set. See [`TestPress`].
    pub(crate) test_press: Option<TestPress>,
    /// Test-only: the window left behind by `YUKI_TEST_LEAK_WINDOW`.
    pub(crate) leaked_window: Option<WindowId>,
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
    ///
    /// `dmabuf_feedback` is the default feedback the session built from the
    /// renderer's device and formats; the session fails startup if it could not
    /// derive a render node, so there is exactly one path: the linux-dmabuf
    /// global is always created with feedback (version 5), never the v3
    /// fallback. The feedback comes from the winit renderer, which `State`
    /// never sees (ADR 0012).
    ///
    /// `syncobj_device` is the opened render node, passed only when it supports
    /// `syncobj_eventfd` (the session tests it); `State` announces the
    /// `wp_linux_drm_syncobj_manager_v1` global then. `loop_handle` is the
    /// calloop handle whose data type is the session, used by the commit
    /// blockers (Bloque 7b).
    pub(crate) fn new(
        loop_signal: LoopSignal,
        display_handle: &DisplayHandle,
        size: Size<i32, Physical>,
        dmabuf_feedback: DmabufFeedback,
        syncobj_device: Option<DrmDeviceFd>,
        blocker_sink: Box<dyn crate::blockers::BlockerSink>,
        idle_sink: Box<dyn crate::idle::TimerSink>,
    ) -> Result<Self, StateError> {
        let output = winit_output(size);
        Self::new_with_output(
            loop_signal,
            display_handle,
            size,
            output,
            dmabuf_feedback,
            syncobj_device,
            blocker_sink,
            idle_sink,
        )
    }

    /// Like [`Self::new`], but with the output the backend already created.
    ///
    /// The DRM backend publishes the connector's own `wl_output`; winit uses
    /// [`winit_output`]. The global is created here so every backend follows
    /// the same path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_output(
        loop_signal: LoopSignal,
        display_handle: &DisplayHandle,
        size: Size<i32, Physical>,
        output: Output,
        dmabuf_feedback: DmabufFeedback,
        syncobj_device: Option<DrmDeviceFd>,
        blocker_sink: Box<dyn crate::blockers::BlockerSink>,
        idle_sink: Box<dyn crate::idle::TimerSink>,
    ) -> Result<Self, StateError> {
        let config = Config::load();
        let idle_timeout = match config.lock.idle_timeout_seconds {
            0 => None,
            seconds => Some(std::time::Duration::from_secs(seconds)),
        };
        let idle = crate::idle::IdleState::new(display_handle, idle_sink, idle_timeout);
        let compositor_state = CompositorState::new::<Self>(display_handle);
        let shm_state = ShmState::new::<Self>(
            display_handle,
            [wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888],
        );
        let mut dmabuf_state = DmabufState::new();
        // One path: version 5 with the default feedback (Bloque 7a). The main
        // device and formats come from the renderer; there is no v3 fallback.
        dmabuf_state.create_global_with_default_feedback::<Self>(display_handle, &dmabuf_feedback);
        // Explicit sync only when the device supports syncobj eventfds; the
        // global is announced exactly then (Bloque 7b).
        let syncobj_state =
            syncobj_device.map(|device| DrmSyncobjState::new::<Self>(display_handle, device));
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(display_handle);
        let decoration_state = XdgDecorationState::new::<Self>(display_handle);
        let kde_decoration_state =
            KdeDecorationState::new::<Self>(display_handle, KdeDefaultMode::Server);
        let layer_shell_state = WlrLayerShellState::new::<Self>(display_handle);
        let viewporter_state = ViewporterState::new::<Self>(display_handle);
        let fractional_scale_state = FractionalScaleManagerState::new::<Self>(display_handle);
        let data_device_state = DataDeviceState::new::<Self>(display_handle);
        let primary_selection_state = PrimarySelectionState::new::<Self>(display_handle);
        // The manager global is advertised to every client for now; restricting
        // it to a client the compositor itself launched is compositor policy
        // that the protocol allows (the lock client is that privileged client).
        let lock_state = SessionLockManagerState::new::<Self, _>(display_handle, |_| true);
        let _idle_inhibit_state =
            smithay::wayland::idle_inhibit::IdleInhibitManagerState::new::<Self>(display_handle);

        let mut seat_state: SeatState<Self> = SeatState::new();
        let mut seat = seat_state.new_wl_seat(display_handle, "yuki");
        // A seat with no keymap is worse than no seat: clients that accepted
        // the capability then get no keymap and misbehave. The keyboard must be
        // added before the seat is useful, so its error is fatal here. The
        // layout comes from the system (or XKB_DEFAULT_*), never a hardcoded
        // "us".
        let keymap = crate::keymap::system();
        seat.add_keyboard(keymap.xkb_config(), 200, 25)?;
        let _ = seat.add_pointer();

        let _ = output.create_global::<Self>(display_handle);

        Ok(Self {
            loop_signal,
            needs_redraw: false,
            pending_vt_switch: None,
            compositor_state,
            shm_state,
            dmabuf_state,
            pending_dmabuf_imports: Vec::new(),
            syncobj_state,
            blocker_sink,
            seat_state,
            seat,
            cursor_status: CursorImageStatus::default_named(),
            xdg_shell_state,
            decoration_state,
            kde_decoration_state,
            layer_shell_state,
            viewporter_state,
            fractional_scale_state,
            layers: Layers::new(),
            data_device_state,
            primary_selection_state,
            lock_state,
            locked: false,
            lock_surfaces: Vec::new(),
            idle,
            idle_inhibitors: Vec::new(),
            lock_recovery_attempts: 0,
            lock_recovery_pending: false,
            display_handle: display_handle.clone(),
            output_manager_state,
            output,
            output_size: size,
            windows: Windows::new(),
            popups: Popups::new(),
            foreign: ForeignState::new(display_handle),
            list_handles: Vec::new(),
            minimized: std::collections::HashSet::new(),
            dock_rectangle: None,
            config,
            grab: None,
            test_press: None,
            leaked_window: None,
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

    /// Asks the DRM backend to switch to virtual terminal `vt`.
    pub(crate) fn request_vt_switch(&mut self, vt: u8) {
        self.pending_vt_switch = Some(vt);
    }

    /// Takes the VT switch requested by the keyboard, if any.
    pub(crate) fn take_vt_switch(&mut self) -> Option<u8> {
        self.pending_vt_switch.take()
    }

    /// Input activity: re-arms the idle clock and the auto-lock timer.
    pub(crate) fn note_idle_activity(&mut self) {
        self.idle.note_activity();
    }

    /// A timer from the backend fired. `0` is auto-lock, the lock-recovery
    /// sentinel is handled here, and the rest are client idle notifications.
    pub(crate) fn idle_timer(&mut self, id: u64) {
        if id == crate::idle::LOCK_RECOVERY_ID {
            self.lock_recovery_timer();
        } else {
            self.idle.idle_timer(id);
        }
    }

    /// Records the compositor's own Wayland socket for the auto-lock child.
    pub(crate) fn set_wayland_display(&mut self, display: String) {
        self.idle.set_wayland_display(display);
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

    /// The workspace for windows: the output minus the layer-shell exclusive
    /// zones (the menu bar on top, the future dock on the bottom).
    pub fn workspace(&self) -> (Point<i32, Logical>, Size<i32, Logical>) {
        let full = self.logical_output_size();
        let top = self.layers.exclusive_top();
        let bottom = self.layers.exclusive_bottom();
        (
            Point::from((0, top)),
            Size::from((full.w, (full.h - top - bottom).max(0))),
        )
    }
}

/// An interactive move or resize in progress.
///
/// A grab belongs to one window and ends on pointer release or when that window
/// is destroyed. It is not a Smithay `PointerGrab`: the compositor already sees
/// every motion event, so it simply stops forwarding them while a grab is set.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Grab {
    /// Dragging a window by the pointer.
    Move {
        /// The window being dragged.
        window: WindowId,
        /// The pointer's position relative to the window's top-left, so the
        /// window does not jump under the pointer.
        offset: ScenePoint<f64, SceneLogical>,
    },
    /// Dragging an edge or corner to resize.
    Resize {
        /// The window being resized.
        window: WindowId,
        /// The edge or corner being dragged.
        edge: Edge,
        /// Where the pointer was when the resize began.
        start_pointer: ScenePoint<f64, SceneLogical>,
        /// The window's outer rectangle when the resize began.
        start_outer: SceneRect<f64, SceneLogical>,
    },
}

impl Grab {
    /// The window this grab acts on.
    pub(crate) fn window(self) -> WindowId {
        match self {
            Grab::Move { window, .. } | Grab::Resize { window, .. } => window,
        }
    }
}

/// Test-only: a pointer press the scripted input is holding.
///
/// Only when `YUKI_TEST_INPUT` is set. The scripted input presses once, on
/// the first window that maps, and holds the button so a client's
/// `xdg_toplevel.move`/`resize` can be validated against a real press. The
/// window and location are remembered so the follow-up drag is aimed at the
/// same window and the release ends the same press.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TestPress {
    /// The window the press landed on.
    pub(crate) window: WindowId,
    /// The pointer's location at the last scripted motion, in logical output
    /// coordinates.
    pub(crate) location: Point<f64, Logical>,
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

/// A dmabuf a client committed, waiting for the session to import it.
///
/// `DmabufHandler` cannot reach the GLES renderer (ADR 0012), so it queues the
/// buffer here and the session drains the queue after dispatching clients and
/// performs the import. The notifier's verdict — `successful` or `failed` — is
/// what answers the client's `create`/`create_immed` request, so a buffer that
/// cannot be imported is rejected as soon as it is committed, not at render time.
pub struct PendingDmabufImport {
    /// The buffer to test-import into the renderer.
    pub dmabuf: Dmabuf,
    /// The notifier that answers the client's import request.
    pub notifier: ImportNotifier,
}

/// The nested backend's single output: a 0×0 winit panel at the given size.
fn winit_output(size: Size<i32, Physical>) -> Output {
    let output = Output::new(
        "yuki".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Yuki".to_string(),
            model: "Winit".to_string(),
            serial_number: String::new(),
        },
    );
    let mode = Mode {
        size,
        refresh: 60_000,
    };
    // The advertised transform stays `Normal` on purpose. The winit
    // framebuffer's bottom-left origin is corrected at render time by the
    // damage tracker's mode source, never by telling clients to pre-rotate:
    // this transform reaches clients in `wl_output.geometry`
    // (`smithay-0.7.0/src/wayland/output/mod.rs:164-177`). See
    // `session::winit_render_transform`.
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Integer(1)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    output
}
