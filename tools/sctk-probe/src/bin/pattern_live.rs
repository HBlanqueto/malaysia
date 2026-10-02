//! Maps a full-workspace toplevel filled with a quadrant colour pattern, held
//! open so the debug capture can be validated against it (Bloque 7.3, 3a).
//!
//! The four quadrants are distinct solid colours (red, green, blue, white).
//! A capture that preserves them in their exact positions proves the dump is
//! not flipped, mirrored or stretched — a uniform wallpaper cannot detect
//! that. The buffer is created and attached on the first configure (a buffer
//! attached before the configure is discarded).
//!
//! ```sh
//! cargo run -p sctk-probe --bin pattern_live
//! ```

use std::time::{Duration, Instant};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{
            self as wayland_client,
            protocol::{wl_output, wl_output::WlOutput, wl_surface::WlSurface},
            Connection, QueueHandle,
        },
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
    shm::{slot::SlotPool, Shm, ShmHandler},
};

/// How long to hold the window open, for the capture.
const HOLD: Duration = Duration::from_secs(8);

/// Fills the buffer with four solid-colour quadrants.
fn fill_quadrants(canvas: &mut [u8], w: i32, h: i32) {
    // ABGR (Xrgb8888 is little-endian RGBA in memory: bytes B,G,R,0).
    let (cw, ch) = (w as usize, h as usize);
    for y in 0..ch {
        for x in 0..cw {
            let (r, g, b) = match (x < cw / 2, y < ch / 2) {
                (true, true) => (255, 0, 0),       // top-left red
                (false, true) => (0, 255, 0),      // top-right green
                (true, false) => (0, 0, 255),      // bottom-left blue
                (false, false) => (255, 255, 255), // bottom-right white
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
    /// Fills the window's usable area with the quadrant pattern and commits.
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
