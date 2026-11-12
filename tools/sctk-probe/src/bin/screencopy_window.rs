use std::time::Duration;

use sctk::{
    compositor::CompositorState,
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::OutputState,
    reexports::{
        calloop,
        client::{self as wayland_client, globals::GlobalList, Connection, Dispatch, QueueHandle},
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
    registry::RegistryState,
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm,
    },
};
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_shm};

const SIDE: i32 = 64;
const GREY: u8 = 0x40;
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
    dma_offered: bool,
    ready: bool,
    failed: bool,
    loop_signal: calloop::LoopSignal,
}

impl App {
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

sctk_probe::noop_compositor_handler!(App);

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

sctk_probe::noop_output_handler!(App);

sctk_probe::shm_and_registry!(App);

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

sctk::reexports::client::delegate_noop!(App: ignore ExtForeignToplevelHandleV1);

sctk::reexports::client::delegate_noop!(App: ignore ExtImageCaptureSourceV1);

sctk::reexports::client::delegate_noop!(App: ignore ExtForeignToplevelImageCaptureSourceManagerV1);

sctk::reexports::client::delegate_noop!(App: ignore ExtImageCopyCaptureManagerV1);

sctk_probe::capture_session_and_frame!(App, "screencopy_window");

delegate_compositor!(App);
delegate_output!(App);
delegate_registry!(App);
delegate_shm!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);

sctk::reexports::client::delegate_noop!(App: ignore ZwpLinuxDmabufV1);

sctk::reexports::client::delegate_noop!(App: ignore ZwpLinuxBufferParamsV1);

sctk::reexports::client::delegate_noop!(App: ignore WlBuffer);
