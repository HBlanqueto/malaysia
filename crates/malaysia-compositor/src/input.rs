use std::time::{Duration, Instant};

use ctdesign::TITLEBAR;
use malaysia_scene::geometry::{Point as ScenePoint, Rect as SceneRect, Size as SceneSize};
use malaysia_scene::scene::{Edge, WindowId};
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    InputTime, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent,
};
use smithay::input::keyboard::FilterResult;
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, GrabStartData, MotionEvent,
    RelativeMotionEvent,
};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER, Serial, Size};
use smithay::wayland::pointer_constraints::{PointerConstraint, with_pointer_constraint};
use smithay::wayland::selection::data_device::set_data_device_focus;
use smithay::wayland::selection::primary_selection::set_primary_focus;
use smithay::wayland::shell::wlr_layer::LayerSurface;
use xkbcommon::xkb::{self, Keycode, keysyms};

use crate::infinity::protocol::infinity_window_manager_v1::{Layout, PairLayout};
use crate::state::{Grab, ScreenshotRequest, State};
use crate::window::round;

use crate::shortcuts::{CompositorShortcut, compositor_shortcut};
pub(crate) const BUTTON_LEFT: u32 = 0x110;
pub(crate) type TitlebarClick = (WindowId, u64, Point<f64, Logical>);

