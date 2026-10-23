//! Asserts `xdg-activation-v1` with two clients.
//!
//! The parent (client A) maps first, so it is focused and receives the
//! compositor's scripted press. While focused it creates activation tokens:
//! one to hand to the child (client B), two to restore its own focus between
//! cases, one without a serial, and one that is left to expire.
//!
//! The child maps second and takes focus. A restores itself, then B activates
//! itself with the token A passed it and must receive keyboard focus. The
//! refused cases each run after A has restored its focus: a token without a
//! serial, a token requested from B's unfocused surface, and a token older
//! than 30 seconds must not move focus to B.
//!
//! Both clients are this same binary: the parent spawns it with
//! `ACTIVATION_CHILD=1` and talks to it over stdin/stdout.
//!
//! Run against a compositor started with `YUKI_TEST_INPUT=1`, or
//! `./dev.sh activation`.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, Dispatch, Proxy, QueueHandle},
        protocols::xdg::activation::v1::client::{
            xdg_activation_token_v1::{self, XdgActivationTokenV1},
            xdg_activation_v1::XdgActivationV1,
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use wayland_client::protocol::{
    wl_keyboard::{self, WlKeyboard},
    wl_output::{self, WlOutput},
    wl_pointer::WlPointer,
    wl_seat::WlSeat,
    wl_shm,
    wl_surface::WlSurface,
};

/// Buffer side, in pixels.
const SIDE: i32 = 64;
/// How long an invalid token is given to (not) move focus.
const REFUSAL_WAIT: Duration = Duration::from_secs(1);
/// A token is only valid for 30 s; wait a little longer before using it.
const TOKEN_EXPIRY: Duration = Duration::from_secs(31);
/// Overall limit; the expired case alone waits 31 s.
const TOTAL_TIMEOUT: Duration = Duration::from_secs(60);
/// Tokens A creates while focused before B maps.
const TOKEN_LABELS: [&str; 5] = ["valid", "restore1", "restore2", "noserial", "expired"];

fn main() {
    if std::env::var_os("ACTIVATION_CHILD").is_some() {
        run_child();
    } else {
        run_parent();
    }
}

// ---------------------------------------------------------------------------
// Parent (client A)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Startup,
    WaitingPress,
    WaitingTokens,
    WaitingMap,
    RestoringAfterMap,
    ActivatingB,
    RestoringAfterValid,
    TestingNoSerial,
    TestingUnfocusedSurface,
    WaitingExpiry,
    TestingExpired,
}

struct Parent {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    seat_state: SeatState,
    activation: XdgActivationV1,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    window: Window,
    buffer: Buffer,
    attached: bool,
    press_serial: Option<u32>,
    a_focused: bool,
    tokens: HashMap<String, String>,
    expired_created: Option<Instant>,
    phase: Phase,
    phase_at: Instant,
    child_ready: bool,
    child_mapped: bool,
    b_focused: bool,
    child: Option<ChildProcess>,
    lines: Receiver<String>,
    started: Instant,
    failed: bool,
    done: bool,
    loop_signal: calloop::LoopSignal,
}

struct ChildProcess {
    child: Child,
    stdin: ChildStdin,
}

fn run_parent() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<Parent>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let activation = globals
        .bind::<XdgActivationV1, Parent, ()>(&qh, 1..=1, ())
        .expect("no xdg_activation_v1");
    let seat_state = SeatState::new(&globals, &qh);
    let mut pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestClient, &qh);
    let (buffer, canvas) = pool
        .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
        .expect("failed to create a buffer");
    canvas.fill(0x30);
    window.commit();

    let (child, lines) = spawn_child();

    let mut app = Parent {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        seat_state,
        activation,
        seat: None,
        pointer: None,
        keyboard: None,
        window,
        buffer,
        attached: false,
        press_serial: None,
        a_focused: false,
        tokens: HashMap::new(),
        expired_created: None,
        phase: Phase::Startup,
        phase_at: Instant::now(),
        child_ready: false,
        child_mapped: false,
        b_focused: false,
        child: Some(child),
        lines,
        started: Instant::now(),
        failed: false,
        done: false,
        loop_signal: event_loop.get_signal(),
    };

    let timer_qh = qh.clone();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            app.tick(&timer_qh);
            if app.done || app.failed {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if app.started.elapsed() >= TOTAL_TIMEOUT {
                app.fail("the scenario did not finish in time");
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(20))
        })
        .expect("failed to insert the timer");

    event_loop
        .run(None, &mut app, |_| {})
        .expect("event loop failed");

    if let Some(mut child) = app.child.take() {
        let _ = child.stdin.write_all(b"quit\n");
        let _ = child.child.kill();
        let _ = child.child.wait();
    }
    if app.failed || !app.done {
        std::process::exit(1);
    }
    println!("activation: PASS");
}

