//! The Wayland runner for Freya's core (ADR 0034).
//!
//! `freya-winit` cannot be used: it drags in winit, GTK, the tray stack and
//! glutin, and winit has no layer-shell or session-lock support. This module
//! owns the other half: it creates the surface (layer-shell or
//! ext-session-lock), an EGL context on the Wayland display, a Skia GL
//! context, and feeds input into a Freya [`Runner`]. Each surface gets its own
//! runner, tree, font collection and Skia context.
//!
//! The event loop is calloop (the same one the shell already uses); Freya's
//! futures are polled from its ping source, never from a runtime thread.

mod input;

use std::{cell::RefCell, ffi::c_void, pin::pin, rc::Rc, sync::Arc, task::Context};

use accesskit::Role;
use freya_clipboard::copypasta::ClipboardProvider;
use freya_components::{cache::AssetCacher, integration::integration};
use freya_core::{
    integration::*,
    prelude::{Color, State},
};
use freya_engine::prelude::{
    ColorType, DirectContext, FontCollection, FontMgr, Format, FramebufferInfo, Interface,
    Surface as SkiaSurface, SurfaceOrigin, TypefaceFontProvider, backend_render_targets,
    direct_contexts, wrap_backend_render_target,
};
use futures_util::task::ArcWake;
use khronos_egl::{self as egl, DynamicInstance};
use ragnarok::{EventsExecutorRunner, EventsMeasurerRunner, NodesState};
use sctk::reexports::client::event_created_child;
use sctk::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_session_lock, delegate_touch,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{
            self, EventLoop, LoopSignal,
            ping::{Ping, PingSource},
        },
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, Dispatch, Proxy, QueueHandle,
            backend::ObjectId,
            delegate_dispatch,
            globals::registry_queue_init,
            protocol::{
                wl_callback::WlCallback, wl_keyboard, wl_output, wl_pointer, wl_seat,
                wl_surface::WlSurface, wl_touch,
            },
        },
        protocols::ext::foreign_toplevel_list::v1::client::{
            ext_foreign_toplevel_handle_v1::{Event as FtlHandleEvent, ExtForeignToplevelHandleV1},
            ext_foreign_toplevel_list_v1::{Event as FtlListEvent, ExtForeignToplevelListV1},
        },
        protocols_wlr::{
            foreign_toplevel::v1::client::{
                zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
                zwlr_foreign_toplevel_manager_v1::{
                    Event as MgmtListEvent, ZwlrForeignToplevelManagerV1,
                },
            },
            layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers as SctkModifiers, RepeatInfo},
        pointer::{PointerEvent, PointerHandler},
        touch::TouchHandler,
    },
    session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    },
    shell::wlr_layer::{
        Anchor as SctkAnchor, KeyboardInteractivity as SctkKeyboardInteractivity,
        Layer as SctkLayer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
        LayerSurfaceData,
    },
};
use std::task::Waker;
use torin::prelude::{CursorPoint, Size2D};
use wayland_egl::WlEglSurface;

/// The Wayland EGL platform (`EGL_PLATFORM_WAYLAND_KHR`/`EXT`).
const EGL_PLATFORM_WAYLAND: egl::Enum = 0x31D8;

/// A layer of `zwlr_layer_shell_v1`, without exposing SCTK in the public API.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    Background,
    Bottom,
    #[default]
    Top,
    Overlay,
}

/// Which edges a layer surface is anchored to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Anchors {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Anchors {
    pub const ALL: Self = Self {
        top: true,
        bottom: true,
        left: true,
        right: true,
    };
    pub const TOP: Self = Self {
        top: true,
        left: true,
        right: true,
        bottom: false,
    };
    pub const BOTTOM: Self = Self {
        bottom: true,
        left: true,
        right: true,
        top: false,
    };
}

/// How a layer surface wants the keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardInteractivity {
    #[default]
    None,
    Exclusive,
    OnDemand,
}

/// What kind of surface the runner owns.
#[derive(Clone, Debug)]
pub enum SurfaceSpec {
    /// A `zwlr_layer_surface_v1` on the given layer.
    Layer {
        namespace: String,
        layer: Layer,
        anchors: Anchors,
        exclusive_zone: i32,
        keyboard: KeyboardInteractivity,
        /// Logical size, or `None` to let the compositor decide.
        size: Option<(u32, u32)>,
    },
    /// One `ext-session-lock-v1` surface per output.
    Lock,
}

