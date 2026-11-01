//! Space reserved around a client's surface for titlebar and other chrome.

use malaysia_scene::scene::Decoration;
use vow_design::{Titlebar, TitlebarPosition};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Insets {
    pub const ZERO: Insets = Insets {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub fn horizontal(&self) -> f64 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f64 {
        self.top + self.bottom
    }

    pub fn for_window(
        role: Role,
        window: WindowState,
        decoration: Decoration,
        titlebar: &Titlebar,
    ) -> Insets {
        match (role, window, decoration) {
            (Role::Toplevel, WindowState::Normal | WindowState::Maximized, Decoration::Server) => {
                titlebar_inset(titlebar)
            }
            _ => Insets::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Toplevel,
    Dialog,
    Popup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Maximized,
    Fullscreen,
}

pub(crate) fn effective_decoration(
    policy: crate::config::DecorationPolicy,
    requested: Option<Decoration>,
    native: bool,
) -> Decoration {
    if native {
        return requested.unwrap_or(Decoration::Client);
    }
    match policy {
        crate::config::DecorationPolicy::ForceServer => Decoration::Server,
        crate::config::DecorationPolicy::ForceClient => Decoration::Client,
        crate::config::DecorationPolicy::Default => requested.unwrap_or(Decoration::Client),
    }
}

fn titlebar_inset(titlebar: &Titlebar) -> Insets {
    let thickness = f64::from(titlebar.thickness);
    match titlebar.position {
        TitlebarPosition::Top => Insets {
            top: thickness,
            ..Insets::ZERO
        },
        TitlebarPosition::Bottom => Insets {
            bottom: thickness,
            ..Insets::ZERO
        },
        TitlebarPosition::Left => Insets {
            left: thickness,
            ..Insets::ZERO
        },
        TitlebarPosition::Right => Insets {
            right: thickness,
            ..Insets::ZERO
        },
        TitlebarPosition::None => Insets::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use vow_design::{TITLEBAR, Titlebar, TitlebarPosition};

    use super::{Insets, Role, WindowState, effective_decoration};
    use crate::config::DecorationPolicy;
    use malaysia_scene::scene::Decoration;

    fn titlebar(position: TitlebarPosition, thickness: u32) -> Titlebar {
        Titlebar {
            position,
            thickness,
            ..TITLEBAR
        }
    }

    #[test]
    fn zero_is_empty_on_every_edge() {
        assert_eq!(
            Insets::ZERO,
            Insets {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            }
        );
        assert_eq!(Insets::ZERO.horizontal(), 0.0);
        assert_eq!(Insets::ZERO.vertical(), 0.0);
    }

    #[test]
    fn a_normal_toplevel_gets_a_top_inset() {
        let bar = titlebar(TitlebarPosition::Top, 24);
        let insets = Insets::for_window(
            Role::Toplevel,
            WindowState::Normal,
            Decoration::Server,
            &bar,
        );
        assert_eq!(insets.top, 24.0);
        assert_eq!(insets.left, 0.0);
        assert_eq!(insets.right, 0.0);
        assert_eq!(insets.bottom, 0.0);
        assert_eq!(insets.vertical(), 24.0);
        assert_eq!(insets.horizontal(), 0.0);
    }

    #[test]
    fn the_titlebar_side_follows_the_configuration() {
        let bar = titlebar(TitlebarPosition::Left, 32);
        let insets = Insets::for_window(
            Role::Toplevel,
            WindowState::Normal,
            Decoration::Server,
            &bar,
        );
        assert_eq!(insets.left, 32.0);
        assert_eq!(insets.top, 0.0);
        assert_eq!(insets.horizontal(), 32.0);

        let bar = titlebar(TitlebarPosition::Bottom, 20);
        let insets = Insets::for_window(
            Role::Toplevel,
            WindowState::Normal,
            Decoration::Server,
            &bar,
        );
        assert_eq!(insets.bottom, 20.0);
        assert_eq!(insets.vertical(), 20.0);
    }

    #[test]
    fn no_titlebar_reserves_nothing() {
        let bar = titlebar(TitlebarPosition::None, 32);
        let insets = Insets::for_window(
            Role::Toplevel,
            WindowState::Normal,
            Decoration::Server,
            &bar,
        );
        assert_eq!(insets, Insets::ZERO);
    }

    #[test]
    fn a_dialog_is_never_decorated() {
        let bar = titlebar(TitlebarPosition::Top, 32);
        let insets =
            Insets::for_window(Role::Dialog, WindowState::Normal, Decoration::Server, &bar);
        assert_eq!(insets, Insets::ZERO);
    }

    #[test]
    fn a_popup_is_never_decorated() {
        let bar = titlebar(TitlebarPosition::Top, 32);
        let insets = Insets::for_window(Role::Popup, WindowState::Normal, Decoration::Server, &bar);
        assert_eq!(insets, Insets::ZERO);
    }

    #[test]
    fn fullscreen_reserves_nothing() {
        let bar = titlebar(TitlebarPosition::Top, 32);
        let insets = Insets::for_window(
            Role::Toplevel,
            WindowState::Fullscreen,
            Decoration::Server,
            &bar,
        );
        assert_eq!(insets, Insets::ZERO);
    }

    #[test]
    fn client_side_chrome_reserves_nothing() {
        let bar = titlebar(TitlebarPosition::Top, 32);
        let insets = Insets::for_window(
            Role::Toplevel,
            WindowState::Normal,
            Decoration::Client,
            &bar,
        );
        assert_eq!(insets, Insets::ZERO);
    }

    #[test]
    fn default_policy_prefers_client_side_when_nothing_is_requested() {
        assert_eq!(
            effective_decoration(DecorationPolicy::Default, None, false),
            Decoration::Client
        );
    }

    #[test]
    fn default_policy_honours_an_explicit_server_request() {
        assert_eq!(
            effective_decoration(DecorationPolicy::Default, Some(Decoration::Server), false),
            Decoration::Server
        );
    }

    #[test]
    fn default_policy_honours_an_explicit_client_request() {
        assert_eq!(
            effective_decoration(DecorationPolicy::Default, Some(Decoration::Client), false),
            Decoration::Client
        );
    }

    #[test]
    fn force_policies_override_any_request() {
        for requested in [None, Some(Decoration::Server), Some(Decoration::Client)] {
            assert_eq!(
                effective_decoration(DecorationPolicy::ForceServer, requested, false),
                Decoration::Server
            );
            assert_eq!(
                effective_decoration(DecorationPolicy::ForceClient, requested, false),
                Decoration::Client
            );
        }
    }

    #[test]
    fn a_maximized_window_keeps_its_titlebar_for_now() {
        let bar = titlebar(TitlebarPosition::Top, 32);
        let insets = Insets::for_window(
            Role::Toplevel,
            WindowState::Maximized,
            Decoration::Server,
            &bar,
        );
        assert_eq!(insets.top, 32.0);
    }
}