fn spawn_child() -> (ChildProcess, Receiver<String>) {
    let exe = std::env::current_exe().expect("cannot locate the probe binary");
    let mut child = Command::new(exe)
        .env("ACTIVATION_CHILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("could not spawn the second activation client");
    let stdout = child.stdout.take().expect("no child stdout");
    let stdin = child.stdin.take().expect("no child stdin");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (ChildProcess { child, stdin }, rx)
}

impl Parent {
    fn tick(&mut self, qh: &QueueHandle<Self>) {
        while let Ok(line) = self.lines.try_recv() {
            self.handle_child_line(&line);
        }

        match self.phase {
            Phase::Startup => {
                if self.child_ready && self.attached {
                    // A maps, so it is the first window and receives the press.
                    self.window.commit();
                    self.phase = Phase::WaitingPress;
                    self.phase_at = Instant::now();
                }
            }
            Phase::WaitingPress => {
                if self.press_serial.is_some() && self.a_focused {
                    self.request_tokens(qh);
                }
            }
            Phase::WaitingTokens => {
                if TOKEN_LABELS
                    .iter()
                    .all(|label| self.tokens.contains_key(*label))
                {
                    println!("activation: five tokens created while A was focused");
                    self.send_child("map");
                    self.phase = Phase::WaitingMap;
                    self.phase_at = Instant::now();
                }
            }
            Phase::WaitingMap => {
                if self.child_mapped && self.b_focused {
                    self.activate("restore1");
                    self.phase = Phase::RestoringAfterMap;
                    self.phase_at = Instant::now();
                }
            }
            Phase::RestoringAfterMap => {
                if self.a_focused {
                    println!("activation: A restored its focus");
                    self.send_activate("valid");
                    self.phase = Phase::ActivatingB;
                    self.phase_at = Instant::now();
                }
            }
            Phase::ActivatingB => {
                if self.b_focused {
                    println!("activation: PASS valid token activated B");
                    self.activate("restore2");
                    self.phase = Phase::RestoringAfterValid;
                    self.phase_at = Instant::now();
                }
            }
            Phase::RestoringAfterValid => {
                if self.a_focused {
                    self.send_activate("noserial");
                    self.phase = Phase::TestingNoSerial;
                    self.phase_at = Instant::now();
                }
            }
            Phase::TestingNoSerial => {
                if self.phase_at.elapsed() >= REFUSAL_WAIT {
                    if self.a_focused && !self.b_focused {
                        println!("activation: PASS token without serial did not activate B");
                        let serial = self.press_serial.expect("the press serial");
                        self.send_child(&format!("token {serial}"));
                        self.phase = Phase::TestingUnfocusedSurface;
                        self.phase_at = Instant::now();
                    } else {
                        self.fail("a token without a serial moved focus to B");
                    }
                }
            }
            Phase::TestingUnfocusedSurface => {
                if self.phase_at.elapsed() >= REFUSAL_WAIT {
                    if self.a_focused && !self.b_focused {
                        println!("activation: PASS unfocused-surface token did not activate B");
                        self.phase = Phase::WaitingExpiry;
                        self.phase_at = Instant::now();
                    } else {
                        self.fail("a token from an unfocused surface moved focus to B");
                    }
                }
            }
            Phase::WaitingExpiry => {
                let expired = self
                    .expired_created
                    .is_some_and(|created| created.elapsed() >= TOKEN_EXPIRY);
                if expired {
                    self.send_activate("expired");
                    self.phase = Phase::TestingExpired;
                    self.phase_at = Instant::now();
                }
            }
            Phase::TestingExpired => {
                if self.phase_at.elapsed() >= REFUSAL_WAIT {
                    if self.a_focused && !self.b_focused {
                        println!("activation: PASS expired token did not activate B");
                        self.done = true;
                    } else {
                        self.fail("an expired token moved focus to B");
                    }
                }
            }
        }
    }

    fn handle_child_line(&mut self, line: &str) {
        match line {
            "ready" => self.child_ready = true,
            "mapped" => self.child_mapped = true,
            "focus 1" => self.b_focused = true,
            "focus 0" => self.b_focused = false,
            "activate done" | "token done" => {}
            other
                if other.starts_with("activate done ") || other.starts_with("token requested") => {}
            other => eprintln!("activation: unexpected message from B: {other}"),
        }
    }

    fn request_tokens(&mut self, qh: &QueueHandle<Self>) {
        let serial = self.press_serial.expect("the press serial");
        for label in TOKEN_LABELS {
            let token = self.activation.get_activation_token(qh, label.to_string());
            token.set_surface(self.window.wl_surface());
            if label != "noserial" {
                if let Some(seat) = &self.seat {
                    token.set_serial(serial, seat);
                }
            }
            token.commit();
        }
        self.phase = Phase::WaitingTokens;
        self.phase_at = Instant::now();
    }

    fn activate(&self, label: &str) {
        let token = self
            .tokens
            .get(label)
            .unwrap_or_else(|| panic!("no token labelled {label}"));
        self.activation
            .activate(token.clone(), self.window.wl_surface());
        println!("activation: activated A with the {label} token");
    }

    fn send_activate(&mut self, label: &str) {
        let token = self
            .tokens
            .get(label)
            .unwrap_or_else(|| panic!("no token labelled {label}"))
            .clone();
        self.send_child(&format!("activate {label} {token}"));
    }

    fn send_child(&mut self, line: &str) {
        let Some(child) = self.child.as_mut() else {
            self.fail("client B is gone");
            return;
        };
        if writeln!(child.stdin, "{line}")
            .and_then(|_| child.stdin.flush())
            .is_err()
        {
            self.fail("could not write to client B");
        }
    }

    fn fail(&mut self, message: &str) {
        eprintln!("activation: FAIL {message}");
        self.failed = true;
    }
}

impl CompositorHandler for Parent {
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }
}

