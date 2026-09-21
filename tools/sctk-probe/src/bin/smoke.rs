//! Assertion-based smoke test for the toplevel mapping sequence.
//!
//! This is not part of the compositor. It launches a real nested session,
//! starts a real Wayland client against it with `WAYLAND_DEBUG=1`, captures the
//! client's protocol trace, and checks the sequence a mapped toplevel must
//! produce. It exits non-zero if any step is missing or out of order, so it can
//! be used as a pass/fail check instead of reading a debug log by eye.
//!
//! The sequence asserted, in order:
//!
//! 1. `xdg_toplevel.configure` precedes the first non-null `wl_surface.attach`.
//! 2. The `xdg_surface.ack_configure` serial matches the configure's serial, and
//!    the ack precedes the attach (the client must not attach before acking).
//! 3. A commit carrying a non-null buffer occurs, and the compositor reports the
//!    mapping.
//! 4. `wl_keyboard.enter` is delivered to that mapped surface.
//! 5. The `wl_callback.done` matching the specific id requested by
//!    `wl_surface.frame` arrives. A bare `wl_callback.done` is not accepted: the
//!    callback id is resolved from the frame request, and completions are
//!    matched against it, so a `wl_display.sync` round-trip cannot be mistaken
//!    for a frame.
//!
//! Run from the dev shell, after building the compositor:
//!
//! ```sh
//! cargo build -p neptune
//! cargo run -p sctk-probe --bin smoke
//! ```
//!
//! Environment overrides: `NEPTUNE_BIN` (path to the compositor binary) and
//! `NEPTUNE_SMOKE_CLIENT` (the client to drive; defaults to `foot`).

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long to wait for the compositor to create its socket.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(15);
/// How long to wait for the client to complete the mapping sequence.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(20);

/// The compositor log line emitted exactly when a window becomes mapped.
const MAPPED_LOG_LINE: &str = "window mapped";
/// The compositor log line emitted when it draws its own pointer.
///
/// There is no client cursor until the host pointer moves and the client
/// receives `wl_pointer.enter`, which this harness cannot cause, so the
/// compositor-drawn pointer is the cursor path it can exercise live. The line
/// is a `debug` event, so the compositor is run at `--log-level=debug`.
const DEFAULT_CURSOR_LOG_LINE: &str = "drawing a pointer cursor";

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("smoke: PASS");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("smoke: FAIL: {message}");
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
    let client = std::env::var("NEPTUNE_SMOKE_CLIENT").unwrap_or_else(|_| "foot".to_string());

    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;

    let mut compositor = ChildGuard::spawn(
        Command::new(&binary)
            .arg("--log-level=debug")
            .env("XDG_RUNTIME_DIR", runtime.path()),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;

    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;
    println!("smoke: compositor is listening on {socket}");

    let mut real_client = ChildGuard::spawn(
        Command::new(&client)
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WAYLAND_DISPLAY", &socket)
            .env("WAYLAND_DEBUG", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null()),
        None,
    )
    .map_err(|error| format!("failed to start client `{client}`: {error}"))?;
    let trace = real_client.capture()?;

    wait_for_sequence(&trace, &mut real_client)?;

    let lines = snapshot(&trace);
    let compositor_lines = snapshot(&compositor_log);
    analyze(&lines, &compositor_lines)
}

/// Spawns the compositor with its own runtime directory, reaching the host
/// compositor through the absolute socket path resolved by the caller.
fn host_wayland_display() -> Result<Option<String>, String> {
    let Ok(display) = std::env::var("WAYLAND_DISPLAY") else {
        // No Wayland host; winit may still start on X11 through DISPLAY.
        return Ok(None);
    };
    if display.starts_with('/') {
        return Ok(Some(display));
    }
    let runtime = std::env::var("XDG_RUNTIME_DIR")
        .map_err(|_| "WAYLAND_DISPLAY is relative but XDG_RUNTIME_DIR is not set".to_string())?;
    Ok(Some(format!("{runtime}/{display}")))
}

/// Waits until the compositor has created a `wayland-*` socket in its runtime
/// directory, or the compositor exits or the deadline passes.
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

