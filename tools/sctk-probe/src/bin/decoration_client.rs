use std::time::Duration;

use sctk::reexports::client::protocol::{
    wl_output::{self, WlOutput},
    wl_shm,
    wl_surface::WlSurface,
};
use sctk::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::OutputState,
    reexports::{
        calloop,
        client::{Connection, QueueHandle},
    },
    registry::RegistryState,
    shell::{
        xdg::{
            window::{DecorationMode, Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm,
    },
};

const SIDE: i32 = 64;
const TITLEBAR: u32 = 32;
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let mut pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
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
        attached: false,
        server_width: None,
        requested_change: false,
        saw_client: false,
        failed: false,
        loop_signal: event_loop.get_signal(),
    };

    let start = std::time::Instant::now();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            if app.saw_client {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if start.elapsed() >= TIMEOUT {
                eprintln!("decoration_client: FAIL no client-side configure within {TIMEOUT:?}");
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
    println!("decoration_client: PASS");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    window: Window,
    buffer: Buffer,
    attached: bool,
    server_width: Option<u32>,
    requested_change: bool,
    saw_client: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl CompositorHandler for App {
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {
        if !self.requested_change {
            self.requested_change = true;
            self.window
                .request_decoration_mode(Some(DecorationMode::Client));
            println!("decoration_client: requested client-side after mapping");
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
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        let (Some(width), _) = configure.new_size else {
            return;
        };
        let width = width.get();
        match configure.decoration_mode {
            DecorationMode::Server => {
                if self.server_width.is_none() {
                    println!("decoration_client: server configure {width}x");
                    self.server_width = Some(width);
                    let surface = window.wl_surface().clone();
                    let _ = surface.frame(qh, surface.clone());
                }
            }
            DecorationMode::Client => {
                if self.requested_change {
                    let Some(server) = self.server_width else {
                        eprintln!("decoration_client: FAIL client-side before server-side");
                        self.failed = true;
                        self.loop_signal.stop();
                        return;
                    };
                    println!("decoration_client: client configure {width}x");
                    if width != server + TITLEBAR {
                        eprintln!(
                            "decoration_client: FAIL expected {} after the change, got {width}",
                            server + TITLEBAR
                        );
                        self.failed = true;
                    }
                    self.saw_client = true;
                    self.loop_signal.stop();
                }
            }
        }
        if !self.attached {
            self.attached = true;
            window.attach(Some(self.buffer.wl_buffer()), 0, 0);
        }
        window.commit();
    }
}

sctk_probe::noop_output_handler!(App);

sctk_probe::shm_and_registry!(App);

delegate_compositor!(App);
delegate_output!(App);
delegate_registry!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
