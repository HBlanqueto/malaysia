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

use std::time::{Duration, Instant};

use malaysia_scene::scene::WindowId;
use malaysia_scene::spaces::{SpaceId, SpaceKind, SpacesError};

use crate::state::State;

/// How long one slide takes (Tengen, `vow-design`).
const SLIDE: Duration = Duration::from_millis(vow_design::SPACE_SLIDE_MS);

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

impl State {
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
        let before = self.spaces.current().id;
        self.spaces.remove_window(id);
        if self.spaces.current().id != before {
            self.sync_space_visibility();
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
        self.switch_space(self.spaces.current_index() + 1)
    }

    /// Shows the space to the left (Ctrl+Alt+←). The row does not wrap.
    pub(crate) fn previous_space(&mut self) -> bool {
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
        // Menus belong to a window or to the bar of the space being left.
        self.dismiss_all_popups();
        let ids: Vec<WindowId> = self.windows.iter().map(|window| window.id()).collect();
        for id in ids {
            self.windows.set_off_space(id, !self.spaces.is_shown(id));
        }
        let current = self.spaces.current();
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
        self.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::cubic_bezier;

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
