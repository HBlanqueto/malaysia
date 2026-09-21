//! Block 4 acceptance harness: open and close many windows, assert the scene
//! returns to empty, and report the compositor's resident-memory delta.
//!
//! It launches a single nested compositor and the `window_stress` client, which
//! opens fifty toplevels and then destroys them. The compositor logs
//! `window removed` for each and `scene empty` when the last is gone; this
//! harness asserts both, so a leak or a missed teardown fails loudly. Resident
//! memory is measured before the client runs and after the scene empties and
//! reported as a delta; it is **measured, not asserted** (a threshold would be
//! flaky), so the numbers are printed for inspection.
//!
//! Run from the dev shell, after building the compositor and probes:
//!
//! ```sh
//! cargo build -p neptune -p sctk-probe
//! NEPTUNE_BIN=target/debug/neptune NEPTUNE_STRESS_BIN=target/debug/window_stress \
//!   target/debug/block4
//! ```
//!
//! Environment overrides: `NEPTUNE_BIN`, `NEPTUNE_STRESS_BIN`.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long to wait for the compositor to create its socket.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(15);
/// How long to wait for the stress client to run and the scene to empty.
const STRESS_TIMEOUT: Duration = Duration::from_secs(30);
/// The compositor log line emitted when the last window is gone.
const SCENE_EMPTY_LOG_LINE: &str = "scene empty";
/// The compositor log line emitted for each destroyed window.
const WINDOW_REMOVED_LOG_LINE: &str = "window removed";
/// How many windows the stress client opens and closes.
const WINDOWS: usize = 50;

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("block4: PASS");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("block4: FAIL: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "cannot locate the repository root from the probe manifest".to_string())?;
    let binary = std::env::var_os("NEPTUNE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("target/debug/neptune"));
    if !binary.is_file() {
        return Err(format!(
            "compositor binary not found at {}; run `cargo build -p neptune` first",
            binary.display()
        ));
    }
    let stress = std::env::var_os("NEPTUNE_STRESS_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("window_stress")))
                .unwrap_or_else(|| repo.join("target/debug/window_stress"))
        });
    if !stress.is_file() {
        return Err(format!(
            "stress client not found at {}; run `cargo build -p sctk-probe` first",
            stress.display()
        ));
    }

    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;

    let mut compositor = ChildGuard::spawn(
        Command::new(&binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path()),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;

    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;
    let before_kb = resident_kb(compositor.id());

    let mut client = ChildGuard::spawn(
        Command::new(&stress)
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WAYLAND_DISPLAY", &socket)
            .stdin(Stdio::null()),
        None,
    )
    .map_err(|error| format!("failed to start the stress client: {error}"))?;
    let _client_log = client.capture()?;

    wait_for_scene_empty(&compositor_log, &mut client)?;

    let lines = snapshot(&compositor_log);
    let removed = lines
        .iter()
        .filter(|line| line.contains(WINDOW_REMOVED_LOG_LINE))
        .count();
    if removed < WINDOWS {
        return Err(format!(
            "the compositor reported only {removed} window removals; expected at least {WINDOWS}"
        ));
    }

    let after_kb = resident_kb(compositor.id());
    let delta = after_kb.saturating_sub(before_kb);
    println!(
        "block4: {removed} windows removed, scene empty; resident memory {before_kb} kB -> \
         {after_kb} kB (delta {delta} kB, measured, not asserted)"
    );
    Ok(())
}

/// Waits until the compositor reports an empty scene, or the client or deadline
/// fails the run.
fn wait_for_scene_empty(log: &Sink, client: &mut ChildGuard) -> Result<(), String> {
    let deadline = Instant::now() + STRESS_TIMEOUT;
    loop {
        if snapshot(log)
            .iter()
            .any(|line| line.contains(SCENE_EMPTY_LOG_LINE))
        {
            return Ok(());
        }
        if let Some(status) = client.poll()? {
            if !status.success() {
                return Err(format!("the stress client exited with {status}"));
            }
            // The client may exit before the compositor finishes tearing the
            // windows down; keep waiting for the log within the deadline.
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for an empty scene; compositor log:\n{}",
                snapshot(log).join("\n")
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// The compositor's resident set size in kB, from `/proc/<pid>/status`.
fn resident_kb(pid: u32) -> u64 {
    let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) else {
        return 0;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// Spawns the compositor with its own runtime directory, reaching the host
/// compositor through the absolute socket path resolved by the caller.
fn host_wayland_display() -> Result<Option<String>, String> {
    let Ok(display) = std::env::var("WAYLAND_DISPLAY") else {
        return Ok(None);
    };
    if display.starts_with('/') {
        return Ok(Some(display));
    }
    let runtime = std::env::var("XDG_RUNTIME_DIR")
        .map_err(|_| "WAYLAND_DISPLAY is relative but XDG_RUNTIME_DIR is not set".to_string())?;
    Ok(Some(format!("{runtime}/{display}")))
}

/// Waits until the compositor has created a `wayland-*` socket.
fn wait_for_socket(dir: &Path, compositor: &mut ChildGuard, log: &Sink) -> Result<String, String> {
    let deadline = Instant::now() + SOCKET_TIMEOUT;
    loop {
        if let Some(name) = find_socket(dir) {
            return Ok(name);
        }
        if let Some(status) = compositor.poll()? {
            return Err(format!(
                "compositor exited ({status}) before creating a socket; log:\n{}",
                snapshot(log).join("\n")
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for the Wayland socket in {}; log:\n{}",
                dir.display(),
                snapshot(log).join("\n")
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// The first (alphabetically) Wayland socket in a directory.
fn find_socket(dir: &Path) -> Option<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("wayland-") && !name.ends_with(".lock"))
        .collect();
    names.sort();
    names.into_iter().next()
}

/// A child process that is killed and reaped when it drops.
struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    fn spawn(command: &mut Command, wayland_display: Option<&str>) -> Result<Self, String> {
        if let Some(display) = wayland_display {
            command.env("WAYLAND_DISPLAY", display);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = command
            .spawn()
            .map_err(|error| format!("failed to start `{:?}`: {error}", command.get_program()))?;
        Ok(Self { child })
    }

    fn capture(&mut self) -> Result<Sink, String> {
        let stdout = self
            .child
            .stdout
            .take()
            .ok_or_else(|| "child stdout was not piped".to_string())?;
        let stderr = self
            .child
            .stderr
            .take()
            .ok_or_else(|| "child stderr was not piped".to_string())?;
        let sink = Arc::new(Mutex::new(Vec::new()));
        drain(stdout, Arc::clone(&sink));
        drain(stderr, Arc::clone(&sink));
        Ok(sink)
    }

    fn id(&self) -> u32 {
        self.child.id()
    }

    fn poll(&mut self) -> Result<Option<std::process::ExitStatus>, String> {
        self.child
            .try_wait()
            .map_err(|error| format!("failed to poll the child: {error}"))
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A shared, thread-safe collection of captured output lines.
type Sink = Arc<Mutex<Vec<String>>>;

/// Reads `reader` line by line into a sink on its own thread.
fn drain<R: Read + Send + 'static>(reader: R, sink: Sink) {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => sink.lock().expect("capture sink poisoned").push(line),
                Err(_) => break,
            }
        }
    });
}

/// A copy of the lines captured so far.
fn snapshot(sink: &Sink) -> Vec<String> {
    sink.lock().expect("capture sink poisoned").clone()
}

/// A runtime directory that is removed when it drops.
struct TempDir(PathBuf);

impl TempDir {
    fn create() -> Result<Self, String> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("clock is before the epoch: {error}"))?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("neptune-block4-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path)
            .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(|error| {
                format!("failed to set permissions on {}: {error}", path.display())
            })?;
        }
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
