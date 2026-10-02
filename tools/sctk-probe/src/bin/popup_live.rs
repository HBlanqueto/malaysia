//! Maps a toplevel and a popup anchored to its bottom edge, keeping them alive
//! so the composed output can be captured (Bloque 7.2, verification by capture).
//!
//! Same setup as `popup_probe`: a toplevel sized so its bottom edge nears the
//! output's bottom, and a popup with `gravity = bottom` that would land
//! off-screen unless the compositor flips it up. Unlike the probe, it does not
//! assert and exit immediately — it holds the surfaces for a few seconds so a
//! `SIGUSR1` debug capture can show the flipped popup.
//!
//! ```sh
//! cargo run -p sctk-probe --bin popup_live
//! ```

use std::time::{Duration, Instant};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_popup,
    delegate_xdg_shell, delegate_xdg_window,
    globals::ProvidesBoundGlobal,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{
            self as wayland_client,
            protocol::{wl_output, wl_output::WlOutput, wl_surface::WlSurface},
            Connection, Dispatch, Proxy, QueueHandle,
        },
        protocols::xdg::shell::client::{
            xdg_positioner, xdg_positioner::XdgPositioner, xdg_wm_base::XdgWmBase,
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        xdg::{
            popup::{Popup, PopupConfigure, PopupHandler},
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell, XdgSurface,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};

/// The toplevel's surface size, in pixels.
const SIDE: u32 = 1000;
/// The popup's size.
const POPUP_W: i32 = 200;
const POPUP_H: i32 = 120;
/// Where the popup anchors on the window, near its bottom edge.
const ANCHOR_Y: i32 = 760;
/// How long to hold the window and popup open, for the capture.
const HOLD: Duration = Duration::from_secs(8);

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
    window.attach(Some(buffer.wl_buffer()), 0, 0);
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
    popup
        .wl_surface()
        .attach(Some(popup_buffer.wl_buffer()), 0, 0);
    popup.wl_surface().commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        started: Instant::now(),
        loop_signal: event_loop.get_signal(),
    };

    let timer = calloop::timer::Timer::from_duration(Duration::from_millis(250));
    event_loop
        .handle()
        .insert_source(timer, |_, _, app| {
            if app.started.elapsed() > HOLD {
                println!("popup_live: done, the popup was held for the capture");
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
    started: Instant,
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
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.loop_signal.stop();
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        _: WindowConfigure,
        _serial: u32,
    ) {
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
        println!("popup_live: popup configure {configure:?}");
    }
    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Popup) {
        println!("popup_live: popup dismissed by the compositor");
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
delegate_xdg_popup!(App);
