use bk::prelude::*;
use ctdesign::MENUBAR_MARGIN;
use torin::prelude::Gaps;

pub fn clock_label(now: String, color: Color) -> Element {
    rect()
        .height(Size::fill())
        .padding(Gaps::new(0.0, 0.0, 0.0, MENUBAR_MARGIN as f32))
        .center()
        .child(Text::new(now).color(color))
        .into_element()
}
