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
                keysyms::KEY_3 => return CompositorShortcut::ScreenshotFull,
                keysyms::KEY_4 => return CompositorShortcut::ScreenshotRegion,
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
    if logo
        && !ctrl
        && !alt
        && !shift
        && syms.iter().any(|sym| u32::from(*sym) == keysyms::KEY_space)
    {
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
        if matches!(sym, keysyms::KEY_l | keysyms::KEY_L) {
            return CompositorShortcut::Relock;
        }
    }
    CompositorShortcut::None
}

#[cfg(test)]
mod tests {
    use super::{CompositorShortcut, compositor_shortcut};
    use xkbcommon::xkb::{self, keysyms};

    #[test]
    fn super_letters_are_the_app_menu_shortcuts_only_when_unlocked() {
        let key = |sym: u32| [xkb::Keysym::from(sym)];
        let shortcut = |alt: bool, sym: u32, locked: bool| {
            compositor_shortcut(false, alt, false, true, &key(sym), locked)
        };
        assert_eq!(
            shortcut(false, keysyms::KEY_h, false),
            CompositorShortcut::HideApp
        );
        assert_eq!(
            shortcut(true, keysyms::KEY_h, false),
            CompositorShortcut::HideOtherApps
        );
        assert_eq!(
            shortcut(false, keysyms::KEY_q, false),
            CompositorShortcut::QuitApp
        );
        assert_eq!(
            shortcut(false, keysyms::KEY_m, false),
            CompositorShortcut::MinimizeWindow
        );
        assert_eq!(
            shortcut(false, keysyms::KEY_w, false),
            CompositorShortcut::CloseWindow
        );
        assert_eq!(
            shortcut(true, keysyms::KEY_q, false),
            CompositorShortcut::None
        );
        assert_eq!(
            shortcut(false, keysyms::KEY_q, true),
            CompositorShortcut::None
        );
        assert_eq!(
            compositor_shortcut(true, false, false, true, &key(keysyms::KEY_q), false),
            CompositorShortcut::None
        );
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
        let l = [keysyms::KEY_l.into()];
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
        let l = xkb::Keysym::new(keysyms::KEY_l);
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
}
