use std::error::Error;
use std::fmt;

use smithay::backend::allocator::{Fourcc, Modifier};
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
use smithay::utils::{Buffer as BufferCoord, Logical, Physical, Point, Rectangle, Size, Transform};

use super::{describe_chain, import_pending_dmabufs, physical_point, winit_mode_source};
use crate::cursor::OutputRenderElement;
use crate::scene_elements::{
    cursor_origin, frame_has_border, frame_order, frame_radius, physical_rect,
};

use sctk::output::{OutputHandler, OutputState};
use sctk::reexports::client::protocol::{
    wl_buffer, wl_buffer::WlBuffer, wl_output, wl_shm, wl_shm_pool, wl_surface,
};
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::reexports::protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1,
};

#[derive(Debug)]
struct Leaf;

impl fmt::Display for Leaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the inner cause")
    }
}

impl Error for Leaf {}

#[derive(Debug)]
struct Wrapper(Leaf);

impl fmt::Display for Wrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the outer message")
    }
}

impl Error for Wrapper {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

#[test]
fn an_ime_popup_sits_below_the_text_cursor() {
    let output = Rectangle::from_size(Size::from((1000, 800)));
    let cursor = Rectangle::new(Point::from((40, 30)), Size::from((2, 20)));
    let location = crate::scene_elements::ime_popup_location(
        Point::from((100, 100)),
        cursor,
        Size::from((200, 60)),
        output,
    );
    assert_eq!(location, Point::from((140, 150)));
}

#[test]
fn an_ime_popup_flips_above_the_cursor_at_the_bottom_and_stays_inside_the_output() {
    let output = Rectangle::from_size(Size::from((1000, 800)));
    let cursor = Rectangle::new(Point::from((880, 700)), Size::from((2, 20)));
    let location = crate::scene_elements::ime_popup_location(
        Point::from((50, 50)),
        cursor,
        Size::from((200, 60)),
        output,
    );
    assert_eq!(location, Point::from((800, 690)));
}

#[test]
fn the_chain_reports_every_cause() {
    assert_eq!(
        describe_chain(&Wrapper(Leaf)),
        "the outer message: the inner cause"
    );
}

#[test]
fn a_lone_error_is_its_own_description() {
    assert_eq!(describe_chain(&Leaf), "the inner cause");
}

#[test]
fn maximized_squares_the_corners_and_only_fullscreen_drops_the_border() {
    use crate::decoration::{Role, WindowState};

    assert_eq!(
        frame_radius(None, WindowState::Normal, Role::Toplevel, false, None),
        ctdesign::WINDOW_CORNER.titlebar
    );
    assert_eq!(
        frame_radius(None, WindowState::Maximized, Role::Toplevel, false, None),
        0
    );
    for state in [WindowState::Normal, WindowState::Maximized] {
        assert!(frame_has_border(state));
    }
    assert_eq!(
        frame_radius(Some(8), WindowState::Fullscreen, Role::Toplevel, true, None),
        0
    );
    assert!(!frame_has_border(WindowState::Fullscreen));
}

#[test]
fn physical_point_floors_position() {
    assert_eq!(
        physical_point(Point::<i32, Logical>::from((2, 2)), 1.25),
        Point::<i32, Physical>::from((2, 2))
    );
    assert_eq!(
        physical_point(Point::<i32, Logical>::from((-2, -2)), 1.25),
        Point::<i32, Physical>::from((-3, -3))
    );
}

#[test]
fn adjacent_logical_rectangles_share_physical_edge() {
    for scale in [1.25, 1.5] {
        let left = Rectangle::<i32, Logical>::new(Point::from((0, 0)), Size::from((101, 100)));
        let right = Rectangle::<i32, Logical>::new(Point::from((101, 0)), Size::from((100, 100)));
        let left_physical = physical_rect(left, scale);
        let right_physical = physical_rect(right, scale);
        assert!(
            left_physical.loc.x + left_physical.size.w > right_physical.loc.x,
            "adjacent rectangles must share the physical boundary pixel at scale {scale}; \
             left ends at {}, right starts at {}",
            left_physical.loc.x + left_physical.size.w,
            right_physical.loc.x,
        );
    }
}

const RED: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 255];

