//! GPU crossfade between the old and new appearance theme.
//!
//! When `appearance.domain` or `wallpaper.state` changes, the compositor captures
//! the current output into a `GlesTexture` and draws it as a full-screen overlay
//! that fades out over 420 ms. The cursor is rendered on top and keeps moving.
//!
//! See ADR 0044.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::{Bind, Offscreen};
use smithay::output::Output;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::utils::{Buffer as BufferCoord, Physical, Point, Scale, Size, Transform};
use tracing::warn;

use crate::cursor::{Cursors, OutputRenderElement};
use crate::state::State;

/// Maximum time to wait for mapped surfaces to commit before forcing the fade.
pub(crate) const WAIT_MAX: Duration = Duration::from_millis(vow_design::THEME_FADE_WAIT_MS);

/// Fade duration from fully opaque snapshot to fully transparent.
pub(crate) const DURATION: Duration = Duration::from_millis(vow_design::DURATION_SLOW_MS);

/// What happened to the fade on a frame tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FadeTick {
    /// Still running; the frame should be submitted and another requested.
    Active,
    /// Transitioned from `Waiting` to `Fading` this tick.
    Started,
    /// The fade finished; the caller should drop it.
    Done,
}

/// A per-output theme crossfade.
#[derive(Debug)]
pub(crate) struct ThemeFade {
    /// The output this fade belongs to (kept for future multi-output support).
    #[allow(dead_code)]
    output: Output,
    /// The physical size of the snapshot (kept for future multi-output support).
    #[allow(dead_code)]
    output_size: Size<i32, Physical>,
    texture_buffer: TextureBuffer<GlesTexture>,
    phase: Phase,
}

#[derive(Debug)]
enum Phase {
    /// Drawing the snapshot opaquely, waiting for every mapped surface to
    /// commit with the new theme or for the timeout.
    Waiting {
        since: Instant,
        pending_surfaces: HashSet<ObjectId>,
    },
    /// Fading the snapshot out.
    Fading { started_at: Instant },
}

impl ThemeFade {
    /// Creates a new fade from an already-rendered snapshot texture.
    pub(crate) fn new(
        renderer: &GlesRenderer,
        output: Output,
        output_size: Size<i32, Physical>,
        texture: GlesTexture,
        pending_surfaces: HashSet<ObjectId>,
        now: Instant,
    ) -> Self {
        let texture_buffer =
            TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None);
        Self {
            output,
            output_size,
            texture_buffer,
            phase: Phase::Waiting {
                since: now,
                pending_surfaces,
            },
        }
    }

    /// The output this fade belongs to (kept for future multi-output support).
    #[allow(dead_code)]
    pub(crate) fn output(&self) -> &Output {
        &self.output
    }

    /// The physical size of the snapshot texture (kept for future multi-output
    /// support).
    #[allow(dead_code)]
    pub(crate) fn output_size(&self) -> Size<i32, Physical> {
        self.output_size
    }

    /// Current opacity of the snapshot: `1.0` during the wait and while fading
    /// starts, decreasing to `0.0` at the end of the fade.
    pub(crate) fn alpha(&self, now: Instant) -> f32 {
        match &self.phase {
            Phase::Waiting { .. } => 1.0,
            Phase::Fading { started_at } => {
                let t = progress(*started_at, now, DURATION);
                1.0 - ease(t)
            }
        }
    }

    /// Advances the state machine and returns what happened this tick.
    pub(crate) fn tick(&mut self, now: Instant) -> FadeTick {
        match &mut self.phase {
            Phase::Waiting {
                since,
                pending_surfaces,
            } => {
                if pending_surfaces.is_empty() || now.duration_since(*since) >= WAIT_MAX {
                    self.phase = Phase::Fading { started_at: now };
                    FadeTick::Started
                } else {
                    FadeTick::Active
                }
            }
            Phase::Fading { started_at } => {
                let t = progress(*started_at, now, DURATION);
                if t >= 1.0 {
                    FadeTick::Done
                } else {
                    FadeTick::Active
                }
            }
        }
    }

    /// Records that a surface committed a new buffer, removing it from the
    /// wait list.
    pub(crate) fn mark_committed(&mut self, surface_id: ObjectId) {
        if let Phase::Waiting {
            pending_surfaces, ..
        } = &mut self.phase
        {
            pending_surfaces.remove(&surface_id);
        }
    }

    /// Returns a render element covering the whole output with the given alpha.
    pub(crate) fn element(&self, alpha: f32) -> OutputRenderElement {
        TextureRenderElement::from_texture_buffer(
            Point::from((0.0, 0.0)),
            &self.texture_buffer,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        )
        .into()
    }
}

