use std::path::PathBuf;
use std::time::Duration;

use smithay::backend::input::ButtonState;

use smithay::backend::input::{
    AbsolutePositionEvent, Device, DeviceCapability, Event, InputBackend, InputEvent, InputTime,
    PointerMotionAbsoluteEvent, UnusedEvent,
};
use smithay::input::pointer::{ButtonEvent, CursorIcon};
use smithay::reexports::wayland_server::Display;
use smithay::utils::{Point, SERIAL_COUNTER, Serial, Size};

use super::{
    BUTTON_LEFT, Edge, clamp_pointer, confined_step, double_click, edge_cursor, parse_ms,
    pointer_focus_blocked,
};
use crate::state::State;
use crate::test_input::TEST_POINTER_STEP_DEFAULT;
use malaysia_scene::scene::WindowId;

#[test]
fn relative_motion_is_clamped_to_the_output_on_all_four_edges() {
    let size = Size::from((1280, 720));
    assert_eq!(
        clamp_pointer(Point::from((-10.0, 100.0)), size),
        Point::from((0.0, 100.0))
    );
    assert_eq!(
        clamp_pointer(Point::from((100.0, -10.0)), size),
        Point::from((100.0, 0.0))
    );
    assert_eq!(
        clamp_pointer(Point::from((2000.0, 100.0)), size),
        Point::from((1279.99, 100.0))
    );
    assert_eq!(
        clamp_pointer(Point::from((100.0, 2000.0)), size),
        Point::from((100.0, 719.99))
    );
    assert_eq!(
        clamp_pointer(Point::from((100.0, 100.0)), size),
        Point::from((100.0, 100.0))
    );
}

#[test]
fn only_the_latest_key_or_button_serial_activates() {
    let (_display, mut state) = test_state((1280, 720));
    let serial = SERIAL_COUNTER.next_serial();
    let other = SERIAL_COUNTER.next_serial();
    state.note_activation_serial(serial);
    assert!(state.activation_serial_is_recent(serial));
    assert!(!state.activation_serial_is_recent(other));
}

#[test]
fn a_shake_starts_the_pointer_animation() {
    let (_display, mut state) = test_state((1280, 720));
    state.trigger_cursor_boost();
    assert!(state.cursor_boost_started.is_some());
    let scale = state.cursor_boost_scale();
    assert!((1.0..=ctdesign::CURSOR_SHAKE_SCALE).contains(&scale));
    state.cursor_boost_started = None;
    assert_eq!(state.cursor_boost_scale(), 1.0);
}

#[test]
fn the_pointer_grows_before_the_shake_holds_it() {
    let (_display, mut state) = test_state((1280, 720));
    let now = std::time::Instant::now();
    state.cursor_boost_started = Some(now);
    state.shake.last_shake = Some(now);
    state.keep_cursor_boost_alive(now);
    let scale = state.cursor_boost_scale();
    assert!(
        scale < 1.5,
        "the growth was skipped: the pointer jumped to {scale}"
    );
}

#[test]
fn a_layer_over_a_band_keeps_pointer_focus() {
    assert!(!pointer_focus_blocked(
        true,
        Some(WindowId::new(1)),
        Some(WindowId::new(2))
    ));
}

#[test]
fn a_band_over_another_window_clears_pointer_focus() {
    let edge = Some(WindowId::new(1));
    assert!(pointer_focus_blocked(false, edge, Some(WindowId::new(2))));
    assert!(pointer_focus_blocked(false, edge, None));
    assert!(!pointer_focus_blocked(false, edge, edge));
}

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
    for _ in 0..5 {
        let _ = SERIAL_COUNTER.next_serial();
    }
    assert!(state.held_grab(Serial::from(7u32)).is_some());
}

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

