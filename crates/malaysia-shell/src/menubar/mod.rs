//! Menubar component: the top layer-shell bar.
pub mod appmenu;
pub mod bar;
pub mod clock;
pub mod dbusmenu;
pub mod dbusmenu_map;
pub mod focused;
pub mod gtkmenus;
pub mod help_topics;
pub mod icons;
pub mod items;
pub mod menu;
pub mod recents;
pub mod remote;
pub mod system;

pub use bar::run;

use std::env;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::desktop::DesktopEntry;

pub fn resolve_app_name(app_id: Option<&str>, entries: &[DesktopEntry]) -> String {
    let Some(app_id) = app_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return "Escritorio".to_string();
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

pub fn load_active_window(store_path: &PathBuf) -> Option<ActiveWindow> {
    let text = std::fs::read_to_string(store_path).ok()?;
    toml::from_str::<ActiveWindowStore>(&text)
        .ok()
        .map(|store| store.active)
}

pub fn spawn_component(component: &str) {
    spawn_component_with(component, &[]);
}

pub fn spawn_component_with(component: &str, args: &[&str]) {
    let Some(display) = env::var_os("WAYLAND_DISPLAY") else {
        eprintln!("menubar: cannot spawn {component}: WAYLAND_DISPLAY not set");
        return;
    };
    let program = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("malaysia-shell")))
        .unwrap_or_else(|| PathBuf::from("malaysia-shell"));
    let mut command = Command::new(&program);
    command
        .arg(component)
        .args(args)
        .env("WAYLAND_DISPLAY", display)
        .stdin(Stdio::null());
    let log_name = format!("malaysia-shell-{component}");
    if let Some(path) = kit_log_path(&log_name) {
        command.stdout(open_log(&path)).stderr(open_log(&path));
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    prepare_kit_env(&mut command);
    spawn_logged(&log_name, &program, command);
}

pub fn spawn_app(binary: &str, args: &[&str]) {
    let Some(display) = env::var_os("WAYLAND_DISPLAY") else {
        eprintln!("menubar: cannot spawn {binary}: WAYLAND_DISPLAY not set");
        return;
    };

    let program = if binary == "malaysia-settings" {
        match resolve_settings_binary() {
            Some(path) => path,
            None => {
                let searched = env::current_exe()
                    .ok()
                    .and_then(|exe| exe.parent().map(|dir| dir.join("malaysia-settings")))
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "next to malaysia-shell".to_string());
                tracing::warn!(
                    "menubar: malaysia-settings not found (searched {searched} and PATH)"
                );
                return;
            }
        }
    } else {
        PathBuf::from(binary)
    };

    let mut command = if is_kit_binary(binary) {
        Command::new(&program)
    } else {
        crate::spawn::app_command(&program)
    };
    command
        .args(args)
        .env("WAYLAND_DISPLAY", display)
        .stdin(Stdio::null());
    if let Some(path) = kit_log_path(binary) {
        command.stdout(open_log(&path)).stderr(open_log(&path));
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    if is_kit_binary(binary) {
        prepare_kit_env(&mut command);
    } else {
        sanitize_client_env(&mut command);
    }
    spawn_logged(binary, &program, command);
}

fn resolve_settings_binary() -> Option<PathBuf> {
    let current = env::current_exe().ok()?;
    let candidate = current.parent()?.join("malaysia-settings");
    if candidate.is_file() {
        return Some(candidate);
    }
    let path = env::var("PATH").ok()?;
    path.split(':')
        .map(|dir| Path::new(dir).join("malaysia-settings"))
        .find(|candidate| candidate.is_file())
}

