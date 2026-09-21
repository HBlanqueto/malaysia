//! Block 4 acceptance harness for the client-initiated move and resize path.
//!
//! It launches a compositor with `NEPTUNE_TEST_INPUT=1` (so the compositor
//! injects the button presses a probe cannot make itself) and runs
//! `move_client`, which sends `xdg_toplevel.move`/`resize` with valid serials
//! and again with a fabricated serial. The compositor reports each accept and
//! reject; this harness asserts all four, and the client asserts it saw the
//! resizing configure. Both must pass.
//!
//! Run from the dev shell, or `./dev.sh move` which builds and runs it.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long to wait for the compositor to create its socket.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(15);
/// How long to wait for the client, which has its own shorter timeout.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(20);

/// The compositor lines this harness requires, in the order it requires them.
const ACCEPTED_MOVE: &str = "accepted move request";
const ACCEPTED_RESIZE: &str = "accepted resize request";
const REJECTED_MOVE: &str = "rejected move request for a stale serial";
const REJECTED_RESIZE: &str = "rejected resize request for a stale serial";

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("move_probe: PASS");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("move_probe: FAIL: {message}");
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
    let client = std::env::var_os("NEPTUNE_MOVE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("move_client")))
                .unwrap_or_else(|| repo.join("target/debug/move_client"))
        });
    if !client.is_file() {
        return Err(format!(
            "move client not found at {}; run `cargo build -p sctk-probe` first",
            client.display()
        ));
    }

    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;

    let mut compositor = ChildGuard::spawn(
        Command::new(&binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("NEPTUNE_TEST_INPUT", "1"),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;

    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;

    let mut move_client = ChildGuard::spawn(
        Command::new(&client)
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WAYLAND_DISPLAY", &socket)
            .stdin(Stdio::null()),
        None,
    )
    .map_err(|error| format!("failed to start the move client: {error}"))?;
    let client_log = move_client.capture()?;

    wait_for_client(&mut move_client, &client_log)?;

    let lines = snapshot(&compositor_log);
    for required in [
        ACCEPTED_MOVE,
        ACCEPTED_RESIZE,
        REJECTED_MOVE,
        REJECTED_RESIZE,
    ] {
        if !lines.iter().any(|line| line.contains(required)) {
            return Err(format!(
                "the compositor never reported `{required}`; compositor log:\n{}",
                lines.join("\n")
            ));
        }
    }

    for required in [
        "sent move with press serial",
        "sent resize with press serial",
        "fabricated serial",
        "resizing state",
    ] {
        if !snapshot(&client_log)
            .iter()
            .any(|line| line.contains(required))
        {
            return Err(format!(
                "the move client never reported `{required}`; client output:\n{}",
                snapshot(&client_log).join("\n")
            ));
        }
    }

    for line in snapshot(&client_log)
        .iter()
        .filter(|line| line.contains("move_client:"))
    {
        println!("{line}");
    }
    println!(
        "move_probe: compositor accepted move and resize, rejected the fabricated serials; client \
         saw the resizing configure"
    );
    Ok(())
}

/// Waits until the move client exits, failing on a non-zero status.
fn wait_for_client(client: &mut ChildGuard, log: &Sink) -> Result<(), String> {
    let deadline = Instant::now() + CLIENT_TIMEOUT;
    loop {
        if let Some(status) = client.poll()? {
            if status.success() {
                return Ok(());
            }
            return Err(format!(
                "the move client exited with {status}; output:\n{}",
                snapshot(log).join("\n")
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the move client did not exit in time; output:\n{}",
                snapshot(log).join("\n")
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
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
            std::env::temp_dir().join(format!("neptune-move-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path)
            .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
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
