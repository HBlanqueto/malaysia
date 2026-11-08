use crate::input::is_escape;
use xkbcommon::xkb::{self, keysyms};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompositorShortcut {
    Vt(u8),
    Quit,
    Blocked,
    Relock,
    Screenshot,
    ScreenshotFull,
    ScreenshotRegion,
    Launcher,
    ForceQuit,
    HideApp,
    HideOtherApps,
    QuitApp,
    MinimizeWindow,
    CloseWindow,
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

pub(crate) fn compositor_shortcut(
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
    if logo && !ctrl && !shift && !locked {
        for sym in syms {
            match u32::from(*sym) {
                keysyms::KEY_h | keysyms::KEY_H if alt => return CompositorShortcut::HideOtherApps,
                keysyms::KEY_h | keysyms::KEY_H => return CompositorShortcut::HideApp,
                keysyms::KEY_q | keysyms::KEY_Q if !alt => return CompositorShortcut::QuitApp,
                keysyms::KEY_m | keysyms::KEY_M if !alt => {
                    return CompositorShortcut::MinimizeWindow;
                }
                keysyms::KEY_w | keysyms::KEY_W if !alt => return CompositorShortcut::CloseWindow,
                _ => {}
            }
        }
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
