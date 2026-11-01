//! Harness: runs real applications under Malaysia and checks their decoration protocol use.
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SOCKET_TIMEOUT: Duration = Duration::from_secs(15);
const APP_TIMEOUT: Duration = Duration::from_secs(6);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Xdg,
    Kde,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Client,
    Server,
}

#[derive(Debug, Clone, Copy)]
struct Expectation {
    protocol: Protocol,
    mode: Option<Mode>,
    inset: i32,
}

struct Case {
    name: &'static str,
    variable: &'static str,
    fallback: &'static str,
    args: Vec<String>,
    env: Vec<(String, String)>,
    expectation: Expectation,
}

enum Outcome {
    Pass,
    Unverified(String),
    Fail(String),
}

#[derive(Debug, Default)]
struct Observation {
    xdg_bound: bool,
    kde_bound: bool,
    modes: Vec<Mode>,
    widths: Vec<i32>,
    output_width: Option<i32>,
}

impl Observation {
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

    fn protocol(&self) -> Protocol {
        if self.xdg_bound {
            Protocol::Xdg
        } else if self.kde_bound {
            Protocol::Kde
        } else {
            Protocol::None
        }
    }

    fn mode(&self) -> Option<Mode> {
        self.modes.last().copied()
    }

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

fn first_integer(body: &str) -> Option<i32> {
    nth_integer(body, 0)
}

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

