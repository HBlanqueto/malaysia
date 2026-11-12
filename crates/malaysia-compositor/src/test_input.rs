use std::collections::VecDeque;
use std::sync::LazyLock;
use std::time::Duration;

use malaysia_scene::scene::WindowId;
use smithay::backend::input::{ButtonState, InputTime, KeyState};
use smithay::input::pointer::ButtonEvent;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Size};
use xkbcommon::xkb::Keycode;

use crate::input::{BUTTON_LEFT, clamp_pointer, parse_ms};
use crate::state::{ScreenshotRequest, State, TestPress};

pub(crate) const TEST_POINTER_STEP_DEFAULT: Duration = Duration::from_millis(100);
pub(crate) const TEST_POINTER_STEP_MS_VAR: &str = "MALAYSIA_TEST_POINTER_STEP_MS";
pub(crate) static TEST_POINTER_STEP: LazyLock<Duration> = LazyLock::new(|| {
    parse_ms(
        std::env::var(TEST_POINTER_STEP_MS_VAR).ok().as_deref(),
        TEST_POINTER_STEP_DEFAULT,
    )
});
pub(crate) const TEST_CLICK_HOLD: Duration = Duration::from_millis(150);
pub(crate) const TEST_KEY_HOLD: Duration = Duration::from_millis(20);

#[derive(Debug, Default)]
pub(crate) struct TestPointer {
    pub(crate) actions: VecDeque<TestPointerAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestKey {
    Up,
    Down,
    Left,
    Right,
    Return,
    Escape,
    Home,
    End,
    Space,
    Ctrl,
    Shift,
    Alt,
    Super,
    Char(char),
}

impl TestKey {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        let mut chars = name.chars();
        if let (Some(ch), None) = (chars.next(), chars.next())
            && ch.is_ascii_alphanumeric()
        {
            return Some(Self::Char(ch.to_ascii_lowercase()));
        }
        match name {
            "Up" => Some(Self::Up),
            "Down" => Some(Self::Down),
            "Left" => Some(Self::Left),
            "Right" => Some(Self::Right),
            "Return" => Some(Self::Return),
            "Escape" => Some(Self::Escape),
            "Home" => Some(Self::Home),
            "End" => Some(Self::End),
            "Space" => Some(Self::Space),
            _ => None,
        }
    }

    pub(crate) fn code(self) -> Keycode {
        Keycode::new(match self {
            Self::Up => 111,
            Self::Down => 116,
            Self::Left => 113,
            Self::Right => 114,
            Self::Return => 36,
            Self::Escape => 9,
            Self::Home => 110,
            Self::End => 115,
            Self::Space => 65,
            Self::Ctrl => 37,
            Self::Shift => 50,
            Self::Alt => 64,
            Self::Super => 133,
            Self::Char(ch) => ascii_keycode(ch),
        })
    }
}

pub(crate) fn parse_key_actions(rest: &str, actions: &mut VecDeque<TestPointerAction>) {
    let tokens: Vec<&str> = rest
        .split('+')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    let mut modifiers = Vec::new();
    let mut key = None;
    for (i, token) in tokens.iter().enumerate() {
        let lower = token.to_ascii_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => modifiers.push(TestKey::Ctrl),
            "shift" => modifiers.push(TestKey::Shift),
            "alt" => modifiers.push(TestKey::Alt),
            "super" | "meta" | "cmd" | "command" => modifiers.push(TestKey::Super),
            _ => {
                if i == tokens.len() - 1 {
                    key = TestKey::from_name(token);
                } else {
                    key = TestKey::from_name(rest.trim());
                    modifiers.clear();
                    break;
                }
            }
        }
    }
    let Some(key) = key else { return };
    for modifier in &modifiers {
        actions.push_back(TestPointerAction::KeyDown(*modifier));
        actions.push_back(TestPointerAction::Wait(TEST_KEY_HOLD));
    }
    actions.push_back(TestPointerAction::KeyDown(key));
    actions.push_back(TestPointerAction::Wait(TEST_KEY_HOLD));
    actions.push_back(TestPointerAction::KeyUp(key));
    actions.push_back(TestPointerAction::Wait(TEST_KEY_HOLD));
    for modifier in modifiers.iter().rev() {
        actions.push_back(TestPointerAction::KeyUp(*modifier));
        actions.push_back(TestPointerAction::Wait(TEST_KEY_HOLD));
    }
}

