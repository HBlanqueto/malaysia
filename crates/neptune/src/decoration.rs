//! How much room Neptune reserves around a client's surface.
//!
//! A client draws its own content; Neptune may draw a titlebar and other chrome
//! around it. [`Insets`] is the space that chrome takes from each edge, in
//! logical pixels, and it answers exactly one question: given this window, in
//! this state, what insets? [`Insets::for_window`] is the one place that
//! decision is made, and its precedence is role, then window state, then what
//! the client negotiated. The reasoning is in ADR 0014.
//!
//! Three things are deliberately **not** here:
//!
//! - **Shadows.** They draw outside the window and reserve no layout space;
//!   adjacent windows' shadows overlap. They belong to the scene graph, not to
//!   insets.
//! - **The input region.** It does not match the visual region: resize borders
//!   extend a few pixels *outside* the visible window, so the input region can
//!   be larger than the window. That is needed only once move and resize land,
//!   and it will be a sibling calculation, not a field here.
//! - **Widths and colors.** Nothing here hardcodes a number: the titlebar's
//!   side and thickness come from [`neptune_design`], and no caller may assume
//!   "the titlebar is N tall".
//!
//! Insets are a function of current state, computed on each call, never a
//! value captured when a window is mapped: fullscreen zeroes them today, and a
//! maximized or tiled window may change them later.

use neptune_design::{Titlebar, TitlebarPosition};
use neptune_scene::scene::Decoration;

/// The space reserved around a client's surface, in logical pixels.
///
/// All four values are non-negative while only visible chrome is modelled; a
/// future input region may use negative values to extend outside the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Insets {
    /// Space reserved above the surface, in logical pixels.
    pub top: f64,
    /// Space reserved to the right of the surface, in logical pixels.
    pub right: f64,
    /// Space reserved below the surface, in logical pixels.
    pub bottom: f64,
    /// Space reserved to the left of the surface, in logical pixels.
    pub left: f64,
}

impl Insets {
    /// No reserved space on any edge.
    pub const ZERO: Insets = Insets {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    /// The space reserved across the window's width.
    pub fn horizontal(&self) -> f64 {
        self.left + self.right
    }

    /// The space reserved across the window's height.
    pub fn vertical(&self) -> f64 {
        self.top + self.bottom
    }

    /// The insets for a window, with `titlebar` describing Neptune's chrome.
    ///
    /// This is the single place the decision is resolved. In order: a popup or
    /// a dialog (a toplevel with a parent) is never decorated; a fullscreen
    /// window reserves nothing; a client that draws its own chrome reserves
    /// nothing; only a normal or maximized toplevel with server-side chrome
    /// gets the titlebar inset on the edge [`neptune_design`] configures.
    ///
    /// `decoration` is the mode stored in the scene (ADR 0019): the negotiation
    /// happened in a protocol handler, and this function only consumes its
    /// result.
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

/// What a window is, from the shell protocol's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A normal application window: a toplevel with no parent.
    Toplevel,
    /// A dialog: a toplevel that has a parent set.
    Dialog,
    /// A popup: a menu, tooltip or similar transient surface.
    Popup,
}

/// The window state that affects decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    /// Ordinary, floating.
    Normal,
    /// Maximized. Keeps its titlebar today; may change later.
    Maximized,
    /// Fullscreen: covers the output, so nothing is reserved.
    Fullscreen,
}

/// The inset that a titlebar occupies on the edge it is configured on.
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
    use neptune_design::{TITLEBAR, Titlebar, TitlebarPosition};

    use super::{Insets, Role, WindowState};
    use neptune_scene::scene::Decoration;

    /// A titlebar on `position` with `thickness`, reusing the real metrics.
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
