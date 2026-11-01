//! Spaces in the compositor: which windows the shown space holds (ADR 0051).
//!
//! The row itself lives in [`malaysia_scene::spaces`]; this module keeps the
//! window tracker in step with it. A window on a space that is not shown is
//! taken out of the scene the way a minimized one is (`set_off_space`), so
//! drawing, hit-testing and focus all skip it with no extra checks.
//!
//! Switching slides the row horizontally with a black gap between spaces
//! (17b). The windows of the target space take input at once; the slide is
//! only drawn ([`SpaceSlide`]), so a fast second press retargets it from where
//! it is.
//!
//! Focus mode (17c): maximizing gives the window a focus space right of its
//! desktop. The row slides there while the window grows from its frame to the
//! whole output ([`FocusTransition`]), drawn above the dock, which stays put
//! until the window covers it. The window gets the xdg `fullscreen` state; in
//! the focus space the bar and the dock are hidden and the background is
//! black. Leaving runs the same animation backwards and only then removes the
//! space, so the black space is what slides away.
//!
//! The top edge (17d): in a still focus space, the pointer at the top edge
//! brings the bar down over black ([`FocusReveal`]). A window the compositor
//! decorates (SSD) also gets its titlebar back, under the bar, over its
//! content. A client-decorated window (barrierkit, libadwaita) slides down by
//! the bar's height instead, so its own bar stays visible under the panel.

use std::time::{Duration, Instant};

use malaysia_scene::geometry::{
    Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
};
use malaysia_scene::scene::WindowId;
use malaysia_scene::spaces::{SpaceId, SpaceKind, SpacesError};
use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::state::State;

/// How long one slide takes (Tengen, `vow-design`).
const SLIDE: Duration = Duration::from_millis(vow_design::SPACE_SLIDE_MS);

/// How long a window that grew into focus mode may keep being drawn by the
/// transition after the slide, waiting for the client to commit a buffer at
/// the output size. Without the wait, the frame where the transition hands
/// over shows the old, smaller buffer over black: a black flash. Not a design
/// value: a protocol timeout for slow clients.
const HANDOVER_WAIT: Duration = Duration::from_millis(300);

/// A slide of the row in progress: from a row position (in spaces, so `1.5`
/// is halfway between the second and third space) to a space.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SpaceSlide {
    from: f64,
    to: SpaceId,
    start: Instant,
}

/// The CSS `cubic-bezier(x1, y1, x2, y2)` timing curve at progress `x` in
/// `[0, 1]`: finds the curve parameter whose x is `x` and returns its y.
pub(crate) fn cubic_bezier(curve: [f32; 4], x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let [x1, y1, x2, y2] = curve.map(f64::from);
    let axis = |t: f64, p1: f64, p2: f64| {
        let u = 1.0 - t;
        3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
    };
    // x(t) is monotonic for x1, x2 in [0, 1]: bisection always converges.
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (low + high) / 2.0;
        if axis(mid, x1, x2) < x {
            low = mid;
        } else {
            high = mid;
        }
    }
    axis((low + high) / 2.0, y1, y2)
}

/// A window growing into its focus space or shrinking back to its desktop.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FocusTransition {
    /// The window.
    pub(crate) window: WindowId,
    /// Its frame on the desktop, in output coordinates.
    desk: Rectangle<i32, Logical>,
    /// Growing into focus (`true`) or shrinking back (`false`).
    entering: bool,
    start: Instant,
}

/// One frame of a [`FocusTransition`]: where the window is drawn and with
/// which corner radius.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FocusFrame {
    /// The window.
    pub(crate) window: WindowId,
    /// The rectangle its visible frame fills, in output coordinates.
    pub(crate) rect: Rectangle<i32, Logical>,
    /// The corner radius, in logical pixels.
    pub(crate) radius: f64,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// The rectangle between `from` and `to` at eased progress `t`.
