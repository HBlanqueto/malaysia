use std::fs::OpenOptions;
use std::os::unix::fs::FileExt;
use std::os::unix::io::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

use ctappearance::ColorScheme;
use infinity_protocol::client::style::infinity_window_style_v1::infinity_window_style_v1::{
    self as wire, InfinityWindowStyleV1,
};
use sctk::reexports::client::globals::{GlobalListContents, registry_queue_init};
use sctk::reexports::client::protocol::{
    wl_buffer, wl_compositor, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_subcompositor,
    wl_subsurface, wl_surface,
};
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use sctk::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use sctk::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
use smithay::reexports::wayland_server::{Client, DisplayHandle, Resource};
use smithay::utils::{Size, Transform};
use smithay::wayland::compositor::get_parent;
use wayland_backend::protocol::{Argument, Message};

use super::*;
use crate::scene_elements::build_window_elements;
use crate::state::State;
use crate::test_support::{insert_client, test_state, try_test_renderer};
use crate::window_style::{Material as Requested, ScrollEdgeEffectStyle};

const WIDTH: i32 = 400;
const HEIGHT: i32 = 300;
const BAND: i32 = 96;
const PERIOD: i32 = 64;
const MARK_X: i32 = 24;
const MARK_WIDTH: i32 = 80;
const MARK_HEIGHT: i32 = 12;
const MARK_DROP: i32 = 36;
const RED: [u8; 4] = [235, 64, 52, 255];
const BLUE: [u8; 4] = [38, 70, 210, 255];
const MARK: [u8; 4] = [40, 200, 80, 255];
const TOLERANCE: u8 = 3;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Layout {
    Above,
    Below,
}

#[derive(Clone, Copy, PartialEq)]
enum Content {
    Split,
    Stripes,
}