pub fn xdg_open(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    let mut command = crate::spawn::app_command("xdg-open");
    command.arg(path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    sanitize_client_env(&mut command);
    match crate::spawn::detached(command) {
        Ok(()) => eprintln!("menubar: xdg-open {}", path.display()),
        Err(error) => eprintln!("menubar: could not open '{}': {error}", path.display()),
    }
}

pub fn xdg_user_dir(kind: &str) -> Option<PathBuf> {
    let output = Command::new("xdg-user-dir").arg(kind).output().ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}

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

pub(crate) fn sanitize_client_env(command: &mut Command) {
    for name in [
        "LD_LIBRARY_PATH",
        "__EGL_VENDOR_LIBRARY_DIRS",
        "LIBGL_DRIVERS_PATH",
        "GBM_BACKENDS_PATH",
    ] {
        command.env_remove(name);
        let saved = format!("MALAYSIA_ORIG_{name}");
        if let Some(value) = env::var_os(&saved).filter(|value| !value.is_empty()) {
            command.env(name, value);
        }
        command.env_remove(saved);
    }
}

fn is_kit_binary(binary: &str) -> bool {
    matches!(binary, "malaysia-shell" | "malaysia-settings")
}

fn prepare_kit_env(command: &mut Command) {
    for name in [
        "LD_LIBRARY_PATH",
        "__EGL_VENDOR_LIBRARY_DIRS",
        "LIBGL_DRIVERS_PATH",
        "GBM_BACKENDS_PATH",
    ] {
        let saved = format!("MALAYSIA_ORIG_{name}");
        if let Some(value) = env::var_os(&saved).filter(|value| !value.is_empty()) {
            command.env(name, value);
        }
        command.env_remove(saved);
    }
}

fn kit_log_path(name: &str) -> Option<PathBuf> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")?;
    let dir = PathBuf::from(runtime).join("malaysia-compositor");
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "menubar: could not create log dir {}: {error}",
            dir.display()
        );
        return None;
    }
    Some(dir.join(format!("{name}.log")))
}

fn open_log(path: &Path) -> Stdio {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(file) => Stdio::from(file),
        Err(_) => Stdio::null(),
    }
}

fn spawn_logged(name: &str, program: &Path, command: Command) {
    tracing::info!(name, program = %program.display(), "spawning kit binary");
    if let Err(error) = crate::spawn::detached(command) {
        tracing::warn!(name, %error, "could not spawn kit binary");
        eprintln!("menubar: could not spawn {name}: {error}");
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
        assert_eq!(resolve_app_name(None, &entries), "Escritorio");
        assert_eq!(resolve_app_name(Some("   "), &entries), "Escritorio");
    }

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_env<F: FnOnce()>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        f();
    }

    struct EnvGuard<'a> {
        name: &'a str,
        old: Option<std::ffi::OsString>,
    }

    impl Drop for EnvGuard<'_> {
        fn drop(&mut self) {
            // SAFETY: the test lock is held while the environment changes.
            unsafe {
                match &self.old {
                    Some(value) => env::set_var(self.name, value),
                    None => env::remove_var(self.name),
                }
            }
        }
    }

    fn guard(name: &str) -> EnvGuard<'_> {
        EnvGuard {
            name,
            old: env::var_os(name),
        }
    }

    #[test]
    fn client_env_strips_wrapper_and_restores_original() {
        with_env(|| {
            let _ld = guard("LD_LIBRARY_PATH");
            let _orig = guard("MALAYSIA_ORIG_LD_LIBRARY_PATH");
            unsafe {
                env::set_var("LD_LIBRARY_PATH", "/wrapper/lib");
                env::set_var("MALAYSIA_ORIG_LD_LIBRARY_PATH", "/orig/lib");
            }

            let mut command = Command::new("env");
            sanitize_client_env(&mut command);
            let output = command.output().expect("env should run");
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(
                text.contains("LD_LIBRARY_PATH=/orig/lib"),
                "sanitized env should restore the original path: {text}"
            );
            assert!(
                !text.contains("/wrapper/lib"),
                "sanitized env should drop the wrapper path: {text}"
            );
        });
    }

    #[test]
    fn kit_env_keeps_dev_library_path_when_no_original_saved() {
        with_env(|| {
            let _ld = guard("LD_LIBRARY_PATH");
            let _orig = guard("MALAYSIA_ORIG_LD_LIBRARY_PATH");
            unsafe {
                env::set_var("LD_LIBRARY_PATH", "/dev/lib");
                env::remove_var("MALAYSIA_ORIG_LD_LIBRARY_PATH");
            }

            let mut command = Command::new("env");
            prepare_kit_env(&mut command);
            let output = command.output().expect("env should run");
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(
                text.contains("LD_LIBRARY_PATH=/dev/lib"),
                "kit env should keep the dev shell path: {text}"
            );
        });
    }

    #[test]
    fn kit_env_restores_original_when_it_was_saved() {
        with_env(|| {
            let _ld = guard("LD_LIBRARY_PATH");
            let _orig = guard("MALAYSIA_ORIG_LD_LIBRARY_PATH");
            unsafe {
                env::set_var("LD_LIBRARY_PATH", "/wrapper/lib");
                env::set_var("MALAYSIA_ORIG_LD_LIBRARY_PATH", "/orig/lib");
            }

            let mut command = Command::new("env");
            prepare_kit_env(&mut command);
            let output = command.output().expect("env should run");
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(
                text.contains("LD_LIBRARY_PATH=/orig/lib"),
                "kit env should restore the original path when it was saved: {text}"
            );
        });
    }
}