/// A failure while setting up or running the surface.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("WAYLAND_DISPLAY is not set to a reachable socket")]
    NoDisplay,
    #[error("the compositor does not advertise {0}")]
    MissingGlobal(&'static str),
    #[error("EGL initialisation failed: {0}")]
    Egl(&'static str),
    #[error("Skia could not create its GL context")]
    Skia,
    #[error(transparent)]
    Wayland(#[from] sctk::reexports::client::ConnectError),
    #[error(transparent)]
    Registry(#[from] sctk::reexports::client::globals::GlobalError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The EGL + Skia GL context of one surface.
struct Graphics {
    egl: DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
    window: Option<WlEglSurface>,
    surface: Option<egl::Surface>,
    gr: DirectContext,
    skia: Option<SkiaSurface>,
    size: (u32, u32),
}

// Safety: EGL display/context handles are used only on the thread that created
// them (the calloop loop); the raw pointers are not shared across threads.
unsafe impl Send for Graphics {}

impl Graphics {
    fn new(display_ptr: *mut c_void) -> Result<Self, RunError> {
        // Safety: the caller passes the live `wl_display*` of the connection.
        let egl = unsafe { DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|_| RunError::Egl("libEGL could not be loaded"))?;
        let display = unsafe {
            egl.get_platform_display(
                EGL_PLATFORM_WAYLAND,
                display_ptr,
                &[egl::NONE as egl::Attrib],
            )
        }
        .map_err(|_| RunError::Egl("eglGetPlatformDisplay failed"))?;
        egl.initialize(display)
            .map_err(|_| RunError::Egl("eglInitialize failed"))?;
        let config = egl
            .choose_first_config(
                display,
                &[
                    egl::SURFACE_TYPE,
                    egl::WINDOW_BIT,
                    egl::RED_SIZE,
                    8,
                    egl::GREEN_SIZE,
                    8,
                    egl::BLUE_SIZE,
                    8,
                    egl::ALPHA_SIZE,
                    8,
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES2_BIT,
                    egl::NONE,
                ],
            )
            .map_err(|_| RunError::Egl("eglChooseConfig failed"))?
            .ok_or(RunError::Egl("no suitable EGL config"))?;
        let context = egl
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
            )
            .map_err(|_| RunError::Egl("eglCreateContext failed"))?;

        // The Skia GL interface loads every function through EGL.
        let interface = Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            egl.get_proc_address(name)
                .map_or(std::ptr::null(), |f| f as *const c_void)
        })
        .ok_or(RunError::Skia)?;
        let mut gr = direct_contexts::make_gl(interface, None).ok_or(RunError::Skia)?;
        gr.set_resource_cache_limit(128 * 1024 * 1024);

        Ok(Self {
            egl,
            display,
            config,
            context,
            window: None,
            surface: None,
            gr,
            skia: None,
            size: (0, 0),
        })
    }

    /// Builds the `wl_egl_window` and EGL surface on a real configure, then
    /// recreates the Skia surface at the new size.
    fn configure(&mut self, surface: &WlSurface, size: (u32, u32)) -> Result<(), RunError> {
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }
        if self.window.is_none() || self.size != size {
            self.window = Some(
                WlEglSurface::new(surface.id(), size.0 as i32, size.1 as i32)
                    .map_err(|_| RunError::Egl("wl_egl_window_create failed"))?,
            );
            if let Some(old) = self.surface.take() {
                let _ = self.egl.destroy_surface(self.display, old);
            }
            let window_ptr = self
                .window
                .as_ref()
                .map(|w| w.ptr() as *mut c_void)
                .expect("window created above");
            // Safety: the wl_egl_window is owned here and lives as long as the
            // EGL surface.
            let egl_surface = unsafe {
                self.egl
                    .create_window_surface(self.display, self.config, window_ptr, None)
            }
            .map_err(|_| RunError::Egl("eglCreateWindowSurface failed"))?;
            self.surface = Some(egl_surface);
            self.size = size;
        }

        let egl_surface = *self.surface.as_ref().expect("surface created above");
        self.egl
            .make_current(
                self.display,
                Some(egl_surface),
                Some(egl_surface),
                Some(self.context),
            )
            .map_err(|_| RunError::Egl("eglMakeCurrent failed"))?;

        let fb_info = FramebufferInfo {
            fboid: 0,
            format: Format::RGBA8.into(),
            ..Default::default()
        };
        let render_target =
            backend_render_targets::make_gl((size.0 as i32, size.1 as i32), 0, 0, fb_info);
        self.skia = wrap_backend_render_target(
            &mut self.gr,
            &render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        );
        Ok(())
    }

    fn make_current(&self) -> Result<(), RunError> {
        let Some(surface) = self.surface else {
            return Err(RunError::Egl("no window surface"));
        };
        self.egl
            .make_current(
                self.display,
                Some(surface),
                Some(surface),
                Some(self.context),
            )
            .map_err(|_| RunError::Egl("eglMakeCurrent failed"))
    }

    fn swap(&self) -> Result<(), RunError> {
        let Some(surface) = self.surface else {
            return Err(RunError::Egl("no window surface"));
        };
        self.egl
            .swap_buffers(self.display, surface)
            .map_err(|_| RunError::Egl("eglSwapBuffers failed"))
    }
}

impl Drop for Graphics {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        if let Some(surface) = self.surface.take() {
            let _ = self.egl.destroy_surface(self.display, surface);
        }
        let _ = self.egl.destroy_context(self.display, self.context);
        self.gr.abandon();
    }
}

/// One Freya app: its runner, tree and render contexts.
struct AppInstance {
    runner: Runner,
    tree: Tree,
    nodes_state: NodesState<NodeId>,
    events_sender: futures_channel::mpsc::UnboundedSender<EventsChunk>,
    events_receiver: futures_channel::mpsc::UnboundedReceiver<EventsChunk>,
    platform: Platform,
    _animation_clock: AnimationClock,
    ticker_sender: RenderingTickerSender,
    font_collection: FontCollection,
    font_manager: FontMgr,
    scale_factor: f64,
    size: Size2D,
    needs_layout: bool,
    waker: Waker,
}

