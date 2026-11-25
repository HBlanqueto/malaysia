use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    InputTime, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent,
};
use smithay::desktop::{Window, WindowSurfaceType};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState, keysyms};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Serial, Size};

use crate::state::{Backend, State};

pub const REPEAT_DELAY: i32 = 200;
pub const REPEAT_RATE: i32 = 25;

#[derive(Debug, PartialEq)]
enum KeyAction {
    Quit,
    SwitchVt(i32),
}

fn key_action(modifiers: &ModifiersState, keysym: Keysym) -> Option<KeyAction> {
    match keysym.raw() {
        vt @ keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12 => Some(
            KeyAction::SwitchVt((vt - keysyms::KEY_XF86Switch_VT_1 + 1) as i32),
        ),
        keysyms::KEY_BackSpace if modifiers.ctrl && modifiers.alt => Some(KeyAction::Quit),
        _ => None,
    }
}

impl State {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
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
                            match key_action(modifiers, handle.modified_sym()) {
                                Some(action) => {
                                    state.suppressed_keys.push(keycode);
                                    FilterResult::Intercept(Some(action))
                                }
                                None => FilterResult::Forward,
                            }
                        },
                    )
                    .flatten();
                match action {
                    Some(KeyAction::Quit) => self.loop_signal.stop(),
                    Some(KeyAction::SwitchVt(vt)) => {
                        if let Backend::Drm(drm) = &mut self.backend {
                            drm.change_vt(vt);
                        }
                    }
                    None => {}
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let geometry = self.space.output_geometry(&self.output).unwrap();
                let inside =
                    Rectangle::new(geometry.loc, geometry.size - Size::from((1, 1))).to_f64();
                let location = self.seat.get_pointer().unwrap().current_location() + event.delta();
                self.move_pointer(location.constrain(inside), event.time());
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output_geometry = self.space.output_geometry(&self.output).unwrap();
                let location =
                    event.position_transformed(output_geometry.size) + output_geometry.loc.to_f64();
                self.move_pointer(location, event.time());
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let serial = SERIAL_COUNTER.next_serial();
                let state = event.state();
                if state == ButtonState::Pressed
                    && !pointer.is_grabbed()
                    && let Some((window, _)) = self.space.element_under(pointer.current_location())
                {
                    let window = window.clone();
                    self.focus_window(&window, serial);
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
            _ => {}
        }
    }

    fn move_pointer(&mut self, location: Point<f64, Logical>, time: InputTime) {
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
        self.needs_redraw = true;
    }

    pub fn focus_window(&mut self, window: &Window, serial: Serial) {
        self.space.raise_element(window, true);
        for window in self.space.elements() {
            let toplevel = window.toplevel().unwrap();
            if toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure();
            }
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
        self.space
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
        assert_eq!(key_action(&ctrl_alt, backspace), Some(KeyAction::Quit));
        assert_eq!(key_action(&ModifiersState::default(), backspace), None);
    }

    #[test]
    fn xf86_switch_vt_n_switches_to_vt_n() {
        let modifiers = ModifiersState::default();
        assert_eq!(
            key_action(&modifiers, Keysym::new(keysyms::KEY_XF86Switch_VT_1)),
            Some(KeyAction::SwitchVt(1))
        );
        assert_eq!(
            key_action(&modifiers, Keysym::new(keysyms::KEY_XF86Switch_VT_12)),
            Some(KeyAction::SwitchVt(12))
        );
    }
}
