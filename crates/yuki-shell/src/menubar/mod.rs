//! The menu bar component: the bar at the top of the screen.
//!
//! A `Top` layer-shell surface with an exclusive zone, like macOS's menu bar.
//! It is split into modules that each own a slice of the bar:
//!
//! - [`bar::BarApp`] lays out the surface and anchors root popups.
//! - [`system::system_title`] is the snowflake/system menu on the far left.
//! - [`appmenu::app_menu_title`] shows the active app's name (or «Yuki»).
//! - [`status::status_placeholder`] reserves space for future extras.
//! - [`clock::clock_label`] is the clock on the right.
//!
//! Drawing is CelestialKit on Freya (ADR 0034).

pub mod appmenu;
pub mod bar;
pub mod clock;
pub mod menu;
pub mod status;
pub mod system;

pub use bar::run;

use std::path::PathBuf;

use crate::desktop::DesktopEntry;

/// Resolves an app id to its menu-bar name.
///
/// 1. If a `.desktop` entry matches by id or `StartupWMClass`, use its
///    localized `Name`.
/// 2. Otherwise, use the last non-empty segment of the app id with an
///    initial capital.
/// 3. With no app id, the desktop is «Yuki».
pub fn resolve_app_name(app_id: Option<&str>, entries: &[DesktopEntry]) -> String {
    let Some(app_id) = app_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return "Yuki".to_string();
    };
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.id == app_id || entry.startup_wm_class.as_deref() == Some(app_id))
    {
        return entry.name.clone();
    }
    let segment = app_id
        .rsplit('.')
        .find(|segment| !segment.is_empty())
        .unwrap_or(app_id);
    capitalize(segment)
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The active window published by the compositor in `window.domain`.
#[derive(Default, serde::Deserialize)]
pub struct ActiveWindow {
    #[serde(default)]
    #[allow(dead_code)]
    pub title: String,
    #[serde(default)]
    pub app_id: String,
}

/// Reads the compositor's active-window store.
pub fn load_active_window(store_path: &PathBuf) -> Option<ActiveWindow> {
    let text = std::fs::read_to_string(store_path).ok()?;
    toml::from_str(&text).ok()
}

/// Converts a Yuki design RGBA into a Freya colour.
pub fn rgba(c: yuki_design::Rgba) -> freya_core::prelude::Color {
    let [r, g, b, a] = c.to_rgba8();
    freya_core::prelude::Color::from_argb(a, r, g, b)
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
    fn no_window_or_no_app_id_is_yuki() {
        let entries = Vec::new();
        assert_eq!(resolve_app_name(None, &entries), "Yuki");
        assert_eq!(resolve_app_name(Some("   "), &entries), "Yuki");
    }
}
