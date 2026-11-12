use bk::prelude::*;
use ctdesign::{MENUBAR_FONT_SIZE, MENUBAR_MARGIN, Rgba};
use torin::prelude::Gaps;

use ctappearance::notifications::Focus;

use crate::notifications::{focus_symbol, toggle_focus, toggle_panel};

// Wayland sends modifiers only to the focused surface, so Option-click becomes a secondary click.
pub fn clock_label(now: String, ink: Rgba, focus: Option<Focus>) -> Element {
    HStack::new()
        .height(Size::fill())
        .padding(Gaps::new(0.0, 0.0, 0.0, MENUBAR_MARGIN as f32))
        .cross_align(Alignment::Center)
        .spacing(MENUBAR_MARGIN as f32 / 2.0)
        .on_press(|_| toggle_panel())
        .on_secondary_down(|_| toggle_focus())
        .maybe_child(focus.map(|focus| {
            Image::system(focus_symbol(focus))
                .size(MENUBAR_FONT_SIZE)
                .color(ink)
        }))
        .child(Text::new(now).color(color(ink)))
        .into_element()
}
