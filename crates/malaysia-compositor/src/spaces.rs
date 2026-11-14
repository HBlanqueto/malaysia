use std::time::{Duration, Instant};

use malaysia_scene::geometry::{
    Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
};
use malaysia_scene::scene::WindowId;
use malaysia_scene::spaces::{SpaceId, SpaceKind, SpacesError};
use smithay::utils::{Logical, Point, Rectangle, Size};
use smithay::wayland::shell::wlr_layer::Anchor;

use crate::state::State;

const SLIDE: Duration = Duration::from_millis(ctdesign::SPACE_SLIDE_MS);

const RELEASE_AT: f64 = 0.6;

const HANDOVER_WAIT: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpaceSlide {
    from: f64,
    to: SpaceId,
    start: Instant,
}

pub(crate) fn cubic_bezier(curve: [f32; 4], x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let [x1, y1, x2, y2] = curve.map(f64::from);
    let axis = |t: f64, p1: f64, p2: f64| {
        let u = 1.0 - t;
        3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
    };
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct FocusTransition {
    pub(crate) window: WindowId,
    desk: Rectangle<i32, Logical>,
    entering: bool,
    start: Instant,
    released: Option<Instant>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FocusFrame {
    pub(crate) window: WindowId,
    pub(crate) rect: Rectangle<i32, Logical>,
    pub(crate) radius: f64,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

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

const REVEAL: Duration = Duration::from_millis(ctdesign::DURATION_BASE);

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FocusReveal {
    shown: bool,
    from: f64,
    start: Option<Instant>,
    notified: Option<WindowId>,
    shifted: bool,
}

impl FocusReveal {
    pub(crate) fn is_shown(&self) -> bool {
        self.shown
    }

    pub(crate) fn progress(&self, now: Instant) -> f64 {
        let target = if self.shown { 1.0 } else { 0.0 };
        let Some(start) = self.start else {
            return target;
        };
        let t = now.saturating_duration_since(start).as_secs_f64() / REVEAL.as_secs_f64();
        lerp(
            self.from,
            target,
            cubic_bezier(ctdesign::SPACE_SLIDE_EASE, t),
        )
    }

    pub(crate) fn animating(&self, now: Instant) -> bool {
        self.start
            .is_some_and(|start| now.saturating_duration_since(start) < REVEAL)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RevealFrame {
    pub(crate) owner: WindowId,
    pub(crate) progress: f64,
    pub(crate) bar: i32,
    pub(crate) titlebar: bool,
    pub(crate) slides: bool,
}

impl State {
    pub(crate) fn focus_owner(&self) -> Option<WindowId> {
        if self.space_slide.is_some() || self.focus_transition.is_some() {
            return None;
        }
        match self.spaces.current().kind {
            SpaceKind::Focus { owner, .. } => Some(owner),
            SpaceKind::Desktop => None,
        }
    }

    pub(crate) fn desktop_bar_height(&self) -> i32 {
        self.layers
            .mapped()
            .iter()
            .find(|entry| entry.is_desktop_bar())
            .map(|entry| entry.geometry.size.h)
            .unwrap_or(ctdesign::MENUBAR_HEIGHT)
    }

    pub(crate) fn set_focus_reveal(&mut self, shown: bool) {
        if self.focus_reveal.shown == shown {
            return;
        }
        let Some(owner) = self.focus_owner() else {
            return;
        };
        let now = Instant::now();
        let from = self.focus_reveal.progress(now);
        let mut notified = self.focus_reveal.notified.take();
        let mut shifted = self.focus_reveal.shifted;
        if shown && !self.focus_titlebar(owner) {
            shifted = !self.focus_overlay(owner);
            if shifted {
                self.windows
                    .set_scene_y(owner, f64::from(self.desktop_bar_height()));
            }
            notified = Some(owner);
            self.send_focus_bar(owner, true);
        } else if shown {
            shifted = false;
        } else if let Some(window) = notified.take() {
            if shifted {
                self.windows.set_scene_y(window, 0.0);
            }
            self.send_focus_bar(window, false);
        }
        self.focus_reveal = FocusReveal {
            shown,
            from,
            start: Some(now),
            notified,
            shifted,
        };
        self.layers.set_bar_revealed(shown);
        tracing::info!(window = owner.get(), shown, "focus space bar");
        self.send_space_state();
        self.request_redraw();
    }

    pub(crate) fn update_focus_reveal(&mut self, location: Point<f64, Logical>) {
        if self.focus_owner().is_none() {
            return;
        }
        self.update_dock_reveal(location);
        if location.y <= 1.0 {
            self.set_focus_reveal(true);
            return;
        }
        if !self.focus_reveal.shown || self.windows.topmost_popup().is_some() {
            return;
        }
        let reach =
            f64::from(self.desktop_bar_height()) + 2.0 * f64::from(ctdesign::TITLEBAR.thickness);
        if location.y > reach {
            self.set_focus_reveal(false);
        }
    }

    pub(crate) fn reveal_frame(&self, now: Instant) -> Option<RevealFrame> {
        let owner = self.focus_owner()?;
        let progress = self.focus_reveal.progress(now);
        (progress > 0.0).then(|| RevealFrame {
            owner,
            progress,
            bar: self.desktop_bar_height(),
            titlebar: self.focus_titlebar(owner),
            slides: self.focus_reveal.shifted && !self.focus_titlebar(owner),
        })
    }

    pub(crate) fn revealed_titlebar(
        &self,
        id: WindowId,
    ) -> Option<(SceneRect<f64, SceneLogical>, crate::decoration::Insets)> {
        if self.focus_owner() != Some(id) || !self.focus_reveal.shown || !self.focus_titlebar(id) {
            return None;
        }
        let size = self.logical_output_size();
        let thickness = f64::from(ctdesign::TITLEBAR.thickness);
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

    // macOS: in a full-screen space the Dock comes up when the pointer reaches its edge, and goes
    // when the pointer leaves it with no menu open.
    fn update_dock_reveal(&mut self, location: Point<f64, Logical>) {
        let Some(dock) = self.layers.dock() else {
            return;
        };
        let revealed = at_dock_edge(dock.anchors, location, self.logical_output_size())
            || (self.layers.dock_revealed()
                && (dock.geometry.to_f64().contains(location)
                    || self.windows.topmost_popup().is_some()));
        if self.layers.set_dock_revealed(revealed) {
            self.request_redraw();
        }
    }

    fn reset_focus_reveal(&mut self) {
        if let Some(window) = self.focus_reveal.notified {
            if self.focus_reveal.shifted {
                self.windows.set_scene_y(window, 0.0);
            }
            self.send_focus_bar(window, false);
        }
        self.focus_reveal = FocusReveal::default();
        self.layers.set_bar_revealed(false);
        self.layers.set_dock_revealed(false);
        self.send_space_state();
    }

    fn focus_titlebar(&self, id: WindowId) -> bool {
        !self.windows.is_client_decorated(id) || self.windows.has_focus_titlebar(id)
    }

    fn keeps_tabs_in_focus(&self, id: WindowId) -> bool {
        let Some(surface) = self.windows.surface_of(id) else {
            return false;
        };
        let app_id = self.windows.app_id(surface);
        self.config.window.focus_titlebar_apps.contains(&app_id)
    }

    pub(crate) fn in_focus_mode(&self, id: WindowId) -> bool {
        self.spaces.focus_space_of(id).is_some()
    }

    pub(crate) fn toggle_focus_mode(&mut self, id: WindowId) {
        if self.in_focus_mode(id) {
            self.exit_focus_mode(id);
        } else {
            self.enter_focus_mode(id, false);
        }
    }

    pub(crate) fn enter_focus_mode(&mut self, id: WindowId, requested_fullscreen: bool) {
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
        self.show_space_of(id);
        self.space_slide = None;
        let Some(desk) = self
            .windows
            .iter_visible(&self.minimized)
            .find(|window| window.id() == id)
            .map(|window| crate::scene_elements::window_content_rect(&window))
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
        self.fullscreen.insert(id, outer);
        self.focus_desk.insert(id, desk);
        let full = self.output_scene_rect();
        let keeps_tabs = !requested_fullscreen && self.keeps_tabs_in_focus(id);
        self.windows.set_focus_titlebar(id, keeps_tabs);
        self.windows.set_fullscreen_with_outer(id, true, full);
        self.foreign_state_changed(id);
        tracing::info!(window = id.get(), index, "focus mode entered");
        self.switch_space(index);
        self.focus_transition = self.space_slide.map(|_| FocusTransition {
            window: id,
            desk,
            entering: true,
            start: Instant::now(),
            released: None,
        });
        self.raise_and_focus(id);
    }

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
        let desk = self.focus_desk.get(&id).copied();
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
                    released: None,
                });
            }
            _ => {
                self.release_focus_window(id);
                self.finish_focus_exit(id);
            }
        }
    }

    fn release_focus_window(&mut self, id: WindowId) {
        let saved = self
            .fullscreen
            .remove(&id)
            .or_else(|| self.windows.outer(id))
            .unwrap_or_else(|| self.output_scene_rect());
        self.windows.set_fullscreen_with_outer(id, false, saved);
        self.windows.set_focus_titlebar(id, false);
        self.foreign_state_changed(id);
    }

    fn finish_focus_exit(&mut self, id: WindowId) {
        self.focus_desk.remove(&id);
        if self.spaces.exit_focus(id).is_ok() {
            self.sync_space_visibility();
            self.raise_and_focus(id);
        }
    }

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
            return Some(FocusFrame {
                window: transition.window,
                rect: full,
                radius: 0.0,
            });
        }
        if !transition.entering
            && transition.released.is_none()
            && committed.is_some()
            && elapsed.as_secs_f64() >= SLIDE.as_secs_f64() * RELEASE_AT
        {
            self.release_focus_window(transition.window);
            self.focus_transition = Some(FocusTransition {
                released: Some(now),
                ..transition
            });
        }
        let transition = self.focus_transition.unwrap_or(transition);
        if !transition.entering && committed.is_some() && elapsed >= SLIDE {
            let released = match transition.released {
                Some(released) => released,
                None => {
                    self.release_focus_window(transition.window);
                    self.focus_transition = Some(FocusTransition {
                        released: Some(now),
                        ..transition
                    });
                    now
                }
            };
            let caught_up = committed.is_some_and(|committed| committed != size);
            if !caught_up && now.saturating_duration_since(released) < HANDOVER_WAIT {
                return Some(FocusFrame {
                    window: transition.window,
                    rect: transition.desk,
                    radius: self.window_corner(transition.window),
                });
            }
            tracing::info!(
                window = transition.window.get(),
                ?committed,
                late_ms = now.saturating_duration_since(released).as_millis() as u64,
                "focus exit handed over"
            );
            self.focus_transition = None;
            self.finish_focus_exit(transition.window);
            return None;
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
                self.release_focus_window(transition.window);
                self.finish_focus_exit(transition.window);
            }
            return None;
        }
        let eased = cubic_bezier(
            ctdesign::SPACE_SLIDE_EASE,
            elapsed.as_secs_f64() / SLIDE.as_secs_f64(),
        );
        let corner = self.window_corner(transition.window);
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

    fn output_scene_rect(&self) -> SceneRect<f64, SceneLogical> {
        let size = self.logical_output_size();
        SceneRect::new(
            ScenePoint::new(0.0, 0.0),
            SceneSize::new(f64::from(size.w), f64::from(size.h)),
        )
    }

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
        let eased = cubic_bezier(ctdesign::SPACE_SLIDE_EASE, progress);
        slide.from + (to as f64 - slide.from) * eased
    }

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

    pub(crate) fn space_window_mapped(&mut self, id: WindowId) {
        let parent = self.windows.dialog_parent(id);
        let assigned = self.spaces.assign(id, parent);
        if assigned.needs_switch
            && let Some(index) = self.spaces.index_of(assigned.space)
        {
            self.switch_space(index);
        }
    }

    pub(crate) fn space_window_removed(&mut self, id: WindowId) {
        if self
            .focus_transition
            .is_some_and(|transition| transition.window == id)
        {
            self.focus_transition = None;
        }
        self.fullscreen.remove(&id);
        self.focus_desk.remove(&id);
        let before = self.spaces.current().id;
        self.spaces.remove_window(id);
        if self.spaces.current().id != before {
            self.sync_space_visibility();
        } else {
            self.send_space_state();
        }
    }

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

    pub(crate) fn next_space(&mut self) -> bool {
        if self.focus_transition.is_some() {
            return false;
        }
        self.switch_space(self.spaces.current_index() + 1)
    }

    pub(crate) fn previous_space(&mut self) -> bool {
        if self.focus_transition.is_some() {
            return false;
        }
        match self.spaces.current_index() {
            0 => false,
            index => self.switch_space(index - 1),
        }
    }

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

    fn sync_space_visibility(&mut self) {
        self.reset_focus_reveal();
        self.dismiss_all_popups();
        let ids: Vec<WindowId> = self.windows.iter().map(|window| window.id()).collect();
        for id in ids {
            self.windows.set_off_space(id, !self.spaces.is_shown(id));
        }
        let current = self.spaces.current();
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
            match self.focus_fallback(None) {
                Some(next) => self.focus_window(next),
                None => self.clear_keyboard_focus(),
            }
        }
        self.send_space_state();
        self.request_redraw();
    }
}

