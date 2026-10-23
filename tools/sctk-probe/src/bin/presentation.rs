//! Asserts that `wp_presentation` feedback reaches a client.
//!
//! Maps a client-decorated toplevel and commits three frames, requesting
//! feedback for each one: two with damage (which must be presented with
//! increasing timestamps) and one with no damage (which must be discarded or
//! presented, never left unanswered). Every event is printed with its
//! timestamp, refresh, sequence and flags.
//!
//! On `./dev.sh drm-vkms` (`PRESENTATION_REQUIRE_VSYNC=1`) every presented
//! event must also carry a non-zero refresh and the `vsync` flag, which only
//! the DRM backend can provide.
//!
//! Run against a running compositor, or `./dev.sh presentation`.

use std::time::Duration;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, Dispatch, Proxy, QueueHandle, WEnum},
        protocols::wp::presentation_time::client::{
            wp_presentation::WpPresentation,
            wp_presentation_feedback::{self, WpPresentationFeedback},
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
/// How long to wait for each feedback event.
const TIMEOUT: Duration = Duration::from_secs(10);
/// Damaged frames committed before the no-damage one.
const DAMAGED_FRAMES: u8 = 2;

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let presentation = globals
        .bind::<WpPresentation, App, ()>(&qh, 1..=2, ())
        .expect("no wp_presentation");
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
        presentation,
        window,
        buffer,
        configured: false,
        configure_at: std::time::Instant::now(),
        started: false,
        frame: 0,
        pending: 0,
        last_time: None,
        require_vsync: std::env::var_os("PRESENTATION_REQUIRE_VSYNC").is_some(),
        done: false,
        failed: false,
        loop_signal: event_loop.get_signal(),
    };

    let start = std::time::Instant::now();
    let qh_for_timer = qh.clone();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            if !app.started
                && app.configured
                && app.configure_at.elapsed() >= Duration::from_millis(50)
            {
                app.started = true;
                app.commit(&qh_for_timer);
            }
            if app.done || app.failed {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if start.elapsed() >= TIMEOUT {
                eprintln!(
                    "presentation: FAIL no feedback for frame {} within {TIMEOUT:?}",
                    app.frame
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

    if app.failed || !app.done {
        std::process::exit(1);
    }
    println!("presentation: PASS");
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    presentation: WpPresentation,
    window: Window,
    buffer: Buffer,
    configured: bool,
    configure_at: std::time::Instant,
    started: bool,
    /// 0..DAMAGED_FRAMES are damaged; DAMAGED_FRAMES is the no-damage commit.
    frame: u8,
    pending: u32,
    last_time: Option<(u64, u32)>,
    require_vsync: bool,
    done: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    /// Requests feedback and commits the current frame.
    fn commit(&mut self, qh: &QueueHandle<Self>) {
        self.presentation
            .feedback(self.window.wl_surface(), qh, self.frame);
        self.pending += 1;
        if self.frame < DAMAGED_FRAMES {
            self.window.attach(Some(self.buffer.wl_buffer()), 0, 0);
            self.window.wl_surface().damage_buffer(0, 0, SIDE, SIDE);
        }
        self.window.commit();
        println!("presentation: committed frame {}", self.frame);
    }

    /// Moves to the next frame, or finishes after the no-damage one.
    fn advance(&mut self, qh: &QueueHandle<Self>) {
        if self.frame < DAMAGED_FRAMES {
            self.frame += 1;
            self.commit(qh);
        } else {
            self.done = true;
            self.loop_signal.stop();
        }
    }

    fn fail(&mut self, message: &str) {
        eprintln!("presentation: FAIL {message}");
        self.failed = true;
        self.loop_signal.stop();
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
        _: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
        // Wait for the decoration negotiation's burst of configures to settle,
        // then attach and map once (the timer does the commit).
        if !self.configured {
            self.configured = true;
            self.configure_at = std::time::Instant::now();
        }
        let _ = qh;
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

impl Dispatch<WpPresentation, ()> for App {
    fn event(
        _: &mut Self,
        _: &WpPresentation,
        _: <WpPresentation as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpPresentationFeedback, u8> for App {
    fn event(
        state: &mut Self,
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        frame: &u8,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                seq_hi,
                seq_lo,
                flags,
            } => {
                let seconds = (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo);
                let sequence = (u64::from(seq_hi) << 32) | u64::from(seq_lo);
                println!(
                    "presentation: presented frame {frame} at {seconds}.{tv_nsec:09}s refresh {refresh} seq {sequence} flags {flags:?}"
                );
                if tv_nsec >= 1_000_000_000 {
                    state.fail("nanosecond timestamp out of range");
                    return;
                }
                let time = (seconds, tv_nsec);
                if let Some(previous) = state.last_time {
                    if time <= previous {
                        state.fail("the timestamp did not increase");
                        return;
                    }
                }
                state.last_time = Some(time);
                if state.require_vsync {
                    if refresh == 0 {
                        state.fail("the refresh is zero");
                        return;
                    }
                    match flags {
                        WEnum::Value(flags)
                            if flags.contains(wp_presentation_feedback::Kind::Vsync) => {}
                        _ => {
                            state.fail("the vsync flag is missing");
                            return;
                        }
                    }
                }
                if *frame != state.frame {
                    state.fail("feedback arrived for a different frame");
                    return;
                }
                state.pending = state.pending.saturating_sub(1);
                state.advance(qh);
            }
            wp_presentation_feedback::Event::Discarded => {
                println!("presentation: discarded frame {frame}");
                if *frame < DAMAGED_FRAMES {
                    state.fail("a damaged frame was discarded");
                    return;
                }
                if *frame != state.frame {
                    state.fail("feedback arrived for a different frame");
                    return;
                }
                state.pending = state.pending.saturating_sub(1);
                state.advance(qh);
            }
            _ => {}
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
delegate_registry!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
