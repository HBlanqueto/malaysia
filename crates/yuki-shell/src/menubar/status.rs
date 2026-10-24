//! Reserved area on the menubar for future status extras.
//!
//! Empty for now; it keeps the clock on the right and the menus on the left.

use freya_core::prelude::*;
use torin::prelude::Size;

/// Renders an empty placeholder that expands as needed.
pub fn status_placeholder() -> Element {
    rect()
        .width(Size::flex(1.0))
        .height(Size::fill())
        .into_element()
}