/// Normalised linear progress `t` in `[0, 1]`.
fn progress(start: Instant, now: Instant, duration: Duration) -> f32 {
    if now <= start {
        return 0.0;
    }
    let elapsed = now.duration_since(start).as_secs_f32();
    let total = duration.as_secs_f32();
    (elapsed / total).clamp(0.0, 1.0)
}

/// Cubic ease-in-out: smooth acceleration and deceleration.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let f = -2.0 * t + 2.0;
        1.0 - f * f * f / 2.0
    }
}

/// Collects the `ObjectId`s of every mapped surface that the fade should wait
/// for: visible toplevels and mapped layer surfaces.
pub(crate) fn pending_surfaces(state: &State) -> HashSet<ObjectId> {
    let mut pending = HashSet::new();
    let visible: Vec<_> = state
        .windows
        .iter_visible(&state.minimized)
        .map(|window| window.id())
        .collect();
    for id in visible {
        if let Some(surface) = state.windows.surface_of(id) {
            pending.insert(surface.id());
        }
    }
    for entry in state.layers.mapped() {
        pending.insert(entry.surface.wl_surface().id());
    }
    pending
}

/// Renders the current output (without the cursor) into a fresh `GlesTexture`.
///
/// The texture is in physical pixels and uses `transform` so it can be drawn
/// 1:1 as a full-screen element in the next frame.
pub(crate) fn take_snapshot(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    output_size: Size<i32, Physical>,
    scale: f64,
    transform: Transform,
) -> Option<GlesTexture> {
    let start = Instant::now();
    let elements = crate::session::build_output_elements(state, renderer, cursor, false);
    let buffer_size = Size::<i32, BufferCoord>::from((output_size.w, output_size.h));
    let mut texture =
        Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, buffer_size).ok()?;
    {
        let mut framebuffer = renderer.bind(&mut texture).ok()?;
        let mut tracker = OutputDamageTracker::new(output_size, Scale::from(scale), transform);
        let rendered = tracker.render_output(
            renderer,
            &mut framebuffer,
            0,
            &elements,
            [0.06, 0.06, 0.09, 1.0],
        );
        let elapsed = start.elapsed();
        if rendered.is_err() {
            return None;
        }
        tracing::info!(
            output = ?state.output.name(),
            size = ?output_size,
            ?elapsed,
            "theme fade snapshot rendered"
        );
    }
    Some(texture)
}

/// Starts a fade if motion is not reduced and the session is not locked.
///
/// On failure to create the snapshot, logs once and returns `None` so the theme
/// still changes immediately.
pub(crate) fn start_if_allowed(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    output_size: Size<i32, Physical>,
    scale: f64,
    transform: Transform,
    now: Instant,
) -> Option<ThemeFade> {
    if state.config.appearance.reduced_motion || state.locked {
        return None;
    }
    let pending = pending_surfaces(state);
    let texture = take_snapshot(state, renderer, cursor, output_size, scale, transform)?;
    Some(ThemeFade::new(
        renderer,
        state.output.clone(),
        output_size,
        texture,
        pending,
        now,
    ))
}

/// Logs a snapshot failure once and tells the caller to fall back to an
/// immediate theme change.
pub(crate) fn log_snapshot_failure(error: &str) {
    warn!(%error, "theme fade snapshot failed; changing theme immediately");
}

#[cfg(test)]
mod tests {
    use super::{DURATION, ThemeFade, WAIT_MAX, ease};
    use std::collections::HashSet;
    use std::time::Instant;

    use smithay::backend::allocator::Fourcc;
    use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
    use smithay::backend::renderer::Offscreen;
    use smithay::backend::renderer::element::Element;
    use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
    use smithay::output::{Output, PhysicalProperties, Subpixel};
    use smithay::reexports::wayland_server::backend::ObjectId;
    use smithay::utils::{Buffer as BufferCoord, Scale, Size};

    /// A GLES renderer on the first EGL device, with no window.
    ///
    /// EGL context creation is unsafe by API. This is the only place this crate
    /// allows it, compiled only for tests, and uses a render node rather than
    /// the host session.
    #[allow(unsafe_code)]
    fn test_renderer() -> GlesRenderer {
        let device = EGLDevice::enumerate()
            .expect("EGL device enumeration should work")
            .next()
            .expect("the theme fade tests need an EGL device");
        let display = unsafe { EGLDisplay::new(device) }.expect("the tests need an EGL display");
        let context = EGLContext::new(&display).expect("the tests need an EGL context");
        unsafe { GlesRenderer::new(context) }.expect("the tests need a GLES renderer")
    }