impl AppInstance {
    fn new(
        app: AppComponent,
        scale_factor: f64,
        size: (u32, u32),
        sender: Rc<dyn Fn(UserEvent)>,
        waker: Waker,
        toplevels: Option<Toplevels>,
        session_unlock: Option<SessionUnlock>,
    ) -> Self {
        let (events_sender, events_receiver) = futures_channel::mpsc::unbounded();
        let mut runner = Runner::new(move || integration(app.clone()).into_element());

        runner.provide_root_context(ScreenReader::new);
        let (ticker_sender, ticker) = RenderingTicker::new();
        runner.provide_root_context(|| ticker);
        let animation_clock = runner.provide_root_context(AnimationClock::new);
        runner.provide_root_context(AssetCacher::create);

        let mut font_collection = FontCollection::new();
        let default_manager = FontMgr::default();
        let custom_manager = FontMgr::custom_empty().unwrap_or_default();
        let provider = TypefaceFontProvider::new();
        let custom: FontMgr = provider.into();
        font_collection.set_default_font_manager(default_manager, None);
        font_collection.set_dynamic_font_manager(custom_manager.clone());
        font_collection.paragraph_cache_mut().turn_on(false);

        let tree = Tree::default();

        let platform = runner.provide_root_context({
            let size = Size2D::new(size.0 as f32, size.1 as f32);
            move || Platform {
                focused_accessibility_id: State::create(ACCESSIBILITY_ROOT_ID),
                focused_accessibility_node: State::create(accesskit::Node::new(Role::Window)),
                root_size: State::create(size),
                scale_factor: State::create(scale_factor),
                navigation_mode: State::create(NavigationMode::NotKeyboard),
                preferred_theme: State::create(PreferredTheme::Light),
                is_app_focused: State::create(true),
                accent_color: State::create(AccentColor::default()),
                sender,
            }
        });

        if let Some(toplevels) = toplevels {
            runner.provide_root_context(move || toplevels);
        }
        if let Some(unlock) = session_unlock {
            runner.provide_root_context(move || unlock);
        }
        runner.provide_root_context(|| State::create(None::<Box<dyn ClipboardProvider>>));
        runner.provide_root_context(|| tree.accessibility_generator.clone());
        runner.provide_root_context(|| font_collection.clone());

        Self {
            runner,
            tree,
            nodes_state: NodesState::default(),
            events_sender,
            events_receiver,
            platform,
            _animation_clock: animation_clock,
            ticker_sender,
            font_collection,
            font_manager: custom,
            scale_factor,
            size: Size2D::new(size.0 as f32, size.1 as f32),
            needs_layout: true,
            waker,
        }
    }

    fn resize(&mut self, size: (u32, u32), scale_factor: f64) {
        self.size = Size2D::new(size.0 as f32, size.1 as f32);
        self.scale_factor = scale_factor;
        self.platform.root_size.set(self.size);
        self.platform.scale_factor.set(scale_factor);
        self.tree.layout.reset();
        self.tree.text_cache.reset();
        self.needs_layout = true;
    }

    /// Drains Freya's events and UI events; returns true when the tree is dirty.
    fn poll(&mut self) -> bool {
        let mut cx = Context::from_waker(&self.waker);
        let mut dirty = false;

        loop {
            let ready = {
                let future = pin!(self.runner.handle_events());
                future.poll(&mut cx).is_ready()
            };

            while let Ok(chunk) = self.events_receiver.try_recv() {
                match chunk {
                    EventsChunk::Processed(events) => {
                        EventsExecutorAdapter {
                            runner: &mut self.runner,
                        }
                        .run(&mut self.nodes_state, events);
                        dirty = true;
                    }
                    EventsChunk::Batch(events) => {
                        for event in events {
                            self.runner.handle_event(
                                event.node_id,
                                event.name,
                                event.data,
                                event.bubbles,
                            );
                        }
                        dirty = true;
                    }
                }
            }

            if !ready {
                break;
            }
            dirty = true;
        }
        dirty
    }

    fn update(&mut self) -> bool {
        let mutations = self.runner.sync_and_update();
        let result = self.runner.run_in(|| self.tree.apply_mutations(mutations));
        if result.needs_render {
            self.needs_layout = true;
        }
        result.needs_render
    }

    fn layout(&mut self) {
        if !self.needs_layout {
            return;
        }
        self.tree.measure_layout(
            self.size,
            &mut self.font_collection,
            &self.font_manager,
            &self.events_sender,
            self.scale_factor,
            &[],
        );
        self.needs_layout = false;
    }

    fn push(&mut self, event: PlatformEvent) {
        let mut adapter = EventsMeasurerAdapter {
            scale_factor: self.scale_factor,
            tree: &mut self.tree,
        };
        let processed = adapter.run(&mut vec![event], &mut self.nodes_state, None);
        let _ = self
            .events_sender
            .unbounded_send(EventsChunk::Processed(processed));
    }
}

/// One running toplevel, as the dock lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToplevelInfo {
    pub title: String,
    pub app_id: String,
}

#[derive(Clone)]
struct ToplevelEntry {
    list: ExtForeignToplevelHandleV1,
    mgmt: Option<ZwlrForeignToplevelHandleV1>,
    info: ToplevelInfo,
}

