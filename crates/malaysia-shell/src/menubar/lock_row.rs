use std::time::Duration;

use bk::prelude::*;
use bk::shell::{ScreenLock, use_screen_lock};
use ctdesign::{DURATION_BASE, LOCK_SETTLE_MS, MENUBAR_TITLE_PADDING_X, SYMBOL};
use freya_animation::prelude::{AnimNum, Function, OnChange, use_animation_with_dependencies};
use freya_components::svg_viewer::SvgViewer;
use torin::prelude::Gaps;

const POWER_SVG: &[u8] = include_bytes!("power.svg");

// ADR 0074: the bar keeps this slot free while locked, and the lock screen draws its power button in it.
pub const POWER_SLOT: f32 = 2.0 * MENUBAR_TITLE_PADDING_X + SYMBOL.grid;

pub struct LockProgress {
    pub cover: f32,
    pub settle: f32,
}

pub fn slot(content: Element) -> Element {
    let resolved = bk::theme::resolved();
    let palette = bk::theme::menu_palette(&resolved, bk::theme::SurfaceKind::LayerShell);
    rect()
        .height(Size::fill())
        .center()
        .child(
            rect()
                .height(Size::px(palette.bar_title_open_height))
                .padding(Gaps::new(
                    0.0,
                    MENUBAR_TITLE_PADDING_X,
                    0.0,
                    MENUBAR_TITLE_PADDING_X,
                ))
                .center()
                .child(content),
        )
        .into_element()
}

pub fn status_row(trailing: Element) -> Element {
    super::bar::bar_root(Color::TRANSPARENT)
        .main_align(Alignment::End)
        .child(trailing)
        .into_element()
}

pub fn power_icon(ink: Color) -> Element {
    SvgViewer::new(("lock-power", Bytes::from_static(POWER_SVG)))
        .width(Size::px(SYMBOL.grid))
        .height(Size::px(SYMBOL.grid))
        .color(ink)
        .show_loader(false)
        .into_element()
}

pub fn use_lock_progress() -> Option<LockProgress> {
    let lock = *use_screen_lock().read();
    let settle_ms = if bk::theme::resolved().reduced_motion {
        0
    } else {
        LOCK_SETTLE_MS
    };
    let total = move |lock: &ScreenLock| match lock {
        ScreenLock::Locking(fade) => fade.as_millis() as u64 + settle_ms,
        ScreenLock::Unlocked => DURATION_BASE,
    };
    let animation = use_animation_with_dependencies(&lock, move |conf, lock| {
        conf.on_change(OnChange::Rerun);
        let total = total(lock);
        AnimNum::new(0.0, total as f32)
            .time(total)
            .function(Function::Linear)
    });
    let elapsed = Duration::from_millis(animation.get().value() as u64);
    match lock {
        ScreenLock::Locking(fade) => Some(LockProgress {
            cover: fraction(elapsed, Duration::ZERO, fade),
            settle: ease_out(fraction(elapsed, fade, Duration::from_millis(settle_ms))),
        }),
        ScreenLock::Unlocked if *animation.has_run_yet().read() => {
            let back = 1.0
                - fraction(
                    elapsed,
                    Duration::ZERO,
                    Duration::from_millis(DURATION_BASE),
                );
            (back > 0.0).then_some(LockProgress {
                cover: back,
                settle: back,
            })
        }
        ScreenLock::Unlocked => None,
    }
}

fn fraction(elapsed: Duration, start: Duration, length: Duration) -> f32 {
    if length.is_zero() {
        return if elapsed >= start { 1.0 } else { 0.0 };
    }
    (elapsed.saturating_sub(start).as_secs_f32() / length.as_secs_f32()).clamp(0.0, 1.0)
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settle_waits_for_the_fade_and_a_zero_length_jumps() {
        let fade = Duration::from_millis(1000);
        let settle = Duration::from_millis(600);
        assert_eq!(fraction(Duration::from_millis(500), fade, settle), 0.0);
        assert_eq!(fraction(Duration::from_millis(1300), fade, settle), 0.5);
        assert_eq!(
            fraction(Duration::from_millis(1000), fade, Duration::ZERO),
            1.0
        );
        assert_eq!(fraction(Duration::ZERO, fade, Duration::ZERO), 0.0);
    }
}
