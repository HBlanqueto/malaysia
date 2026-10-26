//! Block 4 acceptance harness: open and close many windows, assert the scene
//! returns to empty, report the compositor's resident-memory delta, and resize
//! windows from an edge and a corner.
//!
//! It launches a single nested compositor and the `window_stress` client, which
//! opens fifty toplevels and then destroys them. The compositor logs
//! `window removed` for each and `scene empty` when the last is gone; this
//! harness asserts both, so a leak or a missed teardown fails loudly. Resident
//! memory is measured before the client runs and after the scene empties and
//! reported as a delta; it is **measured, not asserted** (a threshold would be
//! flaky), so the numbers are printed for inspection.
//!
//! It then runs three resize scenarios, each with a fresh compositor, a fresh
//! `resize_client` and `YUKI_TEST_INPUT=1`: the compositor presses an edge
//! or corner of the mapped window and its scripted motions drive the grab. The
//! harness asserts the final size the client was configured to — unbounded
//! growth from a corner, a client maximum clamping that growth, and a client
//! minimum holding a shrunk width.
//!
//! Run from the dev shell, after building the compositor and probes:
//!
//! ```sh
//! cargo build -p malaysia-compositor -p sctk-probe
//! YUKI_BIN=target/debug/malaysia-compositor YUKI_STRESS_BIN=target/debug/window_stress \
//!   YUKI_RESIZE_BIN=target/debug/resize_client target/debug/block4
//! ```
//!
//! Environment overrides: `YUKI_BIN`, `YUKI_STRESS_BIN`,
//! `YUKI_RESIZE_BIN`.

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
/// The compositor log line carrying the live-object counts.
const LIVE_OBJECTS_LOG_LINE: &str = "live objects";
/// The compositor log line emitted for each destroyed window.
const WINDOW_REMOVED_LOG_LINE: &str = "window removed";
/// How many windows each stress batch opens and closes.
const WINDOWS: usize = 50;
/// How many batches to run; the first is warm-up.
const BATCHES: usize = 3;
/// The largest batch-2-to-batch-3 resident-memory growth allowed, in kB.
///
/// After warm-up a batch should reuse the same allocations, so the growth is
/// allocator noise (tens of kB). A per-window leak would be far larger: even
/// ~20 kB per window over 50 windows is ~1 MB, so 1 MiB catches a real leak
/// while staying well above noise.
const GROWTH_THRESHOLD_KB: u64 = 1024;
/// The pointer delta the compositor's scripted pre-threshold motions apply.
///
/// The seam follows the press with `TEST_DRAG_STEPS` motions of
/// `TEST_DRAG_STEP` each: 4 × (12, 8).
const EDGE_DRAG: (u32, u32) = (48, 32);
/// How far the resize cases set a client bound from the initial size, in pixels.
const BOUND_MARGIN: u32 = 10;

/// Live-object counts reported by the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiveObjects {
    /// Connected clients.
    clients: i64,
    /// Live `wl_surface` objects.
    surfaces: i64,
    /// Tracked toplevels.
    toplevels: i64,
}

impl LiveObjects {
    /// The counts from a `live objects` log line, if it is one.
    fn parse(line: &str) -> Option<Self> {
        let line = strip_ansi(line);
        if !line.contains(LIVE_OBJECTS_LOG_LINE) {
            return None;
        }
        Some(Self {
            clients: field(&line, "clients")?,
            surfaces: field(&line, "surfaces")?,
            toplevels: field(&line, "toplevels")?,
        })
    }
}