#[derive(Clone, Copy)]
enum Ask {
    Nothing,
    Band(wire::BlendingMode, wire::Material),
    Rects(wire::BlendingMode, wire::Material, u32, &'static [[i32; 4]]),
    Raw(u32, u32, &'static [[i32; 4]]),
    ThenUnknown(wire::Material),
    Edge(wire::Edge, wire::ScrollEdgeEffectStyle, u32),
}

#[derive(Clone, Copy)]
struct Spec {
    width: i32,
    height: i32,
    layout: Layout,
    content: Content,
    ask: Ask,
    scale: i32,
    version: u32,
    desync: bool,
    full: bool,
    moving: bool,
    frames: u32,
    both_regions: bool,
    nested: bool,
}

impl Spec {
    fn new(layout: Layout, ask: Ask) -> Self {
        Self {
            width: WIDTH,
            height: HEIGHT,
            layout,
            content: Content::Split,
            ask,
            scale: 1,
            version: 10,
            desync: false,
            full: false,
            moving: false,
            frames: 1,
            both_regions: false,
            nested: false,
        }
    }

    fn physical(&self) -> Size<i32, Physical> {
        Size::from((self.width * self.scale, self.height * self.scale))
    }

    fn chrome_height(&self) -> i32 {
        if self.full { self.height } else { BAND }
    }

    fn band_top(&self, frame: u32) -> i32 {
        if self.moving {
            (frame as i32 * 9) % (self.height - BAND)
        } else {
            0
        }
    }

    fn band(&self, frame: u32) -> [i32; 4] {
        [0, self.band_top(frame), self.width, BAND]
    }
}

fn argb(rgba: [u8; 4]) -> [u8; 4] {
    [rgba[2], rgba[1], rgba[0], rgba[3]]
}

fn stripe(y: i32, frame: u32) -> [u8; 4] {
    if (y + (frame as i32 * 7) % PERIOD) % PERIOD < PERIOD / 2 {
        RED
    } else {
        BLUE
    }
}

fn content_at(spec: &Spec, x: i32, y: i32, frame: u32) -> [u8; 4] {
    match spec.content {
        Content::Split => {
            if x < spec.width / 2 {
                RED
            } else {
                BLUE
            }
        }
        Content::Stripes => stripe(y, frame),
    }
}

fn content_pixels(spec: &Spec, frame: u32) -> Vec<u8> {
    let scale = spec.scale;
    (0..spec.height * scale)
        .flat_map(|y| {
            (0..spec.width * scale)
                .flat_map(move |x| argb(content_at(spec, x / scale, y / scale, frame)))
        })
        .collect()
}

fn chrome_pixels(spec: &Spec, frame: u32) -> Vec<u8> {
    let scale = spec.scale;
    let width = (spec.width * scale) as usize;
    let mut out = vec![0u8; width * (spec.chrome_height() * scale) as usize * 4];
    let top = (spec.band_top(frame) + MARK_DROP) * scale;
    for y in top..top + MARK_HEIGHT * scale {
        for x in MARK_X * scale..(MARK_X + MARK_WIDTH) * scale {
            let at = (y as usize * width + x as usize) * 4;
            out[at..at + 4].copy_from_slice(&argb(MARK));
        }
    }
    out
}

fn rect_bytes(rects: &[[i32; 4]]) -> Vec<u8> {
    rects
        .iter()
        .flatten()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

struct Probe;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Probe {
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

delegate_noop!(Probe: wl_compositor::WlCompositor);
delegate_noop!(Probe: wl_subcompositor::WlSubcompositor);
delegate_noop!(Probe: wl_subsurface::WlSubsurface);
delegate_noop!(Probe: wl_region::WlRegion);
delegate_noop!(Probe: wl_shm_pool::WlShmPool);
delegate_noop!(Probe: ExtBackgroundEffectSurfaceV1);
delegate_noop!(Probe: ignore wl_shm::WlShm);
delegate_noop!(Probe: ignore wl_buffer::WlBuffer);
delegate_noop!(Probe: ignore wl_surface::WlSurface);
delegate_noop!(Probe: ignore ExtBackgroundEffectManagerV1);
delegate_noop!(Probe: ignore InfinityWindowStyleV1);

struct Layer {
    surface: wl_surface::WlSurface,
    pool: wl_shm_pool::WlShmPool,
    file: std::fs::File,
    width: i32,
    height: i32,
}

impl Layer {
    fn new(
        (compositor, shm): (&wl_compositor::WlCompositor, &wl_shm::WlShm),
        qh: &QueueHandle<Probe>,
        name: &str,
        (width, height): (i32, i32),
        scale: i32,
    ) -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let bytes = (width * height * 4 * 2) as u64;
        let path =
            std::env::temp_dir().join(format!("within-{}-{unique}-{name}", std::process::id()));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        file.set_len(bytes).unwrap();
        std::fs::remove_file(&path).ok();
        let pool = shm.create_pool(file.as_fd(), bytes as i32, qh, ());
        let surface = compositor.create_surface(qh, ());
        surface.set_buffer_scale(scale);
        Self {
            surface,
            pool,
            file,
            width,
            height,
        }
    }

    fn attach(&self, frame: u32, pixels: &[u8], qh: &QueueHandle<Probe>) {
        let offset = (frame % 2) as usize * pixels.len();
        self.file.write_all_at(pixels, offset as u64).unwrap();
        let buffer = self.pool.create_buffer(
            offset as i32,
            self.width,
            self.height,
            self.width * 4,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );
        self.surface.attach(Some(&buffer), 0, 0);
        self.surface.damage_buffer(0, 0, self.width, self.height);
    }
}

enum Msg {
    Ready,
    Mid(u32),
    Frame(u32),
}

fn send_ask(
    spec: &Spec,
    frame: u32,
    (connection, style, surface): (&Connection, &InfinityWindowStyleV1, &wl_surface::WlSurface),
) {
    match spec.ask {
        Ask::Nothing => {}
        Ask::Band(blending, material) => {
            style.set_backdrop(
                surface,
                blending,
                material,
                0,
                rect_bytes(&[spec.band(frame)]),
            );
        }
        Ask::Rects(blending, material, radius, rects) => {
            style.set_backdrop(surface, blending, material, radius, rect_bytes(rects));
        }
        Ask::ThenUnknown(material) => {
            style.set_backdrop(
                surface,
                wire::BlendingMode::WithinWindow,
                material,
                0,
                rect_bytes(&[spec.band(frame)]),
            );
            raw_backdrop(connection, style, surface, (7, 2, &[spec.band(frame)]));
        }
        Ask::Raw(blending, material, rects) => {
            raw_backdrop(connection, style, surface, (blending, material, rects))
        }
        Ask::Edge(edge, shape, depth) => style.set_scroll_edge(surface, edge, shape, depth),
    }
}

fn raw_backdrop(
    connection: &Connection,
    style: &InfinityWindowStyleV1,
    surface: &wl_surface::WlSurface,
    (blending, material, rects): (u32, u32, &[[i32; 4]]),
) {
    let message = Message {
        sender_id: style.id(),
        opcode: wire::REQ_SET_BACKDROP_OPCODE,
        args: [
            Argument::Object(surface.id()),
            Argument::Uint(blending),
            Argument::Uint(material),
            Argument::Uint(0),
            Argument::Array(Box::new(rect_bytes(rects))),
        ]
        .into_iter()
        .collect(),
    };
    connection
        .backend()
        .send_request(message, None, None)
        .unwrap();
}

fn client(spec: Spec, socket: UnixStream, to_main: Sender<Msg>, from_main: Receiver<()>) {
    let connection = Connection::from_socket(socket).unwrap();
    let (globals, mut queue) = registry_queue_init::<Probe>(&connection).unwrap();
    let qh = queue.handle();
    let compositor: wl_compositor::WlCompositor = globals.bind(&qh, 3..=6, ()).unwrap();
    let subcompositor: wl_subcompositor::WlSubcompositor = globals.bind(&qh, 1..=1, ()).unwrap();
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).unwrap();
    let effects: ExtBackgroundEffectManagerV1 = globals.bind(&qh, 1..=1, ()).unwrap();
    let style: Option<InfinityWindowStyleV1> =
        (spec.version > 0).then(|| globals.bind(&qh, 1..=spec.version, ()).unwrap());
    if let Some(style) = &style {
        assert_eq!(
            style.version(),
            spec.version,
            "the compositor serves up to 10"
        );
    }
    let mut probe = Probe;
    let content = Layer::new(
        (&compositor, &shm),
        &qh,
        "content",
        (spec.width * spec.scale, spec.height * spec.scale),
        spec.scale,
    );
    let chrome = Layer::new(
        (&compositor, &shm),
        &qh,
        "chrome",
        (spec.width * spec.scale, spec.chrome_height() * spec.scale),
        spec.scale,
    );
    let (root, sub) = match spec.layout {
        Layout::Above => (&content, &chrome),
        Layout::Below => (&chrome, &content),
    };
    let middle = spec.nested.then(|| {
        Layer::new(
            (&compositor, &shm),
            &qh,
            "middle",
            (spec.width * spec.scale, BAND * spec.scale),
            spec.scale,
        )
    });
    let _middle_sub = middle
        .as_ref()
        .map(|middle| subcompositor.get_subsurface(&middle.surface, &root.surface, &qh, ()));
    let subsurface = subcompositor.get_subsurface(&sub.surface, &root.surface, &qh, ());
    if spec.layout == Layout::Below {
        subsurface.place_below(&root.surface);
    }
    if spec.desync {
        subsurface.set_desync();
    }
    let effect = effects.get_background_effect(&chrome.surface, &qh, ());
    queue.roundtrip(&mut probe).unwrap();
    to_main.send(Msg::Ready).unwrap();
    from_main.recv().unwrap();

    for frame in 0..spec.frames {
        content.attach(frame, &content_pixels(&spec, frame), &qh);
        if frame == 0 || spec.moving {
            chrome.attach(frame, &chrome_pixels(&spec, frame), &qh);
        }
        if let Some(style) = &style {
            send_ask(&spec, frame, (&connection, style, &chrome.surface));
        }
        if let (Some(middle), Some(style)) = (&middle, &style) {
            middle.attach(
                frame,
                &vec![0; (spec.width * BAND * spec.scale * spec.scale * 4) as usize],
                &qh,
            );
            style.set_backdrop(
                &middle.surface,
                wire::BlendingMode::WithinWindow,
                wire::Material::Sidebar,
                0,
                rect_bytes(&[[0, 0, spec.width, BAND]]),
            );
            middle.surface.commit();
        }
        if spec.both_regions {
            let region = compositor.create_region(&qh, ());
            region.add(0, 0, WIDTH, BAND);
            effect.set_blur_region(Some(&region));
            region.destroy();
        }
        sub.surface.commit();
        queue.roundtrip(&mut probe).unwrap();
        if frame > 0 {
            to_main.send(Msg::Mid(frame)).unwrap();
            from_main.recv().unwrap();
        }
        root.surface.commit();
        queue.roundtrip(&mut probe).unwrap();
        to_main.send(Msg::Frame(frame)).unwrap();
        from_main.recv().unwrap();
    }
}

fn next(
    display: &mut smithay::reexports::wayland_server::Display<State>,
    state: &mut State,
    from_client: &Receiver<Msg>,
) -> Msg {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        display.dispatch_clients(state).unwrap();
        display.flush_clients().unwrap();
        match from_client.try_recv() {
            Ok(message) => return message,
            Err(TryRecvError::Empty) => {
                assert!(Instant::now() < deadline, "the client stalled");
                std::thread::sleep(Duration::from_micros(50));
            }
            Err(TryRecvError::Disconnected) => panic!("the client died"),
        }
    }
}

fn surfaces_of(handle: &DisplayHandle, client: &Client) -> Vec<WlSurface> {
    let backend = handle.backend_handle();
    let mut ids = Vec::new();
    let _ = backend.with_all_objects_for(client.id(), |id| ids.push(id));
    ids.into_iter()
        .filter(|id| {
            handle
                .object_info(id.clone())
                .is_ok_and(|info| info.interface.name == "wl_surface")
        })
        .filter_map(|id| WlSurface::from_id(handle, id).ok())
        .collect()
}

#[allow(unsafe_code)]
fn finish(renderer: &mut GlesRenderer) {
    renderer.with_context(|gl| unsafe { gl.Finish() }).unwrap();
}

#[allow(unsafe_code)]
fn renderer_name(renderer: &mut GlesRenderer) -> String {
    renderer
        .with_context(|gl| unsafe {
            std::ffi::CStr::from_ptr(
                gl.GetString(smithay::backend::renderer::gles::ffi::RENDERER) as *const _,
            )
            .to_string_lossy()
            .into_owned()
        })
        .unwrap()
}

struct Persistent {
    target: GlesTexture,
    tracker: OutputDamageTracker,
    size: Size<i32, Physical>,
    drawn: bool,
}

impl Persistent {
    fn new(renderer: &mut GlesRenderer, size: Size<i32, Physical>, scale: f64) -> Self {
        Self {
            target: Offscreen::<GlesTexture>::create_buffer(
                renderer,
                Fourcc::Abgr8888,
                Size::<i32, BufferCoords>::from((size.w, size.h)),
            )
            .unwrap(),
            tracker: OutputDamageTracker::new(size, Scale::from(scale), Transform::Normal),
            size,
            drawn: false,
        }
    }