/// The running toplevels, provided as a root context. The dock reads
/// [`Toplevels::list`] and calls [`Toplevels::activate`] on a click.
type Activate = Rc<RefCell<Option<Rc<dyn Fn(usize)>>>>;

#[derive(Clone)]
pub struct Toplevels {
    pub list: State<Vec<ToplevelInfo>>,
    activate: Activate,
}

impl Toplevels {
    pub fn get() -> Self {
        freya_core::prelude::consume_root_context()
    }

    pub fn activate(&self, index: usize) {
        if let Some(f) = self.activate.borrow().as_ref() {
            f(index);
        }
    }
}

/// Unlocks the session from the lock app. Provided only to lock surfaces.
#[derive(Clone)]
pub struct SessionUnlock {
    unlock: Rc<dyn Fn()>,
}

impl SessionUnlock {
    pub fn get() -> Self {
        freya_core::prelude::consume_root_context()
    }

    pub fn unlock(&self) {
        (self.unlock)();
    }
}

/// A surface plus its Freya app and GL context.
struct SurfaceWindow {
    surface: WlSurface,
    role: SurfaceRole,
    display_ptr: *mut c_void,
    graphics: Option<Graphics>,
    app: AppInstance,
    logical_size: (u32, u32),
    buffer_size: (u32, u32),
    scale: f64,
    output: Option<ObjectId>,
    needs_redraw: bool,
    frame_pending: bool,
}

#[allow(dead_code)]
enum SurfaceRole {
    Layer(LayerSurface),
    Lock(SessionLockSurface),
}

impl SurfaceWindow {
    fn request_redraw(&mut self, qh: &QueueHandle<Shell>) {
        self.needs_redraw = true;
        if !self.frame_pending {
            self.frame_pending = true;
            let _callback: WlCallback = self.surface.frame(qh, self.surface.clone());
            self.surface.commit();
        }
    }

    fn configure(&mut self, size: (u32, u32), scale: f64) {
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        self.logical_size = size;
        self.scale = scale;
        self.buffer_size = (
            (size.0 as f64 * scale).round().max(1.0) as u32,
            (size.1 as f64 * scale).round().max(1.0) as u32,
        );
        self.surface.set_buffer_scale(scale.round().max(1.0) as i32);

        if self.graphics.is_none() {
            match Graphics::new(self.display_ptr) {
                Ok(graphics) => self.graphics = Some(graphics),
                Err(error) => {
                    tracing::error!("CelestialKit: {error}");
                    return;
                }
            }
        }
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        if let Err(error) = graphics.configure(&self.surface, self.buffer_size) {
            tracing::error!("CelestialKit: {error}");
            return;
        }
        self.app.resize(self.buffer_size, scale);
        self.needs_redraw = true;
    }

    fn draw(&mut self) -> bool {
        let Some(graphics) = self.graphics.as_mut() else {
            return false;
        };
        if graphics.make_current().is_err() {
            return false;
        }
        self.app.layout();
        {
            let Some(skia) = graphics.skia.as_mut() else {
                return false;
            };
            let canvas = skia.canvas();
            RenderPipeline {
                font_collection: &mut self.app.font_collection,
                font_manager: &self.app.font_manager,
                canvas,
                tree: &self.app.tree,
                scale_factor: self.scale,
                background: Color::TRANSPARENT,
            }
            .render();
        }
        graphics.gr.flush_submit_and_sync_cpu();
        if graphics.swap().is_err() {
            return false;
        }
        let _ = self.app.ticker_sender.send(());
        self.needs_redraw = false;
        true
    }
}

/// A calloop waker for Freya's futures and user events.
struct PingWake(Ping);

impl ArcWake for PingWake {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.ping();
    }
}

struct Shell {
    conn: Connection,
    registry: RegistryState,
    compositor: CompositorState,
    output: OutputState,
    layer_shell: Option<LayerShell>,
    session_lock: Option<SessionLockState>,
    active_lock: Option<SessionLock>,
    surfaces: Vec<SurfaceWindow>,
    seat: SeatState,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    modifiers: SctkModifiers,
    loop_signal: Option<LoopSignal>,
    app: AppComponent,
    spec: SurfaceSpec,
    needs_exit: bool,
    request_redraw_all: bool,
    queue: Rc<RefCell<Vec<UserEvent>>>,
    ping: Ping,
    ping_source: Option<PingSource>,
    qh: QueueHandle<Shell>,
    _foreign_list: Option<ExtForeignToplevelListV1>,
    _foreign_mgmt: Option<ZwlrForeignToplevelManagerV1>,
    toplevel_entries: Rc<RefCell<Vec<ToplevelEntry>>>,
    toplevels: Toplevels,
    session_unlock: Option<SessionUnlock>,
    session_locked: bool,
}

fn make_sender(queue: Rc<RefCell<Vec<UserEvent>>>, ping: Ping) -> Rc<dyn Fn(UserEvent)> {
    Rc::new(move |event| {
        queue.borrow_mut().push(event);
        ping.ping();
    })
}

