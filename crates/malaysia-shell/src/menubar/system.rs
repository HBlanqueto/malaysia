//! The system menu title (snowflake) on the far left of the menubar.

use barrier_kit::menu::{CheckState, MenuItem};
use barrier_kit::platform::ToplevelInfo;

use crate::menubar::{spawn_app, spawn_component};

const SETTINGS_APP_ID: &str = "io.github.hblanqueto.malaysia.settings";

/// The snowflake drawn as the system-menu title.
pub const SNOWFLAKE_SVG: &[u8] = include_bytes!("snowflake.svg");

/// Items for the system menu.
pub fn system_menu_items() -> Vec<MenuItem> {
    vec![
        action("about-this-computer", "Acerca de este equipo", None, false),
        action("system-settings", "Ajustes del sistema…", None, true),
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

/// Handles enabled system-menu actions using the current toplevel list.
pub fn on_system_action(id: String, toplevels: &[ToplevelInfo]) {
    on_system_action_with(id, toplevels, spawn_app, |index| {
        barrier_kit::platform::Toplevels::get().activate(index)
    });
}

/// Injected version: `spawn` and `activate` are supplied by the caller.
///
/// For "system-settings", if a toplevel with `io.github.hblanqueto.malaysia.settings`
/// exists it is activated; otherwise the binary is spawned. This keeps a single
/// settings window.
pub fn on_system_action_with(
    id: String,
    toplevels: &[ToplevelInfo],
    mut spawn: impl FnMut(&str, &[&str]),
    mut activate: impl FnMut(usize),
) {
    match id.as_str() {
        "lock-screen" => spawn_component("lock"),
        "system-settings" => {
            if let Some(index) = toplevels
                .iter()
                .position(|window| window.app_id == SETTINGS_APP_ID)
            {
                activate(index);
            } else {
                spawn("malaysia-settings", &[]);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toplevel(app_id: &str) -> ToplevelInfo {
        ToplevelInfo {
            title: String::new(),
            app_id: app_id.to_string(),
            active: false,
            maximized: false,
            minimized: false,
            restorable: false,
            app_menu: None,
            gtk_menus: None,
        }
    }

    #[test]
    fn system_settings_spawns_when_no_settings_window_exists() {
        let mut launched: Option<String> = None;
        let mut activated: Option<usize> = None;

        on_system_action_with(
            "system-settings".to_string(),
            &[toplevel("foot"), toplevel("org.kde.kate")],
            |binary, _args| launched = Some(binary.to_string()),
            |index| activated = Some(index),
        );

        assert_eq!(launched, Some("malaysia-settings".to_string()));
        assert!(activated.is_none(), "must not activate another window");
    }

    #[test]
    fn system_settings_activates_existing_window_and_does_not_spawn() {
        let mut launched: Option<String> = None;
        let mut activated: Option<usize> = None;

        on_system_action_with(
            "system-settings".to_string(),
            &[
                toplevel("foot"),
                toplevel(SETTINGS_APP_ID),
                toplevel("kate"),
            ],
            |binary, _args| launched = Some(binary.to_string()),
            |index| activated = Some(index),
        );

        assert_eq!(activated, Some(1), "must activate the settings window");
        assert!(launched.is_none(), "must not spawn when the window exists");
    }
}