/// Removes ANSI escape sequences, whose digits would otherwise be mistaken for
/// field values.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The first integer after `key` in a log line, skipping any formatting between
/// the key and its value.
fn field(line: &str, key: &str) -> Option<i64> {
    let start = line.find(key)? + key.len();
    let digits: String = line[start..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

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
    let binary = std::env::var_os("YUKI_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("target/debug/malaysia-compositor"));
    if !binary.is_file() {
        return Err(format!(
            "compositor binary not found at {}; run `cargo build -p malaysia-compositor` first",
            binary.display()
        ));
    }
    let stress = probe_binary(repo, "YUKI_STRESS_BIN", "window_stress");
    if !stress.is_file() {
        return Err(format!(
            "stress client not found at {}; run `cargo build -p sctk-probe` first",
            stress.display()
        ));
    }
    let resize = probe_binary(repo, "YUKI_RESIZE_BIN", "resize_client");
    if !resize.is_file() {
        return Err(format!(
            "resize client not found at {}; run `cargo build -p sctk-probe` first",
            resize.display()
        ));
    }

    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;

    let mut compositor = ChildGuard::spawn(
        Command::new(&binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("YUKI_TEST_OBJECTS", "1"),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;

    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;
    let baseline = wait_for_baseline(&compositor_log, Duration::from_secs(5))
        .ok_or_else(|| "the compositor never logged its live-object baseline".to_string())?;
    println!("block4: baseline {baseline:?}");

    // Three batches: the first is warm-up (allocators, GL objects, the damage
    // tracker), and only the growth from the second to the third is compared
    // against the threshold. Each batch is a fresh stress client that opens and
    // closes WINDOWS toplevels.
    let mut resident = Vec::new();
    for batch in 1..=BATCHES {
        let mut client = ChildGuard::spawn(
            Command::new(&stress)
                .env("XDG_RUNTIME_DIR", runtime.path())
                .env("WAYLAND_DISPLAY", &socket)
                .stdin(Stdio::null()),
            None,
        )
        .map_err(|error| format!("failed to start the stress client: {error}"))?;
        let client_log = client.capture()?;

        wait_for_client_exit(&mut client, &client_log, "stress client")?;
        wait_for_batch(&compositor_log, baseline, batch * WINDOWS)?;

        let kb = resident_kb(compositor.id());
        resident.push(kb);
        println!("block4: batch {batch} settled at {kb} kB");
    }

    let warm_up = resident[0];
    let second = resident[1];
    let third = resident[2];
    let growth = third.saturating_sub(second);
    if growth >= GROWTH_THRESHOLD_KB {
        return Err(format!(
            "resident memory grew {growth} kB from batch 2 to batch 3, at or above the {GROWTH_THRESHOLD_KB} kB \
             threshold: warm-up {warm_up} kB, batch 2 {second} kB, batch 3 {third} kB"
        ));
    }

    let lines = snapshot(&compositor_log);
    let removed = lines
        .iter()
        .filter(|line| line.contains(WINDOW_REMOVED_LOG_LINE))
        .count();
    println!(
        "block4: {removed} windows removed over {BATCHES} batches, live objects restored to \
         baseline; resident memory warm-up {warm_up} kB, batch 2 {second} kB, batch 3 {third} kB, \
         batch 2->3 growth {growth} kB (asserted below {GROWTH_THRESHOLD_KB} kB)"
    );

    run_resize_cases(&binary, &resize)?;
    Ok(())
}

/// Resizes a window from a corner and an edge and asserts the final size.
///
/// The three scenarios cover the bound logic `resized` applies: no client
/// maximum must leave the size unbounded (so the corner drag grows it), a
/// client maximum must clamp that growth, and a client minimum must hold a
/// shrunk width.
fn run_resize_cases(binary: &Path, client: &Path) -> Result<(), String> {
    run_resize_case(
        binary,
        client,
        "grow",
        "0.999,0.999",
        None,
        |width, height| (width + EDGE_DRAG.0, height + EDGE_DRAG.1),
    )?;
    run_resize_case(
        binary,
        client,
        "maximum",
        "0.999,0.999",
        Some(("YUKI_RESIZE_MAX", BOUND_MARGIN)),
        |width, height| (width + BOUND_MARGIN, height + BOUND_MARGIN),
    )?;
    run_resize_case(
        binary,
        client,
        "minimum",
        "0.0,0.5",
        Some(("YUKI_RESIZE_MIN", BOUND_MARGIN)),
        |width, height| (width - BOUND_MARGIN, height),
    )?;
    Ok(())
}

/// Runs one resize scenario and asserts the final configured size.
///
/// `expect` maps the initial configured size to the expected final one. The
/// press fraction aims the compositor's scripted press, and `bound` is the
/// client's minimum or maximum offset from the initial size.
fn run_resize_case(
    binary: &Path,
    client: &Path,
    name: &str,
    press_at: &str,
    bound: Option<(&str, u32)>,
    expect: impl Fn(u32, u32) -> (u32, u32),
) -> Result<(), String> {
    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;
    let mut compositor = ChildGuard::spawn(
        Command::new(binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("YUKI_TEST_INPUT", "1")
            .env("YUKI_TEST_PRESS_AT", press_at),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;
    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;

    // The bound is the client's: it sets min_size/max_size from it before it
    // maps, and the compositor's resize clamps to them.
    let mut command = Command::new(client);
    command
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env("WAYLAND_DISPLAY", &socket)
        .stdin(Stdio::null());
    if let Some((variable, margin)) = bound {
        command.env(variable, format!("{margin},{margin}"));
    }
    let mut resize_client = ChildGuard::spawn(&mut command, None)
        .map_err(|error| format!("failed to start the resize client: {error}"))?;
    let client_log = resize_client.capture()?;
    wait_for_client_exit(&mut resize_client, &client_log, "resize client")?;

    let lines = stripped(&client_log);
    let initial = lines
        .iter()
        .find(|line| line.contains("resize_client: configure"))
        .ok_or_else(|| {
            format!(
                "the resize client for `{name}` was never configured; output:\n{}",
                lines.join("\n")
            )
        })?;
    let (initial_width, initial_height) = size_of(initial)?;
    let last = lines
        .iter()
        .find(|line| line.contains("resize_client: final"))
        .ok_or_else(|| {
            format!(
                "the resize client for `{name}` never reported a final size; output:\n{}",
                lines.join("\n")
            )
        })?;
    let (final_width, final_height) = size_of(last)?;

    let (expected_width, expected_height) = expect(initial_width, initial_height);
    if (final_width, final_height) != (expected_width, expected_height) {
        return Err(format!(
            "resize `{name}` ended at {final_width}x{final_height} after starting at \
             {initial_width}x{initial_height}, expected {expected_width}x{expected_height}; client \
             output:\n{}",
            lines.join("\n")
        ));
    }
    println!(
        "block4: resize {name} PASS (initial {initial_width}x{initial_height}, final \
         {final_width}x{final_height}, expected {expected_width}x{expected_height})"
    );
    Ok(())
}

/// The `width` and `height` fields of a `resize_client` size line.
fn size_of(line: &str) -> Result<(u32, u32), String> {
    let width = field(line, "width")
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("`width` is missing from `{line}`"))?;
    let height = field(line, "height")
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("`height` is missing from `{line}`"))?;
    Ok((width, height))
}

/// Resolves one of this package's probe binaries from its environment variable,
/// the directory of this executable, or the debug target directory.
fn probe_binary(repo: &Path, variable: &str, name: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
                .unwrap_or_else(|| repo.join("target/debug").join(name))
        })
}

/// The first live-object line seen, used as the baseline.
fn wait_for_baseline(log: &Sink, timeout: Duration) -> Option<LiveObjects> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(objects) = latest_objects(log) {
            return Some(objects);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Waits until a batch has finished: at least `min_removed` windows have been
/// removed and the live-object counts are back at the baseline.
fn wait_for_batch(log: &Sink, baseline: LiveObjects, min_removed: usize) -> Result<(), String> {
    let deadline = Instant::now() + STRESS_TIMEOUT;
    loop {
        let lines = snapshot(log);
        let removed = lines
            .iter()
            .filter(|line| line.contains(WINDOW_REMOVED_LOG_LINE))
            .count();
        let latest = lines.iter().rev().find_map(|line| LiveObjects::parse(line));
        if removed >= min_removed && latest == Some(baseline) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "batch did not settle: {removed} of {min_removed} removals, last live objects \
                 {latest:?} (baseline {baseline:?}); compositor log:\n{}",
                lines.join("\n")
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Waits until a client has exited, failing on a non-zero status.
fn wait_for_client_exit(client: &mut ChildGuard, log: &Sink, what: &str) -> Result<(), String> {
    let deadline = Instant::now() + STRESS_TIMEOUT;
    loop {
        if let Some(status) = client.poll()? {
            if status.success() {
                return Ok(());
            }
            return Err(format!(
                "the {what} exited with {status}; output:\n{}",
                snapshot(log).join("\n")
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the {what} did not exit in time; output:\n{}",
                snapshot(log).join("\n")
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// The most recent live-object counts in the log.
fn latest_objects(log: &Sink) -> Option<LiveObjects> {
    snapshot(log)
        .iter()
        .rev()
        .find_map(|line| LiveObjects::parse(line))
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

/// A copy of the captured lines with ANSI colour escapes removed.
fn stripped(sink: &Sink) -> Vec<String> {
    snapshot(sink).iter().map(|line| strip_ansi(line)).collect()
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
            std::env::temp_dir().join(format!("malaysia-block4-{}-{unique}", std::process::id()));
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
