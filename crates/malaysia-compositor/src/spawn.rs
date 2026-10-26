//! The one place the compositor starts another process.
//!
//! The Nix wrapper gives `malaysia-compositor` and `malaysia-shell` a private Mesa/EGL
//! environment. That environment must not reach the clients they launch: a
//! client has to resolve its own Mesa (the system one) and its own EGL vendor
//! directory. The wrapper also saves the previous values in `MALAYSIA_ORIG_*`, so
//! a child gets the same environment it would have had without the wrapper.
//!
//! Use [`command`] to build the command and [`spawn`] to start it. Do not call
//! `Command::spawn` directly for a client.

use std::ffi::{OsStr, OsString};
use std::io;
use std::process::{Child, Command};

use crate::config::Config;

/// Variables the package wrapper sets for Malaysia itself.
pub const WRAPPER_ENV: &[&str] = &[
    "LD_LIBRARY_PATH",
    "__EGL_VENDOR_LIBRARY_DIRS",
    "LIBGL_DRIVERS_PATH",
    "GBM_BACKENDS_PATH",
];

/// Prefix the wrapper uses to save the value each variable had before it.
pub const ORIGINAL_PREFIX: &str = "MALAYSIA_ORIG_";

/// Builds a command with the wrapper environment removed and the Qt
/// decoration variable set according to the configuration.
///
/// The caller may still add arguments and its own environment; [`spawn`]
/// applies the same cleanup again, so the final child cannot inherit the
/// wrapper's Mesa paths.
pub fn command(program: impl AsRef<OsStr>, config: &Config) -> Command {
    let mut command = Command::new(program);
    sanitize(&mut command);
    apply_qt_decoration(&mut command, config);
    command
}

/// Builds a command for another Yuki process (the shell components).
///
/// Unlike a client, a shell component is part of Malaysia's own runtime and may
/// need the EGL/GBM environment the compositor was started with — in
/// development the dev shell exports it, and the packaged `malaysia-shell` wrapper
/// sets it in production. The client sanitization is deliberately not applied
/// here.
pub fn shell_command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

/// Starts a shell command without stripping Malaysia's own runtime environment.
pub fn spawn_shell(mut command: Command) -> io::Result<Child> {
    command.spawn()
}

/// Starts `command` with the wrapper environment removed and the Qt
/// decoration variable applied.
pub fn spawn(mut command: Command, config: &Config) -> io::Result<Child> {
    sanitize(&mut command);
    apply_qt_decoration(&mut command, config);
    command.spawn()
}

/// Exports `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` in the compositor's own
/// environment, so every client launched in the session inherits it.
///
/// This is done once at startup and only when the value is not already set,
/// letting the user or a wrapper override it.
#[allow(unsafe_code)]
pub fn export_session_env(config: &Config) {
    if !config.window.qt_server_decorations {
        return;
    }
    if std::env::var_os("QT_WAYLAND_DISABLE_WINDOWDECORATION").is_none() {
        // SAFETY: this runs single-threaded during compositor startup, before
        // any other thread reads this environment variable.
        unsafe {
            std::env::set_var("QT_WAYLAND_DISABLE_WINDOWDECORATION", "1");
        }
    }
}

/// Adds the Qt decoration variable to a command when the configuration asks
/// for it and the environment has not already set one.
fn apply_qt_decoration(command: &mut Command, config: &Config) {
    if !config.window.qt_server_decorations {
        return;
    }
    if std::env::var_os("QT_WAYLAND_DISABLE_WINDOWDECORATION").is_none() {
        command.env("QT_WAYLAND_DISABLE_WINDOWDECORATION", "1");
    }
}

/// Removes every managed variable from `command` and restores its original
/// when the wrapper saved a non-empty one.
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
        // The result must not depend on a value inherited from a previous test
        // or from the cargo environment.
        let previous = std::env::var_os("QT_WAYLAND_DISABLE_WINDOWDECORATION");
        // SAFETY: environment mutation is isolated to this single-threaded test.
        unsafe { std::env::remove_var("QT_WAYLAND_DISABLE_WINDOWDECORATION") };

        let command = self::command("true", &Config::default());
        let env = child_env(&command);

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
