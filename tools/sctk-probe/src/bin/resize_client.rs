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
    shm::{
        slot::{Buffer, SlotPool},
        Shm,
    },
};
use wayland_client::protocol::wl_shm;

const SIDE: i32 = 64;
const SETTLE: Duration = Duration::from_millis(400);
const TIMEOUT: Duration = Duration::from_secs(6);

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
        attached: false,
        initial: None,
        last: None,
        last_configure: Instant::now(),
        saw_resizing: false,
        printed_final: false,
        failed: false,
        loop_signal: event_loop.get_signal(),
    };

    let start = Instant::now();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            if app.printed_final {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if app.settled() {
                app.print_final();
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if start.elapsed() >= TIMEOUT {
                eprintln!(
                    "resize_client: FAIL no settled resize within {TIMEOUT:?}; initial {:?}, last {:?}",
                    app.initial, app.last
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

    if app.failed || !app.printed_final {
        std::process::exit(1);
    }
    println!("resize_client: PASS");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    window: Window,
    buffer: Buffer,
    attached: bool,
    initial: Option<(u32, u32)>,
    last: Option<(u32, u32)>,
    last_configure: Instant,
    saw_resizing: bool,
    printed_final: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn apply_bounds(&self, width: u32, height: u32) {
        if let Some((dw, dh)) = offset("MALAYSIA_RESIZE_MIN") {
            self.window
                .set_min_size(Some((width.saturating_sub(dw), height.saturating_sub(dh))));
            println!("resize_client: set the minimum to the initial size minus ({dw}, {dh})");
        }
        if let Some((dw, dh)) = offset("MALAYSIA_RESIZE_MAX") {
            self.window
                .set_max_size(Some((width.saturating_add(dw), height.saturating_add(dh))));
            println!("resize_client: set the maximum to the initial size plus ({dw}, {dh})");
        }
    }

    fn settled(&self) -> bool {
        self.saw_resizing && self.last.is_some() && self.last_configure.elapsed() >= SETTLE
    }

    fn print_final(&mut self) {
        if self.printed_final {
            return;
        }
        self.printed_final = true;
        if let Some((width, height)) = self.last {
            println!("resize_client: final width={width} height={height}");
        }
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
        configure: WindowConfigure,
        _: u32,
    ) {
        if let (Some(width), Some(height)) = (configure.new_size.0, configure.new_size.1) {
            let (width, height) = (width.get(), height.get());
            if self.initial.is_none() {
                self.initial = Some((width, height));
                self.apply_bounds(width, height);
            }
            self.last = Some((width, height));
            self.last_configure = Instant::now();
            self.saw_resizing = self.saw_resizing || configure.is_resizing();
            println!(
                "resize_client: configure width={width} height={height} resizing={}",
                configure.is_resizing()
            );
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

fn offset(name: &str) -> Option<(u32, u32)> {
    let value = std::env::var(name).ok()?;
    let (x, y) = value.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

delegate_compositor!(App);
delegate_output!(App);
delegate_registry!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
