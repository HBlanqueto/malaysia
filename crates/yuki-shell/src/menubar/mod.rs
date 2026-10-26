//! The menu bar component: the bar at the top of the screen.
//!
//! A `Top` layer-shell surface with an exclusive zone, like macOS's menu bar.
//! It is split into modules that each own a slice of the bar:
//!
//! - [`bar::BarApp`] lays out the surface and anchors root popups.
//! - [`system`] owns the snowflake/system menu on the far left.
//! - [`appmenu`] owns the desktop menus and the active app's name.
//! - [`clock::clock_label`] is the clock on the right, pushed by the layout.
//!
//! Drawing is CelestialKit on Freya (ADR 0034).

pub mod appmenu;
pub mod bar;
pub mod clock;
pub mod dbusmenu;
pub mod dbusmenu_map;
pub mod focused;
pub mod help_topics;
pub mod icons;
pub mod menu;
pub mod system;

pub use bar::run;

use std::env;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::desktop::DesktopEntry;

/// Resolves an app id to its menu-bar name.
///
/// 1. If a `.desktop` entry matches by id or `StartupWMClass`, use its
///    localized `Name`.
/// 2. Otherwise, if an entry's `Exec` matches the app id (by its last
///    non-trivial segment), use that entry's `Name`.
/// 3. Otherwise, use the last non-empty segment of the app id with an
///    initial capital.
/// 4. With no app id, the desktop is «Yuki».
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
    let stem = app_id_stem(app_id);
    if let Some(entry) = entries.iter().find(|entry| exec_matches(entry, stem)) {
        return entry.name.clone();
    }
    capitalize(stem)
}

/// Returns the last meaningful segment of an app id, skipping a trailing
/// `.desktop` or generic suffixes like `desktop`.
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

/// Whether the desktop entry's first `Exec` token contains the app id stem.
fn exec_matches(entry: &DesktopEntry, stem: &str) -> bool {
    let needle = stem.to_lowercase();
    entry.exec.iter().any(|token| {
        let token = token.to_lowercase();
        token.contains(&needle) || needle.contains(&token)
    })
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

#[derive(serde::Deserialize)]
struct ActiveWindowStore {
    active: ActiveWindow,
}

/// Reads the compositor's active-window store.
pub fn load_active_window(store_path: &PathBuf) -> Option<ActiveWindow> {
    let text = std::fs::read_to_string(store_path).ok()?;
    toml::from_str::<ActiveWindowStore>(&text)
        .ok()
        .map(|store| store.active)
}

/// Converts a Yuki design RGBA into a Freya colour.
pub fn rgba(c: yuki_design::Rgba) -> freya_core::prelude::Color {
    let [r, g, b, a] = c.to_rgba8();
    freya_core::prelude::Color::from_argb(a, r, g, b)
}

/// Spawns a `yuki-shell` component detached, keeping the Wayland socket but
/// stripping the compositor's private Mesa wrapper environment.
pub fn spawn_component(component: &str) {
    let Some(display) = env::var_os("WAYLAND_DISPLAY") else {
        eprintln!("menubar: cannot spawn {component}: WAYLAND_DISPLAY not set");
        return;
    };
    let program = env::current_exe().unwrap_or_else(|_| PathBuf::from("yuki-shell"));
    let mut command = Command::new(program);
    command
        .arg(component)
        .env("WAYLAND_DISPLAY", display)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    sanitize_client_env(&mut command);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    match command.spawn() {
        Ok(_) => eprintln!("menubar: spawn {component}"),
        Err(error) => eprintln!("menubar: could not spawn {component}: {error}"),
    }
}

/// Opens `path` with the user's default application, stripping the wrapper
/// environment so it does not inherit Mesa debug paths from the compositor.
pub fn xdg_open(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    sanitize_client_env(&mut command);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    match command.spawn() {
        Ok(_) => eprintln!("menubar: xdg-open {}", path.display()),
        Err(error) => eprintln!("menubar: could not open '{}': {error}", path.display()),
    }
}

/// Resolves an XDG user dir (Documents, Downloads, Desktop...) or falls back
/// to `HOME/<name>` when `xdg-user-dir` is unavailable.
pub fn xdg_user_dir(kind: &str) -> Option<PathBuf> {
    let output = Command::new("xdg-user-dir").arg(kind).output().ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}

/// Creates a new folder on the Desktop, appending a number if the name exists.
pub fn create_desktop_folder() {
    let desktop = xdg_user_dir("DESKTOP")
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("Desktop")))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut target = desktop.join("Carpeta sin título");
    if target.exists() {
        for i in 2..100 {
            target = desktop.join(format!("Carpeta sin título {i}"));
            if !target.exists() {
                break;
            }
        }
    }
    if let Err(error) = std::fs::create_dir(&target) {
        eprintln!("menubar: could not create desktop folder: {error}");
    }
}

fn sanitize_client_env(command: &mut Command) {
    for name in [
        "LD_LIBRARY_PATH",
        "__EGL_VENDOR_LIBRARY_DIRS",
        "LIBGL_DRIVERS_PATH",
        "GBM_BACKENDS_PATH",
    ] {
        command.env_remove(name);
        let saved = format!("YUKI_ORIG_{name}");
        if let Some(value) = env::var_os(&saved).filter(|value| !value.is_empty()) {
            command.env(name, value);
        }
        command.env_remove(saved);
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
    fn no_window_or_no_app_id_is_yuki() {
        let entries = Vec::new();
        assert_eq!(resolve_app_name(None, &entries), "Yuki");
        assert_eq!(resolve_app_name(Some("   "), &entries), "Yuki");
    }
}