pub(crate) fn parse_ms(value: Option<&str>, default: Duration) -> Duration {
    match value.and_then(|v| v.parse::<u64>().ok()) {
        Some(0) => Duration::from_millis(1),
        Some(ms) => Duration::from_millis(ms),
        None => default,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesktopClick {
    CloseMenu,
    ClearFocus,
}

fn desktop_click(menu_open: bool) -> DesktopClick {
    if menu_open {
        DesktopClick::CloseMenu
    } else {
        DesktopClick::ClearFocus
    }
}

pub(crate) fn is_escape(syms: &[xkb::Keysym]) -> bool {
    syms.iter()
        .any(|sym| u32::from(*sym) == keysyms::KEY_Escape)
}

pub(crate) fn clamp_pointer(
    location: Point<f64, Logical>,
    size: smithay::utils::Size<i32, Logical>,
) -> Point<f64, Logical> {
    const EDGE_EPSILON: f64 = 0.01;
    let max_x = (f64::from(size.w) - EDGE_EPSILON).max(0.0);
    let max_y = (f64::from(size.h) - EDGE_EPSILON).max(0.0);
    Point::new(location.x.clamp(0.0, max_x), location.y.clamp(0.0, max_y))
}

impl State {
    pub(crate) fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>)
    where
        I::Device: 'static,
    {
        self.note_idle_activity();
        match event {
            InputEvent::Keyboard { event, .. } => {
                let device = event.device();
                let physical = (&device as &dyn std::any::Any)
                    .downcast_ref::<smithay::wayland::virtual_keyboard::VirtualKeyboardDevice>()
                    .is_none();
                if physical {
                    self.restore_configured_keymap();
                }
                self.keyboard_key(event.key_code(), event.state(), event.time());
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let location = event
                    .position_transformed(self.logical_output_size())
                    .to_f64();
                if self.screenshot_mode().is_some() {
                    if let Some(pointer) = self.seat.get_pointer() {
                        pointer.set_location(location);
                    }
                    self.screenshot_pointer_motion(location);
                    return;
                }
                self.pointer_motion(location, event.time());
            }
            InputEvent::PointerMotion { event, .. } => {
                let Some(pointer) = self.seat.get_pointer() else {
                    return;
                };
                let location = clamp_pointer(
                    pointer.current_location() + event.delta(),
                    self.logical_output_size(),
                );
                if self.screenshot_mode().is_some() {
                    if let Some(pointer) = self.seat.get_pointer() {
                        pointer.set_location(location);
                    }
                    self.screenshot_pointer_motion(location);
                    return;
                }
                self.pointer_motion_raw(
                    location,
                    event.time(),
                    Some((event.delta(), event.delta_unaccel())),
                );
            }
            InputEvent::PointerButton { event, .. } => {
                if self.screenshot_mode().is_some() {
                    let location = self.seat.get_pointer().map_or_else(
                        || Point::from((0.0, 0.0)),
                        |pointer| pointer.current_location(),
                    );
                    if event.state() == ButtonState::Pressed {
                        self.screenshot_pointer_press(location);
                    } else {
                        self.screenshot_pointer_release(location);
                    }
                    return;
                }
                let serial = SERIAL_COUNTER.next_serial();
                let button = event.button_code();
                let state = event.state();
                if state == ButtonState::Pressed {
                    self.note_activation_serial(serial);
                }

                if state == ButtonState::Pressed {
                    if let Some(location) = self
                        .seat
                        .get_pointer()
                        .map(|pointer| pointer.current_location())
                    {
                        self.pointer_press(button, location, event.time());
                    }
                } else {
                    self.chrome_release();
                    self.end_grab();
                }

                if let Some(pointer) = self.seat.get_pointer() {
                    if let Some(focus) = pointer.current_focus() {
                        self.note_client_input_serial(&focus, serial);
                    }
                    pointer.button(
                        self,
                        &ButtonEvent {
                            button,
                            state,
                            serial,
                            time: event.time(),
                        },
                    );
                    pointer.frame(self);
                }
            }
            InputEvent::PointerAxis { event, .. } => {
                if self.screenshot_mode().is_some() {
                    return;
                }
                self.pointer_axis(&event);
            }
            _ => {}
        }
    }

    pub(crate) fn keyboard_key(&mut self, key_code: Keycode, key_state: KeyState, time: InputTime) {
        let serial = SERIAL_COUNTER.next_serial();
        let pressed = key_state == KeyState::Pressed;
        if pressed {
            self.note_activation_serial(serial);
        }
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let Some((filtered, mods_changed)) = keyboard.input_intercept::<FilterResult<()>, _>(
            smithay::input::keyboard::KeyboardSource::MAIN,
            self,
            key_code,
            key_state,
            |state, mods, keysym| {
                if state.screenshot_mode().is_some() {
                    if pressed {
                        for sym in keysym.raw_syms() {
                            match u32::from(sym) {
                                keysyms::KEY_Escape => state.cancel_screenshot(),
                                keysyms::KEY_space => state.start_window_screenshot(),
                                _ => {}
                            }
                        }
                    }
                    return FilterResult::Intercept(());
                }
                match compositor_shortcut(
                    mods.ctrl,
                    mods.alt,
                    mods.shift,
                    mods.logo,
                    &keysym.raw_syms(),
                    state.locked,
                ) {
                    CompositorShortcut::None => {}
                    shortcut => {
                        if pressed {
                            match shortcut {
                                CompositorShortcut::Vt(vt) => {
                                    state.request_vt_switch(vt);
                                }
                                CompositorShortcut::Quit => {
                                    tracing::info!("emergency exit: Ctrl+Alt+Backspace");
                                    state.stop();
                                }
                                CompositorShortcut::Blocked => {
                                    tracing::info!(
                                        "Ctrl+Alt+Backspace ignored: the session is locked"
                                    );
                                }
                                CompositorShortcut::Relock => {
                                    if state.locked && !state.lock_surfaces.is_empty() {
                                        tracing::info!(
                                            "Ctrl+Alt+L ignored: a lock surface is already live"
                                        );
                                    } else if state.relaunch_lock_screen() {
                                        tracing::info!("Ctrl+Alt+L: started the lock screen");
                                    }
                                }
                                CompositorShortcut::Screenshot => {
                                    tracing::info!(
                                        "PrintScreen: screenshotting the focused window"
                                    );
                                    state.request_screenshot(ScreenshotRequest::Focused);
                                }
                                CompositorShortcut::ScreenshotFull => {
                                    tracing::info!("Shift+Super+3: screenshotting the output");
                                    state.request_screenshot(ScreenshotRequest::Output);
                                }
                                CompositorShortcut::ScreenshotRegion => {
                                    tracing::info!("Shift+Super+4: selecting a screenshot region");
                                    state.start_region_screenshot();
                                }
                                CompositorShortcut::Launcher => {
                                    state.toggle_launcher();
                                }
                                CompositorShortcut::ForceQuit => {
                                    state.open_force_quit();
                                }
                                CompositorShortcut::HideApp => state.hide_focused_app(),
                                CompositorShortcut::HideOtherApps => state.hide_other_apps(),
                                CompositorShortcut::QuitApp => state.quit_focused_app(),
                                CompositorShortcut::MinimizeWindow => {
                                    if let Some(id) = state.focused_window() {
                                        state.set_minimized(id, true);
                                    }
                                }
                                CompositorShortcut::CloseWindow => {
                                    if let Some(id) = state.focused_window() {
                                        state.windows.send_close(id);
                                    }
                                }
                                CompositorShortcut::ArrangeLeft => {
                                    if let Some(id) = state.focused_window() {
                                        state.infinity_arrange(id, Layout::Left);
                                    }
                                }
                                CompositorShortcut::ArrangeRight => {
                                    if let Some(id) = state.focused_window() {
                                        state.infinity_arrange(id, Layout::Right);
                                    }
                                }
                                CompositorShortcut::ArrangeUp => {
                                    if let Some(id) = state.focused_window() {
                                        state.infinity_arrange(id, Layout::Top);
                                    }
                                }
                                CompositorShortcut::ArrangeDown => {
                                    if let Some(id) = state.focused_window() {
                                        state.infinity_arrange(id, Layout::Bottom);
                                    }
                                }
                                CompositorShortcut::ArrangePairLeft => {
                                    state.infinity_arrange_pair(PairLayout::SideBySide);
                                }
                                CompositorShortcut::ArrangePairRight => {
                                    state.infinity_arrange_pair(PairLayout::SideBySideReversed);
                                }
                                CompositorShortcut::ArrangePairUp => {
                                    state.infinity_arrange_pair(PairLayout::Stacked);
                                }
                                CompositorShortcut::ArrangePairDown => {
                                    state.infinity_arrange_pair(PairLayout::StackedReversed);
                                }
                                CompositorShortcut::ArrangeFill => {
                                    if let Some(id) = state.focused_window() {
                                        state.infinity_arrange(id, Layout::Fill);
                                    }
                                }
                                CompositorShortcut::ArrangeCenter => {
                                    if let Some(id) = state.focused_window() {
                                        state.infinity_arrange(id, Layout::Center);
                                    }
                                }
                                CompositorShortcut::ArrangeRestore => {
                                    if let Some(id) = state.focused_window() {
                                        state.infinity_restore(id);
                                    }
                                }
                                CompositorShortcut::ToggleFullscreen => {
                                    if let Some(id) = state.focused_window() {
                                        state.toggle_focus_mode(id);
                                    }
                                }
                                CompositorShortcut::PreviousSpace => {
                                    state.previous_space();
                                }
                                CompositorShortcut::NextSpace => {
                                    state.next_space();
                                }
                                CompositorShortcut::AddDesktop => {
                                    state.add_desktop();
                                }
                                CompositorShortcut::None => {}
                            }
                        }
                        return FilterResult::Intercept(());
                    }
                }
                if pressed && is_escape(&keysym.raw_syms()) {
                    state.dismiss_topmost_popup();
                }
                FilterResult::Forward
            },
        ) else {
            return;
        };
        let forwarded = matches!(filtered, FilterResult::Forward);
        if forwarded && self.locked {
            // ext-session-lock-v1: an input-method keyboard grab must not read the password.
            keyboard.input_forward_bypassing_grab(
                self,
                key_code,
                key_state,
                serial,
                time,
                mods_changed,
            );
        } else if forwarded {
            keyboard.input_forward(self, key_code, key_state, serial, time, mods_changed);
        }
        if pressed
            && forwarded
            && let Some(focus) = keyboard.current_focus()
        {
            self.note_client_input_serial(&focus, serial);
        }
    }

    pub(crate) fn pointer_press(
        &mut self,
        button: u32,
        location: Point<f64, Logical>,
        time: InputTime,
    ) {
        if self.locked {
            tracing::info!(?location, "press ignored: the session is locked");
            return;
        }
        self.pointer_motion(location, time);
        self.close_notification_panel_outside(location);
        let layer_under = self.layers.surface_under(location);
        if layer_under.is_some() && self.windows.topmost_popup().is_some() {
            tracing::info!(?location, "press outside the popups dismissed them");
            self.dismiss_all_popups();
        }
        if layer_under.is_none() && self.popup_surface_under(location).is_some() {
            tracing::info!(?location, "press landed on a popup");
            return;
        }

        if let Some((surface, _)) = layer_under {
            tracing::info!(?location, "press landed on a layer surface");
            self.clear_chrome_pointer();
            self.alert_press =
                crate::window_style::alert_layer(&surface).map(|alert| (alert, location));
            let focused_window = self
                .seat
                .get_keyboard()
                .and_then(|keyboard| keyboard.current_focus())
                .and_then(|focused| self.windows.id_of(&focused));
            if let Some(entry) = self.layers.entry_of(&surface) {
                let mode = entry.keyboard_interactivity;
                if self
                    .layer_focus
                    .clicked(surface.clone(), mode, focused_window)
                {
                    self.focus_layer_surface(&surface);
                }
            }
            return;
        }
        if self.layer_focus.clicked_outside() {
            self.apply_focus_restore(crate::popup::FocusRestore::Previous(
                None,
                self.focused_window(),
            ));
        }

        let side = f64::from(self.config.window.resize_border);
        let corner = f64::from(self.config.window.resize_corner);
        let edge = self
            .windows
            .edge_under(location, side, corner, &self.minimized);
        let under = self.windows.window_under(location, &self.minimized);

        if let Some((edge_id, edge)) = edge
            && (under != Some(edge_id) || self.windows.is_client_decorated(edge_id))
        {
            tracing::info!(?location, edge = ?edge, "press started a resize on the edge band");
            self.raise_and_focus(edge_id);
            self.start_resize(edge_id, edge, location);
            return;
        }

        let Some(id) = under else {
            match desktop_click(self.windows.topmost_popup().is_some()) {
                DesktopClick::CloseMenu => {
                    tracing::info!(?location, "press outside the popups dismissed them");
                    self.dismiss_all_popups();
                }
                DesktopClick::ClearFocus => {
                    tracing::info!(?location, "press landed on the desktop; focus cleared");
                    self.clear_keyboard_focus();
                    self.request_redraw();
                }
            }
            return;
        };
        if self.takes_focus_on_press(id) {
            self.raise_and_focus(id);
        } else {
            self.windows.raise(id);
            self.request_redraw();
        }

        if button == BUTTON_LEFT
            && let Some(local) = self.titlebar_point(id, location)
        {
            let button_kind = self
                .chrome_frame(id)
                .and_then(|(outer, insets)| crate::chrome::button_at(outer, insets, location));
            if let Some(kind) = button_kind {
                self.clear_chrome_pointer();
                if kind == ctdesign::ButtonKind::Maximize && !self.green_button_admitted(id) {
                    return;
                }
                let entry = self
                    .chrome_pointer
                    .get_or_insert_with(|| (id, Default::default()));
                if entry.0 != id {
                    *entry = (id, Default::default());
                }
                entry.1.on_press(Some(kind), true);
                self.request_redraw();
                return;
            }
            if self.revealed_titlebar(id).is_some() {
                return;
            }
            if self.register_titlebar_click(id, local, time).is_some() {
                self.apply_titlebar_double_click(id);
                return;
            }
        }

        if let Some((edge_id, edge)) = edge
            && edge_id == id
        {
            tracing::info!(?location, edge = ?edge, "press started a resize on the edge band");
            self.start_resize(id, edge, location);
            return;
        }

        if self
            .windows
            .surface_under(location, &self.minimized)
            .is_some()
        {
            tracing::info!(?location, window = ?id, "press landed on the client surface");
            if button == BUTTON_LEFT {
                self.register_client_click(id, location, time);
            }
            return;
        }
        if button == BUTTON_LEFT {
            tracing::info!(?location, window = ?id, "press started a move on the decoration");
            self.start_move(id, location);
        }
    }

    pub(crate) fn raise_and_focus(&mut self, id: WindowId) {
        self.windows.raise(id);
        self.focus_window(id);
        self.request_redraw();
    }

    fn window_button(&mut self, id: WindowId, kind: ctdesign::ButtonKind) {
        match kind {
            ctdesign::ButtonKind::Close => self.windows.send_close(id),
            ctdesign::ButtonKind::Minimize => self.set_minimized(id, true),
            ctdesign::ButtonKind::Maximize => self.green_button(id),
        }
        self.request_redraw();
    }

    fn green_button(&mut self, id: WindowId) {
        use ctappearance::window::{TitlebarAction, green_action};
        match green_action(self.config.window.green_button, self.has_fixed_width(id)) {
            TitlebarAction::Focus => {
                tracing::info!(window = id.get(), "focus mode requested");
                self.toggle_focus_mode(id);
            }
            TitlebarAction::Maximize if self.green_button_admitted(id) => {
                self.toggle_maximize(id);
            }
            TitlebarAction::Maximize => {
                tracing::info!(
                    window = id.get(),
                    "maximize ignored: the window is fixed-size"
                );
            }
            TitlebarAction::Minimize | TitlebarAction::None => {}
        }
    }

    pub(crate) fn green_button_admitted(&self, id: WindowId) -> bool {
        self.config.window.green_button != crate::config::GreenButtonAction::Zoom
            || self.windows.zoom_supported(id)
    }

    #[track_caller]
    pub(crate) fn toggle_maximize(&mut self, id: WindowId) {
        let location = std::panic::Location::caller();
        let requested = !self.maximized.contains_key(&id);
        tracing::info!(
            window = id.get(),
            maximize = requested,
            origin = format!("{}:{}", location.file(), location.line()),
            "toggle_maximize"
        );
        self.set_maximized(id, requested);
    }

    #[track_caller]
    pub(crate) fn set_maximized(&mut self, id: WindowId, maximized: bool) {
        let location = std::panic::Location::caller();
        let currently_maximized = self.maximized.contains_key(&id);
        if maximized == currently_maximized {
            tracing::debug!(
                window = id.get(),
                maximized,
                origin = format!("{}:{}", location.file(), location.line()),
                "set_maximized: already in requested state"
            );
            return;
        }
        if maximized {
            let Some(outer) = self.windows.outer(id) else {
                return;
            };
            tracing::info!(
                window = id.get(),
                origin = format!("{}:{}", location.file(), location.line()),
                "window maximized"
            );
            let target = self.maximize_target(id, outer);
            self.maximized.insert(id, outer);
            self.windows.set_maximized_with_outer(id, true, target);
        } else {
            let Some(saved) = self.maximized.remove(&id) else {
                return;
            };
            tracing::info!(
                window = id.get(),
                origin = format!("{}:{}", location.file(), location.line()),
                "window restored"
            );
            self.windows.set_maximized_with_outer(id, false, saved);
        }
        self.focus_window(id);
        self.foreign_state_changed(id);
        self.request_redraw();
    }

    pub(crate) fn has_fixed_width(&self, id: WindowId) -> bool {
        self.windows
            .client_size_bounds(id)
            .is_some_and(|(min, max)| is_fixed_width(min, max))
    }

    fn maximize_target(
        &self,
        id: WindowId,
        outer: SceneRect<f64, malaysia_scene::geometry::Logical>,
    ) -> SceneRect<f64, malaysia_scene::geometry::Logical> {
        let fixed_width = self
            .windows
            .client_size_bounds(id)
            .is_some_and(|(min, max)| is_fixed_width(min, max));
        maximize_target_rect(fixed_width, self.workspace(), outer)
    }

    fn log_focus_stack(&self, focused: Option<WindowId>) {
        if !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        let mut lines = Vec::new();
        for window in self.windows.iter() {
            let id = window.id();
            let minimized = self.minimized.contains(&id);
            lines.push(format!(
                "app_id={:?} mapped={} hidden={} minimized={} rect={:?} focused={}",
                window.app_id(),
                window.is_mapped(),
                window.is_hidden(),
                minimized,
                window.outer(),
                focused == Some(id),
            ));
        }
        tracing::debug!(focused = ?focused, "focus stack (back-to-front):\n{}", lines.join("\n"));
    }

    pub(crate) fn focus_window(&mut self, id: WindowId) {
        if self.locked {
            return;
        }
        if self.layer_focus.blocks_window_focus() {
            tracing::info!(
                window = id.get(),
                "window focus ignored: an exclusive layer owns it"
            );
            return;
        }
        self.show_space_of(id);
        if !self.windows.is_visible(id, &self.minimized) {
            tracing::info!(
                window = id.get(),
                "window focus ignored: the window is not visible"
            );
            return;
        }
        let Some(surface) = self.windows.surface_of(id).cloned() else {
            return;
        };
        self.windows.raise(id);
        let previous = self.focused_window();
        let changed = self.active_window.set(Some(id));
        let serial = SERIAL_COUNTER.next_serial();
        match self.seat.get_keyboard() {
            Some(keyboard) => {
                tracing::info!("giving keyboard focus to a window");
                let client = surface.client();
                keyboard.set_focus(self, Some(surface), serial);
                set_data_device_focus(&self.display_handle, &self.seat, client.clone());
                set_primary_focus(&self.display_handle, &self.seat, client);
            }
            None => tracing::warn!("a window was focused but the seat has no keyboard"),
        }
        if changed {
            let shown_before = self.shown_active_for(previous);
            self.sync_panels();
            let shown = self.shown_active();
            for window in [previous, shown_before, Some(id), shown]
                .into_iter()
                .flatten()
            {
                self.foreign_state_changed(window);
            }
        }
        self.log_focus_stack(Some(id));
    }

    pub(crate) fn toggle_launcher(&mut self) {
        if self.locked {
            return;
        }
        if let Some(child) = &mut self.launcher_child {
            if child.try_wait().ok().flatten().is_none() {
                let pid = nix::unistd::Pid::from_raw(child.id() as i32);
                let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM);
                tracing::info!("Super+Space: stopping the launcher");
                return;
            }
            self.launcher_child = None;
        }

        self.launcher_child = self.spawn_shell_component("launcher");
    }

    pub(crate) fn open_force_quit(&mut self) {
        if self.locked {
            return;
        }
        if let Some(child) = &mut self.force_quit_child
            && child.try_wait().ok().flatten().is_none()
        {
            return;
        }
        self.force_quit_child = self.spawn_shell_component("force-quit");
    }

    fn spawn_shell_component(&self, component: &str) -> Option<std::process::Child> {
        match crate::spawn::shell_component(component, self.idle.wayland_display()) {
            Ok(child) => {
                tracing::info!(component, "started a shell component");
                Some(child)
            }
            Err(error) => {
                tracing::error!(%error, component, "starting a shell component");
                None
            }
        }
    }

    fn chrome_frame(
        &self,
        id: WindowId,
    ) -> Option<(
        malaysia_scene::geometry::Rect<f64, malaysia_scene::geometry::Logical>,
        crate::decoration::Insets,
    )> {
        self.revealed_titlebar(id)
            .or_else(|| self.windows.outer(id).zip(self.windows.insets(id)))
    }

    fn titlebar_point(
        &self,
        id: WindowId,
        location: Point<f64, Logical>,
    ) -> Option<Point<f64, Logical>> {
        let (outer, insets) = self.chrome_frame(id)?;
        if insets.top < f64::from(TITLEBAR.thickness) {
            return None;
        }
        let local = Point::new(location.x - outer.x(), location.y - outer.y());
        (local.y >= 0.0 && local.y < f64::from(TITLEBAR.thickness)).then_some(local)
    }

    fn chrome_hover_at(
        &self,
        id: WindowId,
        location: Point<f64, Logical>,
    ) -> Option<(Option<ctdesign::ButtonKind>, bool)> {
        let local = self.titlebar_point(id, location)?;
        let hit = ctappearance::traffic::hit((local.x, local.y));
        let (gx, gy, gw, gh) = ctappearance::traffic::group_rect();
        let over_group = local.x >= gx && local.x < gx + gw && local.y >= gy && local.y < gy + gh;
        Some((hit, over_group))
    }

    fn update_chrome_pointer(
        &mut self,
        location: Point<f64, Logical>,
        window: Option<WindowId>,
        blocked_by_layer: bool,
    ) -> bool {
        let window = if blocked_by_layer { None } else { window };
        if let Some((held, mut pointer)) = self.chrome_pointer {
            if Some(held) == window {
                let Some((hit, over_group)) = self.chrome_hover_at(held, location) else {
                    let changed = pointer.on_leave();
                    self.chrome_pointer = None;
                    return changed;
                };
                let changed = pointer.on_motion(hit, over_group);
                self.chrome_pointer = Some((held, pointer));
                return changed;
            }
            let changed = pointer.on_leave();
            self.chrome_pointer = None;
            if let Some(id) = window
                && let Some((hit, over_group)) = self.chrome_hover_at(id, location)
            {
                let mut pointer = ctappearance::traffic::TrafficPointer::default();
                let moved = pointer.on_motion(hit, over_group);
                self.chrome_pointer = Some((id, pointer));
                return changed || moved;
            }
            return changed;
        }
        let Some(id) = window else {
            return false;
        };
        let Some((hit, over_group)) = self.chrome_hover_at(id, location) else {
            return false;
        };
        let mut pointer = ctappearance::traffic::TrafficPointer::default();
        let changed = pointer.on_motion(hit, over_group);
        self.chrome_pointer = Some((id, pointer));
        changed
    }

    pub(crate) fn clear_chrome_pointer(&mut self) -> bool {
        let Some((_, mut pointer)) = self.chrome_pointer.take() else {
            return false;
        };
        let changed = pointer.on_leave();
        if changed {
            self.request_redraw();
        }
        changed
    }

    pub(crate) fn chrome_release(&mut self) {
        let location = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location());
        if let Some(location) = location {
            self.chrome_release_at(location);
        }
    }

    pub(crate) fn chrome_release_at(&mut self, location: Point<f64, Logical>) {
        let Some((id, _)) = self.chrome_pointer else {
            return;
        };
        let (hit, over_group) = self.chrome_hover_at(id, location).unwrap_or((None, false));
        let Some((_, mut pointer)) = self.chrome_pointer else {
            return;
        };
        let action = pointer.on_release(hit, over_group);
        self.chrome_pointer = Some((id, pointer));
        self.request_redraw();
        if let Some(kind) = action {
            self.window_button(id, kind);
        }
    }

    fn register_titlebar_click(
        &mut self,
        id: WindowId,
        local: Point<f64, Logical>,
        time: InputTime,
    ) -> Option<crate::config::DoubleClickAction> {
        let (previous, action) = double_click(
            self.titlebar_click,
            id,
            local,
            u64::from(time.millis()),
            self.config.window.double_click_titlebar,
        );
        self.titlebar_click = previous;
        action
    }

    fn register_client_click(
        &mut self,
        id: WindowId,
        location: Point<f64, Logical>,
        time: InputTime,
    ) {
        let (previous, action) = double_click(
            self.client_click,
            id,
            location,
            u64::from(time.millis()),
            self.config.window.double_click_titlebar,
        );
        self.client_click = previous;
        self.client_double_click = action.map(|_| (id, std::time::Instant::now()));
    }

    pub(crate) fn followed_double_click(&self, id: WindowId) -> bool {
        self.client_double_click.is_some_and(|(clicked, at)| {
            clicked == id
                && at.elapsed().as_millis()
                    <= u128::from(ctdesign::TITLEBAR_DOUBLE_CLICK.interval_ms)
        })
    }

    pub(crate) fn note_client_input_serial(&self, surface: &WlSurface, serial: Serial) {
        if let Some(client) = surface.client()
            && let Some(state) = client.get_data::<crate::state::ClientState>()
        {
            state.note_input_serial(serial);
        }
    }

    pub(crate) fn client_last_input_serial(&self, surface: &WlSurface) -> Option<Serial> {
        let client = surface.client()?;
        let state = client.get_data::<crate::state::ClientState>()?;
        state.last_input_serial()
    }

    pub(crate) fn focus_popup_surface(&mut self, surface: &WlSurface, serial: Serial) {
        if self.locked {
            return;
        }
        if let Some(keyboard) = self.seat.get_keyboard() {
            let previous_focus = keyboard.current_focus();
            let previous_window = previous_focus
                .as_ref()
                .and_then(|focus| self.windows.id_of(focus));
            let root_layer = previous_focus.as_ref().and_then(|focus| {
                if let Some(entry) = self.layers.entry_of(focus) {
                    Some((focus.clone(), entry.keyboard_interactivity))
                } else if self.popups.is_popup_surface(focus) {
                    self.popups.current_root_layer()
                } else {
                    None
                }
            });
            self.popups.push_focus_frame(
                surface.clone(),
                serial,
                root_layer,
                previous_focus.clone(),
                previous_window,
            );
            keyboard.set_focus(self, Some(surface.clone()), serial);
            if let Some(client) = surface.client() {
                set_data_device_focus(&self.display_handle, &self.seat, Some(client.clone()));
                set_primary_focus(&self.display_handle, &self.seat, Some(client));
            }
        }
    }

    pub(crate) fn apply_focus_restore(&mut self, restore: crate::popup::FocusRestore) {
        if self.locked {
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        match restore {
            crate::popup::FocusRestore::Popup(surface) => {
                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.set_focus(self, Some(surface), serial);
                }
            }
            crate::popup::FocusRestore::Layer(surface) => {
                self.focus_layer_surface(&surface);
            }
            crate::popup::FocusRestore::Previous(surface, window) => {
                let target = self
                    .focused_window()
                    .or(window)
                    .filter(|id| self.windows.surface_of(*id).is_some());
                if let Some(id) = target {
                    self.focus_window(id);
                } else if let Some(surface) =
                    surface.filter(|surface| self.surface_still_exists(surface))
                {
                    tracing::info!(
                        surface = ?surface.id(),
                        "giving keyboard focus back to a surface"
                    );
                    if let Some(keyboard) = self.seat.get_keyboard() {
                        keyboard.set_focus(self, Some(surface.clone()), serial);
                        if let Some(client) = surface.client() {
                            set_data_device_focus(
                                &self.display_handle,
                                &self.seat,
                                Some(client.clone()),
                            );
                            set_primary_focus(&self.display_handle, &self.seat, Some(client));
                        }
                    }
                } else {
                    self.clear_keyboard_focus();
                }
            }
        }
    }

    fn surface_still_exists(&self, surface: &WlSurface) -> bool {
        self.windows.id_of(surface).is_some()
            || self.popups.id_of(surface).is_some()
            || self.layers.entry_of(surface).is_some()
    }

    fn focus_layer_surface(&mut self, surface: &WlSurface) {
        if self.locked {
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            tracing::info!(surface = ?surface.id(), "giving keyboard focus to a layer surface");
            let client = surface.client();
            keyboard.set_focus(self, Some(surface.clone()), serial);
            set_data_device_focus(&self.display_handle, &self.seat, client.clone());
            set_primary_focus(&self.display_handle, &self.seat, client);
        }
    }

    pub(crate) fn layer_surface_mapped(&mut self, layer: &LayerSurface) {
        let surface = layer.wl_surface().clone();
        let Some(entry) = self.layers.entry_of(&surface) else {
            return;
        };
        if !matches!(
            entry.layer,
            smithay::wayland::shell::wlr_layer::Layer::Top
                | smithay::wayland::shell::wlr_layer::Layer::Overlay
        ) {
            return;
        }
        let focused_window = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus())
            .and_then(|focused| self.windows.id_of(&focused));
        let mode = entry.keyboard_interactivity;
        if self
            .layer_focus
            .mapped(surface.clone(), mode, focused_window)
        {
            self.focus_layer_surface(&surface);
        }
    }

    pub(crate) fn layer_surface_destroyed(&mut self, surface: &WlSurface) {
        use crate::layers::LayerUnfocus;
        match self.layer_focus.unmapped(surface) {
            LayerUnfocus::None => {}
            LayerUnfocus::Layer(next) => self.focus_layer_surface(&next),
            LayerUnfocus::Window(Some(id)) => {
                if self.windows.surface_of(id).is_some() {
                    self.focus_window(id);
                } else {
                    self.clear_keyboard_focus();
                }
            }
            LayerUnfocus::Window(None) => self.clear_keyboard_focus(),
        }
    }

    #[track_caller]
    pub(crate) fn clear_keyboard_focus(&mut self) {
        let location = std::panic::Location::caller();
        let shown_before = self.shown_active();
        let previous = self.active_window.clear();
        if previous.is_some() {
            tracing::info!(
                previous = previous.map(|id| id.get()),
                origin = format!("{}:{}", location.file(), location.line()),
                space = self.spaces.current_index(),
                bar_revealed = self.focus_reveal.is_shown(),
                "active window cleared"
            );
        }
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, None, serial);
            set_data_device_focus(&self.display_handle, &self.seat, None);
            set_primary_focus(&self.display_handle, &self.seat, None);
        }
        if let Some(previous) = previous {
            self.sync_panels();
            for window in [Some(previous), shown_before].into_iter().flatten() {
                self.foreign_state_changed(window);
            }
        }
        self.log_focus_stack(None);
    }

    pub(crate) fn forget_window(&mut self, surface: &WlSurface) {
        let active = self.focused_window();
        let next = self.focus_fallback(self.windows.id_of(surface));
        let Some(id) = self.windows.remove(surface) else {
            return;
        };
        if self
            .chrome_pointer
            .as_ref()
            .is_some_and(|(held, _)| *held == id)
        {
            self.chrome_pointer = None;
        }
        if self
            .titlebar_click
            .as_ref()
            .is_some_and(|(held, ..)| *held == id)
        {
            self.titlebar_click = None;
        }
        self.foreign_window_destroyed(id);
        self.space_window_removed(id);
        self.maximized.remove(&id);
        self.arranged.remove(&id);
        self.minimized.remove(&id);
        self.chrome.remove(crate::chrome::FrameOwner::Window(id));
        if self.grab.and_then(Grab::window) == Some(id) {
            self.grab = None;
        }
        tracing::info!(
            windows = self.windows.len(),
            topmost = ?self.windows.topmost(),
            "window removed"
        );
        if active == Some(id) {
            match next.filter(|next| self.windows.surface_of(*next).is_some()) {
                Some(next) => self.focus_window(next),
                None => self.clear_keyboard_focus(),
            }
        }
        if self.windows.is_empty() {
            tracing::info!("scene empty");
        }
        self.request_redraw();
    }

    pub(crate) fn held_grab(&self, serial: Serial) -> Option<GrabStartData<State>> {
        let pointer = self.seat.get_pointer()?;
        if !pointer.has_grab(serial) {
            return None;
        }
        pointer.grab_start_data()
    }

    pub(crate) fn start_move(&mut self, id: WindowId, location: Point<f64, Logical>) {
        if self.in_focus_mode(id) {
            tracing::info!(
                window = id.get(),
                "move refused: a full-screen window stays put"
            );
            return;
        }
        self.clear_chrome_pointer();
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        let offset = ScenePoint::new(location.x - outer.x(), location.y - outer.y());
        self.grab = Some(Grab::Move { window: id, offset });
        self.fade_out_popups_of(id);
    }

    pub(crate) fn start_resize(&mut self, id: WindowId, edge: Edge, location: Point<f64, Logical>) {
        if self.in_focus_mode(id) {
            tracing::info!(
                window = id.get(),
                "resize refused: a full-screen window stays put"
            );
            return;
        }
        self.clear_chrome_pointer();
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        self.windows.set_resizing(id, true, Some(edge));
        self.grab = Some(Grab::Resize {
            window: id,
            edge,
            start_pointer: ScenePoint::new(location.x, location.y),
            start_outer: outer,
        });
    }

    pub(crate) fn end_grab(&mut self) {
        if let Some(Grab::Resize { window, .. }) = self.grab {
            self.windows.set_resizing(window, false, None);
        }
        self.grab = None;
        self.alert_press = None;
    }

    fn start_alert_drag(&mut self, location: Point<f64, Logical>) {
        let Some((alert, press)) = self.alert_press else {
            return;
        };
        if (location.x - press.x).hypot(location.y - press.y)
            <= ctdesign::TITLEBAR_DOUBLE_CLICK.slop
        {
            return;
        }
        self.alert_press = None;
        if let Some(origin) = self.layers.alert_location(alert) {
            self.grab = Some(Grab::Alert {
                alert,
                offset: press - origin.to_f64(),
            });
        }
    }

    pub(crate) fn pointer_motion(&mut self, location: Point<f64, Logical>, time: InputTime) {
        self.pointer_motion_raw(location, time, None);
    }

    fn pointer_motion_raw(
        &mut self,
        mut location: Point<f64, Logical>,
        time: InputTime,
        raw_delta: Option<(Point<f64, Logical>, Point<f64, Logical>)>,
    ) {
        self.request_redraw();
        let now = Instant::now();
        if self.config.appearance.shake_to_locate && self.shake.observe(location.x, now) {
            self.trigger_cursor_boost();
        } else {
            self.keep_cursor_boost_alive(now);
        }

        if self.grab.is_none() {
            self.start_alert_drag(location);
        }
        if let Some(grab) = self.grab {
            if let Some(pointer) = self.seat.get_pointer() {
                let delta = location - pointer.current_location();
                let (delta, delta_unaccel) = raw_delta.unwrap_or((delta, delta));
                let serial = SERIAL_COUNTER.next_serial();
                pointer.relative_motion(
                    self,
                    None,
                    &RelativeMotionEvent {
                        delta,
                        delta_unaccel,
                        time,
                    },
                );
                pointer.motion(
                    self,
                    None,
                    &MotionEvent {
                        location,
                        serial,
                        time,
                    },
                );
                pointer.frame(self);
            }
            self.drag(grab, location);
            self.request_redraw();
            return;
        }

        if self.locked {
            self.clear_chrome_pointer();
            let focus = self
                .lock_surfaces
                .first()
                .filter(|surface| surface.alive())
                .map(|surface| (surface.wl_surface().clone(), Point::from((0.0, 0.0))));
            if let Some(pointer) = self.seat.get_pointer() {
                pointer.motion(
                    self,
                    focus,
                    &MotionEvent {
                        location,
                        serial: SERIAL_COUNTER.next_serial(),
                        time,
                    },
                );
                pointer.frame(self);
            }
            return;
        }

        self.update_edge_cursor(location);
        self.update_focus_reveal(location);

        let side = f64::from(self.config.window.resize_border);
        let corner = f64::from(self.config.window.resize_corner);
        let popup_under = self.popup_surface_under(location);
        let layer_under = self.layers.surface_under(location);
        let overlay_under = popup_under.is_some() || layer_under.is_some();
        let edge = if overlay_under {
            None
        } else {
            self.windows
                .edge_under(location, side, corner, &self.minimized)
        };
        let under_window = self.windows.window_under(location, &self.minimized);
        if self.update_chrome_pointer(location, under_window, overlay_under) {
            self.request_redraw();
        }
        let over_revealed_titlebar = layer_under.is_none()
            && under_window.is_some_and(|id| {
                self.titlebar_point(id, location).is_some() && self.revealed_titlebar(id).is_some()
            });
        let blocked = over_revealed_titlebar
            || pointer_focus_blocked(overlay_under, edge.map(|(id, _)| id), under_window);
        let under = if blocked {
            None
        } else {
            layer_under
                .or(popup_under)
                .or_else(|| self.windows.surface_under(location, &self.minimized))
        };

        let serial = SERIAL_COUNTER.next_serial();
        let focus = under.clone().map(|(surface, local)| {
            let origin = Point::new(location.x - local.x, location.y - local.y);
            (surface, origin)
        });
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let delta = location - pointer.current_location();
        let (relative, relative_unaccel) = raw_delta.unwrap_or((delta, delta));

        let mut locked = false;
        let mut confined = None;
        if let Some((surface, local)) = &under {
            with_pointer_constraint(surface, &pointer, |constraint| {
                let Some(constraint) = constraint else {
                    return;
                };
                let point = local.to_i32_round();
                if !constraint
                    .region()
                    .is_none_or(|region| region.contains(point))
                {
                    return;
                }
                if !constraint.is_active() {
                    constraint.activate();
                }
                match &*constraint {
                    PointerConstraint::Locked(_) => locked = true,
                    PointerConstraint::Confined(confine) => {
                        confined = confine.region().cloned();
                    }
                }
            });
        }
        if locked {
            pointer.relative_motion(
                self,
                focus,
                &RelativeMotionEvent {
                    delta: relative,
                    delta_unaccel: relative_unaccel,
                    time,
                },
            );
            pointer.frame(self);
            return;
        }
        if let (Some(region), Some((_, local))) = (confined, &under) {
            let origin = location - *local;
            let old = location - delta;
            let mut clamped = old;
            let candidate = Point::new(old.x + delta.x, old.y);
            if region.contains((candidate - origin).to_i32_round()) {
                clamped.x = candidate.x;
            }
            let candidate = Point::new(clamped.x, old.y + delta.y);
            if region.contains((candidate - origin).to_i32_round()) {
                clamped.y = candidate.y;
            }
            location = clamped;
        }

        pointer.relative_motion(
            self,
            focus.clone(),
            &RelativeMotionEvent {
                delta: relative,
                delta_unaccel: relative_unaccel,
                time,
            },
        );
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location,
                serial,
                time,
            },
        );
        pointer.frame(self);
    }

    fn popup_surface_under(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let id = self.windows.popup_under(location)?;
        let rect = self.windows.popup_rect(id)?;
        let surface = self.popups.surface_of(id)?.clone();
        let geometry = crate::window::window_geometry(&surface).unwrap_or_default();
        let local = Point::new(
            location.x - rect.x() + f64::from(geometry.loc.x),
            location.y - rect.y() + f64::from(geometry.loc.y),
        );
        crate::layers::surface_accepts_input(&surface, local).then_some((surface, local))
    }

    fn drag(&mut self, grab: Grab, location: Point<f64, Logical>) {
        match grab {
            Grab::Move { window, offset } => {
                let Some(offset) = self.unmaximize_on_drag(window, offset, location) else {
                    return;
                };
                let top = self.workspace().0.y - self.client_shadow_top(window);
                let target = Point::from((
                    round(location.x - offset.x),
                    round(location.y - offset.y).max(top),
                ));
                self.windows.move_to(window, target);
            }
            Grab::Resize {
                window,
                edge,
                start_pointer,
                start_outer,
            } => {
                let dx = location.x - start_pointer.x;
                let dy = location.y - start_pointer.y;
                let minimum = self.windows.minimum_outer_size(window);
                let maximum = self.windows.maximum_outer_size(window);
                let outer =
                    malaysia_scene::scene::resized(start_outer, edge, dx, dy, minimum, maximum);
                self.windows.resize(window, outer);
            }
            Grab::Alert { alert, offset } => {
                let target = Point::from((
                    round(location.x - offset.x),
                    round(location.y - offset.y).max(self.workspace().0.y),
                ));
                self.layers.move_alert(alert, target);
            }
        }
    }

    fn client_shadow_top(&self, window: WindowId) -> i32 {
        if !self.windows.is_client_decorated(window) {
            return 0;
        }
        self.windows
            .surface_of(window)
            .and_then(crate::window::window_geometry)
            .map_or(0, |geometry| geometry.loc.y)
    }

    fn unmaximize_on_drag(
        &mut self,
        window: WindowId,
        offset: ScenePoint<f64, malaysia_scene::geometry::Logical>,
        location: Point<f64, Logical>,
    ) -> Option<ScenePoint<f64, malaysia_scene::geometry::Logical>> {
        if !self.maximized.contains_key(&window) {
            return Some(offset);
        }
        let outer = self.windows.outer(window)?;
        let press: ScenePoint<f64, malaysia_scene::geometry::Logical> =
            ScenePoint::new(outer.x() + offset.x, outer.y() + offset.y);
        if (location.x - press.x).hypot(location.y - press.y)
            <= ctdesign::TITLEBAR_DOUBLE_CLICK.slop
        {
            return None;
        }
        let saved = self.maximized.remove(&window)?;
        let restored = restored_under_pointer(outer, saved, press.x);
        self.windows
            .set_maximized_with_outer(window, false, restored);
        self.foreign_state_changed(window);
        tracing::info!(window = window.get(), "window restored by a drag");
        let offset = ScenePoint::new(press.x - restored.x(), offset.y);
        self.grab = Some(Grab::Move { window, offset });
        Some(offset)
    }

    fn update_edge_cursor(&mut self, location: Point<f64, Logical>) {
        let side = f64::from(self.config.window.resize_border);
        let corner = f64::from(self.config.window.resize_corner);
        let overlay_under = self.popup_surface_under(location).is_some()
            || self.layers.surface_under(location).is_some();
        let edge = if overlay_under {
            None
        } else {
            self.windows
                .edge_under(location, side, corner, &self.minimized)
        };
        match edge {
            Some((_, edge)) => {
                if matches!(self.client_cursor, CursorImageStatus::Named(_)) {
                    self.cursor_status = CursorImageStatus::Named(edge_cursor(edge));
                }
            }
            None => {
                if matches!(self.cursor_status, CursorImageStatus::Named(_)) {
                    self.cursor_status = self.client_cursor.clone();
                }
            }
        }
    }

    fn pointer_axis<I: InputBackend>(&mut self, event: &impl PointerAxisEvent<I>) {
        let source = event.source();
        let horizontal = event
            .amount(Axis::Horizontal)
            .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0);
        let vertical = event
            .amount(Axis::Vertical)
            .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0);

        let mut frame = AxisFrame::new(event.time()).source(source);
        if horizontal != 0.0 {
            frame = frame.value(Axis::Horizontal, horizontal);
            if let Some(discrete) = event.amount_v120(Axis::Horizontal) {
                frame = frame.v120(Axis::Horizontal, discrete as i32);
            }
        }
        if vertical != 0.0 {
            frame = frame.value(Axis::Vertical, vertical);
            if let Some(discrete) = event.amount_v120(Axis::Vertical) {
                frame = frame.v120(Axis::Vertical, discrete as i32);
            }
        }
        if source == AxisSource::Finger {
            if event.amount(Axis::Horizontal) == Some(0.0) {
                frame = frame.stop(Axis::Horizontal);
            }
            if event.amount(Axis::Vertical) == Some(0.0) {
                frame = frame.stop(Axis::Vertical);
            }
        }

        if source == AxisSource::Finger {
            self.follow_edge_swipe(horizontal, event.amount(Axis::Horizontal) == Some(0.0));
        }
        if let Some(pointer) = self.seat.get_pointer() {
            pointer.axis(self, frame);
            pointer.frame(self);
        }
    }

    fn close_notification_panel_outside(&self, location: Point<f64, Logical>) {
        let Some(panel) = crate::notification_panel::open_panel(&self.layers) else {
            return;
        };
        let on_bar = self
            .layers
            .mapped()
            .iter()
            .any(|entry| entry.is_desktop_bar() && entry.geometry.to_f64().contains(location));
        if !panel.to_f64().contains(location) && !on_bar {
            crate::notification_panel::send(crate::notification_panel::Request::Close);
        }
    }

    fn follow_edge_swipe(&mut self, horizontal: f64, ended: bool) {
        let Some(location) = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
        else {
            return;
        };
        let panel = crate::notification_panel::open_panel(&self.layers);
        let width = f64::from(self.logical_output_size().w);
        let at_edge = location.x >= width - ctdesign::NOTIFICATION_EDGE
            || panel.is_some_and(|panel| panel.to_f64().contains(location));
        if let Some(request) = self
            .edge_swipe
            .feed(horizontal, ended, at_edge, panel.is_some())
        {
            crate::notification_panel::send(request);
        }
    }
}

