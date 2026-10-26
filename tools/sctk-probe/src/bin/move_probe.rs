//! Acceptance harness for the client-initiated move and resize path.
//!
//! It runs three cases, each with a fresh compositor and `YUKI_TEST_INPUT=1`
//! (so the compositor injects the button press, motions and release a probe
//! cannot make itself):
//!
//! 1. `move_client` repeats a GTK-shaped gesture: press, motions past a
//!    threshold, a second window that takes focus, then `xdg_toplevel.move`
//!    and `resize` with the original press serial while the button is held.
//!    The harness asserts both were accepted, that the window moved by the
//!    injected pointer delta, and that the released and fabricated serials
//!    were rejected.
//! 2. `hover_client` asserts the surface-local coordinates of a buttonless
//!    hover: after the scripted drag moves the window to a non-zero origin and
//!    the release, the compositor hovers the pointer and the client must read
//!    each point at the position it holds over the surface.
//! 3. Nautilus, a real GTK4 + libadwaita client, gets a scripted headerbar
//!    drag. The harness asserts its window moved by the injected delta. If
//!    Nautilus is not installed, or never maps a window, the case is reported
//!    UNVERIFIED rather than failed.
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
/// How long to wait for the move client, which has its own shorter timeout.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(20);
/// How long Nautilus gets to start, map and be dragged.
const NAUTILUS_TIMEOUT: Duration = Duration::from_secs(12);
/// The pointer delta the scripted drag applies, matching `YUKI_TEST_DRAG`.
const DRAG: (f64, f64) = (60.0, 40.0);
/// The hover points, relative to the dragged window's top-left.
///
/// The first is outside the window (the client gets a leave, so the next point
/// enters); the other two are inside it, so the client sees an enter and then a
/// motion. The probe reads the workspace centre from a real window and places
/// the hovered window there instead of assuming (0, 0).
const HOVER: [(f64, f64); 3] = [(-10.0, -10.0), (32.0, 32.0), (40.0, 40.0)];
/// The hover client's surface side, matching `hover_client`'s `SIDE`.
///
/// Its window is client-decorated, so the outer rectangle is exactly this; the
/// move client is server-decorated and its outer is taller, which is why the
/// probe cannot reuse its origin directly.
const HOVER_SIDE: f64 = 64.0;

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
    let binary = std::env::var_os("YUKI_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("target/debug/malaysia-compositor"));
    if !binary.is_file() {
        return Err(format!(
            "compositor binary not found at {}; run `cargo build -p malaysia-compositor` first",
            binary.display()
        ));
    }
    let client = probe_binary(repo, "YUKI_MOVE_BIN", "move_client");
    if !client.is_file() {
        return Err(format!(
            "move client not found at {}; run `cargo build -p sctk-probe` first",
            client.display()
        ));
    }

    let centre = run_move_case(&binary, &client)?;
    println!("move_probe: the GTK-shaped sequence was accepted, the window moved by the delta");
    run_hover_case(&binary, repo, centre)?;
    run_nautilus_case(&binary)?;
    Ok(())
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

/// The GTK-shaped sequence through `move_client`.
///
/// Returns the centre of the workspace, read from the window's real outer
/// rectangle before the drag. The hover case centres its own (client-decorated)
/// window there instead of guessing (0, 0).
fn run_move_case(binary: &Path, client: &Path) -> Result<(f64, f64), String> {
    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;
    let mut compositor = ChildGuard::spawn(
        Command::new(binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("YUKI_TEST_INPUT", "1")
            .env("YUKI_TEST_DRAG", format!("{},{}", DRAG.0, DRAG.1)),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;
    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;

    let mut move_client = ChildGuard::spawn(
        Command::new(client)
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WAYLAND_DISPLAY", &socket)
            .stdin(Stdio::null()),
        None,
    )
    .map_err(|error| format!("failed to start the move client: {error}"))?;
    let client_log = move_client.capture()?;
    wait_for_client(&mut move_client, &client_log, "move client")?;

    let compositor_lines = stripped(&compositor_log);
    let client_lines = stripped(&client_log);

    for required in ["accepted move request", "accepted resize request"] {
        if !compositor_lines.iter().any(|line| line.contains(required)) {
            return Err(format!(
                "the compositor never reported `{required}`; compositor log:\n{}",
                compositor_lines.join("\n")
            ));
        }
    }
    for required in [
        "got the press serial",
        "mapped a second window",
        "after a threshold and a focus change, while held",
        "after the release",
        "(fabricated)",
        "resizing state",
    ] {
        if !client_lines.iter().any(|line| line.contains(required)) {
            return Err(format!(
                "the move client never reported `{required}`; client output:\n{}",
                client_lines.join("\n")
            ));
        }
    }

    let rejected_move = compositor_lines
        .iter()
        .filter(|line| line.contains("rejected move request"))
        .count();
    let rejected_resize = compositor_lines
        .iter()
        .filter(|line| line.contains("rejected resize request"))
        .count();
    if rejected_move < 2 || rejected_resize < 2 {
        return Err(format!(
            "expected the released and fabricated serials to be rejected for both requests, saw \
             {rejected_move} move and {rejected_resize} resize rejections; compositor log:\n{}",
            compositor_lines.join("\n")
        ));
    }

    let released = client_lines
        .iter()
        .find(|line| line.contains("after the release"))
        .and_then(|line| number_after(line, "with serial "))
        .ok_or_else(|| "the move client never logged the released serial".to_string())?;
    for (kind, serial) in [
        ("move", released),
        ("resize", released),
        ("move", 0xdead_0001),
        ("resize", 0xdead_0001),
    ] {
        let wanted = format!("rejected {kind} request");
        let serial = format!("Serial({serial})");
        if !compositor_lines
            .iter()
            .any(|line| line.contains(&wanted) && line.contains(&serial))
        {
            return Err(format!(
                "the compositor never rejected a {kind} with {serial}; compositor log:\n{}",
                compositor_lines.join("\n")
            ));
        }
    }

    let drag = compositor_lines
        .iter()
        .find(|line| line.contains("test drag applied"))
        .ok_or_else(|| "the compositor never applied the scripted drag".to_string())?;
    assert_delta(drag, "from_x", "to_x", DRAG.0)?;
    assert_delta(drag, "from_y", "to_y", DRAG.1)?;
    let field_or_err =
        |name: &str| field(drag, name).ok_or_else(|| format!("`{name}` is missing from `{drag}`"));
    let centre = (
        field_or_err("from_x")? + field_or_err("from_width")? / 2.0,
        field_or_err("from_y")? + field_or_err("from_height")? / 2.0,
    );

    for line in client_lines
        .iter()
        .filter(|line| line.contains("move_client:"))
    {
        println!("{line}");
    }
    println!("move_probe: compositor accepted move and resize after a focus change, rejected the released and fabricated serials, and moved the window by the pointer delta");
    Ok(centre)
}

/// The buttonless hover through `hover_client`, asserting where it arrived.
///
/// `centre` is the workspace centre read from the move case. The hover window
/// is `HOVER_SIDE` square and centred there, and the scripted drag moves it by
/// `DRAG`, so the hover points are that final origin plus [`HOVER`]'s offsets.
fn run_hover_case(binary: &Path, repo: &Path, centre: (f64, f64)) -> Result<(), String> {
    let client = probe_binary(repo, "YUKI_HOVER_BIN", "hover_client");
    if !client.is_file() {
        return Err(format!(
            "hover client not found at {}; run `cargo build -p sctk-probe` first",
            client.display()
        ));
    }
    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;
    let dragged = (
        centre.0 - HOVER_SIDE / 2.0 + DRAG.0,
        centre.1 - HOVER_SIDE / 2.0 + DRAG.1,
    );
    let hover = points(HOVER.map(|(dx, dy)| (dragged.0 + dx, dragged.1 + dy)));
    let mut compositor = ChildGuard::spawn(
        Command::new(binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("YUKI_TEST_INPUT", "1")
            .env("YUKI_TEST_DRAG", format!("{},{}", DRAG.0, DRAG.1))
            .env("YUKI_TEST_HOVER", &hover),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;
    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;

    // The client is client-decorated, so a surface-local position is the global
    // point minus the window origin: exactly the relative offsets.
    let expect = points([HOVER[1], HOVER[2]]);
    let mut hover_client = ChildGuard::spawn(
        Command::new(&client)
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WAYLAND_DISPLAY", &socket)
            .env("YUKI_EXPECT_HOVER", &expect)
            .stdin(Stdio::null()),
        None,
    )
    .map_err(|error| format!("failed to start the hover client: {error}"))?;
    let client_log = hover_client.capture()?;
    wait_for_client(&mut hover_client, &client_log, "hover client")?;

    let client_lines = stripped(&client_log);
    let enter = client_lines
        .iter()
        .find(|line| line.contains("hover_client: enter"))
        .ok_or_else(|| {
            format!(
                "the hover client never reported an enter; output:\n{}",
                client_lines.join("\n")
            )
        })?;
    assert_position(enter, HOVER[1].0, HOVER[1].1)?;
    let motion = client_lines
        .iter()
        .find(|line| line.contains("hover_client: motion"))
        .ok_or_else(|| {
            format!(
                "the hover client never reported a motion; output:\n{}",
                client_lines.join("\n")
            )
        })?;
    assert_position(motion, HOVER[2].0, HOVER[2].1)?;

    for line in client_lines
        .iter()
        .filter(|line| line.contains("hover_client:"))
    {
        println!("{line}");
    }
    println!("move_probe: the hovered client read the pointer at the surface-local positions");
    Ok(())
}

/// Renders points as the `x,y` list separated by `;` the probes parse.
fn points(list: impl IntoIterator<Item = (f64, f64)>) -> String {
    list.into_iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// Asserts that a `hover_client` line reports a position.
fn assert_position(line: &str, expected_x: f64, expected_y: f64) -> Result<(), String> {
    for (axis, expected) in [("x", expected_x), ("y", expected_y)] {
        let got = field(line, axis).ok_or_else(|| format!("`{axis}` is missing from `{line}`"))?;
        if (got - expected).abs() > f64::EPSILON {
            return Err(format!(
                "the client read {axis}={got}, expected {expected}; line: `{line}`"
            ));
        }
    }
    Ok(())
}

/// A scripted headerbar drag on a real Nautilus window.
fn run_nautilus_case(binary: &Path) -> Result<(), String> {
    let Some(nautilus) = resolve("YUKI_APP_NAUTILUS", "nautilus") else {
        println!("move_probe: nautilus UNVERIFIED (not installed)");
        return Ok(());
    };
    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;
    let mut compositor = ChildGuard::spawn(
        Command::new(binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("YUKI_TEST_INPUT", "1")
            // The headerbar is a few percent below the window's top edge.
            .env("YUKI_TEST_PRESS_AT", "0.5,0.05")
            .env("YUKI_TEST_DRAG", format!("{},{}", DRAG.0, DRAG.1)),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;
    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;

    let mut app = ChildGuard::spawn(
        Command::new(&nautilus)
            .arg("--no-desktop")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WAYLAND_DISPLAY", &socket)
            .env("WAYLAND_DEBUG", "1")
            .env("GDK_BACKEND", "wayland")
            .env_remove("DISPLAY")
            .stdin(Stdio::null()),
        None,
    )
    .map_err(|error| format!("failed to start nautilus: {error}"))?;
    let _app_log = app.capture()?;

    let deadline = Instant::now() + NAUTILUS_TIMEOUT;
    let drag = loop {
        let lines = stripped(&compositor_log);
        if let Some(drag) = lines
            .iter()
            .find(|line| line.contains("test drag applied"))
            .cloned()
        {
            break drag;
        }
        if Instant::now() >= deadline {
            if lines.iter().any(|line| line.contains("window mapped")) {
                return Err(format!(
                    "nautilus mapped a window but its headerbar drag never moved it; compositor \
                     log:\n{}",
                    lines.join("\n")
                ));
            }
            println!("move_probe: nautilus UNVERIFIED (it never mapped a window)");
            return Ok(());
        }
        if let Some(status) = compositor.poll()? {
            return Err(format!(
                "the compositor exited ({status}) during the nautilus drag; log:\n{}",
                lines.join("\n")
            ));
        }
        thread::sleep(Duration::from_millis(100));
    };

    assert_delta(&drag, "from_x", "to_x", DRAG.0)?;
    assert_delta(&drag, "from_y", "to_y", DRAG.1)?;
    println!(
        "move_probe: nautilus PASS (its headerbar drag moved the window by the pointer delta)"
    );
    Ok(())
}

/// Waits until a probe client exits, failing on a non-zero status.
fn wait_for_client(client: &mut ChildGuard, log: &Sink, what: &str) -> Result<(), String> {
    let deadline = Instant::now() + CLIENT_TIMEOUT;
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

/// Asserts that `to - from` equals `expected` in a `test drag applied` line.
fn assert_delta(line: &str, from: &str, to: &str, expected: f64) -> Result<(), String> {
    let start = field(line, from).ok_or_else(|| format!("`{from}` is missing from `{line}`"))?;
    let end = field(line, to).ok_or_else(|| format!("`{to}` is missing from `{line}`"))?;
    let delta = end - start;
    if (delta - expected).abs() > f64::EPSILON {
        return Err(format!(
            "the scripted drag moved the window by {delta} on `{to}`, expected {expected}; line: \
             `{line}`"
        ));
    }
    Ok(())
}

/// Reads `name=<number>` from a tracing line.
fn field(line: &str, name: &str) -> Option<f64> {
    let rest = line.split_once(&format!("{name}="))?.1;
    rest.split_whitespace().next()?.parse().ok()
}

/// The first number after `marker` on a client line.
fn number_after(line: &str, marker: &str) -> Option<u32> {
    let rest = line.split_once(marker)?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// A copy of the captured lines with ANSI colour escapes removed.
fn stripped(sink: &Sink) -> Vec<String> {
    snapshot(sink).iter().map(|line| strip_ansi(line)).collect()
}

/// Removes ANSI escape sequences, so tracing's coloured fields can be parsed.
fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Resolves a client from its environment variable or `PATH`.
fn resolve(variable: &str, fallback: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(variable) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    which(fallback)
}

/// Looks a command up on `PATH`.
fn which(command: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
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
            std::env::temp_dir().join(format!("malaysia-move-{}-{unique}", std::process::id()));
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