pub(crate) fn ascii_keycode(ch: char) -> u32 {
    let evdev = match ch {
        'q' => 16,
        'w' => 17,
        'e' => 18,
        'r' => 19,
        't' => 20,
        'y' => 21,
        'u' => 22,
        'i' => 23,
        'o' => 24,
        'p' => 25,
        'a' => 30,
        's' => 31,
        'd' => 32,
        'f' => 33,
        'g' => 34,
        'h' => 35,
        'j' => 36,
        'k' => 37,
        'l' => 38,
        'z' => 44,
        'x' => 45,
        'c' => 46,
        'v' => 47,
        'b' => 48,
        'n' => 49,
        'm' => 50,
        '1' => 2,
        '2' => 3,
        '3' => 4,
        '4' => 5,
        '5' => 6,
        '6' => 7,
        '7' => 8,
        '8' => 9,
        '9' => 10,
        '0' => 11,
        _ => unreachable!("ascii_keycode only accepts ASCII alphanumerics"),
    };
    evdev + 8
}

#[derive(Debug, PartialEq)]
pub(crate) enum TestPointerAction {
    Move(Point<f64, Logical>),
    Press(Point<f64, Logical>),
    Release,
    Wait(Duration),
    KeyDown(TestKey),
    KeyUp(TestKey),
}

impl TestPointer {
    pub(crate) fn from_env() -> Option<Self> {
        Self::parse(std::env::var("MALAYSIA_TEST_INPUT").ok()?.as_str())
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        let mut actions = VecDeque::new();
        for part in value
            .split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            if part == "1" {
                continue;
            }
            if let Some(rest) = part.strip_prefix("move:") {
                if let Some((x, y)) = parse_pair_value(rest) {
                    actions.push_back(TestPointerAction::Move(Point::new(x, y)));
                }
            } else if let Some(rest) = part.strip_prefix("press:") {
                if let Some((x, y)) = parse_pair_value(rest) {
                    let point = Point::new(x, y);
                    actions.push_back(TestPointerAction::Move(point));
                    actions.push_back(TestPointerAction::Press(point));
                }
            } else if part.strip_prefix("release:").is_some() {
                actions.push_back(TestPointerAction::Release);
            } else if let Some(rest) = part.strip_prefix("click:") {
                if let Some((x, y)) = parse_pair_value(rest) {
                    let point = Point::new(x, y);
                    actions.push_back(TestPointerAction::Move(point));
                    actions.push_back(TestPointerAction::Press(point));
                    actions.push_back(TestPointerAction::Wait(TEST_CLICK_HOLD));
                    actions.push_back(TestPointerAction::Release);
                }
            } else if let Some(rest) = part.strip_prefix("doubleclick:") {
                if let Some((x, y)) = parse_pair_value(rest) {
                    let point = Point::new(x, y);
                    actions.push_back(TestPointerAction::Move(point));
                    actions.push_back(TestPointerAction::Press(point));
                    actions.push_back(TestPointerAction::Wait(TEST_CLICK_HOLD));
                    actions.push_back(TestPointerAction::Release);
                    actions.push_back(TestPointerAction::Press(point));
                    actions.push_back(TestPointerAction::Wait(TEST_CLICK_HOLD));
                    actions.push_back(TestPointerAction::Release);
                }
            } else if let Some(rest) = part.strip_prefix("wait:")
                && let Ok(millis) = rest.trim().parse::<u64>()
            {
                actions.push_back(TestPointerAction::Wait(Duration::from_millis(millis)));
            } else if let Some(rest) = part.strip_prefix("key:") {
                parse_key_actions(rest.trim(), &mut actions);
            }
        }
        (!actions.is_empty()).then_some(Self { actions })
    }
}

