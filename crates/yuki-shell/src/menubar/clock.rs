//! The menubar clock.
//!
//! Shows the current time in minutes (`%H:%M`) and commits on the minute edge.

use freya_core::prelude::*;
use torin::prelude::{Alignment, Gaps, Size};
use yuki_design::MENUBAR_HEIGHT;

/// Renders the clock label.
pub fn clock_label(now: String, color: Color) -> Element {
    rect()
        .height(Size::px(MENUBAR_HEIGHT as f32))
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .child(label().text(now).color(color).font_size(13.0))
        .into_element()
}