    fn test_output() -> Output {
        Output::new(
            "test".to_string(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: String::new(),
                model: String::new(),
                serial_number: String::new(),
            },
        )
    }

    fn test_texture(renderer: &mut GlesRenderer) -> GlesTexture {
        let size = Size::<i32, BufferCoord>::from((64, 64));
        Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size)
            .expect("the tests need a 64x64 offscreen texture")
    }

    fn test_fade(
        renderer: &mut GlesRenderer,
        pending: HashSet<ObjectId>,
        now: Instant,
    ) -> ThemeFade {
        let texture = test_texture(renderer);
        ThemeFade::new(
            renderer,
            test_output(),
            Size::from((64, 64)),
            texture,
            pending,
            now,
        )
    }

    #[test]
    fn ease_curve_hits_key_points_and_is_monotonic() {
        assert!((ease(0.0)).abs() < 1e-6);
        assert!((ease(0.5) - 0.5).abs() < 1e-6);
        assert!((ease(1.0) - 1.0).abs() < 1e-6);

        let mut previous = -1.0f32;
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let value = ease(t);
            assert!(
                value >= previous,
                "ease must be monotonic at t={t}: {value} < {previous}"
            );
            previous = value;
        }
    }

    #[test]
    fn empty_pending_starts_fading_immediately() {
        let start = Instant::now();
        let mut renderer = test_renderer();
        let mut fade = test_fade(&mut renderer, HashSet::new(), start);
        assert_eq!(fade.tick(start), super::FadeTick::Started);
    }

    #[test]
    fn pending_surfaces_start_after_wait_max() {
        let start = Instant::now();
        let mut renderer = test_renderer();
        let mut pending = HashSet::new();
        pending.insert(ObjectId::null());
        let mut fade = test_fade(&mut renderer, pending, start);
        assert_eq!(fade.tick(start), super::FadeTick::Active);
        assert_eq!(
            fade.tick(start + WAIT_MAX),
            super::FadeTick::Started,
            "should start right at WAIT_MAX"
        );
    }

    #[test]
    fn committing_a_pending_surface_starts_fading() {
        let start = Instant::now();
        let mut renderer = test_renderer();
        let mut pending = HashSet::new();
        let surface = ObjectId::null();
        pending.insert(surface.clone());
        let mut fade = test_fade(&mut renderer, pending, start);
        assert_eq!(fade.tick(start), super::FadeTick::Active);
        fade.mark_committed(surface);
        assert_eq!(
            fade.tick(start),
            super::FadeTick::Started,
            "should start once the pending surface commits"
        );
    }

    #[test]
    fn fade_finishes_after_duration() {
        let start = Instant::now();
        let mut renderer = test_renderer();
        let mut fade = test_fade(&mut renderer, HashSet::new(), start);
        assert_eq!(fade.tick(start), super::FadeTick::Started);
        assert_eq!(
            fade.tick(start + DURATION),
            super::FadeTick::Done,
            "should finish right at DURATION"
        );
    }

    #[test]
    fn alpha_is_one_while_waiting_and_decreases_while_fading() {
        let start = Instant::now();
        let mut renderer = test_renderer();
        let fade = test_fade(&mut renderer, HashSet::new(), start);
        assert!((fade.alpha(start) - 1.0).abs() < 1e-6);

        let mut fade = test_fade(&mut renderer, HashSet::new(), start);
        fade.tick(start);
        let alpha_start = fade.alpha(start);
        assert!((alpha_start - 1.0).abs() < 1e-6);

        let mid = start + DURATION / 2;
        let alpha_mid = fade.alpha(mid);
        assert!(
            alpha_mid > 0.0 && alpha_mid < 1.0,
            "alpha mid-fade should be between 0 and 1, got {alpha_mid}"
        );

        let end = start + DURATION;
        let alpha_end = fade.alpha(end);
        assert!((alpha_end).abs() < 1e-6, "alpha should reach 0 at end");
    }

    #[test]
    fn element_covers_the_output_size() {
        let start = Instant::now();
        let mut renderer = test_renderer();
        let fade = test_fade(&mut renderer, HashSet::new(), start);
        let element = fade.element(0.5);
        let geometry = element.geometry(Scale::from(1.0));
        assert_eq!(geometry.size, Size::from((64, 64)));
        assert_eq!(geometry.loc, smithay::utils::Point::from((0, 0)));
    }
}
