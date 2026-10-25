//! The system menu title (snowflake) on the far left of the menubar.

use celestial_kit::menu::{CheckState, MenuItem};

use crate::menubar::spawn_component;

/// The snowflake drawn as the system-menu title.
pub const SNOWFLAKE_SVG: &[u8] = include_bytes!("snowflake.svg");

/// Items for the system menu.
pub fn system_menu_items() -> Vec<MenuItem> {
    vec![
        action("about-this-computer", "Acerca de este equipo", None, false),
        action("system-settings", "Ajustes del sistema…", None, false),
        MenuItem::Separator,
        action(
            "force-quit",
            "Forzar salida…",
            Some("Alt+Super+Escape"),
            false,
        ),
        MenuItem::Separator,
        action("sleep", "Suspender", None, false),
        action("restart", "Reiniciar…", None, false),
        action("shutdown", "Apagar…", None, false),
        MenuItem::Separator,
        action("lock-screen", "Bloquear pantalla", Some("Ctrl+Alt+L"), true),
        action("logout", "Cerrar sesión…", None, false),
    ]
}

fn action(id: &str, label: &str, shortcut: Option<&str>, enabled: bool) -> MenuItem {
    MenuItem::Action {
        id: id.to_string(),
        label: label.to_string(),
        shortcut: shortcut.map(ToOwned::to_owned),
        enabled,
        check: CheckState::None,
        icon: None,
    }
}

/// Handles the only enabled system action today: lock the session.
pub fn on_system_action(id: String) {
    if id == "lock-screen" {
        spawn_component("lock");
    }
}