pub(crate) fn test_pointer_delay() -> Duration {
    *TEST_POINTER_STEP
}
pub(crate) const TEST_DRAG_STEPS: usize = 4;
pub(crate) const TEST_DRAG_STEP: (f64, f64) = (12.0, 8.0);
pub(crate) fn test_press_fraction() -> (f64, f64) {
    parse_pair("MALAYSIA_TEST_PRESS_AT").unwrap_or((0.5, 0.5))
}

pub(crate) fn test_titlebar_button() -> Option<ctdesign::ButtonKind> {
    match std::env::var("MALAYSIA_TEST_TITLEBAR_BUTTON")
        .ok()?
        .as_str()
    {
        "close" => Some(ctdesign::ButtonKind::Close),
        "minimize" => Some(ctdesign::ButtonKind::Minimize),
        "maximize" => Some(ctdesign::ButtonKind::Maximize),
        _ => None,
    }
}

pub(crate) fn test_drag_delta() -> (f64, f64) {
    parse_pair("MALAYSIA_TEST_DRAG").unwrap_or((60.0, 40.0))
}

pub(crate) fn test_hover_points() -> Vec<(f64, f64)> {
    let Ok(value) = std::env::var("MALAYSIA_TEST_HOVER") else {
        return Vec::new();
    };
    value
        .split(';')
        .filter_map(parse_pair_value)
        .collect::<Vec<_>>()
}

pub(crate) fn parse_pair(name: &str) -> Option<(f64, f64)> {
    parse_pair_value(&std::env::var(name).ok()?)
}

