use std::time::Duration;

use sctk::{
    compositor::CompositorState,
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::OutputState,
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, QueueHandle},
    },
    registry::RegistryState,
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
use wayland_client::protocol::wl_shm;

const WINDOWS: usize = 50;
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

struct OpenWindow {
    window: Window,
    buffer: Buffer,
    attached: bool,
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor: CompositorState,
    xdg_shell: XdgShell,
    shm: Shm,
    pool: SlotPool,
    qh: QueueHandle<App>,
    windows: Vec<OpenWindow>,
    phase: u32,
    loop_signal: calloop::LoopSignal,
}

impl App {
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
            window.commit();
            self.windows.push(OpenWindow {
                window,
                buffer,
                attached: false,
            });
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
        if let Some(entry) = self
            .windows
            .iter_mut()
            .find(|entry| entry.window == *window)
        {
            if !entry.attached {
                entry.attached = true;
                entry.window.attach(Some(entry.buffer.wl_buffer()), 0, 0);
            }
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