fn anchors_to_sctk(a: Anchors) -> SctkAnchor {
    let mut out = SctkAnchor::empty();
    if a.top {
        out |= SctkAnchor::TOP;
    }
    if a.bottom {
        out |= SctkAnchor::BOTTOM;
    }
    if a.left {
        out |= SctkAnchor::LEFT;
    }
    if a.right {
        out |= SctkAnchor::RIGHT;
    }
    out
}

fn drain_user_events(events: &Rc<RefCell<Vec<UserEvent>>>) -> Vec<UserEvent> {
    std::mem::take(&mut *events.borrow_mut())
}

/// Runs a layer-shell surface with a Freya app until the compositor closes it.
pub fn run_layer(app: AppComponent, spec: SurfaceSpec) -> Result<(), RunError> {
    let (conn, event_queue, qh, mut shell) = Shell::connect(app, spec)?;
    shell.qh = qh.clone();
    let layer_shell = shell
        .layer_shell
        .as_ref()
        .ok_or(RunError::MissingGlobal("zwlr_layer_shell_v1"))?;

    let (namespace, layer, anchors, exclusive_zone, keyboard, size) = match &shell.spec {
        SurfaceSpec::Layer {
            namespace,
            layer,
            anchors,
            exclusive_zone,
            keyboard,
            size,
        } => (
            namespace.clone(),
            *layer,
            *anchors,
            *exclusive_zone,
            *keyboard,
            *size,
        ),
        SurfaceSpec::Lock => return Err(RunError::MissingGlobal("layer surface")),
    };

    let surface = shell.compositor.create_surface(&qh);
    let layer_surface = layer_shell.create_layer_surface(
        &qh,
        surface.clone(),
        match layer {
            Layer::Background => SctkLayer::Background,
            Layer::Bottom => SctkLayer::Bottom,
            Layer::Top => SctkLayer::Top,
            Layer::Overlay => SctkLayer::Overlay,
        },
        Some(&namespace),
        None,
    );
    layer_surface.set_anchor(anchors_to_sctk(anchors));
    layer_surface.set_exclusive_zone(exclusive_zone);
    layer_surface.set_keyboard_interactivity(match keyboard {
        KeyboardInteractivity::None => SctkKeyboardInteractivity::None,
        KeyboardInteractivity::Exclusive => SctkKeyboardInteractivity::Exclusive,
        KeyboardInteractivity::OnDemand => SctkKeyboardInteractivity::OnDemand,
    });
    if let Some((w, h)) = size {
        layer_surface.set_size(w, h);
    }
    surface.commit();

    let display_ptr = conn.display().id().as_ptr().cast::<c_void>();
    let waker = futures_util::task::waker(Arc::new(PingWake(shell.ping.clone())));
    let sender = make_sender(shell.queue.clone(), shell.ping.clone());
    let app_instance = AppInstance::new(
        shell.app.clone(),
        1.0,
        size.unwrap_or((1, 1)),
        sender,
        waker,
        Some(shell.toplevels.clone()),
        None,
    );
    shell.surfaces.push(SurfaceWindow {
        surface: surface.clone(),
        role: SurfaceRole::Layer(layer_surface),
        display_ptr,
        graphics: None,
        app: app_instance,
        logical_size: size.unwrap_or((0, 0)),
        buffer_size: (0, 0),
        scale: 1.0,
        output: None,
        needs_redraw: true,
        frame_pending: false,
    });

    let mut event_loop: EventLoop<Shell> =
        EventLoop::try_new().map_err(|_| RunError::Egl("calloop EventLoop::try_new failed"))?;
    insert_ping(&mut event_loop, &mut shell)?;
    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|_| RunError::Egl("inserting the Wayland source"))?;
    let signal = event_loop.get_signal();
    shell.loop_signal = Some(signal.clone());
    if let Some(lock) = shell.active_lock.clone() {
        shell.session_unlock = Some(SessionUnlock {
            unlock: Rc::new(move || {
                lock.unlock();
                signal.stop();
            }),
        });
    }
    event_loop
        .run(None, &mut shell, |shell| {
            if shell.needs_exit
                && let Some(signal) = &shell.loop_signal
            {
                signal.stop();
            }
        })
        .map_err(|_| RunError::Egl("calloop event loop failed"))?;
    Ok(())
}

/// Runs the session lock: one Freya surface per output.
pub fn run_lock(app: AppComponent) -> Result<(), RunError> {
    let (conn, event_queue, qh, mut shell) = Shell::connect(app, SurfaceSpec::Lock)?;
    shell.qh = qh.clone();
    let session_lock_state = shell
        .session_lock
        .as_ref()
        .ok_or(RunError::MissingGlobal("ext_session_lock_manager_v1"))?;
    let lock = session_lock_state
        .lock(&qh)
        .map_err(|_| RunError::MissingGlobal("ext_session_lock_manager_v1 (lock refused)"))?;
    shell.active_lock = Some(lock);

    let mut event_loop: EventLoop<Shell> =
        EventLoop::try_new().map_err(|_| RunError::Egl("calloop EventLoop::try_new failed"))?;
    insert_ping(&mut event_loop, &mut shell)?;
    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|_| RunError::Egl("inserting the Wayland source"))?;
    let signal = event_loop.get_signal();
    shell.loop_signal = Some(signal.clone());
    if let Some(lock) = shell.active_lock.clone() {
        shell.session_unlock = Some(SessionUnlock {
            unlock: Rc::new(move || {
                lock.unlock();
                signal.stop();
            }),
        });
    }
    event_loop
        .run(None, &mut shell, |shell| {
            if shell.needs_exit
                && let Some(signal) = &shell.loop_signal
            {
                signal.stop();
            }
        })
        .map_err(|_| RunError::Egl("calloop event loop failed"))?;
    Ok(())
}

