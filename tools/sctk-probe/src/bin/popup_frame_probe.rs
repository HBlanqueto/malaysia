//! Maps a toplevel and a popup with grab, then asserts the popup receives
//! three `wl_callback.done` events when only the popup surface commits damage.
//!
//! The compositor is started with `MALAYSIA_TEST_INPUT=1`, which injects a press on
//! the toplevel. The probe captures that press serial and uses it for the popup
//! grab, then drives three damaged commits of the popup and counts the
//! resulting frame callbacks.
//!
//! Run it against a fresh compositor:
//!
//! ```sh
//! cargo run -p sctk-probe --bin popup_frame_probe
//! ```

use std::time::Duration;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_popup, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{
            self as wayland_client,
            protocol::{
                wl_output, wl_output::WlOutput, wl_pointer::WlPointer, wl_seat::WlSeat,
                wl_surface::WlSurface,
            },
            Connection, Dispatch, Proxy, QueueHandle,
        },
        protocols::xdg::shell::client::{
            xdg_positioner, xdg_positioner::XdgPositioner, xdg_wm_base::XdgWmBase,
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
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
        Shm, ShmHandler,
    },
};

/// The toplevel's surface size, in pixels.
const SIDE: u32 = 400;
/// The popup's size.
const POPUP_W: i32 = 200;
const POPUP_H: i32 = 120;

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell = XdgShell::bind(&globals, &qh).unwrap();
    let shm = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);
    let wm_base = <XdgShell as smithay_client_toolkit::globals::ProvidesBoundGlobal<
        XdgWmBase,
        6,
    >>::bound_global(&xdg_shell)
    .expect("no xdg_wm_base");
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
        compositor,
        xdg_shell,
        wm_base,
        seat_state,
        seat: None,
        pointer: None,
        window,
        window_buffer: buffer,
        window_attached: false,
        popup: None,
        popup_buffer,
        popup_attached: false,
        press_serial: None,
        frame_count: 0,
        commit_count: 0,
        loop_signal: event_loop.get_signal(),
    };

    let timer = calloop::timer::Timer::from_duration(Duration::from_millis(500));
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app: &mut App| {
            app.check_timeout();
            calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(500))
        })
        .unwrap();

    event_loop.run(None, &mut app, |_| {}).unwrap();
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    compositor: CompositorState,
    xdg_shell: XdgShell,
    wm_base: XdgWmBase,
    seat_state: SeatState,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    window: Window,
    window_buffer: Buffer,
    window_attached: bool,
    popup: Option<Popup>,
    popup_buffer: Buffer,
    popup_attached: bool,
    press_serial: Option<u32>,
    frame_count: u32,
    commit_count: u32,
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn open_popup(&mut self, qh: &QueueHandle<App>) {
        let positioner = self.wm_base.create_positioner(qh, ());
        positioner.set_size(POPUP_W, POPUP_H);
        positioner.set_anchor_rect((SIDE as i32) / 2 - POPUP_W / 2, 100, POPUP_W, 1);
        positioner.set_anchor(xdg_positioner::Anchor::Bottom);
        positioner.set_gravity(xdg_positioner::Gravity::Bottom);
        positioner.set_constraint_adjustment(
            xdg_positioner::ConstraintAdjustment::FlipX
                | xdg_positioner::ConstraintAdjustment::FlipY,
        );

        let popup = Popup::new(
            self.window.xdg_surface(),
            &positioner,
            qh,
            &self.compositor,
            &self.xdg_shell,
        )
        .expect("no popup");

        if let Some(serial) = self.press_serial {
            if let Some(seat) = &self.seat {
                popup.xdg_popup().grab(seat, serial);
            }
        }

        self.popup = Some(popup);
    }

    fn commit_popup(&mut self, qh: &QueueHandle<App>) {
        let Some(popup) = self.popup.as_ref() else {
            return;
        };
        self.commit_count += 1;
        // Damage a small region so the compositor has surface damage to repaint.
        popup.wl_surface().damage_buffer(0, 0, POPUP_W, POPUP_H);
        popup
            .wl_surface()
            .attach(Some(self.popup_buffer.wl_buffer()), 0, 0);
        popup.wl_surface().frame(qh, popup.wl_surface().clone());
        popup.wl_surface().commit();
    }

    fn check_timeout(&mut self) {
        if self.commit_count >= 3 && self.frame_count >= 3 {
            println!(
                "popup_frame_probe: PASS received {}/{} frame callbacks",
                self.frame_count, self.commit_count
            );
            self.loop_signal.stop();
        }
    }
}

impl CompositorHandler for App {
    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, surface: &WlSurface, _: u32) {
        let Some(popup) = self.popup.as_ref() else {
            return;
        };
        if surface.id() != popup.wl_surface().id() {
            return;
        }
        self.frame_count += 1;
        println!(
            "popup_frame_probe: frame callback {} for popup (commits {})",
            self.frame_count, self.commit_count
        );
        if self.commit_count < 3 {
            self.commit_popup(qh);
        } else {
            self.check_timeout();
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
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.loop_signal.stop();
    }
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        _: WindowConfigure,
        _serial: u32,
    ) {
        if !self.window_attached {
            self.window_attached = true;
            window.attach(Some(self.window_buffer.wl_buffer()), 0, 0);
        }
        window.commit();

        if self.popup.is_none() && self.press_serial.is_some() {
            self.open_popup(qh);
        }
    }
}

impl PopupHandler for App {
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &Popup,
        configure: PopupConfigure,
    ) {
        println!("popup_frame_probe: popup configure {configure:?}");
        if !self.popup_attached {
            self.popup_attached = true;
            self.commit_popup(qh);
        }
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Popup) {
        println!("popup_frame_probe: popup dismissed by the compositor");
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

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seat = Some(seat);
    }
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = Some(self.seat_state.get_pointer(qh, &seat).unwrap());
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if let PointerEventKind::Press { serial, .. } = event.kind {
                if self.press_serial.is_none() {
                    self.press_serial = Some(serial);
                    println!("popup_frame_probe: captured press serial {serial}");
                    if self.window_attached && self.popup.is_none() {
                        self.open_popup(qh);
                    }
                }
            }
        }
    }
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
delegate_pointer!(App);
delegate_registry!(App);
delegate_seat!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_xdg_popup!(App);