    fn draw(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &[OutputRenderElement<GlesRenderer>],
    ) -> Vec<u8> {
        let age = usize::from(self.drawn);
        self.drawn = true;
        let buffer_size = Size::<i32, BufferCoords>::from((self.size.w, self.size.h));
        let mut framebuffer = renderer.bind(&mut self.target).unwrap();
        self.tracker
            .render_output(renderer, &mut framebuffer, age, elements, [0.0; 4])
            .unwrap();
        let mapping = renderer
            .copy_framebuffer(
                &framebuffer,
                Rectangle::from_size(buffer_size),
                Fourcc::Abgr8888,
            )
            .unwrap();
        renderer.map_texture(&mapping).unwrap().to_vec()
    }
}

fn read(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: f64,
    elements: &[OutputRenderElement<GlesRenderer>],
) -> Vec<u8> {
    Persistent::new(renderer, size, scale).draw(renderer, elements)
}

struct World {
    renderer: GlesRenderer,
    state: State,
    root: WlSurface,
    requester: WlSurface,
    spec: Spec,
}

impl World {
    fn capture(&mut self, active: bool) -> Vec<u8> {
        let scale = f64::from(self.spec.scale);
        let elements = build_window_elements::<GlesRenderer>(
            &mut self.state,
            &mut self.renderer,
            (&self.root, active),
            scale,
        );
        read(&mut self.renderer, self.spec.physical(), scale, &elements)
    }

    fn scene(&self) -> Scene {
        let scale = f64::from(self.spec.scale);
        Scene {
            origin: Point::default(),
            frame: Shape {
                rect: Rectangle::from_size(self.spec.physical()),
                radius: 0,
                exponent: ctdesign::CIRCLE_EXPONENT,
            },
            scale,
            active: true,
            arrow: Arrow::default(),
            deep: false,
        }
    }

    fn on_screen(&mut self) -> Vec<OutputRenderElement<GlesRenderer>> {
        let scene = self.scene();
        self.on_screen_in(&scene)
    }

    fn on_screen_in(&mut self, scene: &Scene) -> Vec<OutputRenderElement<GlesRenderer>> {
        let (mut within, mut behind) = (Vec::new(), Vec::new());
        append(
            &mut self.state,
            &mut self.renderer,
            (&self.root, &self.requester),
            scene,
            (&mut within, Some(&mut behind)),
        );
        behind.extend(within);
        behind
    }

    fn slot_commits(&self) -> Vec<CommitCounter> {
        let blur = self.state.blur.as_ref().unwrap();
        let mut commits: Vec<_> = blur
            .within
            .slots
            .iter()
            .filter_map(|(key, slot)| slot.behind.as_ref().map(|behind| (key.1, behind.commit)))
            .collect();
        commits.sort_by_key(|(slot, _)| *slot);
        commits.into_iter().map(|(_, commit)| commit).collect()
    }

