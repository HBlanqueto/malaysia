//! Probe: a client that commits only `wl_surface.frame` gets one callback per refresh cycle,
//! with no input and no busy loop.
use std::time::{Duration, Instant};

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

const SIDE: i32 = 64;
const MEASURE: Duration = Duration::from_secs(2);
const TIMEOUT: Duration = Duration::from_secs(10);
const MIN_PER_SECOND: f64 = 30.0;
const MAX_PER_SECOND: f64 = 75.0;

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
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
        window,
        buffer,
        mapped: false,
        started: None,
        callbacks: 0,
        loop_signal: event_loop.get_signal(),
    };

    let start = Instant::now();
    event_loop
        .handle()
        .insert_source(calloop::timer::Timer::immediate(), move |_, _, app| {
            let measured = app
                .started
                .is_some_and(|started| started.elapsed() >= MEASURE);
            if measured || start.elapsed() >= TIMEOUT {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(20))
        })
        .expect("failed to insert the timer");

    event_loop
        .run(None, &mut app, |_| {})
        .expect("event loop failed");

    let Some(started) = app.started else {
        eprintln!("frame_callbacks: FAIL the window never received its first frame callback");
        std::process::exit(1);
    };
    let per_second = f64::from(app.callbacks) / started.elapsed().as_secs_f64();
    println!(
        "frame_callbacks: {} callbacks, {per_second:.1}/s without damage",
        app.callbacks
    );
    if !(MIN_PER_SECOND..=MAX_PER_SECOND).contains(&per_second) {
        eprintln!(
            "frame_callbacks: FAIL expected {MIN_PER_SECOND}..={MAX_PER_SECOND} callbacks per second"
        );
        std::process::exit(1);
    }
    println!("frame_callbacks: PASS");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    window: Window,
    buffer: Buffer,
    mapped: bool,
    started: Option<Instant>,
    callbacks: u32,
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn request_frame(&self, qh: &QueueHandle<Self>) {
        let surface = self.window.wl_surface();
        surface.frame(qh, surface.clone());
        self.window.commit();
    }
}

impl CompositorHandler for App {
    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &WlSurface, _: u32) {
        match self.started {
            Some(_) => self.callbacks += 1,
            None => self.started = Some(Instant::now()),
        }
        self.request_frame(qh);
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
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
        if self.mapped {
            return;
        }
        self.mapped = true;
        let surface = self.window.wl_surface();
        surface.frame(qh, surface.clone());
        self.window.attach(Some(self.buffer.wl_buffer()), 0, 0);
        surface.damage_buffer(0, 0, SIDE, SIDE);
        self.window.commit();
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
