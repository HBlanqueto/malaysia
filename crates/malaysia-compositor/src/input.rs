//! Translates backend input events into seat events.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

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
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Serial, Size};
use smithay::wayland::pointer_constraints::{PointerConstraint, with_pointer_constraint};
use smithay::wayland::selection::data_device::set_data_device_focus;
use smithay::wayland::selection::primary_selection::set_primary_focus;
use smithay::wayland::shell::wlr_layer::LayerSurface;
use vow_design::TITLEBAR;
use xkbcommon::xkb::{self, Keycode, keysyms};

use crate::infinity::protocol::infinity_window_manager_v1::{Layout, PairLayout};
use crate::state::{Grab, ScreenshotRequest, State, TestPress};

const BUTTON_LEFT: u32 = 0x110;
const TEST_POINTER_STEP_DEFAULT: Duration = Duration::from_millis(100);
const TEST_POINTER_STEP_MS_VAR: &str = "MALAYSIA_TEST_POINTER_STEP_MS";
static TEST_POINTER_STEP: LazyLock<Duration> = LazyLock::new(|| {
    parse_ms(
        std::env::var(TEST_POINTER_STEP_MS_VAR).ok().as_deref(),
        TEST_POINTER_STEP_DEFAULT,
    )
});
const TEST_CLICK_HOLD: Duration = Duration::from_millis(150);
const TEST_KEY_HOLD: Duration = Duration::from_millis(20);

pub(crate) type TitlebarClick = (WindowId, u64, Point<f64, Logical>);