fn lerp_rect(
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
    t: f64,
) -> Rectangle<i32, Logical> {
    let at = |a: i32, b: i32| lerp(f64::from(a), f64::from(b), t).round() as i32;
    Rectangle::new(
        Point::from((at(from.loc.x, to.loc.x), at(from.loc.y, to.loc.y))),
        Size::from((at(from.size.w, to.size.w), at(from.size.h, to.size.h))),
    )
}

/// How long the bar takes to come down or go up at the top edge.
const REVEAL: Duration = Duration::from_millis(vow_design::DURATION_BASE);

/// The bar coming down in a focus space, and where its animation is.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FocusReveal {
    /// Whether the bar is (going) down.
    shown: bool,
    /// The progress when the last change started.
    from: f64,
    /// When the last change started; `None` while at rest.
    start: Option<Instant>,
    /// The client-decorated window moved down by the bar's height, to move
    /// back when the bar goes up.
    shifted: Option<WindowId>,
}

impl FocusReveal {
    /// Whether the bar is (going) down.
    pub(crate) fn is_shown(&self) -> bool {
        self.shown
    }

    /// How far down the bar is, from 0 (hidden) to 1 (down).
    pub(crate) fn progress(&self, now: Instant) -> f64 {
        let target = if self.shown { 1.0 } else { 0.0 };
        let Some(start) = self.start else {
            return target;
        };
        let t = now.saturating_duration_since(start).as_secs_f64() / REVEAL.as_secs_f64();
        lerp(
            self.from,
            target,
            cubic_bezier(vow_design::SPACE_SLIDE_EASE, t),
        )
    }

    /// Whether the bar is still moving.
    pub(crate) fn animating(&self, now: Instant) -> bool {
        self.start
            .is_some_and(|start| now.saturating_duration_since(start) < REVEAL)
    }
}

/// One frame of the top-edge reveal in a focus space.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RevealFrame {
    /// The window that owns the focus space.
    pub(crate) owner: WindowId,
    /// How far down the bar is, from 0 to 1.
    pub(crate) progress: f64,
    /// The bar's height, in logical pixels.
    pub(crate) bar: i32,
    /// Whether the compositor decorates the window (it gets its titlebar
    /// back) or the client does (it slides down).
    pub(crate) server_decorated: bool,
}

impl State {
    /// The window owning the shown focus space, while nothing slides.
    pub(crate) fn focus_owner(&self) -> Option<WindowId> {
        if self.space_slide.is_some() || self.focus_transition.is_some() {
            return None;
        }
        match self.spaces.current().kind {
            SpaceKind::Focus { owner, .. } => Some(owner),
            SpaceKind::Desktop => None,
        }
    }

    /// The height of the desktop bar: its layer surface's, or the token.
    pub(crate) fn desktop_bar_height(&self) -> i32 {
        self.layers
            .mapped()
            .iter()
            .find(|entry| entry.is_desktop_bar())
            .map(|entry| entry.geometry.size.h)
            .unwrap_or(vow_design::MENUBAR_HEIGHT)
    }

    /// Shows or hides the bar in the focus space. A client-decorated window
    /// moves down by the bar's height at once for input; the drawing follows
    /// the animation.
    pub(crate) fn set_focus_reveal(&mut self, shown: bool) {
        if self.focus_reveal.shown == shown {
            return;
        }
        let Some(owner) = self.focus_owner() else {
            return;
        };
        let now = Instant::now();
        let from = self.focus_reveal.progress(now);
        let mut shifted = self.focus_reveal.shifted.take();
        if shown && self.windows.is_client_decorated(owner) {
            self.windows
                .set_scene_y(owner, f64::from(self.desktop_bar_height()));
            shifted = Some(owner);
        } else if !shown && let Some(window) = shifted.take() {
            self.windows.set_scene_y(window, 0.0);
        }
        self.focus_reveal = FocusReveal {
            shown,
            from,
            start: Some(now),
            shifted,
        };
        self.layers.set_bar_revealed(shown);
        tracing::info!(window = owner.get(), shown, "focus space bar");
        self.send_space_state();
        self.request_redraw();
    }

