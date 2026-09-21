//! Exercises the client-initiated move and resize path the way a real gesture
//! does.
//!
//! A GTK headerbar drag does not ask for a move on the button press: the
//! gesture waits for the pointer to pass a threshold, and a focus change can
//! happen before the request. This client repeats that shape:
//!
//! 1. on the first injected press, remember the serial and wait;
//! 2. after three motions, map a second window (the compositor focuses it),
//!    then send `move` and `resize` with the original press serial, both while
//!    the button is held;
//! 3. after the compositor releases the button, send `move` and `resize` with
//!    the same serial again (now released), and both with a fabricated serial;
//! 4. the valid resize makes the compositor send a configure carrying the
//!    `resizing` state, which is the success signal.
//!
//! The released and fabricated requests must be rejected; the valid ones must
//! be accepted even though a raise, a focus change and a keyboard enter
//! happened in between.
//!
//! `NEPTUNE_TEST_INPUT=1` makes the compositor press once, on the first window
//! that maps, and hold the button; the motions and the release come from the
//! same scripted input. Run from the dev shell, or `./dev.sh move`.

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

/// A serial no compositor could have issued for this session's presses.
const FABRICATED_SERIAL: u32 = 0xdead_0001;
/// Buffer side, in pixels.
const SIDE: i32 = 64;
/// How many motions the gesture waits for before it asks for a move.
const THRESHOLD_MOTIONS: usize = 3;
/// How long to wait for the resizing configure.
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let seat_state = SeatState::new(&globals, &qh);
    let mut pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    let (buffer, canvas) = pool
        .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
        .expect("failed to create a buffer");
    canvas.fill(0x40);
    window.attach(Some(buffer.wl_buffer()), 0, 0);
    window.commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        seat_state,
        seat: None,
        pointer: None,
        compositor,
        xdg_shell,
        pool,
        window,
        second: None,
        _first_buffer: buffer,
        _second_buffer: None,
        press_serial: None,
        motions: 0,
        requested: false,
        saw_resizing: false,
        failed: false,
        loop_signal: event_loop.get_signal(),
    };

    let start = std::time::Instant::now();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            if app.saw_resizing {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if start.elapsed() >= TIMEOUT {
                eprintln!("move_client: FAIL no resizing configure within {TIMEOUT:?}");
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

    if app.failed {
        std::process::exit(1);
    }
    println!("move_client: PASS");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    seat_state: SeatState,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    compositor: CompositorState,
    xdg_shell: XdgShell,
    pool: SlotPool,
    window: Window,
    second: Option<Window>,
    _first_buffer: Buffer,
    _second_buffer: Option<Buffer>,
    press_serial: Option<u32>,
    motions: usize,
    requested: bool,
    saw_resizing: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    /// Maps a second window, so the compositor raises and focuses it between
    /// the press and the requests.
    fn map_second_window(&mut self, qh: &QueueHandle<Self>) {
        if self.second.is_some() {
            return;
        }
        let surface = self.compositor.create_surface(qh);
        let window = self
            .xdg_shell
            .create_window(surface, WindowDecorations::RequestServer, qh);
        let (buffer, canvas) = self
            .pool
            .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
            .expect("failed to create a second buffer");
        canvas.fill(0x20);
        window.attach(Some(buffer.wl_buffer()), 0, 0);
        window.commit();
        println!("move_client: mapped a second window to take focus");
        self.second = Some(window);
        self._second_buffer = Some(buffer);
    }

    /// Sends move and resize with `serial`, the requests the gesture makes.
    fn request_move_and_resize(&self, serial: u32, note: &str) {
        let Some(seat) = &self.seat else {
            return;
        };
        self.window.move_(seat, serial);
        self.window.resize(seat, serial, ResizeEdge::BottomRight);
        println!("move_client: sent move/resize with serial {serial} {note}");
    }

    /// Sends move and resize with the fabricated serial.
    fn send_fabricated(&self) {
        self.request_move_and_resize(FABRICATED_SERIAL, "(fabricated)");
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
        configure: WindowConfigure,
        _: u32,
    ) {
        // Acknowledge and commit, so the window maps and later configures flow.
        window.commit();
        if configure.is_resizing() {
            println!("move_client: received a configure with the resizing state");
            self.saw_resizing = true;
        }
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
        qh: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            match event.kind {
                PointerEventKind::Press { serial, .. } => {
                    if self.press_serial.is_none() {
                        self.press_serial = Some(serial);
                        println!("move_client: got the press serial {serial}");
                    }
                }
                PointerEventKind::Motion { .. } => {
                    let Some(serial) = self.press_serial else {
                        continue;
                    };
                    if self.requested {
                        continue;
                    }
                    self.motions += 1;
                    if self.motions < THRESHOLD_MOTIONS {
                        continue;
                    }
                    self.requested = true;
                    self.map_second_window(qh);
                    self.request_move_and_resize(
                        serial,
                        "after a threshold and a focus change, while held",
                    );
                    self.send_fabricated();
                }
                PointerEventKind::Release { .. } => {
                    if let Some(serial) = self.press_serial {
                        self.request_move_and_resize(serial, "after the release");
                    }
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

delegate_compositor!(App);
delegate_output!(App);
delegate_pointer!(App);
delegate_registry!(App);
delegate_seat!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