#[allow(unsafe_code)]
fn offscreen_renderer() -> GlesRenderer {
    let device = EGLDevice::enumerate()
        .expect("EGL device enumeration should work")
        .next()
        .expect("the pixel test needs an EGL device");
    // SAFETY: the display owns `device`, so the native handle lives as long as the display.
    let display = unsafe { EGLDisplay::new(device) }.expect("the test needs an EGL display");
    let context = EGLContext::new(&display).expect("the test needs an EGL context");
    // SAFETY: `context` was just created on this thread and is not shared with another.
    unsafe { GlesRenderer::new(context) }.expect("the test needs a GLES renderer")
}

fn render_and_read(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    elements: &[OutputRenderElement<GlesRenderer>],
) -> Vec<u8> {
    let buffer_size: Size<i32, BufferCoord> = Size::from((size.w, size.h));
    let mut target =
        Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Argb8888, buffer_size)
            .expect("the test needs an offscreen buffer");
    let mut framebuffer = renderer.bind(&mut target).expect("the buffer should bind");

    let mut tracker = OutputDamageTracker::from_mode_source(winit_mode_source(size, 1.0));
    tracker
        .render_output(
            renderer,
            &mut framebuffer,
            0,
            elements,
            [0.0, 0.0, 0.0, 1.0],
        )
        .expect("the frame should render");

    let mapping = renderer
        .copy_framebuffer(
            &framebuffer,
            Rectangle::from_size(buffer_size),
            Fourcc::Argb8888,
        )
        .expect("the framebuffer should be readable");
    let pixels = renderer
        .map_texture(&mapping)
        .expect("the mapping should read");
    let mut window_order = pixels.to_vec();
    let stride = (size.w * 4) as usize;
    for row in 0..(size.h as usize) / 2 {
        let top = row * stride;
        let bottom = (size.h as usize - 1 - row) * stride;
        for byte in 0..stride {
            window_order.swap(top + byte, bottom + byte);
        }
    }
    window_order
}

fn pixel(pixels: &[u8], size: Size<i32, Physical>, x: i32, y: i32) -> [u8; 4] {
    let stride = (size.w * 4) as usize;
    let offset = (y as usize) * stride + (x as usize) * 4;
    pixels[offset..offset + 4]
        .try_into()
        .expect("the readback should cover the whole frame")
}

fn solid(
    renderer: &mut GlesRenderer,
    color: [u8; 4],
    side: i32,
) -> OutputRenderElement<GlesRenderer> {
    let buffer = MemoryRenderBuffer::from_slice(
        &color.repeat((side * side) as usize),
        Fourcc::Argb8888,
        (side, side),
        1,
        Transform::Normal,
        None,
    );
    let element = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        (0.0, 0.0),
        &buffer,
        None,
        None,
        None,
        Kind::Unspecified,
    )
    .expect("the element should build");
    element.into()
}

#[test]
fn the_frame_is_outer_border_inner_line_content_then_shadow() {
    let frame = frame_order(
        Some("outer"),
        Some("inner"),
        vec!["content"],
        vec!["shadow"],
    );
    assert_eq!(frame, ["outer", "inner", "content", "shadow"]);
}

#[test]
fn a_top_left_quadrant_stays_in_the_top_left() {
    let size = Size::from((64, 64));
    let mut renderer = offscreen_renderer();
    let quadrant = solid(&mut renderer, RED, 32);
    let pixels = render_and_read(&mut renderer, size, &[quadrant]);

    assert_eq!(
        pixel(&pixels, size, 4, 4),
        RED,
        "the element is at render (0, 0), so its pixels must appear at the window's top-left"
    );
    assert_eq!(
        pixel(&pixels, size, 4, 60),
        CLEAR,
        "the bottom-left must stay clear: an inverted frame puts the element there"
    );
    assert_eq!(
        pixel(&pixels, size, 60, 4),
        CLEAR,
        "the top-right must stay clear"
    );
}