    match run_native_decoration(host_display.as_deref()) {
        Ok(lines) => {
            for line in lines
                .iter()
                .filter(|line| line.contains("native_decoration:"))
            {
                println!("{line}");
            }
            println!("native app ids: PASS");
        }
        Err(reason) => failed.push(format!("native app ids: {reason}")),
    }

    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

fn run_mode_change(socket: &str, runtime: &Path) -> Result<Vec<String>, String> {
    let client = std::env::var_os("MALAYSIA_DECORATION_CLIENT")
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

fn run_native_decoration(host_display: Option<&str>) -> Result<Vec<String>, String> {
    let client = std::env::var_os("MALAYSIA_NATIVE_DECORATION")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("native_decoration")))
        })
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            "native_decoration not found next to the probe; run `cargo build -p sctk-probe` first"
                .to_string()
        })?;
    let binary = std::env::var_os("MALAYSIA_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("malaysia-compositor"));
    if !binary.is_file() {
        return Err(format!(
            "compositor binary not found at {}; run `cargo build -p malaysia-compositor` first",
            binary.display()
        ));
    }

    let mut lines = Vec::new();
    for (policy, scenarios) in [
        ("force_client", &["a"][..]),
        ("force_server", &["b", "c", "d"][..]),
    ] {
        let runtime = TempDir::create()?;
        let config = runtime.path().join("config");
        fs::create_dir_all(&config).map_err(|error| format!("creating config: {error}"))?;
        fs::write(
            config.join("window.domain"),
            format!("decorations = \"{policy}\"\n"),
        )
        .map_err(|error| format!("writing window.domain: {error}"))?;

        let mut compositor = ChildGuard::spawn(
            Command::new(&binary)
                .arg("--log-level=info")
                .env("XDG_RUNTIME_DIR", runtime.path())
                .env("MALAYSIA_CONFIG_DIR", &config),
            host_display,
        )?;
        let compositor_log = compositor.capture()?;
        let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;

        for scenario in scenarios {
            let mut child = ChildGuard::spawn(
                Command::new(&client)
                    .env("XDG_RUNTIME_DIR", runtime.path())
                    .env("WAYLAND_DISPLAY", &socket)
                    .env("MALAYSIA_NATIVE_SCENARIO", scenario)
                    .stdin(Stdio::null()),
                None,
            )?;
            let log = child.capture()?;
            let deadline = Instant::now() + APP_TIMEOUT;
            loop {
                if let Some(status) = child.poll()? {
                    let output = snapshot(&log);
                    lines.extend(output.iter().cloned());
                    if !status.success()
                        || !output
                            .iter()
                            .any(|line| line.contains(&format!("PASS scenario {scenario}")))
                    {
                        return Err(format!(
                            "scenario {scenario} failed ({status}); output:\n{}",
                            output.join("\n")
                        ));
                    }
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(format!("scenario {scenario} did not exit in time"));
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        if policy == "force_server" {
            let compositor_lines = snapshot(&compositor_log);
            if !compositor_lines
                .iter()
                .any(|line| line.contains("app propia sin modo de decoración"))
            {
                return Err(format!(
                    "scenario c produced no native-app warning; compositor log:\n{}",
                    compositor_lines.join("\n")
                ));
            }
        }
    }
    Ok(lines)
}

fn check(
    case: &Case,
    observation: &Observation,
    lines: &[String],
    exited: Option<std::process::ExitStatus>,
) -> Outcome {
    if lines.is_empty() {
        return Outcome::Unverified("the application produced no output".to_string());
    }
    if observation.widths.is_empty() {
        if !lines.iter().any(|line| line.contains("wl_display#")) {
            return Outcome::Unverified(
                "the application never connected to the Wayland display".to_string(),
            );
        }
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

fn cases(runtime: &Path) -> Vec<Case> {
    let qml = runtime.join("probe.qml");
    let _ = fs::write(
        &qml,
        "import QtQuick\nimport QtQuick.Window\nWindow { width: 320; height: 200; visible: true }\n",
    );
    let mut qt6 = vec![(
        "QML_IMPORT_PATH".to_string(),
        std::env::var("MALAYSIA_APP_QT6_QML_IMPORT_PATH").unwrap_or_default(),
    )];
    qt6.retain(|(_, value)| !value.is_empty());

    vec![
        Case {
            name: "nautilus",
            variable: "MALAYSIA_APP_NAUTILUS",
            fallback: "nautilus",
            args: vec!["--no-desktop".to_string()],
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "gtk3-demo",
            variable: "MALAYSIA_APP_GTK3_DEMO",
            fallback: "gtk3-demo",
            args: Vec::new(),
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Client),
                inset: 0,
            },
        },
        Case {
            name: "gtk3-widget-factory",
            variable: "MALAYSIA_APP_GTK3_WIDGET_FACTORY",
            fallback: "gtk3-widget-factory",
            args: Vec::new(),
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Client),
                inset: 0,
            },
        },
        Case {
            name: "gtk3-plain",
            variable: "MALAYSIA_APP_GTK3_PLAIN",
            fallback: "gtk3_plain",
            args: Vec::new(),
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "gtk4-demo",
            variable: "MALAYSIA_APP_GTK4_DEMO",
            fallback: "gtk4-demo",
            args: Vec::new(),
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: None,
                inset: 0,
            },
        },
        Case {
            name: "gtk4-widget-factory",
            variable: "MALAYSIA_APP_GTK4_WIDGET_FACTORY",
            fallback: "gtk4-widget-factory",
            args: Vec::new(),
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Client),
                inset: 0,
            },
        },
        Case {
            name: "gtk4-plain",
            variable: "MALAYSIA_APP_GTK4_PLAIN",
            fallback: "gtk4_plain",
            args: Vec::new(),
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::Kde,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "qt6",
            variable: "MALAYSIA_APP_QT6",
            fallback: "qml",
            args: vec![qml.to_string_lossy().into_owned()],
            env: qt6,
            expectation: Expectation {
                protocol: Protocol::Xdg,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "qt5",
            variable: "MALAYSIA_APP_QT5",
            fallback: "qmlscene",
            args: vec![qml.to_string_lossy().into_owned()],
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::Xdg,
                mode: Some(Mode::Server),
                inset: TITLEBAR,
            },
        },
        Case {
            name: "onlyoffice",
            variable: "MALAYSIA_APP_ONLYOFFICE",
            fallback: "onlyoffice-desktopeditors",
            args: Vec::new(),
            env: Vec::new(),
            expectation: Expectation {
                protocol: Protocol::None,
                mode: None,
                inset: 0,
            },
        },
    ]
}

fn resolve(variable: &str, fallback: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(variable) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    which(fallback)
}

fn which(command: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
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

struct TempDir(PathBuf);

impl TempDir {
    fn create() -> Result<Self, String> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("clock is before the epoch: {error}"))?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "malaysia-decoration-{}-{unique}",
            std::process::id()
        ));
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
