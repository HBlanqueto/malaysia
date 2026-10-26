//! Wayland input to Freya's `PlatformEvent`, following `freya-winit`'s
//! `winit_mappings.rs` but with the keys SCTK gives us.

use freya_core::prelude::{Code, Key, MouseButton, NamedKey};
use sctk::seat::keyboard::{Keysym, Modifiers as SctkModifiers};

pub fn modifiers(m: SctkModifiers) -> keyboard_types::Modifiers {
    let mut out = keyboard_types::Modifiers::empty();
    if m.ctrl {
        out |= keyboard_types::Modifiers::CONTROL;
    }
    if m.alt {
        out |= keyboard_types::Modifiers::ALT;
    }
    if m.shift {
        out |= keyboard_types::Modifiers::SHIFT;
    }
    if m.logo {
        out |= keyboard_types::Modifiers::META;
    }
    if m.caps_lock {
        out |= keyboard_types::Modifiers::CAPS_LOCK;
    }
    if m.num_lock {
        out |= keyboard_types::Modifiers::NUM_LOCK;
    }
    out
}

fn named(keysym: Keysym) -> Option<NamedKey> {
    Some(match keysym {
        Keysym::Return | Keysym::KP_Enter => NamedKey::Enter,
        Keysym::BackSpace => NamedKey::Backspace,
        Keysym::Tab => NamedKey::Tab,
        Keysym::Escape => NamedKey::Escape,
        Keysym::Delete => NamedKey::Delete,
        Keysym::Insert => NamedKey::Insert,
        Keysym::Home => NamedKey::Home,
        Keysym::End => NamedKey::End,
        Keysym::Page_Up => NamedKey::PageUp,
        Keysym::Page_Down => NamedKey::PageDown,
        Keysym::Left => NamedKey::ArrowLeft,
        Keysym::Right => NamedKey::ArrowRight,
        Keysym::Up => NamedKey::ArrowUp,
        Keysym::Down => NamedKey::ArrowDown,
        Keysym::Shift_L | Keysym::Shift_R => NamedKey::Shift,
        Keysym::Control_L | Keysym::Control_R => NamedKey::Control,
        Keysym::Alt_L | Keysym::Alt_R => NamedKey::Alt,
        Keysym::Super_L | Keysym::Super_R => NamedKey::Meta,
        Keysym::Caps_Lock => NamedKey::CapsLock,
        Keysym::Num_Lock => NamedKey::NumLock,
        Keysym::F1 => NamedKey::F1,
        Keysym::F2 => NamedKey::F2,
        Keysym::F3 => NamedKey::F3,
        Keysym::F4 => NamedKey::F4,
        Keysym::F5 => NamedKey::F5,
        Keysym::F6 => NamedKey::F6,
        Keysym::F7 => NamedKey::F7,
        Keysym::F8 => NamedKey::F8,
        Keysym::F9 => NamedKey::F9,
        Keysym::F10 => NamedKey::F10,
        Keysym::F11 => NamedKey::F11,
        Keysym::F12 => NamedKey::F12,
        _ => return None,
    })
}

fn code(keysym: Keysym) -> Code {
    match keysym {
        Keysym::Return | Keysym::KP_Enter => Code::Enter,
        Keysym::BackSpace => Code::Backspace,
        Keysym::Tab => Code::Tab,
        Keysym::Escape => Code::Escape,
        Keysym::Delete => Code::Delete,
        Keysym::Insert => Code::Insert,
        Keysym::Home => Code::Home,
        Keysym::End => Code::End,
        Keysym::Page_Up => Code::PageUp,
        Keysym::Page_Down => Code::PageDown,
        Keysym::Left => Code::ArrowLeft,
        Keysym::Right => Code::ArrowRight,
        Keysym::Up => Code::ArrowUp,
        Keysym::Down => Code::ArrowDown,
        Keysym::Shift_L => Code::ShiftLeft,
        Keysym::Shift_R => Code::ShiftRight,
        Keysym::Control_L => Code::ControlLeft,
        Keysym::Control_R => Code::ControlRight,
        Keysym::Alt_L => Code::AltLeft,
        Keysym::Alt_R => Code::AltRight,
        Keysym::Super_L => Code::MetaLeft,
        Keysym::Super_R => Code::MetaRight,
        Keysym::Caps_Lock => Code::CapsLock,
        Keysym::space => Code::Space,
        _ => Code::Unidentified,
    }
}

pub fn key(keysym: Keysym, utf8: Option<String>) -> (Key, Code) {
    if keysym == Keysym::space {
        return (Key::Character(" ".into()), Code::Space);
    }
    let raw = keysym.raw();
    let mapped = match named(keysym) {
        Some(name) => Key::Named(name),
        None => match keysym.key_char() {
            Some(ch) => Key::Character(ch.to_ascii_lowercase().to_string()),
            None => {
                if (0x20..=0x7e).contains(&raw) {
                    Key::Character((raw as u8 as char).to_ascii_lowercase().to_string())
                } else {
                    match utf8.as_deref() {
                        Some(text) if !text.is_empty() => Key::Character(text.to_string()),
                        _ => Key::Named(NamedKey::Unidentified),
                    }
                }
            }
        },
    };
    (mapped, code(keysym))
}

pub fn mouse_button(button: u32) -> MouseButton {
    use sctk::seat::pointer::{BTN_BACK, BTN_FORWARD, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT};
    match button {
        BTN_LEFT => MouseButton::Left,
        BTN_RIGHT => MouseButton::Right,
        BTN_MIDDLE => MouseButton::Middle,
        BTN_BACK => MouseButton::Back,
        BTN_FORWARD => MouseButton::Forward,
        other => MouseButton::Other(other as u16),
    }
}
