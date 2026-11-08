use std::time::{Duration, Instant};

use smithay_client_toolkit::{
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
    shm::{slot::SlotPool, Shm},
};

const HOLD: Duration = Duration::from_secs(8);

fn fill_quadrants(canvas: &mut [u8], w: i32, h: i32) {
    let (cw, ch) = (w as usize, h as usize);
    for y in 0..ch {
        for x in 0..cw {
            let (r, g, b) = match (x < cw / 2, y < ch / 2) {
                (true, true) => (255, 0, 0),
                (false, true) => (0, 255, 0),
                (true, false) => (0, 0, 255),
                (false, false) => (255, 255, 255),
            };
            let o = (y * w as usize + x) * 4;
            canvas[o] = b;
            canvas[o + 1] = g;
            canvas[o + 2] = r;
            canvas[o + 3] = 0xff;
        }
    }
}

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell = XdgShell::bind(&globals, &qh).unwrap();
    let shm = Shm::bind(&globals, &qh).unwrap();
    let pool = SlotPool::new((1280 * 772 * 4) as usize, &shm).expect("no shm pool");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestClient, &qh);
    window.commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        started: Instant::now(),
        rendered: false,
        last_size: (0, 0),
        loop_signal: event_loop.get_signal(),
    };

    let timer = calloop::timer::Timer::from_duration(Duration::from_millis(250));
    event_loop
        .handle()
        .insert_source(timer, |_, _, app| {
            if app.started.elapsed() > HOLD {
                println!("pattern_live: done, the pattern was held for the capture");
                app.loop_signal.stop();
            }
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(250))
        })
        .unwrap();

    event_loop.run(None, &mut app, |_| {}).unwrap();
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    started: Instant,
    rendered: bool,
    last_size: (i32, i32),
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn render(&mut self, size: (i32, i32)) {
        let (w, h) = size;
        let (buffer, canvas) = self
            .pool
            .create_buffer(
                w,
                h,
                w * 4,
                wayland_client::protocol::wl_shm::Format::Xrgb8888,
            )
            .expect("no buffer");
        fill_quadrants(canvas, w, h);
        self.window.attach(Some(buffer.wl_buffer()), 0, 0);
        self.window.commit();
        self.rendered = true;
        println!("pattern_live: rendered {w}x{h}");
    }
}

sctk_probe::noop_compositor_handler!(App);

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.loop_signal.stop();
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        println!("pattern_live: configure {configure:?}");
        let size = match configure.new_size {
            (Some(w), Some(h)) => (w.get() as i32, h.get() as i32),
            _ => return,
        };
        if !self.rendered || size != self.last_size {
            self.last_size = size;
            self.render(size);
        }
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
