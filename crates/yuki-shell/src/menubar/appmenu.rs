//! The application menu title in the menubar.
//!
//! Shows the active application's name in bold. When no window is focused it
//! reads «Yuki» and will later expose the Yuki desktop menu.

use freya_core::prelude::*;
use torin::prelude::{Alignment, Gaps, Size};
use yuki_design::{MENUBAR_APP_NAME_WEIGHT, MENUBAR_HEIGHT, MENUBAR_TITLE_MAX_WIDTH};

/// Renders the application-name title.
pub fn app_menu_title(app: String) -> Element {
    rect()
        .height(Size::px(MENUBAR_HEIGHT as f32))
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .child(
            label()
                .max_lines(1)
                .width(Size::px(MENUBAR_TITLE_MAX_WIDTH))
                .text(app)
                .font_weight(MENUBAR_APP_NAME_WEIGHT)
                .font_size(13.0),
        )
        .into_element()
}
