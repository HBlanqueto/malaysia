use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, BorrowedFd, RawFd};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{SocketAddr, UnixListener};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use tracing::{error, info, warn};

use crate::config::Config;

const SOCKET_DIR: &str = "/tmp/.X11-unix";
const SATELLITE: &str = "xwayland-satellite";
const DISPLAYS: std::ops::Range<u32> = 0..64;
const MIN_UPTIME: Duration = Duration::from_secs(5);

static DISPLAY: OnceLock<String> = OnceLock::new();

pub fn display() -> Option<&'static str> {
    DISPLAY.get().map(String::as_str)
}

pub struct Xwayland {
    lock: PathBuf,
    socket: PathBuf,
}

impl Drop for Xwayland {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket);
        let _ = fs::remove_file(&self.lock);
    }
}

pub fn start(wayland_display: &str) -> Option<Xwayland> {
    match try_start(wayland_display) {
        Ok(xwayland) => Some(xwayland),
        Err(error) => {
            warn!(%error, "X11 apps unavailable");
            None
        }
    }
}

fn try_start(wayland_display: &str) -> io::Result<Xwayland> {
    ensure_socket_dir()?;
    let (number, lock) = claim_display()?;
    let socket = PathBuf::from(format!("{SOCKET_DIR}/X{number}"));
    let _ = fs::remove_file(&socket);
    let xwayland = Xwayland {
        lock,
        socket: socket.clone(),
    };
    let abstract_name = SocketAddr::from_abstract_name(socket.as_os_str().as_bytes())?;
    let listeners = [
        UnixListener::bind(&socket)?,
        UnixListener::bind_addr(&abstract_name)?,
    ];
    let x11_display = format!(":{number}");
    let child = spawn_satellite(&x11_display, wayland_display, &listeners)?;
    info!(display = %x11_display, "X11 display ready");
    let _ = DISPLAY.set(x11_display.clone());
    let wayland_display = wayland_display.to_owned();
    std::thread::Builder::new()
        .name("xwayland".into())
        .spawn(move || supervise(child, &x11_display, &wayland_display, &listeners))?;
    Ok(xwayland)
}

// X11 clients trust whatever listens in this directory; refuse one another user can hijack.
fn ensure_socket_dir() -> io::Result<()> {
    match fs::create_dir(SOCKET_DIR) {
        Ok(()) => fs::set_permissions(SOCKET_DIR, fs::Permissions::from_mode(0o1777)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let owner = fs::metadata(SOCKET_DIR)?.uid();
            if owner == 0 || owner == nix::unistd::getuid().as_raw() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "{SOCKET_DIR} belongs to uid {owner}"
                )))
            }
        }
        Err(error) => Err(error),
    }
}

fn claim_display() -> io::Result<(u32, PathBuf)> {
    for number in DISPLAYS {
        let lock = PathBuf::from(format!("/tmp/.X{number}-lock"));
        if is_stale(&lock) {
            let _ = fs::remove_file(&lock);
        }
        match OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(mut file) => {
                writeln!(file, "{:>10}", std::process::id())?;
                return Ok((number, lock));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("no free X11 display"))
}

fn is_stale(lock: &Path) -> bool {
    fs::read_to_string(lock)
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok())
        .is_some_and(|pid| !Path::new(&format!("/proc/{pid}")).exists())
}

#[allow(unsafe_code)]
fn spawn_satellite(
    display: &str,
    wayland_display: &str,
    listeners: &[UnixListener],
) -> io::Result<Child> {
    let fds: Vec<RawFd> = listeners.iter().map(AsRawFd::as_raw_fd).collect();
    let mut command = crate::spawn::command(SATELLITE, &Config::default());
    command
        .arg(display)
        .env("WAYLAND_DISPLAY", wayland_display)
        .env_remove("DISPLAY");
    for fd in &fds {
        command.arg("-listenfd").arg(fd.to_string());
    }
    // SAFETY: the hook only calls fcntl, which is async-signal-safe, on fds that stay open in the parent.
    unsafe {
        command.pre_exec(move || {
            for &fd in &fds {
                fcntl(
                    BorrowedFd::borrow_raw(fd),
                    FcntlArg::F_SETFD(FdFlag::empty()),
                )?;
            }
            Ok(())
        });
    }
    crate::spawn::spawn(command, &Config::default())
}

fn supervise(mut child: Child, display: &str, wayland_display: &str, listeners: &[UnixListener]) {
    loop {
        let started = Instant::now();
        let status = child.wait();
        if started.elapsed() < MIN_UPTIME {
            error!(
                ?status,
                "xwayland-satellite keeps exiting; X11 apps unavailable"
            );
            return;
        }
        warn!(?status, "xwayland-satellite exited; restarting it");
        child = match spawn_satellite(display, wayland_display, listeners) {
            Ok(child) => child,
            Err(error) => {
                error!(%error, "xwayland-satellite could not restart");
                return;
            }
        };
    }
}
