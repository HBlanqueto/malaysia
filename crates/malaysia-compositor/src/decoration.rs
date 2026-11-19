use ctdesign::Titlebar;
use malaysia_scene::scene::Decoration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Insets {
    pub(crate) top: f64,
    pub(crate) right: f64,
    pub(crate) bottom: f64,
    pub(crate) left: f64,
}

impl Insets {
    pub(crate) const ZERO: Insets = Insets {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub(crate) fn horizontal(&self) -> f64 {
        self.left + self.right
    }

    pub(crate) fn vertical(&self) -> f64 {
        self.top + self.bottom
    }

    pub(crate) fn for_window(
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
pub(crate) enum Role {
    Toplevel,
    Dialog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowState {
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
        crate::config::DecorationPolicy::Default => requested.unwrap_or(Decoration::Server),
    }
}

fn titlebar_inset(titlebar: &Titlebar) -> Insets {
    Insets {
        top: f64::from(titlebar.thickness),
        ..Insets::ZERO
    }
}

#[cfg(test)]
mod tests {
    use ctdesign::TITLEBAR;

    use super::{Insets, Role, WindowState, effective_decoration};
    use crate::config::DecorationPolicy;
    use malaysia_scene::scene::Decoration;

    #[test]
    fn only_a_server_decorated_toplevel_reserves_the_titlebar() {
        let top = f64::from(TITLEBAR.thickness);
        let inset =
            |role, window, decoration| Insets::for_window(role, window, decoration, &TITLEBAR);
        for window in [WindowState::Normal, WindowState::Maximized] {
            let insets = inset(Role::Toplevel, window, Decoration::Server);
            assert_eq!((insets.vertical(), insets.horizontal()), (top, 0.0));
        }
        assert_eq!(
            inset(Role::Dialog, WindowState::Normal, Decoration::Server),
            Insets::ZERO
        );
        assert_eq!(
            inset(Role::Toplevel, WindowState::Fullscreen, Decoration::Server),
            Insets::ZERO
        );
        assert_eq!(
            inset(Role::Toplevel, WindowState::Normal, Decoration::Client),
            Insets::ZERO
        );
    }

    #[test]
    fn the_default_policy_honours_the_request_and_prefers_server_side() {
        for (requested, expected) in [
            (None, Decoration::Server),
            (Some(Decoration::Server), Decoration::Server),
            (Some(Decoration::Client), Decoration::Client),
        ] {
            assert_eq!(
                effective_decoration(DecorationPolicy::Default, requested, false),
                expected
            );
        }
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
}