impl WindowHandler for Parent {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
        if self.attached {
            return;
        }
        self.attached = true;
        self.window.attach(Some(self.buffer.wl_buffer()), 0, 0);
        // Mapping waits for the child to be ready, so A is the first window.
        if self.child_ready {
            self.window.commit();
            self.phase = Phase::WaitingPress;
            self.phase_at = Instant::now();
        }
    }
}

impl OutputHandler for Parent {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

impl ShmHandler for Parent {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl SeatHandler for Parent {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seat = Some(seat);
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(qh, ()));
        }
        self.seat = Some(seat);
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            self.pointer = None;
        }
        if capability == Capability::Keyboard {
            self.keyboard = None;
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl PointerHandler for Parent {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if let PointerEventKind::Press { serial, .. } = event.kind {
                if self.press_serial.is_none() {
                    self.press_serial = Some(serial);
                    println!("activation: A got the press serial {serial}");
                }
            }
        }
    }
}

impl Dispatch<WlKeyboard, ()> for Parent {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { surface, .. }
                if surface.id() == state.window.wl_surface().id() =>
            {
                state.a_focused = true;
                println!("activation: A focused");
            }
            wl_keyboard::Event::Leave { surface, .. }
                if surface.id() == state.window.wl_surface().id() =>
            {
                state.a_focused = false;
                println!("activation: A unfocused");
            }
            _ => {}
        }
    }
}

impl Dispatch<XdgActivationV1, ()> for Parent {
    fn event(
        _: &mut Self,
        _: &XdgActivationV1,
        _: <XdgActivationV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<XdgActivationTokenV1, String> for Parent {
    fn event(
        state: &mut Self,
        _: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        label: &String,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            if label == "expired" {
                state.expired_created = Some(Instant::now());
            }
            println!("activation: token {label} is ready");
            state.tokens.insert(label.clone(), token);
        }
    }
}

impl ProvidesRegistryState for Parent {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState,];
}

delegate_compositor!(Parent);
delegate_output!(Parent);
delegate_pointer!(Parent);
delegate_registry!(Parent);
delegate_seat!(Parent);
delegate_shm!(Parent);
delegate_xdg_shell!(Parent);
delegate_xdg_window!(Parent);

// ---------------------------------------------------------------------------
// Child (client B)
// ---------------------------------------------------------------------------

struct ChildApp {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    seat_state: SeatState,
    activation: XdgActivationV1,
    seat: Option<WlSeat>,
    keyboard: Option<WlKeyboard>,
    window: Window,
    pool: Option<SlotPool>,
    buffer: Option<Buffer>,
    configured: bool,
    mapped: bool,
    focused: bool,
    pending_self: bool,
    lines: Receiver<String>,
    failed: bool,
    done: bool,
    loop_signal: calloop::LoopSignal,
}

