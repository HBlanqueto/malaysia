use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SOCKET_TIMEOUT: Duration = Duration::from_secs(15);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(20);
const NAUTILUS_TIMEOUT: Duration = Duration::from_secs(12);
const DRAG: (f64, f64) = (60.0, 40.0);
const HOVER: [(f64, f64); 3] = [(-10.0, -10.0), (32.0, 32.0), (40.0, 40.0)];
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
    let binary = std::env::var_os("MALAYSIA_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("target/debug/malaysia-compositor"));
    if !binary.is_file() {
        return Err(format!(
            "compositor binary not found at {}; run `cargo build -p malaysia-compositor` first",
            binary.display()
        ));
    }
    let client = probe_binary(repo, "MALAYSIA_MOVE_BIN", "move_client");
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

fn run_move_case(binary: &Path, client: &Path) -> Result<(f64, f64), String> {
    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;
    let mut compositor = ChildGuard::spawn(
        Command::new(binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("MALAYSIA_TEST_INPUT", "1")
            .env("MALAYSIA_TEST_DRAG", format!("{},{}", DRAG.0, DRAG.1)),
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

fn run_hover_case(binary: &Path, repo: &Path, centre: (f64, f64)) -> Result<(), String> {
    let client = probe_binary(repo, "MALAYSIA_HOVER_BIN", "hover_client");
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
            .env("MALAYSIA_TEST_INPUT", "1")
            .env("MALAYSIA_TEST_DRAG", format!("{},{}", DRAG.0, DRAG.1))
            .env("MALAYSIA_TEST_HOVER", &hover),
        host_display.as_deref(),
    )?;
    let compositor_log = compositor.capture()?;
    let socket = wait_for_socket(runtime.path(), &mut compositor, &compositor_log)?;

    let expect = points([HOVER[1], HOVER[2]]);
    let mut hover_client = ChildGuard::spawn(
        Command::new(&client)
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WAYLAND_DISPLAY", &socket)
            .env("MALAYSIA_EXPECT_HOVER", &expect)
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

fn points(list: impl IntoIterator<Item = (f64, f64)>) -> String {
    list.into_iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(";")
}

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

fn run_nautilus_case(binary: &Path) -> Result<(), String> {
    let Some(nautilus) = resolve("MALAYSIA_APP_NAUTILUS", "nautilus") else {
        println!("move_probe: nautilus UNVERIFIED (not installed)");
        return Ok(());
    };
    let runtime = TempDir::create()?;
    let host_display = host_wayland_display()?;
    let mut compositor = ChildGuard::spawn(
        Command::new(binary)
            .arg("--log-level=info")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("MALAYSIA_TEST_INPUT", "1")
            .env("MALAYSIA_TEST_PRESS_AT", "0.5,0.05")
            .env("MALAYSIA_TEST_DRAG", format!("{},{}", DRAG.0, DRAG.1)),
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

fn field(line: &str, name: &str) -> Option<f64> {
    let rest = line.split_once(&format!("{name}="))?.1;
    rest.split_whitespace().next()?.parse().ok()
}

fn number_after(line: &str, marker: &str) -> Option<u32> {
    let rest = line.split_once(marker)?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn stripped(sink: &Sink) -> Vec<String> {
    snapshot(sink).iter().map(|line| strip_ansi(line)).collect()
}

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
