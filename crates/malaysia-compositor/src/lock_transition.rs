use std::time::{Duration, Instant};

use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Point;
use smithay::wayland::session_lock::SessionLocker;
use smithay::wayland::shell::wlr_layer::Layer;

use crate::cursor::OutputRenderElement;
use crate::lock::LOCK_START_GRACE;
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::{append_solid, background_of};
use crate::session::physical_point;
use crate::state::State;
use crate::theme_fade::{ease, progress};

const UNLOCK_FADE: Duration = Duration::from_millis(ctdesign::DURATION_BASE);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kept {
    Wallpaper,
    Menubar,
}

// ADR 0074: the desktop fades under the wallpaper; `locked` waits for the lock screen's first frame.
#[derive(Default)]
pub(crate) struct LockTransition {
    locking: Option<Locking>,
    shown: bool,
    unlocking: Option<Instant>,
}

struct Locking {
    since: Instant,
    fade: Duration,
    settle: Duration,
    confirmation: Option<SessionLocker>,
    drawn: bool,
}

impl LockTransition {
    pub(crate) fn shown(&self) -> bool {
        self.shown
    }

    pub(crate) fn animating(&self, now: Instant) -> bool {
        self.locking.is_some()
            || self
                .unlocking
                .is_some_and(|since| progress(since, now, UNLOCK_FADE) < 1.0)
    }

    fn cover(&self, now: Instant) -> Option<f32> {
        if let Some(locking) = &self.locking
            && !self.shown
        {
            return Some(if locking.fade.is_zero() {
                1.0
            } else {
                ease(progress(locking.since, now, locking.fade))
            });
        }
        self.unlocking
            .map(|since| 1.0 - ease(progress(since, now, UNLOCK_FADE)))
    }
}

pub(crate) fn mark(surface: &WlSurface, kept: Kept) {
    smithay::wayland::compositor::with_states(surface, |states| {
        states.data_map.insert_if_missing_threadsafe(|| kept);
    });
}

pub(crate) fn kept(surface: &WlSurface) -> Option<Kept> {
    smithay::wayland::compositor::with_states(surface, |states| {
        states.data_map.get::<Kept>().copied()
    })
}

pub(crate) fn is_menubar(entry: &crate::layers::LayerEntry) -> bool {
    kept(entry.surface.wl_surface()) == Some(Kept::Menubar)
}

impl State {
    pub(crate) fn start_locking(&mut self, confirmation: SessionLocker, relock: bool) {
        let (fade, settle) = if relock || self.idle.locking_for_sleep() {
            (Duration::ZERO, Duration::ZERO)
        } else if self.config.appearance.reduced_motion {
            (
                Duration::from_millis(ctdesign::DURATION_BASE),
                Duration::ZERO,
            )
        } else {
            (
                Duration::from_millis(ctdesign::LOCK_FADE_MS),
                Duration::from_millis(ctdesign::LOCK_SETTLE_MS),
            )
        };
        self.lock_transition.unlocking = None;
        self.lock_transition.locking = Some(Locking {
            since: Instant::now(),
            fade,
            settle,
            confirmation: Some(confirmation),
            drawn: false,
        });
        if !relock {
            self.lock_transition.shown = false;
            self.notify_locking(fade);
        }
    }

    pub(crate) fn start_unlocking(&mut self) {
        self.lock_transition = LockTransition {
            unlocking: Some(Instant::now()),
            ..LockTransition::default()
        };
        self.notify_unlocked();
    }

    pub(crate) fn update_lock_transition(&mut self, now: Instant) {
        if self
            .lock_transition
            .unlocking
            .is_some_and(|since| progress(since, now, UNLOCK_FADE) >= 1.0)
        {
            self.lock_transition.unlocking = None;
        }
        let Some(locking) = &self.lock_transition.locking else {
            return;
        };
        if self.lock_transition.shown {
            return;
        }
        let elapsed = now.saturating_duration_since(locking.since);
        let settled = elapsed >= locking.fade + locking.settle && self.lock_surfaces_drawn();
        if settled || elapsed >= LOCK_START_GRACE {
            self.lock_transition.shown = true;
        }
    }