#[derive(Debug, Default)]
pub(crate) struct TestPointer {
    actions: VecDeque<TestPointerAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestKey {
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
    fn from_name(name: &str) -> Option<Self> {
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

    fn code(self) -> Keycode {
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

fn parse_key_actions(rest: &str, actions: &mut VecDeque<TestPointerAction>) {
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

fn ascii_keycode(ch: char) -> u32 {
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
enum TestPointerAction {
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

    #[cfg(test)]
    pub(crate) fn from_env_with(value: &str) -> Option<Self> {
        Self::parse(value)
    }

    fn parse(value: &str) -> Option<Self> {
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

fn parse_ms(value: Option<&str>, default: Duration) -> Duration {
    match value.and_then(|v| v.parse::<u64>().ok()) {
        Some(0) => Duration::from_millis(1),
        Some(ms) => Duration::from_millis(ms),
        None => default,
    }
}

pub(crate) fn test_pointer_delay() -> Duration {
    *TEST_POINTER_STEP
}
const TEST_DRAG_STEPS: usize = 4;
const TEST_DRAG_STEP: (f64, f64) = (12.0, 8.0);
const SHAKE_MIN_DX: f64 = 2.0;
const SHAKE_MIN_SWING: f64 = 24.0;
const SHAKE_MAX_SWING: Duration = Duration::from_millis(300);
const SHAKE_MAX_TOTAL: Duration = Duration::from_secs(1);
const SHAKE_REVERSALS: u8 = 3;

#[derive(Debug, Default)]
pub(crate) struct ShakeDetector {
    last_x: Option<f64>,
    direction: i8,
    swing: f64,
    reversals: u8,
    started: Option<Instant>,
    last_reversal: Option<Instant>,
    last_shake: Option<Instant>,
}

impl ShakeDetector {
    pub(crate) fn observe(&mut self, x: f64, now: Instant) -> bool {
        let Some(last) = self.last_x.replace(x) else {
            return false;
        };
        let dx = x - last;
        if dx.abs() < SHAKE_MIN_DX {
            return false;
        }
        let direction = if dx > 0.0 { 1 } else { -1 };

        if self.direction == 0 {
            self.start(direction, dx.abs(), now);
            return false;
        }
        if direction == self.direction {
            self.swing += dx.abs();
            if self
                .started
                .is_some_and(|started| now.duration_since(started) > SHAKE_MAX_TOTAL)
            {
                self.start(direction, dx.abs(), now);
            }
            return false;
        }

        let swing = self.swing;
        let reversal_dt = self
            .last_reversal
            .map(|last| now.duration_since(last))
            .unwrap_or_default();
        let total = self
            .started
            .map(|started| now.duration_since(started))
            .unwrap_or_default();
        if swing < SHAKE_MIN_SWING || reversal_dt > SHAKE_MAX_SWING || total > SHAKE_MAX_TOTAL {
            self.start(direction, dx.abs(), now);
            return false;
        }

        self.reversals += 1;
        self.direction = direction;
        self.last_reversal = Some(now);
        self.last_shake = Some(now);
        self.swing = dx.abs();
        if self.reversals >= SHAKE_REVERSALS {
            self.direction = 0;
            self.swing = 0.0;
            self.reversals = 0;
            self.started = None;
            self.last_reversal = None;
            self.last_shake = Some(now);
            return true;
        }
        false
    }

    pub(crate) fn last_shake(&self) -> Option<Instant> {
        self.last_shake
    }

    fn start(&mut self, direction: i8, swing: f64, now: Instant) {
        self.direction = direction;
        self.swing = swing;
        self.reversals = 0;
        self.started = Some(now);
        self.last_reversal = Some(now);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompositorShortcut {
    Vt(u8),
    Quit,
    Blocked,
    Relock,
    Screenshot,
    ScreenshotFull,
    ScreenshotRegion,
    Launcher,
    ForceQuit,
    ArrangeLeft,
    ArrangeRight,
    ArrangeUp,
    ArrangeDown,
    ArrangePairLeft,
    ArrangePairRight,
    ArrangePairUp,
    ArrangePairDown,
    ArrangeFill,
    ArrangeCenter,
    ArrangeRestore,
    ToggleFullscreen,
    PreviousSpace,
    NextSpace,
    AddDesktop,
    None,
}

fn compositor_shortcut(
    ctrl: bool,
    alt: bool,
    shift: bool,
    logo: bool,
    syms: &[xkb::Keysym],
    locked: bool,
) -> CompositorShortcut {
    if shift && logo {
        for sym in syms {
            match u32::from(*sym) {
                0x33 => return CompositorShortcut::ScreenshotFull,
                0x34 => return CompositorShortcut::ScreenshotRegion,
                _ => {}
            }
        }
    }
    if syms.iter().any(|sym| u32::from(*sym) == keysyms::KEY_Print) {
        return CompositorShortcut::Screenshot;
    }
    if alt && logo && !ctrl && !shift && is_escape(syms) {
        return if locked {
            CompositorShortcut::None
        } else {
            CompositorShortcut::ForceQuit
        };
    }
    if logo && !ctrl && !alt && !shift && syms.iter().any(|sym| u32::from(*sym) == 0x20) {
        return if locked {
            CompositorShortcut::None
        } else {
            CompositorShortcut::Launcher
        };
    }
    if ctrl && logo && !alt && !locked {
        for sym in syms {
            let sym = u32::from(*sym);
            match sym {
                keysyms::KEY_Left => {
                    return if shift {
                        CompositorShortcut::ArrangePairLeft
                    } else {
                        CompositorShortcut::ArrangeLeft
                    };
                }
                keysyms::KEY_Right => {
                    return if shift {
                        CompositorShortcut::ArrangePairRight
                    } else {
                        CompositorShortcut::ArrangeRight
                    };
                }
                keysyms::KEY_Up => {
                    return if shift {
                        CompositorShortcut::ArrangePairUp
                    } else {
                        CompositorShortcut::ArrangeUp
                    };
                }
                keysyms::KEY_Down => {
                    return if shift {
                        CompositorShortcut::ArrangePairDown
                    } else {
                        CompositorShortcut::ArrangeDown
                    };
                }
                keysyms::KEY_Return => return CompositorShortcut::ArrangeFill,
                keysyms::KEY_c => return CompositorShortcut::ArrangeCenter,
                keysyms::KEY_r => return CompositorShortcut::ArrangeRestore,
                keysyms::KEY_f => return CompositorShortcut::ToggleFullscreen,
                _ => {}
            }
        }
    }
    if !(ctrl && alt) {
        return CompositorShortcut::None;
    }
    if !shift && !logo && !locked {
        for sym in syms {
            match u32::from(*sym) {
                keysyms::KEY_Left => return CompositorShortcut::PreviousSpace,
                keysyms::KEY_Right => return CompositorShortcut::NextSpace,
                keysyms::KEY_n | keysyms::KEY_N => return CompositorShortcut::AddDesktop,
                _ => {}
            }
        }
    }
    for sym in syms {
        let sym = u32::from(*sym);
        if (keysyms::KEY_F1..=keysyms::KEY_F12).contains(&sym) {
            return CompositorShortcut::Vt((sym - keysyms::KEY_F1 + 1) as u8);
        }
        if sym == keysyms::KEY_BackSpace {
            return if locked {
                CompositorShortcut::Blocked
            } else {
                CompositorShortcut::Quit
            };
        }
        if sym == keysyms::KEY_L {
            return CompositorShortcut::Relock;
        }
    }
    CompositorShortcut::None
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

fn is_escape(syms: &[xkb::Keysym]) -> bool {
    syms.iter()
        .any(|sym| u32::from(*sym) == keysyms::KEY_Escape)
}

fn activation_updates(previous: Option<WindowId>, next: Option<WindowId>) -> Vec<(WindowId, bool)> {
    let mut updates = Vec::new();
    if let Some(previous) = previous.filter(|previous| Some(*previous) != next) {
        updates.push((previous, false));
    }
    if let Some(next) = next {
        updates.push((next, true));
    }
    updates
}

fn clamp_pointer(
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

    fn keyboard_key(&mut self, key_code: Keycode, key_state: KeyState, time: InputTime) {
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

    fn pointer_press(&mut self, button: u32, location: Point<f64, Logical>, time: InputTime) {
        if self.locked {
            tracing::info!(?location, "press ignored: the session is locked");
            return;
        }
        self.pointer_motion(location, time);
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
        self.raise_and_focus(id);

        if button == BUTTON_LEFT
            && let Some(local) = self.titlebar_point(id, location)
        {
            let button_kind = self
                .chrome_frame(id)
                .and_then(|(outer, insets)| crate::chrome::button_at(outer, insets, location));
            if let Some(kind) = button_kind {
                self.clear_chrome_pointer();
                if kind == vow_design::ButtonKind::Maximize && !self.green_button_admitted(id) {
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

    fn window_button(&mut self, id: WindowId, kind: vow_design::ButtonKind) {
        match kind {
            vow_design::ButtonKind::Close => self.windows.send_close(id),
            vow_design::ButtonKind::Minimize => self.set_minimized(id, true),
            vow_design::ButtonKind::Maximize => self.green_button(id),
        }
        self.request_redraw();
    }

    fn green_button(&mut self, id: WindowId) {
        use crate::config::GreenButtonAction;
        match self.config.window.green_button {
            GreenButtonAction::Focus if self.has_fixed_width(id) => {
                self.toggle_maximize(id);
            }
            GreenButtonAction::Focus => {
                tracing::info!(window = id.get(), "focus mode requested");
                self.toggle_focus_mode(id);
            }
            GreenButtonAction::Zoom => {
                if self.windows.zoom_supported(id) {
                    self.toggle_maximize(id);
                } else {
                    tracing::info!(
                        window = id.get(),
                        "maximize ignored: the window is fixed-size"
                    );
                }
            }
            GreenButtonAction::Fullscreen => {
                tracing::info!(window = id.get(), "fullscreen requested");
            }
            GreenButtonAction::None => {}
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
            for (window, activated) in activation_updates(previous, Some(id)) {
                self.windows.set_activated(window, activated);
            }
            self.publish_active(Some(id));
            if let Some(previous) = previous {
                self.foreign_state_changed(previous);
            }
            self.foreign_state_changed(id);
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
        let shell = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("malaysia-shell")))
            .unwrap_or_else(|| "malaysia-shell".into());
        let mut command = crate::spawn::shell_command(shell);
        command.arg(component);
        if let Some(display) = self.idle.wayland_display() {
            command.env("WAYLAND_DISPLAY", display);
        }
        match crate::spawn::spawn_shell(command) {
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
        if TITLEBAR.position != vow_design::TitlebarPosition::Top {
            return None;
        }
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
    ) -> Option<(Option<vow_design::ButtonKind>, bool)> {
        let local = self.titlebar_point(id, location)?;
        let hit = vow_appearance::traffic::hit((local.x, local.y));
        let (gx, gy, gw, gh) = vow_appearance::traffic::group_rect();
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
                let mut pointer = vow_appearance::traffic::TrafficPointer::default();
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
        let mut pointer = vow_appearance::traffic::TrafficPointer::default();
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

    fn chrome_release(&mut self) {
        let location = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location());
        if let Some(location) = location {
            self.chrome_release_at(location);
        }
    }

    fn chrome_release_at(&mut self, location: Point<f64, Logical>) {
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

    fn note_client_input_serial(&self, surface: &WlSurface, serial: Serial) {
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
            for (window, activated) in activation_updates(Some(previous), None) {
                self.windows.set_activated(window, activated);
            }
            self.foreign_state_changed(previous);
        }
        self.publish_active(None);
        self.log_focus_stack(None);
    }

    pub(crate) fn publish_active(&self, id: Option<WindowId>) {
        self.publish_active_to(&vow_appearance::data_dir(), id);
    }

    fn publish_active_to(&self, dir: &Path, id: Option<WindowId>) {
        let info = id.and_then(|id| self.windows.active_info(id));
        let active = info.map(|(title, app_id)| crate::active::ActiveWindow { title, app_id });
        crate::active::publish_to(dir, active.as_ref());
    }

    pub(crate) fn publish_active_if_focused(&self, id: WindowId) {
        self.publish_active_if_focused_to(&vow_appearance::data_dir(), id);
    }

    fn publish_active_if_focused_to(&self, dir: &Path, id: WindowId) {
        if self.focused_window() == Some(id) {
            self.publish_active_to(dir, Some(id));
        }
    }

    pub(crate) fn forget_window(&mut self, surface: &WlSurface) {
        if self.leak_window(surface) {
            return;
        }
        let active = self.focused_window();
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
        if self.grab.map(Grab::window) == Some(id) {
            self.grab = None;
        }
        tracing::info!(
            windows = self.windows.len(),
            topmost = ?self.windows.topmost(),
            "window removed"
        );
        if active == Some(id) {
            match self.windows.topmost_visible(&self.minimized) {
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

    fn leak_window(&mut self, surface: &WlSurface) -> bool {
        if std::env::var_os("MALAYSIA_TEST_LEAK_WINDOW").is_none() {
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
        tracing::warn!("MALAYSIA_TEST_LEAK_WINDOW: deliberately leaving a window behind");
        tracing::info!(
            windows = self.windows.len(),
            topmost = ?self.windows.topmost(),
            "window removed"
        );
        true
    }

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

    fn test_pointer_press(&mut self, location: Point<f64, Logical>, time: InputTime) {
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

    fn test_pointer_release(&mut self, time: InputTime) {
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
        if self.windows.outer(id).is_none() {
            return;
        }
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

    fn inject_test_hovers(&mut self) {
        for (x, y) in test_hover_points() {
            self.pointer_motion(Point::new(x, y), InputTime::from_millis(0));
        }
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
        if self.shake.observe(location.x, now) {
            self.trigger_cursor_boost();
        } else {
            self.keep_cursor_boost_alive(now);
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
                    malaysia_scene::scene::resized(start_outer, edge, dx, dy, minimum, maximum);
                self.windows.resize(window, outer);
            }
        }
    }

    // A press on a maximized titlebar may be a click or a double click: restore only once it drags.
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
            <= vow_design::TITLEBAR_DOUBLE_CLICK.slop
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

        if let Some(pointer) = self.seat.get_pointer() {
            pointer.axis(self, frame);
            pointer.frame(self);
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

// The pointer keeps its share of the titlebar width, as in macOS and GNOME.
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
    let interval = u64::from(vow_design::TITLEBAR_DOUBLE_CLICK.interval_ms);
    let slop = vow_design::TITLEBAR_DOUBLE_CLICK.slop;
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

fn round(value: f64) -> i32 {
    value.round() as i32
}

fn test_press_fraction() -> (f64, f64) {
    parse_pair("MALAYSIA_TEST_PRESS_AT").unwrap_or((0.5, 0.5))
}

fn test_titlebar_button() -> Option<vow_design::ButtonKind> {
    match std::env::var("MALAYSIA_TEST_TITLEBAR_BUTTON")
        .ok()?
        .as_str()
    {
        "close" => Some(vow_design::ButtonKind::Close),
        "minimize" => Some(vow_design::ButtonKind::Minimize),
        "maximize" => Some(vow_design::ButtonKind::Maximize),
        _ => None,
    }
}

fn test_drag_delta() -> (f64, f64) {
    parse_pair("MALAYSIA_TEST_DRAG").unwrap_or((60.0, 40.0))
}

fn test_hover_points() -> Vec<(f64, f64)> {
    let Ok(value) = std::env::var("MALAYSIA_TEST_HOVER") else {
        return Vec::new();
    };
    value
        .split(';')
        .filter_map(parse_pair_value)
        .collect::<Vec<_>>()
}

fn parse_pair(name: &str) -> Option<(f64, f64)> {
    parse_pair_value(&std::env::var(name).ok()?)
}

fn parse_pair_value(value: &str) -> Option<(f64, f64)> {
    let (x, y) = value.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use smithay::backend::input::ButtonState;

    use smithay::backend::input::{
        AbsolutePositionEvent, Device, DeviceCapability, Event, InputBackend, InputEvent,
        InputTime, PointerMotionAbsoluteEvent, UnusedEvent,
    };
    use smithay::input::pointer::{ButtonEvent, CursorIcon};
    use smithay::reexports::calloop::EventLoop;
    use smithay::reexports::wayland_server::Display;
    use smithay::utils::{Point, SERIAL_COUNTER, Serial, Size};
    use xkbcommon::xkb::{self, Keycode, keysyms};

    use super::{
        BUTTON_LEFT, CompositorShortcut, DesktopClick, Edge, ShakeDetector, TEST_KEY_HOLD,
        TEST_POINTER_STEP_DEFAULT, TestKey, TestPointer, TestPointerAction, activation_updates,
        clamp_pointer, compositor_shortcut, desktop_click, double_click, edge_cursor, is_escape,
        parse_ms, pointer_focus_blocked,
    };
    use crate::state::State;
    use malaysia_scene::scene::WindowId;

    #[test]
    fn escape_is_recognised_by_its_raw_sym() {
        assert!(is_escape(&[keysyms::KEY_Escape.into()]));
        assert!(!is_escape(&[keysyms::KEY_Down.into()]));
    }

    #[test]
    fn ctrl_alt_backspace_quits_only_when_unlocked() {
        let backspace = [keysyms::KEY_BackSpace.into()];
        assert_eq!(
            compositor_shortcut(true, true, false, false, &backspace, false),
            CompositorShortcut::Quit
        );
        assert_eq!(
            compositor_shortcut(true, true, false, false, &backspace, true),
            CompositorShortcut::Blocked
        );
    }

    #[test]
    fn alt_super_escape_opens_force_quit_when_unlocked() {
        let escape = [keysyms::KEY_Escape.into()];
        assert_eq!(
            compositor_shortcut(false, true, false, true, &escape, false),
            CompositorShortcut::ForceQuit
        );
        assert_eq!(
            compositor_shortcut(false, true, false, true, &escape, true),
            CompositorShortcut::None
        );
        assert_eq!(
            compositor_shortcut(true, true, false, true, &escape, false),
            CompositorShortcut::None
        );
    }

    #[test]
    fn ctrl_super_arrange_shortcuts_are_ignored_while_locked() {
        let left = [keysyms::KEY_Left.into()];
        assert_eq!(
            compositor_shortcut(true, false, false, true, &left, false),
            CompositorShortcut::ArrangeLeft
        );
        assert_eq!(
            compositor_shortcut(true, false, true, true, &left, true),
            CompositorShortcut::None
        );
    }

    #[test]
    fn ctrl_alt_l_locks_or_relaunches() {
        let l = [keysyms::KEY_L.into()];
        assert_eq!(
            compositor_shortcut(true, true, false, false, &l, false),
            CompositorShortcut::Relock
        );
        assert_eq!(
            compositor_shortcut(true, true, false, false, &l, true),
            CompositorShortcut::Relock
        );
        assert_eq!(
            compositor_shortcut(false, true, false, false, &l, false),
            CompositorShortcut::None
        );
    }

    #[test]
    fn ctrl_alt_f_keys_request_their_vt() {
        let f2 = [keysyms::KEY_F2.into()];
        assert_eq!(
            compositor_shortcut(true, true, false, false, &f2, false),
            CompositorShortcut::Vt(2)
        );
        assert_eq!(
            compositor_shortcut(true, true, false, false, &f2, true),
            CompositorShortcut::Vt(2)
        );
        assert_eq!(
            compositor_shortcut(false, true, false, false, &f2, false),
            CompositorShortcut::None
        );
    }

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
    fn an_identity_change_republishes_only_the_focused_window() {
        let (_display, mut state) = test_state((1280, 720));
        let dir =
            std::env::temp_dir().join(format!("malaysia-active-focused-{}", std::process::id()));
        let store = dir.join("window.domain");

        let focused = WindowId::new(1);
        let other = WindowId::new(2);
        state.active_window.set(Some(focused));

        state.publish_active_if_focused_to(&dir, other);
        assert!(
            !store.exists(),
            "an inactive window's title or app_id must not rewrite the store"
        );

        state.publish_active_if_focused_to(&dir, focused);
        assert!(
            store.exists(),
            "the focused window's title or app_id must rewrite the store"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_shake_starts_the_pointer_animation() {
        let (_display, mut state) = test_state((1280, 720));
        state.trigger_cursor_boost();
        assert!(state.cursor_boost_started.is_some());
        let scale = state.cursor_boost_scale();
        assert!((1.0..=vow_design::CURSOR_SHAKE_SCALE).contains(&scale));
        state.cursor_boost_started = None;
        assert_eq!(state.cursor_boost_scale(), 1.0);
    }

    #[test]
    fn super_space_toggles_the_launcher_unless_locked() {
        let space = [xkb::Keysym::new(0x20)];
        assert_eq!(
            compositor_shortcut(false, false, false, true, &space, false),
            CompositorShortcut::Launcher
        );
        assert_eq!(
            compositor_shortcut(false, false, false, true, &space, true),
            CompositorShortcut::None
        );
        assert_eq!(
            compositor_shortcut(false, false, true, true, &space, false),
            CompositorShortcut::None
        );
    }

    #[test]
    fn print_screen_maps_to_the_screenshot_shortcut() {
        let print = xkb::Keysym::new(keysyms::KEY_Print);
        assert!(matches!(
            compositor_shortcut(false, false, false, false, &[print], false),
            CompositorShortcut::Screenshot
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, false, &[print], false),
            CompositorShortcut::Screenshot
        ));
        let l = xkb::Keysym::new(keysyms::KEY_L);
        assert!(matches!(
            compositor_shortcut(true, true, false, false, &[l], false),
            CompositorShortcut::Relock
        ));
    }

    #[test]
    fn macos_shift_super_keys_map_to_the_screenshot_modes() {
        let three = xkb::Keysym::new(0x33);
        let four = xkb::Keysym::new(0x34);
        assert!(matches!(
            compositor_shortcut(false, false, true, true, &[three], false),
            CompositorShortcut::ScreenshotFull
        ));
        assert!(matches!(
            compositor_shortcut(false, false, true, true, &[four], false),
            CompositorShortcut::ScreenshotRegion
        ));
        assert!(matches!(
            compositor_shortcut(false, false, true, false, &[three], false),
            CompositorShortcut::None
        ));
    }

    #[test]
    fn ctrl_super_keys_map_to_infinity_arrange_shortcuts() {
        let left = xkb::Keysym::new(keysyms::KEY_Left);
        let right = xkb::Keysym::new(keysyms::KEY_Right);
        let up = xkb::Keysym::new(keysyms::KEY_Up);
        let down = xkb::Keysym::new(keysyms::KEY_Down);
        let ret = xkb::Keysym::new(keysyms::KEY_Return);
        let c = xkb::Keysym::new(keysyms::KEY_c);
        let r = xkb::Keysym::new(keysyms::KEY_r);
        let f = xkb::Keysym::new(keysyms::KEY_f);

        assert!(matches!(
            compositor_shortcut(true, false, false, true, &[left], false),
            CompositorShortcut::ArrangeLeft
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, true, &[right], false),
            CompositorShortcut::ArrangeRight
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, true, &[up], false),
            CompositorShortcut::ArrangeUp
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, true, &[down], false),
            CompositorShortcut::ArrangeDown
        ));
        assert!(matches!(
            compositor_shortcut(true, false, true, true, &[left], false),
            CompositorShortcut::ArrangePairLeft
        ));
        assert!(matches!(
            compositor_shortcut(true, false, true, true, &[right], false),
            CompositorShortcut::ArrangePairRight
        ));
        assert!(matches!(
            compositor_shortcut(true, false, true, true, &[up], false),
            CompositorShortcut::ArrangePairUp
        ));
        assert!(matches!(
            compositor_shortcut(true, false, true, true, &[down], false),
            CompositorShortcut::ArrangePairDown
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, true, &[ret], false),
            CompositorShortcut::ArrangeFill
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, true, &[c], false),
            CompositorShortcut::ArrangeCenter
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, true, &[r], false),
            CompositorShortcut::ArrangeRestore
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, true, &[f], false),
            CompositorShortcut::ToggleFullscreen
        ));

        assert!(matches!(
            compositor_shortcut(true, true, false, true, &[left], false),
            CompositorShortcut::None
        ));
    }

    #[test]
    fn ctrl_alt_arrows_switch_spaces_and_ctrl_alt_n_adds_a_desktop() {
        let left = xkb::Keysym::from(keysyms::KEY_Left);
        let right = xkb::Keysym::from(keysyms::KEY_Right);
        let n = xkb::Keysym::from(keysyms::KEY_n);
        assert!(matches!(
            compositor_shortcut(true, true, false, false, &[left], false),
            CompositorShortcut::PreviousSpace
        ));
        assert!(matches!(
            compositor_shortcut(true, true, false, false, &[right], false),
            CompositorShortcut::NextSpace
        ));
        assert!(matches!(
            compositor_shortcut(true, true, false, false, &[n], false),
            CompositorShortcut::AddDesktop
        ));
        assert!(matches!(
            compositor_shortcut(true, false, false, false, &[left], false),
            CompositorShortcut::None
        ));
        assert!(matches!(
            compositor_shortcut(true, true, false, false, &[right], true),
            CompositorShortcut::None
        ));
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
    fn a_rapid_side_to_side_shake_triggers_the_locate_growth() {
        let mut shake = ShakeDetector::default();
        let start = std::time::Instant::now();
        let mut fired = false;
        for (index, x) in [0.0, 100.0, 0.0, 100.0, 0.0].into_iter().enumerate() {
            fired |= shake.observe(
                x,
                start + std::time::Duration::from_millis(index as u64 * 20),
            );
        }
        assert!(fired, "left-right-left-right should trigger");
    }

    #[test]
    fn a_slow_reversal_does_not_trigger_the_locate_growth() {
        let mut shake = ShakeDetector::default();
        let start = std::time::Instant::now();
        let mut fired = false;
        for (index, x) in [0.0, 100.0, 0.0, 100.0, 0.0].into_iter().enumerate() {
            let elapsed = std::time::Duration::from_millis(index as u64 * 400);
            fired |= shake.observe(x, start + elapsed);
        }
        assert!(!fired, "one swing per few hundred ms is not a shake");
    }

    #[test]
    fn tiny_jitter_does_not_trigger_the_locate_growth() {
        let mut shake = ShakeDetector::default();
        let start = std::time::Instant::now();
        let mut fired = false;
        for (index, x) in [0.0, 1.0, 0.0, 1.0, 0.0].into_iter().enumerate() {
            fired |= shake.observe(
                x,
                start + std::time::Duration::from_millis(index as u64 * 10),
            );
        }
        assert!(!fired, "sub-pixel jitter is not a deliberate shake");
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
        use smithay::backend::allocator::{Format, Fourcc, Modifier};
        use smithay::wayland::dmabuf::DmabufFeedbackBuilder;

        let event_loop =
            EventLoop::<crate::session::Session>::try_new().expect("the test needs an event loop");
        let display = Display::<State>::new().expect("the test needs a display");
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
            Box::new(()),
        )
        .expect("the test state should build");
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
        let mut pointer = vow_appearance::traffic::TrafficPointer::default();
        pointer.on_press(Some(vow_design::ButtonKind::Close), true);
        assert_eq!(pointer.on_release(None, false), None);
    }

    #[test]
    fn pressing_and_releasing_the_same_titlebar_button_fires_it() {
        let mut pointer = vow_appearance::traffic::TrafficPointer::default();
        pointer.on_press(Some(vow_design::ButtonKind::Close), true);
        assert_eq!(
            pointer.on_release(Some(vow_design::ButtonKind::Close), true),
            Some(vow_design::ButtonKind::Close)
        );
    }

    #[test]
    fn a_click_on_the_desktop_clears_the_keyboard_focus() {
        assert_eq!(desktop_click(false), DesktopClick::ClearFocus);
        assert_eq!(desktop_click(true), DesktopClick::CloseMenu);
    }

    #[test]
    fn focus_moves_activated_to_the_new_window_only() {
        use std::collections::HashSet;

        let a = WindowId::new(1);
        let b = WindowId::new(2);
        let mut activated = HashSet::new();
        let apply = |activated: &mut HashSet<WindowId>, updates: Vec<(WindowId, bool)>| {
            for (window, on) in updates {
                if on {
                    activated.insert(window);
                } else {
                    activated.remove(&window);
                }
            }
        };
        apply(&mut activated, activation_updates(None, Some(a)));
        assert_eq!(activated, HashSet::from([a]));
        apply(&mut activated, activation_updates(Some(a), Some(b)));
        assert_eq!(activated, HashSet::from([b]), "only B keeps Activated");
        apply(&mut activated, activation_updates(Some(b), None));
        assert!(activated.is_empty(), "an unfocused window never keeps it");
    }

    #[test]
    fn the_traffic_lights_are_not_the_double_click_title_area() {
        let (x, y, size) = vow_appearance::traffic::button_rect(0);
        assert_eq!(
            vow_appearance::traffic::hit((x + size / 2.0, y + size / 2.0)),
            Some(vow_design::ButtonKind::Close)
        );
    }

    #[test]
    fn scripted_text_keys_map_to_ascii_keycodes() {
        assert_eq!(TestKey::from_name("w").unwrap().code(), Keycode::new(25));
        assert_eq!(TestKey::from_name("7").unwrap().code(), Keycode::new(16));
        assert!(TestKey::from_name("ocul").is_none());
    }

    #[test]
    fn fixed_width_window_maximizes_vertically() {
        use malaysia_scene::geometry::{Point as ScenePoint, Rect as SceneRect, Size as SceneSize};

        let outer = SceneRect::new(ScenePoint::new(300.0, 100.0), SceneSize::new(760.0, 620.0));
        let workspace = (Point::from((0, 0)), Size::from((1920, 1052)));
        let target = super::maximize_target_rect(true, workspace, outer);
        assert_eq!(target.x(), 300.0);
        assert_eq!(target.y(), 0.0);
        assert_eq!(target.width(), 760.0);
        assert_eq!(target.height(), 1052.0);
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
    fn fixed_width_is_detected_with_unbounded_height() {
        let min = Size::from((760, 620));
        let max = Size::from((760, 0));
        assert!(super::is_fixed_width(min, max));
    }

    #[test]
    fn unbounded_width_is_not_fixed_width() {
        let min = Size::from((0, 620));
        let max = Size::from((0, 0));
        assert!(!super::is_fixed_width(min, max));
    }

    #[test]
    fn zero_max_width_is_not_fixed_width() {
        let min = Size::from((760, 620));
        let max = Size::from((0, 0));
        assert!(!super::is_fixed_width(min, max));
    }

    #[test]
    fn regular_window_maximizes_to_the_full_workspace() {
        use malaysia_scene::geometry::{Point as ScenePoint, Rect as SceneRect, Size as SceneSize};

        let outer = SceneRect::new(ScenePoint::new(300.0, 100.0), SceneSize::new(760.0, 620.0));
        let workspace = (Point::from((0, 0)), Size::from((1920, 1052)));
        let target = super::maximize_target_rect(false, workspace, outer);
        assert_eq!(target.x(), 0.0);
        assert_eq!(target.y(), 0.0);
        assert_eq!(target.width(), 1920.0);
        assert_eq!(target.height(), 1052.0);
    }

    #[test]
    fn scripted_modifier_keys_hold_while_key_is_pressed() {
        let pointer = TestPointer::from_env_with("key:ctrl+r").unwrap();
        let actions: Vec<_> = pointer.actions.into_iter().collect();
        assert_eq!(
            actions,
            vec![
                TestPointerAction::KeyDown(TestKey::Ctrl),
                TestPointerAction::Wait(TEST_KEY_HOLD),
                TestPointerAction::KeyDown(TestKey::Char('r')),
                TestPointerAction::Wait(TEST_KEY_HOLD),
                TestPointerAction::KeyUp(TestKey::Char('r')),
                TestPointerAction::Wait(TEST_KEY_HOLD),
                TestPointerAction::KeyUp(TestKey::Ctrl),
                TestPointerAction::Wait(TEST_KEY_HOLD),
            ]
        );
    }

    #[test]
    fn scripted_multiple_modifiers_keep_order() {
        let pointer = TestPointer::from_env_with("key:ctrl+shift+s").unwrap();
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

    #[test]
    fn parse_ms_returns_parsed_value() {
        assert_eq!(
            parse_ms(Some("8"), TEST_POINTER_STEP_DEFAULT),
            Duration::from_millis(8)
        );
    }

    #[test]
    fn parse_ms_empty_uses_default() {
        assert_eq!(
            parse_ms(Some(""), TEST_POINTER_STEP_DEFAULT),
            TEST_POINTER_STEP_DEFAULT
        );
        assert_eq!(
            parse_ms(None, TEST_POINTER_STEP_DEFAULT),
            TEST_POINTER_STEP_DEFAULT
        );
    }

    #[test]
    fn parse_ms_garbage_uses_default() {
        assert_eq!(
            parse_ms(Some("fast"), TEST_POINTER_STEP_DEFAULT),
            TEST_POINTER_STEP_DEFAULT
        );
    }

    #[test]
    fn parse_ms_zero_uses_minimum() {
        assert_eq!(
            parse_ms(Some("0"), TEST_POINTER_STEP_DEFAULT),
            Duration::from_millis(1)
        );
    }
}
