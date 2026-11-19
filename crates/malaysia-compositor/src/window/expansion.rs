use std::time::{Duration, Instant};

use malaysia_scene::geometry::{
    Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
};
use malaysia_scene::scene::WindowId;
use smithay::utils::{Logical, Point};

use super::{Window, WindowData, Windows};

const SHORTEST: Duration = Duration::from_millis(ctdesign::WINDOW_EXPAND_MIN_MS);
const LONGEST: Duration = Duration::from_millis(ctdesign::WINDOW_EXPAND_MAX_MS);

#[derive(Clone, Copy)]
pub(super) struct Expansion {
    maximized: bool,
    from: SceneRect<f64, SceneLogical>,
    to: SceneRect<f64, SceneLogical>,
    start: Instant,
    duration: Duration,
    arrived: bool,
}

impl Expansion {
    fn linear_at(&self, now: Instant) -> f64 {
        (now.saturating_duration_since(self.start).as_secs_f64() / self.duration.as_secs_f64())
            .clamp(0.0, 1.0)
    }

    fn running(self) -> Option<Self> {
        Some(self).filter(|expansion| !expansion.arrived)
    }
}

impl Window<'_> {
    pub(crate) fn is_expanding(&self) -> bool {
        self.data.expansion.is_some()
    }
}

impl WindowData {
    pub(super) fn settle_expansion(&mut self) {
        if self.expansion.is_some_and(|expansion| expansion.arrived) {
            self.expansion = None;
        }
    }
}

impl Windows {
    pub(crate) fn expand(
        &mut self,
        id: WindowId,
        maximized: bool,
        to: SceneRect<f64, SceneLogical>,
        output: SceneSize<f64, SceneLogical>,
        now: Instant,
    ) {
        let (Some(index), Some(from)) = (self.index(id), self.outer(id)) else {
            return;
        };
        self.windows[index].expansion = (from != to).then(|| Expansion {
            maximized,
            from,
            to,
            start: now,
            duration: malaysia_scene::expansion::duration(from, to, output, SHORTEST, LONGEST),
            arrived: false,
        });
        self.set_maximized_with_outer(id, maximized, from);
    }

    pub(crate) fn advance_expansions(&mut self, now: Instant) {
        for index in 0..self.windows.len() {
            if let Some(expansion) = self.windows[index].expansion {
                self.step_expansion(index, expansion.linear_at(now));
            }
        }
    }

    pub(super) fn retarget_expansion(
        &mut self,
        id: WindowId,
        location: Point<i32, Logical>,
    ) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        let Some(expansion) = self.windows[index].expansion.and_then(Expansion::running) else {
            return false;
        };
        let to = SceneRect::new(
            ScenePoint::new(f64::from(location.x), f64::from(location.y)),
            expansion.to.size(),
        );
        self.windows[index].expansion = Some(Expansion { to, ..expansion });
        true
    }

    pub(crate) fn finish_expansion(&mut self, id: WindowId) {
        if let Some(index) = self.index(id) {
            self.step_expansion(index, 1.0);
        }
    }

    pub(crate) fn settled_outer(&self, id: WindowId) -> Option<SceneRect<f64, SceneLogical>> {
        let running = self.windows[self.index(id)?].expansion;
        running
            .and_then(Expansion::running)
            .map(|expansion| expansion.to)
            .or_else(|| self.outer(id))
    }

    pub(crate) fn is_expanding(&self) -> bool {
        self.windows
            .iter()
            .any(|data| data.expansion.and_then(Expansion::running).is_some())
    }

    fn step_expansion(&mut self, index: usize, linear: f64) {
        let Some(expansion) = self.windows[index].expansion.and_then(Expansion::running) else {
            return;
        };
        let outer = malaysia_scene::expansion::rect_at(
            expansion.from,
            expansion.to,
            f64::from(ctdesign::ease_in_out(linear as f32)),
        );
        let id = self.windows[index].id;
        self.set_maximized_with_outer(id, expansion.maximized, outer);
        if linear >= 1.0 {
            self.windows[index].expansion = Some(Expansion {
                arrived: true,
                ..expansion
            });
        }
    }
}
