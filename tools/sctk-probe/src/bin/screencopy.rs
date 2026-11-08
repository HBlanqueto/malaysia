use std::time::Duration;

use smithay_client_toolkit::{
    delegate_registry, delegate_shm,
    reexports::{
        calloop,
        client::{self as wayland_client, QueueHandle},
        protocols::{
            ext::{
                image_capture_source::v1::client::{
                    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
                    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
                },
                image_copy_capture::v1::client::{
                    ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
                    ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
                    ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
                },
            },
            wp::linux_dmabuf::zv1::client::{
                zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
                zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
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
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_output::WlOutput, wl_shm};

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

    let paint_cursors = std::env::var_os("CAPTURE_PAINT_CURSORS").is_some();
    let dmabuf_global = if std::env::var_os("CAPTURE_DMABUF").is_some() {
        Some(
            globals
                .bind::<ZwpLinuxDmabufV1, App, ()>(&qh, 3..=5, ())
                .expect("no zwp_linux_dmabuf_v1"),
        )
    } else {
        None
    };
    let mut options = Options::empty();
    if paint_cursors {
        options |= Options::PaintCursors;
    }

    let source = sources.create_source(&output, &qh, ());
    let session = capture.create_session(&source, options, &qh, ());

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        shm,
        session: Some(session),
        frame: None,
        buffer: None,
        pool: None,
        size: None,
        shm_format: None,
        dmabuf_global,
        dma_format: None,
        dma: None,
        dma_buffer: None,
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
    dmabuf_global: Option<ZwpLinuxDmabufV1>,
    dma_format: Option<(u32, u64)>,
    dma: Option<sctk_probe::dmabuf::DmaBuffer>,
    dma_buffer: Option<WlBuffer>,
    done: bool,
    dma_offered: bool,
    ready: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    fn start_capture(&mut self, qh: &QueueHandle<Self>) {
        if !self.done || self.frame.is_some() {
            return;
        }
        let Some((width, height)) = self.size else {
            eprintln!("screencopy: FAIL the session never offered a size");
            self.failed = true;
            self.loop_signal.stop();
            return;
        };
        let Some(session) = &self.session else {
            return;
        };
        let stride = width * 4;
        let frame = session.create_frame(qh, ());

        if let Some(dmabuf) = &self.dmabuf_global {
            let Some((format, modifier)) = self.dma_format else {
                eprintln!("screencopy: FAIL the session offered no linear dma-buf format");
                self.failed = true;
                self.loop_signal.stop();
                return;
            };
            let dma = match sctk_probe::dmabuf::DmaBuffer::new(width, height, stride as usize) {
                Ok(dma) => dma,
                Err(error) => {
                    eprintln!("screencopy: FAIL could not create the dma-buf: {error:?}");
                    self.failed = true;
                    self.loop_signal.stop();
                    return;
                }
            };
            let params = dmabuf.create_params(qh, ());
            params.add(
                dma.fd(),
                0,
                0,
                stride as u32,
                (modifier >> 32) as u32,
                (modifier & 0xffff_ffff) as u32,
            );
            let buffer = params.create_immed(
                width,
                height,
                format,
                zwp_linux_buffer_params_v1::Flags::empty(),
                qh,
                (),
            );
            self.dma = Some(dma);
            self.dma_buffer = Some(buffer.clone());
            println!(
                "screencopy: capturing {width}x{height} as dma-buf {format:#x} modifier {modifier:#x}"
            );
            frame.attach_buffer(&buffer);
        } else {
            let Some(format) = self.shm_format else {
                eprintln!("screencopy: FAIL the session never offered a shm format");
                self.failed = true;
                self.loop_signal.stop();
                return;
            };
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
            println!("screencopy: capturing {width}x{height} as {format:?}");
            frame.attach_buffer(buffer.wl_buffer());
            self.buffer = Some(buffer);
            self.pool = Some(pool);
        }
        frame.capture();
        self.frame = Some(frame);
    }

    fn check_pixels(&mut self) {
        let Some((width, height)) = self.size else {
            return;
        };
        let (x, y) = ((width / 2) as usize, (height / 2) as usize);
        if let Some(dma) = &self.dma {
            let pixel = dma.pixel(x, y);
            let background = dma.pixel(0, 0);
            let different = dma.count_different(background);
            println!(
                "screencopy: centre pixel {pixel:?} (dma-buf), {different} pixels differ from {background:?}"
            );
            if pixel == [0, 0, 0, 0] {
                eprintln!("screencopy: FAIL the captured buffer is blank");
                self.failed = true;
            }
            self.check_cursor(different);
            return;
        }
        let (Some(buffer), Some(pool)) = (&self.buffer, &mut self.pool) else {
            return;
        };
        let Some(canvas) = buffer.canvas(pool) else {
            eprintln!("screencopy: FAIL the buffer has no canvas");
            self.failed = true;
            return;
        };
        let offset = (y * width as usize + x) * 4;
        let pixel = &canvas[offset..offset + 4];
        let background = &canvas[..4];
        let different = canvas
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|candidate| *candidate != background)
            .count();
        println!(
            "screencopy: centre pixel {:?} ({:?}), {different} pixels differ from {background:?}",
            pixel, self.shm_format
        );
        if pixel == [0, 0, 0, 0] {
            eprintln!("screencopy: FAIL the captured buffer is blank");
            self.failed = true;
        }
        self.check_cursor(different);
    }

    fn check_cursor(&mut self, different: usize) {
        if std::env::var_os("CAPTURE_EXPECT_CURSOR").is_some() && different == 0 {
            eprintln!("screencopy: FAIL paint_cursors did not include the cursor");
            self.failed = true;
        }
        if std::env::var_os("CAPTURE_EXPECT_NO_CURSOR").is_some() && different != 0 {
            eprintln!("screencopy: FAIL the cursor was painted without paint_cursors");
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

smithay_client_toolkit::reexports::client::delegate_noop!(App: ignore WlOutput);

smithay_client_toolkit::reexports::client::delegate_noop!(App: ignore ExtOutputImageCaptureSourceManagerV1);

smithay_client_toolkit::reexports::client::delegate_noop!(App: ignore ExtImageCaptureSourceV1);

smithay_client_toolkit::reexports::client::delegate_noop!(App: ignore ExtImageCopyCaptureManagerV1);

sctk_probe::capture_session_and_frame!(App, "screencopy");

delegate_registry!(App);
delegate_shm!(App);

smithay_client_toolkit::reexports::client::delegate_noop!(App: ignore ZwpLinuxDmabufV1);

smithay_client_toolkit::reexports::client::delegate_noop!(App: ignore ZwpLinuxBufferParamsV1);

smithay_client_toolkit::reexports::client::delegate_noop!(App: ignore WlBuffer);
