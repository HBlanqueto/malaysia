use std::time::Duration;

use smithay_client_toolkit::{
    compositor::CompositorState,
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_popup,
    delegate_xdg_shell, delegate_xdg_window,
    globals::ProvidesBoundGlobal,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{
            self as wayland_client, protocol::wl_output::WlOutput, Connection, Dispatch, Proxy,
            QueueHandle,
        },
        protocols::xdg::shell::client::{
            xdg_positioner, xdg_positioner::XdgPositioner, xdg_wm_base::XdgWmBase,
        },
    },
    registry::RegistryState,
    shell::{
        xdg::{
            popup::{Popup, PopupConfigure, PopupHandler},
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell, XdgSurface,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm,
    },
};

const SIDE: u32 = 1000;
const POPUP_W: i32 = 200;
const POPUP_H: i32 = 120;
const ANCHOR_Y: i32 = 760;

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell = XdgShell::bind(&globals, &qh).unwrap();
    let shm = Shm::bind(&globals, &qh).unwrap();
    let mut pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    let (buffer, canvas) = pool
        .create_buffer(
            SIDE as i32,
            SIDE as i32,
            (SIDE * 4) as i32,
            wayland_client::protocol::wl_shm::Format::Xrgb8888,
        )
        .expect("no buffer");
    canvas.fill(0x40);
    window.commit();

    let wm_base: XdgWmBase =
        <XdgShell as ProvidesBoundGlobal<XdgWmBase, 6>>::bound_global(&xdg_shell)
            .expect("no xdg_wm_base");
    let positioner = wm_base.create_positioner(&qh, ());
    positioner.set_size(POPUP_W, POPUP_H);
    positioner.set_anchor_rect((SIDE as i32) / 2 - POPUP_W / 2, ANCHOR_Y, POPUP_W, 1);
    positioner.set_anchor(xdg_positioner::Anchor::Bottom);
    positioner.set_gravity(xdg_positioner::Gravity::Bottom);
    positioner.set_constraint_adjustment(
        xdg_positioner::ConstraintAdjustment::FlipX | xdg_positioner::ConstraintAdjustment::FlipY,
    );
    let popup = Popup::new(
        window.xdg_surface(),
        &positioner,
        &qh,
        &compositor,
        &xdg_shell,
    )
    .expect("no popup");
    let (popup_buffer, popup_canvas) = pool
        .create_buffer(
            POPUP_W,
            POPUP_H,
            POPUP_W * 4,
            wayland_client::protocol::wl_shm::Format::Xrgb8888,
        )
        .expect("no popup buffer");
    popup_canvas.fill(0x80);

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        _window: window,
        window_buffer: buffer,
        window_attached: false,
        popup,
        popup_buffer,
        popup_attached: false,
        popup_configure: None,
        output_size: None,
        phase: 0,
        loop_signal: event_loop.get_signal(),
    };

    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, |_, _, app| {
            app.step();
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(500))
        })
        .unwrap();

    event_loop.run(None, &mut app, |_| {}).unwrap();
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    _window: Window,
    window_buffer: Buffer,
    window_attached: bool,
    popup: Popup,
    popup_buffer: Buffer,
    popup_attached: bool,
    popup_configure: Option<(i32, i32, i32, i32)>,
    output_size: Option<(u32, u32)>,
    phase: u32,
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn step(&mut self) {
        match self.phase {
            0 => println!("probe: phase 0 (settling)"),
            _ => {
                match self.verify() {
                    Ok(()) => println!("probe: PASS the popup was flipped to fit the output"),
                    Err(message) => {
                        eprintln!("probe: FAIL {message}");
                        std::process::exit(1);
                    }
                }
                self.loop_signal.stop();
            }
        }
        self.phase += 1;
    }

    fn verify(&self) -> Result<(), String> {
        let (output_w, output_h) = self.output_size.ok_or("no output size was seen")?;
        let geometry = self
            .popup_configure
            .ok_or("the popup was never configured")?;
        let (x, y, w, h) = geometry;
        println!("probe: popup configure ({x}, {y}) {w}x{h}, output {output_w}x{output_h}");
        if x < 0 || y < 0 || x + w > output_w as i32 || y + h > output_h as i32 {
            return Err(format!(
                "the popup sticks out of the output: ({x}, {y}) {w}x{h} not inside {output_w}x{output_h}"
            ));
        }
        Ok(())
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
        window: &Window,
        _: WindowConfigure,
        _serial: u32,
    ) {
        if !self.window_attached {
            self.window_attached = true;
            window.attach(Some(self.window_buffer.wl_buffer()), 0, 0);
        }
        window.commit();
    }
}

impl PopupHandler for App {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Popup,
        configure: PopupConfigure,
    ) {
        println!("probe: popup configure {configure:?}");
        if !self.popup_attached {
            self.popup_attached = true;
            self.popup
                .wl_surface()
                .attach(Some(self.popup_buffer.wl_buffer()), 0, 0);
            self.popup.wl_surface().commit();
        }
        self.popup_configure = Some((
            configure.position.0,
            configure.position.1,
            configure.width,
            configure.height,
        ));
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Popup) {
        println!("probe: popup dismissed by the compositor");
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: WlOutput) {
        if let Some(info) = self.output_state.info(&output) {
            let size = info.logical_size;
            self.output_size = size.map(|s| (s.0 as u32, s.1 as u32));
        }
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

sctk_probe::shm_and_registry!(App);

impl Dispatch<XdgPositioner, ()> for App {
    fn event(
        _: &mut Self,
        _: &XdgPositioner,
        _: <XdgPositioner as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("xdg_positioner has no events")
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_registry!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_xdg_popup!(App);
