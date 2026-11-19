use std::time::Duration;

use bk::prelude::*;
use bk::shell::{ScreenLock, use_screen_lock};
use ctdesign::{
    DURATION_BASE, LOCK_KEYBOARD_WIDTH, LOCK_SETTLE_MS, MENUBAR_TITLE_PADDING_X, Rgba, SYMBOL,
    ease_out, progress,
};
use freya_animation::prelude::{AnimNum, Function, OnChange, use_animation_with_dependencies};
use torin::prelude::Gaps;

const POWER_SVG: &[u8] = include_bytes!("power.svg");

pub(crate) const LOCK_SLOT: f32 = 2.0 * MENUBAR_TITLE_PADDING_X + SYMBOL.grid + LOCK_KEYBOARD_WIDTH;

pub(crate) struct LockProgress {
    pub(crate) cover: f32,
    pub(crate) settle: f32,
}

pub(crate) fn slot(content: Element) -> Element {
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

pub(crate) fn status_row(trailing: Element) -> Element {
    super::bar::bar_root(Color::TRANSPARENT)
        .main_align(Alignment::End)
        .child(trailing)
        .into_element()
}

pub(crate) fn power_icon(ink: Rgba) -> Element {
    Image::data(Bytes::from_static(POWER_SVG))
        .rendering_mode(TemplateRenderingMode::Template)
        .color(ink)
        .size(SYMBOL.grid)
        .into_element()
}

pub(crate) fn use_lock_progress() -> Option<LockProgress> {
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
            cover: progress(0.0, elapsed.as_secs_f32(), fade.as_secs_f32()),
            settle: ease_out(progress(
                fade.as_secs_f32(),
                elapsed.as_secs_f32(),
                settle_ms as f32 / 1000.0,
            )),
        }),
        ScreenLock::Unlocked if *animation.has_run_yet().read() => {
            let back = 1.0 - progress(0.0, elapsed.as_secs_f32(), DURATION_BASE as f32 / 1000.0);
            (back > 0.0).then_some(LockProgress {
                cover: back,
                settle: back,
            })
        }
        ScreenLock::Unlocked => None,
    }
}
