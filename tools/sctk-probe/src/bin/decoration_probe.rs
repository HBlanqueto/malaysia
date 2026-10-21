//! Phase 6 acceptance harness: real applications under Yuki.
//!
//! For each application in the table below it launches the application inside a
//! fresh Yuki, with `WAYLAND_DEBUG=1`, captures the client's protocol trace
//! and asserts what the client negotiated:
//!
//! - which decoration protocol it bound (xdg-decoration, the KDE one, or none);
//! - the mode Yuki configured (server-side or client-side);
//! - whether the client surface was inset by the titlebar: the xdg configure
//!   width drops by the titlebar thickness exactly when Yuki reserves chrome.
//!
//! The expectations are derived from the toolkits' sources (ADR 0019): Qt
//! defers to the compositor, GTK3/GTK4 ask through the KDE protocol and keep
//! their own chrome when they have a headerbar, libadwaita windows always have
//! one, and a client that never negotiates is left alone.
//!
//! Applications are located through `YUKI_APP_<NAME>` environment variables
//! (see the crate README); a name with no binary is reported UNVERIFIED rather
//! than skipped silently. Whether two titlebars are *visible* is not machine
//! checkable here: it needs a pixel comparison against the decoration region,
//! so it stays manual.
//!
//! Run from the dev shell, or `./dev.sh decoration` which builds and runs it.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long to wait for the compositor to create its socket.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(15);
/// How long each application gets to start, negotiate and be killed.
const APP_TIMEOUT: Duration = Duration::from_secs(6);
/// The titlebar thickness Yuki reserves, in logical pixels
/// (`yuki_design::TITLEBAR.thickness`).
const TITLEBAR: i32 = 32;

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("decoration_probe: PASS");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("decoration_probe: FAIL: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The decoration protocol a client is expected to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    /// `zxdg_decoration_manager_v1`.
    Xdg,
    /// `org_kde_kwin_server_decoration_manager`.
    Kde,
    /// Neither: the client never negotiates decorations.
    None,
}

/// The mode Yuki is expected to end up configuring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Client,
    Server,
}

/// What one application should negotiate.
#[derive(Debug, Clone, Copy)]
struct Expectation {
    /// The protocol the client binds.
    protocol: Protocol,
    /// The final decoration mode, or `None` when no mode is negotiated.
    mode: Option<Mode>,
    /// How much the xdg configure width should shrink below its maximum: the
    /// titlebar thickness when Yuki reserves chrome, zero when it does not.
    inset: i32,
}

/// One application under test.
struct Case {
    /// The name in the report.
    name: &'static str,
    /// The `YUKI_APP_<NAME>` variable that points at the binary.
    variable: &'static str,
    /// A command to fall back to when the variable is unset.
    fallback: &'static str,
    /// Arguments passed to the binary.
    args: Vec<String>,
    /// Extra environment for this application.
    env: Vec<(String, String)>,
    /// What the run must show.
    expectation: Expectation,
}

/// The verdict for one application.
enum Outcome {
    /// The observation matched the expectation.
    Pass,
    /// The application could not be exercised here; reported, not failed.
    Unverified(String),
    /// The observation contradicted the expectation.
    Fail(String),
}

/// What the trace actually showed.
///
/// Events after the client starts resizing or changing state itself are ignored:
/// a host pointer can click into a window and make the client request an
/// interactive resize, which changes the configured size for reasons that have
/// nothing to do with decorations.
#[derive(Debug, Default)]
struct Observation {
    /// The client bound `zxdg_decoration_manager_v1`.
    xdg_bound: bool,
    /// The client bound `org_kde_kwin_server_decoration_manager`.
    kde_bound: bool,
    /// Every decoration mode the client was configured with, in order.
    modes: Vec<Mode>,
    /// Every `xdg_toplevel.configure` width the client received, in order.
    widths: Vec<i32>,
    /// The width the compositor advertised for its output, from `wl_output`.
    output_width: Option<i32>,
}

