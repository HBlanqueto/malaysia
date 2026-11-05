use std::sync::OnceLock;

use barrier_kit::menu::{MenuIcon, MenuItem};
use barrier_kit::shell::ToplevelInfo;

use crate::desktop::DesktopEntry;
use crate::menubar::{icons, items, recents};
use crate::power::{self, PowerAction};
use crate::spawn::{spawn_app, spawn_component, spawn_component_with, xdg_open};

const SETTINGS_APP_ID: &str = "io.github.hblanqueto.malaysia.settings";

pub const SNOWFLAKE_SVG: &[u8] = include_bytes!("snowflake.svg");

pub fn system_menu_items(entries: &[DesktopEntry]) -> Vec<MenuItem> {
    static LOGOUT: OnceLock<String> = OnceLock::new();
    let logout = LOGOUT.get_or_init(|| logout_label(user_display_name().as_deref()));
    let mut items = base_items(logout);
    if let Some(MenuItem::Submenu { items: recent, .. }) = items
        .iter_mut()
        .find(|item| matches!(item, MenuItem::Submenu { .. }))
    {
        *recent = recents::menu_items(entries);
    }
    items
}

fn base_items(logout: &str) -> Vec<MenuItem> {
    vec![
        items::action(
            "about-this-computer",
            "Acerca de este PC",
            None,
            true,
            Some(MenuIcon::svg(icons::LAPTOP)),
        ),
        items::action(
            "system-settings",
            "Ajustes del sistema…",
            None,
            true,
            Some(MenuIcon::svg(icons::GEAR)),
        ),
        MenuItem::Separator,
        MenuItem::Submenu {
            id: None,
            label: "Elementos recientes".to_string(),
            enabled: true,
            items: Vec::new(),
            icon: None,
        },
        MenuItem::Separator,
        items::action("sleep", "Suspender", None, true, None),
        items::action("restart", "Reiniciar…", None, true, None),
        items::action("shutdown", "Apagar…", None, true, None),
        MenuItem::Separator,
        items::action(
            "lock-screen",
            "Bloquear pantalla",
            Some("Ctrl+Alt+L"),
            true,
            None,
        ),
        items::action("logout", logout, None, true, None),
    ]
}

fn logout_label(name: Option<&str>) -> String {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("Cerrar sesión de {name}…"),
        None => "Cerrar sesión…".to_string(),
    }
}

fn user_display_name() -> Option<String> {
    // SAFETY: the static entry is copied below before any other `getpw*` call on this thread.
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    if pw.is_null() {
        return None;
    }
    let read = |ptr: *const libc::c_char| {
        (!ptr.is_null()).then(|| {
            // SAFETY: `read` only receives `passwd` fields, NUL-terminated and checked non-null.
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned()
        })
    };
    // SAFETY: `pw` was checked non-null above.
    let gecos = read(unsafe { (*pw).pw_gecos })
        .and_then(|gecos| gecos.split(',').next().map(str::trim).map(str::to_string))
        .filter(|name| !name.is_empty());
    // SAFETY: `pw` was checked non-null above.
    gecos.or_else(|| read(unsafe { (*pw).pw_name }))
}

pub fn on_system_action(id: String, toplevels: &[ToplevelInfo], entries: &[DesktopEntry]) {
    if let Some(app) = id.strip_prefix(recents::APP_PREFIX) {
        if let Some(entry) = entries.iter().find(|entry| entry.id == app) {
            recents::record_app(&entry.id);
            crate::launcher::spawn_entry(entry, None);
        }
        return;
    }
    if let Some(path) = id.strip_prefix(recents::DOC_PREFIX) {
        xdg_open(path);
        return;
    }
    if id == recents::CLEAR_ID {
        recents::clear();
        return;
    }
    on_system_action_with(id, toplevels, spawn_app, |index| {
        barrier_kit::shell::Toplevels::get().activate(index)
    });
}

pub fn on_system_action_with(
    id: String,
    toplevels: &[ToplevelInfo],
    mut spawn: impl FnMut(&str, &[&str]),
    mut activate: impl FnMut(usize),
) {
    match id.as_str() {
        "lock-screen" => spawn_component("lock"),
        "about-this-computer" => {
            if let Some(index) = toplevels.iter().position(|window| {
                window.app_id.strip_prefix(vow_design::NATIVE_APP_ID_PREFIX) == Some("about")
            }) {
                activate(index);
            } else {
                spawn_component("about");
            }
        }
        "sleep" => {
            std::thread::spawn(|| power::run(PowerAction::Suspend));
        }
        "restart" => spawn_component_with("power", &[PowerAction::Restart.arg()]),
        "shutdown" => spawn_component_with("power", &[PowerAction::ShutDown.arg()]),
        "logout" => spawn_component_with("power", &[PowerAction::LogOut.arg()]),
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

    fn labels(items: &[MenuItem]) -> Vec<&str> {
        items
            .iter()
            .map(|item| item.label().unwrap_or("-"))
            .collect()
    }

    #[test]
    fn the_system_menu_follows_the_macos_layout() {
        let items = base_items(&logout_label(Some("H. Blanqueto")));
        assert_eq!(
            labels(&items),
            [
                "Acerca de este PC",
                "Ajustes del sistema…",
                "-",
                "Elementos recientes",
                "-",
                "Suspender",
                "Reiniciar…",
                "Apagar…",
                "-",
                "Bloquear pantalla",
                "Cerrar sesión de H. Blanqueto…",
            ]
        );
        assert!(matches!(items[3], MenuItem::Submenu { .. }));
        assert!(matches!(&items[0], MenuItem::Action { icon: Some(_), .. }));
        assert!(matches!(&items[1], MenuItem::Action { icon: Some(_), .. }));
        assert_eq!(logout_label(Some(" ")), "Cerrar sesión…");
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

    #[test]
    fn about_this_computer_activates_its_only_window() {
        let mut activated: Option<usize> = None;
        let about = format!("{}about", vow_design::NATIVE_APP_ID_PREFIX);

        on_system_action_with(
            "about-this-computer".to_string(),
            &[toplevel("foot"), toplevel(&about)],
            |_, _| {},
            |index| activated = Some(index),
        );

        assert_eq!(activated, Some(1));
    }
}