#[test]
fn an_enlarged_cursor_fills_its_scaled_rectangle() {
    let size = Size::from((64, 64));
    let mut renderer = offscreen_renderer();
    let cursor = MemoryRenderBuffer::from_slice(
        &GREEN.repeat(16),
        Fourcc::Argb8888,
        (4, 4),
        1,
        Transform::Normal,
        None,
    );
    let source = Rectangle::from_size(Size::from((4.0, 4.0)));
    let element = MemoryRenderBufferRenderElement::from_buffer(
        &mut renderer,
        Point::from((0.0, 0.0)),
        &cursor,
        None,
        Some(source),
        Some(Size::from((12, 12))),
        Kind::Cursor,
    )
    .expect("the enlarged cursor element should build");
    let pixels = render_and_read(&mut renderer, size, &[element.into()]);

    assert_eq!(
        pixel(&pixels, size, 10, 10),
        GREEN,
        "an enlarged cursor must paint the far corner of its destination"
    );
}

#[test]
fn the_cursor_lands_at_the_hotspot_adjusted_position() {
    let size = Size::from((64, 64));
    let mut renderer = offscreen_renderer();

    let cursor = MemoryRenderBuffer::from_slice(
        &GREEN.repeat(16),
        Fourcc::Argb8888,
        (4, 4),
        1,
        Transform::Normal,
        None,
    );
    let origin = cursor_origin(Point::from((10.0, 10.0)), Point::from((3.0, 4.0)), 1.0);
    assert_eq!(origin, Point::from((7.0, 6.0)));
    let element = MemoryRenderBufferRenderElement::from_buffer(
        &mut renderer,
        origin,
        &cursor,
        None,
        None,
        None,
        Kind::Cursor,
    )
    .expect("the cursor element should build");
    let pixels = render_and_read(&mut renderer, size, &[element.into()]);

    assert_eq!(
        pixel(&pixels, size, 7, 6),
        GREEN,
        "the cursor image's top-left must land at the pointer minus the hotspot"
    );
    assert_eq!(
        pixel(&pixels, size, 6, 6),
        CLEAR,
        "nothing may be drawn left of the hotspot-adjusted origin"
    );
    assert_eq!(
        pixel(&pixels, size, 7, 5),
        CLEAR,
        "nothing may be drawn above the hotspot-adjusted origin"
    );
}

struct TestClient {
    registry: sctk::registry::RegistryState,
    compositor: Option<sctk::compositor::CompositorState>,
    output: OutputState,
    dmabuf: sctk::dmabuf::DmabufState,
}

impl TestClient {
    fn new(
        globals: &sctk::reexports::client::globals::GlobalList,
        qh: &QueueHandle<TestClient>,
    ) -> Self {
        Self {
            registry: sctk::registry::RegistryState::new(globals),
            compositor: sctk::compositor::CompositorState::bind(globals, qh).ok(),
            output: OutputState::new(globals, qh),
            dmabuf: sctk::dmabuf::DmabufState::new(globals, qh),
        }
    }
}

impl sctk::registry::ProvidesRegistryState for TestClient {
    fn registry(&mut self) -> &mut sctk::registry::RegistryState {
        &mut self.registry
    }

    fn runtime_add_global(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: u32,
        _: &str,
        _: u32,
    ) {
    }

    fn runtime_remove_global(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: u32,
        _: &str,
    ) {
    }
}

sctk::delegate_registry!(TestClient);

impl OutputHandler for TestClient {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<TestClient>, _: wl_output::WlOutput) {}

    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: wl_output::WlOutput,
    ) {
    }
}

sctk::delegate_output!(TestClient);

