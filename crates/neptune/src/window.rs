//! The tracked toplevels, behind one boundary.
//!
//! Block 1 has one output and one window, and no scene graph yet. This module
//! tracks toplevels directly rather than depending on Smithay's `desktop::Space`:
//! `Space` is replaced by `neptune-scene` in Block 2, so pulling it in would add
//! a dependency that gets deleted almost immediately (ADR 0009). No other module
//! reaches for a window's geometry — when Block 2 lands, this file is what is
//! rewritten, and the rest of the compositor is untouched.
//!
//! Two things this module is careful about:
//!
//! - **The initial configure.** A toplevel that is tracked without being sent a
//!   configure waits forever, with no error and no log. The configure is sent as
//!   soon as the toplevel exists, before the client attaches a buffer.
//! - **Insets are recomputed, never fixed.** Every commit reads the surface's
//!   current role and state and recomputes the insets. A `set_parent` can arrive
//!   after the window is mapped, and it can be unset again; a fullscreen change
//!   can come at any time. A configure that froze the insets at map time would
//!   be wrong from then on (ADR 0014).

use neptune_design::TITLEBAR;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Serial, Size};
use smithay::wayland::compositor::{BufferAssignment, SurfaceAttributes, with_states};
use smithay::wayland::shell::xdg::{Configure, ToplevelSurface};

use crate::decoration::{Chrome, Insets, Role, WindowState};

/// One tracked toplevel and where it sits.
pub struct Window {
    /// The client's toplevel surface.
    toplevel: ToplevelSurface,
    /// The outer top-left of the window, in logical coordinates.
    location: Point<i32, Logical>,
    /// The outer window size, in logical coordinates, excluding nothing.
    size: Size<i32, Logical>,
    /// Whether the client has acknowledged the initial configure.
    acked: bool,
    /// The serial of the configure most recently sent, awaiting an ack.
    pending_configure: Option<Serial>,
    /// Whether the window is mapped and should be shown.
    mapped: bool,
    /// The usable size last sent, so a state change can be noticed.
    configured: Option<Size<i32, Logical>>,
}

impl Window {
    /// The client's surface.
    pub fn surface(&self) -> &WlSurface {
        self.toplevel.wl_surface()
    }

    /// The outer top-left of the window, in logical coordinates.
    pub fn location(&self) -> Point<i32, Logical> {
        self.location
    }

    /// The outer window size, in logical coordinates.
    pub fn size(&self) -> Size<i32, Logical> {
        self.size
    }

    /// Whether the window is mapped and should be shown.
    pub fn is_mapped(&self) -> bool {
        self.mapped
    }

    /// The insets for the window's current role, state and chrome.
    pub fn insets(&self) -> Insets {
        Insets::for_window(self.role(), self.state(), self.chrome(), &TITLEBAR)
    }

    /// The size configured to the client: the window minus its insets.
    pub fn usable_size(&self) -> Size<i32, Logical> {
        usable(self.size, self.insets())
    }

    /// Where the client's surface sits inside the window, past the left and top
    /// insets.
    pub fn surface_location(&self) -> Point<i32, Logical> {
        let insets = self.insets();
        Point::from((round(insets.left), round(insets.top)))
    }

    /// A toplevel with a parent is a dialog; otherwise an ordinary toplevel.
    fn role(&self) -> Role {
        if self.toplevel.parent().is_some() {
            Role::Dialog
        } else {
            Role::Toplevel
        }
    }

    /// The window state that affects decoration, from the surface's state.
    fn state(&self) -> WindowState {
        let states = self.toplevel.current_state().states;
        if states.contains(xdg_toplevel::State::Fullscreen) {
            WindowState::Fullscreen
        } else if states.contains(xdg_toplevel::State::Maximized) {
            WindowState::Maximized
        } else {
            WindowState::Normal
        }
    }

    /// Who draws the chrome. Until xdg_decoration is served, this is always the
    /// server: clients cannot have negotiated anything else.
    fn chrome(&self) -> Chrome {
        match self.toplevel.current_state().decoration_mode {
            Some(zxdg_toplevel_decoration_v1::Mode::ClientSide) => Chrome::Client,
            _ => Chrome::Server,
        }
    }

    /// Sends a configure carrying the usable size for the current state.
    fn send_configure(&mut self) {
        let usable = usable(self.size, self.insets());
        self.toplevel.with_pending_state(|state| {
            state.size = Some(usable);
        });
        self.pending_configure = Some(self.toplevel.send_configure());
        self.configured = Some(usable);
    }
}

