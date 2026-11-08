use std::ffi::{OsStr, OsString};
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command};

use crate::config::Config;
use ctappearance::Toolkit;

pub const WRAPPER_ENV: &[&str] = &[
    "LD_LIBRARY_PATH",
    "__EGL_VENDOR_LIBRARY_DIRS",
    "LIBGL_DRIVERS_PATH",
    "GBM_BACKENDS_PATH",
];

pub const ORIGINAL_PREFIX: &str = "MALAYSIA_ORIG_";

pub fn bin_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

pub fn command(program: impl AsRef<OsStr>, config: &Config) -> Command {
    let mut command = Command::new(program);
    sanitize(&mut command);
    apply_qt_decoration(&mut command, config);
    apply_gtk3_text_size(&mut command, config);
    apply_x11_display(&mut command);
    command
}

pub fn shell_component(component: &str, display: Option<&str>) -> io::Result<Child> {
    let shell = bin_dir()
        .map(|dir| dir.join("malaysia-shell"))
        .unwrap_or_else(|| PathBuf::from("malaysia-shell"));
    let mut command = Command::new(shell);
    command.arg(component);
    if let Some(display) = display {
        command.env("WAYLAND_DISPLAY", display);
    }
    apply_x11_display(&mut command);
    command.spawn()
}

pub fn spawn(mut command: Command, config: &Config) -> io::Result<Child> {
    sanitize(&mut command);
    apply_qt_decoration(&mut command, config);
    apply_gtk3_text_size(&mut command, config);
    apply_x11_display(&mut command);
    if let Some(language) = ctappearance::language::system_language() {
        command.env("LANGUAGE", language);
    }
    command.spawn()
}

#[cfg(not(test))]
#[allow(unsafe_code)]
pub fn export_session_env(config: &Config) {
    // ADR 0067: qt5ct and qt6ct both answer to this key; the portal writes their palette and font.
    if std::env::var_os("QT_QPA_PLATFORMTHEME").is_none() {
        // SAFETY: runs single-threaded during startup, before any other thread reads the environment.
        unsafe {
            std::env::set_var("QT_QPA_PLATFORMTHEME", "qt6ct");
        }
    }
    if !config.window.qt_server_decorations {
        return;
    }
    if std::env::var_os("QT_WAYLAND_DISABLE_WINDOWDECORATION").is_none() {
        // SAFETY: runs single-threaded during startup, before any other thread reads the environment.
        unsafe {
            std::env::set_var("QT_WAYLAND_DISABLE_WINDOWDECORATION", "1");
        }
    }
}

#[allow(unsafe_code)]
pub fn mark_nested_session() {
    // SAFETY: called once at nested-backend startup, before any other thread reads the environment.
    unsafe {
        std::env::set_var("MALAYSIA_POWER_DRY_RUN", "1");
    }
}

fn apply_qt_decoration(command: &mut Command, config: &Config) {
    if !config.window.qt_server_decorations {
        return;
    }
    if std::env::var_os("QT_WAYLAND_DISABLE_WINDOWDECORATION").is_none() {
        command.env("QT_WAYLAND_DISABLE_WINDOWDECORATION", "1");
    }
}

// GTK 3 reads GSettings, not the portal, outside a sandbox; GDK_DPI_SCALE
// scales only its fonts (gtk-3-24, gdk/wayland/gdkscreen-wayland.c).
fn apply_gtk3_text_size(command: &mut Command, config: &Config) {
    if std::env::var_os("GDK_DPI_SCALE").is_none() {
        let scale = config.appearance.text_size_for(Toolkit::Gtk);
        command.env("GDK_DPI_SCALE", scale.to_string());
    }
}

fn apply_x11_display(command: &mut Command) {
    let chosen = command.get_envs().any(|(name, _)| name == "DISPLAY");
    if let Some(display) = crate::xwayland::display()
        && !chosen
    {
        command.env("DISPLAY", display);
    }
}

fn sanitize(command: &mut Command) {
    sanitize_with(command, |name| std::env::var_os(name));
}

