//! Asserts that a window can be captured through `ext-image-copy-capture-v1`.
//!
//! Maps a client-decorated toplevel, waits for its `ext-foreign-toplevel-list`
//! handle, creates a capture source from it and opens a session. On the
//! compositor's constraints it allocates a matching SHM buffer, captures and
//! checks the frame is ready and shows the window's own (grey) content.
//!
//! Run against a running compositor, or `./dev.sh screencopy-window`.

use std::time::Duration;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        client::{
            self as wayland_client, globals::GlobalList, Connection, Dispatch, Proxy, QueueHandle,
            WEnum,
        },
        protocols::{
            ext::{
                foreign_toplevel_list::v1::client::{
                    ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
                    ext_foreign_toplevel_list_v1::{
                        self, Event as ListEvent, ExtForeignToplevelListV1,
                    },
                },
                image_capture_source::v1::client::{
                    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
                    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
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
            wp::linux_dmabuf::zv1::client::{
                zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
                zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
            },
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
    wl_buffer::WlBuffer,
    wl_output::{self, WlOutput},
    wl_shm,
    wl_surface::WlSurface,
};

/// Buffer side, in pixels.
const SIDE: i32 = 64;
/// The grey every byte of the window buffer is filled with.
const GREY: u8 = 0x40;
/// How long to wait for the frame to become ready.
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let (mut event_loop, globals, qh) = sctk_probe::init_connection::<App>();

    let compositor = CompositorState::bind(&globals, &qh).expect("no wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("no xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("no wl_shm");
    let list = bind_list(&globals, &qh);
    let sources = bind_sources(&globals, &qh);
    let capture = bind_capture(&globals, &qh);
    let dmabuf_global = if std::env::var_os("CAPTURE_DMABUF").is_some() {
        Some(
            globals
                .bind::<ZwpLinuxDmabufV1, App, ()>(&qh, 3..=5, ())
                .expect("no zwp_linux_dmabuf_v1"),
        )
    } else {
        None
    };
    let pool = SlotPool::new((SIDE * SIDE * 4) as usize, &shm).expect("no shm pool");

    let decorations = if std::env::var_os("CAPTURE_SSD").is_some() {
        WindowDecorations::RequestServer
    } else {
        WindowDecorations::RequestClient
    };
    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, decorations, &qh);
    window.commit();

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        _list: list,
        sources,
        capture,
        dmabuf_global,
        _window: window,
        pool,
        window_buffer: None,
        window_size: None,
        handle: None,
        session: None,
        frame: None,
        capture_buffer: None,
        size: None,
        shm_format: None,
        dma_format: None,
        dma: None,
        dma_buffer: None,
        done: false,
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
                eprintln!("screencopy_window: FAIL no ready frame within {TIMEOUT:?}");
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
    println!("screencopy_window: PASS");
}

fn bind_list(globals: &GlobalList, qh: &QueueHandle<App>) -> ExtForeignToplevelListV1 {
    globals
        .bind(qh, 1..=1, ())
        .expect("no ext_foreign_toplevel_list_v1")
}

fn bind_sources(
    globals: &GlobalList,
    qh: &QueueHandle<App>,
) -> ExtForeignToplevelImageCaptureSourceManagerV1 {
    globals
        .bind(qh, 1..=1, ())
        .expect("no ext_foreign_toplevel_image_capture_source_manager_v1")
}

fn bind_capture(globals: &GlobalList, qh: &QueueHandle<App>) -> ExtImageCopyCaptureManagerV1 {
    globals
        .bind(qh, 1..=1, ())
        .expect("no ext_image_copy_capture_manager_v1")
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    _list: ExtForeignToplevelListV1,
    sources: ExtForeignToplevelImageCaptureSourceManagerV1,
    capture: ExtImageCopyCaptureManagerV1,
    dmabuf_global: Option<ZwpLinuxDmabufV1>,
    _window: Window,
    pool: SlotPool,
    window_buffer: Option<Buffer>,
    window_size: Option<(i32, i32)>,
    handle: Option<ExtForeignToplevelHandleV1>,
    session: Option<ExtImageCopyCaptureSessionV1>,
    frame: Option<ExtImageCopyCaptureFrameV1>,
    capture_buffer: Option<Buffer>,
    size: Option<(i32, i32)>,
    shm_format: Option<wl_shm::Format>,
    dma_format: Option<(u32, u64)>,
    dma: Option<sctk_probe::dmabuf::DmaBuffer>,
    dma_buffer: Option<WlBuffer>,
    done: bool,
    ready: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
    /// Opens the capture session once the toplevel handle and the mapped
    /// window are both known.
    fn open_session(&mut self, qh: &QueueHandle<Self>) {
        if self.session.is_some() {
            return;
        }
        let Some(handle) = &self.handle else {
            return;
        };
        let source = self.sources.create_source(handle, qh, ());
        let session = self
            .capture
            .create_session(&source, Options::empty(), qh, ());
        println!("screencopy_window: session opened for the toplevel");
        self.session = Some(session);
    }

    /// Allocates a buffer from the constraints and starts the capture.
    fn start_capture(&mut self, qh: &QueueHandle<Self>) {
        if !self.done || self.frame.is_some() {
            return;
        }
        let (Some((width, height)), Some(session)) = (self.size, self.session.as_ref()) else {
            eprintln!("screencopy_window: FAIL the session never offered a size");
            self.failed = true;
            self.loop_signal.stop();
            return;
        };
        let stride = width * 4;
        let frame = session.create_frame(qh, ());
        if let Some(dmabuf) = &self.dmabuf_global {
            let Some((format, modifier)) = self.dma_format else {
                eprintln!("screencopy_window: FAIL the session offered no linear dma-buf format");
                self.failed = true;
                self.loop_signal.stop();
                return;
            };
            let dma = match sctk_probe::dmabuf::DmaBuffer::new(width, height, stride as usize) {
                Ok(dma) => dma,
                Err(error) => {
                    eprintln!("screencopy_window: FAIL could not create the dma-buf: {error:?}");
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
                "screencopy_window: capturing {width}x{height} as dma-buf {format:#x} modifier {modifier:#x}"
            );
            frame.attach_buffer(&buffer);
        } else {
            let Some(format) = self.shm_format else {
                eprintln!("screencopy_window: FAIL the session never offered a shm format");
                self.failed = true;
                self.loop_signal.stop();
                return;
            };
            let (buffer, _canvas) = match self.pool.create_buffer(width, height, stride, format) {
                Ok(pair) => pair,
                Err(error) => {
                    eprintln!("screencopy_window: FAIL no shm buffer: {error:?}");
                    self.failed = true;
                    self.loop_signal.stop();
                    return;
                }
            };
            println!("screencopy_window: capturing {width}x{height} as {format:?}");
            frame.attach_buffer(buffer.wl_buffer());
            self.capture_buffer = Some(buffer);
        }
        frame.capture();
        self.frame = Some(frame);
    }

    /// Reads the captured pixels back and checks the window's grey content.
    fn check_pixels(&mut self) {
        let Some((width, height)) = self.size else {
            return;
        };
        let (x, y) = ((width / 2) as usize, (height / 2) as usize);
        if let Some(dma) = &self.dma {
            let pixel = dma.pixel(x, y);
            println!("screencopy_window: centre pixel {pixel:?} (dma-buf)");
            if pixel == [0, 0, 0, 0] {
                eprintln!("screencopy_window: FAIL the captured window is blank");
                self.failed = true;
            }
            return;
        }
        let Some(buffer) = &self.capture_buffer else {
            return;
        };
        let Some(canvas) = buffer.canvas(&mut self.pool) else {
            eprintln!("screencopy_window: FAIL the buffer has no canvas");
            self.failed = true;
            return;
        };
        let offset = (y * width as usize + x) * 4;
        let pixel = &canvas[offset..offset + 4];
        println!(
            "screencopy_window: centre pixel {pixel:?} ({:?})",
            self.shm_format
        );
        if pixel == [0, 0, 0, 0] {
            eprintln!("screencopy_window: FAIL the captured window is blank");
            self.failed = true;
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
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        // Honor the compositor's size so the window's usable rect and the
        // surface match; a fixed buffer would capture mostly background.
        let width = configure.new_size.0.map_or(0, |w| w.get() as i32);
        let height = configure.new_size.1.map_or(0, |h| h.get() as i32);
        if width > 0 && height > 0 && self.window_size != Some((width, height)) {
            println!("screencopy_window: configured content {width}x{height}");
            match self
                .pool
                .create_buffer(width, height, width * 4, wl_shm::Format::Xrgb8888)
            {
                Ok((buffer, canvas)) => {
                    canvas.fill(GREY);
                    window.attach(Some(buffer.wl_buffer()), 0, 0);
                    self.window_buffer = Some(buffer);
                    self.window_size = Some((width, height));
                }
                Err(error) => {
                    eprintln!("screencopy_window: FAIL no window buffer: {error:?}");
                    self.failed = true;
                    self.loop_signal.stop();
                }
            }
        }
        window.commit();
        self.open_session(qh);
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

impl Dispatch<ExtForeignToplevelListV1, ()> for App {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ListEvent,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let ListEvent::Toplevel { toplevel } = event {
            println!("screencopy_window: got a toplevel handle");
            state.handle = Some(toplevel);
            state.open_session(qh);
        }
    }

    wayland_client::event_created_child!(App, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ExtForeignToplevelHandleV1,
        _: <ExtForeignToplevelHandleV1 as Proxy>::Event,
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

impl Dispatch<ExtForeignToplevelImageCaptureSourceManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ExtForeignToplevelImageCaptureSourceManagerV1,
        _: <ExtForeignToplevelImageCaptureSourceManagerV1 as Proxy>::Event,
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
            SessionEvent::DmabufFormat { format, modifiers } => {
                if state.dma_format.is_none() && is_linear_capture_format(format) {
                    let linear = modifiers
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|chunk| u64::from_le_bytes(*chunk))
                        .find(|modifier| *modifier == 0);
                    if let Some(modifier) = linear {
                        state.dma_format = Some((format, modifier));
                    }
                }
            }
            SessionEvent::Done => {
                state.done = true;
                state.start_capture(qh);
            }
            SessionEvent::Stopped => {
                eprintln!("screencopy_window: FAIL the compositor stopped the session");
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
                println!("screencopy_window: frame ready");
                state.check_pixels();
                state.ready = true;
                state.loop_signal.stop();
            }
            FrameEvent::Failed { reason } => {
                eprintln!("screencopy_window: FAIL the frame failed: {reason:?}");
                state.failed = true;
                state.loop_signal.stop();
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

impl Dispatch<ZwpLinuxDmabufV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ZwpLinuxDmabufV1,
        _: <ZwpLinuxDmabufV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ZwpLinuxBufferParamsV1,
        _: <ZwpLinuxBufferParamsV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for App {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

fn is_linear_capture_format(format: u32) -> bool {
    matches!(format, 0x3432_5241 | 0x3432_5258)
}
