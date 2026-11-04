use freya_core::prelude::*;
use torin::prelude::{Alignment, Gaps, Size};
use vow_design::{MENUBAR_FONT_SIZE, MENUBAR_MARGIN};

pub fn clock_label(now: String, color: Color) -> Element {
    rect()
        .height(Size::fill())
        .padding(Gaps::new(0.0, 0.0, 0.0, MENUBAR_MARGIN as f32))
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .child(label().text(now).color(color).font_size(MENUBAR_FONT_SIZE))
        .into_element()
}
