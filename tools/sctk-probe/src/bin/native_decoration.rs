//! Exercises the native-app decoration rule (ADR 0035).
//!
//! Each scenario talks to a compositor started with
//! `window.decorations = "force_server"` and checks the configure width:
//! native app ids must never get MalaysiaDreams's titlebar, while a foreign app still
//! does.
//!
//! - `a`: a native app asking for server-side.
//! - `b`: a native app asking for client-side after setting its app id.
//! - `c`: a native app that never sends `request_mode`.
//! - `d`: a foreign app with no request.

use std::time::Duration;

use smithay_client_toolkit::reexports::client::protocol::{
    wl_output::{self, WlOutput},
    wl_surface::WlSurface,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{Connection, Dispatch, Proxy, QueueHandle},
        protocols::xdg::decoration::zv1::client::{
            zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
            zxdg_toplevel_decoration_v1::{self, Mode, ZxdgToplevelDecorationV1},
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
    shm::{Shm, ShmHandler},
};

/// The native app-id prefix from `vow_design`.
const NATIVE_PREFIX: &str = "io.github.hblanqueto.malaysia.";
/// The titlebar inset MalaysiaDreams reserves for foreign SSD windows.
const TITLEBAR: i32 = 32;
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let scenario = std::env::var("MALAYSIA_NATIVE_SCENARIO").unwrap_or_default();
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let decoration_manager = globals
        .bind::<ZxdgDecorationManagerV1, App, ()>(&qh, 1..=1, ())
        .expect("no zxdg_decoration_manager_v1");
    let output_state = OutputState::new(&globals, &qh);

    let surface = compositor.create_surface(&qh);
    let (window, expected_bar) = match scenario.as_str() {
        "a" => {
            let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
            window.set_app_id(format!("{NATIVE_PREFIX}demo"));
            (window, true)
        }
        "b" => {
            let window = xdg_shell.create_window(surface, WindowDecorations::None, &qh);
            window.set_app_id(format!("{NATIVE_PREFIX}demo"));
            let decoration =
                decoration_manager.get_toplevel_decoration(window.xdg_toplevel(), &qh, ());
            decoration.set_mode(Mode::ClientSide);
            (window, false)
        }
        "c" => {
            let window = xdg_shell.create_window(surface, WindowDecorations::None, &qh);
            window.set_app_id(format!("{NATIVE_PREFIX}demo"));
            (window, false)
        }
        "d" => {
            let window = xdg_shell.create_window(surface, WindowDecorations::None, &qh);
            window.set_app_id("foot");
            (window, true)
        }
        other => panic!("unknown scenario {other:?}"),
    };
    window.commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state,
        shm,
        _decoration_manager: decoration_manager,
        _window: window,
        scenario: scenario.clone(),
        expected_bar,
        checked: false,
        failed: false,
        loop_signal: event_loop.get_signal(),
    };

    let start = std::time::Instant::now();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            if app.checked || app.failed {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if start.elapsed() >= TIMEOUT {
                eprintln!("native_decoration: FAIL no configure within {TIMEOUT:?}");
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

    if app.failed || !app.checked {
        std::process::exit(1);
    }
    println!("native_decoration: PASS scenario {scenario}");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    _decoration_manager: ZxdgDecorationManagerV1,
    _window: Window,
    scenario: String,
    expected_bar: bool,
    checked: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
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
        _: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        let (Some(width), Some(height)) = configure.new_size else {
            return;
        };
        let (width, height) = (width.get() as i32, height.get() as i32);
        let output_height = self
            .output_state
            .outputs()
            .next()
            .and_then(|output| self.output_state.info(&output))
            .and_then(|info| info.logical_size)
            .map(|(_, height)| height)
            .unwrap_or(800);
        let expected_height = if self.expected_bar {
            output_height - TITLEBAR
        } else {
            output_height
        };
        println!(
            "native_decoration: scenario {} mode {:?} size {width}x{height} (output height {output_height})",
            self.scenario, configure.decoration_mode
        );
        // The first configure can arrive before the app id is known. The
        // titlebar sits on top, so its presence shows up as a shorter height.
        if height == expected_height {
            self.checked = true;
            self.loop_signal.stop();
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

impl Dispatch<ZxdgDecorationManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ZxdgDecorationManagerV1,
        _: <ZxdgDecorationManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZxdgToplevelDecorationV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_toplevel_decoration_v1::Event::Configure { mode } = event {
            println!("native_decoration: requested mode {mode:?}");
        }
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_registry!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