struct TestMouseMove {
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

fn test_state(size: (i32, i32)) -> (Display<State>, State) {
    let (_, display, state) = crate::test_support::test_state(size);
    (display, state)
}

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
#[test]
fn a_second_press_within_the_double_click_interval_zooms() {
    let id = WindowId::new(1);
    let point = Point::from((200.0, 70.0));
    let (previous, action) = double_click(
        None,
        id,
        point,
        1_000,
        crate::config::DoubleClickAction::Zoom,
    );
    assert_eq!(action, None);
    let (_, action) = double_click(
        previous,
        id,
        point,
        1_300,
        crate::config::DoubleClickAction::Zoom,
    );
    assert_eq!(action, Some(crate::config::DoubleClickAction::Zoom));
}

#[test]
fn a_second_press_too_late_is_not_a_double_click() {
    let id = WindowId::new(1);
    let point = Point::from((200.0, 70.0));
    let (previous, _) = double_click(
        None,
        id,
        point,
        1_000,
        crate::config::DoubleClickAction::Zoom,
    );
    let (_, action) = double_click(
        previous,
        id,
        point,
        1_500,
        crate::config::DoubleClickAction::Zoom,
    );
    assert_eq!(action, None);
}

#[test]
fn a_second_press_far_from_the_first_is_not_a_double_click() {
    let id = WindowId::new(1);
    let (previous, _) = double_click(
        None,
        id,
        Point::from((200.0, 70.0)),
        1_000,
        crate::config::DoubleClickAction::Zoom,
    );
    let (_, action) = double_click(
        previous,
        id,
        Point::from((220.0, 70.0)),
        1_100,
        crate::config::DoubleClickAction::Zoom,
    );
    assert_eq!(action, None);
}
#[test]
fn releasing_a_titlebar_button_outside_does_not_fire_it() {
    let mut pointer = ctappearance::traffic::TrafficPointer::default();
    pointer.on_press(Some(ctdesign::ButtonKind::Close), true);
    assert_eq!(pointer.on_release(None, false), None);
}

#[test]
fn pressing_and_releasing_the_same_titlebar_button_fires_it() {
    let mut pointer = ctappearance::traffic::TrafficPointer::default();
    pointer.on_press(Some(ctdesign::ButtonKind::Close), true);
    assert_eq!(
        pointer.on_release(Some(ctdesign::ButtonKind::Close), true),
        Some(ctdesign::ButtonKind::Close)
    );
}

#[test]
fn a_confined_pointer_slides_along_the_edge_it_cannot_cross() {
    let inside = |point: Point<f64, _>| point.x < 10.0 && point.y < 10.0;
    let from = Point::new(8.0, 8.0);
    assert_eq!(
        confined_step(from, Point::new(12.0, 9.0), inside),
        Point::new(8.0, 9.0)
    );
    assert_eq!(
        confined_step(from, Point::new(9.0, 12.0), inside),
        Point::new(9.0, 8.0)
    );
    assert_eq!(confined_step(from, Point::new(12.0, 12.0), inside), from);
}

#[test]
fn the_traffic_lights_are_not_the_double_click_title_area() {
    let (x, y, size) = ctappearance::traffic::button_rect(0);
    assert_eq!(
        ctappearance::traffic::hit((x + size / 2.0, y + size / 2.0)),
        Some(ctdesign::ButtonKind::Close)
    );
}

#[test]
fn only_a_fixed_width_window_maximizes_vertically() {
    use malaysia_scene::geometry::{Point as ScenePoint, Rect as SceneRect, Size as SceneSize};

    let outer = SceneRect::new(ScenePoint::new(300.0, 100.0), SceneSize::new(760.0, 620.0));
    let workspace = (Point::from((0, 0)), Size::from((1920, 1052)));
    let xywh = |fixed_width| {
        let target = super::maximize_target_rect(fixed_width, workspace, outer);
        (target.x(), target.y(), target.width(), target.height())
    };
    assert_eq!(xywh(true), (300.0, 0.0, 760.0, 1052.0));
    assert_eq!(xywh(false), (0.0, 0.0, 1920.0, 1052.0));
}

#[test]
fn a_dragged_maximized_window_regains_its_size_under_the_pointer() {
    use malaysia_scene::geometry::{Point as ScenePoint, Rect as SceneRect, Size as SceneSize};

    let maximized = SceneRect::new(ScenePoint::new(0.0, 28.0), SceneSize::new(1920.0, 1052.0));
    let saved = SceneRect::new(ScenePoint::new(300.0, 100.0), SceneSize::new(800.0, 600.0));
    let restored = super::restored_under_pointer(maximized, saved, 1440.0);
    assert_eq!(restored.width(), 800.0);
    assert_eq!(restored.height(), 600.0);
    assert_eq!(restored.x(), 1440.0 - 600.0);
    assert_eq!(restored.y(), 28.0);
}

#[test]
fn fixed_width_needs_equal_nonzero_min_and_max_widths() {
    assert!(super::is_fixed_width(
        Size::from((760, 620)),
        Size::from((760, 0))
    ));
    assert!(!super::is_fixed_width(
        Size::from((0, 620)),
        Size::from((0, 0))
    ));
    assert!(!super::is_fixed_width(
        Size::from((760, 620)),
        Size::from((0, 0))
    ));
}

#[test]
fn a_test_delay_is_at_least_a_millisecond_and_falls_back_to_the_default() {
    let default = TEST_POINTER_STEP_DEFAULT;
    assert_eq!(parse_ms(Some("8"), default), Duration::from_millis(8));
    assert_eq!(parse_ms(Some("0"), default), Duration::from_millis(1));
    for value in [None, Some(""), Some("fast")] {
        assert_eq!(parse_ms(value, default), default);
    }
}