fn is_fixed_width(min: Size<i32, Logical>, max: Size<i32, Logical>) -> bool {
    min.w > 0 && max.w > 0 && min.w == max.w
}

fn maximize_target_rect(
    fixed_width: bool,
    workspace: (Point<i32, Logical>, Size<i32, Logical>),
    outer: SceneRect<f64, malaysia_scene::geometry::Logical>,
) -> SceneRect<f64, malaysia_scene::geometry::Logical> {
    let (origin, size) = workspace;
    if fixed_width {
        SceneRect::new(
            ScenePoint::new(outer.x(), f64::from(origin.y)),
            SceneSize::new(outer.width(), f64::from(size.h)),
        )
    } else {
        SceneRect::new(
            ScenePoint::new(f64::from(origin.x), f64::from(origin.y)),
            SceneSize::new(f64::from(size.w), f64::from(size.h)),
        )
    }
}

fn restored_under_pointer(
    maximized: SceneRect<f64, malaysia_scene::geometry::Logical>,
    saved: SceneRect<f64, malaysia_scene::geometry::Logical>,
    pointer_x: f64,
) -> SceneRect<f64, malaysia_scene::geometry::Logical> {
    let share = (pointer_x - maximized.x()) / maximized.width();
    SceneRect::new(
        ScenePoint::new(pointer_x - saved.width() * share, maximized.y()),
        saved.size(),
    )
}