fn insert_ping(event_loop: &mut EventLoop<Shell>, shell: &mut Shell) -> Result<(), RunError> {
    let source = shell
        .ping_source
        .take()
        .ok_or(RunError::Egl("the Freya ping source is already installed"))?;
    event_loop
        .handle()
        .insert_source(source, |_, _, shell: &mut Shell| {
            for event in drain_user_events(&shell.queue) {
                if matches!(event, UserEvent::RequestRedraw) {
                    shell.request_redraw_all = true;
                }
            }
            shell.poll_all();
            shell.redraw_all();
        })
        .map_err(|_| RunError::Egl("inserting the Freya ping source"))?;
    Ok(())
}

impl Shell {
    fn connect(
        app: AppComponent,
        spec: SurfaceSpec,
    ) -> Result<
        (
            Connection,
            sctk::reexports::client::EventQueue<Shell>,
            QueueHandle<Shell>,
            Shell,
        ),
        RunError,
    > {
        let conn = Connection::connect_to_env().map_err(|_| RunError::NoDisplay)?;
        let (globals, event_queue) = registry_queue_init::<Shell>(&conn)?;
        let qh = event_queue.handle();
        let queue = Rc::new(RefCell::new(Vec::new()));
        let (ping, ping_source) =
            calloop::ping::make_ping().map_err(|_| RunError::Egl("calloop ping failed"))?;
        let toplevel_entries = Rc::new(RefCell::new(Vec::new()));
        let toplevels = Toplevels {
            list: State::create_global(Vec::new()),
            activate: Rc::new(RefCell::new(None)),
        };
        let foreign_list = globals
            .bind::<ExtForeignToplevelListV1, _, _>(&qh, 1..=1, ())
            .ok();
        let foreign_mgmt = globals
            .bind::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 3..=3, ())
            .ok();
        let shell = Shell {
            conn: conn.clone(),
            registry: RegistryState::new(&globals),
            compositor: CompositorState::bind(&globals, &qh)
                .map_err(|_| RunError::MissingGlobal("wl_compositor"))?,
            output: OutputState::new(&globals, &qh),
            layer_shell: LayerShell::bind(&globals, &qh).ok(),
            session_lock: Some(SessionLockState::new(&globals, &qh)),
            active_lock: None,
            surfaces: Vec::new(),
            seat: SeatState::new(&globals, &qh),
            pointer: None,
            keyboard: None,
            touch: None,
            modifiers: SctkModifiers::default(),
            loop_signal: None,
            app,
            spec,
            needs_exit: false,
            request_redraw_all: false,
            queue,
            ping,
            ping_source: Some(ping_source),
            qh: qh.clone(),
            _foreign_list: foreign_list,
            _foreign_mgmt: foreign_mgmt,
            toplevel_entries,
            toplevels,
            session_unlock: None,
            session_locked: false,
        };
        Ok((conn, event_queue, qh, shell))
    }

    fn surface_for(&self, surface: &WlSurface) -> Option<usize> {
        self.surfaces.iter().position(|s| s.surface == *surface)
    }

    fn create_lock_surface(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &wl_output::WlOutput,
    ) -> Option<usize> {
        if self.surfaces.iter().any(|s| s.output == Some(output.id())) {
            return None;
        }
        let lock = self.active_lock.as_ref()?;
        let surface = self.compositor.create_surface(qh);
        let lock_surface = lock.create_lock_surface(surface.clone(), output, qh);
        let display_ptr = self.conn.display().id().as_ptr().cast::<c_void>();
        let waker = futures_util::task::waker(Arc::new(PingWake(self.ping.clone())));
        let sender = make_sender(self.queue.clone(), self.ping.clone());
        let app = AppInstance::new(
            self.app.clone(),
            1.0,
            (1, 1),
            sender,
            waker,
            Some(self.toplevels.clone()),
            self.session_unlock.clone(),
        );
        self.surfaces.push(SurfaceWindow {
            surface,
            role: SurfaceRole::Lock(lock_surface),
            display_ptr,
            graphics: None,
            app,
            logical_size: (0, 0),
            buffer_size: (0, 0),
            scale: 1.0,
            output: Some(output.id()),
            needs_redraw: true,
            frame_pending: false,
        });
        Some(self.surfaces.len() - 1)
    }

    fn poll_all(&mut self) {
        for window in &mut self.surfaces {
            let dirty = window.app.poll();
            if dirty {
                window.app.update();
                window.needs_redraw = true;
            }
            if self.request_redraw_all {
                window.needs_redraw = true;
            }
        }
        self.request_redraw_all = false;
    }

    fn sync_toplevels(&mut self) {
        let infos: Vec<ToplevelInfo> = self
            .toplevel_entries
            .borrow()
            .iter()
            .map(|entry| entry.info.clone())
            .collect();
        let mut list = self.toplevels.list;
        if *list.peek() != infos {
            list.set(infos);
            self.request_redraw_all = true;
        }
    }

    fn redraw_all(&mut self) {
        let qh = self.qh.clone();
        for window in &mut self.surfaces {
            if window.needs_redraw {
                window.request_redraw(&qh);
            }
        }
    }
}