/// The toplevels being tracked.
#[derive(Default)]
pub struct Windows {
    /// Tracked windows, in creation order.
    windows: Vec<Window>,
}

impl Windows {
    /// Creates an empty set of tracked windows.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts tracking `toplevel` at `size` and sends its initial configure.
    ///
    /// `size` is the outer window size; the configure carries it minus the
    /// insets, so the client can size its surface for the decorated window from
    /// the start.
    pub fn add_toplevel(&mut self, toplevel: ToplevelSurface, size: Size<i32, Logical>) {
        let mut window = Window {
            toplevel,
            location: Point::from((0, 0)),
            size,
            acked: false,
            pending_configure: None,
            mapped: false,
            configured: None,
        };
        window.send_configure();
        self.windows.push(window);
    }

    /// Handles a surface commit.
    ///
    /// Recomputes the insets from current state and sends a fresh configure if
    /// they changed, then maps the window once the initial configure has been
    /// acked and a buffer has been attached.
    pub fn commit(&mut self, surface: &WlSurface) {
        let Some(window) = self.find_mut(surface) else {
            return;
        };

        let usable = usable(window.size, window.insets());
        if window.configured != Some(usable) {
            window.send_configure();
        }

        if window.acked && has_buffer(surface) {
            window.mapped = true;
        }
    }

    /// Records that the client acknowledged a configure for `surface`.
    ///
    /// Only the serial of the configure most recently sent is accepted. An ack
    /// for any other serial is stale — the client is acknowledging a configure
    /// Neptune has already replaced — so it is discarded rather than marking
    /// the window ready, which would let a reconfiguration race a stale ack
    /// into wrong geometry once there is more than one window.
    pub fn ack_configure(&mut self, surface: &WlSurface, configure: Configure) {
        let Configure::Toplevel(configure) = configure else {
            return;
        };
        let Some(window) = self.find_mut(surface) else {
            return;
        };
        if window.pending_configure == Some(configure.serial) {
            window.pending_configure = None;
            window.acked = true;
        } else {
            tracing::debug!("ignoring an ack for a configure no longer pending");
        }
    }

    /// Stops tracking the toplevel for `surface`.
    pub fn remove(&mut self, surface: &WlSurface) {
        self.windows.retain(|window| window.surface() != surface);
    }

    /// The tracked windows, in creation order.
    pub fn iter(&self) -> impl Iterator<Item = &Window> {
        self.windows.iter()
    }

    fn find_mut(&mut self, surface: &WlSurface) -> Option<&mut Window> {
        self.windows
            .iter_mut()
            .find(|window| window.surface() == surface)
    }
}

/// The configured size: the outer size minus the insets, never below one pixel.
fn usable(window: Size<i32, Logical>, insets: Insets) -> Size<i32, Logical> {
    let width = (f64::from(window.w) - insets.horizontal()).max(1.0).round();
    let height = (f64::from(window.h) - insets.vertical()).max(1.0).round();
    Size::from((width as i32, height as i32))
}

/// Rounds a logical length to whole pixels.
fn round(value: f64) -> i32 {
    value.round() as i32
}

/// Whether the surface currently has a buffer attached.
///
/// A commit may attach a buffer or remove one (a `NULL` attach), so this checks
/// for an actual new buffer rather than for the presence of an entry.
fn has_buffer(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        let mut cached = states.cached_state.get::<SurfaceAttributes>();
        matches!(
            cached.current().buffer.as_ref(),
            Some(BufferAssignment::NewBuffer(_))
        )
    })
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Size};

    use super::usable;
    use crate::decoration::Insets;

    fn size(width: i32, height: i32) -> Size<i32, Logical> {
        Size::from((width, height))
    }

    #[test]
    fn usable_size_subtracts_the_top_inset() {
        let insets = Insets {
            top: 32.0,
            ..Insets::ZERO
        };
        assert_eq!(usable(size(800, 600), insets), size(800, 568));
    }

    #[test]
    fn usable_size_subtracts_both_axes() {
        let insets = Insets {
            top: 10.0,
            right: 5.0,
            bottom: 10.0,
            left: 5.0,
        };
        assert_eq!(usable(size(800, 600), insets), size(790, 580));
    }

    #[test]
    fn usable_size_is_the_window_when_nothing_is_reserved() {
        assert_eq!(usable(size(800, 600), Insets::ZERO), size(800, 600));
    }

    #[test]
    fn usable_size_never_collapses() {
        let insets = Insets {
            top: 500.0,
            bottom: 500.0,
            ..Insets::ZERO
        };
        assert_eq!(usable(size(800, 600), insets), size(800, 1));
    }
}
