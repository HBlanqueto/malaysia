use std::time::Duration;

use smithay_client_toolkit::{
    compositor::CompositorState,
    delegate_compositor, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::OutputState,
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, QueueHandle},
        protocols::xdg::shell::client::xdg_toplevel::ResizeEdge,
    },
    registry::RegistryState,
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
        Shm,
    },
};
use wayland_client::protocol::{wl_pointer::WlPointer, wl_seat::WlSeat, wl_shm};

const SIDE: i32 = 64;
const THRESHOLD_MOTIONS: usize = 3;
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let expect = parse_points("MALAYSIA_EXPECT_HOVER");
    if expect.is_empty() {
        eprintln!("hover_client: FAIL MALAYSIA_EXPECT_HOVER is not set");
        std::process::exit(1);
    }
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let seat_state = SeatState::new(&globals, &qh);
    let mut pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestClient, &qh);
    let (buffer, canvas) = pool
        .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
        .expect("failed to create a buffer");
    canvas.fill(0x40);
    window.commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        seat_state,
        seat: None,
        pointer: None,
        window,
        buffer,
        attached: false,
        press_serial: None,
        motions: 0,
        requested: false,
        released: false,
        seen: 0,
        expect,
        failed: false,
        loop_signal: event_loop.get_signal(),
    };

    let start = std::time::Instant::now();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            if app.seen == app.expect.len() {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if start.elapsed() >= TIMEOUT {
                eprintln!(
                    "hover_client: FAIL no complete hover within {TIMEOUT:?}: saw {} of {} positions",
                    app.seen,
                    app.expect.len()
                );
                app.failed = true;
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(50))
        })
        .expect("failed to insert the timer");

    event_loop
        .run(None, &mut app, |_| {})
        .expect("event loop failed");

    if app.failed || app.seen != app.expect.len() {
        std::process::exit(1);
    }
    println!("hover_client: PASS");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    seat_state: SeatState,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    window: Window,
    buffer: Buffer,
    attached: bool,
    press_serial: Option<u32>,
    motions: usize,
    requested: bool,
    released: bool,
    seen: usize,
    expect: Vec<(f64, f64)>,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn request_move_and_resize(&self, serial: u32) {
        let Some(seat) = &self.seat else {
            return;
        };
        self.window.move_(seat, serial);
        self.window.resize(seat, serial, ResizeEdge::BottomRight);
        println!("hover_client: sent move/resize with serial {serial}");
    }

    fn check(&mut self, kind: &str, position: (f64, f64)) {
        println!("hover_client: {kind} x={} y={}", position.0, position.1);
        let Some(expected) = self.expect.get(self.seen) else {
            return;
        };
        if (position.0 - expected.0).abs() > 0.01 || (position.1 - expected.1).abs() > 0.01 {
            eprintln!(
                "hover_client: FAIL the {kind} arrived at ({}, {}), expected ({}, {})",
                position.0, position.1, expected.0, expected.1
            );
            self.failed = true;
            self.loop_signal.stop();
            return;
        }
        self.seen += 1;
        if self.seen == self.expect.len() {
            self.loop_signal.stop();
        }
    }
}

sctk_probe::noop_compositor_handler!(App);

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        window: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
        if !self.attached {
            self.attached = true;
            window.attach(Some(self.buffer.wl_buffer()), 0, 0);
        }
        window.commit();
    }
}

sctk_probe::noop_output_handler!(App);

sctk_probe::shm_and_registry!(App);

impl SeatHandler for App {
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
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            match event.kind {
                PointerEventKind::Press { serial, .. } => {
                    if self.press_serial.is_none() {
                        self.press_serial = Some(serial);
                        println!("hover_client: got the press serial {serial}");
                    }
                }
                PointerEventKind::Motion { .. } => {
                    if self.released {
                        self.check("motion", event.position);
                    } else if let Some(serial) = self.press_serial {
                        self.motions += 1;
                        if self.motions >= THRESHOLD_MOTIONS && !self.requested {
                            self.requested = true;
                            self.request_move_and_resize(serial);
                        }
                    }
                }
                PointerEventKind::Release { .. } => {
                    self.released = true;
                }
                PointerEventKind::Enter { .. } if self.released => {
                    self.check("enter", event.position);
                }
                _ => {}
            }
        }
    }
}

fn parse_points(name: &str) -> Vec<(f64, f64)> {
    let Ok(value) = std::env::var(name) else {
        return Vec::new();
    };
    value
        .split(';')
        .filter_map(|point| {
            let (x, y) = point.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        })
        .collect()
}

delegate_compositor!(App);
delegate_output!(App);
delegate_pointer!(App);
delegate_registry!(App);
delegate_seat!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