fn pointer_focus_blocked(
    layer_under: bool,
    edge_window: Option<WindowId>,
    under_window: Option<WindowId>,
) -> bool {
    !layer_under && edge_window.is_some_and(|id| Some(id) != under_window)
}

fn edge_cursor(edge: Edge) -> CursorIcon {
    use Edge;
    match edge {
        Edge::Top | Edge::Bottom => CursorIcon::NsResize,
        Edge::Left | Edge::Right => CursorIcon::EwResize,
        Edge::TopLeft | Edge::BottomRight => CursorIcon::NwseResize,
        Edge::TopRight | Edge::BottomLeft => CursorIcon::NeswResize,
    }
}
fn double_click(
    previous: Option<TitlebarClick>,
    id: WindowId,
    local: Point<f64, Logical>,
    now: u64,
    action: crate::config::DoubleClickAction,
) -> (
    Option<TitlebarClick>,
    Option<crate::config::DoubleClickAction>,
) {
    let interval = u64::from(ctdesign::TITLEBAR_DOUBLE_CLICK.interval_ms);
    let slop = ctdesign::TITLEBAR_DOUBLE_CLICK.slop;
    let double = previous.is_some_and(|(window, then, point)| {
        window == id
            && now.saturating_sub(then) <= interval
            && (point.x - local.x).hypot(point.y - local.y) <= slop
    });
    if double {
        (None, Some(action))
    } else {
        (Some((id, now, local)), None)
    }
}

#[cfg(test)]
mod tests;
