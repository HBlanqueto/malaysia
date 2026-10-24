//! The system menu title (snowflake) on the far left of the menubar.

use freya_core::prelude::*;
use torin::prelude::{Alignment, Gaps, Size};
use yuki_design::MENUBAR_HEIGHT;

/// Renders the system-menu title.
pub fn system_title() -> Element {
    rect()
        .height(Size::px(MENUBAR_HEIGHT as f32))
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .child(label().text("\u{2744}").font_size(13.0))
        .into_element()
}