fn at_dock_edge(
    anchors: Anchor,
    location: Point<f64, Logical>,
    output: Size<i32, Logical>,
) -> bool {
    if !anchors.contains(Anchor::TOP) {
        location.y >= f64::from(output.h) - 1.0
    } else if !anchors.contains(Anchor::RIGHT) {
        location.x <= 1.0
    } else {
        location.x >= f64::from(output.w) - 1.0
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
        let curve = ctdesign::SPACE_SLIDE_EASE;
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
        let curve = ctdesign::SPACE_SLIDE_EASE;
        assert!(cubic_bezier(curve, 0.25) > 0.5);
        let mut previous = 0.0;
        for step in 1..=20 {
            let y = cubic_bezier(curve, f64::from(step) / 20.0);
            assert!(y >= previous);
            previous = y;
        }
    }

    #[test]
    fn the_dock_comes_up_at_the_edge_it_is_anchored_to() {
        use super::at_dock_edge;
        use smithay::utils::{Point, Size};
        use smithay::wayland::shell::wlr_layer::Anchor;

        let output = Size::from((1920, 1080));
        let bottom = Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT;
        let left = Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM;
        let right = Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM;
        assert!(at_dock_edge(bottom, Point::from((900.0, 1079.5)), output));
        assert!(!at_dock_edge(bottom, Point::from((900.0, 1070.0)), output));
        assert!(at_dock_edge(left, Point::from((0.0, 500.0)), output));
        assert!(!at_dock_edge(left, Point::from((1919.5, 500.0)), output));
        assert!(at_dock_edge(right, Point::from((1919.5, 500.0)), output));
    }
}
