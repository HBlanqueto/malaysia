//! Asserts that an output can be captured through `ext-image-copy-capture-v1`.
//!
//! Binds the output capture source manager and the image copy capture manager,
//! creates a source for the first `wl_output`, opens a session, and — once the
//! compositor sends its buffer constraints (`BufferSize`, `ShmFormat`, `Done`)
//! — allocates a matching SHM buffer, attaches it to a frame and captures.
//! It passes when the frame's `Ready` event arrives; a `Failed`, or a timeout,
//! fails.
//!
//! Run against a running compositor, or `./dev.sh screencopy`.

use std::time::Duration;

use smithay_client_toolkit::{
    delegate_registry, delegate_shm,
    reexports::{
        calloop,
        client::{self as wayland_client, Connection, Dispatch, Proxy, QueueHandle, WEnum},
        protocols::ext::{
            image_capture_source::v1::client::{
                ext_image_capture_source_v1::ExtImageCaptureSourceV1,
                ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
            },
            image_copy_capture::v1::client::{
                ext_image_copy_capture_frame_v1::{
                    Event as FrameEvent, ExtImageCopyCaptureFrameV1,
                },
                ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
                ext_image_copy_capture_session_v1::{
                    Event as SessionEvent, ExtImageCopyCaptureSessionV1,
                },
            },
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use wayland_client::protocol::{wl_output::WlOutput, wl_shm};

/// How long to wait for the frame to become ready.
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let output = globals
        .bind::<WlOutput, App, ()>(&qh, 1..=4, ())
        .expect("no wl_output");
    let sources = globals
        .bind::<ExtOutputImageCaptureSourceManagerV1, App, ()>(&qh, 1..=1, ())
        .expect("no ext_output_image_capture_source_manager_v1");
    let capture = globals
        .bind::<ExtImageCopyCaptureManagerV1, App, ()>(&qh, 1..=1, ())
        .expect("no ext_image_copy_capture_manager_v1");

    let source = sources.create_source(&output, &qh, ());
    let session = capture.create_session(&source, Options::empty(), &qh, ());

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        shm,
        session: Some(session),
        frame: None,
        buffer: None,
        pool: None,
        size: None,
        shm_format: None,
        done: false,
        dma_offered: false,
        ready: false,
        failed: false,
        loop_signal: event_loop.get_signal(),
    };

    let start = std::time::Instant::now();
    let timer = calloop::timer::Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, app| {
            if app.ready || app.failed {
                app.loop_signal.stop();
                return calloop::timer::TimeoutAction::Drop;
            }
            if start.elapsed() >= TIMEOUT {
                eprintln!("screencopy: FAIL no ready frame within {TIMEOUT:?}");
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

    if app.failed || !app.ready {
        std::process::exit(1);
    }
    println!("screencopy: PASS");
}

struct App {
    registry_state: RegistryState,
    shm: Shm,
    session: Option<ExtImageCopyCaptureSessionV1>,
    frame: Option<ExtImageCopyCaptureFrameV1>,
    buffer: Option<Buffer>,
    pool: Option<SlotPool>,
    size: Option<(i32, i32)>,
    shm_format: Option<wl_shm::Format>,
    done: bool,
    dma_offered: bool,
    ready: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    /// Once the constraints are in, allocate a buffer and start the capture.
    fn start_capture(&mut self, qh: &QueueHandle<Self>) {
        if !self.done || self.frame.is_some() {
            return;
        }
        let (Some((width, height)), Some(format)) = (self.size, self.shm_format) else {
            eprintln!("screencopy: FAIL the session never offered a shm format and size");
            self.failed = true;
            self.loop_signal.stop();
            return;
        };
        let Some(session) = &self.session else {
            return;
        };
        let stride = width * 4;
        let mut pool = match SlotPool::new((stride * height) as usize, &self.shm) {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("screencopy: FAIL could not create the shm pool: {error:?}");
                self.failed = true;
                self.loop_signal.stop();
                return;
            }
        };
        let buffer = match pool.create_buffer(width, height, stride, format) {
            Ok((buffer, _canvas)) => buffer,
            Err(error) => {
                eprintln!("screencopy: FAIL could not create the shm buffer: {error:?}");
                self.failed = true;
                self.loop_signal.stop();
                return;
            }
        };
        let frame = session.create_frame(qh, ());
        frame.attach_buffer(buffer.wl_buffer());
        frame.capture();
        println!("screencopy: capturing {width}x{height} as {format:?}");
        self.buffer = Some(buffer);
        self.pool = Some(pool);
        self.frame = Some(frame);
    }

    /// Reads back the captured pixels and checks they are not blank.
    ///
    /// The compositor clears the output to a dark colour, so the centre pixel
    /// must be that colour (with an opaque alpha). All-zero bytes would mean
    /// the copy never wrote anything.
    fn check_pixels(&mut self) {
        let (Some(buffer), Some(pool)) = (&self.buffer, &mut self.pool) else {
            return;
        };
        let Some(canvas) = buffer.canvas(pool) else {
            eprintln!("screencopy: FAIL the buffer has no canvas");
            self.failed = true;
            return;
        };
        let (Some((width, height)), Some(format)) = (self.size, self.shm_format) else {
            return;
        };
        let (x, y) = (width / 2, height / 2);
        let offset = (y * width + x) as usize * 4;
        let pixel = &canvas[offset..offset + 4];
        println!("screencopy: centre pixel {:?} ({format:?})", pixel);
        if pixel == [0, 0, 0, 0] {
            eprintln!("screencopy: FAIL the captured buffer is blank");
            self.failed = true;
        }
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![];
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl Dispatch<WlOutput, ()> for App {
    fn event(
        _: &mut Self,
        _: &WlOutput,
        _: <WlOutput as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtOutputImageCaptureSourceManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ExtOutputImageCaptureSourceManagerV1,
        _: <ExtOutputImageCaptureSourceManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCaptureSourceV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ExtImageCaptureSourceV1,
        _: <ExtImageCaptureSourceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCopyCaptureManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ExtImageCopyCaptureManagerV1,
        _: <ExtImageCopyCaptureManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, ()> for App {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: SessionEvent,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            SessionEvent::BufferSize { width, height } => {
                state.size = Some((width as i32, height as i32));
            }
            SessionEvent::ShmFormat { format } => {
                if state.shm_format.is_none() {
                    if let WEnum::Value(format) = format {
                        state.shm_format = Some(format);
                    }
                }
            }
            SessionEvent::DmabufDevice { .. } | SessionEvent::DmabufFormat { .. } => {
                state.dma_offered = true;
            }
            SessionEvent::Done => {
                println!(
                    "screencopy: constraints done (dma-buf offered: {})",
                    state.dma_offered
                );
                state.done = true;
                state.start_capture(qh);
            }
            SessionEvent::Stopped => {
                eprintln!("screencopy: FAIL the compositor stopped the session");
                state.failed = true;
                state.loop_signal.stop();
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, ()> for App {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: FrameEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            FrameEvent::Ready => {
                println!("screencopy: frame ready");
                state.check_pixels();
                state.ready = true;
                state.loop_signal.stop();
            }
            FrameEvent::Failed { reason } => {
                eprintln!("screencopy: FAIL the frame failed: {reason:?}");
                state.failed = true;
                state.loop_signal.stop();
            }
            _ => {}
        }
    }
}

delegate_registry!(App);
delegate_shm!(App);