sctk::reexports::client::delegate_noop!(TestClient: ignore wl_shm::WlShm);

sctk::reexports::client::delegate_noop!(TestClient: ignore wl_shm_pool::WlShmPool);

sctk::reexports::client::delegate_noop!(TestClient: ignore wl_buffer::WlBuffer);

impl sctk::compositor::CompositorHandler for TestClient {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

sctk::delegate_compositor!(TestClient);

impl sctk::dmabuf::DmabufHandler for TestClient {
    fn dmabuf_state(&mut self) -> &mut sctk::dmabuf::DmabufState {
        &mut self.dmabuf
    }

    fn dmabuf_feedback(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
        _: sctk::dmabuf::DmabufFeedback,
    ) {
    }

    fn created(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        _: WlBuffer,
    ) {
    }

    fn failed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<TestClient>,
        _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    ) {
    }

    fn released(&mut self, _: &Connection, _: &QueueHandle<TestClient>, _: &WlBuffer) {}
}

sctk::delegate_dmabuf!(TestClient);

#[test]
fn the_global_filter_does_not_stall_the_next_client() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use sctk::reexports::client::Connection;
    use sctk::reexports::client::globals::registry_queue_init;

    let (_event_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));

    fn registry_globals(stream: std::os::unix::net::UnixStream) -> Vec<String> {
        let conn = Connection::from_socket(stream).expect("the client should connect");
        let (globals, _queue) =
            registry_queue_init::<TestClient>(&conn).expect("the registry should init");
        globals
            .contents()
            .clone_list()
            .into_iter()
            .map(|global| global.interface)
            .collect()
    }

    let (first_side, first_server) = std::os::unix::net::UnixStream::pair().expect("a socket pair");
    let (first_tx, first_rx) = mpsc::channel();
    std::thread::spawn(move || {
        first_tx
            .send(registry_globals(first_side))
            .expect("the main thread should read the globals");
    });
    crate::test_support::insert_client(&display, first_server);

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        if let Ok(globals) = first_rx.try_recv() {
            assert!(globals.iter().any(|name| name == "wl_compositor"));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the first client never received its globals"
        );
    }

    let (second_side, second_server) =
        std::os::unix::net::UnixStream::pair().expect("a socket pair");
    let (second_tx, second_rx) = mpsc::channel();
    std::thread::spawn(move || {
        second_tx
            .send(registry_globals(second_side))
            .expect("the main thread should read the globals");
    });
    crate::test_support::insert_client(&display, second_server);

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        if let Ok(globals) = second_rx.try_recv() {
            assert!(
                globals.iter().any(|name| name == "wl_compositor"),
                "the second client must receive the globals"
            );
            assert!(
                !globals
                    .iter()
                    .any(|name| name == "infinity_window_manager_v1"),
                "an untrusted client must not see the Infinity global"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "a client connected and the compositor stopped dispatching"
        );
    }
}

