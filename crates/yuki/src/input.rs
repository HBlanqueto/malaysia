//! Translating backend input events into seat events.
//!
//! The winit backend reports input in Smithay's unified [`InputEvent`] form, so
//! this module is backend-agnostic and will keep working when the Direct
//! Session milestone replaces winit. Every serial comes from Smithay's single
//! counter; none is invented.
//!
//! Interactive move and resize live here: a pointer button press on a window's
//! decoration starts a grab, motion drives it, and release ends it. The grab is
//! the compositor's own state, not a Smithay `PointerGrab`, because every event
//! already passes through this module.

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    InputTime, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
};
use smithay::input::keyboard::FilterResult;
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, GrabStartData, MotionEvent,
};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER, Serial};
use smithay::wayland::selection::data_device::set_data_device_focus;
use smithay::wayland::selection::primary_selection::set_primary_focus;
use xkbcommon::xkb::{self, keysyms};
use yuki_scene::geometry::Point as ScenePoint;
use yuki_scene::scene::{Edge, WindowId};

use crate::state::{Grab, State, TestPress};
use crate::window::RESIZE_BORDER;

/// The Linux button code for the primary pointer button.
const BUTTON_LEFT: u32 = 0x110;
/// The number of motions the scripted input sends after its press.
///
/// A real drag gesture does not ask for a move on the press: GTK's, for one,
/// waits until the pointer has passed a threshold. Following the press with
/// several motions lets such a client send its request while the button is
/// still held, which is what the protocol requires.
const TEST_DRAG_STEPS: usize = 4;
/// The pointer movement of each scripted pre-threshold motion, in logical
/// pixels.
const TEST_DRAG_STEP: (f64, f64) = (12.0, 8.0);

