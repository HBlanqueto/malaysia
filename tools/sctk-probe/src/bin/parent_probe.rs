use std::time::Duration;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, Proxy, QueueHandle},
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
};
use wayland_client::protocol::{
    wl_output::{self, WlOutput},
    wl_surface::WlSurface,
};

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell = XdgShell::bind(&globals, &qh).unwrap();

    let parent_surface = compositor.create_surface(&qh);
    let parent = xdg_shell.create_window(parent_surface, WindowDecorations::RequestServer, &qh);
    parent.commit();

    let child_surface = compositor.create_surface(&qh);
    let child = xdg_shell.create_window(child_surface, WindowDecorations::RequestServer, &qh);
    child.commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        parent,
        child,
        phase: 0,
        child_configures: Vec::new(),
        loop_signal: event_loop.get_signal(),
    };

    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, |_, _, app| {
            app.step();
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(700))
        })
        .unwrap();

    event_loop.run(None, &mut app, |_| {}).unwrap();
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    parent: Window,
    child: Window,
    phase: u32,
    child_configures: Vec<(u32, u32)>,
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn step(&mut self) {
        match self.phase {
            0 => println!("probe: phase 0 (initial)"),
            1 => {
                println!("probe: phase 1 (set parent on child)");
                self.child.set_parent(Some(&self.parent));
                self.child.commit();
            }
            2 => {
                println!("probe: phase 2 (unset parent on child)");
                self.child.set_parent(None);
                self.child.commit();
            }
            3 => {
                println!("probe: phase 3 (settling)");
            }
            _ => {
                match self.verify() {
                    Ok(()) => println!("probe: PASS set_parent toggles the child's configure size"),
                    Err(message) => {
                        eprintln!("probe: FAIL {message}");
                        self.loop_signal.stop();
                        std::process::exit(1);
                    }
                }
                self.loop_signal.stop();
            }
        }
        self.phase += 1;
    }

    fn verify(&self) -> Result<(), String> {
        let [initial, with_parent, restored] = self.child_configures[..] else {
            return Err(format!(
                "expected 3 child configures (decorated, dialog, decorated); saw {:?}",
                self.child_configures
            ));
        };
        if with_parent.0 <= initial.0 || with_parent.1 != initial.1 {
            return Err(format!(
                "setting a parent must widen the child and keep its height: {initial:?} -> {with_parent:?}"
            ));
        }
        if restored != initial {
            return Err(format!(
                "unsetting the parent must restore the initial size: expected {initial:?}, got {restored:?}"
            ));
        }
        Ok(())
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
        window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let is_child = window.wl_surface().id() == self.child.wl_surface().id();
        let width = configure.new_size.0.map(|value| value.get()).unwrap_or(0);
        let height = configure.new_size.1.map(|value| value.get()).unwrap_or(0);
        if is_child {
            self.child_configures.push((width, height));
        }
        let which = if is_child { "child" } else { "parent" };
        println!("probe: configure {which} -> ({width}, {height})");
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

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState,];
}

delegate_compositor!(App);
delegate_output!(App);
delegate_registry!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
