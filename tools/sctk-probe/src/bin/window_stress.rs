//! Opens many toplevels, then closes them, so the compositor's teardown can be
//! asserted from the outside.
//!
//! This is a dev-only client for the Block 4 acceptance check. It creates
//! [`WINDOWS`] decorated toplevels, each with a small shared-memory buffer so
//! they map, waits for their configures, then destroys every one and exits. The
//! compositor is expected to return to an empty scene; the `block4` harness
//! asserts that from the compositor's own report.
//!
//! Run against a running compositor:
//!
//! ```sh
//! cargo run -p sctk-probe --bin window_stress
//! ```

use std::time::Duration;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, QueueHandle},
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
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
    wl_shm,
    wl_surface::WlSurface,
};

/// How many windows to open and close.
const WINDOWS: usize = 50;
/// The side of each window's buffer, in pixels.
const SIDE: i32 = 64;

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let pool = SlotPool::new(WINDOWS * (SIDE * SIDE * 4) as usize, &shm)
        .expect("failed to create the shm pool");

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        xdg_shell,
        shm,
        pool,
        qh: qh.clone(),
        windows: Vec::new(),
        phase: 0,
        loop_signal: event_loop.get_signal(),
    };

    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, |_, _, app| {
            app.step();
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(200))
        })
        .expect("failed to insert the timer");

    event_loop
        .run(None, &mut app, |_| {})
        .expect("event loop failed");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor: CompositorState,
    xdg_shell: XdgShell,
    shm: Shm,
    pool: SlotPool,
    qh: QueueHandle<App>,
    windows: Vec<(Window, Buffer)>,
    phase: u32,
    loop_signal: calloop::LoopSignal,
}

impl App {
    /// Creates every toplevel with a buffer attached, so each one maps.
    fn open_all(&mut self) {
        let qh = self.qh.clone();
        for _ in 0..WINDOWS {
            let surface = self.compositor.create_surface(&qh);
            let window =
                self.xdg_shell
                    .create_window(surface, WindowDecorations::RequestServer, &qh);
            let (buffer, canvas) = self
                .pool
                .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
                .expect("failed to create a buffer");
            canvas.fill(0x40);
            window.attach(Some(buffer.wl_buffer()), 0, 0);
            window.commit();
            self.windows.push((window, buffer));
        }
        println!("stress: opened {WINDOWS} windows");
    }

    fn step(&mut self) {
        match self.phase {
            0 => self.open_all(),
            1 => {
                let closed = self.windows.len();
                self.windows.clear();
                println!("stress: closed {closed} windows");
            }
            _ => self.loop_signal.stop(),
        }
        self.phase += 1;
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
        // Acknowledging the configure and committing maps the window.
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

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState,];
}

delegate_compositor!(App);
delegate_output!(App);
delegate_registry!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
