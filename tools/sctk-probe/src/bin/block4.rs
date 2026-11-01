//! Harness: opens and closes many windows, checks memory and edge/corner resizes.
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SOCKET_TIMEOUT: Duration = Duration::from_secs(15);
const STRESS_TIMEOUT: Duration = Duration::from_secs(30);
const LIVE_OBJECTS_LOG_LINE: &str = "live objects";
const WINDOW_REMOVED_LOG_LINE: &str = "window removed";
const WINDOWS: usize = 50;
const BATCHES: usize = 3;
const GROWTH_THRESHOLD_KB: u64 = 1024;
const EDGE_DRAG: (u32, u32) = (48, 32);
const BOUND_MARGIN: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiveObjects {
    clients: i64,
    surfaces: i64,
    toplevels: i64,
}

impl LiveObjects {
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
    let binary = std::env::var_os("MALAYSIA_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("target/debug/malaysia-compositor"));
    if !binary.is_file() {
        return Err(format!(
            "compositor binary not found at {}; run `cargo build -p malaysia-compositor` first",
            binary.display()
        ));
    }
    let stress = probe_binary(repo, "MALAYSIA_STRESS_BIN", "window_stress");
    if !stress.is_file() {
        return Err(format!(
            "stress client not found at {}; run `cargo build -p sctk-probe` first",
            stress.display()
        ));
    }
    let resize = probe_binary(repo, "MALAYSIA_RESIZE_BIN", "resize_client");
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
            .env("MALAYSIA_TEST_OBJECTS", "1"),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;

    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;
    let baseline = wait_for_baseline(&compositor_log, Duration::from_secs(5))
        .ok_or_else(|| "the compositor never logged its live-object baseline".to_string())?;
    println!("block4: baseline {baseline:?}");

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
        Some(("MALAYSIA_RESIZE_MAX", BOUND_MARGIN)),
        |width, height| (width + BOUND_MARGIN, height + BOUND_MARGIN),
    )?;
    run_resize_case(
        binary,
        client,
        "minimum",
        "0.0,0.5",
        Some(("MALAYSIA_RESIZE_MIN", BOUND_MARGIN)),
        |width, height| (width - BOUND_MARGIN, height),
    )?;
    Ok(())
}

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
            .env("MALAYSIA_TEST_INPUT", "1")
            .env("MALAYSIA_TEST_PRESS_AT", press_at),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;
    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;

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

fn size_of(line: &str) -> Result<(u32, u32), String> {
    let width = field(line, "width")
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("`width` is missing from `{line}`"))?;
    let height = field(line, "height")
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("`height` is missing from `{line}`"))?;
    Ok((width, height))
}

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

fn latest_objects(log: &Sink) -> Option<LiveObjects> {
    snapshot(log)
        .iter()
        .rev()
        .find_map(|line| LiveObjects::parse(line))
}

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

type Sink = Arc<Mutex<Vec<String>>>;

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

fn snapshot(sink: &Sink) -> Vec<String> {
    sink.lock().expect("capture sink poisoned").clone()
}

fn stripped(sink: &Sink) -> Vec<String> {
    snapshot(sink).iter().map(|line| strip_ansi(line)).collect()
}

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
