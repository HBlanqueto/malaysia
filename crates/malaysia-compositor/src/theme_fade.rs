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

/// Debug hold before the snapshot starts fading.
///
/// shortcut: MALAYSIA_THEME_FADE_HOLD_MS is only for looking at the screen /
/// taking a photo of the snapshot before it fades. Remove once the fade is
/// verified on real hardware.
fn hold_duration() -> Duration {
    std::env::var("MALAYSIA_THEME_FADE_HOLD_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::ZERO)
}

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

/// Pure state machine of a theme crossfade: phase, timers and pending
/// surfaces. It has no GPU state and can be unit-tested without EGL.
#[derive(Debug)]
pub(crate) struct FadeState {
    phase: Phase,
}

#[derive(Debug)]
enum Phase {
    /// Drawing the snapshot opaquely, waiting for every mapped surface to
    /// commit with the new theme or for the timeout.
    Waiting {
        since: Instant,
        pending_surfaces: HashSet<ObjectId>,
        initial_pending: usize,
    },
    /// Fading the snapshot out.
    Fading {
        /// Debug hold end; until this instant the snapshot stays opaque.
        hold_until: Instant,
    },
}

impl FadeState {
    pub(crate) fn new(pending_surfaces: HashSet<ObjectId>, now: Instant) -> Self {
        Self {
            phase: Phase::Waiting {
                since: now,
                initial_pending: pending_surfaces.len(),
                pending_surfaces,
            },
        }
    }

    /// Current opacity of the snapshot: `1.0` during the wait and the optional
    /// debug hold, decreasing to `0.0` at the end of the fade.
    pub(crate) fn alpha(&self, now: Instant) -> f32 {
        match &self.phase {
            Phase::Waiting { .. } => 1.0,
            Phase::Fading { hold_until, .. } => {
                if now < *hold_until {
                    return 1.0;
                }
                let t = progress(*hold_until, now, DURATION);
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
                initial_pending,
            } => {
                let pending_before = pending_surfaces.len();
                if pending_surfaces.is_empty() || now.duration_since(*since) >= WAIT_MAX {
                    let reason = if pending_surfaces.is_empty() {
                        "commits"
                    } else {
                        "wait_max"
                    };
                    tracing::info!(
                        reason,
                        pending_before,
                        initial_pending = *initial_pending,
                        "theme fade started"
                    );
                    let hold = hold_duration();
                    if hold > Duration::ZERO {
                        tracing::info!(?hold, "theme fade hold");
                    }
                    self.phase = Phase::Fading {
                        hold_until: now + hold,
                    };
                    FadeTick::Started
                } else {
                    FadeTick::Active
                }
            }
            Phase::Fading { hold_until, .. } => {
                if now < *hold_until {
                    return FadeTick::Active;
                }
                let t = progress(*hold_until, now, DURATION);
                if t >= 1.0 {
                    let elapsed = now.duration_since(*hold_until);
                    tracing::info!(?elapsed, "theme fade finished");
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
}

/// A per-output theme crossfade.
#[derive(Debug)]
pub(crate) struct ThemeFade {
    state: FadeState,
    texture_buffer: TextureBuffer<GlesTexture>,
}

impl ThemeFade {
    /// Creates a new fade from an already-rendered snapshot texture.
    ///
    /// The texture is rendered upright by `take_snapshot`, so the overlay uses
    /// `Transform::Normal` on both backends.
    pub(crate) fn new(
        renderer: &GlesRenderer,
        texture: GlesTexture,
        pending_surfaces: HashSet<ObjectId>,
        now: Instant,
    ) -> Self {
        let texture_buffer =
            TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None);
        Self {
            state: FadeState::new(pending_surfaces, now),
            texture_buffer,
        }
    }

    /// Current opacity of the snapshot.
    pub(crate) fn alpha(&self, now: Instant) -> f32 {
        self.state.alpha(now)
    }

    /// Advances the state machine.
    pub(crate) fn tick(&mut self, now: Instant) -> FadeTick {
        self.state.tick(now)
    }

    /// Records that a surface committed a new buffer.
    pub(crate) fn mark_committed(&mut self, surface_id: ObjectId) {
        self.state.mark_committed(surface_id);
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
/// The texture is in physical pixels and rendered with `Transform::Normal` so
/// it can be sampled 1:1 with a `Transform::Normal` TextureBuffer.
pub(crate) fn take_snapshot(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    output_size: Size<i32, Physical>,
    scale: f64,
) -> Option<GlesTexture> {
    let start = Instant::now();
    let elements = crate::session::build_output_elements(state, renderer, cursor, false);
    let buffer_size = Size::<i32, BufferCoord>::from((output_size.w, output_size.h));
    let mut texture =
        Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, buffer_size).ok()?;
    {
        let mut framebuffer = renderer.bind(&mut texture).ok()?;
        let mut tracker =
            OutputDamageTracker::new(output_size, Scale::from(scale), Transform::Normal);
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
    now: Instant,
) -> Option<ThemeFade> {
    if state.config.appearance.reduced_motion || state.locked {
        return None;
    }
    let pending = pending_surfaces(state);
    let texture = take_snapshot(state, renderer, cursor, output_size, scale)?;
    Some(ThemeFade::new(renderer, texture, pending, now))
}

/// Logs a snapshot failure once and tells the caller to fall back to an
/// immediate theme change.
pub(crate) fn log_snapshot_failure(error: &str) {
    warn!(%error, "theme fade snapshot failed; changing theme immediately");
}

#[cfg(test)]
mod tests {
    use super::{DURATION, FadeState, ThemeFade, WAIT_MAX, ease};
    use std::collections::HashSet;
    use std::time::Instant;

    use smithay::backend::allocator::Fourcc;
    use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
    use smithay::backend::renderer::damage::OutputDamageTracker;
    use smithay::backend::renderer::{
        Bind, ExportMem, Offscreen,
        element::solid::{SolidColorBuffer, SolidColorRenderElement},
        gles::{GlesRenderer, GlesTexture},
    };
    use smithay::reexports::wayland_server::backend::ObjectId;
    use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Scale, Size, Transform};

    fn test_state(pending: HashSet<ObjectId>, now: Instant) -> FadeState {
        FadeState::new(pending, now)
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
        let mut state = test_state(HashSet::new(), start);
        assert_eq!(state.tick(start), super::FadeTick::Started);
    }

    #[test]
    fn pending_surfaces_start_after_wait_max() {
        let start = Instant::now();
        let mut pending = HashSet::new();
        pending.insert(ObjectId::null());
        let mut state = test_state(pending, start);
        assert_eq!(state.tick(start), super::FadeTick::Active);
        assert_eq!(
            state.tick(start + WAIT_MAX),
            super::FadeTick::Started,
            "should start right at WAIT_MAX"
        );
    }

    #[test]
    fn committing_a_pending_surface_starts_fading() {
        let start = Instant::now();
        let mut pending = HashSet::new();
        let surface = ObjectId::null();
        pending.insert(surface.clone());
        let mut state = test_state(pending, start);
        assert_eq!(state.tick(start), super::FadeTick::Active);
        state.mark_committed(surface);
        assert_eq!(
            state.tick(start),
            super::FadeTick::Started,
            "should start once the pending surface commits"
        );
    }

    #[test]
    fn fade_finishes_after_duration() {
        let start = Instant::now();
        let mut state = test_state(HashSet::new(), start);
        assert_eq!(state.tick(start), super::FadeTick::Started);
        assert_eq!(
            state.tick(start + DURATION),
            super::FadeTick::Done,
            "should finish right at DURATION"
        );
    }

    #[test]
    fn alpha_is_one_while_waiting_and_decreases_while_fading() {
        let start = Instant::now();
        let state = test_state(HashSet::new(), start);
        assert!((state.alpha(start) - 1.0).abs() < 1e-6);

        let mut state = test_state(HashSet::new(), start);
        state.tick(start);
        let alpha_start = state.alpha(start);
        assert!((alpha_start - 1.0).abs() < 1e-6);

        let mid = start + DURATION / 2;
        let alpha_mid = state.alpha(mid);
        assert!(
            alpha_mid > 0.0 && alpha_mid < 1.0,
            "alpha mid-fade should be between 0 and 1, got {alpha_mid}"
        );

        let end = start + DURATION;
        let alpha_end = state.alpha(end);
        assert!((alpha_end).abs() < 1e-6, "alpha should reach 0 at end");
    }

    /// A GLES renderer on the first EGL device, with no window.
    ///
    /// EGL context creation is unsafe by API. This is the only place this crate
    /// allows it, compiled only for tests, and uses a render node rather than
    /// the host session.
    #[allow(unsafe_code)]
    fn try_test_renderer() -> Option<GlesRenderer> {
        let device = EGLDevice::enumerate().ok()?.next()?;
        let display = unsafe { EGLDisplay::new(device) }.ok()?;
        let context = EGLContext::new(&display).ok()?;
        unsafe { GlesRenderer::new(context) }.ok()
    }

    #[test]
    fn fade_snapshot_keeps_top_bottom_orientation() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping fade_snapshot_keeps_top_bottom_orientation: no EGL device");
            return;
        };

        let size = Size::<i32, Physical>::from((64, 64));
        let buffer_size = Size::<i32, BufferCoord>::from((64, 64));
        let red = [1.0, 0.0, 0.0, 1.0];
        let blue = [0.0, 0.0, 1.0, 1.0];
        let top = SolidColorRenderElement::from_buffer(
            &SolidColorBuffer::new(Size::from((64, 32)), red),
            smithay::utils::Point::from((0, 0)),
            1.0,
            1.0,
            smithay::backend::renderer::element::Kind::Unspecified,
        );
        let bottom = SolidColorRenderElement::from_buffer(
            &SolidColorBuffer::new(Size::from((64, 32)), blue),
            smithay::utils::Point::from((0, 32)),
            1.0,
            1.0,
            smithay::backend::renderer::element::Kind::Unspecified,
        );
        let source_elements: Vec<super::OutputRenderElement> = vec![top.into(), bottom.into()];

        // Texture A: the source scene rendered directly with Normal.
        let mut direct_texture =
            Offscreen::<GlesTexture>::create_buffer(&mut renderer, Fourcc::Abgr8888, buffer_size)
                .expect("create direct texture");
        {
            let mut target = renderer.bind(&mut direct_texture).expect("bind direct");
            let mut tracker = OutputDamageTracker::new(size, Scale::from(1.0), Transform::Normal);
            tracker
                .render_output(&mut renderer, &mut target, 0, &source_elements, [0.0; 4])
                .expect("render source scene")
                .sync
                .wait()
                .expect("direct sync");
        }

        // Texture S: the snapshot path, identical to take_snapshot (Normal transform).
        let mut snapshot_texture =
            Offscreen::<GlesTexture>::create_buffer(&mut renderer, Fourcc::Abgr8888, buffer_size)
                .expect("create snapshot texture");
        {
            let mut target = renderer.bind(&mut snapshot_texture).expect("bind snapshot");
            let mut tracker = OutputDamageTracker::new(size, Scale::from(1.0), Transform::Normal);
            tracker
                .render_output(&mut renderer, &mut target, 0, &source_elements, [0.0; 4])
                .expect("render snapshot")
                .sync
                .wait()
                .expect("snapshot sync");
        }

        let start = Instant::now();
        let fade = ThemeFade::new(&renderer, snapshot_texture, HashSet::new(), start);

        // Texture B: the fade element composited with Normal.
        let mut output_texture =
            Offscreen::<GlesTexture>::create_buffer(&mut renderer, Fourcc::Abgr8888, buffer_size)
                .expect("create output texture");
        {
            let mut target = renderer.bind(&mut output_texture).expect("bind output");
            let mut tracker = OutputDamageTracker::new(size, Scale::from(1.0), Transform::Normal);
            tracker
                .render_output(
                    &mut renderer,
                    &mut target,
                    0,
                    &[fade.element(1.0)],
                    [0.0; 4],
                )
                .expect("render fade element")
                .sync
                .wait()
                .expect("output sync");
        }

        let mut read = |texture: &mut GlesTexture| {
            let target = renderer.bind(texture).expect("bind for read");
            let mapping = renderer
                .copy_framebuffer(&target, Rectangle::from_size(buffer_size), Fourcc::Abgr8888)
                .expect("copy framebuffer");
            renderer
                .map_texture(&mapping)
                .expect("map texture")
                .to_vec()
        };

        let direct_bytes = read(&mut direct_texture);
        let output_bytes = read(&mut output_texture);

        // Guard against a flat image: direct render must have red top and blue bottom.
        let pixel = |bytes: &[u8], x: usize, y: usize| {
            let offset = (y * 64 + x) * 4;
            [
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]
        };
        assert_eq!(
            pixel(&direct_bytes, 8, 8),
            [255, 0, 0, 255],
            "direct top-left should be red"
        );
        assert_eq!(
            pixel(&direct_bytes, 8, 56),
            [0, 0, 255, 255],
            "direct bottom-left should be blue"
        );

        // The fade must reproduce the source scene byte-for-byte.
        assert_eq!(
            direct_bytes, output_bytes,
            "fade element must match the source scene exactly"
        );
    }
}
