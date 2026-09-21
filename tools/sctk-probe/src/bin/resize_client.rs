//! Maps one toplevel, applies a client size bound, and reports every size the
//! compositor configures it to.
//!
//! `block4` uses this client to assert pointer-driven resizes: with
//! `NEPTUNE_TEST_INPUT=1` the compositor presses an edge or corner of the
//! window (aimed by `NEPTUNE_TEST_PRESS_AT`) and drags the grab with its
//! scripted motions. The client prints each configure and, once the stream
//! settles, the final size:
//!
//! ```text
//! resize_client: configure width=1280 height=720 resizing=false
//! resize_client: configure width=1328 height=752 resizing=true
//! resize_client: final width=1328 height=752
//! ```
//!
//! It is client-decorated, so its configured size is its window size and no
//! decoration reconfigure races the first configure. The bound comes from the
//! environment as an offset from the initial configure, in the surface
//! coordinates `xdg_toplevel.set_min_size`/`set_max_size` take:
//!
//! - `NEPTUNE_RESIZE_MIN=10,10` — the minimum is the initial size minus
//!   `(10, 10)`;
//! - `NEPTUNE_RESIZE_MAX=10,10` — the maximum is the initial size plus
//!   `(10, 10)`.
//!
//! Only one of the two is set per run. It exits non-zero if the window is never
//! configured, or if no resize settles within the timeout, so a missed edge
//! press fails loudly instead of reporting a stale size.

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

/// Buffer side, in pixels.
const SIDE: i32 = 64;
/// How long the size must hold still after a resizing configure to be final.
const SETTLE: Duration = Duration::from_millis(400);
/// How long to wait for a resize to settle.
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
    window.attach(Some(buffer.wl_buffer()), 0, 0);
    window.commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        window,
        _buffer: buffer,
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
    _buffer: Buffer,
    initial: Option<(u32, u32)>,
    last: Option<(u32, u32)>,
    last_configure: Instant,
    saw_resizing: bool,
    printed_final: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    /// Sets the client's minimum or maximum from the environment, relative to
    /// the initial configure, before the mapping commit.
    fn apply_bounds(&self, width: u32, height: u32) {
        if let Some((dw, dh)) = offset("NEPTUNE_RESIZE_MIN") {
            self.window
                .set_min_size(Some((width.saturating_sub(dw), height.saturating_sub(dh))));
            println!("resize_client: set the minimum to the initial size minus ({dw}, {dh})");
        }
        if let Some((dw, dh)) = offset("NEPTUNE_RESIZE_MAX") {
            self.window
                .set_max_size(Some((width.saturating_add(dw), height.saturating_add(dh))));
            println!("resize_client: set the maximum to the initial size plus ({dw}, {dh})");
        }
    }

    /// Whether a resizing configure has arrived and the size has held still.
    fn settled(&self) -> bool {
        self.saw_resizing && self.last.is_some() && self.last_configure.elapsed() >= SETTLE
    }

    /// Prints the last configured size, once.
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
        // Acknowledge and commit, so the window maps and later configures flow.
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

/// Reads an `x,y` offset pair from the environment.
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