#[test]
fn a_failed_create_immed_is_drained_and_posts_the_protocol_error() {
    use std::os::unix::io::AsFd;
    use std::sync::mpsc;

    use sctk::reexports::client::Connection;
    use sctk::reexports::client::globals::registry_queue_init;

    let mut renderer = offscreen_renderer();

    let (_event_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));

    let (client_side, server_side) = std::os::unix::net::UnixStream::pair().expect("a socket pair");
    let (sent_tx, sent_rx) = mpsc::channel::<bool>();

    std::thread::spawn(move || {
        let conn = Connection::from_socket(client_side).expect("the client should connect");
        let (globals, mut event_queue) =
            registry_queue_init(&conn).expect("the registry should init");
        let qh = event_queue.handle();
        let mut client = TestClient::new(&globals, &qh);
        event_queue
            .roundtrip(&mut client)
            .expect("the bind should roundtrip");

        let (fake_fd, _sink) = std::os::unix::net::UnixStream::pair().expect("a fake plane fd");
        let params = client
            .dmabuf
            .create_params(&qh)
            .expect("the compositor advertises zwp_linux_dmabuf_v1");
        params.add(fake_fd.as_fd(), 0, 0, 4 * 16, u64::from(Modifier::Invalid));
        let (buffer, _params) = params.create_immed(
            16,
            16,
            Fourcc::Xrgb8888 as u32,
            zwp_linux_buffer_params_v1::Flags::empty(),
            &qh,
        );

        let surface = client
            .compositor
            .as_ref()
            .expect("compositor bound")
            .create_surface(&qh);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        event_queue
            .roundtrip(&mut client)
            .expect("the batch should reach the compositor");
        sent_tx
            .send(false)
            .expect("the main thread should read the signal");

        let dead = event_queue.roundtrip(&mut client).is_err();
        sent_tx
            .send(dead)
            .expect("the main thread should read the verdict");
    });

    crate::test_support::insert_client(&display, server_side);

    let mut spins = 0;
    loop {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        if matches!(sent_rx.try_recv(), Ok(false)) {
            break;
        }
        spins += 1;
        assert!(spins < 100_000, "the client never signalled its batch");
    }

    while state.pending_dmabuf_imports.is_empty() {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        spins += 1;
        assert!(spins < 1000, "the import was never queued");
    }
    assert_eq!(
        state.pending_dmabuf_imports.len(),
        1,
        "the single create_immed queues exactly one import"
    );

    import_pending_dmabufs(&mut state, &mut renderer);
    assert!(
        state.pending_dmabuf_imports.is_empty(),
        "the queue must be drained before a frame can be drawn"
    );
    display
        .flush_clients()
        .expect("the error must reach the client");

    let dead = sent_rx
        .recv()
        .expect("the client should report its verdict");
    assert!(
        dead,
        "the failed create_immed must kill the client, never leave a black window"
    );
}

#[test]
fn without_a_syncobj_device_no_explicit_sync_global_is_announced() {
    let (_event_loop, _display, state) = crate::test_support::test_state((1280, 720));
    assert!(
        state.syncobj_state.is_none(),
        "without a device there must be no wp_linux_drm_syncobj_manager_v1 global"
    );
}