pub(crate) fn parse_pair_value(value: &str) -> Option<(f64, f64)> {
    let (x, y) = value.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

impl State {
    pub(crate) fn test_pointer_step(&mut self) -> Option<Duration> {
        let action = self.test_pointer.as_mut()?.actions.pop_front()?;
        tracing::info!(?action, "test input action");
        let delay = match &action {
            TestPointerAction::Wait(delay) => *delay,
            TestPointerAction::KeyDown(_) | TestPointerAction::KeyUp(_) => Duration::ZERO,
            _ => *TEST_POINTER_STEP,
        };
        let time = InputTime::from_millis(0);
        match action {
            TestPointerAction::Move(location) => {
                self.pointer_motion(location, time);
                self.request_redraw();
            }
            TestPointerAction::Press(location) => {
                self.test_pointer_press(location, time);
            }
            TestPointerAction::Release => {
                self.test_pointer_release(time);
            }
            TestPointerAction::KeyDown(key) => {
                self.keyboard_key(key.code(), KeyState::Pressed, time);
            }
            TestPointerAction::KeyUp(key) => {
                self.keyboard_key(key.code(), KeyState::Released, time);
            }
            TestPointerAction::Wait(_) => {}
        }
        Some(delay)
    }

    pub(crate) fn test_pointer_press(&mut self, location: Point<f64, Logical>, time: InputTime) {
        self.pointer_press(BUTTON_LEFT, location, time);
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        self.note_activation_serial(serial);
        if let Some(focus) = pointer.current_focus() {
            self.note_client_input_serial(&focus, serial);
        }
        pointer.button(
            self,
            &ButtonEvent {
                button: BUTTON_LEFT,
                state: ButtonState::Pressed,
                serial,
                time,
            },
        );
        pointer.frame(self);
        self.request_redraw();
    }

    pub(crate) fn test_pointer_release(&mut self, time: InputTime) {
        self.chrome_release();
        self.end_grab();
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(focus) = pointer.current_focus() {
            self.note_client_input_serial(&focus, serial);
        }
        pointer.button(
            self,
            &ButtonEvent {
                button: BUTTON_LEFT,
                state: ButtonState::Released,
                serial,
                time,
            },
        );
        pointer.frame(self);
        self.inject_test_hovers();
        self.request_redraw();
    }

    pub(crate) fn inject_test_press(&mut self, id: WindowId) {
        if std::env::var_os("MALAYSIA_TEST_INPUT").is_none()
            || self.test_press.is_some()
            || self.test_pointer.is_some()
        {
            return;
        }
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        let titlebar_button = test_titlebar_button();
        let location = if let Some(kind) = titlebar_button {
            let Some(insets) = self.windows.insets(id) else {
                return;
            };
            let Some(center) = crate::chrome::button_center(outer, insets, kind) else {
                return;
            };
            center
        } else {
            let (fx, fy) = test_press_fraction();
            Point::new(
                outer.x() + outer.width() * fx,
                outer.y() + outer.height() * fy,
            )
        };
        let time = InputTime::from_millis(0);
        self.pointer_motion(location, time);
        self.pointer_press(BUTTON_LEFT, location, time);
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let press = SERIAL_COUNTER.next_serial();
        self.note_activation_serial(press);
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
        if titlebar_button.is_none() {
            let mut location = location;
            for _ in 0..TEST_DRAG_STEPS {
                location = Point::new(location.x + TEST_DRAG_STEP.0, location.y + TEST_DRAG_STEP.1);
                self.pointer_motion(location, time);
                if let Some(press) = self.test_press.as_mut() {
                    press.location = location;
                }
            }
        }
        if self.grab.is_some() || titlebar_button.is_some() {
            self.inject_test_release();
        }
    }

    pub(crate) fn inject_test_drag(&mut self, id: WindowId) {
        if std::env::var_os("MALAYSIA_TEST_INPUT").is_none() {
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

    pub(crate) fn inject_test_release(&mut self) {
        let Some(press) = self.test_press.take() else {
            return;
        };
        self.chrome_release_at(press.location);
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

    pub(crate) fn inject_test_screenshot(&mut self) {
        let Ok(mode) = std::env::var("MALAYSIA_TEST_SCREENSHOT") else {
            return;
        };
        let request = if mode == "region" {
            ScreenshotRequest::Region(Rectangle::new(
                Point::from((100, 100)),
                Size::from((300, 200)),
            ))
        } else {
            ScreenshotRequest::Focused
        };
        self.request_screenshot(request);
    }

    pub(crate) fn inject_test_shake(&mut self) {
        if std::env::var_os("MALAYSIA_TEST_SHAKE").is_none() {
            return;
        }
        let Some(location) = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
        else {
            return;
        };
        let size = self.logical_output_size();
        let center = clamp_pointer(location, size);
        let y = center.y;
        for x in [
            center.x - 60.0,
            center.x + 60.0,
            center.x - 60.0,
            center.x + 60.0,
            center.x,
        ] {
            let point = clamp_pointer(Point::new(x, y), size);
            self.pointer_motion(point, InputTime::from_millis(0));
        }
    }

    pub(crate) fn inject_test_hovers(&mut self) {
        for (x, y) in test_hover_points() {
            self.pointer_motion(Point::new(x, y), InputTime::from_millis(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TEST_KEY_HOLD, TestKey, TestPointer, TestPointerAction};
    use xkbcommon::xkb::Keycode;

    #[test]
    fn scripted_text_keys_map_to_ascii_keycodes() {
        assert_eq!(TestKey::from_name("w").unwrap().code(), Keycode::new(25));
        assert_eq!(TestKey::from_name("7").unwrap().code(), Keycode::new(16));
        assert!(TestKey::from_name("ocul").is_none());
    }

    #[test]
    fn scripted_modifiers_press_in_order_and_release_in_reverse() {
        let pointer = TestPointer::parse("key:ctrl+shift+s").unwrap();
        let actions: Vec<_> = pointer.actions.into_iter().collect();
        assert_eq!(
            actions,
            vec![
                TestPointerAction::KeyDown(TestKey::Ctrl),
                TestPointerAction::Wait(TEST_KEY_HOLD),
                TestPointerAction::KeyDown(TestKey::Shift),
                TestPointerAction::Wait(TEST_KEY_HOLD),
                TestPointerAction::KeyDown(TestKey::Char('s')),
                TestPointerAction::Wait(TEST_KEY_HOLD),
                TestPointerAction::KeyUp(TestKey::Char('s')),
                TestPointerAction::Wait(TEST_KEY_HOLD),
                TestPointerAction::KeyUp(TestKey::Shift),
                TestPointerAction::Wait(TEST_KEY_HOLD),
                TestPointerAction::KeyUp(TestKey::Ctrl),
                TestPointerAction::Wait(TEST_KEY_HOLD),
            ]
        );
    }
}
