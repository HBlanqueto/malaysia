use std::sync::OnceLock;

use bk::menu::{MenuIcon, MenuItem};
use bk::shell::ToplevelInfo;
use bk::titlebar::app_id;

use crate::desktop::DesktopEntry;
use crate::dialogs::ABOUT_APP_ID;
use crate::localize::tr;
use crate::menubar::{icons, recents};
use crate::power::{self, PowerAction};
use crate::spawn::{spawn_component, spawn_component_with, spawn_settings, xdg_open};

pub const SNOWFLAKE_SVG: &[u8] = include_bytes!("snowflake.svg");

pub fn system_menu_items(entries: &[DesktopEntry]) -> Vec<MenuItem> {
    static LOGOUT: OnceLock<String> = OnceLock::new();
    let logout = LOGOUT.get_or_init(|| logout_label(user_display_name().as_deref()));
    vec![
        MenuItem::action("about-this-computer", tr!("about-this-computer"))
            .with_icon(MenuIcon::svg(icons::LAPTOP)),
        MenuItem::action("system-settings", tr!("system-settings"))
            .with_icon(MenuIcon::svg(icons::GEAR)),
        MenuItem::Separator,
        MenuItem::submenu(tr!("system-recent-items"), recents::menu_items(entries)),
        MenuItem::Separator,
        MenuItem::action("sleep", tr!("system-sleep")),
        MenuItem::action("restart", tr!("system-restart")),
        MenuItem::action("shutdown", tr!("system-shut-down")),
        MenuItem::Separator,
        MenuItem::action("lock-screen", tr!("system-lock-screen")).with_shortcut("Ctrl+Alt+L"),
        MenuItem::action("logout", logout),
    ]
}

fn logout_label(name: Option<&str>) -> String {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => tr!("system-log-out-user", name = name),
        None => tr!("system-log-out").to_string(),
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
    on_system_action_with(
        id,
        toplevels,
        |_, _| spawn_settings(),
        |id| bk::shell::Toplevels::get().activate(id),
    );
}

fn on_system_action_with(
    id: String,
    toplevels: &[ToplevelInfo],
    mut spawn: impl FnMut(&str, &[&str]),
    mut activate: impl FnMut(u64),
) {
    match id.as_str() {
        "lock-screen" => spawn_component("lock"),
        "about-this-computer" => {
            let about = app_id(ABOUT_APP_ID);
            if let Some(window) = toplevels.iter().find(|window| window.app_id == about) {
                activate(window.id);
            } else {
                spawn_component("about");
            }
        }
        "sleep" => {
            std::thread::spawn(power::suspend);
        }
        "restart" => spawn_component_with("power", &[PowerAction::Restart.arg()]),
        "shutdown" => spawn_component_with("power", &[PowerAction::ShutDown.arg()]),
        "logout" => spawn_component_with("power", &[PowerAction::LogOut.arg()]),
        "system-settings" => {
            let settings = app_id("settings");
            if let Some(window) = toplevels.iter().find(|window| window.app_id == settings) {
                activate(window.id);
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

    fn toplevel(id: u64, app_id: &str) -> ToplevelInfo {
        ToplevelInfo {
            id,
            app_id: app_id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn the_logout_label_names_the_user_when_known() {
        assert!(logout_label(Some("H. Blanqueto")).contains("H. Blanqueto"));
        assert_eq!(logout_label(Some(" ")), tr!("system-log-out"));
    }

    #[test]
    fn system_settings_spawns_when_no_settings_window_exists() {
        let mut launched: Option<String> = None;
        let mut activated: Option<u64> = None;

        on_system_action_with(
            "system-settings".to_string(),
            &[toplevel(10, "foot"), toplevel(11, "org.kde.kate")],
            |binary, _args| launched = Some(binary.to_string()),
            |id| activated = Some(id),
        );

        assert_eq!(launched, Some("malaysia-settings".to_string()));
        assert!(activated.is_none(), "must not activate another window");
    }

    #[test]
    fn system_settings_activates_existing_window_and_does_not_spawn() {
        let mut launched: Option<String> = None;
        let mut activated: Option<u64> = None;

        on_system_action_with(
            "system-settings".to_string(),
            &[
                toplevel(10, "foot"),
                toplevel(11, &app_id("settings")),
                toplevel(12, "kate"),
            ],
            |binary, _args| launched = Some(binary.to_string()),
            |id| activated = Some(id),
        );

        assert_eq!(activated, Some(11), "must activate the settings window");
        assert!(launched.is_none(), "must not spawn when the window exists");
    }

    #[test]
    fn about_this_computer_activates_its_only_window() {
        let mut activated: Option<u64> = None;
        let about = app_id(ABOUT_APP_ID);

        on_system_action_with(
            "about-this-computer".to_string(),
            &[toplevel(10, "foot"), toplevel(11, &about)],
            |_, _| {},
            |id| activated = Some(id),
        );

        assert_eq!(activated, Some(11));
    }
}
