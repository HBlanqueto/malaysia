use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use malaysia_scene::geometry::{Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect};
use malaysia_scene::scene::{Edge, WindowId};
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::renderer::gles::{GlesPixelProgram, GlesTexProgram};
use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use smithay::input::{Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::LoopSignal;
use smithay::reexports::calloop::ping::Ping;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::backend::renderer::element::solid::SolidColorBuffer;
use smithay::utils::{Clock, Logical, Monotonic, Physical, Point, Rectangle, Serial, Size, Transform};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::{DmabufFeedback, DmabufState, ImportNotifier};
use smithay::wayland::drm_syncobj::DrmSyncobjState;
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::image_capture_source::{
    OutputCaptureSourceState, ToplevelCaptureSourceState,
};
use smithay::wayland::image_copy_capture::{ImageCopyCaptureHandler, ImageCopyCaptureState, Session};
use smithay::wayland::pointer_constraints::PointerConstraintsState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::{DataDeviceState, set_data_device_selection};
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::session_lock::{LockSurface, SessionLockManagerState};
use smithay::reexports::wayland_protocols::ext::session_lock::v1::server::ext_session_lock_v1::ExtSessionLockV1;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDefaultMode;
use smithay::wayland::shell::kde::decoration::KdeDecorationState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;

use crate::appmenu::AppMenuState;
use crate::config::Config;
use crate::foreign::{ForeignState, ListHandles};
use crate::gtk::GtkShellState;
use crate::infinity::protocol::infinity_window_manager_v1::InfinityWindowManagerV1;
use crate::layers::Layers;
use crate::popup::Popups;
use crate::shake::ShakeDetector;
use crate::window::Windows;

const CURSOR_BOOST_ID: u64 = u64::MAX - 1;
const ACTIVATION_SERIAL_MAX_AGE: Duration = Duration::from_secs(30);
const CURSOR_SHAKE_ACTIVE_WINDOW: Duration = Duration::from_millis(150);

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("failed to set up the seat keyboard")]
    Keyboard(#[from] smithay::input::keyboard::Error),
}