fn run_child() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<ChildApp>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let activation = globals
        .bind::<XdgActivationV1, ChildApp, ()>(&qh, 1..=1, ())
        .expect("no xdg_activation_v1");
    let seat_state = SeatState::new(&globals, &qh);

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestClient, &qh);
    window.commit();

    let (tx, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut app = ChildApp {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        seat_state,
        activation,
        seat: None,
        keyboard: None,
        window,
        pool: None,
        buffer: None,
        configured: false,
        mapped: false,
        focused: false,
        pending_self: false,
        lines,
        failed: false,
        done: false,
        loop_signal: event_loop.get_signal(),
    };

    let timer_qh = qh.clone();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            app.tick(&timer_qh);
            if app.done {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(20))
        })
        .expect("failed to insert the timer");

    event_loop
        .run(None, &mut app, |_| {})
        .expect("event loop failed");

    if app.failed {
        std::process::exit(1);
    }
}

impl ChildApp {
    fn tick(&mut self, qh: &QueueHandle<Self>) {
        while let Ok(line) = self.lines.try_recv() {
            match line.as_str() {
                "quit" => {
                    self.done = true;
                    return;
                }
                "map" => self.map(),
                other if other.starts_with("activate ") => {
                    let (label, token) = other
                        .split_once(' ')
                        .and_then(|(_, rest)| rest.split_once(' '))
                        .expect("bad activate command");
                    self.activation
                        .activate(token.to_string(), self.window.wl_surface());
                    println!("activate done {label}");
                }
                other if other.starts_with("token ") => {
                    let serial = other
                        .split_whitespace()
                        .nth(1)
                        .and_then(|value| value.parse().ok())
                        .expect("bad token serial");
                    self.request_self_token(qh, serial);
                }
                other => eprintln!("activation: B got an unknown command: {other}"),
            }
        }
    }

    fn map(&mut self) {
        if self.mapped {
            return;
        }
        let Some(buffer) = &self.buffer else {
            self.failed = true;
            return;
        };
        self.window.attach(Some(buffer.wl_buffer()), 0, 0);
        self.window.commit();
        self.mapped = true;
        println!("mapped");
    }

    fn request_self_token(&mut self, qh: &QueueHandle<Self>, serial: u32) {
        let Some(seat) = &self.seat else {
            self.failed = true;
            return;
        };
        let token = self.activation.get_activation_token(qh, "self".to_string());
        token.set_serial(serial, seat);
        token.set_surface(self.window.wl_surface());
        token.commit();
        self.pending_self = true;
        println!("token requested from B's unfocused surface");
    }
}

impl CompositorHandler for ChildApp {
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }
}

impl WindowHandler for ChildApp {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.done = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
        if self.configured {
            return;
        }
        self.configured = true;
        let mut pool =
            SlotPool::new((SIDE * SIDE * 4) as usize, &self.shm).expect("no shm pool for client B");
        let (buffer, canvas) = pool
            .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
            .expect("no buffer for client B");
        canvas.fill(0x60);
        self.pool = Some(pool);
        self.buffer = Some(buffer);
        self.window.commit();
        println!("ready");
    }
}

impl OutputHandler for ChildApp {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

impl ShmHandler for ChildApp {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl SeatHandler for ChildApp {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seat = Some(seat);
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(qh, ()));
        }
        self.seat = Some(seat);
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            self.keyboard = None;
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl Dispatch<WlKeyboard, ()> for ChildApp {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { surface, .. }
                if surface.id() == state.window.wl_surface().id() =>
            {
                state.focused = true;
                println!("focus 1");
            }
            wl_keyboard::Event::Leave { surface, .. }
                if surface.id() == state.window.wl_surface().id() =>
            {
                state.focused = false;
                println!("focus 0");
            }
            _ => {}
        }
    }
}

impl Dispatch<XdgActivationV1, ()> for ChildApp {
    fn event(
        _: &mut Self,
        _: &XdgActivationV1,
        _: <XdgActivationV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<XdgActivationTokenV1, String> for ChildApp {
    fn event(
        state: &mut Self,
        _: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _label: &String,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            if state.pending_self {
                state.pending_self = false;
                state.activation.activate(token, state.window.wl_surface());
                println!("token done");
            }
        }
    }
}

impl ProvidesRegistryState for ChildApp {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState,];
}

delegate_compositor!(ChildApp);
delegate_output!(ChildApp);
delegate_registry!(ChildApp);
delegate_seat!(ChildApp);
delegate_shm!(ChildApp);
delegate_xdg_shell!(ChildApp);
delegate_xdg_window!(ChildApp);