impl ProvidesRegistryState for Shell {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    fn runtime_add_global(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: u32,
        _: &str,
        _: u32,
    ) {
    }

    fn runtime_remove_global(&mut self, _: &Connection, _: &QueueHandle<Self>, _: u32, _: &str) {}
}

impl CompositorHandler for Shell {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &WlSurface,
        scale_factor: i32,
    ) {
        let Some(index) = self.surface_for(surface) else {
            return;
        };
        let scale = scale_factor.max(1) as f64;
        let window = &mut self.surfaces[index];
        if window.scale != scale && window.logical_size.0 > 0 {
            let size = window.logical_size;
            window.configure(size, scale);
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, surface: &WlSurface, _: u32) {
        let Some(index) = self.surface_for(surface) else {
            return;
        };
        self.surfaces[index].frame_pending = false;
        if self.surfaces[index].needs_redraw {
            let _ = self.surfaces[index].draw();
        }
        if self.surfaces[index].needs_redraw {
            self.surfaces[index].request_redraw(qh);
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Shell {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.needs_exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer_surface: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(index) = self.surfaces.iter().position(
            |s| matches!(&s.role, SurfaceRole::Layer(existing) if existing == layer_surface),
        ) else {
            return;
        };
        let scale = self.surfaces[index].scale;
        self.surfaces[index].configure(configure.new_size, scale);
        self.surfaces[index].request_redraw(qh);
    }
}

impl SessionLockHandler for Shell {
    fn locked(&mut self, _: &Connection, qh: &QueueHandle<Self>, session_lock: SessionLock) {
        self.active_lock = Some(session_lock);
        self.session_locked = true;
        let outputs: Vec<wl_output::WlOutput> = self.output.outputs().collect();
        for output in outputs {
            self.create_lock_surface(qh, &output);
        }
    }

    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        self.session_locked = false;
        eprintln!("yuki-shell lock: the compositor refused the session lock");
        std::process::exit(1);
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _serial: u32,
    ) {
        let wl_surface = surface.wl_surface().clone();
        let Some(index) = self.surface_for(&wl_surface) else {
            return;
        };
        let scale = self.surfaces[index].scale;
        self.surfaces[index].configure(configure.new_size, scale);
        self.surfaces[index].request_redraw(qh);
    }
}

impl OutputHandler for Shell {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }

    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.session_locked
            && matches!(self.spec, SurfaceSpec::Lock)
            && !self.surfaces.iter().any(|s| s.output == Some(output.id()))
        {
            self.create_lock_surface(qh, &output);
        }
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl SeatHandler for Shell {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        let entries = self.toplevel_entries.clone();
        let seat = seat.clone();
        *self.toplevels.activate.borrow_mut() = Some(Rc::new(move |index| {
            if let Some(entry) = entries.borrow().get(index)
                && let Some(mgmt) = &entry.mgmt
            {
                mgmt.activate(&seat);
            }
        }));
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => {
                if let Ok(pointer) = self.seat.get_pointer::<Self>(qh, &seat) {
                    self.pointer = Some(pointer);
                }
            }
            Capability::Keyboard => {
                if let Ok(keyboard) = self.seat.get_keyboard::<Self, Self>(qh, &seat, None) {
                    self.keyboard = Some(keyboard);
                }
            }
            Capability::Touch => {
                if let Ok(touch) = self.seat.get_touch::<Self>(qh, &seat) {
                    self.touch = Some(touch);
                }
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => self.pointer = None,
            Capability::Keyboard => self.keyboard = None,
            Capability::Touch => self.touch = None,
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Shell {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some(index) = self.surface_for(&event.surface) else {
                continue;
            };
            let position = CursorPoint::from(event.position);
            use sctk::seat::pointer::PointerEventKind;
            let platform_event = match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    PlatformEvent::Mouse {
                        name: MouseEventName::MouseMove,
                        cursor: position,
                        button: None,
                    }
                }
                PointerEventKind::Leave { .. } => PlatformEvent::Mouse {
                    name: MouseEventName::MouseMove,
                    cursor: CursorPoint::from((-1.0, -1.0)),
                    button: None,
                },
                PointerEventKind::Press { button, .. } => PlatformEvent::Mouse {
                    name: MouseEventName::MouseDown,
                    cursor: position,
                    button: Some(input::mouse_button(button)),
                },
                PointerEventKind::Release { button, .. } => PlatformEvent::Mouse {
                    name: MouseEventName::MouseUp,
                    cursor: position,
                    button: Some(input::mouse_button(button)),
                },
                PointerEventKind::Axis {
                    vertical,
                    horizontal,
                    ..
                } => PlatformEvent::Wheel {
                    name: WheelEventName::Wheel,
                    scroll: CursorPoint::from((horizontal.absolute, vertical.absolute)),
                    cursor: position,
                    source: WheelSource::Device,
                },
            };
            self.surfaces[index].app.push(platform_event);
            self.surfaces[index].request_redraw(qh);
        }
    }
}

