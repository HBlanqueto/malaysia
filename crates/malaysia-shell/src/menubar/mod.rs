pub mod appmenu;
pub mod bar;
pub mod clock;
pub mod dbusmenu;
pub mod dbusmenu_map;
pub mod focused;
pub mod gtkmenus;
pub mod help_topics;
pub mod icons;
pub mod menu;
pub mod recents;
pub mod remote;
pub mod shortcuts;
pub mod status;
pub mod system;

pub use bar::run;

use std::env;
use std::path::PathBuf;

use crate::desktop::DesktopEntry;
use crate::localize::tr;

pub fn resolve_app_name(app_id: Option<&str>, entries: &[DesktopEntry]) -> String {
    let Some(app_id) = app_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return tr!("desktop").to_string();
    };
    match find_entry(app_id, entries) {
        Some(entry) => entry.name.clone(),
        None => capitalize(app_id_stem(app_id)),
    }
}

pub fn find_entry<'a>(app_id: &str, entries: &'a [DesktopEntry]) -> Option<&'a DesktopEntry> {
    let app_id = app_id.trim();
    entries
        .iter()
        .find(|entry| entry.id == app_id || entry.startup_wm_class.as_deref() == Some(app_id))
        .or_else(|| {
            let stem = app_id_stem(app_id);
            entries.iter().find(|entry| exec_matches(entry, stem))
        })
}

fn app_id_stem(app_id: &str) -> &str {
    for segment in app_id.rsplit('.') {
        if !segment.is_empty() && segment != "desktop" {
            return segment;
        }
    }
    app_id
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn exec_matches(entry: &DesktopEntry, stem: &str) -> bool {
    let needle = stem.to_lowercase();
    entry.exec.iter().any(|token| {
        let token = token.to_lowercase();
        token.contains(&needle) || needle.contains(&token)
    })
}

pub fn create_desktop_folder() {
    let desktop = ctappearance::xdg::user_dir("DESKTOP")
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("Desktop")))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut target = desktop.join(tr!("desktop-untitled-folder"));
    if target.exists() {
        for i in 2..100 {
            target = desktop.join(tr!("desktop-untitled-folder-numbered", number = i));
            if !target.exists() {
                break;
            }
        }
    }
    if let Err(error) = std::fs::create_dir(&target) {
        eprintln!("menubar: could not create desktop folder: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::DesktopEntry;

    fn entry(id: &str, name: &str, wm_class: Option<&str>) -> DesktopEntry {
        DesktopEntry {
            id: id.to_string(),
            name: name.to_string(),
            generic_name: None,
            keywords: Vec::new(),
            icon: None,
            exec: vec![id.to_string()],
            path: None,
            terminal: false,
            startup_wm_class: wm_class.map(ToOwned::to_owned),
        }
    }

    fn entry_with_exec(id: &str, name: &str, exec: &[&str]) -> DesktopEntry {
        DesktopEntry {
            id: id.to_string(),
            name: name.to_string(),
            generic_name: None,
            keywords: Vec::new(),
            icon: None,
            exec: exec.iter().map(|s| s.to_string()).collect(),
            path: None,
            terminal: false,
            startup_wm_class: None,
        }
    }

    #[test]
    fn foot_resolves_through_its_desktop_entry() {
        let entries = vec![entry("foot", "Foot", None)];
        assert_eq!(resolve_app_name(Some("foot"), &entries), "Foot");
    }

    #[test]
    fn wezterm_resolves_through_its_desktop_id() {
        let entries = vec![entry("org.wezfurlong.wezterm", "WezTerm", None)];
        assert_eq!(
            resolve_app_name(Some("org.wezfurlong.wezterm"), &entries),
            "WezTerm"
        );
    }

    #[test]
    fn startup_wm_class_matches_when_the_id_does_not() {
        let entries = vec![entry("wezterm", "WezTerm", Some("org.wezfurlong.wezterm"))];
        assert_eq!(
            resolve_app_name(Some("org.wezfurlong.wezterm"), &entries),
            "WezTerm"
        );
    }

    #[test]
    fn telegram_resolves_through_a_matching_exec_entry() {
        let entries = vec![entry_with_exec(
            "telegramdesktop",
            "Telegram",
            &["/usr/bin/telegram-desktop"],
        )];
        assert_eq!(
            resolve_app_name(Some("org.telegram.desktop"), &entries),
            "Telegram"
        );
    }

    #[test]
    fn an_app_without_a_desktop_entry_uses_the_last_segment() {
        let entries = Vec::new();
        assert_eq!(
            resolve_app_name(Some("org.example.notes"), &entries),
            "Notes"
        );
        assert_eq!(
            resolve_app_name(Some("com.example.my-app"), &entries),
            "My-app"
        );
    }

    #[test]
    fn trailing_desktop_segment_is_skipped_for_the_stem() {
        let entries = Vec::new();
        assert_eq!(
            resolve_app_name(Some("org.telegram.desktop"), &entries),
            "Telegram"
        );
    }

    #[test]
    fn no_window_or_no_app_id_is_escritorio() {
        let entries = Vec::new();
        assert_eq!(resolve_app_name(None, &entries), tr!("desktop"));
        assert_eq!(resolve_app_name(Some("   "), &entries), tr!("desktop"));
    }
}