/// Waits until the trace shows the specific frame callback completing, or the
/// client exits, or the deadline passes.
fn wait_for_sequence(trace: &Sink, client: &mut ChildGuard) -> Result<(), String> {
    let deadline = Instant::now() + CLIENT_TIMEOUT;
    loop {
        if frame_callback_completed(&snapshot(trace)) {
            return Ok(());
        }
        if let Some(status) = client.poll()? {
            return Err(format!(
                "client exited ({status}) before the sequence completed; trace:\n{}",
                snapshot(trace).join("\n")
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for the mapping sequence; trace:\n{}",
                snapshot(trace).join("\n")
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Whether a `wl_surface.frame` callback has completed for its own id.
fn frame_callback_completed(lines: &[String]) -> bool {
    let mut requested = Vec::new();
    for raw in lines {
        let line = Line::parse(raw);
        if line.request && line.body.contains(".frame(new id wl_callback#") {
            if let Some(id) = find_id_after(line.body, "wl_callback#") {
                requested.push(id);
            }
        } else if let Some(id) = callback_done_id(&line) {
            if requested.contains(&id) {
                return true;
            }
        }
    }
    false
}

/// Checks every step of the mapping sequence and returns the first failure.
fn analyze(lines: &[String], compositor_log: &[String]) -> Result<(), String> {
    let trace = Trace::parse(lines);
    let surface = trace.toplevel_surface.ok_or_else(|| {
        "the trace never identified the toplevel's wl_surface (no xdg_wm_base.get_xdg_surface)"
            .to_string()
    })?;
    let toplevel = trace.toplevel.ok_or_else(|| {
        "the trace never identified the xdg_toplevel (no xdg_surface.get_toplevel)".to_string()
    })?;
    let xdg_surface = trace
        .xdg_surface
        .expect("toplevel_surface implies xdg_surface");

    let configure = trace
        .first_toplevel_configure(toplevel)
        .ok_or_else(|| format!("no xdg_toplevel#{toplevel}.configure was received"))?;
    let surface_configure = trace
        .first_surface_configure(xdg_surface)
        .ok_or_else(|| format!("no xdg_surface#{xdg_surface}.configure was received"))?;
    let ack = trace
        .first_ack_configure(xdg_surface)
        .ok_or_else(|| format!("no xdg_surface#{xdg_surface}.ack_configure was sent"))?;
    let attach = trace.first_nonnull_attach(surface).ok_or_else(|| {
        format!("no non-null wl_surface#{surface}.attach was sent; the client never drew")
    })?;

    if configure.index >= attach.index {
        return Err(format!(
            "out of order: xdg_toplevel.configure (line {}) must precede the first non-null \
             wl_surface.attach (line {})",
            configure.index + 1,
            attach.index + 1
        ));
    }

    if surface_configure.serial != ack.serial {
        return Err(format!(
            "ack_configure serial {} does not match the configure serial {} (configure at line {}, \
             ack at line {})",
            ack.serial,
            surface_configure.serial,
            surface_configure.index + 1,
            ack.index + 1
        ));
    }
    if ack.index >= attach.index {
        return Err(format!(
            "out of order: ack_configure for serial {} (line {}) must precede the first non-null \
             attach (line {})",
            ack.serial,
            ack.index + 1,
            attach.index + 1
        ));
    }

    let commit = trace
        .first_commit_after(surface, attach.index)
        .ok_or_else(|| {
            format!("the non-null attach on wl_surface#{surface} was never committed")
        })?;
    if !compositor_log
        .iter()
        .any(|line| line.contains(MAPPED_LOG_LINE))
    {
        return Err(format!(
            "the client attached and committed a buffer but the compositor never reported a \
             mapping; compositor log:\n{}",
            compositor_log.join("\n")
        ));
    }

    let enter = trace.first_keyboard_enter(surface).ok_or_else(|| {
        format!("wl_keyboard.enter was never delivered to the mapped surface #{surface}")
    })?;
    if enter.index <= commit.index {
        return Err(format!(
            "out of order: wl_keyboard.enter (line {}) must follow the mapping commit (line {})",
            enter.index + 1,
            commit.index + 1
        ));
    }

    let frame = trace.first_frame(surface).ok_or_else(|| {
        format!("wl_surface#{surface}.frame was never called; no frame callback to check")
    })?;
    let done = trace.first_callback_done_after(frame.callback, frame.index);
    let Some(done) = done else {
        return Err(format!(
            "wl_surface#{surface}.frame requested wl_callback#{}, but wl_callback#{} .done never \
             arrived after the request (line {}); a bare wl_callback.done is not accepted",
            frame.callback,
            frame.callback,
            frame.index + 1
        ));
    };

    check_pointer_cursor(&trace)?;

    if !compositor_log
        .iter()
        .any(|line| line.contains(DEFAULT_CURSOR_LOG_LINE))
    {
        return Err(format!(
            "the compositor never reported drawing its default pointer; compositor log:\n{}",
            compositor_log.join("\n")
        ));
    }

    println!(
        "smoke: configure(serial {}) -> ack({}) -> attach/commit -> keyboard.enter -> \
         frame callback #{} done at line {}; cursor surfaces set: {}",
        surface_configure.serial,
        ack.serial,
        frame.callback,
        done.index + 1,
        trace
            .set_cursors
            .iter()
            .filter(|set| set.surface.is_some())
            .count(),
    );
    Ok(())
}

/// Checks that every `wl_pointer.set_cursor` used the serial of a pointer enter
/// the client had already received.
///
/// This is the client half of the cursor contract: the compositor's own serial
/// check is Smithay's, against the pointer's last enter serial, which the
/// compositor generated from its single counter. A trace with no pointer enter
/// (the smoke cannot move the host pointer) has nothing to check and passes.
fn check_pointer_cursor(trace: &Trace) -> Result<(), String> {
    for set in &trace.set_cursors {
        let matched = trace
            .pointer_enters
            .iter()
            .any(|enter| enter.index < set.index && enter.serial == set.serial);
        if !matched {
            return Err(format!(
                "wl_pointer.set_cursor at line {} used serial {}, which matches no preceding \
                 wl_pointer.enter; a client may only set a cursor for the serial of an enter it \
                 received",
                set.index + 1,
                set.serial
            ));
        }
    }
    Ok(())
}

/// A parsed `WAYLAND_DEBUG` line, split into direction and body.
struct Line<'a> {
    request: bool,
    body: &'a str,
}

impl<'a> Line<'a> {
    fn parse(raw: &'a str) -> Self {
        let rest = raw.find(']').map_or(raw, |end| raw[end + 1..].trim_start());
        let rest = if rest.starts_with('{') {
            rest.find('}')
                .map_or(rest, |end| rest[end + 1..].trim_start())
        } else {
            rest
        };
        if let Some(body) = rest.strip_prefix("->") {
            Self {
                request: true,
                body: body.trim_start(),
            }
        } else if let Some(body) = rest.strip_prefix("<-") {
            Self {
                request: false,
                body: body.trim_start(),
            }
        } else {
            Self {
                request: false,
                body: rest.trim_start(),
            }
        }
    }
}

/// One protocol event of interest, with the index of the line it came from.
struct ToplevelConfigure {
    index: usize,
    toplevel: u64,
}

/// The events and object associations extracted from a trace.
#[derive(Default)]
struct Trace {
    toplevel_surface: Option<u64>,
    xdg_surface: Option<u64>,
    toplevel: Option<u64>,
    toplevel_configures: Vec<ToplevelConfigure>,
    surface_configures: Vec<Configure>,
    acks: Vec<Configure>,
    attaches: Vec<Attach>,
    commits: Vec<Commit>,
    frames: Vec<Frame>,
    callback_dones: Vec<CallbackDone>,
    keyboard_enters: Vec<KeyboardEnter>,
    pointer_enters: Vec<PointerEnter>,
    set_cursors: Vec<SetCursor>,
}

struct Configure {
    index: usize,
    xdg_surface: u64,
    serial: u64,
}

struct Attach {
    index: usize,
    surface: u64,
    buffer: Option<u64>,
}

struct Commit {
    index: usize,
    surface: u64,
}

struct Frame {
    index: usize,
    surface: u64,
    callback: u64,
}

struct CallbackDone {
    index: usize,
    callback: u64,
}

struct KeyboardEnter {
    index: usize,
    surface: u64,
}

struct PointerEnter {
    index: usize,
    serial: u64,
}

struct SetCursor {
    index: usize,
    serial: u64,
    /// The cursor surface, or `None` for `nil` (hide the cursor).
    surface: Option<u64>,
}

impl Trace {
    fn parse(lines: &[String]) -> Self {
        let mut trace = Trace::default();
        for (index, raw) in lines.iter().enumerate() {
            let line = Line::parse(raw);
            let body = line.body;

            if line.request && body.contains("get_xdg_surface(new id xdg_surface#") {
                trace.xdg_surface = find_id_after(body, "xdg_surface#");
                trace.toplevel_surface = find_id_after(body, "wl_surface#");
                continue;
            }
            if line.request && body.contains("get_toplevel(new id xdg_toplevel#") {
                trace.toplevel = find_id_after(body, "xdg_toplevel#");
                continue;
            }

            if let Some(toplevel) = object_id(body, "xdg_toplevel") {
                if body.contains(".configure(") {
                    trace
                        .toplevel_configures
                        .push(ToplevelConfigure { index, toplevel });
                }
            } else if let Some(xdg_surface) = object_id(body, "xdg_surface") {
                if body.contains(".configure(") {
                    trace.surface_configures.push(Configure {
                        index,
                        xdg_surface,
                        serial: first_argument(body).unwrap_or(0),
                    });
                } else if line.request && body.contains(".ack_configure(") {
                    trace.acks.push(Configure {
                        index,
                        xdg_surface,
                        serial: first_argument(body).unwrap_or(0),
                    });
                }
            } else if line.request && object_id(body, "wl_surface").is_some() {
                let surface = object_id(body, "wl_surface").expect("checked above");
                if body.contains(".attach(") {
                    trace.attaches.push(Attach {
                        index,
                        surface,
                        buffer: find_id_after(body, "wl_buffer#"),
                    });
                } else if body.contains(".commit(") {
                    trace.commits.push(Commit { index, surface });
                } else if body.contains(".frame(new id wl_callback#") {
                    if let Some(callback) = find_id_after(body, "wl_callback#") {
                        trace.frames.push(Frame {
                            index,
                            surface,
                            callback,
                        });
                    }
                }
            } else if let Some(callback) = callback_done_id(&line) {
                trace.callback_dones.push(CallbackDone { index, callback });
            } else if object_id(body, "wl_keyboard").is_some() && body.contains(".enter(") {
                if let Some(surface) = find_id_after(body, "wl_surface#") {
                    trace.keyboard_enters.push(KeyboardEnter { index, surface });
                }
            } else if object_id(body, "wl_pointer").is_some() {
                if !line.request && body.contains(".enter(") {
                    trace.pointer_enters.push(PointerEnter {
                        index,
                        serial: first_argument(body).unwrap_or(0),
                    });
                } else if line.request && body.contains(".set_cursor(") {
                    trace.set_cursors.push(SetCursor {
                        index,
                        serial: first_argument(body).unwrap_or(0),
                        surface: find_id_after(body, "wl_surface#"),
                    });
                }
            }
        }
        trace
    }

    fn first_toplevel_configure(&self, toplevel: u64) -> Option<&ToplevelConfigure> {
        self.toplevel_configures
            .iter()
            .find(|configure| configure.toplevel == toplevel)
    }

    fn first_surface_configure(&self, xdg_surface: u64) -> Option<&Configure> {
        self.surface_configures
            .iter()
            .find(|configure| configure.xdg_surface == xdg_surface)
    }

    fn first_ack_configure(&self, xdg_surface: u64) -> Option<&Configure> {
        self.acks.iter().find(|ack| ack.xdg_surface == xdg_surface)
    }

    fn first_nonnull_attach(&self, surface: u64) -> Option<&Attach> {
        self.attaches
            .iter()
            .find(|attach| attach.surface == surface && attach.buffer.is_some())
    }

    fn first_commit_after(&self, surface: u64, index: usize) -> Option<&Commit> {
        self.commits
            .iter()
            .find(|commit| commit.surface == surface && commit.index > index)
    }

    fn first_keyboard_enter(&self, surface: u64) -> Option<&KeyboardEnter> {
        self.keyboard_enters
            .iter()
            .find(|enter| enter.surface == surface)
    }

    fn first_frame(&self, surface: u64) -> Option<&Frame> {
        self.frames.iter().find(|frame| frame.surface == surface)
    }

    fn first_callback_done_after(&self, callback: u64, index: usize) -> Option<&CallbackDone> {
        self.callback_dones
            .iter()
            .find(|done| done.callback == callback && done.index > index)
    }
}

/// The callback id of a `wl_callback#N.done(...)` event, if the line is one.
fn callback_done_id(line: &Line<'_>) -> Option<u64> {
    if line.request || !line.body.contains(".done(") {
        return None;
    }
    object_id(line.body, "wl_callback")
}

/// The id of an object `ty#N` at the start of a line body.
fn object_id(body: &str, ty: &str) -> Option<u64> {
    let rest = body.strip_prefix(ty)?.strip_prefix('#')?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The first number following a `marker#` substring, e.g. the id in
/// `wl_surface#3.attach(wl_buffer#48, 0, 0)`.
fn find_id_after(body: &str, marker: &str) -> Option<u64> {
    let start = body.find(marker)? + marker.len();
    let digits: String = body[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// The first integer argument of a call, e.g. the serial in
/// `xdg_surface#18.configure(2)`.
fn first_argument(body: &str) -> Option<u64> {
    let rest = &body[body.find('(')? + 1..];
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// A child process that is killed and reaped when it drops.
struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    /// Spawns `command` with piped output and optionally an absolute
    /// `WAYLAND_DISPLAY` for reaching the host compositor.
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

    /// Starts draining the child's stdout and stderr into sinks. Both streams
    /// are kept because the compositor logs to stdout while `WAYLAND_DEBUG`
    /// client output goes to stderr.
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

    /// Returns `Some(status)` once the child has exited.
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
            std::env::temp_dir().join(format!("neptune-smoke-{}-{unique}", std::process::id()));
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

#[cfg(test)]
mod tests {
    use super::{analyze, check_pointer_cursor, Trace, DEFAULT_CURSOR_LOG_LINE, MAPPED_LOG_LINE};

    /// Wraps a body in the framing libwayland's debug output uses.
    fn line(body: &str) -> String {
        format!("[00:00:00.000000] {{Default Queue}} {body}")
    }

    /// The smallest trace that satisfies every assertion.
    fn complete_trace() -> Vec<String> {
        vec![
            line(" -> xdg_wm_base#8.get_xdg_surface(new id xdg_surface#18, wl_surface#3)"),
            line(" -> xdg_surface#18.get_toplevel(new id xdg_toplevel#19)"),
            line("xdg_toplevel#19.configure(1248, 800, array[0])"),
            line("xdg_surface#18.configure(2)"),
            line(" -> xdg_surface#18.ack_configure(2)"),
            line(" -> wl_surface#3.attach(wl_buffer#48, 0, 0)"),
            line(" -> wl_surface#3.commit()"),
            line(" -> wl_surface#3.frame(new id wl_callback#49)"),
            line("wl_keyboard#14.enter(3, wl_surface#3, array[0])"),
            line("wl_callback#49.done(1706)"),
        ]
    }

    fn compositor_log() -> Vec<String> {
        vec![
            format!("INFO {MAPPED_LOG_LINE}"),
            format!("DEBUG {DEFAULT_CURSOR_LOG_LINE}"),
        ]
    }

    #[test]
    fn a_complete_trace_passes() {
        assert_eq!(analyze(&complete_trace(), &compositor_log()), Ok(()));
    }

    #[test]
    fn configure_must_precede_the_first_attach() {
        let mut trace = complete_trace();
        let configure = trace.remove(2);
        trace.push(configure);
        let error = analyze(&trace, &compositor_log()).expect_err("should be out of order");
        assert!(error.contains("must precede"), "unexpected error: {error}");
    }

    #[test]
    fn the_ack_serial_must_match_the_configure_serial() {
        let mut trace = complete_trace();
        trace[4] = line(" -> xdg_surface#18.ack_configure(1)");
        let error = analyze(&trace, &compositor_log()).expect_err("serial mismatch should fail");
        assert!(
            error.contains("does not match"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_sync_completion_is_not_a_frame_callback() {
        let mut trace = complete_trace();
        trace.retain(|l| !l.contains(".frame(") && !l.contains("wl_callback#49.done"));
        trace.push(line(" -> wl_display#1.sync(new id wl_callback#60)"));
        trace.push(line("wl_callback#60.done(0)"));
        let error = analyze(&trace, &compositor_log()).expect_err("sync is not a frame");
        assert!(
            error.contains(".frame was never called"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn only_the_callback_id_from_the_frame_request_completes_it() {
        let mut trace = complete_trace();
        trace.retain(|l| !l.contains("wl_callback#49.done"));
        trace.push(line("wl_callback#99.done(1706)"));
        let error = analyze(&trace, &compositor_log()).expect_err("wrong id should fail");
        assert!(
            error.contains("#49"),
            "error should name the requested id: {error}"
        );
    }

    #[test]
    fn keyboard_enter_must_reach_the_mapped_surface() {
        let mut trace = complete_trace();
        trace.retain(|l| !l.contains("wl_keyboard#14.enter"));
        let error = analyze(&trace, &compositor_log()).expect_err("no keyboard focus should fail");
        assert!(
            error.contains("wl_keyboard.enter"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_buffered_commit_without_a_mapping_report_fails() {
        let log = vec!["INFO client connected".to_string()];
        let error = analyze(&complete_trace(), &log).expect_err("no mapping report should fail");
        assert!(
            error.contains("never reported a mapping"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_set_cursor_that_matches_a_preceding_enter_is_accepted() {
        let trace = vec![
            line("wl_pointer#16.enter(7, wl_surface#3, 0.0, 0.0)"),
            line(" -> wl_pointer#16.set_cursor(7, wl_surface#25, 6, 19)"),
        ];
        assert_eq!(check_pointer_cursor(&Trace::parse(&trace)), Ok(()));
    }

    #[test]
    fn a_set_cursor_with_a_serial_no_enter_carried_is_rejected() {
        let trace = vec![
            line("wl_pointer#16.enter(7, wl_surface#3, 0.0, 0.0)"),
            line(" -> wl_pointer#16.set_cursor(8, wl_surface#25, 6, 19)"),
        ];
        let error =
            check_pointer_cursor(&Trace::parse(&trace)).expect_err("a foreign serial should fail");
        assert!(error.contains("serial 8"), "unexpected error: {error}");
    }

    #[test]
    fn a_set_cursor_before_any_enter_is_rejected() {
        let trace = vec![line(
            " -> wl_pointer#16.set_cursor(7, wl_surface#25, 6, 19)",
        )];
        let error = check_pointer_cursor(&Trace::parse(&trace)).expect_err("no enter should fail");
        assert!(error.contains("no preceding"), "unexpected error: {error}");
    }

    #[test]
    fn a_null_cursor_surface_is_recognized_as_hiding_the_cursor() {
        let trace = vec![
            line("wl_pointer#16.enter(7, wl_surface#3, 0.0, 0.0)"),
            line(" -> wl_pointer#16.set_cursor(7, nil, 0, 0)"),
        ];
        let parsed = Trace::parse(&trace);
        assert_eq!(check_pointer_cursor(&parsed), Ok(()));
        assert_eq!(parsed.set_cursors[0].surface, None);
    }

    #[test]
    fn the_default_pointer_draw_must_be_reported() {
        let log = vec![format!("INFO {MAPPED_LOG_LINE}")];
        let error = analyze(&complete_trace(), &log).expect_err("no default pointer draw");
        assert!(
            error.contains("default pointer"),
            "unexpected error: {error}"
        );
    }
}