    /// Follows the pointer for the top edge: at the very top the bar comes
    /// down; well below the bar (and the titlebar it brings) it goes up. An
    /// open bar menu keeps it down.
    pub(crate) fn update_focus_reveal(&mut self, location: Point<f64, Logical>) {
        if self.focus_owner().is_none() {
            return;
        }
        if location.y <= 1.0 {
            self.set_focus_reveal(true);
            return;
        }
        if !self.focus_reveal.shown || self.windows.topmost_popup().is_some() {
            return;
        }
        // Below the bar there is the SSD titlebar the compositor shows, or the
        // client's own bar (a libadwaita headerbar is about 47 px): two
        // titlebars of room cover both before the bar goes up.
        let reach =
            f64::from(self.desktop_bar_height()) + 2.0 * f64::from(vow_design::TITLEBAR.thickness);
        if location.y > reach {
            self.set_focus_reveal(false);
        }
    }

    /// The reveal for the frame being drawn, or `None` when the bar is up
    /// and still, or no focus space is shown.
    pub(crate) fn reveal_frame(&self, now: Instant) -> Option<RevealFrame> {
        let owner = self.focus_owner()?;
        let progress = self.focus_reveal.progress(now);
        (progress > 0.0).then(|| RevealFrame {
            owner,
            progress,
            bar: self.desktop_bar_height(),
            server_decorated: !self.windows.is_client_decorated(owner),
        })
    }

    /// The titlebar a server-decorated focus window shows while the bar is
    /// down: its outer rectangle and insets for the chrome hit-tests. `None`
    /// otherwise.
    pub(crate) fn revealed_titlebar(
        &self,
        id: WindowId,
    ) -> Option<(SceneRect<f64, SceneLogical>, crate::decoration::Insets)> {
        if self.focus_owner() != Some(id)
            || !self.focus_reveal.shown
            || self.windows.is_client_decorated(id)
        {
            return None;
        }
        let size = self.logical_output_size();
        let thickness = f64::from(vow_design::TITLEBAR.thickness);
        let outer = SceneRect::new(
            ScenePoint::new(0.0, f64::from(self.desktop_bar_height())),
            SceneSize::new(f64::from(size.w), thickness),
        );
        let insets = crate::decoration::Insets {
            top: thickness,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        };
        Some((outer, insets))
    }

    /// Puts the bar up at once, without animation (leaving a focus space).
    fn reset_focus_reveal(&mut self) {
        if let Some(window) = self.focus_reveal.shifted {
            self.windows.set_scene_y(window, 0.0);
        }
        self.focus_reveal = FocusReveal::default();
        self.layers.set_bar_revealed(false);
        self.send_space_state();
    }

    /// Whether a window is in focus mode (has a focus space).
    pub(crate) fn in_focus_mode(&self, id: WindowId) -> bool {
        self.spaces.focus_space_of(id).is_some()
    }

    /// Enters focus mode, or leaves it if the window is already there. What
    /// the green light, a client's maximize request and F11 do (ADR 0051).
    pub(crate) fn toggle_focus_mode(&mut self, id: WindowId) {
        if self.in_focus_mode(id) {
            self.exit_focus_mode(id);
        } else {
            self.enter_focus_mode(id);
        }
    }

    /// Gives a window its own focus space and slides there while it grows.
    pub(crate) fn enter_focus_mode(&mut self, id: WindowId) {
        if self.focus_transition.is_some() || self.in_focus_mode(id) || self.locked {
            return;
        }
        if self.has_fixed_width(id) {
            tracing::info!(
                window = id.get(),
                "focus mode refused: the window has a fixed width"
            );
            return;
        }
        // A window on another space is shown first, without its own slide.
        self.show_space_of(id);
        self.space_slide = None;
        let Some(desk) = self
            .windows
            .iter_visible(&self.minimized)
            .find(|window| window.id() == id)
            .map(|window| crate::session::window_frame_rect(&window))
        else {
            return;
        };
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        let index = match self.spaces.enter_focus(id) {
            Ok(index) => index,
            Err(error) => {
                tracing::warn!(window = id.get(), ?error, "focus mode refused");
                return;
            }
        };
        // Fullscreen first, while the window is still shown: its scene
        // rectangle becomes the output and the client is configured.
        self.fullscreen.insert(id, outer);
        let full = self.output_scene_rect();
        self.windows.set_fullscreen_with_outer(id, true, full);
        self.foreign_state_changed(id);
        tracing::info!(window = id.get(), index, "focus mode entered");
        self.switch_space(index);
        self.focus_transition = self.space_slide.map(|_| FocusTransition {
            window: id,
            desk,
            entering: true,
            start: Instant::now(),
        });
        self.raise_and_focus(id);
    }

