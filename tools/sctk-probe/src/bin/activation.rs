//! Asserts that `xdg-activation-v1` focuses a window in the same client.
//!
//! Maps two client-decorated toplevels of one client (A then B, so B ends up
//! focused). The compositor's scripted input (`YUKI_TEST_INPUT=1`) presses on
//! the first mapped window, which gives this client a recent pointer serial and
//! leaves A focused until B maps. The client then asks for an activation token
//! with that serial and A's surface, and activates A through it.
//!
//! It passes when the keyboard enters A afterwards: the token was accepted and
//! the compositor moved focus. A rejected token (no focus, stale serial) or a
//! wrong policy would leave B focused and time out.
//!
//! Run against a compositor started with `YUKI_TEST_INPUT=1`, or
//! `./dev.sh activation`.

use std::time::Duration;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, Dispatch, Proxy, QueueHandle},
        protocols::xdg::activation::v1::client::{
            xdg_activation_token_v1::{self, XdgActivationTokenV1},
            xdg_activation_v1::XdgActivationV1,
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
    wl_keyboard::{self, WlKeyboard},
    wl_output::{self, WlOutput},
    wl_pointer::WlPointer,
    wl_seat::WlSeat,
    wl_shm,
    wl_surface::WlSurface,
};

/// Buffer side, in pixels.
const SIDE: i32 = 64;
/// How long to wait for the activation to move focus.
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let activation = globals
        .bind::<XdgActivationV1, App, ()>(&qh, 1..=1, ())
        .expect("no xdg_activation_v1");
    let seat_state = SeatState::new(&globals, &qh);
    let mut pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let surface_a = compositor.create_surface(&qh);
    let window_a = xdg_shell.create_window(surface_a, WindowDecorations::RequestClient, &qh);
    let (buffer_a, canvas_a) = pool
        .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
        .expect("failed to create a buffer");
    canvas_a.fill(0x30);
    window_a.commit();

    let surface_b = compositor.create_surface(&qh);
    let window_b = xdg_shell.create_window(surface_b, WindowDecorations::RequestClient, &qh);
    let (buffer_b, canvas_b) = pool
        .create_buffer(SIDE, SIDE, SIDE * 4, wl_shm::Format::Xrgb8888)
        .expect("failed to create a buffer");
    canvas_b.fill(0x60);
    window_b.commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        seat_state,
        activation,
        seat: None,
        pointer: None,
        keyboard: None,
        window_a,
        window_b,
        buffer_a,
        buffer_b,
        attached_a: false,
        attached_b: false,
        press_serial: None,
        focused: None,
        requested: false,
        activated: false,
        failed: false,
        loop_signal: event_loop.get_signal(),
    };

    let start = std::time::Instant::now();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            if app.activated || app.failed {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if start.elapsed() >= TIMEOUT {
                eprintln!("activation: FAIL focus never moved to the first window");
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

    if app.failed || !app.activated {
        std::process::exit(1);
    }
    println!("activation: PASS");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    seat_state: SeatState,
    activation: XdgActivationV1,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    window_a: Window,
    window_b: Window,
    buffer_a: Buffer,
    buffer_b: Buffer,
    attached_a: bool,
    attached_b: bool,
    press_serial: Option<u32>,
    focused: Option<WlSurface>,
    requested: bool,
    activated: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    /// Requests the token once both windows are mapped and the press serial and
    /// focus are known. The scripted input presses on A, so the serial is valid
    /// and the requesting surface (A) is in the same client as the focused B.
    fn maybe_request(&mut self, qh: &QueueHandle<Self>) {
        if self.requested || !self.attached_a || !self.attached_b {
            return;
        }
        let (Some(serial), Some(seat)) = (self.press_serial, self.seat.as_ref()) else {
            return;
        };
        self.requested = true;
        let token = self.activation.get_activation_token(qh, ());
        token.set_serial(serial, seat);
        token.set_surface(self.window_a.wl_surface());
        token.commit();
        println!("activation: requested a token with serial {serial}");
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
        qh: &QueueHandle<Self>,
        window: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
        if window == &self.window_a && !self.attached_a {
            self.attached_a = true;
            window.attach(Some(self.buffer_a.wl_buffer()), 0, 0);
        }
        if window == &self.window_b && !self.attached_b {
            self.attached_b = true;
            window.attach(Some(self.buffer_b.wl_buffer()), 0, 0);
        }
        window.commit();
        self.maybe_request(qh);
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
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(qh, ()));
        }
        self.seat = Some(seat);
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            self.pointer = None;
        }
        if capability == Capability::Keyboard {
            self.keyboard = None;
        }
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
                    println!("activation: got the press serial {serial}");
                    self.maybe_request(qh);
                }
            }
        }
    }
}

impl Dispatch<WlKeyboard, ()> for App {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Enter { surface, .. } = event {
            println!("activation: keyboard entered {:?}", surface.id());
            let first = state.window_a.wl_surface().id();
            state.focused = Some(surface.clone());
            if surface.id() == first && state.requested {
                state.activated = true;
                state.loop_signal.stop();
            }
        }
    }
}

impl Dispatch<XdgActivationV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &XdgActivationV1,
        _: <XdgActivationV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<XdgActivationTokenV1, ()> for App {
    fn event(
        state: &mut Self,
        _: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            println!("activation: token done, activating the first window");
            state
                .activation
                .activate(token, state.window_a.wl_surface());
        }
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
