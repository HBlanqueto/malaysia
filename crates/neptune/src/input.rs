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

use neptune_scene::geometry::Point as ScenePoint;
use neptune_scene::scene::{Edge, WindowId};
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
};
use smithay::input::keyboard::FilterResult;
use smithay::input::pointer::{AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, MotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER, Serial};

use crate::state::{Grab, State};
use crate::window::RESIZE_BORDER;

/// The Linux button code for the primary pointer button.
const BUTTON_LEFT: u32 = 0x110;

impl State {
    /// Handles one input event from the backend.
    pub(crate) fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.input::<(), _>(
                        self,
                        event.key_code(),
                        event.state(),
                        serial,
                        time,
                        |_, _, _| FilterResult::Forward,
                    );
                }
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let location = event
                    .position_transformed(self.logical_output_size())
                    .to_f64();
                self.pointer_motion(location, event.time_msec());
            }
            InputEvent::PointerButton { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let button = event.button_code();
                let state = event.state();

                if state == ButtonState::Pressed {
                    self.pointer_press_serial = Some(serial);
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
                            time: event.time_msec(),
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
    /// press just raises and focuses the window.
    fn pointer_press(&mut self, button: u32, location: Point<f64, Logical>) {
        if let Some((id, edge)) = self.windows.edge_under(location, RESIZE_BORDER) {
            self.raise_and_focus(id);
            self.start_resize(id, edge, location);
            return;
        }

        let Some(id) = self.windows.window_under(location) else {
            return;
        };
        self.raise_and_focus(id);

        if self.windows.surface_under(location).is_some() {
            return;
        }
        if button == BUTTON_LEFT {
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
        let Some(surface) = self.windows.surface_of(id).cloned() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        match self.seat.get_keyboard() {
            Some(keyboard) => {
                tracing::info!("giving keyboard focus to a window");
                keyboard.set_focus(self, Some(surface), serial);
            }
            None => tracing::warn!("a window was focused but the seat has no keyboard"),
        }
    }

    /// Drops keyboard focus entirely.
    pub(crate) fn clear_keyboard_focus(&mut self) {
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, None, serial);
        }
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

    /// Consumes the remembered button press serial if `serial` is it.
    ///
    /// Used to validate `xdg_toplevel.move`/`resize`: a request must name a
    /// press this compositor actually saw, and can only be used once.
    pub(crate) fn take_pointer_press(&mut self, serial: Serial) -> bool {
        consume(&mut self.pointer_press_serial, serial)
    }

    /// Test-only: leave one window (and with it its surface) behind.
    ///
    /// Only when `NEPTUNE_TEST_LEAK_WINDOW` is set. The first destroyed window,
    /// and every repeat of that same surface's destruction, is not removed, so
    /// the live-object counts cannot return to their baseline. The Block 4
    /// harness runs it once to show that its teardown assertion can fail.
    fn leak_window(&mut self, surface: &WlSurface) -> bool {
        if std::env::var_os("NEPTUNE_TEST_LEAK_WINDOW").is_none() {
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
        tracing::warn!("NEPTUNE_TEST_LEAK_WINDOW: deliberately leaving a window behind");
        tracing::info!(
            windows = self.windows.len(),
            topmost = ?self.windows.topmost(),
            "window removed"
        );
        true
    }

    /// Test-only: inject one pointer press and release over a window.
    ///
    /// Only when `NEPTUNE_TEST_INPUT` is set. The client-initiated
    /// move/resize path needs a real button press to supply a serial, which a
    /// probe cannot make on its own; this moves the pointer over the window,
    /// presses and releases, and remembers the press serial so a client request
    /// can be validated against it.
    pub(crate) fn inject_test_press(&mut self, id: WindowId) {
        if std::env::var_os("NEPTUNE_TEST_INPUT").is_none() {
            return;
        }
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        let center = Point::new(
            outer.x() + outer.width() / 2.0,
            outer.y() + outer.height() / 2.0,
        );
        let Some((surface, local)) = self.windows.surface_under(center) else {
            return;
        };
        let origin = Point::new(center.x - local.x, center.y - local.y);
        let time = 0;
        let serial = SERIAL_COUNTER.next_serial();
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        pointer.motion(
            self,
            Some((surface, origin)),
            &MotionEvent {
                location: center,
                serial,
                time,
            },
        );
        pointer.frame(self);
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
        self.pointer_press_serial = Some(press);
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
    fn pointer_motion(&mut self, location: Point<f64, Logical>, time: u32) {
        if let Some(grab) = self.grab {
            self.drag(grab, location);
            self.request_redraw();
            return;
        }

        self.update_edge_cursor(location);

        let serial = SERIAL_COUNTER.next_serial();
        let focus = self.windows.surface_under(location);
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
                let outer =
                    neptune_scene::scene::resized(start_outer, edge, dx, dy, minimum, maximum);
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

        let mut frame = AxisFrame::new(event.time_msec()).source(source);
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

/// Consumes `remembered` when it holds `serial`, clearing it either way.
///
/// This is the whole of the `xdg_toplevel.move`/`resize` serial check: a
/// request is honoured only if its serial is the last press seen, and that press
/// cannot authorise a second request.
fn consume(remembered: &mut Option<Serial>, serial: Serial) -> bool {
    if *remembered == Some(serial) {
        *remembered = None;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use smithay::backend::input::{
        AbsolutePositionEvent, Device, DeviceCapability, Event, InputBackend, InputEvent,
        PointerMotionAbsoluteEvent, UnusedEvent,
    };
    use smithay::input::pointer::CursorIcon;
    use smithay::reexports::calloop::EventLoop;
    use smithay::reexports::wayland_server::Display;
    use smithay::utils::{Logical, Point, Serial, Size};

    use super::{Edge, consume, edge_cursor};
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

    #[test]
    fn a_matching_press_serial_is_accepted_once() {
        let mut remembered = Some(Serial::from(7u32));
        assert!(consume(&mut remembered, Serial::from(7u32)));
        assert_eq!(remembered, None);
        assert!(!consume(&mut remembered, Serial::from(7u32)));
    }

    #[test]
    fn a_foreign_press_serial_is_rejected_and_kept() {
        let mut remembered = Some(Serial::from(7u32));
        assert!(!consume(&mut remembered, Serial::from(8u32)));
        assert_eq!(remembered, Some(Serial::from(7u32)));
    }

    #[test]
    fn a_request_with_no_press_behind_it_is_rejected() {
        let mut remembered = None;
        assert!(!consume(&mut remembered, Serial::from(1u32)));
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
        fn time(&self) -> u64 {
            0
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
        let event_loop = EventLoop::<()>::try_new().expect("the test needs an event loop");
        let display = Display::<State>::new().expect("the test needs a display");
        let state = State::new(event_loop.get_signal(), &display.handle(), Size::from(size))
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