pub struct State {
    loop_signal: LoopSignal,
    needs_redraw: bool,
    pub(crate) frame_callback_cycle: u32,
    pending_vt_switch: Option<u8>,
    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub dmabuf_state: DmabufState,
    pub pending_dmabuf_imports: Vec<PendingDmabufImport>,
    pub(crate) security_listeners: Vec<smithay::wayland::security_context::SecurityContextListenerSource>,
    pub syncobj_state: Option<DrmSyncobjState>,
    pub(crate) blocker_sink: Box<dyn crate::blockers::BlockerSink>,
    pub seat_state: SeatState<State>,
    pub seat: Seat<State>,
    pub cursor_status: CursorImageStatus,
    pub(crate) client_cursor: CursorImageStatus,
    pub activation_state: XdgActivationState,
    pub(crate) output_capture_source_state: OutputCaptureSourceState,
    pub(crate) toplevel_capture_source_state: ToplevelCaptureSourceState,
    pub(crate) image_copy_capture_state: ImageCopyCaptureState,
    pub(crate) capture_sessions: Vec<Session>,
    pub(crate) pending_captures: Vec<crate::capture::PendingCapture>,
    pub(crate) capture_dma: Option<crate::capture::CaptureDma>,
    capture_dma_checked: bool,
    pub(crate) last_activation_serial: Option<(Serial, Instant)>,
    pub(crate) launcher_child: Option<std::process::Child>,
    pub(crate) force_quit_child: Option<std::process::Child>,
    pub(crate) cursor_boost_started: Option<Instant>,
    pub(crate) shake: ShakeDetector,
    pub xdg_shell_state: XdgShellState,
    pub kde_decoration_state: KdeDecorationState,
    pub layer_shell_state: WlrLayerShellState,
    pub layers: Layers,
    pub edge_swipe: crate::notification_panel::EdgeSwipe,
    pub layer_focus: crate::layers::LayerFocus<WlSurface>,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub lock_state: SessionLockManagerState,
    pub locked: bool,
    pub(crate) keymap: crate::keymap::Keymap,
    pub(crate) virtual_keymap: Option<(
        smithay::wayland::virtual_keyboard::VirtualKeyboardDevice,
        smithay::input::keyboard::KeymapFileId,
    )>,
    pub(crate) ime_popups: Vec<smithay::wayland::input_method::PopupSurface>,
    pub(crate) activity_noted: bool,
    pub(crate) clock: Clock<Monotonic>,
    pub(crate) lock_owner: Option<ExtSessionLockV1>,
    pub lock_surfaces: Vec<LockSurface>,
    pub idle: crate::idle::IdleState,
    pub idle_inhibitors: Vec<WlSurface>,
    pub(crate) lock_recovery_attempts: u32,
    pub(crate) lock_recovery_pending: bool,
    pub(crate) lock_transition: crate::lock_transition::LockTransition,
    pub(crate) session_notices: Vec<
        infinity_protocol::server::session::infinity_session_v1::infinity_session_v1::InfinitySessionV1,
    >,
    pub display_handle: DisplayHandle,
    pub output: Output,
    pub output_size: Size<i32, Physical>,
    pub scale: Scale,
    pub windows: Windows,
    pub(crate) active_window: ActiveWindowState,
    pub popups: Popups,
    pub foreign: ForeignState,
    pub(crate) appmenu: AppMenuState,
    pub(crate) gtk: GtkShellState,
    pub list_handles: ListHandles,
    pub minimized: std::collections::HashSet<WindowId>,
    pub(crate) spaces: malaysia_scene::spaces::Spaces,
    pub(crate) space_slide: Option<crate::spaces::SpaceSlide>,
    pub(crate) focus_transition: Option<crate::spaces::FocusTransition>,
    pub(crate) focus_reveal: crate::spaces::FocusReveal,
    pub(crate) last_space_state: Option<
        infinity_protocol::server::infinity_window_manager_v1::infinity_window_manager_v1::FocusBar,
    >,
    pub(crate) maximized: HashMap<WindowId, SceneRect<f64, SceneLogical>>,
    pub(crate) arranged: HashMap<WindowId, SceneRect<f64, SceneLogical>>,
    pub(crate) fullscreen: HashMap<WindowId, SceneRect<f64, SceneLogical>>,
    pub(crate) focus_desk: HashMap<WindowId, Rectangle<i32, Logical>>,
    pub(crate) infinity_managers: Vec<InfinityWindowManagerV1>,
    pub(crate) wallpaper: ctappearance::WallpaperState,
    pub(crate) focus_notices: Vec<
        infinity_protocol::server::focus::infinity_focus_v1::infinity_focus_surface_v1::InfinityFocusSurfaceV1,
    >,
    pub config: Config,
    pub(crate) grab: Option<Grab>,
    pub(crate) alert_press: Option<(u64, Point<f64, Logical>)>,
    pub(crate) test_press: Option<TestPress>,
    pub(crate) test_pointer: Option<crate::test_input::TestPointer>,
    pub(crate) chrome: crate::chrome::ChromeCache,
    pub(crate) focus_backdrop: SolidColorBuffer,
    pub(crate) revealed_titlebar_backdrop: SolidColorBuffer,
    pub(crate) chrome_theme: crate::chrome::ChromeTheme,
    pub(crate) theme_fade: Option<crate::theme_fade::ThemeFade>,
    pub(crate) popup_fade_request: Option<WindowId>,
    pub(crate) popup_fades: Vec<crate::popup_fade::PopupFade>,
    pub(crate) rounded_clip: Option<GlesTexProgram>,
    pub(crate) shadow: Option<GlesPixelProgram>,
    pub(crate) border: Option<GlesPixelProgram>,
    pub(crate) display_filter: Option<GlesTexProgram>,
    pub(crate) blur: Option<crate::blur::Blur>,
    pub(crate) chrome_pointer: Option<(WindowId, ctappearance::traffic::TrafficPointer)>,
    pub(crate) titlebar_click: Option<crate::input::TitlebarClick>,
    pub(crate) client_click: Option<crate::input::TitlebarClick>,
    pub(crate) client_double_click: Option<(WindowId, std::time::Instant)>,
    pub(crate) screenshot: Option<ScreenshotRequest>,
    pub(crate) screenshot_mode: Option<ScreenshotMode>,
}

#[derive(Debug, Default)]
pub(crate) struct ActiveWindowState {
    id: Option<WindowId>,
}

impl ActiveWindowState {
    pub(crate) fn get(&self) -> Option<WindowId> {
        self.id
    }

    pub(crate) fn set(&mut self, id: Option<WindowId>) -> bool {
        if self.id == id {
            return false;
        }
        self.id = id;
        true
    }