    fn pixel(&self, pixels: &[u8], x: i32, y: i32) -> [u8; 4] {
        let scale = self.spec.scale;
        let at = ((y * scale + scale / 2) * self.spec.width * scale + x * scale + scale / 2)
            as usize
            * 4;
        pixels[at..at + 4].try_into().unwrap()
    }
}

fn drive(spec: Spec, scheme: ColorScheme, mut observe: impl FnMut(&mut World, &Msg)) -> Option<()> {
    let mut renderer = try_test_renderer()?;
    let (_event_loop, mut display, mut state) = test_state((1280, 720));
    state.blur = crate::blur::compile(&mut renderer);
    state.rounded_clip = crate::effects::compile_rounded_clip(&mut renderer);
    state.chrome_theme.resolved.scheme = scheme;
    let (client_side, server_side) = UnixStream::pair().unwrap();
    let (to_main, from_client) = channel::<Msg>();
    let (ack, from_main) = channel::<()>();
    let thread = std::thread::spawn(move || client(spec, client_side, to_main, from_main));
    let handle = display.handle();
    let wl_client = insert_client(&display, server_side);
    assert!(matches!(
        next(&mut display, &mut state, &from_client),
        Msg::Ready
    ));
    let surfaces = surfaces_of(&handle, &wl_client);
    assert_eq!(surfaces.len(), if spec.nested { 3 } else { 2 });
    let root = surfaces
        .iter()
        .find(|surface| get_parent(surface).is_none())
        .unwrap()
        .clone();
    let sub = surfaces
        .iter()
        .find(|surface| get_parent(surface).is_some())
        .unwrap()
        .clone();
    let requester = match spec.layout {
        Layout::Above => sub,
        Layout::Below => root.clone(),
    };
    let mut world = World {
        renderer,
        state,
        root,
        requester,
        spec,
    };
    ack.send(()).unwrap();
    let mut seen = 0;
    while seen < spec.frames {
        let message = next(&mut display, &mut world.state, &from_client);
        observe(&mut world, &message);
        if matches!(message, Msg::Frame(_)) {
            seen += 1;
        }
        ack.send(()).unwrap();
    }
    thread.join().unwrap();
    Some(())
}

fn once(spec: Spec, scheme: ColorScheme, mut look_at: impl FnMut(&mut World)) -> Option<()> {
    drive(spec, scheme, |world, message| {
        if matches!(message, Msg::Frame(_)) {
            look_at(world);
        }
    })
}

fn close(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= tolerance)
}

fn encoded(shade: [f32; 3]) -> [u8; 4] {
    let level = |channel: f32| (channel * 255.0).round() as u8;
    [level(shade[0]), level(shade[1]), level(shade[2]), 255]
}

fn blurred_row(spec: &Spec, sigma: f32) -> Vec<[f32; 3]> {
    let scale = spec.scale;
    let channels: [Vec<f32>; 3] = std::array::from_fn(|channel| {
        let line: Vec<f32> = (0..spec.width * scale)
            .map(|x| f32::from(content_at(spec, x / scale, 0, 0)[channel]) / 255.0)
            .collect();
        ctappearance::material::blur_region(&line, sigma * scale as f32)
    });
    (0..channels[0].len())
        .map(|at| [channels[0][at], channels[1][at], channels[2][at]])
        .collect()
}

fn the_model_of(
    material: ctappearance::material::Material,
    scheme: ColorScheme,
) -> ctdesign::MaterialLook {
    ctappearance::material::looks(material, scheme).active
}

fn model_row(spec: &Spec, look: &ctdesign::MaterialLook) -> Vec<[u8; 4]> {
    blurred_row(spec, look.sigma)
        .into_iter()
        .map(|backdrop| encoded(ctappearance::material::over(look, backdrop)))
        .collect()
}

const MATERIALS: [(wire::Material, ctappearance::material::Material); 4] = [
    (
        wire::Material::Titlebar,
        ctappearance::material::Material::Titlebar,
    ),
    (wire::Material::Menu, ctappearance::material::Material::Menu),
    (
        wire::Material::Sidebar,
        ctappearance::material::Material::Sidebar,
    ),
    (
        wire::Material::HudWindow,
        ctappearance::material::Material::HudWindow,
    ),
];

#[test]
fn a_within_window_material_blurs_what_lies_below_the_requester_in_both_layouts_at_every_scale() {
    for layout in [Layout::Above, Layout::Below] {
        for scale in [1, 2] {
            for scheme in [ColorScheme::Light, ColorScheme::Dark] {
                for (asked, model) in MATERIALS {
                    let mut spec =
                        Spec::new(layout, Ask::Band(wire::BlendingMode::WithinWindow, asked));
                    spec.scale = scale;
                    let look = the_model_of(model, scheme);
                    let expected = model_row(&spec, &look);
                    let ran = once(spec, scheme, |world| {
                        let pixels = world.capture(true);
                        for x in (4..WIDTH - 4).step_by(12) {
                            let seen = world.pixel(&pixels, x, 70);
                            let want = expected[(x * scale + scale / 2) as usize];
                            assert!(
                                close(seen, want, TOLERANCE),
                                "{layout:?} {scale}x {model:?} {scheme:?} x={x}: {seen:?} against {want:?}"
                            );
                        }
                        for x in (4..WIDTH - 4).step_by(40) {
                            let below = world.pixel(&pixels, x, BAND + 20);
                            assert!(
                                close(below, content_at(&spec, x, 0, 0), 1),
                                "{layout:?} {scale}x: under the band the content stays sharp"
                            );
                        }
                        let mark = world.pixel(
                            &pixels,
                            MARK_X + MARK_WIDTH / 2,
                            MARK_DROP + MARK_HEIGHT / 2,
                        );
                        assert!(
                            close(mark, MARK, 1),
                            "the requester's own pixels stay sharp"
                        );
                    });
                    if ran.is_none() {
                        eprintln!("skipping: no EGL device");
                        return;
                    }
                }
            }
        }
    }
}

#[test]
fn the_requester_and_what_is_above_it_stay_in_front_of_the_material() {
    let mut spec = Spec::new(
        Layout::Above,
        Ask::Band(wire::BlendingMode::WithinWindow, wire::Material::Titlebar),
    );
    spec.content = Content::Split;
    let Some(()) = once(spec, ColorScheme::Light, |world| {
        let elements = world.on_screen();
        assert_eq!(elements.len(), 1, "one rectangle, one element");
        let mark = world.capture(true);
        assert!(close(
            world.pixel(&mark, MARK_X + 4, MARK_DROP + 4),
            MARK,
            1
        ));
    }) else {
        eprintln!("skipping: no EGL device");
        return;
    };
}