impl Observation {
    /// Reads a client's `WAYLAND_DEBUG` output.
    fn parse(lines: &[String]) -> Self {
        let mut observation = Self::default();
        let mut cutoff: Option<usize> = None;
        for (index, line) in lines.iter().enumerate() {
            let Some(body) = body(line) else {
                continue;
            };
            if body.contains("->") && body.contains(".bind(") {
                if body.contains("\"zxdg_decoration_manager_v1\"") {
                    observation.xdg_bound = true;
                }
                if body.contains("\"org_kde_kwin_server_decoration_manager\"") {
                    observation.kde_bound = true;
                }
                continue;
            }
            if body.contains("->") {
                if cutoff.is_none() {
                    if let Some(token) = event_token(body) {
                        let is_state_change = (token.starts_with("xdg_toplevel@")
                            || token.starts_with("xdg_toplevel#"))
                            && (token.ends_with(".resize")
                                || token.ends_with(".set_maximized")
                                || token.ends_with(".set_fullscreen"));
                        if is_state_change {
                            cutoff = Some(index);
                        }
                    }
                }
                continue;
            }
            if cutoff.is_some_and(|cutoff| index >= cutoff) {
                continue;
            }
            let Some(token) = event_token(body) else {
                continue;
            };
            if token.starts_with("wl_output") && token.ends_with(".mode") {
                observation.output_width = nth_integer(body, 1).or(observation.output_width);
            }
            let is_decoration_mode = (token.starts_with("zxdg_toplevel_decoration_v1")
                && token.ends_with(".configure"))
                || (token.starts_with("org_kde_kwin_server_decoration")
                    && token.ends_with(".mode"));
            if is_decoration_mode {
                if let Some(mode) = first_integer(body) {
                    observation.modes.push(if mode == 2 {
                        Mode::Server
                    } else {
                        Mode::Client
                    });
                }
            } else if (token.starts_with("xdg_toplevel@") || token.starts_with("xdg_toplevel#"))
                && token.ends_with(".configure")
                && !body.contains("zxdg_toplevel_decoration")
            {
                if let Some(width) = first_integer(body) {
                    observation.widths.push(width);
                }
            }
        }
        observation
    }

    /// The protocol the client actually used.
    fn protocol(&self) -> Protocol {
        if self.xdg_bound {
            Protocol::Xdg
        } else if self.kde_bound {
            Protocol::Kde
        } else {
            Protocol::None
        }
    }

    /// The last mode the compositor configured.
    fn mode(&self) -> Option<Mode> {
        self.modes.last().copied()
    }

    /// How much narrower the client's surface was configured than the output.
    ///
    /// The compositor configures the outer size minus its insets, so the last
    /// configure is the one to read. Where the output width is unknown (no
    /// `wl_output` event seen), the spread of the configures is the fallback.
    fn inset(&self) -> i32 {
        let last = self.widths.last().copied();
        match (self.output_width, last) {
            (Some(output), Some(last)) => (output - last).max(0),
            _ => match (
                self.widths.iter().max().copied(),
                self.widths.iter().min().copied(),
            ) {
                (Some(max), Some(min)) => max - min,
                _ => 0,
            },
        }
    }
}

