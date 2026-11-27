use std::any::Any;

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, GestureBeginEvent,
    GestureEndEvent, GestureSwipeUpdateEvent, InputBackend, InputEvent, InputTime, KeyState,
    KeyboardKeyEvent, MouseButton, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::desktop::{Window, WindowSurfaceType};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState, keysyms};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::reexports::input::event::gesture::{
    GestureEventCoordinates, GestureSwipeUpdateEvent as LibinputSwipeUpdate,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Serial, Size};

use crate::state::{Backend, State};
use crate::tiling::Tile;
use crate::workspaces::desktops::{Direction, send_changed_state};
use crate::workspaces::full_screen::nth_desktop;
use crate::workspaces::swipe::{self, Swipe};

pub const REPEAT_DELAY: i32 = 200;
pub const REPEAT_RATE: i32 = 25;
const SWIPE_FINGERS: u32 = 3;
const KEYCODE_1: u32 = 10;
const KEYCODE_0: u32 = 19;

#[derive(Debug, PartialEq)]
enum KeyAction {
    Quit,
    SwitchVt(i32),
    SwitchDesktop(Direction),
    GoToDesktop(usize),
    Tile(Tile),
    RestoreTiled,
    OpenExpansion,
    CloseExpansion,
    Preview,
    OpenAppExpose,
    ToggleFullScreen,
    Split(Direction),
}

fn key_action(
    modifiers: &ModifiersState,
    keysym: Keysym,
    keycode: u32,
    expansion_open: bool,
) -> Option<KeyAction> {
    let control_only = modifiers.ctrl && !modifiers.alt && !modifiers.shift && !modifiers.logo;
    let control_super = modifiers.ctrl && modifiers.logo && !modifiers.alt && !modifiers.shift;
    let control_alt = modifiers.ctrl && modifiers.alt && !modifiers.shift && !modifiers.logo;
    let unmodified = !modifiers.ctrl && !modifiers.alt && !modifiers.shift && !modifiers.logo;
    match keysym.raw() {
        vt @ keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12 => Some(
            KeyAction::SwitchVt((vt - keysyms::KEY_XF86Switch_VT_1 + 1) as i32),
        ),
        keysyms::KEY_BackSpace if modifiers.ctrl && modifiers.alt => Some(KeyAction::Quit),
        keysyms::KEY_Up | keysyms::KEY_Down if control_only && expansion_open => {
            Some(KeyAction::CloseExpansion)
        }
        keysyms::KEY_Escape if expansion_open => Some(KeyAction::CloseExpansion),
        keysyms::KEY_Left if control_only => Some(KeyAction::SwitchDesktop(Direction::Left)),
        keysyms::KEY_Right if control_only => Some(KeyAction::SwitchDesktop(Direction::Right)),
        _ if control_only && expansion_open && (KEYCODE_1..=KEYCODE_0).contains(&keycode) => {
            Some(KeyAction::GoToDesktop((keycode - KEYCODE_1 + 1) as usize))
        }
        keysyms::KEY_space if unmodified && expansion_open => Some(KeyAction::Preview),
        _ if expansion_open => None,
        keysyms::KEY_Up if control_only => Some(KeyAction::OpenExpansion),
        keysyms::KEY_Down if control_only => Some(KeyAction::OpenAppExpose),
        keysyms::KEY_f | keysyms::KEY_F if control_super => Some(KeyAction::Tile(Tile::Fill)),
        keysyms::KEY_f | keysyms::KEY_F if control_alt => Some(KeyAction::ToggleFullScreen),
        keysyms::KEY_Left if control_alt => Some(KeyAction::Split(Direction::Left)),
        keysyms::KEY_Right if control_alt => Some(KeyAction::Split(Direction::Right)),
        keysyms::KEY_c | keysyms::KEY_C if control_super => Some(KeyAction::Tile(Tile::Center)),
        keysyms::KEY_Left if control_super => Some(KeyAction::Tile(Tile::Left)),
        keysyms::KEY_Right if control_super => Some(KeyAction::Tile(Tile::Right)),
        keysyms::KEY_Up if control_super => Some(KeyAction::Tile(Tile::Top)),
        keysyms::KEY_Down if control_super => Some(KeyAction::Tile(Tile::Bottom)),
        keysyms::KEY_r | keysyms::KEY_R if control_super => Some(KeyAction::RestoreTiled),
        _ => None,
    }
}