#[test]
fn a_surface_without_a_current_buffer_is_not_visible() {
    use std::os::unix::io::AsFd;
    use std::sync::mpsc;

    use sctk::reexports::client::Connection;
    use sctk::reexports::client::globals::registry_queue_init;

    use smithay::reexports::wayland_server::Resource;
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

    let (_event_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));

    let (client_side, server_side) = std::os::unix::net::UnixStream::pair().expect("a socket pair");
    let (sent_tx, sent_rx) = mpsc::channel::<bool>();

    std::thread::spawn(move || {
        let conn = Connection::from_socket(client_side).expect("the client should connect");
        let (globals, mut event_queue) =
            registry_queue_init(&conn).expect("the registry should init");
        let qh = event_queue.handle();
        let mut client = TestClient::new(&globals, &qh);
        event_queue
            .roundtrip(&mut client)
            .expect("the bind should roundtrip");

        let surface = client
            .compositor
            .as_ref()
            .expect("compositor bound")
            .create_surface(&qh);
        surface.commit();
        event_queue
            .roundtrip(&mut client)
            .expect("the no-buffer commit should reach the compositor");
        sent_tx
            .send(false)
            .expect("the main thread should read the first signal");

        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())
            .expect("the compositor advertises wl_shm");
        let path = std::env::temp_dir().join(format!("malaysia-shm-test-{}", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .expect("a temp file for the shm pool");
        file.set_len(64 * 64 * 4)
            .expect("the pool fits a 64x64 buffer");
        let pool = shm.create_pool(file.as_fd(), 64 * 64 * 4, &qh, ());
        let buffer = pool.create_buffer(0, 64, 64, 64 * 4, wl_shm::Format::Argb8888, &qh, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        std::fs::remove_file(&path).ok();
        event_queue
            .roundtrip(&mut client)
            .expect("the buffered commit should reach the compositor");
        sent_tx
            .send(true)
            .expect("the main thread should read the second signal");
    });

    let handle = display.handle();
    let client = crate::test_support::insert_client(&display, server_side);

    fn surface_of_client(
        handle: &smithay::reexports::wayland_server::DisplayHandle,
        client: &smithay::reexports::wayland_server::Client,
    ) -> Option<WlSurface> {
        let backend = handle.backend_handle();
        let mut ids = Vec::new();
        let _ = backend.with_all_objects_for(client.id(), |object_id| ids.push(object_id));
        let surface_id = ids.into_iter().find(|id| {
            handle
                .object_info(id.clone())
                .map(|info| info.interface.name == "wl_surface")
                .unwrap_or(false)
        })?;
        WlSurface::from_id(handle, surface_id).ok()
    }

    let mut spins = 0;
    loop {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        if matches!(sent_rx.try_recv(), Ok(false)) {
            break;
        }
        spins += 1;
        assert!(
            spins < 100_000,
            "the client never committed without a buffer"
        );
    }
    let surface =
        surface_of_client(&handle, &client).expect("the client should have created a wl_surface");
    assert!(
        !crate::window::surface_has_buffer(&surface),
        "a surface that never attached a buffer has no current buffer"
    );

    loop {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        if matches!(sent_rx.try_recv(), Ok(true)) {
            break;
        }
        spins += 1;
        assert!(spins < 100_000, "the client never committed with a buffer");
    }
    assert!(
        crate::window::surface_has_buffer(&surface),
        "a surface with an attached buffer has a current buffer"
    );
}

#[test]
fn ssd_frame_rect_uses_committed_geometry_plus_insets() {
    use smithay::utils::{Point, Rectangle, Size};

    let insets = crate::decoration::Insets {
        top: 32.0,
        left: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    let configured = Size::from((1280, 708));
    let geometry = Rectangle::from_size(Size::from((640, 420)));
    let rect = crate::scene_elements::ssd_frame_rect(
        Point::from((0, 0)),
        configured,
        geometry,
        insets,
        false,
    );
    assert_eq!(
        rect.size,
        Size::from((640, 452)),
        "frame size is committed geometry + insets"
    );
}

#[test]
fn ssd_frame_rect_keeps_configured_size_while_resizing() {
    use smithay::utils::{Point, Rectangle, Size};

    let insets = crate::decoration::Insets::ZERO;
    let configured = Size::from((800, 600));
    let geometry = Rectangle::from_size(Size::from((640, 420)));
    let rect = crate::scene_elements::ssd_frame_rect(
        Point::from((100, 50)),
        configured,
        geometry,
        insets,
        true,
    );
    assert_eq!(
        rect.size, configured,
        "during resize the frame follows the configured/pointer size"
    );
    assert_eq!(
        rect.loc,
        Point::from((100, 50)),
        "location anchors the dragged edge"
    );
}

#[test]
fn unplaced_toplevel_gets_frame_callbacks() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use nix::poll::{PollFd, PollFlags, poll};

    use sctk::reexports::client::globals::GlobalListContents;
    use sctk::reexports::client::globals::registry_queue_init;
    use sctk::reexports::client::protocol::{wl_callback, wl_compositor, wl_registry, wl_surface};
    use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
    use sctk::reexports::protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

    let (_event_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));

    let (client_side, server_side) = std::os::unix::net::UnixStream::pair().expect("a socket pair");
    let (done_tx, done_rx) = mpsc::channel::<()>();

    let client_handle = std::thread::spawn(move || {
        struct Client {
            frame_done: bool,
            done_tx: mpsc::Sender<()>,
        }

        impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Client {
            fn event(
                _: &mut Self,
                _: &wl_registry::WlRegistry,
                _: wl_registry::Event,
                _: &GlobalListContents,
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }

        sctk::reexports::client::delegate_noop!(Client: ignore wl_compositor::WlCompositor);

        impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
            fn event(
                _: &mut Self,
                wm_base: &xdg_wm_base::XdgWmBase,
                event: xdg_wm_base::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                if let xdg_wm_base::Event::Ping { serial } = event {
                    wm_base.pong(serial);
                }
            }
        }

        impl Dispatch<xdg_surface::XdgSurface, ()> for Client {
            fn event(
                _: &mut Self,
                surface: &xdg_surface::XdgSurface,
                event: xdg_surface::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                if let xdg_surface::Event::Configure { serial } = event {
                    surface.ack_configure(serial);
                }
            }
        }

        sctk::reexports::client::delegate_noop!(Client: ignore xdg_toplevel::XdgToplevel);

        impl Dispatch<wl_callback::WlCallback, ()> for Client {
            fn event(
                state: &mut Self,
                _: &wl_callback::WlCallback,
                event: wl_callback::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                if let wl_callback::Event::Done { .. } = event {
                    state.frame_done = true;
                    let _ = state.done_tx.send(());
                }
            }
        }

        sctk::reexports::client::delegate_noop!(Client: ignore wl_surface::WlSurface);

        let conn = Connection::from_socket(client_side).expect("client connects");
        let (globals, mut event_queue) = registry_queue_init(&conn).expect("registry initializes");
        let qh = event_queue.handle();
        let mut client = Client {
            frame_done: false,
            done_tx,
        };

        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=5, ())
            .expect("compositor global exists");
        let xdg_wm = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&qh, 1..=6, ())
            .expect("xdg_wm_base global exists");

        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = xdg_wm.get_xdg_surface(&surface, &qh, ());
        let _toplevel = xdg_surface.get_toplevel(&qh, ());
        surface.frame(&qh, ());
        surface.commit();

        event_queue
            .roundtrip(&mut client)
            .expect("initial configure reaches the client");

        let mut spins = 0;
        while !client.frame_done {
            event_queue
                .blocking_dispatch(&mut client)
                .expect("frame callback is delivered");
            spins += 1;
            assert!(spins < 1000, "frame callback never arrived");
        }
    });

    let _client = crate::test_support::insert_client(&display, server_side);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        if state.windows.len() == 1 {
            break;
        }
        if Instant::now() >= deadline {
            panic!("the toplevel was never tracked");
        }
        let fd = display.backend().poll_fd();
        let poll_fd = PollFd::new(fd, PollFlags::POLLIN);
        let _ = poll(&mut [poll_fd], 1u8);
    }

    let unplaced = state
        .windows
        .surfaces()
        .next()
        .map(|surface| !state.windows.is_placed(surface))
        .unwrap_or(false);
    assert!(
        unplaced,
        "the toplevel must not be placed before its first buffer"
    );

    super::send_frame_callbacks_for_state(&state, Duration::from_millis(1));
    display.flush_clients().expect("flush callbacks");

    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the client should receive the frame callback");

    if let Err(error) = client_handle.join() {
        std::panic::resume_unwind(error);
    }
}

#[test]
fn the_estimated_vblank_lands_on_the_refresh_grid() {
    let refresh = std::time::Duration::from_millis(16);
    let delay = crate::frame_clock::next_vblank_delay(
        Some(std::time::Duration::from_millis(10)),
        std::time::Duration::from_millis(35),
        refresh,
    );
    assert_eq!(delay, std::time::Duration::from_millis(7));
}

#[test]
fn without_a_past_vblank_the_estimate_waits_one_refresh() {
    let refresh = std::time::Duration::from_millis(16);
    let now = std::time::Duration::from_millis(35);
    assert_eq!(
        crate::frame_clock::next_vblank_delay(None, now, refresh),
        refresh
    );
    assert_eq!(
        crate::frame_clock::next_vblank_delay(
            Some(std::time::Duration::from_millis(50)),
            now,
            refresh
        ),
        refresh
    );
}