    pub(crate) fn lock_frame_built(&mut self) {
        if let Some(locking) = self.lock_transition.locking.as_mut()
            && self.lock_transition.shown
        {
            locking.drawn = true;
        }
    }

    pub(crate) fn lock_frame_presented(&mut self) {
        if !self
            .lock_transition
            .locking
            .as_ref()
            .is_some_and(|locking| locking.drawn)
        {
            return;
        }
        if let Some(confirmation) = self
            .lock_transition
            .locking
            .take()
            .and_then(|mut locking| locking.confirmation.take())
        {
            tracing::info!("session lock confirmed after the locked frame");
            confirmation.lock();
        }
        self.idle.confirm_locked();
    }

    fn lock_surfaces_drawn(&self) -> bool {
        !self.lock_surfaces.is_empty()
            && self.lock_surfaces.iter().all(|surface| {
                with_renderer_surface_state(surface.wl_surface(), |state| state.buffer().is_some())
                    .unwrap_or(false)
            })
    }

    pub(crate) fn lock_cover(&self, now: Instant) -> Option<f32> {
        self.lock_transition.cover(now)
    }
}

pub(crate) fn append_wallpaper<R: MalaysiaRenderer>(
    state: &State,
    renderer: &mut R,
    alpha: f32,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let scale = state.scale.fractional_scale();
    let desktop = state.spaces.desktop_number(state.spaces.current().id);
    let shows = background_of(state, desktop);
    let surfaces: Vec<_> = state
        .layers
        .on_layer(Layer::Background)
        .filter(|entry| shows(entry) && kept(entry.surface.wl_surface()) == Some(Kept::Wallpaper))
        .map(|entry| (entry.surface.wl_surface().clone(), entry.geometry.loc))
        .collect();
    if surfaces.is_empty() {
        let [r, g, b, a] = ctdesign::DESKTOP_CLEAR.premultiplied();
        append_solid(
            Point::default(),
            state.logical_output_size(),
            [r * alpha, g * alpha, b * alpha, a * alpha],
            scale,
            elements,
        );
        return;
    }
    for (surface, location) in surfaces {
        elements.extend(
            render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
                renderer,
                &surface,
                physical_point(location, scale),
                scale,
                alpha,
                Kind::Unspecified,
            )
            .into_iter()
            .map(Into::into),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locking(fade: Duration, since: Instant) -> LockTransition {
        LockTransition {
            locking: Some(Locking {
                since,
                fade,
                settle: Duration::ZERO,
                confirmation: None,
                drawn: false,
            }),
            ..LockTransition::default()
        }
    }

    #[test]
    fn a_lock_without_transition_covers_the_desktop_at_once() {
        let now = Instant::now();
        assert_eq!(locking(Duration::ZERO, now).cover(now), Some(1.0));
    }

    #[test]
    fn the_cover_grows_over_the_fade_and_stops_once_the_lock_screen_shows() {
        let now = Instant::now();
        let fade = Duration::from_millis(1000);
        let mut transition = locking(fade, now);
        assert_eq!(transition.cover(now), Some(0.0));
        assert_eq!(transition.cover(now + fade), Some(1.0));
        transition.shown = true;
        assert_eq!(transition.cover(now + fade), None);
    }

    #[test]
    fn unlocking_uncovers_the_desktop_and_then_stops() {
        let now = Instant::now();
        let transition = LockTransition {
            unlocking: Some(now),
            ..LockTransition::default()
        };
        assert_eq!(transition.cover(now), Some(1.0));
        assert_eq!(transition.cover(now + UNLOCK_FADE), Some(0.0));
        assert!(!transition.animating(now + UNLOCK_FADE));
    }
}