    /// Leaves focus mode: the window shrinks back to its frame while the row
    /// slides to its desktop; the focus space is removed at the end.
    pub(crate) fn exit_focus_mode(&mut self, id: WindowId) {
        if self.focus_transition.is_some() {
            return;
        }
        let Some(focus) = self.spaces.focus_space_of(id) else {
            return;
        };
        let Some(SpaceKind::Focus { home, .. }) = self.spaces.get(focus).map(|space| space.kind)
        else {
            return;
        };
        self.show_space_of(id);
        self.space_slide = None;
        self.reset_focus_reveal();
        let saved = self
            .fullscreen
            .remove(&id)
            .or_else(|| self.windows.outer(id))
            .unwrap_or_else(|| self.output_scene_rect());
        self.windows.set_fullscreen_with_outer(id, false, saved);
        self.foreign_state_changed(id);
        let desk = self
            .windows
            .iter_visible(&self.minimized)
            .find(|window| window.id() == id)
            .map(|window| crate::session::window_frame_rect(&window));
        tracing::info!(window = id.get(), "focus mode left");
        let home_index = self
            .spaces
            .index_of(home)
            .expect("a focus space's home exists");
        self.switch_space(home_index);
        match (self.space_slide, desk) {
            (Some(_), Some(desk)) => {
                self.focus_transition = Some(FocusTransition {
                    window: id,
                    desk,
                    entering: false,
                    start: Instant::now(),
                });
            }
            _ => self.finish_focus_exit(id),
        }
    }

    /// Removes the focus space after the shrink and shows the window on its
    /// desktop again.
    fn finish_focus_exit(&mut self, id: WindowId) {
        if self.spaces.exit_focus(id).is_ok() {
            self.sync_space_visibility();
            self.raise_and_focus(id);
        }
    }

    /// The focus transition for the frame being drawn, or `None` when none
    /// runs. Ends a transition whose time is up (removing the focus space of
    /// a window that left focus).
    pub(crate) fn focus_transition_frame(&mut self, now: Instant) -> Option<FocusFrame> {
        let transition = self.focus_transition?;
        let elapsed = now.saturating_duration_since(transition.start);
        let size = self.logical_output_size();
        let full = Rectangle::new(Point::from((0, 0)), size);
        let committed = self
            .windows
            .iter()
            .find(|window| window.id() == transition.window)
            .map(|window| window.window_geometry().size);
        if transition.entering
            && elapsed >= SLIDE
            && elapsed < SLIDE + HANDOVER_WAIT
            && committed.is_some_and(|committed| committed != size)
        {
            // Hold the full rectangle until the client catches up.
            return Some(FocusFrame {
                window: transition.window,
                rect: full,
                radius: 0.0,
            });
        }
        if elapsed >= SLIDE || committed.is_none() {
            if transition.entering {
                tracing::info!(
                    window = transition.window.get(),
                    ?committed,
                    output = ?size,
                    late_ms = elapsed.saturating_sub(SLIDE).as_millis() as u64,
                    "focus transition handed over"
                );
            }
            self.focus_transition = None;
            if !transition.entering {
                self.finish_focus_exit(transition.window);
            }
            return None;
        }
        let eased = cubic_bezier(
            vow_design::SPACE_SLIDE_EASE,
            elapsed.as_secs_f64() / SLIDE.as_secs_f64(),
        );
        let corner = f64::from(vow_design::WINDOW.corner_radius);
        let (rect, radius) = if transition.entering {
            (
                lerp_rect(transition.desk, full, eased),
                lerp(corner, 0.0, eased),
            )
        } else {
            (
                lerp_rect(full, transition.desk, eased),
                lerp(0.0, corner, eased),
            )
        };
        Some(FocusFrame {
            window: transition.window,
            rect,
            radius,
        })
    }

