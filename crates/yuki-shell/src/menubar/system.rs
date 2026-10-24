//! The system menu title (snowflake) on the far left of the menubar.

use std::rc::Rc;

use celestial_kit::menu::{CheckState, MenuItem};
use freya_core::prelude::*;

use crate::menubar::menu::{MenuState, MenuTitleSpec, menu_title};
use crate::menubar::spawn_component;

/// Items for the system menu (dummy entries for the layout commit).
pub fn system_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::Action {
            id: "about-this-computer".to_string(),
            label: "Acerca de este equipo".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Action {
            id: "system-settings".to_string(),
            label: "Ajustes del sistema…".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Separator,
        MenuItem::Action {
            id: "lock-screen".to_string(),
            label: "Bloquear pantalla".to_string(),
            shortcut: Some("Ctrl+Alt+L".to_string()),
            enabled: true,
            check: CheckState::None,
        },
        MenuItem::Separator,
        MenuItem::Action {
            id: "sleep".to_string(),
            label: "Suspender".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Action {
            id: "restart".to_string(),
            label: "Reiniciar…".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Action {
            id: "shutdown".to_string(),
            label: "Apagar…".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Separator,
        MenuItem::Action {
            id: "logout".to_string(),
            label: "Cerrar sesión…".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
    ]
}

/// Renders the system-menu title.
pub fn system_title(state: MenuState, titles: Rc<Vec<MenuTitleSpec>>) -> Element {
    menu_title(0, titles, state)
}

/// Handles the only enabled system action today: lock the session.
pub fn on_system_action(id: String) {
    if id == "lock-screen" {
        spawn_component("lock");
    }
}