impl State {
    /// Handles one input event from the backend.
    pub(crate) fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = event.time();
                if let Some(keyboard) = self.seat.get_keyboard() {
                    let pressed = event.state() == KeyState::Pressed;
                    keyboard.input::<(), _>(
                        self,
                        event.key_code(),
                        event.state(),
                        serial,
                        time,
                        |state, mods, keysym| {
                            // Ctrl+Alt+F1..F12 asks the DRM backend for a VT
                            // switch; Ctrl+Alt+Backspace is the documented
                            // emergency exit. The nested backend ignores the
                            // VT request (there is nothing to switch to).
                            if pressed && mods.ctrl && mods.alt {
                                if let Some(vt) = keysym.raw_syms().iter().find_map(|sym| {
                                    let sym = u32::from(*sym);
                                    (keysyms::KEY_F1..=keysyms::KEY_F12)
                                        .contains(&sym)
                                        .then(|| (sym - keysyms::KEY_F1 + 1) as u8)
                                }) {
                                    state.request_vt_switch(vt);
                                }
                                if keysym
                                    .raw_syms()
                                    .iter()
                                    .any(|sym| u32::from(*sym) == keysyms::KEY_BackSpace)
                                {
                                    tracing::info!("emergency exit: Ctrl+Alt+Backspace");
                                    state.stop();
                                }
                            }
                            // Esc closes one popup level (the topmost menu).
                            if pressed
                                && keysym
                                    .raw_syms()
                                    .iter()
                                    .any(|sym| xkb::keysym_to_utf32(*sym) == keysyms::KEY_Escape)
                            {
                                state.dismiss_topmost_popup();
                            }
                            FilterResult::Forward
                        },
                    );
                }
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let location = event
                    .position_transformed(self.logical_output_size())
                    .to_f64();
                self.pointer_motion(location, event.time());
            }
            InputEvent::PointerButton { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let button = event.button_code();
                let state = event.state();

                if state == ButtonState::Pressed {
                    if let Some(location) = self
                        .seat
                        .get_pointer()
                        .map(|pointer| pointer.current_location())
                    {
                        self.pointer_press(button, location);
                    }
                } else {
                    self.end_grab();
                }

                if let Some(pointer) = self.seat.get_pointer() {
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
                self.pointer_axis(&event);
            }
            _ => {}
        }
    }

    /// Handles a pointer button press at `location`.
    ///
    /// An edge band starts a resize; the primary button on the decorated band
    /// off the client surface starts a move; over the client's own surface the
    /// press just raises and focuses the window. A press outside every open
    /// popup dismisses them first (leaf to root).
    fn pointer_press(&mut self, button: u32, location: Point<f64, Logical>) {
        // While the session is locked, presses target the lock surface only;
        // the protocol event is still forwarded to it by the caller. No window
        // may be raised, focused, moved or resized from under the lock.
        if self.locked {
            tracing::info!(?location, "press ignored: the session is locked");
            return;
        }
        // A press on an open popup targets the popup, not the window behind it.
        if self.windows.popup_under(location).is_some() {
            tracing::info!(?location, "press landed on a popup");
            return;
        }
        // A press outside every open popup closes them (Bloque 5).
        if self.windows.topmost_popup().is_some() {
            tracing::info!(?location, "press outside the popups dismissed them");
            self.dismiss_all_popups();
        }

        if let Some((id, edge)) = self.windows.edge_under(location, RESIZE_BORDER) {
            tracing::info!(?location, edge = ?edge, "press started a resize on the edge band");
            self.raise_and_focus(id);
            self.start_resize(id, edge, location);
            return;
        }

        let Some(id) = self.windows.window_under(location) else {
            tracing::info!(?location, "press landed outside every window");
            return;
        };
        self.raise_and_focus(id);

        if self.windows.surface_under(location).is_some() {
            tracing::info!(?location, window = ?id, "press landed on the client surface");
            return;
        }
        if button == BUTTON_LEFT {
            tracing::info!(?location, window = ?id, "press started a move on the decoration");
            self.start_move(id, location);
        }
    }

    /// Raises a window and gives it keyboard focus.
    pub(crate) fn raise_and_focus(&mut self, id: WindowId) {
        self.windows.raise(id);
        self.focus_window(id);
        self.request_redraw();
    }

    /// Gives keyboard focus to a window.
    pub(crate) fn focus_window(&mut self, id: WindowId) {
        // While the session is locked, focus belongs to the lock surface; a
        // window mapping or activating itself must not steal it (lock.rs).
        if self.locked {
            return;
        }
        let Some(surface) = self.windows.surface_of(id).cloned() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        match self.seat.get_keyboard() {
            Some(keyboard) => {
                tracing::info!("giving keyboard focus to a window");
                // The clipboard and primary-selection focuses follow the
                // keyboard focus: the focused client is the one that receives
                // the current selection and offers the next one (Block 6).
                let client = surface.client();
                keyboard.set_focus(self, Some(surface), serial);
                set_data_device_focus(&self.display_handle, &self.seat, client.clone());
                set_primary_focus(&self.display_handle, &self.seat, client);
            }
            None => tracing::warn!("a window was focused but the seat has no keyboard"),
        }
        // Publish the active window for the menu bar (the store the shell
        // watches, same as the wallpaper).
        self.publish_active(Some(id));
    }

    /// Drops keyboard focus entirely.
    pub(crate) fn clear_keyboard_focus(&mut self) {
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, None, serial);
            set_data_device_focus(&self.display_handle, &self.seat, None);
            set_primary_focus(&self.display_handle, &self.seat, None);
        }
        self.publish_active(None);
    }

    /// Publishes the active window (or its absence) to the shell's store.
    fn publish_active(&self, id: Option<WindowId>) {
        let info = id.and_then(|id| self.windows.active_info(id));
        let active = info.map(|(title, app_id)| crate::active::ActiveWindow { title, app_id });
        crate::active::publish(active.as_ref());
    }

    /// Removes every reference to a destroyed window and moves focus on.
    ///
    /// The window is removed from the scene (which also removes it from the
    /// stacking order and hit-testing), and any grab that named it is dropped.
    /// If it held keyboard focus, the focus passes to the new topmost window,
    /// or nowhere when none is left.
    pub(crate) fn forget_window(&mut self, surface: &WlSurface) {
        if self.leak_window(surface) {
            return;
        }
        let focused = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus());
        let Some(id) = self.windows.remove(surface) else {
            return;
        };
        self.foreign_window_destroyed(id);
        if self.grab.map(Grab::window) == Some(id) {
            self.grab = None;
        }
        tracing::info!(
            windows = self.windows.len(),
            topmost = ?self.windows.topmost(),
            "window removed"
        );
        if focused.as_ref() == Some(surface) {
            match self.windows.topmost() {
                Some(next) => self.focus_window(next),
                None => self.clear_keyboard_focus(),
            }
        }
        if self.windows.is_empty() {
            tracing::info!("scene empty");
        }
        self.request_redraw();
    }

    /// The implicit grab a client's move or resize request may act on.
    ///
    /// The protocol ties `xdg_toplevel.move`/`resize` to the pointer's
    /// implicit grab: the serial must name a button press that is still held.
    /// Smithay tracks exactly that: `DefaultGrab::button` installs a
    /// [`ClickGrab`] keyed by the press serial and drops it when the last
    /// button is released (`smithay-0.7.0/src/input/pointer/grab.rs:238-252`
    /// and `:374-380`). The check is therefore `has_grab(serial)` plus the
    /// grab's start data, as the reference compositor does
    /// (`smithay-0.7.0/anvil/src/shell/xdg.rs:152-160`).
    ///
    /// Focus changes, raises, configures and keyboard enter do not touch the
    /// grab, so none of them can invalidate a request made while the button is
    /// held. A request after the release names no grab and is rejected.
    pub(crate) fn held_grab(&self, serial: Serial) -> Option<GrabStartData<State>> {
        let pointer = self.seat.get_pointer()?;
        if !pointer.has_grab(serial) {
            return None;
        }
        pointer.grab_start_data()
    }

    /// Test-only: leave one window (and with it its surface) behind.
    ///
    /// Only when `YUKI_TEST_LEAK_WINDOW` is set. The first destroyed window,
    /// and every repeat of that same surface's destruction, is not removed, so
    /// the live-object counts cannot return to their baseline. The Block 4
    /// harness runs it once to show that its teardown assertion can fail.
    fn leak_window(&mut self, surface: &WlSurface) -> bool {
        if std::env::var_os("YUKI_TEST_LEAK_WINDOW").is_none() {
            return false;
        }
        let Some(id) = self.windows.id_of(surface) else {
            return false;
        };
        match self.leaked_window {
            None => self.leaked_window = Some(id),
            Some(leaked) if leaked == id => {}
            Some(_) => return false,
        }
        tracing::warn!("YUKI_TEST_LEAK_WINDOW: deliberately leaving a window behind");
        tracing::info!(
            windows = self.windows.len(),
            topmost = ?self.windows.topmost(),
            "window removed"
        );
        true
    }

    /// Test-only: press the primary button on the first window that maps.
    ///
    /// Only when `YUKI_TEST_INPUT` is set. The client-initiated
    /// move/resize path needs a real button press to supply a serial, which a
    /// probe cannot make on its own; this moves the pointer onto the window (at
    /// the fraction named by `YUKI_TEST_PRESS_AT`, the center by default),
    /// runs the compositor's real press path so click-to-focus happens, presses
    /// and holds. Several motions follow, so a client whose drag gesture waits
    /// for a threshold still sends its request while the button is held. The
    /// release is sent by [`Self::inject_test_release`].
    pub(crate) fn inject_test_press(&mut self, id: WindowId) {
        if std::env::var_os("YUKI_TEST_INPUT").is_none() || self.test_press.is_some() {
            return;
        }
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        let (fx, fy) = test_press_fraction();
        let location = Point::new(
            outer.x() + outer.width() * fx,
            outer.y() + outer.height() * fy,
        );
        if self.windows.surface_under(location).is_none() {
            return;
        }
        let time = InputTime::from_millis(0);
        // The hover goes through the real motion path, as a host pointer's
        // would, so the seam exercises the same focus-origin computation.
        self.pointer_motion(location, time);
        // The compositor's own press logic runs before the protocol event, as in
        // `process_input_event`: click-to-focus, and a move grab when the press
        // lands on Yuki's decoration rather than the client's surface.
        self.pointer_press(BUTTON_LEFT, location);
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let press = SERIAL_COUNTER.next_serial();
        pointer.button(
            self,
            &ButtonEvent {
                button: BUTTON_LEFT,
                state: ButtonState::Pressed,
                serial: press,
                time,
            },
        );
        pointer.frame(self);
        self.test_press = Some(TestPress {
            window: id,
            location,
        });
        let mut location = location;
        for _ in 0..TEST_DRAG_STEPS {
            location = Point::new(location.x + TEST_DRAG_STEP.0, location.y + TEST_DRAG_STEP.1);
            self.pointer_motion(location, time);
            if let Some(press) = self.test_press.as_mut() {
                press.location = location;
            }
        }
        // A press on an edge band or the decorated band starts a compositor
        // grab, and the scripted gesture is complete: end it now, or the grab
        // would stay held and a real host pointer moving over the window would
        // drag the window with it. A press on the client's surface starts no
        // grab, and the client's own move or resize request is still to come,
        // so the button stays held for it.
        if self.grab.is_some() {
            self.inject_test_release();
        }
    }

    /// Test-only: drag the pressed window by `YUKI_TEST_DRAG`.
    ///
    /// Called when the compositor accepts a client's move or resize request.
    /// The motion goes through the normal input path, so the grab that just
    /// started drives it, and the window's outer rectangle before and after is
    /// logged for a harness to assert. The button stays held, so a client can
    /// still make a second request with the same serial.
    pub(crate) fn inject_test_drag(&mut self, id: WindowId) {
        if std::env::var_os("YUKI_TEST_INPUT").is_none() {
            return;
        }
        let Some(press) = self.test_press else {
            return;
        };
        if press.window != id {
            return;
        }
        let (dx, dy) = test_drag_delta();
        let location = Point::new(press.location.x + dx, press.location.y + dy);
        let before = self.windows.outer(id);
        self.pointer_motion(location, InputTime::from_millis(0));
        if let Some(press) = self.test_press.as_mut() {
            press.location = location;
        }
        let after = self.windows.outer(id);
        if let (Some(before), Some(after)) = (before, after) {
            tracing::info!(
                from_x = before.x(),
                from_y = before.y(),
                from_width = before.width(),
                from_height = before.height(),
                to_x = after.x(),
                to_y = after.y(),
                to_width = after.width(),
                to_height = after.height(),
                "test drag applied"
            );
        }
    }

    /// Test-only: release the scripted press, ending any grab it started.
    pub(crate) fn inject_test_release(&mut self) {
        if self.test_press.take().is_none() {
            return;
        }
        self.end_grab();
        let time = InputTime::from_millis(0);
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        pointer.button(
            self,
            &ButtonEvent {
                button: BUTTON_LEFT,
                state: ButtonState::Released,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
        self.inject_test_hovers();
    }

    /// Test-only: hover the pointer over the points in `YUKI_TEST_HOVER`.
    ///
    /// The value is a `;`-separated list of global `x,y` points. They are
    /// injected with no button held, through the same motion path a host
    /// pointer takes, so a client can assert the surface-local coordinates it
    /// receives. They land after the scripted release, when a grab can no
    /// longer mask the focus.
    fn inject_test_hovers(&mut self) {
        for (x, y) in test_hover_points() {
            self.pointer_motion(Point::new(x, y), InputTime::from_millis(0));
        }
    }

    /// Starts dragging a window from its decoration.
    pub(crate) fn start_move(&mut self, id: WindowId, location: Point<f64, Logical>) {
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        let offset = ScenePoint::new(location.x - outer.x(), location.y - outer.y());
        self.grab = Some(Grab::Move { window: id, offset });
    }

    /// Starts resizing a window from an edge or corner.
    pub(crate) fn start_resize(&mut self, id: WindowId, edge: Edge, location: Point<f64, Logical>) {
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        self.windows.set_resizing(id, true);
        self.grab = Some(Grab::Resize {
            window: id,
            edge,
            start_pointer: ScenePoint::new(location.x, location.y),
            start_outer: outer,
        });
    }

    /// Ends an interactive move or resize.
    fn end_grab(&mut self) {
        if let Some(Grab::Resize { window, .. }) = self.grab {
            self.windows.set_resizing(window, false);
        }
        self.grab = None;
    }

    /// Moves the pointer, driving a grab or updating focus and the edge cursor.
    ///
    /// The focus Smithay gets is the surface's origin in global coordinates,
    /// not the surface-local point `surface_under` reports: Smithay subtracts
    /// the origin itself before sending the client the surface-local position
    /// (`smithay-0.7.0/src/input/pointer/mod.rs:220-243`).
    fn pointer_motion(&mut self, location: Point<f64, Logical>, time: InputTime) {
        if let Some(grab) = self.grab {
            self.drag(grab, location);
            self.request_redraw();
            return;
        }

        // While the session is locked the lock surface owns the pointer: it
        // covers the whole output, so the pointer's focus is always it, at the
        // origin. No edge cursor, no layer hit-testing.
        if self.locked {
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

        let serial = SERIAL_COUNTER.next_serial();
        let focus = self
            .windows
            .surface_under(location)
            .or_else(|| self.layers.surface_under(location))
            .map(|(surface, local)| {
                let origin = Point::new(location.x - local.x, location.y - local.y);
                (surface, origin)
            });
        if let Some(pointer) = self.seat.get_pointer() {
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
    }

    /// Advances an in-progress move or resize.
    fn drag(&mut self, grab: Grab, location: Point<f64, Logical>) {
        match grab {
            Grab::Move { window, offset } => {
                let target =
                    Point::from((round(location.x - offset.x), round(location.y - offset.y)));
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
                let outer = yuki_scene::scene::resized(start_outer, edge, dx, dy, minimum, maximum);
                self.windows.resize(window, outer);
            }
        }
    }

    /// Shows a resize cursor over an edge band, or restores the default.
    ///
    /// A client-set surface cursor is left alone: the override only replaces a
    /// named (compositor-drawn) cursor, which is what is active over decoration.
    fn update_edge_cursor(&mut self, location: Point<f64, Logical>) {
        match self.windows.edge_under(location, RESIZE_BORDER) {
            Some((_, edge)) => self.cursor_status = CursorImageStatus::Named(edge_cursor(edge)),
            None => {
                if matches!(self.cursor_status, CursorImageStatus::Named(_)) {
                    self.cursor_status = CursorImageStatus::default_named();
                }
            }
        }
    }

    /// Sends one scroll frame to the pointer.
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

        if let Some(pointer) = self.seat.get_pointer() {
            pointer.axis(self, frame);
            pointer.frame(self);
        }
    }
}

/// The cursor shown while an edge or corner is resized.
fn edge_cursor(edge: Edge) -> CursorIcon {
    use Edge;
    match edge {
        Edge::Top | Edge::Bottom => CursorIcon::NsResize,
        Edge::Left | Edge::Right => CursorIcon::EwResize,
        Edge::TopLeft | Edge::BottomRight => CursorIcon::NwseResize,
        Edge::TopRight | Edge::BottomLeft => CursorIcon::NeswResize,
    }
}
/// Rounds a logical position to whole pixels.
fn round(value: f64) -> i32 {
    value.round() as i32
}

/// The window fraction the scripted press lands on, from `YUKI_TEST_PRESS_AT`
/// (`x,y` as fractions of the window's outer size), the center by default.
///
/// The Nautilus drag needs the press on its headerbar, a few percent below the
/// top edge; the probes press the center.
fn test_press_fraction() -> (f64, f64) {
    parse_pair("YUKI_TEST_PRESS_AT").unwrap_or((0.5, 0.5))
}

/// The pointer delta the scripted drag applies once a grab starts, from
/// `YUKI_TEST_DRAG` (`dx,dy` in logical pixels), `(60, 40)` by default.
fn test_drag_delta() -> (f64, f64) {
    parse_pair("YUKI_TEST_DRAG").unwrap_or((60.0, 40.0))
}

/// The global points the scripted hover visits, from `YUKI_TEST_HOVER`
/// (`x,y` points separated by `;`), none by default.
fn test_hover_points() -> Vec<(f64, f64)> {
    let Ok(value) = std::env::var("YUKI_TEST_HOVER") else {
        return Vec::new();
    };
    value
        .split(';')
        .filter_map(parse_pair_value)
        .collect::<Vec<_>>()
}

/// Reads an `x,y` pair of numbers from the environment.
///
/// Any missing or unparsable value is `None`, so a malformed variable falls
/// back to the default rather than failing a run.
fn parse_pair(name: &str) -> Option<(f64, f64)> {
    parse_pair_value(&std::env::var(name).ok()?)
}

/// Reads an `x,y` pair of numbers from a string.
fn parse_pair_value(value: &str) -> Option<(f64, f64)> {
    let (x, y) = value.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use smithay::backend::input::ButtonState;
    use smithay::backend::input::{
        AbsolutePositionEvent, Device, DeviceCapability, Event, InputBackend, InputEvent,
        InputTime, PointerMotionAbsoluteEvent, UnusedEvent,
    };
    use smithay::input::pointer::{ButtonEvent, CursorIcon};
    use smithay::reexports::calloop::EventLoop;
    use smithay::reexports::wayland_server::Display;
    use smithay::utils::{Point, SERIAL_COUNTER, Serial, Size};

    use super::{BUTTON_LEFT, Edge, edge_cursor};
    use crate::state::State;

    #[test]
    fn each_edge_gets_its_own_resize_cursor() {
        assert_eq!(edge_cursor(Edge::Top), CursorIcon::NsResize);
        assert_eq!(edge_cursor(Edge::Bottom), CursorIcon::NsResize);
        assert_eq!(edge_cursor(Edge::Left), CursorIcon::EwResize);
        assert_eq!(edge_cursor(Edge::Right), CursorIcon::EwResize);
        assert_eq!(edge_cursor(Edge::TopLeft), CursorIcon::NwseResize);
        assert_eq!(edge_cursor(Edge::BottomRight), CursorIcon::NwseResize);
        assert_eq!(edge_cursor(Edge::TopRight), CursorIcon::NeswResize);
        assert_eq!(edge_cursor(Edge::BottomLeft), CursorIcon::NeswResize);
    }

    /// Sends one button event through the pointer.
    fn press_button(state: &mut State, state_: ButtonState, serial: Serial) {
        let pointer = state.seat.get_pointer().expect("the seat has a pointer");
        pointer.button(
            state,
            &ButtonEvent {
                button: BUTTON_LEFT,
                state: state_,
                serial,
                time: InputTime::from_millis(0),
            },
        );
        pointer.frame(state);
    }

    #[test]
    fn a_request_without_a_held_press_is_rejected() {
        let (_display, state) = test_state((1280, 720));
        assert!(
            state.held_grab(Serial::from(1u32)).is_none(),
            "no button was pressed, so no serial can name a grab"
        );
    }

    #[test]
    fn a_fabricated_serial_does_not_name_the_held_press() {
        let (_display, mut state) = test_state((1280, 720));
        press_button(&mut state, ButtonState::Pressed, Serial::from(7u32));
        assert!(state.held_grab(Serial::from(8u32)).is_none());
        assert!(
            state.held_grab(Serial::from(7u32)).is_some(),
            "the real press must still name the grab"
        );
    }

    #[test]
    fn a_released_press_no_longer_names_a_grab() {
        let (_display, mut state) = test_state((1280, 720));
        press_button(&mut state, ButtonState::Pressed, Serial::from(7u32));
        assert!(state.held_grab(Serial::from(7u32)).is_some());
        press_button(&mut state, ButtonState::Released, Serial::from(8u32));
        assert!(
            state.held_grab(Serial::from(7u32)).is_none(),
            "the release must end the grab the press started"
        );
    }

    #[test]
    fn a_held_press_survives_other_serial_traffic() {
        let (_display, mut state) = test_state((1280, 720));
        press_button(&mut state, ButtonState::Pressed, Serial::from(7u32));
        // Serials allocated for other events, as click-to-focus, a configure or
        // a keyboard enter would, do not touch the grab.
        for _ in 0..5 {
            let _ = SERIAL_COUNTER.next_serial();
        }
        assert!(state.held_grab(Serial::from(7u32)).is_some());
    }

    /// The virtual device of the test backend.
    #[derive(Debug, PartialEq, Eq, Hash)]
    struct TestDevice;

    impl Device for TestDevice {
        fn id(&self) -> String {
            "test".to_string()
        }

        fn name(&self) -> String {
            "test pointer".to_string()
        }

        fn has_capability(&self, _capability: DeviceCapability) -> bool {
            false
        }

        fn usb_id(&self) -> Option<(u32, u32)> {
            None
        }

        fn syspath(&self) -> Option<PathBuf> {
            None
        }
    }

    /// A backend whose only real event is an absolute pointer motion.
    ///
    /// The compositor's input handling is generic over [`InputBackend`], and the
    /// winit backend's event type cannot be constructed outside Smithay, so this
    /// backend stands in for it. Its motion event mirrors the winit contract
    /// exactly: the position is a fraction of the window with a top-left origin
    /// (`smithay-0.7.0/src/backend/winit/mod.rs:481-490`) and `x_transformed`
    /// multiplies that fraction by the target width
    /// (`smithay-0.7.0/src/backend/winit/input.rs:100-110`).
    struct TestBackend;

    impl InputBackend for TestBackend {
        type Device = TestDevice;
        type KeyboardKeyEvent = UnusedEvent;
        type PointerAxisEvent = UnusedEvent;
        type PointerButtonEvent = UnusedEvent;
        type PointerMotionEvent = UnusedEvent;
        type PointerMotionAbsoluteEvent = TestMouseMove;
        type GestureSwipeBeginEvent = UnusedEvent;
        type GestureSwipeUpdateEvent = UnusedEvent;
        type GestureSwipeEndEvent = UnusedEvent;
        type GesturePinchBeginEvent = UnusedEvent;
        type GesturePinchUpdateEvent = UnusedEvent;
        type GesturePinchEndEvent = UnusedEvent;
        type GestureHoldBeginEvent = UnusedEvent;
        type GestureHoldEndEvent = UnusedEvent;
        type TouchDownEvent = UnusedEvent;
        type TouchUpEvent = UnusedEvent;
        type TouchMotionEvent = UnusedEvent;
        type TouchCancelEvent = UnusedEvent;
        type TouchFrameEvent = UnusedEvent;
        type TabletToolAxisEvent = UnusedEvent;
        type TabletToolProximityEvent = UnusedEvent;
        type TabletToolTipEvent = UnusedEvent;
        type TabletToolButtonEvent = UnusedEvent;
        type SwitchToggleEvent = UnusedEvent;
        type SpecialEvent = ();
    }

    /// An absolute motion carrying winit's normalized, top-left-origin position.
    struct TestMouseMove {
        /// Position as a fraction of the window, `(x, y)`, top-left origin.
        fraction: (f64, f64),
    }

    impl Event<TestBackend> for TestMouseMove {
        fn time(&self) -> InputTime {
            InputTime::from_millis(0)
        }

        fn device(&self) -> TestDevice {
            TestDevice
        }
    }

    impl PointerMotionAbsoluteEvent<TestBackend> for TestMouseMove {}

    impl AbsolutePositionEvent<TestBackend> for TestMouseMove {
        fn x(&self) -> f64 {
            0.0
        }

        fn y(&self) -> f64 {
            0.0
        }

        fn x_transformed(&self, width: i32) -> f64 {
            f64::max(self.fraction.0 * f64::from(width), 0.0)
        }

        fn y_transformed(&self, height: i32) -> f64 {
            f64::max(self.fraction.1 * f64::from(height), 0.0)
        }
    }

    /// A compositor state with one output of `size`, built without a backend.
    fn test_state(size: (i32, i32)) -> (Display<State>, State) {
        use smithay::backend::allocator::{Format, Fourcc, Modifier};
        use smithay::wayland::dmabuf::DmabufFeedbackBuilder;

        // The loop's data type is the session's; only the handle is needed
        // here (it is a phantom in the type), so no session is constructed.
        let event_loop =
            EventLoop::<crate::session::Session>::try_new().expect("the test needs an event loop");
        let display = Display::<State>::new().expect("the test needs a display");
        // A fake device and one linear format: `State` requires a feedback
        // (there is no v3 fallback), but the tests never bind a client to it.
        let feedback = DmabufFeedbackBuilder::new(
            0,
            [Format {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::Invalid,
            }],
        )
        .build()
        .expect("the feedback should build");
        let state = State::new(
            event_loop.get_signal(),
            &display.handle(),
            Size::from(size),
            feedback,
            None,
            Box::new(event_loop.handle()),
        )
        .expect("the test state should build");
        (display, state)
    }

    /// Sends one absolute motion event with the given window fraction.
    fn move_pointer(state: &mut State, fraction: (f64, f64)) {
        state.process_input_event(InputEvent::<TestBackend>::PointerMotionAbsolute {
            event: TestMouseMove { fraction },
        });
    }

    #[test]
    fn host_pointer_corners_map_to_logical_corners() {
        let (_display, mut state) = test_state((1280, 720));
        let pointer = state.seat.get_pointer().expect("the seat has a pointer");
        assert_eq!(state.logical_output_size(), Size::from((1280, 720)));

        move_pointer(&mut state, (0.0, 0.0));
        assert_eq!(
            pointer.current_location(),
            Point::from((0.0, 0.0)),
            "the host's top-left must be logical (0, 0)"
        );

        move_pointer(&mut state, (1.0, 1.0));
        assert_eq!(
            pointer.current_location(),
            Point::from((1280.0, 720.0)),
            "the host's bottom-right must be the logical output size"
        );
    }

    #[test]
    fn host_pointer_moves_down_in_logical_space() {
        let (_display, mut state) = test_state((1280, 720));
        let pointer = state.seat.get_pointer().expect("the seat has a pointer");

        move_pointer(&mut state, (0.5, 0.25));
        let first = pointer.current_location();
        move_pointer(&mut state, (0.5, 0.75));
        let second = pointer.current_location();

        assert!(
            second.y > first.y,
            "moving the host pointer down must increase logical y ({first:?} -> {second:?})"
        );
        assert_eq!(first.x, second.x);
    }
}
