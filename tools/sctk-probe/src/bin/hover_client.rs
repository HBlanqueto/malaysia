//! Asserts the surface-local coordinates a hovered client receives.
//!
//! Yuki's test backend, with `YUKI_TEST_INPUT=1`, presses on the first
//! mapped window, follows the press with motions and releases. This client asks
//! for client-side decorations, so its surface origin is its window origin, and
//! sends `move` and `resize` with the press serial: the move makes the
//! compositor's scripted drag move the window to a non-zero origin, and the
//! resize makes the compositor release the button. It then hovers the pointer
//! with no button held over the global points in `YUKI_TEST_HOVER`.
//!
//! The client asserts that the `wl_pointer.enter` and `wl_pointer.motion` it
//! receives for that hover carry the surface-local positions in
//! `YUKI_EXPECT_HOVER` (an `x,y` list separated by `;`). A compositor that
//! hands Smithay the surface-local point where it expects the surface origin
//! makes every client read the window origin instead, which fails here.
//!
//! Run it through `./dev.sh move`; `move_probe` sets both variables and checks
//! the positions again from the printed lines.

use std::time::Duration;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, QueueHandle},
        protocols::xdg::shell::client::xdg_toplevel::ResizeEdge,
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
    wl_output::{self, WlOutput},
    wl_pointer::WlPointer,
    wl_seat::WlSeat,
    wl_shm,
    wl_surface::WlSurface,
};

/// Buffer side, in pixels.
const SIDE: i32 = 64;
/// How many motions the gesture waits for before it asks for a move.
const THRESHOLD_MOTIONS: usize = 3;
/// How long to wait for the scripted hover.
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let expect = parse_points("YUKI_EXPECT_HOVER");
    if expect.is_empty() {
        eprintln!("hover_client: FAIL YUKI_EXPECT_HOVER is not set");
        std::process::exit(1);
    }
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let seat_state = SeatState::new(&globals, &qh);
    let mut pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let surface = compositor.create_surface(&qh);
    // Client-side decorations keep the insets at zero, so a surface-local
    // position is the global position minus the window origin, with no
    // compositor-chosen chrome to account for.
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestClient, &qh);
    let (buffer, canvas) = pool
        .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
        .expect("failed to create a buffer");
    canvas.fill(0x40);
    // Commit without a buffer first: the initial xdg_surface.configure must
    // arrive before the client attaches anything.
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
    /// Sends the move and resize a drag gesture makes, both with the press
    /// serial: the move drives the scripted drag, the resize the release.
    fn request_move_and_resize(&self, serial: u32) {
        let Some(seat) = &self.seat else {
            return;
        };
        self.window.move_(seat, serial);
        self.window.resize(seat, serial, ResizeEdge::BottomRight);
        println!("hover_client: sent move/resize with serial {serial}");
    }

    /// Records a post-release position and asserts it against the expectation.
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

impl CompositorHandler for App {
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
        // Acknowledge and commit, so the window maps and later configures flow.
        window.commit();
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

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

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState,];
}

/// Parses the `;`-separated `x,y` points of an environment variable.
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