impl KeyboardHandler for Shell {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key_event(qh, event, KeyboardEventName::KeyDown);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key_event(qh, event, KeyboardEventName::KeyUp);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: SctkModifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }

    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: RepeatInfo,
    ) {
    }
}

impl Shell {
    fn key_event(&mut self, qh: &QueueHandle<Self>, event: KeyEvent, name: KeyboardEventName) {
        let (key, code) = input::key(event.keysym, event.utf8);
        let modifiers = input::modifiers(self.modifiers);
        let platform_event = PlatformEvent::Keyboard {
            name,
            key,
            code,
            modifiers,
        };
        // Lock surfaces all show the same app: deliver to the first one. Layer
        // surfaces usually have a single surface as well.
        if let Some(window) = self.surfaces.first_mut() {
            window.app.push(platform_event);
            window.request_redraw(qh);
        }
    }
}

impl TouchHandler for Shell {
    fn down(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: u32,
        _: u32,
        surface: WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        self.touch_event(
            qh,
            surface,
            id,
            position,
            freya_core::prelude::TouchPhase::Started,
        );
    }

    fn up(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: u32,
        _: u32,
        id: i32,
    ) {
        let Some(surface) = self.surfaces.first().map(|s| s.surface.clone()) else {
            return;
        };
        self.touch_event(
            qh,
            surface,
            id,
            (0.0, 0.0),
            freya_core::prelude::TouchPhase::Ended,
        );
    }

    fn motion(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let Some(surface) = self.surfaces.first().map(|s| s.surface.clone()) else {
            return;
        };
        self.touch_event(
            qh,
            surface,
            id,
            position,
            freya_core::prelude::TouchPhase::Moved,
        );
    }

    fn shape(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: i32,
        _: f64,
        _: f64,
    ) {
    }

    fn orientation(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: i32,
        _: f64,
    ) {
    }

    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch) {}
}

impl Shell {
    fn touch_event(
        &mut self,
        qh: &QueueHandle<Self>,
        surface: WlSurface,
        id: i32,
        position: (f64, f64),
        phase: freya_core::prelude::TouchPhase,
    ) {
        let Some(index) = self.surface_for(&surface) else {
            return;
        };
        let platform_event = PlatformEvent::Touch {
            name: match phase {
                freya_core::prelude::TouchPhase::Started => TouchEventName::TouchStart,
                freya_core::prelude::TouchPhase::Moved => TouchEventName::TouchMove,
                freya_core::prelude::TouchPhase::Ended => TouchEventName::TouchEnd,
                freya_core::prelude::TouchPhase::Cancelled => TouchEventName::TouchCancel,
            },
            location: CursorPoint::from(position),
            finger_id: id as u64,
            phase,
            force: None,
        };
        self.surfaces[index].app.push(platform_event);
        self.surfaces[index].request_redraw(qh);
    }
}

impl Dispatch<ExtForeignToplevelListV1, (), Shell> for Shell {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: <ExtForeignToplevelListV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let FtlListEvent::Toplevel { toplevel } = event {
            state.toplevel_entries.borrow_mut().push(ToplevelEntry {
                list: toplevel,
                mgmt: None,
                info: ToplevelInfo {
                    title: String::new(),
                    app_id: String::new(),
                },
            });
            state.sync_toplevels();
        }
    }

    event_created_child!(Shell, ExtForeignToplevelListV1, [
        sctk::reexports::protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, (), Shell> for Shell {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: <ExtForeignToplevelHandleV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, FtlHandleEvent::Closed) {
            state
                .toplevel_entries
                .borrow_mut()
                .retain(|entry| entry.list.id() != handle.id());
            state.sync_toplevels();
            return;
        }
        let mut entries = state.toplevel_entries.borrow_mut();
        if let Some(entry) = entries.iter_mut().find(|e| e.list.id() == handle.id()) {
            match event {
                FtlHandleEvent::Title { title } => entry.info.title = title,
                FtlHandleEvent::AppId { app_id } => entry.info.app_id = app_id,
                _ => {}
            }
        }
        drop(entries);
        state.sync_toplevels();
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, (), Shell> for Shell {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: <ZwlrForeignToplevelManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let MgmtListEvent::Toplevel { toplevel } = event
            && let Some(entry) = state.toplevel_entries.borrow_mut().last_mut()
        {
            entry.mgmt = Some(toplevel);
        }
    }

    event_created_child!(Shell, ZwlrForeignToplevelManagerV1, [
        sctk::reexports::protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, (), Shell> for Shell {
    fn event(
        _: &mut Self,
        _: &ZwlrForeignToplevelHandleV1,
        _: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_compositor!(Shell);
delegate_output!(Shell);
delegate_registry!(Shell);
delegate_seat!(Shell);
delegate_pointer!(Shell);
delegate_keyboard!(Shell);
delegate_touch!(Shell);
delegate_session_lock!(Shell);
delegate_dispatch!(Shell: [zwlr_layer_shell_v1::ZwlrLayerShellV1: sctk::globals::GlobalData] => LayerShell);
delegate_dispatch!(Shell: [zwlr_layer_surface_v1::ZwlrLayerSurfaceV1: LayerSurfaceData] => LayerShell);