    /// The whole output as a scene rectangle.
    fn output_scene_rect(&self) -> SceneRect<f64, SceneLogical> {
        let size = self.logical_output_size();
        SceneRect::new(
            ScenePoint::new(0.0, 0.0),
            SceneSize::new(f64::from(size.w), f64::from(size.h)),
        )
    }

    /// Where the row is right now, in spaces: the shown index when still,
    /// somewhere between two indices while sliding.
    pub(crate) fn space_position(&self, now: Instant) -> f64 {
        let current = self.spaces.current_index() as f64;
        let Some(slide) = self.space_slide else {
            return current;
        };
        let Some(to) = self.spaces.index_of(slide.to) else {
            return current;
        };
        let progress =
            now.saturating_duration_since(slide.start).as_secs_f64() / SLIDE.as_secs_f64();
        let eased = cubic_bezier(vow_design::SPACE_SLIDE_EASE, progress);
        slide.from + (to as f64 - slide.from) * eased
    }

    /// The row position for the frame being drawn, or `None` when the row is
    /// still. Ends a slide whose time is up.
    pub(crate) fn space_frame(&mut self, now: Instant) -> Option<f64> {
        let slide = self.space_slide?;
        let done = now.saturating_duration_since(slide.start) >= SLIDE
            || self.spaces.index_of(slide.to).is_none();
        if done {
            self.space_slide = None;
            return None;
        }
        Some(self.space_position(now))
    }

    /// Puts a newly mapped window on a space, switching to that space when it
    /// is not the shown one (a window opened from a focus space).
    pub(crate) fn space_window_mapped(&mut self, id: WindowId) {
        let parent = self.windows.parent_of(id);
        let assigned = self.spaces.assign(id, parent);
        if assigned.needs_switch
            && let Some(index) = self.spaces.index_of(assigned.space)
        {
            self.switch_space(index);
        }
    }

    /// Stops tracking a destroyed window. Its focus space, if any, goes away.
    pub(crate) fn space_window_removed(&mut self, id: WindowId) {
        if self
            .focus_transition
            .is_some_and(|transition| transition.window == id)
        {
            self.focus_transition = None;
        }
        self.fullscreen.remove(&id);
        let before = self.spaces.current().id;
        self.spaces.remove_window(id);
        if self.spaces.current().id != before {
            self.sync_space_visibility();
        } else {
            // The row may have lost a focus space that was not shown.
            self.send_space_state();
        }
    }

    /// Shows the space at `index`. Returns whether the shown space changed.
    pub(crate) fn switch_space(&mut self, index: usize) -> bool {
        let now = Instant::now();
        let from = self.space_position(now);
        if !self.spaces.switch_to(index) {
            return false;
        }
        self.space_slide = (!self.config.appearance.reduced_motion).then(|| SpaceSlide {
            from,
            to: self.spaces.current().id,
            start: now,
        });
        self.sync_space_visibility();
        true
    }

    /// Shows the space to the right (Ctrl+Alt+→). The row does not wrap.
    pub(crate) fn next_space(&mut self) -> bool {
        if self.focus_transition.is_some() {
            return false;
        }
        self.switch_space(self.spaces.current_index() + 1)
    }

    /// Shows the space to the left (Ctrl+Alt+←). The row does not wrap.
    pub(crate) fn previous_space(&mut self) -> bool {
        if self.focus_transition.is_some() {
            return false;
        }
        match self.spaces.current_index() {
            0 => false,
            index => self.switch_space(index - 1),
        }
    }