impl State {
    pub fn process_input_event<I: InputBackend + 'static>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let keycode = event.key_code();
                let pressed = event.state() == KeyState::Pressed;
                let action = self
                    .seat
                    .get_keyboard()
                    .unwrap()
                    .input(
                        self,
                        keycode,
                        event.state(),
                        serial,
                        event.time(),
                        |state, modifiers, handle| {
                            if !pressed {
                                return match state
                                    .suppressed_keys
                                    .iter()
                                    .position(|key| *key == keycode)
                                {
                                    Some(index) => {
                                        state.suppressed_keys.swap_remove(index);
                                        FilterResult::Intercept(None)
                                    }
                                    None => FilterResult::Forward,
                                };
                            }
                            match key_action(
                                modifiers,
                                handle.modified_sym(),
                                keycode.raw(),
                                state.expansion.open,
                            ) {
                                Some(action) => {
                                    state.suppressed_keys.push(keycode);
                                    FilterResult::Intercept(Some(action))
                                }
                                None => FilterResult::Forward,
                            }
                        },
                    )
                    .flatten();
                if action.is_some() {
                    self.end_swipe(true, event.time());
                }
                match action {
                    Some(KeyAction::Quit) => {
                        tracing::info!("stopping: quit key");
                        self.loop_signal.stop();
                    }
                    Some(KeyAction::SwitchVt(vt)) => {
                        if let Backend::Drm(drm) = &mut self.backend {
                            drm.change_vt(vt);
                        }
                    }
                    Some(KeyAction::SwitchDesktop(_) | KeyAction::GoToDesktop(_))
                        if self.expansion.app.is_some() => {}
                    Some(KeyAction::SwitchDesktop(direction)) => {
                        self.switch_desktop(direction, event.time())
                    }
                    Some(KeyAction::GoToDesktop(number)) => {
                        if let Some(index) =
                            nth_desktop(number, self.desktops.len(), |index| self.is_desktop(index))
                        {
                            self.go_to_desktop(index, event.time());
                        }
                    }
                    Some(KeyAction::Tile(tile)) => self.tile(tile),
                    Some(KeyAction::RestoreTiled) => self.restore_tiled(),
                    Some(KeyAction::OpenExpansion) => self.open_expansion(None, event.time()),
                    Some(KeyAction::CloseExpansion) => self.close_expansion(None, event.time()),
                    Some(KeyAction::Preview) => self.toggle_preview(),
                    Some(KeyAction::OpenAppExpose) => {
                        if let Some(window) = self.top_window() {
                            self.open_expansion(Some(window), event.time());
                        }
                    }
                    Some(KeyAction::ToggleFullScreen) => self.toggle_full_screen(),
                    Some(KeyAction::Split(side)) => self.request_split(side),
                    None => {}
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let geometry = self.output_geometry().unwrap();
                let inside =
                    Rectangle::new(geometry.loc, geometry.size - Size::from((1, 1))).to_f64();
                let location = self.seat.get_pointer().unwrap().current_location() + event.delta();
                self.move_pointer(location.constrain(inside), event.time());
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output_geometry = self.output_geometry().unwrap();
                let location =
                    event.position_transformed(output_geometry.size) + output_geometry.loc.to_f64();
                self.move_pointer(location, event.time());
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let serial = SERIAL_COUNTER.next_serial();
                let state = event.state();
                let expansion_open = self.expansion.open;
                if state == ButtonState::Pressed && !expansion_open && !pointer.is_grabbed() {
                    match self.desktops[self.current_desktop]
                        .element_under(pointer.current_location())
                        .map(|(window, _)| window.clone())
                    {
                        Some(window) => self.focus_window(&window, serial),
                        None if event.button() == Some(MouseButton::Left) => {
                            self.press_divider(event.button_code(), event.time(), serial)
                        }
                        None => {}
                    }
                }
                pointer.button(
                    self,
                    &ButtonEvent {
                        button: event.button_code(),
                        state,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
                if event.button() == Some(MouseButton::Left) {
                    let picking =
                        expansion_open && state == ButtonState::Released && self.expansion.pressed;
                    self.expansion.pressed = expansion_open && state == ButtonState::Pressed;
                    self.needs_redraw |= expansion_open;
                    self.expansion.drag_origin =
                        self.expansion.pressed.then(|| pointer.current_location());
                    if picking {
                        self.pick_in_expansion(event.time());
                    }
                }
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();
                let mut frame = AxisFrame::new(event.time()).source(source);
                for axis in [Axis::Horizontal, Axis::Vertical] {
                    let v120 = event.amount_v120(axis);
                    let amount = event
                        .amount(axis)
                        .unwrap_or_else(|| v120.unwrap_or(0.0) * 15.0 / 120.0);
                    if amount != 0.0 {
                        frame = frame.value(axis, amount);
                        if let Some(discrete) = v120 {
                            frame = frame.v120(axis, discrete as i32);
                        }
                    }
                    if source == AxisSource::Finger && event.amount(axis) == Some(0.0) {
                        frame = frame.stop(axis);
                    }
                }
                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            InputEvent::GestureSwipeBegin { event, .. } => {
                self.end_swipe(true, event.time());
                self.swipe = (event.fingers() == SWIPE_FINGERS).then(Swipe::default);
            }
            InputEvent::GestureSwipeUpdate { event, .. } => {
                let delta = (&event as &dyn Any)
                    .downcast_ref::<LibinputSwipeUpdate>()
                    .map_or_else(
                        || event.delta(),
                        |event| (event.dx_unaccelerated(), event.dy_unaccelerated()).into(),
                    );
                self.update_swipe(delta, event.time());
            }
            InputEvent::GestureSwipeEnd { event, .. } => {
                self.end_swipe(event.cancelled(), event.time())
            }
            _ => {}
        }
    }

    fn update_swipe(&mut self, delta: Point<f64, Logical>, time: InputTime) {
        let Some(mut swipe) = self.swipe.take() else {
            return;
        };
        if swipe.axis.is_none() {
            let Some((axis, along)) = swipe.lock(delta) else {
                self.swipe = Some(swipe);
                return;
            };
            self.start_swipe(&mut swipe, axis, along, time);
        }
        if let Some(axis) = swipe.axis.filter(|axis| self.swipe_held(*axis)) {
            let progress = swipe.follow(delta, time.micros());
            self.hold_swipe(axis, progress);
            self.swipe = Some(swipe);
        }
    }

    fn start_swipe(&mut self, swipe: &mut Swipe, axis: swipe::Axis, along: f64, time: InputTime) {
        let now = self.start_time.elapsed();
        let (progress, cancel, last) = match axis {
            swipe::Axis::Vertical if along > 0.0 && !self.expansion.open => {
                self.open_expansion(None, time);
                (self.expansion.at(now).0, 0.0, 1.0)
            }
            swipe::Axis::Vertical if along < 0.0 && self.expansion.open => {
                (self.expansion.at(now).0, 1.0, 1.0)
            }
            swipe::Axis::Horizontal
                if self.expansion.dragged.is_none()
                    && self.expansion.app.is_none()
                    && !self.seat.get_pointer().unwrap().is_grabbed() =>
            {
                self.wake_desktops();
                (
                    self.strip(now).0,
                    self.current_desktop as f64,
                    (self.desktops.len() - 1) as f64,
                )
            }
            _ => return,
        };
        swipe.track(axis, progress, cancel, last, time.micros());
        self.hold_swipe(axis, progress);
    }

    fn swipe_held(&self, axis: swipe::Axis) -> bool {
        match axis {
            swipe::Axis::Vertical => self.expansion.held,
            swipe::Axis::Horizontal => self.slide_held(),
        }
    }

    fn hold_swipe(&mut self, axis: swipe::Axis, progress: f64) {
        match axis {
            swipe::Axis::Vertical => self.hold_expansion(progress),
            swipe::Axis::Horizontal => self.hold_slide(progress),
        }
    }

    fn end_swipe(&mut self, cancelled: bool, time: InputTime) {
        let Some(swipe) = self.swipe.take() else {
            return;
        };
        let Some(axis) = swipe.axis.filter(|axis| self.swipe_held(*axis)) else {
            return;
        };
        let (target, velocity) = swipe.release(cancelled, time.micros());
        match axis {
            swipe::Axis::Vertical => self.release_expansion(target > 0.5, velocity, time),
            swipe::Axis::Horizontal => self.release_slide(target as usize, velocity, time),
        }
    }

    pub fn move_pointer(&mut self, location: Point<f64, Logical>, time: InputTime) {
        let under = self.surface_under(location);
        let pointer = self.seat.get_pointer().unwrap();
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
        if self.expansion.open {
            self.hover_expansion(location);
        }
        if let Backend::Drm(_) = self.backend {
            self.needs_redraw = true;
        }
    }

    pub fn focus_window(&mut self, window: &Window, serial: Serial) {
        let desktop = &mut self.desktops[self.current_desktop];
        desktop.raise_element(window, true);
        for window in desktop.elements() {
            send_changed_state(window);
        }
        let surface = window.toplevel().unwrap().wl_surface().clone();
        self.seat
            .get_keyboard()
            .unwrap()
            .set_focus(self, Some(surface), serial);
        self.needs_redraw = true;
    }

    fn surface_under(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        if self.expansion.open {
            return None;
        }
        self.desktops[self.current_desktop]
            .element_under(location)
            .and_then(|(window, window_location)| {
                window
                    .surface_under(location - window_location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, surface_location)| {
                        (surface, (surface_location + window_location).to_f64())
                    })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_alt_backspace_quits_but_backspace_alone_is_forwarded() {
        let ctrl_alt = ModifiersState {
            ctrl: true,
            alt: true,
            ..Default::default()
        };
        let backspace = Keysym::new(keysyms::KEY_BackSpace);
        assert_eq!(
            key_action(&ctrl_alt, backspace, 0, false),
            Some(KeyAction::Quit)
        );
        assert_eq!(
            key_action(&ModifiersState::default(), backspace, 0, false),
            None
        );
    }

    #[test]
    fn ctrl_arrow_switches_desktop_but_ctrl_shift_arrow_is_forwarded() {
        let ctrl = ModifiersState {
            ctrl: true,
            ..Default::default()
        };
        let ctrl_shift = ModifiersState {
            shift: true,
            ..ctrl
        };
        let left = Keysym::new(keysyms::KEY_Left);
        let right = Keysym::new(keysyms::KEY_Right);
        assert_eq!(
            key_action(&ctrl, left, 0, false),
            Some(KeyAction::SwitchDesktop(Direction::Left))
        );
        assert_eq!(
            key_action(&ctrl, right, 0, false),
            Some(KeyAction::SwitchDesktop(Direction::Right))
        );
        assert_eq!(key_action(&ctrl_shift, left, 0, false), None);
        assert_eq!(key_action(&ctrl_shift, right, 0, false), None);
    }

    #[test]
    fn ctrl_super_keys_tile_also_with_caps_lock_and_ctrl_super_left_does_not_switch_desktop() {
        let ctrl_super = ModifiersState {
            ctrl: true,
            logo: true,
            ..Default::default()
        };
        let action = |keysym| key_action(&ctrl_super, Keysym::new(keysym), 0, false);
        assert_eq!(action(keysyms::KEY_f), Some(KeyAction::Tile(Tile::Fill)));
        assert_eq!(action(keysyms::KEY_F), Some(KeyAction::Tile(Tile::Fill)));
        let ctrl_alt = ModifiersState {
            ctrl: true,
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            key_action(&ctrl_alt, Keysym::new(keysyms::KEY_f), 0, false),
            Some(KeyAction::ToggleFullScreen)
        );
        assert_eq!(action(keysyms::KEY_C), Some(KeyAction::Tile(Tile::Center)));
        assert_eq!(action(keysyms::KEY_R), Some(KeyAction::RestoreTiled));
        assert_eq!(action(keysyms::KEY_Left), Some(KeyAction::Tile(Tile::Left)));
        assert_eq!(
            action(keysyms::KEY_Down),
            Some(KeyAction::Tile(Tile::Bottom))
        );
        assert_eq!(
            key_action(
                &ModifiersState::default(),
                Keysym::new(keysyms::KEY_f),
                0,
                false
            ),
            None
        );
    }

    #[test]
    fn ctrl_up_opens_expansion_and_while_open_ctrl_up_ctrl_down_and_escape_close_it_ctrl_left_switches_desktop_and_tiling_keys_do_nothing()
     {
        let ctrl = ModifiersState {
            ctrl: true,
            ..Default::default()
        };
        let ctrl_super = ModifiersState { logo: true, ..ctrl };
        let up = Keysym::new(keysyms::KEY_Up);
        let down = Keysym::new(keysyms::KEY_Down);
        let escape = Keysym::new(keysyms::KEY_Escape);
        let left = Keysym::new(keysyms::KEY_Left);
        assert_eq!(
            key_action(&ctrl, up, 0, false),
            Some(KeyAction::OpenExpansion)
        );
        assert_eq!(
            key_action(&ctrl, up, 0, true),
            Some(KeyAction::CloseExpansion)
        );
        assert_eq!(
            key_action(&ctrl, down, 0, true),
            Some(KeyAction::CloseExpansion)
        );
        assert_eq!(
            key_action(&ModifiersState::default(), escape, 0, true),
            Some(KeyAction::CloseExpansion)
        );
        assert_eq!(
            key_action(&ModifiersState::default(), escape, 0, false),
            None
        );
        assert_eq!(
            key_action(&ctrl_super, up, 0, false),
            Some(KeyAction::Tile(Tile::Top))
        );
        assert_eq!(key_action(&ctrl_super, up, 0, true), None);
        assert_eq!(
            key_action(&ctrl, left, 0, true),
            Some(KeyAction::SwitchDesktop(Direction::Left))
        );
    }

    #[test]
    fn ctrl_and_a_digit_row_key_while_expansion_is_open_go_to_that_desktop_whatever_the_layout_and_zero_is_the_tenth()
     {
        let ctrl = ModifiersState {
            ctrl: true,
            ..Default::default()
        };
        let ctrl_shift = ModifiersState {
            shift: true,
            ..ctrl
        };
        let one = Keysym::new(keysyms::KEY_1);
        let ampersand = Keysym::new(keysyms::KEY_ampersand);
        let zero = Keysym::new(keysyms::KEY_0);
        assert_eq!(
            key_action(&ctrl, one, 10, true),
            Some(KeyAction::GoToDesktop(1))
        );
        assert_eq!(
            key_action(&ctrl, ampersand, 10, true),
            Some(KeyAction::GoToDesktop(1))
        );
        assert_eq!(
            key_action(&ctrl, zero, 19, true),
            Some(KeyAction::GoToDesktop(10))
        );
        assert_eq!(key_action(&ctrl, one, 10, false), None);
        assert_eq!(key_action(&ctrl_shift, one, 10, true), None);
        assert_eq!(key_action(&ModifiersState::default(), one, 10, true), None);
    }

    #[test]
    fn space_alone_previews_only_while_expansion_is_open() {
        let space = Keysym::new(keysyms::KEY_space);
        let ctrl = ModifiersState {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(
            key_action(&ModifiersState::default(), space, 0, true),
            Some(KeyAction::Preview)
        );
        assert_eq!(
            key_action(&ModifiersState::default(), space, 0, false),
            None
        );
        assert_eq!(key_action(&ctrl, space, 0, true), None);
    }

    #[test]
    fn ctrl_down_opens_app_expose_only_while_expansion_is_closed() {
        let ctrl = ModifiersState {
            ctrl: true,
            ..Default::default()
        };
        let down = Keysym::new(keysyms::KEY_Down);
        assert_eq!(
            key_action(&ctrl, down, 0, false),
            Some(KeyAction::OpenAppExpose)
        );
        assert_eq!(
            key_action(&ctrl, down, 0, true),
            Some(KeyAction::CloseExpansion)
        );
    }

    #[test]
    fn xf86_switch_vt_n_switches_to_vt_n() {
        let modifiers = ModifiersState::default();
        assert_eq!(
            key_action(
                &modifiers,
                Keysym::new(keysyms::KEY_XF86Switch_VT_1),
                0,
                false
            ),
            Some(KeyAction::SwitchVt(1))
        );
        assert_eq!(
            key_action(
                &modifiers,
                Keysym::new(keysyms::KEY_XF86Switch_VT_12),
                0,
                false
            ),
            Some(KeyAction::SwitchVt(12))
        );
    }
}
