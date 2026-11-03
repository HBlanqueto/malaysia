//! `zwp_virtual_keyboard_v1` and `zwlr_virtual_pointer_v1`: synthetic input from trusted clients.

use std::sync::Arc;

use smithay::backend::input::{Event, InputEvent, KeyboardKeyEvent};
use smithay::reexports::wayland_server::Client;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::input_method::InputMethodSeat;
use smithay::wayland::virtual_keyboard::{
    VirtualKeyboardBackend, VirtualKeyboardDevice, VirtualKeyboardHandler,
    VirtualKeyboardSpecialEvent,
};
use smithay::wayland::virtual_pointer::{VirtualPointerBackend, VirtualPointerHandler};

use crate::state::{ClientState, State};

pub(crate) fn trusted(client: &Client) -> bool {
    client
        .get_data::<ClientState>()
        .is_some_and(|state| state.trusted_shell)
}

impl VirtualKeyboardHandler for State {
    fn process_virtual_keyboard_event(&mut self, event: InputEvent<VirtualKeyboardBackend>) {
        match event {
            InputEvent::Keyboard { event } => {
                let device = event.device();
                self.activate_virtual_keymap(&device);
                let grab_client = self.seat.input_method().keyboard_grab_client();
                // An input method forwards the keys it does not consume; sending them back to its grab loops.
                if grab_client.is_some() && grab_client == device.client() {
                    if let Some(keyboard) = self.seat.get_keyboard() {
                        keyboard.input_forward_bypassing_grab(
                            self,
                            event.key_code(),
                            event.state(),
                            SERIAL_COUNTER.next_serial(),
                            event.time(),
                            true,
                        );
                    }
                } else {
                    self.process_input_event(InputEvent::<VirtualKeyboardBackend>::Keyboard {
                        event,
                    });
                }
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::Modifiers {
                device,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            }) => {
                self.activate_virtual_keymap(&device);
                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.with_xkb_state(self, |mut context| {
                        context.set_modifier_mask(mods_depressed, mods_latched, mods_locked, group);
                    });
                }
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::KeymapChanged { .. }) => {}
            InputEvent::DeviceRemoved { device } => {
                if self
                    .virtual_keymap
                    .as_ref()
                    .is_some_and(|(active, _)| *active == device)
                {
                    self.restore_configured_keymap();
                }
                self.process_input_event(InputEvent::<VirtualKeyboardBackend>::DeviceRemoved {
                    device,
                });
            }
            event => self.process_input_event(event),
        }
    }
}

impl VirtualPointerHandler for State {
    fn process_virtual_pointer_event(&mut self, event: InputEvent<VirtualPointerBackend>) {
        self.process_input_event(event);
    }
}

impl State {
    fn activate_virtual_keymap(&mut self, device: &VirtualKeyboardDevice) {
        let Some(keymap) = device.keymap() else {
            self.restore_configured_keymap();
            return;
        };
        if let Some((active, active_keymap)) = &mut self.virtual_keymap
            && *active_keymap == keymap
        {
            *active = device.clone();
            return;
        }
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        if let Err(error) = keyboard.set_keymap_from_string(self, keymap.to_string()) {
            tracing::warn!(?error, "could not apply the virtual keyboard keymap");
            return;
        }
        self.virtual_keymap = Some((device.clone(), Arc::clone(&keymap)));
    }

    pub(crate) fn restore_configured_keymap(&mut self) {
        if self.virtual_keymap.take().is_none() {
            return;
        }
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let keymap = self.keymap.clone();
        if let Err(error) = keyboard.set_xkb_config(self, keymap.xkb_config()) {
            tracing::warn!(?error, "could not restore the configured keymap");
        }
    }
}
