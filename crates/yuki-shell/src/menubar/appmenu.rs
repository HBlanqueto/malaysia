//! The application menu title in the menubar.
//!
//! Shows the active application's name in bold. When no window is focused it
//! reads «Yuki» and exposes the Yuki desktop menu.

use std::rc::Rc;

use celestial_kit::menu::{CheckState, MenuItem};
use freya_core::prelude::*;

use crate::menubar::menu::{MenuState, MenuTitleSpec, menu_title};

/// Items for the Yuki desktop menu (dummy entries for the layout commit).
pub fn yuki_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::Action {
            id: "about-yuki".to_string(),
            label: "Acerca de Yuki".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Action {
            id: "settings".to_string(),
            label: "Ajustes…".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Separator,
        MenuItem::Action {
            id: "empty-trash".to_string(),
            label: "Vaciar papelera…".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
    ]
}

/// Renders the application-name title (or the Yuki menu when no app is focused).
pub fn app_menu_title(_app: String, state: MenuState, titles: Rc<Vec<MenuTitleSpec>>) -> Element {
    menu_title(1, titles, state)
}