/// The protocol message on a `WAYLAND_DEBUG` line, or `None` for other output.
fn body(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let body = rest[close + 1..].trim();
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

/// The `interface@id.method` token on a protocol line, without the arguments.
fn event_token(body: &str) -> Option<&str> {
    let start = body.rfind('.').map(|dot| {
        let before = &body[..dot];
        before.rfind(' ').map(|space| space + 1).unwrap_or(0)
    })?;
    let token = &body[start..];
    let open = token.find('(')?;
    let token = &token[..open];
    if token.contains('@') || token.contains('#') {
        Some(token)
    } else {
        None
    }
}

/// The first integer argument on a protocol line.
fn first_integer(body: &str) -> Option<i32> {
    nth_integer(body, 0)
}

/// The `n`-th integer argument on a protocol line, counting from zero.
fn nth_integer(body: &str, n: usize) -> Option<i32> {
    let open = body.find('(')? + 1;
    let rest = &body[open..];
    rest.split(',')
        .filter_map(|part| {
            let part = part.trim();
            let end = part
                .find(|c: char| !(c.is_ascii_digit() || c == '-'))
                .unwrap_or(part.len());
            part[..end].parse::<i32>().ok()
        })
        .nth(n)
}

/// Runs every case against one Yuki.
fn run() -> Result<(), String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "cannot locate the repository root from the probe manifest".to_string())?;
    let binary = std::env::var_os("YUKI_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("target/debug/yuki"));
    if !binary.is_file() {
        return Err(format!(
            "compositor binary not found at {}; run `cargo build -p yuki` first",
            binary.display()
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

    let mut failed = Vec::new();
    println!("application            protocol mode       inset      chrome status");
    for case in cases(runtime.path()) {
        let Some(program) = resolve(case.variable, case.fallback) else {
            println!(
                "{:<22} {:<8} {:<10} {:<10} {:<6} UNVERIFIED (not installed)",
                case.name, "-", "-", "-", "-"
            );
            continue;
        };
        // Apps such as Nautilus are single-instance over the session bus: a
        // plain launch would activate the instance already running in the host
        // GNOME session instead of creating a window here. A private bus makes
        // the app a new process that connects to this Yuki.
        let mut command = Command::new("dbus-run-session");
        command
            .arg("--")
            .arg(&program)
            .args(&case.args)
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WAYLAND_DISPLAY", &socket)
            .env("WAYLAND_DEBUG", "1")
            .env("GDK_BACKEND", "wayland")
            .env("QT_QPA_PLATFORM", "wayland")
            // Clients under test must reach this Yuki, not the host session.
            .env_remove("DISPLAY")
            .stdin(Stdio::null());
        for (key, value) in &case.env {
            command.env(key, value);
        }
        let mut app = ChildGuard::spawn(&mut command, None)
            .map_err(|error| format!("failed to start {}: {error}", case.name))?;
        let app_log = app.capture()?;
        thread::sleep(APP_TIMEOUT);
        let exited = app.poll().ok().flatten();
        app.kill();
        let lines = snapshot(&app_log);

        let observation = Observation::parse(&lines);
        let status = match check(&case, &observation, &lines, exited) {
            Outcome::Pass => "PASS".to_string(),
            Outcome::Unverified(reason) => format!("UNVERIFIED ({reason})"),
            Outcome::Fail(reason) => {
                failed.push(format!("{}: {reason}", case.name));
                format!("FAIL: {reason}")
            }
        };
        let chrome = match observation.inset() {
            0 => "no",
            _ => "yes",
        };
        println!(
            "{:<22} {:<8} {:<10} {:<10} {:<6} {}",
            case.name,
            format!("{:?}", observation.protocol()).to_lowercase(),
            observation
                .mode()
                .map(|mode| format!("{mode:?}").to_lowercase())
                .unwrap_or_else(|| "-".to_string()),
            format!("{} px", observation.inset()),
            chrome,
            status
        );
    }

    match run_mode_change(&socket, runtime.path()) {
        Ok(lines) => {
            for line in lines
                .iter()
                .filter(|line| line.contains("decoration_client:"))
            {
                println!("{line}");
            }
            println!("mode change after mapping: PASS");
        }
        Err(reason) => failed.push(format!("mode change after mapping: {reason}")),
    }

    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

/// Runs the mode-change client against the same compositor.
///
/// This is the one policy rule the real applications cannot be made to
/// exercise: a window that changes its decoration mode while it is mapped must
/// be reconfigured with the new mode and the new usable size, leaving no stale
/// titlebar (ADR 0019, rule 4).
fn run_mode_change(socket: &str, runtime: &Path) -> Result<Vec<String>, String> {
    let client = std::env::var_os("YUKI_DECORATION_CLIENT")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("decoration_client")))
        })
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            "decoration_client not found next to the probe; run `cargo build -p sctk-probe` first"
                .to_string()
        })?;

    let mut child = ChildGuard::spawn(
        Command::new(&client)
            .env("XDG_RUNTIME_DIR", runtime)
            .env("WAYLAND_DISPLAY", socket)
            .stdin(Stdio::null()),
        None,
    )?;
    let log = child.capture()?;
    let deadline = Instant::now() + APP_TIMEOUT * 3;
    loop {
        if let Some(status) = child.poll()? {
            let lines = snapshot(&log);
            if !status.success() {
                return Err(format!(
                    "the mode-change client exited with {status}; output:\n{}",
                    lines.join("\n")
                ));
            }
            for required in [
                "server configure",
                "requested client-side after mapping",
                "client configure",
            ] {
                if !lines.iter().any(|line| line.contains(required)) {
                    return Err(format!(
                        "the mode-change client never reported `{required}`; output:\n{}",
                        lines.join("\n")
                    ));
                }
            }
            return Ok(lines);
        }
        if Instant::now() >= deadline {
            return Err("the mode-change client did not exit in time".to_string());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Compares one observation against its expectation.
fn check(
    case: &Case,
    observation: &Observation,
    lines: &[String],
    exited: Option<std::process::ExitStatus>,
) -> Outcome {
    if lines.is_empty() {
        return Outcome::Unverified("the application produced no output".to_string());
    }
    // An application that created no toplevel was never decorated; passing it
    // would be a vacuous result.
    if observation.widths.is_empty() {
        // An application that never connected to this compositor is using
        // another display; that is an environment fact, not a policy failure.
        if !lines.iter().any(|line| line.contains("wl_display#")) {
            return Outcome::Unverified(
                "the application never connected to the Wayland display".to_string(),
            );
        }
        // One that exits on its own never got to map a window either.
        return match exited {
            Some(status) => Outcome::Unverified(format!(
                "the application exited with {status} before mapping a window"
            )),
            None => Outcome::Fail("no toplevel window was created".to_string()),
        };
    }
    if observation.protocol() != case.expectation.protocol {
        return Outcome::Fail(format!(
            "expected the {:?} protocol, saw {:?}",
            case.expectation.protocol,
            observation.protocol()
        ));
    }
    if observation.mode() != case.expectation.mode {
        return Outcome::Fail(format!(
            "expected mode {:?}, saw {:?}",
            case.expectation.mode,
            observation.mode()
        ));
    }
    if observation.inset() != case.expectation.inset {
        return Outcome::Fail(format!(
            "expected a {}-pixel titlebar inset, saw {} px (configure widths {:?})",
            case.expectation.inset,
            observation.inset(),
            observation.widths
        ));
    }
    Outcome::Pass
}

/// The applications under test and what each must negotiate.
fn cases(runtime: &Path) -> Vec<Case> {
    let qml = runtime.join("probe.qml");
    let _ = fs::write(
        &qml,
        "import QtQuick\nimport QtQuick.Window\nWindow { width: 320; height: 200; visible: true }\n",
    );
    let mut qt6 = vec![(
        "QML_IMPORT_PATH".to_string(),
        std::env::var("YUKI_APP_QT6_QML_IMPORT_PATH").unwrap_or_default(),
    )];
    qt6.retain(|(_, value)| !value.is_empty());

    vec![
        Case {
            name: "nautilus",
            variable: "YUKI_APP_NAUTILUS",
            fallback: "nautilus",
            args: vec!["--no-desktop".to_string()],
            env: Vec::new(),
            // Nautilus 50 requests the KDE server-side mode explicitly, even
            // though it is GTK4. ADR 0019 says the compositor honours that
            // request; forcing client-side here would hide a real double
            // decoration. The expectation is pinned to the version in the dev
            // shell and must be revisited if Nautilus changes its mind.
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "gtk3-demo",
            variable: "YUKI_APP_GTK3_DEMO",
            fallback: "gtk3-demo",
            args: Vec::new(),
            env: Vec::new(),
            // Its window is built with a GtkHeaderBar, so it keeps CSD.
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Client),
                inset: 0,
            },
        },
        Case {
            name: "gtk3-widget-factory",
            variable: "YUKI_APP_GTK3_WIDGET_FACTORY",
            fallback: "gtk3-widget-factory",
            args: Vec::new(),
            env: Vec::new(),
            // Its window is built with a GtkHeaderBar too, so client-side.
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Client),
                inset: 0,
            },
        },
        Case {
            name: "gtk3-plain",
            variable: "YUKI_APP_GTK3_PLAIN",
            fallback: "gtk3_plain",
            args: Vec::new(),
            env: Vec::new(),
            // A plain GtkWindow with no headerbar: GTK3 requests server-side
            // through the KDE protocol and Yuki insets the surface.
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "gtk4-demo",
            variable: "YUKI_APP_GTK4_DEMO",
            fallback: "gtk4-demo",
            args: Vec::new(),
            env: Vec::new(),
            // This demo keeps client-side chrome of its own, so it never
            // creates a decoration object; the policy leaves it alone.
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: None,
                inset: 0,
            },
        },
        Case {
            name: "gtk4-widget-factory",
            variable: "YUKI_APP_GTK4_WIDGET_FACTORY",
            fallback: "gtk4-widget-factory",
            args: Vec::new(),
            env: Vec::new(),
            // Its window sets a GtkHeaderBar as titlebar: client-side.
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Client),
                inset: 0,
            },
        },
        Case {
            name: "gtk4-plain",
            variable: "YUKI_APP_GTK4_PLAIN",
            fallback: "gtk4_plain",
            args: Vec::new(),
            env: Vec::new(),
            // A plain GtkWindow: GTK4 asks for server-side, and it asks before
            // the xdg_toplevel exists, so this pins that ordering too.
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "qt6",
            variable: "YUKI_APP_QT6",
            fallback: "qml",
            args: vec![qml.to_string_lossy().into_owned()],
            env: qt6,
            // Qt creates the xdg-decoration object, unset_mode, and defers to
            // the compositor: Yuki chooses server-side.
            expectation: Expectation {
                protocol: Protocol::Xdg,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "qt5",
            variable: "YUKI_APP_QT5",
            fallback: "qmlscene",
            args: vec![qml.to_string_lossy().into_owned()],
            env: Vec::new(),
            // Qt 5.15 has the same xdg-decoration handling as Qt 6.
            expectation: Expectation {
                protocol: Protocol::Xdg,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "onlyoffice",
            variable: "YUKI_APP_ONLYOFFICE",
            fallback: "onlyoffice-desktopeditors",
            args: Vec::new(),
            env: Vec::new(),
            // It draws its own title area; if it is frameless Qt, no decoration
            // object is created at all.
            expectation: Expectation {
                protocol: Protocol::None,
                mode: None,
                inset: 0,
            },
        },
    ]
}

