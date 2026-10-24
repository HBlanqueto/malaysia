//! Maps an xdg_toplevel and asserts it receives frame callbacks for commits
//! that carry no damage.
//!
//! The first commit attaches a buffer so the surface maps. After that, every
//! frame callback triggers a commit that only requests another frame callback.
//! A compositor that answers frame callbacks only when there is damage will
//! stall and the probe will time out.
//!
//! Run it against a fresh compositor:
//!
//! ```sh
//! cargo run -p sctk-probe --bin toplevel_frame_probe
//! ```

use std::time::Duration;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{
            protocol::{
                wl_output, wl_output::WlOutput, wl_pointer::WlPointer, wl_seat::WlSeat,
                wl_surface::WlSurface,
            },
            Connection, Proxy, QueueHandle,
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerHandler},
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

/// The toplevel's surface size, in pixels.
const SIDE: u32 = 200;

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell = XdgShell::bind(&globals, &qh).unwrap();
    let shm = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);

    let mut pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    let (buffer, canvas) = pool
        .create_buffer(
            SIDE as i32,
            SIDE as i32,
            (SIDE * 4) as i32,
            smithay_client_toolkit::reexports::client::protocol::wl_shm::Format::Xrgb8888,
        )
        .expect("no buffer");
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
        frame_count: 0,
        commit_count: 0,
        loop_signal: event_loop.get_signal(),
    };

    let timer = calloop::timer::Timer::from_duration(Duration::from_millis(500));
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app: &mut App| {
            app.check_timeout();
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(500))
        })
        .unwrap();

    event_loop.run(None, &mut app, |_| {}).unwrap();
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
    frame_count: u32,
    commit_count: u32,
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn commit_frame_only(&mut self, qh: &QueueHandle<Self>) {
        self.commit_count += 1;
        let surface = self.window.wl_surface();
        surface.frame(qh, surface.clone());
        surface.commit();
    }

    fn check_timeout(&mut self) {
        if self.frame_count >= 3 && self.commit_count >= 3 {
            println!(
                "toplevel_frame_probe: PASS received {}/{} frame callbacks",
                self.frame_count, self.commit_count
            );
            self.loop_signal.stop();
        }
    }
}

impl CompositorHandler for App {
    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, surface: &WlSurface, _: u32) {
        if surface.id() != self.window.wl_surface().id() {
            return;
        }
        self.frame_count += 1;
        println!(
            "toplevel_frame_probe: frame callback {} (commits {})",
            self.frame_count, self.commit_count
        );
        if self.commit_count < 3 {
            self.commit_frame_only(qh);
        } else {
            self.check_timeout();
        }
    }

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
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.loop_signal.stop();
    }
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        _: WindowConfigure,
        _serial: u32,
    ) {
        if !self.attached {
            self.attached = true;
            window.attach(Some(self.buffer.wl_buffer()), 0, 0);
        }
        window.commit();

        // The first configure carries no frame callback yet; request one so the
        // no-damage cycle can start once the surface maps.
        if self.commit_count == 0 {
            self.commit_frame_only(qh);
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
            self.pointer = Some(self.seat_state.get_pointer(qh, &seat).unwrap());
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlPointer,
        _: &[PointerEvent],
    ) {
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
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
