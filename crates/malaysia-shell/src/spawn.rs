use std::env;
use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const WRAPPER_ENV: [&str; 4] = [
    "LD_LIBRARY_PATH",
    "__EGL_VENDOR_LIBRARY_DIRS",
    "LIBGL_DRIVERS_PATH",
    "GBM_BACKENDS_PATH",
];

pub(crate) fn app_command(program: impl AsRef<OsStr>) -> Command {
    // A component restart stops its whole cgroup; apps get their own scope under app.slice.
    if std::env::var_os("INVOCATION_ID").is_none() {
        return Command::new(program);
    }
    let mut command = Command::new("systemd-run");
    command
        .args([
            "--user",
            "--scope",
            "--slice=app.slice",
            "--collect",
            "--quiet",
            "--",
        ])
        .arg(program);
    command
}

pub(crate) fn detached(mut command: Command) -> io::Result<()> {
    // SAFETY: setsid, fork and _exit are async-signal-safe, as pre_exec requires.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            match libc::fork() {
                -1 => Err(io::Error::last_os_error()),
                0 => Ok(()),
                _ => libc::_exit(0),
            }
        });
    }
    command.spawn()?.wait()?;
    Ok(())
}

pub(crate) fn spawn_component(component: &str) {
    spawn_component_with(component, &[]);
}

pub(crate) fn spawn_component_with(component: &str, args: &[&str]) {
    let Some(display) = env::var_os("WAYLAND_DISPLAY") else {
        eprintln!("menubar: cannot spawn {component}: WAYLAND_DISPLAY not set");
        return;
    };
    let program =
        sibling_binary("malaysia-shell").unwrap_or_else(|| PathBuf::from("malaysia-shell"));
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
    restore_original_env(&mut command);
    spawn_logged(&log_name, &program, command);
}

pub(crate) fn spawn_app(binary: &str, args: &[&str]) {
    let Some(display) = env::var_os("WAYLAND_DISPLAY") else {
        eprintln!("menubar: cannot spawn {binary}: WAYLAND_DISPLAY not set");
        return;
    };

    let program = if binary == "malaysia-settings" {
        match resolve_settings_binary() {
            Some(path) => path,
            None => {
                tracing::warn!(
                    "menubar: malaysia-settings not found next to malaysia-shell or in PATH"
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
        app_command(&program)
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
        restore_original_env(&mut command);
    } else {
        sanitize_client_env(&mut command);
    }
    spawn_logged(binary, &program, command);
}

fn sibling_binary(name: &str) -> Option<PathBuf> {
    Some(env::current_exe().ok()?.parent()?.join(name))
}

fn resolve_settings_binary() -> Option<PathBuf> {
    if let Some(candidate) = sibling_binary("malaysia-settings").filter(|path| path.is_file()) {
        return Some(candidate);
    }
    let path = env::var("PATH").ok()?;
    path.split(':')
        .map(|dir| Path::new(dir).join("malaysia-settings"))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn xdg_open(path: impl AsRef<Path>) {
    let path = path.as_ref();
    let mut command = app_command("xdg-open");
    command.arg(path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    sanitize_client_env(&mut command);
    match detached(command) {
        Ok(()) => eprintln!("menubar: xdg-open {}", path.display()),
        Err(error) => eprintln!("menubar: could not open '{}': {error}", path.display()),
    }
}

pub(crate) fn sanitize_client_env(command: &mut Command) {
    for name in WRAPPER_ENV {
        command.env_remove(name);
    }
    restore_original_env(command);
}

fn is_kit_binary(binary: &str) -> bool {
    matches!(binary, "malaysia-shell" | "malaysia-settings")
}

fn restore_original_env(command: &mut Command) {
    for name in WRAPPER_ENV {
        let saved = format!("MALAYSIA_ORIG_{name}");
        if let Some(value) = env::var_os(&saved).filter(|value| !value.is_empty()) {
            command.env(name, value);
        }
        command.env_remove(saved);
    }
}

fn kit_log_path(name: &str) -> Option<PathBuf> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")?;
    let dir = PathBuf::from(runtime).join("malaysia");
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
    if let Err(error) = detached(command) {
        tracing::warn!(name, %error, "could not spawn kit binary");
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::process::Command;

    use super::{restore_original_env, sanitize_client_env};

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
            // SAFETY: `with_env` holds the test lock while the environment changes.
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
            // SAFETY: `with_env` holds the test lock while the environment changes.
            unsafe {
                env::set_var("LD_LIBRARY_PATH", "/dev/lib");
                env::remove_var("MALAYSIA_ORIG_LD_LIBRARY_PATH");
            }

            let mut command = Command::new("env");
            restore_original_env(&mut command);
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
            // SAFETY: `with_env` holds the test lock while the environment changes.
            unsafe {
                env::set_var("LD_LIBRARY_PATH", "/wrapper/lib");
                env::set_var("MALAYSIA_ORIG_LD_LIBRARY_PATH", "/orig/lib");
            }

            let mut command = Command::new("env");
            restore_original_env(&mut command);
            let output = command.output().expect("env should run");
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(
                text.contains("LD_LIBRARY_PATH=/orig/lib"),
                "kit env should restore the original path when it was saved: {text}"
            );
        });
    }
}