/// Resolves an application binary from its environment variable or `PATH`.
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

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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
            std::env::temp_dir().join(format!("yuki-decoration-{}-{unique}", std::process::id()));
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

#[cfg(test)]
mod tests {
    use super::{Mode, Observation, Protocol};

    fn trace(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| line.to_string()).collect()
    }

    #[test]
    fn an_xdg_client_that_gets_server_side_shows_a_titlebar_inset() {
        let lines = trace(&[
            "[1] wl_registry#2.global(6, \"zxdg_decoration_manager_v1\", 1)",
            "[2]  -> wl_registry#2.bind(6, \"zxdg_decoration_manager_v1\", 1, new id [unknown]#9)",
            "[3] wl_output#11.mode(3, 1280, 800, 60000)",
            "[4] zxdg_toplevel_decoration_v1#21.configure(2)",
            "[5] xdg_toplevel#20.configure(1248, 800, array[0])",
        ]);
        let observation = Observation::parse(&lines);
        assert_eq!(observation.protocol(), Protocol::Xdg);
        assert_eq!(observation.mode(), Some(Mode::Server));
        assert_eq!(observation.inset(), 32);
    }

    #[test]
    fn a_kde_client_that_gets_client_side_shows_no_inset() {
        let lines = trace(&[
            "[1] wl_registry#2.global(7, \"org_kde_kwin_server_decoration_manager\", 1)",
            "[2]  -> wl_registry#2.bind(7, \"org_kde_kwin_server_decoration_manager\", 1, new id [unknown]#9)",
            "[3] wl_output#11.mode(3, 1280, 800, 60000)",
            "[4] org_kde_kwin_server_decoration#30.mode(1)",
            "[5] xdg_toplevel#29.configure(1280, 800, array[0])",
        ]);
        let observation = Observation::parse(&lines);
        assert_eq!(observation.protocol(), Protocol::Kde);
        assert_eq!(observation.mode(), Some(Mode::Client));
        assert_eq!(observation.inset(), 0);
    }

    #[test]
    fn a_single_inset_configure_is_still_a_titlebar() {
        // GTK4 asks for server-side before the toplevel exists, so the initial
        // configure already carries the inset and there is no full-size one.
        let lines = trace(&[
            "[1] wl_registry#2.global(7, \"org_kde_kwin_server_decoration_manager\", 1)",
            "[2] wl_output#11.mode(3, 1280, 800, 60000)",
            "[3] org_kde_kwin_server_decoration#27.mode(2)",
            "[4] xdg_toplevel#29.configure(1248, 800, array[0])",
        ]);
        assert_eq!(Observation::parse(&lines).inset(), 32);
    }

    #[test]
    fn configures_after_a_client_resize_are_ignored() {
        let lines = trace(&[
            "[1] wl_registry#2.global(7, \"org_kde_kwin_server_decoration_manager\", 1)",
            "[2] wl_output#11.mode(3, 1280, 800, 60000)",
            "[3] xdg_toplevel#29.configure(1280, 800, array[0])",
            "[4]  -> xdg_toplevel#29.resize(wl_seat#3, 105, 5)",
            "[5] xdg_toplevel#29.configure(1, 1, array[0])",
        ]);
        let observation = Observation::parse(&lines);
        assert_eq!(
            observation.inset(),
            0,
            "the 1x1 resize configure is not a decoration"
        );
    }

    #[test]
    fn an_egl_window_resize_is_not_a_toplevel_resize() {
        let lines = trace(&[
            "[1] wl_registry#2.global(7, \"org_kde_kwin_server_decoration_manager\", 1)",
            "[2] wl_output#11.mode(3, 1280, 800, 60000)",
            "[3]  -> wl_egl_window#5.resize(1280, 800)",
            "[4] org_kde_kwin_server_decoration#22.mode(2)",
            "[5] xdg_toplevel#21.configure(1248, 800, array[0])",
        ]);
        assert_eq!(Observation::parse(&lines).inset(), 32);
    }
}