    /// Appends a desktop at the right end of the row and shows it
    /// (Ctrl+Alt+N). Refused at 99 desktops.
    pub(crate) fn add_desktop(&mut self) -> bool {
        match self.spaces.add_desktop() {
            Ok(id) => {
                tracing::info!(
                    desktop = self.spaces.desktop_number(id),
                    desktops = self.spaces.desktop_count(),
                    "desktop added"
                );
                let index = self
                    .spaces
                    .index_of(id)
                    .expect("a new desktop is in the row");
                self.switch_space(index);
                true
            }
            Err(SpacesError::Full) => {
                tracing::info!("desktop not added: the row already has 99 desktops");
                false
            }
            Err(error) => {
                tracing::warn!(?error, "desktop not added");
                false
            }
        }
    }

    /// Shows the space a window lives on, if it is not the shown one. Used
    /// before focusing a window from the dock or by activation.
    pub(crate) fn show_space_of(&mut self, id: WindowId) {
        if self.spaces.is_shown(id) {
            return;
        }
        if let Some(index) = self
            .spaces
            .space_of(id)
            .and_then(|space| self.spaces.index_of(space))
        {
            self.switch_space(index);
        }
    }

    /// Hides every window off the shown space and shows the ones on it, then
    /// moves keyboard focus onto the shown space.
    fn sync_space_visibility(&mut self) {
        // The bar of a focus space goes up with the space, before the window
        // that slid down under it is hidden.
        self.reset_focus_reveal();
        // Menus belong to a window or to the bar of the space being left.
        self.dismiss_all_popups();
        let ids: Vec<WindowId> = self.windows.iter().map(|window| window.id()).collect();
        for id in ids {
            self.windows.set_off_space(id, !self.spaces.is_shown(id));
        }
        let current = self.spaces.current();
        // A focus space hides the bar and the dock, for drawing and input.
        self.layers
            .set_chrome_hidden(matches!(current.kind, SpaceKind::Focus { .. }));
        tracing::info!(
            index = self.spaces.current_index(),
            spaces = self.spaces.len(),
            desktop = self.spaces.desktop_number(current.id),
            focus = matches!(current.kind, SpaceKind::Focus { .. }),
            "space shown"
        );
        let focused = self.focused_window();
        if focused.is_none_or(|id| !self.spaces.is_shown(id)) {
            match self.windows.topmost_visible(&self.minimized) {
                Some(next) => self.focus_window(next),
                None => self.clear_keyboard_focus(),
            }
        }
        self.send_space_state();
        self.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::cubic_bezier;

    #[test]
    fn rectangles_interpolate_from_one_to_the_other() {
        use smithay::utils::{Point, Rectangle, Size};
        let from = Rectangle::new(Point::from((100, 50)), Size::from((400, 300)));
        let to = Rectangle::new(Point::from((0, 0)), Size::from((1280, 800)));
        assert_eq!(super::lerp_rect(from, to, 0.0), from);
        assert_eq!(super::lerp_rect(from, to, 1.0), to);
        let half = super::lerp_rect(from, to, 0.5);
        assert_eq!(half.loc, Point::from((50, 25)));
        assert_eq!(half.size, Size::from((840, 550)));
    }

    #[test]
    fn the_curve_starts_at_zero_and_ends_at_one() {
        let curve = vow_design::SPACE_SLIDE_EASE;
        assert!(cubic_bezier(curve, 0.0).abs() < 1e-6);
        assert!((cubic_bezier(curve, 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_linear_curve_is_the_identity() {
        let linear = [0.25, 0.25, 0.75, 0.75];
        for x in [0.1, 0.33, 0.5, 0.9] {
            assert!((cubic_bezier(linear, x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn the_slide_curve_starts_fast() {
        let curve = vow_design::SPACE_SLIDE_EASE;
        assert!(cubic_bezier(curve, 0.25) > 0.5);
        let mut previous = 0.0;
        for step in 1..=20 {
            let y = cubic_bezier(curve, f64::from(step) / 20.0);
            assert!(y >= previous);
            previous = y;
        }
    }
}