fn sanitize_with(command: &mut Command, original: impl Fn(&str) -> Option<OsString>) {
    for name in WRAPPER_ENV {
        command.env_remove(name);

        let saved = format!("{ORIGINAL_PREFIX}{name}");
        if let Some(value) = original(&saved).filter(|value| !value.is_empty()) {
            command.env(name, value);
        }
        command.env_remove(&saved);
    }
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;

    use super::*;

    fn child_env(command: &Command) -> Vec<(OsString, Option<OsString>)> {
        command
            .get_envs()
            .map(|(name, value)| (name.to_owned(), value.map(ToOwned::to_owned)))
            .collect()
    }

    fn value<'a>(
        env: &'a [(OsString, Option<OsString>)],
        name: &str,
    ) -> Option<&'a Option<OsString>> {
        env.iter()
            .find(|(key, _)| key == OsStr::new(name))
            .map(|(_, value)| value)
    }

    fn removed(env: &[(OsString, Option<OsString>)], name: &str) -> bool {
        matches!(value(env, name), None | Some(None))
    }

    #[test]
    fn strips_the_wrapper_and_restores_the_originals() {
        let mut command = Command::new("true");
        command.env("LD_LIBRARY_PATH", "/wrapper/lib");
        command.env("__EGL_VENDOR_LIBRARY_DIRS", "/wrapper/egl");
        command.env("LIBGL_DRIVERS_PATH", "/wrapper/dri");
        command.env("GBM_BACKENDS_PATH", "/wrapper/gbm");
        command.env("MALAYSIA_ORIG_LD_LIBRARY_PATH", "/orig/lib");
        command.env("MALAYSIA_ORIG___EGL_VENDOR_LIBRARY_DIRS", "/orig/egl");
        command.env("MALAYSIA_ORIG_LIBGL_DRIVERS_PATH", "");

        sanitize_with(&mut command, |name| match name {
            "MALAYSIA_ORIG_LD_LIBRARY_PATH" => Some(OsString::from("/orig/lib")),
            "MALAYSIA_ORIG___EGL_VENDOR_LIBRARY_DIRS" => Some(OsString::from("/orig/egl")),
            "MALAYSIA_ORIG_LIBGL_DRIVERS_PATH" => Some(OsString::new()),
            "MALAYSIA_ORIG_GBM_BACKENDS_PATH" => None,
            _ => None,
        });

        let env = child_env(&command);
        assert_eq!(
            value(&env, "LD_LIBRARY_PATH"),
            Some(&Some(OsString::from("/orig/lib")))
        );
        assert_eq!(
            value(&env, "__EGL_VENDOR_LIBRARY_DIRS"),
            Some(&Some(OsString::from("/orig/egl")))
        );
        assert!(removed(&env, "LIBGL_DRIVERS_PATH"));
        assert!(removed(&env, "GBM_BACKENDS_PATH"));
        for name in WRAPPER_ENV {
            assert!(
                removed(&env, &format!("{ORIGINAL_PREFIX}{name}")),
                "{name} original leaked"
            );
        }
    }

    #[test]
    fn the_public_path_does_not_forward_the_wrapper() {
        let command = self::command("true", &Config::default());
        let env = child_env(&command);
        for name in WRAPPER_ENV {
            assert!(removed(&env, name), "{name} was forwarded");
        }
    }

    #[test]
    fn a_spawned_child_does_not_see_the_wrapper() {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("printf '%s' \"${LD_LIBRARY_PATH-}\"")
            .env("LD_LIBRARY_PATH", "/wrapper/lib")
            .stdout(Stdio::piped());

        let output = spawn(command, &Config::default())
            .expect("spawn the probe")
            .wait_with_output()
            .expect("wait for the probe");

        assert!(
            output.stdout.is_empty(),
            "child inherited the wrapper: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    #[test]
    #[allow(unsafe_code)]
    fn qt_server_decorations_are_exported_by_default() {
        let previous = std::env::var_os("QT_WAYLAND_DISABLE_WINDOWDECORATION");
        // SAFETY: environment mutation is isolated to this single-threaded test.
        unsafe { std::env::remove_var("QT_WAYLAND_DISABLE_WINDOWDECORATION") };

        let command = self::command("true", &Config::default());
        let env = child_env(&command);

        // SAFETY: environment mutation is isolated to this single-threaded test.
        match previous {
            Some(value) => unsafe {
                std::env::set_var("QT_WAYLAND_DISABLE_WINDOWDECORATION", value)
            },
            None => unsafe { std::env::remove_var("QT_WAYLAND_DISABLE_WINDOWDECORATION") },
        }

        assert_eq!(
            value(&env, "QT_WAYLAND_DISABLE_WINDOWDECORATION"),
            Some(&Some(OsString::from("1")))
        );
    }

    #[test]
    fn qt_server_decorations_can_be_disabled() {
        let mut config = Config::default();
        config.window.qt_server_decorations = false;
        let command = self::command("true", &config);
        let env = child_env(&command);
        assert!(removed(&env, "QT_WAYLAND_DISABLE_WINDOWDECORATION"));
    }
}
