use freya_core::prelude::*;
use torin::prelude::{Gaps, Size};
use vow_design::MENUBAR_MARGIN;

pub fn clock_label(now: String, color: Color) -> Element {
    rect()
        .height(Size::fill())
        .padding(Gaps::new(0.0, 0.0, 0.0, MENUBAR_MARGIN as f32))
        .center()
        .child(label().text(now).color(color))
        .into_element()
}