#[test]
fn reduce_transparency_draws_the_solid_and_increase_contrast_alone_keeps_the_blur() {
    let spec = Spec::new(
        Layout::Above,
        Ask::Band(wire::BlendingMode::WithinWindow, wire::Material::Titlebar),
    );
    let scheme = ColorScheme::Dark;
    let set = ctappearance::material::looks(ctappearance::material::Material::Titlebar, scheme);
    let sample = |world: &mut World, active: bool| {
        let pixels = world.capture(active);
        (
            world.pixel(&pixels, 40, 70),
            world.pixel(&pixels, WIDTH - 40, 70),
        )
    };
    let ran = once(spec, scheme, |world| {
        let mut gray =
            |reduce: bool, contrast: bool, active: bool, want: ctdesign::MaterialLook| {
                world.state.config.appearance.reduce_transparency = reduce;
                world.state.config.appearance.contrast = contrast;
                let (left, right) = sample(world, active);
                let solid = encoded([want.veil.r, want.veil.g, want.veil.b]);
                assert!(
                    close(left, solid, 1),
                    "{reduce} {contrast} {active}: {left:?}"
                );
                assert!(
                    close(right, solid, 1),
                    "a solid shows nothing of the backdrop"
                );
            };
        gray(true, false, true, set.reduced);
        gray(true, true, true, set.reduced_contrast);
        gray(false, false, false, set.inactive);
        let mut contrast_only = |contrast: bool| {
            world.state.config.appearance.reduce_transparency = false;
            world.state.config.appearance.contrast = contrast;
            sample(world, true)
        };
        assert_eq!(contrast_only(true), contrast_only(false));
        let (left, right) = contrast_only(true);
        assert_ne!(left, right, "the blur still shows the two halves");
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn a_behind_window_material_is_drawn_on_screen_and_left_out_of_a_window_capture() {
    let spec = Spec::new(
        Layout::Above,
        Ask::Band(wire::BlendingMode::BehindWindow, wire::Material::Titlebar),
    );
    let scheme = ColorScheme::Light;
    let ran = once(spec, scheme, |world| {
        let pixels = world.capture(true);
        for x in [40, WIDTH - 40] {
            assert!(
                close(world.pixel(&pixels, x, 70), content_at(&spec, x, 0, 0), 1),
                "the capture holds no behind-window material"
            );
        }
        let elements = world.on_screen();
        assert_eq!(elements.len(), 1, "the screen draws it");
        let scale = f64::from(spec.scale);
        let drawn = read(&mut world.renderer, spec.physical(), scale, &elements);
        let look = the_model_of(ctappearance::material::Material::Titlebar, scheme);
        let clear = ctdesign::DESKTOP_CLEAR;
        let want = encoded(ctappearance::material::over(
            &look,
            [clear.r, clear.g, clear.b],
        ));
        let seen = world.pixel(&drawn, 200, 70);
        assert!(close(seen, want, TOLERANCE), "{seen:?} against {want:?}");
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn several_rectangles_blur_independently_and_the_sixteenth_is_the_last_one_kept() {
    static TWO: [[i32; 4]; 2] = [[0, 0, WIDTH, 40], [0, 120, WIDTH, 40]];
    let spec = Spec::new(
        Layout::Above,
        Ask::Rects(
            wire::BlendingMode::WithinWindow,
            wire::Material::Titlebar,
            0,
            &TWO,
        ),
    );
    let mut spec = spec;
    spec.full = true;
    let ran = once(spec, ColorScheme::Light, |world| {
        let pixels = world.capture(true);
        let sharp = |y| {
            close(
                world.pixel(&pixels, WIDTH / 2 - 6, y),
                content_at(&spec, WIDTH / 2 - 6, 0, 0),
                1,
            )
        };
        assert!(!sharp(20) && !sharp(140), "both rectangles are blurred");
        assert!(
            sharp(80) && sharp(200),
            "what lies between and after them is not"
        );
        assert_eq!(world.slot_commits().len(), 2);
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn a_rectangle_beyond_the_sixteenth_is_dropped_and_a_corner_radius_rounds_the_material() {
    static MANY: [[i32; 4]; 20] = {
        let mut rects = [[0, 0, 0, 0]; 20];
        let mut at = 0;
        while at < 20 {
            rects[at] = [0, at as i32 * 10, 8, 8];
            at += 1;
        }
        rects
    };
    let mut spec = Spec::new(
        Layout::Above,
        Ask::Rects(
            wire::BlendingMode::WithinWindow,
            wire::Material::Titlebar,
            0,
            &MANY,
        ),
    );
    spec.full = true;
    let ran = once(spec, ColorScheme::Light, |world| {
        let kept = window_style::backdrop(&world.requester).unwrap();
        assert_eq!(kept.rects().len(), 16);
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
    static ROUND: [[i32; 4]; 1] = [[0, 0, WIDTH, BAND]];
    let spec = Spec::new(
        Layout::Above,
        Ask::Rects(
            wire::BlendingMode::WithinWindow,
            wire::Material::Titlebar,
            24,
            &ROUND,
        ),
    );
    let ran = once(spec, ColorScheme::Light, |world| {
        let pixels = world.capture(true);
        let corner = world.pixel(&pixels, 1, 1);
        assert!(
            close(corner, content_at(&spec, 1, 1, 0), 1),
            "the corner of a rounded rectangle shows the sharp content: {corner:?}"
        );
        assert!(!close(
            world.pixel(&pixels, 200, 50),
            content_at(&spec, 200, 50, 0),
            40
        ));
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn an_invalid_request_is_ignored_and_the_previous_one_stays() {
    static GOOD: [[i32; 4]; 1] = [[0, 0, WIDTH, BAND]];
    let cases: [(u32, u32, &'static [[i32; 4]]); 3] = [(7, 2, &GOOD), (1, 99, &GOOD), (1, 2, &[])];
    for (blending, material, rects) in cases {
        let mut spec = Spec::new(Layout::Above, Ask::Raw(blending, material, rects));
        spec.frames = 1;
        let ran = once(spec, ColorScheme::Light, |world| {
            assert!(
                window_style::backdrop(&world.requester).is_none(),
                "{blending} {material} {}: ignored or empty, so nothing is asked",
                rects.len()
            );
        });
        if ran.is_none() {
            eprintln!("skipping: no EGL device");
            return;
        }
    }
}

#[test]
fn an_unknown_value_leaves_the_earlier_backdrop_in_place() {
    let spec = Spec::new(Layout::Above, Ask::ThenUnknown(wire::Material::Sidebar));
    let ran = once(spec, ColorScheme::Light, |world| {
        let kept = window_style::backdrop(&world.requester).unwrap();
        assert!(kept.material == Requested::Sidebar);
        assert_eq!(kept.rects().len(), 1);
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn a_client_that_bound_version_nine_works_as_before_and_is_not_served_the_new_requests() {
    let mut spec = Spec::new(Layout::Above, Ask::Nothing);
    spec.version = 9;
    let ran = once(spec, ColorScheme::Light, |world| {
        assert!(window_style::is_kit_client(&world.root));
        assert!(window_style::backdrop(&world.requester).is_none());
        assert!(window_style::scroll_edge(&world.requester).is_none());
        assert!(
            requesters(&world.root)
                .of(&Id::from_wayland_resource(&world.requester))
                .is_none()
        );
        let scale = f64::from(world.spec.scale);
        let plain: Vec<OutputRenderElement<GlesRenderer>> = {
            let tree = render_elements_from_surface_tree::<
                GlesRenderer,
                WaylandSurfaceRenderElement<GlesRenderer>,
            >(
                &mut world.renderer,
                &world.root,
                Point::default(),
                scale,
                1.0,
                Kind::Unspecified,
            );
            tree.into_iter().map(Into::into).collect()
        };
        let before = read(&mut world.renderer, world.spec.physical(), scale, &plain);
        let after = world.capture(true);
        assert!(
            before == after,
            "a window that asks for nothing is drawn as it was"
        );
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn set_backdrop_wins_over_the_region_of_ext_background_effect_on_the_same_surface() {
    let mut spec = Spec::new(
        Layout::Above,
        Ask::Band(wire::BlendingMode::WithinWindow, wire::Material::Titlebar),
    );
    spec.both_regions = true;
    let ran = once(spec, ColorScheme::Light, |world| {
        assert!(super::super::blur_region(&world.requester).is_empty());
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
        return;
    }
    let mut spec = Spec::new(Layout::Above, Ask::Nothing);
    spec.both_regions = true;
    once(spec, ColorScheme::Light, |world| {
        assert_eq!(super::super::blur_region(&world.requester).len(), 1);
    });
}

fn scroll_edge_world(edge: wire::Edge, shape: wire::ScrollEdgeEffectStyle, depth: u32) -> Spec {
    let mut spec = Spec::new(Layout::Above, Ask::Edge(edge, shape, depth));
    spec.full = true;
    spec
}

#[test]
fn a_hard_scroll_edge_blurs_its_strip_uniformly_and_ends_in_an_edge() {
    let spec = scroll_edge_world(wire::Edge::Top, wire::ScrollEdgeEffectStyle::Hard, 64);
    let ran = once(spec, ColorScheme::Light, |world| {
        let pixels = world.capture(true);
        let x = WIDTH / 2 - 6;
        let sharp = content_at(&spec, x, 0, 0);
        for y in [2, 30, 60] {
            assert!(
                !close(world.pixel(&pixels, x, y), sharp, 20),
                "y={y} is inside the strip and blurred"
            );
        }
        assert!(
            close(world.pixel(&pixels, x, 64 + 8), sharp, 1),
            "below the strip it is sharp"
        );
        let edge = ctdesign::FULLSCREEN_BAND.blur;
        let row = blurred_row(&spec, edge);
        let want = encoded(row[x as usize]);
        assert!(
            close(world.pixel(&pixels, x, 2), want, TOLERANCE + 3),
            "uniform plain blur"
        );
        assert!(
            close(world.pixel(&pixels, x, 60), want, TOLERANCE + 3),
            "uniform to the end"
        );
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn a_soft_scroll_edge_fades_the_blur_to_nothing_across_its_depth() {
    let spec = scroll_edge_world(wire::Edge::Top, wire::ScrollEdgeEffectStyle::Soft, 96);
    let ran = once(spec, ColorScheme::Light, |world| {
        let pixels = world.capture(true);
        let x = WIDTH / 2 - 6;
        let sharp = content_at(&spec, x, 0, 0);
        let away = |y| {
            let seen = world.pixel(&pixels, x, y);
            (0..3)
                .map(|c| i32::from(seen[c].abs_diff(sharp[c])))
                .sum::<i32>()
        };
        let (edge, middle, end) = (away(2), away(60), away(93));
        assert!(edge > 30, "full blur at the edge: {edge}");
        assert!(
            middle < edge,
            "the blur has faded by the middle: {middle} {edge}"
        );
        assert!(
            end < middle / 2 + 3,
            "and is almost gone at the far end: {end} {middle}"
        );
        assert_eq!(away(96 + 12), 0, "past the depth nothing changes");
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn a_scroll_edge_strip_follows_its_edge_at_the_depth_asked_for_at_each_scale() {
    let (_loop, _display, state) = test_state((1280, 720));
    let surface = Rectangle::<i32, Physical>::new((10, 20).into(), (300, 200).into());
    for scale in [1.0, 2.0] {
        let scene = Scene {
            origin: Point::default(),
            frame: Shape {
                rect: surface,
                radius: 0,
                exponent: ctdesign::CIRCLE_EXPONENT,
            },
            scale,
            active: true,
            arrow: Arrow::default(),
            deep: false,
        };
        let strip = |edge| {
            edge_job(
                &state,
                ScrollEdge {
                    edge,
                    style: ScrollEdgeEffectStyle::Hard,
                    depth: 40,
                },
                surface,
                &scene,
            )
            .unwrap()
            .full
        };
        let depth = (40.0 * scale) as i32;
        assert_eq!(
            strip(Edge::Top),
            Rectangle::new((10, 20).into(), (300, depth).into())
        );
        assert_eq!(
            strip(Edge::Bottom),
            Rectangle::new((10, 220 - depth).into(), (300, depth).into())
        );
        assert_eq!(
            strip(Edge::Leading),
            Rectangle::new((10, 20).into(), (depth, 200).into())
        );
        assert_eq!(
            strip(Edge::Trailing),
            Rectangle::new((310 - depth, 20).into(), (depth, 200).into())
        );
    }
}

#[test]
fn a_scroll_edge_is_the_solid_of_the_titlebar_under_reduce_transparency() {
    let (_loop, _display, mut state) = test_state((1280, 720));
    state.chrome_theme.resolved.scheme = ColorScheme::Light;
    let blurred = look::scroll_edge(&state, ScrollEdgeEffectStyle::Soft);
    assert!(blurred.sigma > 0.0 && blurred.post == look::Post::Plain);
    state.config.appearance.reduce_transparency = true;
    let solid = look::scroll_edge(&state, ScrollEdgeEffectStyle::Soft);
    assert!(!solid.sees_backdrop());
    let set = ctappearance::material::looks(
        ctappearance::material::Material::Titlebar,
        ColorScheme::Light,
    );
    assert!(solid.post == look::Post::Material(set.reduced));
}

#[test]
fn every_material_of_the_protocol_has_a_look_and_the_glass_styles_use_the_glass_tokens() {
    let (_loop, _display, mut state) = test_state((1280, 720));
    state.chrome_theme.resolved.scheme = ColorScheme::Dark;
    let all = [
        Requested::Titlebar,
        Requested::Selection,
        Requested::Menu,
        Requested::Popover,
        Requested::Sidebar,
        Requested::HeaderView,
        Requested::Sheet,
        Requested::WindowBackground,
        Requested::HudWindow,
        Requested::FullScreenUi,
        Requested::ToolTip,
        Requested::ContentBackground,
        Requested::UnderWindowBackground,
        Requested::UnderPageBackground,
        Requested::GlassRegular,
        Requested::GlassClear,
    ];
    for material in all {
        assert!(
            look::requested(&state, material, true, 200.0).is_some(),
            "{material:?}"
        );
    }
    let sigma = |material| {
        look::requested(&state, material, true, 200.0)
            .unwrap()
            .sigma
    };
    assert_eq!(
        sigma(Requested::Titlebar),
        ctdesign::MATERIAL_TITLEBAR_DARK.active.sigma
    );
    assert!(sigma(Requested::GlassRegular) > 0.0);
    assert_eq!(sigma(Requested::GlassClear), 0.0);
    assert_eq!(sigma(Requested::Sheet), 0.0, "a sheet does not blur");
    let solid = |active| look::requested(&state, Requested::Menu, active, 200.0).unwrap();
    assert!(
        solid(true).sigma > 0.0 && solid(false).sigma == 0.0,
        "inactive is a solid"
    );
}

#[test]
fn a_slot_nobody_asked_for_in_two_frames_is_dropped() {
    let mut within = Within::default();
    let key = (Id::new(), 0);
    within.slots.insert(
        key.clone(),
        Slot {
            behind: None,
            id: Id::new(),
            used: within.epoch,
        },
    );
    within.sweep();
    assert!(within.slots.contains_key(&key), "one frame of grace");
    within.sweep();
    assert!(!within.slots.contains_key(&key));
}

#[test]
fn a_solid_look_ignores_the_backdrop_and_a_translucent_one_does_not() {
    let (_loop, _display, mut state) = test_state((1280, 720));
    state.config.appearance.reduce_transparency = true;
    let solid = look::requested(&state, Requested::Menu, true, 100.0).unwrap();
    assert!(!solid.sees_backdrop());
    state.config.appearance.reduce_transparency = false;
    let clear = look::requested(&state, Requested::GlassClear, true, 100.0).unwrap();
    assert!(
        clear.sees_backdrop(),
        "clear glass has no veil and shows the backdrop"
    );
}

#[test]
fn the_blur_is_drawn_again_only_when_what_lies_below_changes_and_follows_the_scroll_without_lag() {
    for layout in [Layout::Above, Layout::Below] {
        for scale in [1, 2] {
            let mut spec = Spec::new(
                layout,
                Ask::Band(wire::BlendingMode::WithinWindow, wire::Material::Titlebar),
            );
            spec.content = Content::Stripes;
            spec.scale = scale;
            spec.full = true;
            spec.moving = true;
            spec.frames = 12;
            let mut previous: Option<Vec<u8>> = None;
            let mut commits: Option<Vec<CommitCounter>> = None;
            let mut persistent: Option<Persistent> = None;
            let ran = drive(spec, ColorScheme::Light, |world, message| match message {
                Msg::Mid(frame) => {
                    let now = world.capture(true);
                    assert!(
                        previous.as_ref() == Some(&now),
                        "{layout:?} {scale}x frame {frame}: between the sub-surface's commit and the parent's nothing changes"
                    );
                }
                Msg::Frame(frame) => {
                    let now = world.capture(true);
                    let again = world.slot_commits();
                    assert!(world.capture(true) == now);
                    assert!(
                        world.slot_commits() == again,
                        "the same scene draws no new blur"
                    );
                    if let Some(before) = &commits {
                        assert!(
                            &again != before,
                            "{layout:?} {scale}x frame {frame}: the scroll is a change"
                        );
                    }
                    commits = Some(again);
                    let band = spec.band(*frame);
                    for y in (0..HEIGHT).filter(|y| *y < band[1] || *y >= band[1] + BAND) {
                        let seen = world.pixel(&now, 300, y);
                        assert!(
                            close(seen, stripe(y, *frame), 1),
                            "{layout:?} {scale}x frame {frame}: row {y} outside the band is sharp and current"
                        );
                    }
                    let mark = world.pixel(
                        &now,
                        MARK_X + MARK_WIDTH / 2,
                        band[1] + MARK_DROP + MARK_HEIGHT / 2,
                    );
                    assert!(close(mark, MARK, 1), "the mark moves with the band");
                    let inside = world.pixel(&now, 300, band[1] + 8);
                    assert!(
                        !close(inside, stripe(band[1] + 8, *frame), 10),
                        "the band is blurred"
                    );
                    let physical = spec.physical();
                    let tracked = persistent.get_or_insert_with(|| {
                        Persistent::new(&mut world.renderer, physical, f64::from(scale))
                    });
                    let elements = build_window_elements::<GlesRenderer>(
                        &mut world.state,
                        &mut world.renderer,
                        (&world.root, true),
                        f64::from(scale),
                    );
                    assert!(
                        tracked.draw(&mut world.renderer, &elements) == now,
                        "{layout:?} {scale}x frame {frame}: a frame with age 1 equals a full repaint"
                    );
                    previous = Some(now);
                }
                Msg::Ready => unreachable!(),
            });
            if ran.is_none() {
                eprintln!("skipping: no EGL device");
                return;
            }
        }
    }
}

#[test]
fn a_desynchronized_sub_surface_tears_the_material_and_the_content_apart() {
    let mut spec = Spec::new(
        Layout::Above,
        Ask::Band(wire::BlendingMode::WithinWindow, wire::Material::Titlebar),
    );
    spec.content = Content::Stripes;
    spec.full = true;
    spec.moving = true;
    spec.desync = true;
    spec.frames = 12;
    let mut previous: Option<Vec<u8>> = None;
    let mut torn = 0;
    let ran = drive(spec, ColorScheme::Light, |world, message| match message {
        Msg::Mid(_) => {
            let now = world.capture(true);
            torn += usize::from(previous.as_ref() != Some(&now));
        }
        Msg::Frame(_) => previous = Some(world.capture(true)),
        Msg::Ready => unreachable!(),
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
        return;
    }
    assert!(
        torn > 0,
        "the control tears: the request applies at the sub-surface's own commit"
    );
}

#[test]
#[ignore = "measures the frame time of the in-window pass: run with --ignored --nocapture --test-threads=1"]
fn the_band_blur_pass_costs_this_much_per_frame() {
    for scale in [1, 2] {
        let mut spec = Spec::new(
            Layout::Above,
            Ask::Band(wire::BlendingMode::WithinWindow, wire::Material::Titlebar),
        );
        spec.content = Content::Stripes;
        spec.scale = scale;
        spec.width = 1280;
        spec.height = 800;
        spec.frames = 44;
        let mut runs = Vec::new();
        let mut renderer_label = String::new();
        let ran = drive(spec, ColorScheme::Dark, |world, message| {
            let Msg::Frame(frame) = message else { return };
            if renderer_label.is_empty() {
                let name = renderer_name(&mut world.renderer);
                let software = ["llvmpipe", "softpipe", "swrast", "software"]
                    .iter()
                    .any(|word| name.to_lowercase().contains(word));
                renderer_label = if software {
                    format!("{name}, software rendering (llvmpipe)")
                } else {
                    name
                };
            }
            let started = Instant::now();
            let elements = world.on_screen();
            finish(&mut world.renderer);
            let elapsed = started.elapsed();
            assert_eq!(elements.len(), 1);
            if *frame >= 4 {
                runs.push(elapsed);
            }
        });
        if ran.is_none() {
            eprintln!("WITHIN TIMING skipped: no EGL device");
            return;
        }
        runs.sort();
        let (width, height) = (spec.width, spec.height);
        let milliseconds = |duration: Duration| duration.as_secs_f64() * 1000.0;
        let median = milliseconds(runs[runs.len() / 2]);
        eprintln!(
            "WITHIN TIMING renderer {renderer_label}: {scale}x window {width}x{height} logical, band {BAND} logical rows: source render, Gaussian passes and material, median {median:.2} ms (min {:.2}, max {:.2}) over {} frames, {:.1} % of a 60 Hz frame and {:.1} % of a 120 Hz frame",
            milliseconds(runs[0]),
            milliseconds(runs[runs.len() - 1]),
            runs.len(),
            median / (1000.0 / 60.0) * 100.0,
            median / (1000.0 / 120.0) * 100.0,
        );
    }
}

#[test]
fn a_requester_over_another_blurs_the_material_of_the_one_below_it() {
    static OVER: [[i32; 4]; 1] = [[0, 20, WIDTH, 60]];
    let mut spec = Spec::new(
        Layout::Above,
        Ask::Rects(
            wire::BlendingMode::WithinWindow,
            wire::Material::Titlebar,
            0,
            &OVER,
        ),
    );
    spec.nested = true;
    let scheme = ColorScheme::Light;
    let below = the_model_of(ctappearance::material::Material::Sidebar, scheme);
    let above = the_model_of(ctappearance::material::Material::Titlebar, scheme);
    let shaded = |line: Vec<[u8; 4]>| -> Vec<[f32; 3]> {
        line.into_iter()
            .map(|pixel| [pixel[0], pixel[1], pixel[2]].map(|channel| f32::from(channel) / 255.0))
            .collect()
    };
    let through = |backdrop: Vec<[f32; 3]>| -> Vec<[u8; 4]> {
        let channels: [Vec<f32>; 3] = std::array::from_fn(|channel| {
            let line: Vec<f32> = backdrop.iter().map(|pixel| pixel[channel]).collect();
            ctappearance::material::blur_region(&line, above.sigma)
        });
        (0..backdrop.len())
            .map(|at| {
                encoded(ctappearance::material::over(
                    &above,
                    [channels[0][at], channels[1][at], channels[2][at]],
                ))
            })
            .collect()
    };
    let nested = through(shaded(model_row(&spec, &below)));
    let raw = through(blurred_row(&spec, 0.0));
    assert!(
        (0..WIDTH as usize).any(|x| !close(nested[x], raw[x], 10)),
        "the test tells the two apart"
    );
    let ran = once(spec, scheme, |world| {
        let pixels = world.capture(true);
        for x in (4..WIDTH - 4).step_by(12) {
            let seen = world.pixel(&pixels, x, 50);
            assert!(
                close(seen, nested[x as usize], TOLERANCE + 2),
                "x={x}: {seen:?} against {:?}",
                nested[x as usize]
            );
        }
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}

#[test]
fn a_popup_or_layer_material_behind_the_window_samples_the_scene_behind_it() {
    let spec = Spec::new(
        Layout::Above,
        Ask::Band(wire::BlendingMode::BehindWindow, wire::Material::Menu),
    );
    let scheme = ColorScheme::Dark;
    let ran = once(spec, scheme, |world| {
        let scene = Scene {
            deep: true,
            ..world.scene()
        };
        let elements = world.on_screen_in(&scene);
        assert_eq!(elements.len(), 1);
        let scale = f64::from(spec.scale);
        let drawn = read(&mut world.renderer, spec.physical(), scale, &elements);
        let look = the_model_of(ctappearance::material::Material::Menu, scheme);
        let clear = ctdesign::DESKTOP_CLEAR;
        let want = encoded(ctappearance::material::over(
            &look,
            [clear.r, clear.g, clear.b],
        ));
        let seen = world.pixel(&drawn, 200, 70);
        assert!(close(seen, want, TOLERANCE), "{seen:?} against {want:?}");
    });
    if ran.is_none() {
        eprintln!("skipping: no EGL device");
    }
}
