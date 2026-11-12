use std::path::Path;
use std::process::Stdio;

use tracing::{error, info, warn};

pub(crate) fn start(socket: &str) -> bool {
    if let Some(config_home) = ctappearance::xdg::config_home()
        && remove_stale_unit_links(&config_home.join("systemd/user"), Path::new(SYSTEM_UNITS))
    {
        match systemctl(&["--user", "daemon-reload"], None) {
            Ok(output) if output.status.success() => {}
            Ok(output) => warn!(
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "could not reload the user manager"
            ),
            Err(error) => warn!(%error, "could not reload the user manager"),
        }
    }

    let import = systemctl(
        &[
            "--user",
            "import-environment",
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_ID",
            "MALAYSIA_BIN_DIR",
            "FREETYPE_PROPERTIES",
        ],
        Some(socket),
    );
    if let Ok(output) = &import
        && !output.status.success()
    {
        error!(
            stderr = %String::from_utf8_lossy(&output.stderr).trim(),
            "could not import WAYLAND_DISPLAY into the user manager"
        );
    }

    match systemctl(
        &["--user", "start", "malaysia-session.target"],
        Some(socket),
    ) {
        Ok(output) if output.status.success() => {
            info!("malaysia-session.target started");
            true
        }
        Ok(output) => {
            error!(
                status = %output.status,
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "{}",
                missing_target_help()
            );
            false
        }
        Err(error) => {
            error!(%error, "{}", missing_target_help());
            false
        }
    }
}

const SYSTEM_UNITS: &str = "/etc/systemd/user";

// ~/.config/systemd/user outranks /etc/systemd/user: a link there to another package pins an old shell.
fn remove_stale_unit_links(user_units: &Path, system_units: &Path) -> bool {
    if !system_units.join("malaysia-session.target").exists() {
        return false;
    }
    let mut removed = false;
    for dir in [
        user_units.to_path_buf(),
        user_units.join("malaysia-session.target.wants"),
    ] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if !is_stale_unit_link(&path, system_units) {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    info!(path = %path.display(), "removed a stale Malaysia unit link");
                    removed = true;
                }
                Err(error) => {
                    warn!(%error, path = %path.display(), "could not remove a stale unit link")
                }
            }
        }
    }
    removed
}

fn is_stale_unit_link(path: &Path, system_units: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let ours = name.starts_with("malaysia-") || name == "xdg-desktop-portal-malaysia.service";
    let link = path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink());
    if !ours || !link {
        return false;
    }
    let Ok(target) = std::fs::canonicalize(path) else {
        return true;
    };
    let system_target = std::fs::canonicalize(system_units.join(unit_template(name))).ok();
    target.starts_with("/nix/store") && Some(target) != system_target
}

fn unit_template(name: &str) -> String {
    match (name.split_once('@'), name.rsplit_once('.')) {
        (Some((prefix, _)), Some((_, kind))) => format!("{prefix}@.{kind}"),
        _ => name.to_string(),
    }
}

pub(crate) fn stop() {
    let _ = systemctl(&["--user", "stop", "malaysia-session.target"], None);
}

fn systemctl(args: &[&str], socket: Option<&str>) -> std::io::Result<std::process::Output> {
    let mut command = crate::spawn::command("systemctl", &crate::config::Config::default());
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(socket) = socket {
        command.env("WAYLAND_DISPLAY", socket);
        command.env("XDG_CURRENT_DESKTOP", "Malaysia");
        // The shell units start from MALAYSIA_BIN_DIR: Infinity trusts only the shell next to this binary.
        if let Some(dir) = crate::spawn::bin_dir() {
            command.env("MALAYSIA_BIN_DIR", dir);
        }
    }
    crate::spawn::spawn(command, &crate::config::Config::default())?.wait_with_output()
}

fn missing_target_help() -> &'static str {
    "malaysia-session.target is not installed in the user manager; the session runs without a shell. \
     Install it with the NixOS module (programs.malaysia.enable)."
}

#[cfg(test)]
mod tests {
    use super::{remove_stale_unit_links, unit_template};
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("malaysia-units-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("user/malaysia-session.target.wants")).unwrap();
        std::fs::create_dir_all(dir.join("system")).unwrap();
        dir
    }

    #[test]
    fn an_instance_resolves_to_its_template() {
        assert_eq!(
            unit_template("malaysia-shell@menubar.service"),
            "malaysia-shell@.service"
        );
        assert_eq!(
            unit_template("malaysia-session.target"),
            "malaysia-session.target"
        );
    }

    #[test]
    fn only_broken_links_with_malaysia_names_are_removed() {
        let dir = scratch("stale");
        let user = dir.join("user");
        std::fs::write(dir.join("system/malaysia-session.target"), "").unwrap();
        symlink(
            "/missing/result/malaysia-shell@.service",
            user.join("malaysia-shell@.service"),
        )
        .unwrap();
        symlink(
            "/missing/result/malaysia-shell@.service",
            user.join("malaysia-session.target.wants/malaysia-shell@dock.service"),
        )
        .unwrap();
        symlink("/dev/null", user.join("malaysia-wallpaper.service")).unwrap();
        symlink("/missing/other.service", user.join("other.service")).unwrap();
        std::fs::write(user.join("malaysia-session.target"), "").unwrap();

        assert!(remove_stale_unit_links(&user, &dir.join("system")));
        assert!(
            user.join("malaysia-shell@.service")
                .symlink_metadata()
                .is_err()
        );
        assert!(
            user.join("malaysia-session.target.wants/malaysia-shell@dock.service")
                .symlink_metadata()
                .is_err()
        );
        assert!(
            user.join("malaysia-wallpaper.service")
                .symlink_metadata()
                .is_ok()
        );
        assert!(user.join("other.service").symlink_metadata().is_ok());
        assert!(user.join("malaysia-session.target").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_removed_when_the_system_does_not_install_the_units() {
        let dir = scratch("no-system");
        let user = dir.join("user");
        symlink(
            "/missing/result/malaysia-shell@.service",
            user.join("malaysia-shell@.service"),
        )
        .unwrap();

        assert!(!remove_stale_unit_links(&user, &dir.join("system")));
        assert!(
            user.join("malaysia-shell@.service")
                .symlink_metadata()
                .is_ok()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
