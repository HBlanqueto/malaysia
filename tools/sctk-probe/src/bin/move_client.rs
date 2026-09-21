//! Exercises the client-initiated move and resize path.
//!
//! A Wayland client can ask the compositor to move or resize a toplevel with
//! `xdg_toplevel.move` and `xdg_toplevel.resize`, but only with the serial of a
//! pointer button press it received. `NEPTUNE_TEST_INPUT=1` makes the compositor
//! press once when the window maps and once more when it accepts the move, so
//! this client can obtain two valid serials without real input.
//!
//! The sequence, all asserted by exit code and the harness:
//!
//! 1. on the first injected press, send `move` with that serial;
//! 2. on the second, send `resize` (bottom-right) with that serial, then send
//!    `move` and `resize` again with a fabricated serial;
//! 3. the valid resize makes the compositor send a configure carrying the
//!    `resizing` state, which this client observes as success.
//!
//! If no resizing configure arrives within the deadline, it exits non-zero.

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
    shm::{slot::SlotPool, Shm, ShmHandler},
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

    // The buffer must outlive the run, or the surface loses its content.
    let _keep_buffer = buffer;

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        seat_state,
        seat: None,
        pointer: None,
        window,
        presses: 0,
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
    window: Window,
    presses: usize,
    saw_resizing: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    /// Sends move and resize with the fabricated serial, after the valid ones.
    fn send_stale(&self) {
        let Some(seat) = &self.seat else {
            return;
        };
        self.window.move_(seat, FABRICATED_SERIAL);
        self.window
            .resize(seat, FABRICATED_SERIAL, ResizeEdge::TopLeft);
        println!("move_client: sent move/resize with fabricated serial {FABRICATED_SERIAL}");
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
        _: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let PointerEventKind::Press { serial, .. } = event.kind else {
                continue;
            };
            self.presses += 1;
            let Some(seat) = &self.seat else {
                continue;
            };
            match self.presses {
                1 => {
                    self.window.move_(seat, serial);
                    println!("move_client: sent move with press serial {serial}");
                }
                2 => {
                    self.window.resize(seat, serial, ResizeEdge::BottomRight);
                    println!("move_client: sent resize with press serial {serial}");
                    self.send_stale();
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