    pub(crate) fn clear(&mut self) -> Option<WindowId> {
        self.id.take()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ScreenshotRequest {
    Focused,
    Output,
    Window(WindowId),
    Region(Rectangle<i32, Logical>),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ScreenshotMode {
    Region {
        start: Option<Point<f64, Logical>>,
        current: Point<f64, Logical>,
    },
    Window {
        under: Option<WindowId>,
    },
}

impl State {
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
        #[cfg(not(test))]
        crate::spawn::export_session_env(&config);
        let chrome_theme = crate::chrome::ChromeTheme::from_config(&config);
        let scale = output_scale(config.display.scale);
        output.change_current_state(None, None, Some(scale), None);
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
        dmabuf_state.create_global_with_default_feedback::<Self>(display_handle, &dmabuf_feedback);
        let syncobj_state =
            syncobj_device.map(|device| DrmSyncobjState::new::<Self>(display_handle, device));
        OutputManagerState::new_with_xdg_output::<Self>(display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(display_handle);
        XdgDecorationState::new::<Self>(display_handle);
        smithay::wayland::shell::xdg::dialog::XdgDialogState::new::<Self>(display_handle);
        let kde_decoration_state =
            KdeDecorationState::new::<Self>(display_handle, KdeDefaultMode::Server);
        let layer_shell_state = WlrLayerShellState::new_with_filter::<Self, _>(
            display_handle,
            crate::security::unrestricted,
        );
        ViewporterState::new::<Self>(display_handle);
        FractionalScaleManagerState::new::<Self>(display_handle);
        PresentationState::new::<Self>(display_handle, 1);
        let activation_state = XdgActivationState::new::<Self>(display_handle);
        CursorShapeManagerState::new::<Self>(display_handle);
        PointerConstraintsState::new::<Self>(display_handle);
        RelativePointerManagerState::new::<Self>(display_handle);
        smithay::wayland::text_input::TextInputManagerState::new::<Self>(display_handle);
        smithay::wayland::input_method::InputMethodManagerState::new::<Self, _>(
            display_handle,
            crate::security::unrestricted,
        );
        smithay::wayland::security_context::SecurityContextState::new::<Self, _>(
            display_handle,
            crate::security::unrestricted,
        );
        let output_capture_source_state = OutputCaptureSourceState::new_with_filter::<Self, _>(
            display_handle,
            crate::security::unrestricted,
        );
        let toplevel_capture_source_state = ToplevelCaptureSourceState::new_with_filter::<Self, _>(
            display_handle,
            crate::security::unrestricted,
        );
        let image_copy_capture_state = ImageCopyCaptureState::new_with_filter::<Self, _>(
            display_handle,
            crate::security::unrestricted,
        );
        let data_device_state = DataDeviceState::new::<Self>(display_handle);
        let primary_selection_state = PrimarySelectionState::new::<Self>(display_handle);
        let lock_state =
            SessionLockManagerState::new::<Self, _>(display_handle, crate::security::unrestricted);
        let _idle_inhibit_state =
            smithay::wayland::idle_inhibit::IdleInhibitManagerState::new::<Self>(display_handle);

        let mut seat_state: SeatState<Self> = SeatState::new();
        let mut seat = seat_state.new_wl_seat(display_handle, "malaysia-compositor");
        let keymap = crate::keymap::system();
        seat.add_keyboard(keymap.xkb_config(), 200, 25)?;
        smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState::new::<Self, _>(
            display_handle,
            crate::security::unrestricted,
        );
        smithay::wayland::virtual_pointer::VirtualPointerManagerState::new::<Self, _>(
            display_handle,
            crate::security::unrestricted,
        );
        let _ = seat.add_pointer();

        let _ = output.create_global::<Self>(display_handle);

        crate::infinity::InfinityGlobalData::create_global(display_handle);
        smithay::wayland::background_effect::BackgroundEffectState::new::<Self>(display_handle);
        crate::focus_notice::FocusNoticeGlobalData::create_global(display_handle);
        crate::session_notice::SessionNoticeGlobalData::create_global(display_handle);
        crate::window_style::WindowStyleGlobalData::create_global(display_handle);
        Ok(Self {
            loop_signal,
            needs_redraw: false,
            frame_callback_cycle: 0,
            pending_vt_switch: None,
            compositor_state,
            shm_state,
            dmabuf_state,
            pending_dmabuf_imports: Vec::new(),
            security_listeners: Vec::new(),
            syncobj_state,
            blocker_sink,
            seat_state,
            seat,
            cursor_status: CursorImageStatus::default_named(),
            client_cursor: CursorImageStatus::default_named(),
            activation_state,
            output_capture_source_state,
            toplevel_capture_source_state,
            image_copy_capture_state,
            capture_sessions: Vec::new(),
            pending_captures: Vec::new(),
            capture_dma: None,
            capture_dma_checked: false,
            last_activation_serial: None,
            launcher_child: None,
            force_quit_child: None,
            cursor_boost_started: None,
            shake: ShakeDetector::default(),
            xdg_shell_state,
            kde_decoration_state,
            layer_shell_state,
            layers: Layers::new(),
            edge_swipe: Default::default(),
            layer_focus: crate::layers::LayerFocus::default(),
            data_device_state,
            primary_selection_state,
            lock_state,
            locked: false,
            keymap,
            virtual_keymap: None,
            ime_popups: Vec::new(),
            activity_noted: false,
            clock: Clock::new(),
            lock_owner: None,
            lock_surfaces: Vec::new(),
            idle,
            idle_inhibitors: Vec::new(),
            lock_recovery_attempts: 0,
            lock_recovery_pending: false,
            lock_transition: crate::lock_transition::LockTransition::default(),
            session_notices: Vec::new(),
            display_handle: display_handle.clone(),
            output,
            output_size: size,
            scale,
            windows: Windows::new(),
            active_window: ActiveWindowState::default(),
            popups: Popups::new(),
            foreign: ForeignState::new(display_handle),
            appmenu: AppMenuState::new(display_handle),
            gtk: GtkShellState::new(display_handle),
            list_handles: Vec::new(),
            wallpaper: ctappearance::load_wallpaper_state(),
            focus_notices: Vec::new(),
            minimized: std::collections::HashSet::new(),
            spaces: malaysia_scene::spaces::Spaces::new(),
            space_slide: None,
            focus_transition: None,
            focus_reveal: crate::spaces::FocusReveal::default(),
            last_space_state: None,
            maximized: HashMap::new(),
            arranged: HashMap::new(),
            fullscreen: HashMap::new(),
            focus_desk: HashMap::new(),
            infinity_managers: Vec::new(),
            config,
            grab: None,
            alert_press: None,
            test_press: None,
            test_pointer: crate::test_input::TestPointer::from_env(),
            chrome: crate::chrome::ChromeCache::new(),
            focus_backdrop: SolidColorBuffer::default(),
            revealed_titlebar_backdrop: SolidColorBuffer::default(),
            chrome_theme,
            theme_fade: None,
            popup_fade_request: None,
            popup_fades: Vec::new(),
            rounded_clip: None,
            display_filter: None,
            blur: None,
            shadow: None,
            border: None,
            chrome_pointer: None,
            titlebar_click: None,
            client_click: None,
            client_double_click: None,
            screenshot: None,
            screenshot_mode: None,
        })
    }

    pub(crate) fn refresh_theme(&mut self, restyled: bool) {
        self.config = Config::load();
        self.request_redraw();
        if restyled {
            self.wallpaper = ctappearance::load_wallpaper_state();
            self.chrome_theme = crate::chrome::ChromeTheme::from_config(&self.config);
            self.request_redraw();
        }
    }

    pub(crate) fn theme_fade_mark_committed(
        &mut self,
        surface_id: smithay::reexports::wayland_server::backend::ObjectId,
    ) {
        if let Some(fade) = self.theme_fade.as_mut() {
            fade.mark_committed(surface_id);
        }
    }

    pub fn stop(&self) {
        self.loop_signal.stop();
    }

    pub fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    pub fn take_redraw_request(&mut self) -> bool {
        std::mem::take(&mut self.needs_redraw)
    }

    pub(crate) fn animating(&self, now: Instant) -> bool {
        self.space_slide.is_some()
            || self.focus_transition.is_some()
            || !self.popup_fades.is_empty()
            || self.focus_reveal.animating(now)
            || self.theme_fade.is_some()
            || self.lock_transition.animating(now)
    }

    pub(crate) fn init_capture_dma(
        &mut self,
        renderer: &smithay::backend::renderer::gles::GlesRenderer,
    ) {
        if self.capture_dma_checked {
            return;
        }
        self.capture_dma_checked = true;
        self.capture_dma = crate::capture::dmabuf_constraints(renderer);
    }

    pub(crate) fn refresh_capture_constraints(&mut self) {
        let sessions = std::mem::take(&mut self.capture_sessions);
        for session in &sessions {
            if let Some(constraints) = self.capture_constraints(&session.source()) {
                session.update_constraints(constraints);
            }
        }
        self.capture_sessions = sessions;
    }

    pub(crate) fn request_screenshot(&mut self, request: ScreenshotRequest) {
        if self.locked {
            tracing::info!("screenshot ignored: the session is locked");
            return;
        }
        self.screenshot = Some(request);
        self.request_redraw();
    }

    pub(crate) fn take_screenshot_request(&mut self) -> Option<ScreenshotRequest> {
        self.screenshot.take()
    }

    pub(crate) fn focused_window(&self) -> Option<WindowId> {
        self.active_window.get()
    }

    pub(crate) fn apply_titlebar_double_click(&mut self, id: WindowId) {
        use ctappearance::window::{TitlebarAction, double_click_action};
        let in_focus_mode = self.in_focus_mode(id);
        let zoomable = in_focus_mode || self.windows.zoom_supported(id);
        match double_click_action(self.config.window.double_click_titlebar, zoomable) {
            TitlebarAction::Maximize if in_focus_mode => self.exit_focus_mode(id),
            TitlebarAction::Maximize => self.toggle_maximize(id),
            TitlebarAction::Minimize => self.set_minimized(id, true),
            TitlebarAction::Focus | TitlebarAction::None => {}
        }
    }

    pub(crate) fn start_region_screenshot(&mut self) {
        if self.locked {
            return;
        }
        let current = self.seat.get_pointer().map_or_else(
            || Point::from((0.0, 0.0)),
            |pointer| pointer.current_location(),
        );
        self.screenshot_mode = Some(ScreenshotMode::Region {
            start: None,
            current,
        });
        self.cursor_status = CursorImageStatus::Named(CursorIcon::Crosshair);
        self.request_redraw();
    }

    pub(crate) fn start_window_screenshot(&mut self) {
        if self.locked || !matches!(self.screenshot_mode, Some(ScreenshotMode::Region { .. })) {
            return;
        }
        self.screenshot_mode = Some(ScreenshotMode::Window { under: None });
        self.cursor_status = CursorImageStatus::Named(CursorIcon::Default);
        self.request_redraw();
    }

    pub(crate) fn cancel_screenshot(&mut self) {
        if self.screenshot_mode.take().is_some() {
            self.cursor_status = CursorImageStatus::Named(CursorIcon::Default);
            self.request_redraw();
        }
    }

    pub(crate) fn screenshot_mode(&self) -> Option<&ScreenshotMode> {
        self.screenshot_mode.as_ref()
    }

    pub(crate) fn screenshot_pointer_motion(&mut self, location: Point<f64, Logical>) -> bool {
        match self.screenshot_mode.as_mut() {
            Some(ScreenshotMode::Region { current, .. }) => *current = location,
            Some(ScreenshotMode::Window { under }) => {
                *under = self.windows.window_under(location, &self.minimized);
            }
            None => return false,
        }
        self.request_redraw();
        true
    }

    pub(crate) fn screenshot_pointer_press(&mut self, location: Point<f64, Logical>) -> bool {
        let pick = match self.screenshot_mode.as_mut() {
            Some(ScreenshotMode::Region { start, current }) => {
                *start = Some(location);
                *current = location;
                None
            }
            Some(ScreenshotMode::Window { under }) => under.take(),
            None => return false,
        };
        if let Some(id) = pick {
            self.finish_screenshot(ScreenshotRequest::Window(id));
        }
        true
    }

    pub(crate) fn screenshot_pointer_release(&mut self, location: Point<f64, Logical>) -> bool {
        let rect = match self.screenshot_mode.as_mut() {
            Some(ScreenshotMode::Region {
                start: Some(start),
                current,
            }) => {
                *current = location;
                Some(region_rect(*start, *current, self.logical_output_size()))
            }
            Some(_) => None,
            None => return false,
        };
        if let Some(rect) = rect.filter(|rect| rect.size.w > 0 && rect.size.h > 0) {
            self.finish_screenshot(ScreenshotRequest::Region(rect));
        }
        true
    }

    fn finish_screenshot(&mut self, request: ScreenshotRequest) {
        if self.locked {
            return;
        }
        self.screenshot = Some(request);
        self.screenshot_mode = None;
        self.cursor_status = CursorImageStatus::Named(CursorIcon::Default);
        self.request_redraw();
    }

    pub(crate) fn request_vt_switch(&mut self, vt: u8) {
        self.pending_vt_switch = Some(vt);
    }

    pub(crate) fn take_vt_switch(&mut self) -> Option<u8> {
        self.pending_vt_switch.take()
    }

    pub(crate) fn note_idle_activity(&mut self) {
        if !std::mem::replace(&mut self.activity_noted, true) {
            self.idle.note_activity();
        }
    }

    pub(crate) fn idle_timer(&mut self, id: u64) {
        if id == CURSOR_BOOST_ID {
            self.cursor_boost_tick();
        } else if id == crate::idle::LOCK_RECOVERY_ID {
            self.lock_recovery_timer();
        } else {
            self.idle.idle_timer(id);
        }
    }

    pub(crate) fn note_activation_serial(&mut self, serial: Serial) {
        self.last_activation_serial = Some((serial, Instant::now()));
    }

    pub(crate) fn activation_serial_is_recent(&self, serial: Serial) -> bool {
        self.last_activation_serial
            .is_some_and(|(last, at)| last == serial && at.elapsed() <= ACTIVATION_SERIAL_MAX_AGE)
    }

    pub(crate) fn cursor_boost_scale(&self) -> f32 {
        self.cursor_boost_started
            .map_or(1.0, |started| boost_scale_at(started.elapsed()))
    }

    pub(crate) fn set_clipboard_png(&mut self, png: Vec<u8>) {
        let data: Arc<[u8]> = png.into();
        set_data_device_selection(
            &self.display_handle,
            &self.seat,
            vec!["image/png".to_string()],
            data,
        );
    }

    pub(crate) fn trigger_cursor_boost(&mut self) {
        tracing::info!("pointer enlarged: shake to locate");
        self.cursor_boost_started = Some(Instant::now());
        self.idle.arm(
            CURSOR_BOOST_ID,
            Duration::from_millis(ctdesign::CURSOR_SHAKE_TICK_MS),
        );
        self.request_redraw();
    }

    pub(crate) fn keep_cursor_boost_alive(&mut self, now: Instant) {
        let Some(started) = self.cursor_boost_started else {
            return;
        };
        let recent = self
            .shake
            .last_shake()
            .is_some_and(|last| now.duration_since(last) <= CURSOR_SHAKE_ACTIVE_WINDOW);
        let grown =
            now.duration_since(started) >= Duration::from_millis(ctdesign::CURSOR_SHAKE_GROW_MS);
        if recent && grown {
            self.cursor_boost_started =
                Some(now - Duration::from_millis(ctdesign::CURSOR_SHAKE_GROW_MS));
        }
    }

    fn cursor_boost_tick(&mut self) {
        let Some(started) = self.cursor_boost_started else {
            return;
        };
        if boost_animation_active(started.elapsed()) {
            self.idle.arm(
                CURSOR_BOOST_ID,
                Duration::from_millis(ctdesign::CURSOR_SHAKE_TICK_MS),
            );
        } else {
            self.cursor_boost_started = None;
        }
        self.request_redraw();
    }

    pub(crate) fn set_wayland_display(&mut self, display: String) {
        self.idle.set_wayland_display(display);
    }

    pub(crate) fn client_disconnected(&mut self, client_id: ClientId) {
        let display = self.display_handle.clone();
        let owned = |surface: &WlSurface| {
            display
                .get_client(surface.id())
                .is_ok_and(|client| client.id() == client_id)
        };
        let windows: Vec<WlSurface> = self
            .windows
            .surfaces()
            .filter(|s| owned(s))
            .cloned()
            .collect();
        for surface in windows {
            self.forget_window(&surface);
        }
        let popups: Vec<WlSurface> = self
            .popups
            .surfaces()
            .filter(|s| owned(s))
            .cloned()
            .collect();
        for surface in popups {
            self.popup_destroyed(&surface);
        }
        let layers: Vec<WlSurface> = self
            .layers
            .surfaces()
            .map(|layer| layer.wl_surface().clone())
            .filter(owned)
            .collect();
        for surface in layers {
            self.layer_surface_destroyed(&surface);
        }

        self.request_redraw();
    }

    pub fn logical_output_size(&self) -> Size<i32, Logical> {
        logical_output_size(self.output_size, self.scale.fractional_scale())
    }

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

fn logical_output_size(output_size: Size<i32, Physical>, scale: f64) -> Size<i32, Logical> {
    output_size.to_f64().to_logical(scale).to_i32_round()
}

pub(crate) fn region_rect(
    start: Point<f64, Logical>,
    current: Point<f64, Logical>,
    output: Size<i32, Logical>,
) -> Rectangle<i32, Logical> {
    let x0 = start.x.min(current.x).round() as i32;
    let y0 = start.y.min(current.y).round() as i32;
    let x1 = start.x.max(current.x).round() as i32;
    let y1 = start.y.max(current.y).round() as i32;
    let x0 = x0.clamp(0, output.w);
    let y0 = y0.clamp(0, output.h);
    let x1 = x1.clamp(x0, output.w);
    let y1 = y1.clamp(y0, output.h);
    Rectangle::new(Point::from((x0, y0)), Size::from((x1 - x0, y1 - y0)))
}

fn boost_scale_at(elapsed: Duration) -> f32 {
    let ms = elapsed.as_millis() as f32;
    let grow = ctdesign::CURSOR_SHAKE_GROW_MS as f32;
    let shrink = ctdesign::CURSOR_SHAKE_SHRINK_MS as f32;
    let full = ctdesign::CURSOR_SHAKE_SCALE;
    if ms < grow {
        1.0 + (full - 1.0) * ease_out_cubic(ms / grow)
    } else if ms < grow + shrink {
        full - (full - 1.0) * smoothstep((ms - grow) / shrink)
    } else {
        1.0
    }
}

fn boost_animation_active(elapsed: Duration) -> bool {
    let total = ctdesign::CURSOR_SHAKE_GROW_MS + ctdesign::CURSOR_SHAKE_SHRINK_MS;
    elapsed < Duration::from_millis(total)
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Grab {
    Move {
        window: WindowId,
        offset: ScenePoint<f64, SceneLogical>,
    },
    Resize {
        window: WindowId,
        edge: Edge,
        start_pointer: ScenePoint<f64, SceneLogical>,
        start_outer: SceneRect<f64, SceneLogical>,
    },
    Alert {
        alert: u64,
        offset: Point<f64, Logical>,
    },
}

impl Grab {
    pub(crate) fn window(self) -> Option<WindowId> {
        match self {
            Grab::Move { window, .. } | Grab::Resize { window, .. } => Some(window),
            Grab::Alert { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TestPress {
    pub(crate) window: WindowId,
    pub(crate) location: Point<f64, Logical>,
}

#[derive(Default, Clone)]
pub(crate) struct ClientDisconnectQueue {
    inner: Arc<Mutex<Vec<ClientId>>>,
}

impl ClientDisconnectQueue {
    pub(crate) fn push(&self, client_id: ClientId) {
        self.inner.lock().unwrap().push(client_id);
    }

    pub(crate) fn drain(&self) -> Vec<ClientId> {
        std::mem::take(&mut *self.inner.lock().unwrap())
    }
}

pub struct ClientState {
    pub compositor_state: CompositorClientState,
    last_input_serial: AtomicU32,
    disconnect_queue: ClientDisconnectQueue,
    disconnect_ping: Ping,
    pub trusted_shell: bool,
    pub restricted: bool,
    pub kit_client: AtomicBool,
}

impl ClientState {
    pub(crate) fn new(
        disconnect_queue: ClientDisconnectQueue,
        disconnect_ping: Ping,
        trusted_shell: bool,
    ) -> Self {
        Self {
            compositor_state: CompositorClientState::default(),
            last_input_serial: AtomicU32::new(0),
            disconnect_queue,
            disconnect_ping,
            trusted_shell,
            restricted: false,
            kit_client: AtomicBool::new(false),
        }
    }

    pub(crate) fn restricted(
        disconnect_queue: ClientDisconnectQueue,
        disconnect_ping: Ping,
    ) -> Self {
        Self {
            restricted: true,
            ..Self::new(disconnect_queue, disconnect_ping, false)
        }
    }

    pub(crate) fn note_input_serial(&self, serial: Serial) {
        self.last_input_serial
            .store(serial.into(), Ordering::Relaxed);
    }

    pub(crate) fn last_input_serial(&self) -> Option<Serial> {
        let serial = self.last_input_serial.load(Ordering::Relaxed);
        (serial != 0).then(|| Serial::from(serial))
    }
}

impl ClientData for ClientState {
    fn disconnected(&self, client_id: ClientId, _reason: DisconnectReason) {
        self.disconnect_queue.push(client_id);
        self.disconnect_ping.ping();
    }
}

pub struct PendingDmabufImport {
    pub dmabuf: Dmabuf,
    pub notifier: ImportNotifier,
}

fn winit_output(size: Size<i32, Physical>) -> Output {
    let output = Output::new(
        "malaysia-compositor".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Malaysia".to_string(),
            model: "Winit".to_string(),
            serial_number: String::new(),
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
    output
}

pub(crate) fn output_scale(config_scale: f64) -> Scale {
    match std::env::var("MALAYSIA_SCALE").ok().as_deref() {
        Some(value) => parse_scale(Some(value)),
        None => clamp_scale(config_scale),
    }
}

fn parse_scale(value: Option<&str>) -> Scale {
    value
        .and_then(|value| value.parse::<f64>().ok())
        .map_or_else(|| Scale::Integer(1), clamp_scale)
}

fn clamp_scale(scale: f64) -> Scale {
    if !(1.0..=4.0).contains(&scale) || !scale.is_finite() {
        return Scale::Integer(1);
    }
    if scale.fract() == 0.0 {
        Scale::Integer(scale as i32)
    } else {
        Scale::Custom {
            advertised_integer: scale.floor() as i32,
            fractional: scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shake_animation_grows_then_shrinks() {
        let grow = ctdesign::CURSOR_SHAKE_GROW_MS;
        let shrink = ctdesign::CURSOR_SHAKE_SHRINK_MS;
        let full = ctdesign::CURSOR_SHAKE_SCALE;

        assert_eq!(boost_scale_at(Duration::from_millis(0)), 1.0);
        assert!(boost_scale_at(Duration::from_millis(grow / 2)) > 1.0);
        assert_eq!(boost_scale_at(Duration::from_millis(grow)), full);
        assert!(boost_scale_at(Duration::from_millis(grow + shrink / 2)) < full);
        assert_eq!(boost_scale_at(Duration::from_millis(grow + shrink)), 1.0);
        assert!(!boost_animation_active(Duration::from_millis(
            grow + shrink
        )));
    }

    #[test]
    fn logical_output_size_fits_inside_the_physical_output() {
        let physical = Size::from((103, 100));
        let logical = logical_output_size(physical, 1.25);
        assert_eq!(logical, Size::from((82, 80)));
        let back_to_physical_w = (logical.w as f64 * 1.25).ceil() as i32;
        assert!(
            back_to_physical_w <= physical.w,
            "logical width must scale back to no more than the physical width"
        );
    }

    #[test]
    fn the_screenshot_region_is_normalized_and_clamped() {
        let output = Size::from((1280, 800));
        let rect = region_rect(
            Point::from((300.0, 200.0)),
            Point::from((100.0, 50.0)),
            output,
        );
        assert_eq!(rect.loc, Point::from((100, 50)));
        assert_eq!(rect.size, Size::from((200, 150)));
        let rect = region_rect(
            Point::from((1200.0, 700.0)),
            Point::from((1400.0, 900.0)),
            output,
        );
        assert_eq!(rect.loc, Point::from((1200, 700)));
        assert_eq!(rect.size, Size::from((80, 100)));
    }

    #[test]
    fn scales_are_parsed_and_clamped() {
        assert_eq!(parse_scale(None).integer_scale(), 1);
        assert_eq!(parse_scale(Some("2")).integer_scale(), 2);
        assert_eq!(parse_scale(Some("1.5")).fractional_scale(), 1.5);
        assert_eq!(parse_scale(Some("1.5")).integer_scale(), 1);
        assert_eq!(parse_scale(Some("2.5")).fractional_scale(), 2.5);
        assert_eq!(parse_scale(Some("0.5")).integer_scale(), 1);
        assert_eq!(parse_scale(Some("abc")).integer_scale(), 1);
    }

    #[test]
    fn a_bar_popup_leaves_the_active_window_alone() {
        use crate::layers::{LayerFocus, LayerUnfocus};
        use smithay::wayland::shell::wlr_layer::KeyboardInteractivity;

        let window = WindowId::new(1);
        let mut active = ActiveWindowState::default();
        assert!(active.set(Some(window)));
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        assert!(!focus.mapped(10, KeyboardInteractivity::None, active.get()));
        assert!(focus.clicked(10, KeyboardInteractivity::OnDemand, active.get()));
        assert_eq!(
            active.get(),
            Some(window),
            "opening the popup keeps Activated"
        );
        assert_eq!(focus.unmapped(&10), LayerUnfocus::Window(Some(window)));
        assert_eq!(active.get(), Some(window), "closing it keeps Activated");
    }

    #[test]
    fn the_launcher_leaves_the_active_window_alone() {
        use crate::layers::{LayerFocus, LayerUnfocus};
        use smithay::wayland::shell::wlr_layer::KeyboardInteractivity;

        let window = WindowId::new(2);
        let mut active = ActiveWindowState::default();
        active.set(Some(window));
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        assert!(focus.mapped(11, KeyboardInteractivity::Exclusive, active.get()));
        assert_eq!(active.get(), Some(window));
        assert_eq!(focus.unmapped(&11), LayerUnfocus::Window(Some(window)));
        assert_eq!(active.get(), Some(window));
    }

    #[test]
    fn the_desktop_and_the_lock_clear_the_active_window() {
        let window = WindowId::new(3);
        let mut active = ActiveWindowState::default();
        active.set(Some(window));
        assert_eq!(active.clear(), Some(window), "a desktop click clears it");
        assert_eq!(active.get(), None);
        active.set(Some(window));
        assert_eq!(active.clear(), Some(window), "locking clears it");
        assert_eq!(active.get(), None);
    }
}
