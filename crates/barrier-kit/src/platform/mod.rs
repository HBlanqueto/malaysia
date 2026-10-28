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

mod infinity;
mod input;

pub use input::inserts_text;

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    ffi::c_void,
    os::fd::AsFd,
    pin::pin,
    rc::Rc,
    sync::Arc,
    task::Context,
    time::Duration,
};

use accesskit::Role;
use freya_clipboard::copypasta::ClipboardProvider;
use freya_components::{cache::AssetCacher, integration::integration};
use freya_core::{
    accessibility::tree::{ACCESSIBILITY_ROOT_ID, AccessibilityTree},
    integration::*,
    prelude::{AccessibilityFocusStrategy, Color, CursorIcon, State},
};
use freya_engine::prelude::{
    ColorType, DirectContext, FontCollection, FontMgr, Format, FramebufferInfo, Interface,
    Surface as SkiaSurface, SurfaceOrigin, backend_render_targets, direct_contexts,
    wrap_backend_render_target,
};
use futures_util::task::ArcWake;
use khronos_egl::{self as egl, DynamicInstance};
use ragnarok::{EventsExecutorRunner, EventsMeasurerRunner, NodesState};
use sctk::reexports::client::event_created_child;
use sctk::reexports::protocols::xdg::shell::client::xdg_positioner::{
    Anchor as SctkAnchorPos, ConstraintAdjustment as SctkConstraint, Gravity as SctkGravity,
};
use sctk::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_session_lock, delegate_touch, delegate_xdg_popup, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{
            self, EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction,
            generic::Generic,
            ping::{Ping, PingSource},
            timer::{TimeoutAction, Timer},
        },
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, Dispatch, Proxy, QueueHandle, delegate_dispatch,
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
        protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
            Shape as CursorShape, WpCursorShapeDeviceV1,
        },
        protocols::xdg::activation::v1::client::{
            xdg_activation_token_v1::{self, XdgActivationTokenV1},
            xdg_activation_v1::XdgActivationV1,
        },
        protocols::xdg::shell::client::xdg_toplevel,
        protocols_wlr::{
            foreign_toplevel::v1::client::{
                zwlr_foreign_toplevel_handle_v1::{
                    Event as ZwlrForeignToplevelHandleV1Event,
                    State as ZwlrForeignToplevelHandleV1State, ZwlrForeignToplevelHandleV1,
                },
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
        pointer::{PointerEvent, PointerHandler, cursor_shape::CursorShapeManager},
        touch::TouchHandler,
    },
    session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor as SctkAnchor, KeyboardInteractivity as SctkKeyboardInteractivity,
            Layer as SctkLayer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
            LayerSurfaceData,
        },
        xdg::{
            XdgPositioner, XdgShell, XdgSurface,
            popup::{Popup, PopupConfigure, PopupHandler},
            window::{DecorationMode, Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
};
use std::task::Waker;
use torin::prelude::{CursorPoint, Size2D};
use vow_appearance::{Resolved, load_appearance, load_dominant, resolve, watch::AppearanceWatch};
use wayland_egl::WlEglSurface;
use wayland_protocols_plasma::appmenu::client::{
    org_kde_kwin_appmenu::{self, OrgKdeKwinAppmenu},
    org_kde_kwin_appmenu_manager::{self, OrgKdeKwinAppmenuManager},
};

pub use infinity::{
    InfinityWindowManagerV1, Layout as InfinityLayout, PairLayout as InfinityPairLayout,
};
pub use sctk::reexports::client::backend::ObjectId;

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

/// Identifier of an open popup within this process.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PopupId(u64);

impl PopupId {
    fn next(counter: &Rc<Cell<u64>>) -> Self {
        let id = counter.get();
        counter.set(id + 1);
        Self(id)
    }

    pub fn get(&self) -> u64 {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn test(id: u64) -> Self {
        Self(id)
    }
}

/// Which edge of the anchor rectangle a popup attaches to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopupAnchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// In which direction a popup grows from its anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopupGravity {
    /// Up and left.
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// How the compositor may adjust a popup that does not fit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopupConstraintAdjustment {
    pub flip_x: bool,
    pub flip_y: bool,
    pub slide_x: bool,
    pub slide_y: bool,
}

/// Options for opening a popup from the shell (menubar, submenus).
#[derive(Clone)]
pub struct PopupSpec {
    /// Rectangle on the parent surface, in logical pixels, that the popup anchors to.
    pub anchor_rect: (i32, i32, u32, u32),
    pub anchor: PopupAnchor,
    pub gravity: PopupGravity,
    /// Offset in logical pixels applied after anchoring and gravity.
    pub offset: (i32, i32),
    /// Preferred logical size of the popup.
    pub size: (u32, u32),
    pub constraint_adjustment: PopupConstraintAdjustment,
    /// Request an implicit grab with the last input serial.
    pub grab: bool,
    /// Called once when the popup is dismissed for any reason.
    pub on_dismissed: Option<Rc<dyn Fn()>>,
}

impl Default for PopupSpec {
    fn default() -> Self {
        Self {
            anchor_rect: (0, 0, 0, 0),
            anchor: PopupAnchor::default(),
            gravity: PopupGravity::default(),
            offset: (0, 0),
            size: (100, 100),
            constraint_adjustment: PopupConstraintAdjustment::default(),
            grab: false,
            on_dismissed: None,
        }
    }
}

/// Opens and closes popups from inside a BarrierKit surface.
#[derive(Clone)]
pub struct Popups {
    queue: Rc<RefCell<Vec<PopupCommand>>>,
    ping: Ping,
    next_id: Rc<Cell<u64>>,
    last_serial: Rc<Cell<u32>>,
    owner: ObjectId,
    current: Option<PopupId>,
    surface_count: Rc<Cell<usize>>,
    opens_left: Rc<Cell<bool>>,
}

impl Popups {
    pub fn get() -> Self {
        freya_core::prelude::consume_root_context()
    }

    /// Opens a popup attached to the current surface (or to this popup, if the
    /// context belongs to a popup).
    pub fn open(&self, spec: PopupSpec, content: AppComponent) -> PopupId {
        let id = PopupId::next(&self.next_id);
        self.queue.borrow_mut().push(PopupCommand::Open {
            owner: self.owner.clone(),
            parent: self.current,
            id,
            spec,
            content,
            serial: self.last_serial.get(),
        });
        self.ping.ping();
        id
    }

    /// Closes a popup and its descendants.
    pub fn close(&self, id: PopupId) {
        self.queue.borrow_mut().push(PopupCommand::Close { id });
        self.ping.ping();
    }

    /// Repositions an already open popup with a new geometry.
    fn reposition(&self, id: PopupId, spec: PopupSpec) {
        self.queue
            .borrow_mut()
            .push(PopupCommand::Reposition { id, spec });
        self.ping.ping();
    }

    /// The number of popup surfaces currently tracked by the runner. Useful for
    /// leak checks in tests and self-tests.
    pub fn surface_count(&self) -> usize {
        self.surface_count.get()
    }
}

#[cfg(test)]
impl Popups {
    pub(crate) fn for_test(
        owner: ObjectId,
        current: Option<PopupId>,
    ) -> (Self, Rc<RefCell<Vec<PopupCommand>>>) {
        let queue = Rc::new(RefCell::new(Vec::new()));
        let (ping, _) = calloop::ping::make_ping().expect("test ping");
        let popups = Popups {
            queue: queue.clone(),
            ping,
            next_id: Rc::new(Cell::new(0)),
            last_serial: Rc::new(Cell::new(0)),
            owner,
            current,
            surface_count: Rc::new(Cell::new(0)),
            opens_left: Rc::new(Cell::new(false)),
        };
        (popups, queue)
    }
}

#[cfg(test)]
impl PopupHandle {
    pub(crate) fn for_test(popups: Popups, id: PopupId) -> Self {
        Self { id, popups }
    }
}

/// Handle available inside the content of a popup surface.
#[derive(Clone)]
pub struct PopupHandle {
    id: PopupId,
    popups: Popups,
}

impl PopupHandle {
    pub fn get() -> Self {
        freya_core::prelude::consume_root_context()
    }

    pub fn id(&self) -> PopupId {
        self.id
    }

    /// True when the compositor configured this popup to the left of its
    /// parent (its configure `x` is negative). Descendant submenus use it to
    /// keep opening on the same side.
    pub fn opens_left(&self) -> bool {
        self.popups.opens_left.get()
    }

    pub fn close(&self) {
        self.popups.close(self.id);
    }

    /// Repositions this popup with a new size and anchor rectangle.
    ///
    /// The caller is responsible for requesting a redraw so the surface is
    /// resized before the compositor commits it.
    pub fn reposition(&self, spec: PopupSpec) {
        self.popups.reposition(self.id, spec);
    }

    /// Opens a submenu attached to this popup.
    pub fn open_child(&self, spec: PopupSpec, content: AppComponent) -> PopupId {
        self.popups.open(spec, content)
    }
}

/// Who draws a MalaysiaDreams app's titlebar (ADR 0035).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decorations {
    /// BarrierKit draws its 44 px titlebar.
    #[default]
    Client,
    /// MalaysiaDreams's compositor draws the 32 px titlebar.
    Server,
}

/// Which edge of a client-side window is being resized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// How the compositor answered the decoration request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorationResponse {
    Client,
    Server,
}

/// Handler called when a global menu or in-app shortcut fires an action.
pub type MenuActionHandler = Arc<dyn Fn(&str) + Send + Sync>;

/// Public options for a normal BarrierKit window, without SCTK types.
#[derive(Clone)]
pub struct WindowOptions {
    pub title: String,
    pub app_name: String,
    /// Optional override for the Wayland `app_id`; defaults to
    /// `NATIVE_APP_ID_PREFIX + app_name`.
    pub app_id: Option<String>,
    pub decorations: Decorations,
    pub size: (u32, u32),
    pub min_size: Option<(u32, u32)>,
    pub max_size: Option<(u32, u32)>,
    pub parent: Option<ObjectId>,
    pub menu: Option<crate::app_menu::AppMenu>,
    pub on_menu_action: Option<MenuActionHandler>,
}

impl std::fmt::Debug for WindowOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowOptions")
            .field("title", &self.title)
            .field("app_name", &self.app_name)
            .field("app_id", &self.app_id)
            .field("decorations", &self.decorations)
            .field("size", &self.size)
            .field("min_size", &self.min_size)
            .field("max_size", &self.max_size)
            .field("parent", &self.parent)
            .field("menu", &self.menu)
            .field("on_menu_action", &self.on_menu_action.is_some())
            .finish()
    }
}

impl WindowOptions {
    /// A 640×420 window with client-side chrome and the given title/app name.
    pub fn new(title: impl Into<String>, app_name: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            app_name: app_name.into(),
            app_id: None,
            decorations: Decorations::Client,
            size: (640, 420),
            min_size: None,
            max_size: None,
            parent: None,
            menu: None,
            on_menu_action: None,
        }
    }

    pub fn app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = Some(app_id.into());
        self
    }

    pub fn decorations(mut self, decorations: Decorations) -> Self {
        self.decorations = decorations;
        self
    }

    pub fn size(mut self, size: (u32, u32)) -> Self {
        self.size = size;
        self
    }

    pub fn min_size(mut self, size: Option<(u32, u32)>) -> Self {
        self.min_size = size;
        self
    }

    pub fn max_size(mut self, size: Option<(u32, u32)>) -> Self {
        self.max_size = size;
        self
    }

    pub fn parent(mut self, parent: Option<ObjectId>) -> Self {
        self.parent = parent;
        self
    }

    pub fn menu(mut self, menu: crate::app_menu::AppMenu) -> Self {
        self.menu = Some(menu);
        self
    }

    pub fn on_menu_action(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_menu_action = Some(Arc::new(callback));
        self
    }
}

/// The state a BarrierKit window's titlebar reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowStatus {
    pub activated: bool,
    pub maximized: bool,
    pub decoration: DecorationResponse,
    pub fixed: bool,
}

/// Actions a titlebar can request from the runner.
#[derive(Clone)]
pub struct WindowControls {
    queue: Rc<RefCell<Vec<WindowCommand>>>,
    ping: Ping,
    pub(crate) pointer_serial: Rc<std::cell::Cell<u32>>,
    pub status: State<WindowStatus>,
    pub title: String,
    pub fixed: bool,
    surface_id: ObjectId,
    menu_handle: Option<crate::app_menu::MenuHandle>,
}

impl WindowControls {
    pub fn get() -> Self {
        freya_core::prelude::consume_root_context()
    }

    pub fn pointer_serial(&self) -> u32 {
        self.pointer_serial.get()
    }

    pub fn minimize(&self) {
        self.queue.borrow_mut().push(WindowCommand::Minimize);
        self.ping.ping();
    }

    pub fn toggle_maximize(&self) {
        self.queue.borrow_mut().push(WindowCommand::ToggleMaximize);
        self.ping.ping();
    }

    /// Toggle vertical maximise for fixed-width windows.
    ///
    /// The kit still sends the standard maximise/restore request; the compositor
    /// recognizes fixed-width windows and performs the vertical fill itself.
    /// The stored restore rectangle is the window's current size.
    pub fn toggle_vertical_maximize(&self) {
        self.queue
            .borrow_mut()
            .push(WindowCommand::ToggleVerticalMaximize);
        self.ping.ping();
    }

    pub fn move_window(&self) {
        self.queue
            .borrow_mut()
            .push(WindowCommand::Move(self.pointer_serial.get()));
        self.ping.ping();
    }

    pub fn resize(&self, edge: WindowEdge) {
        self.queue
            .borrow_mut()
            .push(WindowCommand::Resize(self.pointer_serial.get(), edge));
        self.ping.ping();
    }

    /// The Wayland object id of the toplevel surface these controls belong to.
    pub fn surface_id(&self) -> ObjectId {
        self.surface_id.clone()
    }

    /// The global menu handle for this window, if it was configured with one.
    pub fn menu_handle(&self) -> Option<crate::app_menu::MenuHandle> {
        self.menu_handle.clone()
    }

    /// Opens another toplevel in the same process, attached to this window as
    /// its parent when `options.parent` is `Some(surface_id())`.
    pub fn open_window(&self, app: AppComponent, options: WindowOptions) {
        self.queue
            .borrow_mut()
            .push(WindowCommand::Open(app, options));
        self.ping.ping();
    }
}

enum WindowCommand {
    Minimize,
    ToggleMaximize,
    ToggleVerticalMaximize,
    Move(u32),
    Resize(u32, WindowEdge),
    Open(AppComponent, WindowOptions),
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
    /// A normal xdg_toplevel owned by a BarrierKit app.
    Window(WindowOptions),
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

/// The EGL + Skia GL state shared by every surface in the process.
struct GraphicsShared {
    egl: DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
    gr: RefCell<DirectContext>,
    font_collection: FontCollection,
    font_manager: FontMgr,
    /// Counter of `wl_egl_window` creations, for resize tests.
    egl_window_creations: Cell<usize>,
}

// Safety: EGL display/context handles are used only on the thread that created
// them (the calloop loop); the raw pointers are not shared across threads.
unsafe impl Send for GraphicsShared {}

impl GraphicsShared {
    fn new(display_ptr: *mut c_void) -> Result<Self, RunError> {
        // SAFETY: called once on the calloop thread; libEGL is present because
        // the compositor already initialized EGL. `load_required` validates the
        // minimum version (EGL 1.5) before returning.
        let egl = unsafe { DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|_| RunError::Egl("libEGL could not be loaded"))?;
        // Safety: the caller passes the live `wl_display*` of the connection.
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
        // Skia's GL direct context needs a current EGL context to query the
        // driver; the window surface is made current later in `configure`.
        egl.make_current(display, None, None, Some(context))
            .map_err(|_| RunError::Egl("eglMakeCurrent(surfaceless) failed"))?;

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
            gr: RefCell::new(gr),
            font_collection: crate::fonts::collection(),
            font_manager: crate::fonts::manager(),
            egl_window_creations: Cell::new(0),
        })
    }
}

impl Drop for GraphicsShared {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        self.gr.borrow_mut().abandon();
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}

/// The per-surface EGL window/surface and Skia render target. The expensive
/// process-wide state (EGL context, `DirectContext`, fonts) lives in
/// [`GraphicsShared`].
struct Graphics {
    shared: Rc<GraphicsShared>,
    window: Option<WlEglSurface>,
    surface: Option<egl::Surface>,
    skia: Option<SkiaSurface>,
    size: (u32, u32),
    scale: f64,
}

// Safety: the EGLSurface is only used on the calloop thread.
unsafe impl Send for Graphics {}

impl Graphics {
    fn new(shared: Rc<GraphicsShared>) -> Self {
        Self {
            shared,
            window: None,
            surface: None,
            skia: None,
            size: (0, 0),
            scale: 1.0,
        }
    }

    /// Builds the `wl_egl_window` and EGL surface on a real configure, then
    /// recreates the Skia surface at the new size.
    ///
    /// Reuses the existing `wl_egl_window` via `resize` when only the size
    /// changed; only recreates it when the scale changed or the resize failed.
    fn configure(
        &mut self,
        surface: &WlSurface,
        size: (u32, u32),
        scale: f64,
    ) -> Result<(), RunError> {
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }
        let shared = self.shared.clone();
        let size_changed = self.size != size;
        let scale_changed = self.scale != scale;
        let needs_window = self.window.is_none() || size_changed || scale_changed;
        if needs_window {
            if let Some(window) = self.window.as_ref() {
                // Try to resize the existing wl_egl_window without destroying
                // the EGL surface. dx/dy are zero because the Wayland surface
                // origin does not move.
                window.resize(size.0 as i32, size.1 as i32, 0, 0);
                let (w, h) = window.get_size();
                if w == size.0 as i32 && h == size.1 as i32 && !scale_changed {
                    self.size = size;
                } else {
                    self.recreate_window(surface, size)?;
                }
            } else {
                self.recreate_window(surface, size)?;
            }
        }

        let egl_surface = *self.surface.as_ref().expect("surface created above");
        shared
            .egl
            .make_current(
                shared.display,
                Some(egl_surface),
                Some(egl_surface),
                Some(shared.context),
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
            &mut shared.gr.borrow_mut(),
            &render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        );
        self.scale = scale;
        Ok(())
    }

    /// Recreates the `wl_egl_window` and its EGL surface.
    fn recreate_window(&mut self, surface: &WlSurface, size: (u32, u32)) -> Result<(), RunError> {
        let shared = self.shared.clone();
        self.window = Some(
            WlEglSurface::new(surface.id(), size.0 as i32, size.1 as i32)
                .map_err(|_| RunError::Egl("wl_egl_window_create failed"))?,
        );
        shared
            .egl_window_creations
            .set(shared.egl_window_creations.get() + 1);
        if let Some(old) = self.surface.take() {
            let _ = shared.egl.destroy_surface(shared.display, old);
        }
        let window_ptr = self
            .window
            .as_ref()
            .map(|w| w.ptr() as *mut c_void)
            .expect("window created above");
        // Safety: the wl_egl_window is owned here and lives as long as the
        // EGL surface.
        let egl_surface = unsafe {
            shared
                .egl
                .create_window_surface(shared.display, shared.config, window_ptr, None)
        }
        .map_err(|_| RunError::Egl("eglCreateWindowSurface failed"))?;
        self.surface = Some(egl_surface);
        self.size = size;
        Ok(())
    }

    fn make_current(&self) -> Result<(), RunError> {
        let Some(surface) = self.surface else {
            return Err(RunError::Egl("no window surface"));
        };
        self.shared
            .egl
            .make_current(
                self.shared.display,
                Some(surface),
                Some(surface),
                Some(self.shared.context),
            )
            .map_err(|_| RunError::Egl("eglMakeCurrent failed"))
    }

    fn swap(&self) -> Result<(), RunError> {
        let Some(surface) = self.surface else {
            return Err(RunError::Egl("no window surface"));
        };
        self.shared
            .egl
            .swap_buffers(self.shared.display, surface)
            .map_err(|_| RunError::Egl("eglSwapBuffers failed"))
    }
}

impl Drop for Graphics {
    fn drop(&mut self) {
        // shortcut: per-popup GPU cleanup is deferred to the shared context's
        // abandon() at process exit. Flushing here requires borrowing the
        // shared DirectContext, which panics if a frame is currently being
        // rendered, and free_gpu_resources() would free textures that other
        // surfaces still need. We only destroy the EGL surface so the
        // compositor sees popup_destroyed before the wl_egl_window is gone.
        if let Some(surface) = self.surface.take() {
            let _ = self
                .shared
                .egl
                .destroy_surface(self.shared.display, surface);
        }
        self.window = None;
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
    fallback_fonts: Vec<Cow<'static, str>>,
    scale_factor: f64,
    size: Size2D,
    needs_layout: bool,
    waker: Waker,
    menu: Option<crate::app_menu::AppMenu>,
    on_menu_action: Option<MenuActionHandler>,
    resolved: State<Resolved>,
    accessibility: AccessibilityTree,
    requested_focus: Option<AccessibilityFocusStrategy>,
}

impl AppInstance {
    #[allow(clippy::too_many_arguments)]
    fn new(
        app: AppComponent,
        scale_factor: f64,
        size: (u32, u32),
        sender: Rc<dyn Fn(UserEvent)>,
        waker: Waker,
        toplevels: Option<Toplevels>,
        session_unlock: Option<SessionUnlock>,
        activation: Activation,
        window: Option<WindowControls>,
        timers: Timers,
        popups: Popups,
        popup_handle: Option<PopupHandle>,
        font_collection: FontCollection,
        font_manager: FontMgr,
        menu: Option<crate::app_menu::AppMenu>,
        on_menu_action: Option<MenuActionHandler>,
        resolved: State<Resolved>,
    ) -> Self {
        let (events_sender, events_receiver) = futures_channel::mpsc::unbounded();
        let mut runner = Runner::new(move || integration(app.clone()).into_element());

        runner.provide_root_context(ScreenReader::new);
        runner.provide_root_context(move || resolved);
        runner.provide_root_context(move || popups.clone());
        let (ticker_sender, ticker) = RenderingTicker::new();
        runner.provide_root_context(|| ticker);
        let animation_clock = runner.provide_root_context(AnimationClock::new);
        runner.provide_root_context(AssetCacher::create);

        let mut tree = Tree::default();

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
        runner.provide_root_context(move || activation);
        if let Some(window) = window {
            runner.provide_root_context(move || window);
        }
        if let Some(handle) = popup_handle {
            runner.provide_root_context(move || handle);
        }
        runner.provide_root_context(move || timers);
        runner.provide_root_context(|| State::create(None::<Box<dyn ClipboardProvider>>));
        runner.provide_root_context(|| tree.accessibility_generator.clone());
        runner.provide_root_context(|| font_collection.clone());

        // Build the root node before the first layout pass: measuring an empty
        // tree panics inside Torin.
        let mutations = runner.sync_and_update();
        let _ = runner.run_in(|| tree.apply_mutations(mutations));

        let mut accessibility = AccessibilityTree::default();
        let _ = accessibility.init(&mut tree);

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
            font_manager,
            fallback_fonts: default_fonts(),
            scale_factor,
            size: Size2D::new(size.0 as f32, size.1 as f32),
            needs_layout: true,
            waker,
            menu,
            on_menu_action,
            resolved,
            accessibility,
            requested_focus: None,
        }
    }

    /// Mirrors the compositor's keyboard focus into the Freya platform state.
    fn set_focused(&mut self, focused: bool) {
        self.platform.is_app_focused.set(focused);
        self.waker.wake_by_ref();
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
    /// Polls Freya once. `handle_events` returns ready while dirty scopes are
    /// pending; looping on it without calling `sync_and_update` spins forever,
    /// so the caller clears them in `update` and polls again on its next wake.
    fn poll(&mut self) -> bool {
        let mut cx = Context::from_waker(&self.waker);
        let ready = {
            let future = pin!(self.runner.handle_events());
            future.poll(&mut cx).is_ready()
        };

        let mut dirty = ready;
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
        dirty
    }

    fn update(&mut self) -> bool {
        if let Some(strategy) = self.requested_focus.take() {
            self.tree.accessibility_diff.request_focus(strategy);
        }
        let mutations = self.runner.sync_and_update();
        let result = self.runner.run_in(|| self.tree.apply_mutations(mutations));
        if result.needs_render {
            self.needs_layout = true;
        }
        self.layout();
        let accessibility_update = self
            .accessibility
            .process_updates(&mut self.tree, &self.events_sender);
        self.platform
            .focused_accessibility_id
            .set_if_modified(accessibility_update.focus);
        if let Some(node_id) = self.accessibility.focused_node_id()
            && let Some(layout_node) = self.tree.layout.get(&node_id)
        {
            self.platform.focused_accessibility_node.set_if_modified(
                AccessibilityTree::create_node(node_id, layout_node, &self.tree),
            );
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
            &self.fallback_fonts,
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
        // The event must be dispatched on the next loop wake; without this the
        // frame callback draws before Freya has processed the input.
        self.waker.wake_by_ref();
    }
}

/// Where a window's global menu lives on D-Bus (ADR 0040).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AppMenuAddress {
    pub service: String,
    pub path: String,
}

/// One running toplevel, as the dock lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToplevelInfo {
    pub title: String,
    pub app_id: String,
    pub active: bool,
    pub maximized: bool,
    pub minimized: bool,
    pub restorable: bool,
    /// The app's dbusmenu address, when it announced one.
    pub app_menu: Option<AppMenuAddress>,
    /// The GTK 3/4 D-Bus menu address, when announced via `gtk_shell1`.
    pub gtk_menus: Option<GtkMenusAddress>,
}

/// D-Bus address of a GTK global menu (ADR 0045).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GtkMenusAddress {
    pub unique_bus_name: String,
    pub application_id: String,
    pub menubar_path: String,
    pub app_menu_path: String,
    pub application_object_path: String,
    pub window_object_path: String,
}

impl GtkMenusAddress {
    /// Returns a `GtkMenusAddress` only when `unique_bus_name` is non-empty and
    /// at least one of `menubar_path` / `app_menu_path` is non-empty.
    pub(crate) fn new(
        unique_bus_name: String,
        application_id: String,
        menubar_path: String,
        app_menu_path: String,
        application_object_path: String,
        window_object_path: String,
    ) -> Option<Self> {
        if unique_bus_name.is_empty() || (menubar_path.is_empty() && app_menu_path.is_empty()) {
            return None;
        }
        Some(Self {
            unique_bus_name,
            application_id,
            menubar_path,
            app_menu_path,
            application_object_path,
            window_object_path,
        })
    }
}

/// A change that only touches one toplevel's metadata. Kept pure so the
/// interleaved-event ordering can be tested without Wayland.
#[derive(Clone, Debug, PartialEq, Eq)]
enum InfoEvent {
    Title(String),
    AppId(String),
    State {
        active: bool,
        maximized: bool,
        minimized: bool,
    },
    AppMenu(Option<AppMenuAddress>),
    GtkMenus(Option<GtkMenusAddress>),
    Closed,
}

fn apply_info_event(infos: &mut Vec<ToplevelInfo>, index: usize, event: InfoEvent) {
    let Some(info) = infos.get_mut(index) else {
        return;
    };
    match event {
        InfoEvent::Title(title) => info.title = title,
        InfoEvent::AppId(app_id) => info.app_id = app_id,
        InfoEvent::State {
            active,
            maximized,
            minimized,
        } => {
            info.active = active;
            info.maximized = maximized;
            info.minimized = minimized;
        }
        InfoEvent::AppMenu(app_menu) => info.app_menu = app_menu,
        InfoEvent::GtkMenus(gtk_menus) => info.gtk_menus = gtk_menus,
        InfoEvent::Closed => {
            infos.remove(index);
        }
    }
}

#[derive(Clone)]
struct ToplevelEntry {
    list: Option<ExtForeignToplevelHandleV1>,
    mgmt: Option<ZwlrForeignToplevelHandleV1>,
    restorable: bool,
    info: ToplevelInfo,
}

/// The running toplevels, provided as a root context. The dock reads
/// [`Toplevels::list`] and calls [`Toplevels::activate`] on a click.
type Activate = Rc<RefCell<Option<Rc<dyn Fn(usize)>>>>;
type Maximize = Rc<RefCell<Option<Rc<dyn Fn(usize, bool)>>>>;
type Arrange = Rc<RefCell<Option<Rc<dyn Fn(usize, InfinityLayout)>>>>;
type ArrangePair = Rc<RefCell<Option<Rc<dyn Fn(InfinityPairLayout)>>>>;
type ArrangeQuarters = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

#[derive(Clone)]
pub struct Toplevels {
    pub list: State<Vec<ToplevelInfo>>,
    activate: Activate,
    close: Activate,
    minimize: Activate,
    maximize: Maximize,
    unminimize: Activate,
    arrange: Arrange,
    arrange_pair: ArrangePair,
    arrange_quarters: ArrangeQuarters,
    restore: Activate,
    set_fullscreen: Maximize,
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

    pub fn close(&self, index: usize) {
        if let Some(f) = self.close.borrow().as_ref() {
            f(index);
        }
    }

    pub fn minimize(&self, index: usize) {
        if let Some(f) = self.minimize.borrow().as_ref() {
            f(index);
        }
    }

    pub fn unset_minimized(&self, index: usize) {
        if let Some(f) = self.unminimize.borrow().as_ref() {
            f(index);
        }
    }

    pub fn set_maximized(&self, index: usize, maximized: bool) {
        if let Some(f) = self.maximize.borrow().as_ref() {
            f(index, maximized);
        }
    }

    /// Minimizes every toplevel in the list.
    pub fn minimize_all(&self) {
        let len = self.list.read().len();
        for index in 0..len {
            self.minimize(index);
        }
    }

    /// Arranges the toplevel at `index` in the requested Infinity layout.
    pub fn arrange(&self, index: usize, layout: InfinityLayout) {
        if let Some(f) = self.arrange.borrow().as_ref() {
            f(index, layout);
        }
    }

    /// Tiles the active and previous toplevels as a pair.
    pub fn arrange_pair(&self, layout: InfinityPairLayout) {
        if let Some(f) = self.arrange_pair.borrow().as_ref() {
            f(layout);
        }
    }

    /// Tiles the four most recent toplevels in quarters.
    pub fn arrange_quarters(&self) {
        if let Some(f) = self.arrange_quarters.borrow().as_ref() {
            f();
        }
    }

    /// Restores the toplevel at `index` to its pre-arrange rectangle.
    pub fn restore(&self, index: usize) {
        if let Some(f) = self.restore.borrow().as_ref() {
            f(index);
        }
    }

    /// Sets or unsets fullscreen on the toplevel at `index`.
    pub fn set_fullscreen(&self, index: usize, fullscreen: bool) {
        if let Some(f) = self.set_fullscreen.borrow().as_ref() {
            f(index, fullscreen);
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

type TimerQueue = Rc<RefCell<Vec<(Duration, Rc<dyn Fn()>)>>>;
type WakeQueue = Rc<RefCell<Vec<Rc<dyn Fn()>>>>;

/// Wakes the render loop from another thread. `wake` only pings the loop; the
/// callback registered by [`Timers::wakeup`] runs on the render thread.
#[derive(Clone, Debug)]
pub struct WakeHandle {
    ping: Ping,
}

impl WakeHandle {
    pub fn wake(&self) {
        self.ping.ping();
    }

    pub(crate) fn from_ping(ping: Ping) -> Self {
        Self { ping }
    }
}

#[derive(Clone)]
struct Wake {
    ping: Ping,
    callbacks: WakeQueue,
}

/// Schedules one-shot callbacks on the runner's calloop loop. The app uses it
/// for the menubar clock; there is no periodic polling.
#[derive(Clone)]
pub struct Timers {
    queue: TimerQueue,
    ping: Ping,
    wake: Wake,
}

impl Timers {
    pub fn get() -> Self {
        freya_core::prelude::consume_root_context()
    }

    pub fn after(&self, delay: Duration, callback: impl Fn() + 'static) {
        self.queue.borrow_mut().push((delay, Rc::new(callback)));
        self.ping.ping();
    }

    /// Registers a callback that runs on the render thread whenever the
    /// returned handle is woken from any thread.
    pub fn wakeup(&self, callback: impl Fn() + 'static) -> WakeHandle {
        self.wake.callbacks.borrow_mut().push(Rc::new(callback));
        WakeHandle {
            ping: self.wake.ping.clone(),
        }
    }
}

type ActivationCallback = Box<dyn Fn(Option<String>)>;
type ActivationQueue = Rc<RefCell<Vec<ActivationCallback>>>;

/// Requests an `xdg-activation-v1` token for the next launched app.
///
/// The compositor only accepts a token created by the client that currently
/// has keyboard focus and that carries the last key press serial. The runner
/// owns both, so the app asks here and receives the token (or `None` when the
/// protocol is unavailable) in a callback.
#[derive(Clone)]
pub struct Activation {
    queue: ActivationQueue,
    ping: Ping,
}

impl Activation {
    pub fn get() -> Self {
        freya_core::prelude::consume_root_context()
    }

    /// Queues one token request. The callback runs on the Wayland loop with the
    /// token string, or with `None` if the compositor does not support it.
    pub fn request(&self, callback: impl Fn(Option<String>) + 'static) {
        self.queue.borrow_mut().push(Box::new(callback));
        self.ping.ping();
    }
}

/// Popup bookkeeping attached to a `SurfaceWindow` whose role is `Popup`.
struct PopupInfo {
    id: PopupId,
    parent: Option<PopupId>,
    on_dismissed: Option<Rc<dyn Fn()>>,
    opens_left: Rc<Cell<bool>>,
}

pub(crate) enum PopupCommand {
    Open {
        owner: ObjectId,
        parent: Option<PopupId>,
        id: PopupId,
        spec: PopupSpec,
        content: AppComponent,
        serial: u32,
    },
    Reposition {
        id: PopupId,
        spec: PopupSpec,
    },
    Close {
        id: PopupId,
    },
}

/// A surface plus its Freya app and GL context.
struct SurfaceWindow {
    surface: WlSurface,
    role: SurfaceRole,
    popup: Option<PopupInfo>,
    shared: Rc<GraphicsShared>,
    graphics: Option<Graphics>,
    app: AppInstance,
    logical_size: (u32, u32),
    buffer_size: (u32, u32),
    scale: f64,
    output: Option<ObjectId>,
    needs_redraw: bool,
    frame_pending: bool,
    draw_count: u64,
    configure_count: u64,
    /// The most recent window configure not yet applied; older configures are
    /// dropped so only the last one is drawn.
    pending_configure: Option<(WindowConfigure, u32)>,
    appmenu: Option<OrgKdeKwinAppmenu>,
    appmenu_address: Option<AppMenuAddress>,
    menu_handle: Option<crate::app_menu::MenuHandle>,
}

#[allow(dead_code)]
#[derive(Debug)]
enum SurfaceRole {
    Layer(LayerSurface),
    Lock(SessionLockSurface),
    Window(Window),
    Popup(Popup),
    None,
}

impl SurfaceWindow {
    /// Schedules a compositor frame callback without marking the surface dirty.
    fn request_frame_callback(&mut self, qh: &QueueHandle<Shell>) {
        if !self.frame_pending {
            self.frame_pending = true;
            let _callback: WlCallback = self.surface.frame(qh, self.surface.clone());
            self.surface.commit();
        }
    }

    fn request_redraw(&mut self, qh: &QueueHandle<Shell>) {
        self.needs_redraw = true;
        self.request_frame_callback(qh);
    }

    fn configure(&mut self, size: (u32, u32), scale: f64) {
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let buffer_size = (
            (size.0 as f64 * scale).round().max(1.0) as u32,
            (size.1 as f64 * scale).round().max(1.0) as u32,
        );

        if self.graphics.is_none() {
            self.graphics = Some(Graphics::new(self.shared.clone()));
        }
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        if let Err(error) = graphics.configure(&self.surface, buffer_size, scale) {
            tracing::error!("BarrierKit: {error}");
            // Keep the previous size coherent, but do not push a 0×0 size into
            // the app on the first configure; that panics inside Torin.
            if self.buffer_size.0 > 0 && self.buffer_size.1 > 0 {
                self.app.resize(self.buffer_size, self.scale);
            }
            self.needs_redraw = true;
            return;
        }
        self.logical_size = size;
        self.scale = scale;
        self.buffer_size = buffer_size;
        self.surface.set_buffer_scale(scale.round().max(1.0) as i32);
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
        self.draw_count += 1;
        tracing::debug!(
            surface = ?self.surface.id(),
            draws = self.draw_count,
            role = ?self.role,
            "BarrierKit: draw"
        );
        self.app.layout();
        let background = if matches!(&self.role, SurfaceRole::Window(_)) {
            let resolved = *self.app.resolved.read();
            crate::theme::color(crate::theme::surface_background(
                &resolved,
                crate::theme::SurfaceKind::Toplevel,
            ))
        } else {
            Color::TRANSPARENT
        };
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
                background,
            }
            .render();
        }
        graphics.shared.gr.borrow_mut().flush_and_submit();
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

struct RepeatState {
    event: KeyEvent,
    delay: Duration,
    rate: Duration,
    first: bool,
    pending: bool,
    generation: u64,
}

/// Cancels the current repeat and invalidates any timer already in flight.
fn stop_repeat(repeat: &mut Option<RepeatState>, generation: &mut u64) {
    *repeat = None;
    *generation = generation.wrapping_add(1);
}

struct Shell {
    registry: RegistryState,
    compositor: CompositorState,
    output: OutputState,
    layer_shell: Option<LayerShell>,
    xdg_shell: Option<XdgShell>,
    session_lock: Option<SessionLockState>,
    active_lock: Option<SessionLock>,
    surfaces: Vec<SurfaceWindow>,
    graphics: Rc<GraphicsShared>,
    seat: SeatState,
    pointer: Option<wl_pointer::WlPointer>,
    cursor_shape: Option<CursorShapeManager>,
    cursor_shape_device: Option<WpCursorShapeDeviceV1>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    keyboard_focus: Option<WlSurface>,
    repeat: Option<RepeatState>,
    repeat_info: Option<RepeatInfo>,
    repeat_generation: u64,
    modifiers: SctkModifiers,
    loop_signal: Option<LoopSignal>,
    app: AppComponent,
    spec: SurfaceSpec,
    needs_exit: bool,
    request_redraw_all: bool,
    queue: Rc<RefCell<Vec<(ObjectId, UserEvent)>>>,
    ping: Ping,
    ping_source: Option<PingSource>,
    wake: Wake,
    wake_source: Option<PingSource>,
    qh: QueueHandle<Shell>,
    _foreign_list: Option<ExtForeignToplevelListV1>,
    _foreign_mgmt: Option<ZwlrForeignToplevelManagerV1>,
    _infinity_manager: Rc<RefCell<Option<InfinityWindowManagerV1>>>,
    appmenu_manager: Option<OrgKdeKwinAppmenuManager>,
    toplevel_entries: Rc<RefCell<Vec<ToplevelEntry>>>,
    toplevels: Toplevels,
    session_unlock: Option<SessionUnlock>,
    session_locked: bool,
    timers: TimerQueue,
    activation: Activation,
    activation_global: Option<XdgActivationV1>,
    activation_callbacks: Vec<Option<ActivationCallback>>,
    activation_seat: Option<wl_seat::WlSeat>,
    last_key_serial: u32,
    last_input_serial: Rc<Cell<u32>>,
    last_pointer_serial: Cell<u32>,
    touch_surfaces: std::collections::HashMap<i32, WlSurface>,
    popup_commands: Rc<RefCell<Vec<PopupCommand>>>,
    popup_next_id: Rc<Cell<u64>>,
    surface_count: Rc<Cell<usize>>,
    window_contexts: Vec<WindowContext>,
    decoration_manager: bool,
    decoration_warned: bool,
    appearance_watch: Option<AppearanceWatch>,
    resolved: State<Resolved>,
}

#[derive(Clone)]
struct WindowContext {
    controls: WindowControls,
    queue: Rc<RefCell<Vec<WindowCommand>>>,
}

fn make_sender(
    queue: Rc<RefCell<Vec<(ObjectId, UserEvent)>>>,
    ping: Ping,
    owner: ObjectId,
) -> Rc<dyn Fn(UserEvent)> {
    Rc::new(move |event| {
        queue.borrow_mut().push((owner.clone(), event));
        ping.ping();
    })
}

fn resize_edge(edge: WindowEdge) -> xdg_toplevel::ResizeEdge {
    match edge {
        WindowEdge::Top => xdg_toplevel::ResizeEdge::Top,
        WindowEdge::Bottom => xdg_toplevel::ResizeEdge::Bottom,
        WindowEdge::Left => xdg_toplevel::ResizeEdge::Left,
        WindowEdge::Right => xdg_toplevel::ResizeEdge::Right,
        WindowEdge::TopLeft => xdg_toplevel::ResizeEdge::TopLeft,
        WindowEdge::TopRight => xdg_toplevel::ResizeEdge::TopRight,
        WindowEdge::BottomLeft => xdg_toplevel::ResizeEdge::BottomLeft,
        WindowEdge::BottomRight => xdg_toplevel::ResizeEdge::BottomRight,
    }
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

fn anchor_to_sctk(anchor: PopupAnchor) -> SctkAnchorPos {
    match anchor {
        PopupAnchor::TopLeft => SctkAnchorPos::TopLeft,
        PopupAnchor::Top => SctkAnchorPos::Top,
        PopupAnchor::TopRight => SctkAnchorPos::TopRight,
        PopupAnchor::Left => SctkAnchorPos::Left,
        PopupAnchor::Right => SctkAnchorPos::Right,
        PopupAnchor::BottomLeft => SctkAnchorPos::BottomLeft,
        PopupAnchor::Bottom => SctkAnchorPos::Bottom,
        PopupAnchor::BottomRight => SctkAnchorPos::BottomRight,
    }
}

fn gravity_to_sctk(gravity: PopupGravity) -> SctkGravity {
    match gravity {
        PopupGravity::TopLeft => SctkGravity::TopLeft,
        PopupGravity::Top => SctkGravity::Top,
        PopupGravity::TopRight => SctkGravity::TopRight,
        PopupGravity::Left => SctkGravity::Left,
        PopupGravity::Right => SctkGravity::Right,
        PopupGravity::BottomLeft => SctkGravity::BottomLeft,
        PopupGravity::Bottom => SctkGravity::Bottom,
        PopupGravity::BottomRight => SctkGravity::BottomRight,
    }
}

fn constraint_to_sctk(c: PopupConstraintAdjustment) -> SctkConstraint {
    let mut out = SctkConstraint::empty();
    if c.flip_x {
        out |= SctkConstraint::FlipX;
    }
    if c.flip_y {
        out |= SctkConstraint::FlipY;
    }
    if c.slide_x {
        out |= SctkConstraint::SlideX;
    }
    if c.slide_y {
        out |= SctkConstraint::SlideY;
    }
    out
}

fn drain_user_events(
    events: &Rc<RefCell<Vec<(ObjectId, UserEvent)>>>,
) -> Vec<(ObjectId, UserEvent)> {
    std::mem::take(&mut *events.borrow_mut())
}

fn cursor_shape_from_icon(icon: CursorIcon) -> CursorShape {
    match icon {
        CursorIcon::Text => CursorShape::Text,
        CursorIcon::Pointer => CursorShape::Pointer,
        CursorIcon::Grab => CursorShape::Grab,
        CursorIcon::Grabbing => CursorShape::Grabbing,
        CursorIcon::NotAllowed => CursorShape::NotAllowed,
        CursorIcon::NResize => CursorShape::NResize,
        CursorIcon::EResize => CursorShape::EResize,
        CursorIcon::SResize => CursorShape::SResize,
        CursorIcon::WResize => CursorShape::WResize,
        CursorIcon::NeResize => CursorShape::NeResize,
        CursorIcon::NwResize => CursorShape::NwResize,
        CursorIcon::SeResize => CursorShape::SeResize,
        CursorIcon::SwResize => CursorShape::SwResize,
        CursorIcon::EwResize => CursorShape::EwResize,
        CursorIcon::NsResize => CursorShape::NsResize,
        CursorIcon::NeswResize => CursorShape::NeswResize,
        CursorIcon::NwseResize => CursorShape::NwseResize,
        CursorIcon::ColResize => CursorShape::ColResize,
        CursorIcon::RowResize => CursorShape::RowResize,
        CursorIcon::Move => CursorShape::Move,
        CursorIcon::Copy => CursorShape::Copy,
        CursorIcon::Alias => CursorShape::Alias,
        CursorIcon::NoDrop => CursorShape::NoDrop,
        CursorIcon::ContextMenu => CursorShape::ContextMenu,
        CursorIcon::Help => CursorShape::Help,
        CursorIcon::Progress => CursorShape::Progress,
        CursorIcon::Wait => CursorShape::Wait,
        CursorIcon::Cell => CursorShape::Cell,
        CursorIcon::Crosshair => CursorShape::Crosshair,
        CursorIcon::VerticalText => CursorShape::VerticalText,
        CursorIcon::AllScroll => CursorShape::AllScroll,
        CursorIcon::ZoomIn => CursorShape::ZoomIn,
        CursorIcon::ZoomOut => CursorShape::ZoomOut,
        CursorIcon::DndAsk => CursorShape::DndAsk,
        CursorIcon::AllResize => CursorShape::AllResize,
        CursorIcon::Default => CursorShape::Default,
        // cursor-icon is non-exhaustive; unknown icons fall back to the arrow.
        _ => CursorShape::Default,
    }
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
        SurfaceSpec::Lock | SurfaceSpec::Window(_) => {
            return Err(RunError::MissingGlobal("layer surface"));
        }
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

    let waker = futures_util::task::waker(Arc::new(PingWake(shell.ping.clone())));
    let sender = make_sender(shell.queue.clone(), shell.ping.clone(), surface.id());
    let popups = Popups {
        queue: shell.popup_commands.clone(),
        ping: shell.ping.clone(),
        next_id: shell.popup_next_id.clone(),
        last_serial: shell.last_input_serial.clone(),
        owner: surface.id(),
        current: None,
        surface_count: shell.surface_count.clone(),
        opens_left: Rc::new(Cell::new(false)),
    };
    let app_instance = AppInstance::new(
        shell.app.clone(),
        1.0,
        size.unwrap_or((1, 1)),
        sender,
        waker,
        Some(shell.toplevels.clone()),
        None,
        shell.activation.clone(),
        None,
        Timers {
            queue: shell.timers.clone(),
            ping: shell.ping.clone(),
            wake: shell.wake.clone(),
        },
        popups,
        None,
        shell.graphics.font_collection.clone(),
        shell.graphics.font_manager.clone(),
        None,
        None,
        shell.resolved,
    );
    shell.surfaces.push(SurfaceWindow {
        surface: surface.clone(),
        role: SurfaceRole::Layer(layer_surface),
        popup: None,
        shared: shell.graphics.clone(),
        graphics: None,
        app: app_instance,
        logical_size: size.unwrap_or((0, 0)),
        buffer_size: (0, 0),
        scale: 1.0,
        output: None,
        needs_redraw: true,
        frame_pending: false,
        draw_count: 0,
        configure_count: 0,
        pending_configure: None,
        appmenu: None,
        appmenu_address: None,
        menu_handle: None,
    });
    shell.surface_count.set(shell.surfaces.len());

    let mut event_loop: EventLoop<Shell> =
        EventLoop::try_new().map_err(|_| RunError::Egl("calloop EventLoop::try_new failed"))?;
    insert_ping(&mut event_loop, &mut shell)?;
    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|_| RunError::Egl("inserting the Wayland source"))?;
    let signal = event_loop.get_signal();
    shell.loop_signal = Some(signal.clone());
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

/// xdg_toplevel.set_min_size and set_max_size take signed 32-bit ints where
/// 0 means "unbounded". Values that do not fit in i32 must be sent as 0 to
/// avoid the `invalid_size` protocol error.
fn clamp_toplevel_size_dimension(value: u32) -> u32 {
    if value > i32::MAX as u32 { 0 } else { value }
}

/// Runs a normal BarrierKit window (`xdg_toplevel`, ADR 0035).
pub fn run_window(app: AppComponent, options: WindowOptions) -> Result<(), RunError> {
    let (conn, event_queue, qh, mut shell) =
        Shell::connect(app.clone(), SurfaceSpec::Window(options.clone()))?;
    shell.qh = qh.clone();
    shell
        .create_window(app, options, None)
        .ok_or(RunError::MissingGlobal("xdg_wm_base"))?;

    let mut event_loop: EventLoop<Shell> =
        EventLoop::try_new().map_err(|_| RunError::Egl("calloop EventLoop::try_new failed"))?;
    insert_ping(&mut event_loop, &mut shell)?;
    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|_| RunError::Egl("inserting the Wayland source"))?;
    let signal = event_loop.get_signal();
    shell.loop_signal = Some(signal);
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
    let flush_conn = conn.clone();
    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|_| RunError::Egl("inserting the Wayland source"))?;
    let signal = event_loop.get_signal();
    shell.loop_signal = Some(signal.clone());
    if let Some(lock) = shell.active_lock.clone() {
        shell.session_unlock = Some(SessionUnlock {
            unlock: Rc::new(move || {
                lock.unlock();
                let _ = flush_conn.flush();
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
    let handle = event_loop.handle();
    event_loop
        .handle()
        .insert_source(source, move |_, _, shell: &mut Shell| {
            for (surface_id, event) in drain_user_events(&shell.queue) {
                match event {
                    UserEvent::RequestRedraw => {
                        shell.request_redraw_all = true;
                    }
                    UserEvent::SetCursorIcon(icon) => {
                        if let Some(device) = &shell.cursor_shape_device {
                            device.set_shape(
                                shell.last_pointer_serial.get(),
                                cursor_shape_from_icon(icon),
                            );
                        }
                    }
                    UserEvent::FocusAccessibilityNode(strategy) => {
                        if let Some(index) = shell
                            .surfaces
                            .iter()
                            .position(|s| s.surface.id() == surface_id)
                        {
                            shell.surfaces[index].app.requested_focus = Some(strategy);
                            shell.surfaces[index].needs_redraw = true;
                        }
                    }
                    UserEvent::Erased(_) => {}
                }
            }
            shell.poll_all();
            shell.process_popup_commands();
            shell.install_timers(&handle);
            shell.install_repeat(&handle);
            shell.redraw_all();
        })
        .map_err(|_| RunError::Egl("inserting the Freya ping source"))?;

    let wake_source = shell
        .wake_source
        .take()
        .ok_or(RunError::Egl("the wake ping source is already installed"))?;
    let callbacks = shell.wake.callbacks.clone();
    event_loop
        .handle()
        .insert_source(wake_source, move |_, _, shell: &mut Shell| {
            let callbacks: Vec<Rc<dyn Fn()>> = callbacks.borrow().clone();
            for callback in callbacks {
                callback();
            }
            shell.poll_all();
            shell.redraw_all();
        })
        .map_err(|_| RunError::Egl("inserting the wake ping source"))?;

    // Watch appearance.domain and wallpaper.state; when they change, resolve
    // the theme again and update the process-wide reactive state so every
    // surface that reads theme::resolved() re-executes.
    let watch = AppearanceWatch::new().map_err(|error| {
        RunError::Io(std::io::Error::other(format!(
            "creating the appearance watcher: {error}"
        )))
    })?;
    let watch_fd = watch.as_fd().try_clone_to_owned().map_err(RunError::Io)?;
    shell.appearance_watch = Some(watch);
    event_loop
        .handle()
        .insert_source(
            Generic::new(watch_fd, Interest::READ, Mode::Level),
            |_, _, shell: &mut Shell| {
                if let Some(watch) = shell.appearance_watch.as_ref()
                    && let Ok(events) = watch.drain()
                    && !events.is_empty()
                {
                    let resolved = resolve(&load_appearance(), load_dominant());
                    if *shell.resolved.peek() != resolved {
                        shell.resolved.set(resolved);
                        shell.request_redraw_all = true;
                        shell.ping.ping();
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|_| RunError::Egl("inserting the appearance watch source"))?;
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
        let queue: Rc<RefCell<Vec<(ObjectId, UserEvent)>>> = Rc::new(RefCell::new(Vec::new()));
        let (ping, ping_source) =
            calloop::ping::make_ping().map_err(|_| RunError::Egl("calloop ping failed"))?;
        let (wake_ping, wake_source) =
            calloop::ping::make_ping().map_err(|_| RunError::Egl("calloop wake ping failed"))?;
        let resolved = State::create_global(resolve(&load_appearance(), load_dominant()));
        let toplevel_entries: Rc<RefCell<Vec<ToplevelEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let close_entries = toplevel_entries.clone();
        let minimize_entries = toplevel_entries.clone();
        let maximize_entries = toplevel_entries.clone();
        let unminimize_entries = toplevel_entries.clone();
        let activate_entries = toplevel_entries.clone();
        let arrange_entries = toplevel_entries.clone();
        let restore_entries = toplevel_entries.clone();
        let fullscreen_entries = toplevel_entries.clone();
        let infinity_manager = Rc::new(RefCell::new(None::<InfinityWindowManagerV1>));
        let infinity_manager_arrange = infinity_manager.clone();
        let infinity_manager_pair = infinity_manager.clone();
        let infinity_manager_quarters = infinity_manager.clone();
        let infinity_manager_restore = infinity_manager.clone();
        let infinity_manager_fullscreen = infinity_manager.clone();
        let toplevels = Toplevels {
            list: State::create_global(Vec::new()),
            activate: Rc::new(RefCell::new(None)),
            close: Rc::new(RefCell::new(Some(Rc::new(move |index| {
                if let Some(entry) = close_entries.borrow().get(index)
                    && let Some(mgmt) = &entry.mgmt
                {
                    mgmt.close();
                }
            })))),
            minimize: Rc::new(RefCell::new(Some(Rc::new(move |index| {
                if let Some(entry) = minimize_entries.borrow().get(index)
                    && let Some(mgmt) = &entry.mgmt
                {
                    mgmt.set_minimized();
                }
            })))),
            maximize: Rc::new(RefCell::new(Some(Rc::new(move |index, maximized| {
                if let Some(entry) = maximize_entries.borrow().get(index)
                    && let Some(mgmt) = &entry.mgmt
                {
                    if maximized {
                        mgmt.set_maximized();
                    } else {
                        mgmt.unset_maximized();
                    }
                }
            })))),
            unminimize: Rc::new(RefCell::new(Some(Rc::new(move |index| {
                if let Some(entry) = unminimize_entries.borrow().get(index)
                    && let Some(mgmt) = &entry.mgmt
                {
                    mgmt.unset_minimized();
                }
            })))),
            arrange: Rc::new(RefCell::new(Some(Rc::new(move |index, layout| {
                if let Some(entry) = arrange_entries.borrow().get(index)
                    && let Some(mgmt) = &entry.mgmt
                    && let Some(manager) = infinity_manager_arrange.borrow().as_ref()
                {
                    manager.arrange(mgmt, layout);
                }
            })))),
            arrange_pair: Rc::new(RefCell::new(Some(Rc::new(move |layout| {
                if let Some(manager) = infinity_manager_pair.borrow().as_ref() {
                    manager.arrange_pair(layout);
                }
            })))),
            arrange_quarters: Rc::new(RefCell::new(Some(Rc::new(move || {
                if let Some(manager) = infinity_manager_quarters.borrow().as_ref() {
                    manager.arrange_quarters();
                }
            })))),
            restore: Rc::new(RefCell::new(Some(Rc::new(move |index| {
                if let Some(entry) = restore_entries.borrow().get(index)
                    && let Some(mgmt) = &entry.mgmt
                    && let Some(manager) = infinity_manager_restore.borrow().as_ref()
                {
                    manager.restore(mgmt);
                }
            })))),
            set_fullscreen: Rc::new(RefCell::new(Some(Rc::new(move |index, fullscreen| {
                if let Some(entry) = fullscreen_entries.borrow().get(index)
                    && let Some(mgmt) = &entry.mgmt
                    && let Some(manager) = infinity_manager_fullscreen.borrow().as_ref()
                {
                    manager.set_fullscreen(mgmt, fullscreen as u32);
                }
            })))),
        };
        let foreign_list = globals
            .bind::<ExtForeignToplevelListV1, _, _>(&qh, 1..=1, ())
            .ok();
        let foreign_mgmt = globals
            .bind::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 3..=3, ())
            .ok();
        let infinity_manager_bound = globals
            .bind::<InfinityWindowManagerV1, _, _>(&qh, 1..=3, ())
            .ok();
        *infinity_manager.borrow_mut() = infinity_manager_bound;
        let activation_global = globals.bind::<XdgActivationV1, _, _>(&qh, 1..=1, ()).ok();
        let appmenu_manager = globals
            .bind::<OrgKdeKwinAppmenuManager, _, _>(&qh, 1..=1, ())
            .ok();
        // `SeatState::new` binds the seats that already exist without calling
        // `SeatHandler::new_seat`, so remember one here for activation and
        // popup grabs. Hotplugged seats still arrive through `new_seat`.
        let seat = SeatState::new(&globals, &qh);
        let activation_seat = seat.seats().next();
        // `SeatState::new` binds existing seats without calling `new_seat`, so
        // install the activation command for the seat already present.
        if let Some(seat) = activation_seat.clone() {
            *toplevels.activate.borrow_mut() = Some(Rc::new(move |index| {
                if let Some(entry) = activate_entries.borrow().get(index)
                    && let Some(mgmt) = &entry.mgmt
                {
                    mgmt.activate(&seat);
                }
            }));
        }
        let shell = Shell {
            registry: RegistryState::new(&globals),
            compositor: CompositorState::bind(&globals, &qh)
                .map_err(|_| RunError::MissingGlobal("wl_compositor"))?,
            output: OutputState::new(&globals, &qh),
            layer_shell: LayerShell::bind(&globals, &qh).ok(),
            xdg_shell: XdgShell::bind(&globals, &qh).ok(),
            session_lock: Some(SessionLockState::new(&globals, &qh)),
            active_lock: None,
            surfaces: Vec::new(),
            graphics: {
                let display_ptr = conn.display().id().as_ptr().cast::<c_void>();
                Rc::new(GraphicsShared::new(display_ptr)?)
            },
            seat,
            pointer: None,
            cursor_shape: CursorShapeManager::bind(&globals, &qh).ok(),
            cursor_shape_device: None,
            keyboard: None,
            touch: None,
            keyboard_focus: None,
            repeat: None,
            repeat_info: None,
            repeat_generation: 0,
            modifiers: SctkModifiers::default(),
            loop_signal: None,
            app,
            spec,
            needs_exit: false,
            request_redraw_all: false,
            queue,
            ping: ping.clone(),
            ping_source: Some(ping_source),
            wake: Wake {
                ping: wake_ping,
                callbacks: Rc::new(RefCell::new(Vec::new())),
            },
            wake_source: Some(wake_source),
            qh: qh.clone(),
            _foreign_list: foreign_list,
            _foreign_mgmt: foreign_mgmt,
            _infinity_manager: infinity_manager,
            appmenu_manager,
            toplevel_entries,
            toplevels,
            session_unlock: None,
            session_locked: false,
            timers: Rc::new(RefCell::new(Vec::new())),
            activation: Activation {
                queue: Rc::new(RefCell::new(Vec::new())),
                ping: ping.clone(),
            },
            activation_global,
            activation_callbacks: Vec::new(),
            activation_seat,
            last_key_serial: 0,
            last_input_serial: Rc::new(Cell::new(0)),
            last_pointer_serial: Cell::new(0),
            touch_surfaces: std::collections::HashMap::new(),
            popup_commands: Rc::new(RefCell::new(Vec::new())),
            popup_next_id: Rc::new(Cell::new(1)),
            surface_count: Rc::new(Cell::new(0)),
            window_contexts: Vec::new(),
            decoration_manager: globals.contents().with_list(|list| {
                list.iter()
                    .any(|global| global.interface == "zxdg_decoration_manager_v1")
            }),
            decoration_warned: false,
            appearance_watch: None,
            resolved,
        };
        Ok((conn, event_queue, qh, shell))
    }

    fn surface_for(&self, surface: &WlSurface) -> Option<usize> {
        self.surfaces.iter().position(|s| s.surface == *surface)
    }

    fn create_window(
        &mut self,
        app: AppComponent,
        options: WindowOptions,
        parent: Option<ObjectId>,
    ) -> Option<WindowControls> {
        let xdg_shell = self.xdg_shell.as_ref()?;
        let qh = &self.qh;
        let surface = self.compositor.create_surface(qh);
        let decorations = match options.decorations {
            Decorations::Client => WindowDecorations::RequestClient,
            Decorations::Server => WindowDecorations::RequestServer,
        };
        let window = xdg_shell.create_window(surface.clone(), decorations, qh);
        let app_id = options
            .app_id
            .clone()
            .unwrap_or_else(|| format!("{}{}", vow_design::NATIVE_APP_ID_PREFIX, options.app_name));
        window.set_app_id(app_id);
        window.set_title(options.title.clone());
        if let Some(min) = options.min_size {
            window.set_min_size(Some((
                clamp_toplevel_size_dimension(min.0),
                clamp_toplevel_size_dimension(min.1),
            )));
        }
        if let Some(max) = options.max_size {
            window.set_max_size(Some((
                clamp_toplevel_size_dimension(max.0),
                clamp_toplevel_size_dimension(max.1),
            )));
        }
        if let Some(parent_id) = parent
            && let Some(parent_window) = self.surfaces.iter().find_map(|s| {
                if s.surface.id() == parent_id
                    && let SurfaceRole::Window(parent) = &s.role
                {
                    return Some(parent);
                }
                None
            })
        {
            window.set_parent(Some(parent_window));
        }
        window.commit();

        let menu = options.menu.clone();
        let on_menu_action = options.on_menu_action.clone();
        let mut menu_handle: Option<crate::app_menu::MenuHandle> = None;
        let mut appmenu_address: Option<AppMenuAddress> = None;
        if let Some(menu) = menu {
            let handle = crate::app_menu::MenuHandle::new(
                menu.clone(),
                Some(WakeHandle::from_ping(self.wake.ping.clone())),
            );
            let on_action = on_menu_action.clone().unwrap_or_else(|| Arc::new(|_| {}));
            if let Some(address) =
                crate::app_menu::server::register(menu, handle.clone(), on_action)
            {
                appmenu_address = Some(address);
            }
            menu_handle = Some(handle);
        }

        let mut appmenu_object: Option<OrgKdeKwinAppmenu> = None;
        if let (Some(manager), Some(address)) = (&self.appmenu_manager, &appmenu_address) {
            let appmenu = manager.create(&surface, qh, surface.id());
            appmenu.set_address(address.service.clone(), address.path.clone());
            appmenu_object = Some(appmenu);
        }

        let queue: Rc<RefCell<Vec<WindowCommand>>> = Rc::new(RefCell::new(Vec::new()));
        let pointer_serial = Rc::new(std::cell::Cell::new(0));
        let fixed = options.min_size.is_some() && options.min_size == options.max_size;
        let status = State::create_global(WindowStatus {
            activated: true,
            maximized: false,
            decoration: match options.decorations {
                Decorations::Client => DecorationResponse::Client,
                Decorations::Server => DecorationResponse::Server,
            },
            fixed,
        });
        let surface_id = surface.id();
        let controls = WindowControls {
            queue: queue.clone(),
            ping: self.ping.clone(),
            pointer_serial,
            status,
            title: options.title.clone(),
            fixed,
            surface_id,
            menu_handle: menu_handle.clone(),
        };
        self.window_contexts.push(WindowContext {
            controls: controls.clone(),
            queue,
        });

        let waker = futures_util::task::waker(Arc::new(PingWake(self.ping.clone())));
        let sender = make_sender(self.queue.clone(), self.ping.clone(), surface.id());
        let popups = Popups {
            queue: self.popup_commands.clone(),
            ping: self.ping.clone(),
            next_id: self.popup_next_id.clone(),
            last_serial: self.last_input_serial.clone(),
            owner: surface.id(),
            current: None,
            surface_count: self.surface_count.clone(),
            opens_left: Rc::new(Cell::new(false)),
        };
        let app_instance = AppInstance::new(
            app,
            1.0,
            options.size,
            sender,
            waker,
            Some(self.toplevels.clone()),
            None,
            self.activation.clone(),
            Some(controls.clone()),
            Timers {
                queue: self.timers.clone(),
                ping: self.ping.clone(),
                wake: self.wake.clone(),
            },
            popups,
            None,
            self.graphics.font_collection.clone(),
            self.graphics.font_manager.clone(),
            options.menu.clone(),
            options.on_menu_action.clone(),
            self.resolved,
        );
        self.surfaces.push(SurfaceWindow {
            surface,
            role: SurfaceRole::Window(window),
            popup: None,
            shared: self.graphics.clone(),
            graphics: None,
            app: app_instance,
            logical_size: options.size,
            buffer_size: (0, 0),
            scale: 1.0,
            output: None,
            needs_redraw: true,
            frame_pending: false,
            draw_count: 0,
            configure_count: 0,
            pending_configure: None,
            appmenu: appmenu_object,
            appmenu_address: appmenu_address.clone(),
            menu_handle,
        });
        self.surface_count.set(self.surfaces.len());
        Some(controls)
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
        let waker = futures_util::task::waker(Arc::new(PingWake(self.ping.clone())));
        let sender = make_sender(self.queue.clone(), self.ping.clone(), surface.id());
        let popups = Popups {
            queue: self.popup_commands.clone(),
            ping: self.ping.clone(),
            next_id: self.popup_next_id.clone(),
            last_serial: self.last_input_serial.clone(),
            owner: surface.id(),
            current: None,
            surface_count: self.surface_count.clone(),
            opens_left: Rc::new(Cell::new(false)),
        };
        let app = AppInstance::new(
            self.app.clone(),
            1.0,
            (1, 1),
            sender,
            waker,
            Some(self.toplevels.clone()),
            self.session_unlock.clone(),
            self.activation.clone(),
            None,
            Timers {
                queue: self.timers.clone(),
                ping: self.ping.clone(),
                wake: self.wake.clone(),
            },
            popups,
            None,
            self.graphics.font_collection.clone(),
            self.graphics.font_manager.clone(),
            None,
            None,
            self.resolved,
        );
        self.surfaces.push(SurfaceWindow {
            surface,
            role: SurfaceRole::Lock(lock_surface),
            popup: None,
            shared: self.graphics.clone(),
            graphics: None,
            app,
            logical_size: (0, 0),
            buffer_size: (0, 0),
            scale: 1.0,
            output: Some(output.id()),
            needs_redraw: true,
            frame_pending: false,
            draw_count: 0,
            configure_count: 0,
            pending_configure: None,
            appmenu: None,
            appmenu_address: None,
            menu_handle: None,
        });
        self.surface_count.set(self.surfaces.len());
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
        self.poll_activation();
        self.process_window_commands();
    }

    fn process_window_commands(&mut self) {
        let contexts = self.window_contexts.clone();
        for context in contexts {
            let commands = std::mem::take(&mut *context.queue.borrow_mut());
            for command in commands {
                if let WindowCommand::Open(app, options) = command {
                    let parent = options.parent.clone();
                    self.create_window(app, options, parent);
                    continue;
                }
                let Some(index) = self.surfaces.iter().position(|surface| {
                    matches!(surface.role, SurfaceRole::Window(_))
                        && surface.surface.id() == context.controls.surface_id
                }) else {
                    continue;
                };
                let SurfaceRole::Window(window) = &self.surfaces[index].role else {
                    continue;
                };
                match command {
                    WindowCommand::Minimize => window.set_minimized(),
                    WindowCommand::ToggleMaximize | WindowCommand::ToggleVerticalMaximize => {
                        if context.controls.status.read().maximized {
                            window.unset_maximized();
                        } else {
                            window.set_maximized();
                        }
                    }
                    WindowCommand::Move(serial) => {
                        if let Some(seat) = &self.activation_seat {
                            window.move_(seat, serial);
                        }
                    }
                    WindowCommand::Resize(serial, edge) => {
                        if let Some(seat) = &self.activation_seat {
                            window.resize(seat, serial, resize_edge(edge));
                        }
                    }
                    WindowCommand::Open(..) => unreachable!(),
                }
                self.surfaces[index].needs_redraw = true;
            }
        }
    }

    fn process_popup_commands(&mut self) {
        let commands = std::mem::take(&mut *self.popup_commands.borrow_mut());
        for command in commands {
            match command {
                PopupCommand::Open {
                    owner,
                    parent,
                    id,
                    spec,
                    content,
                    serial,
                } => {
                    self.open_popup(owner, parent, id, spec, content, serial);
                }
                PopupCommand::Reposition { id, spec } => {
                    self.reposition_popup(id, spec);
                }
                PopupCommand::Close { id } => {
                    self.close_popup(id);
                }
            }
        }
    }

    fn open_popup(
        &mut self,
        owner: ObjectId,
        parent: Option<PopupId>,
        id: PopupId,
        spec: PopupSpec,
        content: AppComponent,
        serial: u32,
    ) {
        let Some(owner_index) = self.surfaces.iter().position(|s| s.surface.id() == owner) else {
            return;
        };
        let Some(xdg_shell) = self.xdg_shell.as_ref() else {
            return;
        };
        let scale = self.surfaces[owner_index].scale;

        let parent_xdg_surface = parent
            .and_then(|parent_id| {
                self.surfaces.iter().find_map(|s| {
                    if let SurfaceRole::Popup(popup) = &s.role
                        && s.popup.as_ref().is_some_and(|info| info.id == parent_id)
                    {
                        Some(popup.xdg_surface().clone())
                    } else {
                        None
                    }
                })
            })
            .or_else(|| {
                // A toplevel parent must be passed as the xdg_surface parent,
                // not left `None`: xdg_popup requires an xdg_surface parent.
                // Layer-shell parents are set later with `get_popup`.
                if parent.is_none()
                    && let SurfaceRole::Window(window) = &self.surfaces[owner_index].role
                {
                    Some(window.xdg_surface().clone())
                } else {
                    None
                }
            });

        let positioner = match XdgPositioner::new(xdg_shell) {
            Ok(positioner) => positioner,
            Err(error) => {
                tracing::error!(%error, "BarrierKit: could not create xdg_positioner");
                return;
            }
        };
        let (ax, ay, aw, ah) = spec.anchor_rect;
        positioner.set_anchor_rect(ax, ay, aw as i32, ah as i32);
        positioner.set_anchor(anchor_to_sctk(spec.anchor));
        positioner.set_gravity(gravity_to_sctk(spec.gravity));
        let (ox, oy) = spec.offset;
        positioner.set_offset(ox, oy);
        positioner.set_size(spec.size.0 as i32, spec.size.1 as i32);
        positioner.set_constraint_adjustment(constraint_to_sctk(spec.constraint_adjustment));

        let surface = match sctk::compositor::Surface::new(&self.compositor, &self.qh) {
            Ok(surface) => surface,
            Err(error) => {
                tracing::error!(%error, "BarrierKit: could not create popup surface");
                return;
            }
        };
        let wl_surface = surface.wl_surface().clone();
        let popup = match Popup::from_surface(
            parent_xdg_surface.as_ref(),
            &positioner,
            &self.qh,
            surface,
            xdg_shell,
        ) {
            Ok(popup) => popup,
            Err(error) => {
                tracing::error!(%error, "BarrierKit: could not create xdg_popup");
                return;
            }
        };

        // If the parent is a layer surface, use get_popup to set it as the
        // implicit parent (an xdg_popup cannot have a layer surface as its
        // direct xdg parent).
        if parent.is_none()
            && let SurfaceRole::Layer(layer_surface) = &self.surfaces[owner_index].role
        {
            layer_surface.get_popup(popup.xdg_popup());
            // `get_popup` takes effect when the parent layer surface commits.
            self.surfaces[owner_index].surface.commit();
        }

        if spec.grab
            && let Some(seat) = self.activation_seat.as_ref()
        {
            popup.xdg_popup().grab(seat, serial);
        }
        wl_surface.commit();

        let waker = futures_util::task::waker(Arc::new(PingWake(self.ping.clone())));
        let sender = make_sender(self.queue.clone(), self.ping.clone(), wl_surface.id());
        let opens_left = Rc::new(Cell::new(false));
        let popups = Popups {
            queue: self.popup_commands.clone(),
            ping: self.ping.clone(),
            next_id: self.popup_next_id.clone(),
            last_serial: self.last_input_serial.clone(),
            owner: wl_surface.id(),
            current: Some(id),
            surface_count: self.surface_count.clone(),
            opens_left: opens_left.clone(),
        };
        let handle = PopupHandle {
            id,
            popups: popups.clone(),
        };
        let app = AppInstance::new(
            content,
            scale,
            spec.size,
            sender,
            waker,
            Some(self.toplevels.clone()),
            None,
            self.activation.clone(),
            None,
            Timers {
                queue: self.timers.clone(),
                ping: self.ping.clone(),
                wake: self.wake.clone(),
            },
            popups,
            Some(handle),
            self.graphics.font_collection.clone(),
            self.graphics.font_manager.clone(),
            None,
            None,
            self.resolved,
        );

        self.surfaces.push(SurfaceWindow {
            surface: wl_surface,
            role: SurfaceRole::Popup(popup),
            popup: Some(PopupInfo {
                id,
                parent,
                on_dismissed: spec.on_dismissed,
                opens_left,
            }),
            shared: self.graphics.clone(),
            graphics: None,
            app,
            logical_size: spec.size,
            buffer_size: (0, 0),
            scale,
            output: self.surfaces[owner_index].output.clone(),
            needs_redraw: true,
            frame_pending: false,
            draw_count: 0,
            configure_count: 0,
            pending_configure: None,
            appmenu: None,
            appmenu_address: None,
            menu_handle: None,
        });
        self.surface_count.set(self.surfaces.len());
    }

    fn reposition_popup(&mut self, id: PopupId, spec: PopupSpec) {
        let Some(index) = self
            .surfaces
            .iter()
            .position(|s| s.popup.as_ref().is_some_and(|info| info.id == id))
        else {
            return;
        };
        let Some(xdg_shell) = self.xdg_shell.as_ref() else {
            return;
        };
        let popup = match &self.surfaces[index].role {
            SurfaceRole::Popup(popup) => popup,
            _ => return,
        };

        let positioner = match XdgPositioner::new(xdg_shell) {
            Ok(positioner) => positioner,
            Err(error) => {
                tracing::error!(%error, "BarrierKit: could not create xdg_positioner");
                return;
            }
        };
        let (ax, ay, aw, ah) = spec.anchor_rect;
        positioner.set_anchor_rect(ax, ay, aw as i32, ah as i32);
        positioner.set_anchor(anchor_to_sctk(spec.anchor));
        positioner.set_gravity(gravity_to_sctk(spec.gravity));
        let (ox, oy) = spec.offset;
        positioner.set_offset(ox, oy);
        positioner.set_size(spec.size.0 as i32, spec.size.1 as i32);
        positioner.set_constraint_adjustment(constraint_to_sctk(spec.constraint_adjustment));

        popup.reposition(&positioner, 0);
        self.surfaces[index].surface.commit();
    }

    fn close_popup(&mut self, id: PopupId) {
        // Close descendants first so Wayland objects are destroyed leaf-to-root.
        let children: Vec<PopupId> = self
            .surfaces
            .iter()
            .filter_map(|s| {
                s.popup
                    .as_ref()
                    .filter(|info| info.parent == Some(id))
                    .map(|info| info.id)
            })
            .collect();
        for child in children {
            self.close_popup(child);
        }

        let Some(index) = self
            .surfaces
            .iter()
            .position(|s| s.popup.as_ref().is_some_and(|info| info.id == id))
        else {
            return;
        };
        if let Some(info) = self.surfaces[index].popup.take()
            && let Some(callback) = info.on_dismissed
        {
            callback();
        }
        // Drop the xdg_popup before its wl_surface: the other order destroys
        // the surface first, so the compositor never sees `popup_destroyed`
        // and keeps the old popup in its scene.
        let mut window = self.surfaces.remove(index);
        drop(std::mem::replace(&mut window.role, SurfaceRole::None));
        drop(window);
        self.surface_count.set(self.surfaces.len());
    }

    fn poll_activation(&mut self) {
        let requests = std::mem::take(&mut *self.activation.queue.borrow_mut());
        for callback in requests {
            let (Some(global), Some(seat), Some(window)) = (
                self.activation_global.as_ref(),
                self.activation_seat.as_ref(),
                self.surfaces.first(),
            ) else {
                callback(None);
                continue;
            };
            let serial = self.last_input_serial.get();
            if serial == 0 {
                callback(None);
                continue;
            }
            let id = self.activation_callbacks.len();
            self.activation_callbacks.push(Some(callback));
            let token = global.get_activation_token(&self.qh, id);
            token.set_serial(serial, seat);
            token.set_surface(&window.surface);
            token.commit();
        }
    }

    fn sync_toplevels(&mut self) {
        let infos: Vec<ToplevelInfo> = self
            .toplevel_entries
            .borrow()
            .iter()
            .map(|entry| {
                let mut info = entry.info.clone();
                info.restorable = entry.restorable;
                info
            })
            .collect();
        let mut list = self.toplevels.list;
        if *list.peek() != infos {
            list.set(infos);
            self.request_redraw_all = true;
            self.ping.ping();
        }
    }

    fn install_timers(&mut self, handle: &LoopHandle<Shell>) {
        let pending: Vec<(Duration, Rc<dyn Fn()>)> = std::mem::take(&mut *self.timers.borrow_mut());
        for (delay, callback) in pending {
            let _ = handle.insert_source(
                Timer::from_duration(delay),
                move |_, _, shell: &mut Shell| {
                    callback();
                    shell.poll_all();
                    shell.redraw_all();
                    TimeoutAction::Drop
                },
            );
        }
    }

    fn redraw_all(&mut self) {
        let qh = self.qh.clone();
        for window in &mut self.surfaces {
            if window.needs_redraw {
                // With the compositor now scheduling a repaint for any surface
                // commit that carries damage or asks for a frame callback, we
                // can go back to drawing on the frame callback instead of
                // committing immediately on every user event.
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
        if window.scale != scale {
            window.scale = scale;
            if window.logical_size.0 > 0 {
                let size = window.logical_size;
                window.configure(size, scale);
            }
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
        // Collapse any window configures that arrived since the last frame.
        self.apply_pending_window_configures(qh);
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

impl WindowHandler for Shell {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        let surface_id = window.wl_surface().id();
        let window_count = self
            .surfaces
            .iter()
            .filter(|s| matches!(s.role, SurfaceRole::Window(_)))
            .count();
        if window_count <= 1 {
            self.needs_exit = true;
        }
        if let Some(context) = self
            .window_contexts
            .iter()
            .find(|ctx| ctx.controls.surface_id == surface_id)
        {
            let mut status = context.controls.status;
            status.set(WindowStatus {
                activated: false,
                ..*status.read()
            });
        }
        if !self.needs_exit {
            // Drop the closed surface and its controls so later commands are
            // ignored. The window_contexts entry is removed as well.
            if let Some(index) = self.surfaces.iter().position(|s| {
                matches!(s.role, SurfaceRole::Window(_)) && s.surface.id() == surface_id
            }) {
                if let Some(address) = self.surfaces[index].appmenu_address.take() {
                    crate::app_menu::server::unregister(&address.path);
                }
                if let Some(appmenu) = self.surfaces[index].appmenu.take() {
                    appmenu.release();
                }
                let mut role = SurfaceRole::None;
                std::mem::swap(&mut self.surfaces[index].role, &mut role);
                drop(role);
                self.surfaces.remove(index);
                self.window_contexts
                    .retain(|ctx| ctx.controls.surface_id != surface_id);
                self.surface_count.set(self.surfaces.len());
            }
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        serial: u32,
    ) {
        let Some(index) = self.surface_for(window.wl_surface()) else {
            return;
        };
        // Drop any older pending configure: only the last one is drawn.  The
        // request_redraw ensures a frame callback will process it.
        self.surfaces[index].pending_configure = Some((configure, serial));
        self.surfaces[index].needs_redraw = true;
        self.surfaces[index].request_frame_callback(qh);
    }
}

impl Shell {
    /// Applies the most recent pending window configure for each surface.
    ///
    /// Called from the frame callback so several configures that arrive between
    /// frames collapse into a single resize/draw.
    fn apply_pending_window_configures(&mut self, qh: &QueueHandle<Self>) {
        for index in 0..self.surfaces.len() {
            let Some((configure, _serial)) = self.surfaces[index].pending_configure.take() else {
                continue;
            };
            let scale = self.surfaces[index].scale;
            let size = (
                configure.new_size.0.map_or(0, |value| value.get()),
                configure.new_size.1.map_or(0, |value| value.get()),
            );
            // A 0×0 configure leaves the size to the client: keep the size the app
            // asked for. Dropping the configure would leave the surface without a
            // buffer, so the window would never map.
            let size = if size.0 > 0 && size.1 > 0 {
                size
            } else {
                self.surfaces[index].logical_size
            };
            // Only commit immediately when the size did not change. A size-changing
            // configure will be followed by a redraw that commits a buffer at the
            // new size; committing the old buffer here makes the compositor see a
            // transient size mismatch and loop (B2).
            let size_changed = size != self.surfaces[index].logical_size;
            self.surfaces[index].configure_count += 1;
            let count = self.surfaces[index].configure_count;
            if count.is_multiple_of(30) {
                tracing::info!(
                    surface = ?self.surfaces[index].surface.id(),
                    configures = count,
                    egl_windows = self.surfaces[index].shared.egl_window_creations.get(),
                    "BarrierKit: resize configuration count"
                );
            }
            self.surfaces[index].configure(size, scale);
            let surface_id = self.surfaces[index].surface.id();
            let requested_server = self
                .window_contexts
                .iter()
                .find(|ctx| ctx.controls.surface_id == surface_id)
                .map(|ctx| ctx.controls.status.read().decoration == DecorationResponse::Server)
                .unwrap_or(false);
            // MalaysiaDreams always grants an explicit Server request. Some compositors do
            // not send the decoration configure at all, so keep the requested mode
            // when they advertise the protocol; without it, fall back to the
            // client-side bar.
            let decoration = if requested_server && self.decoration_manager {
                DecorationResponse::Server
            } else {
                match configure.decoration_mode {
                    DecorationMode::Server => DecorationResponse::Server,
                    DecorationMode::Client => DecorationResponse::Client,
                }
            };
            if let Some(context) = self
                .window_contexts
                .iter()
                .find(|ctx| ctx.controls.surface_id == surface_id)
            {
                if requested_server
                    && decoration == DecorationResponse::Client
                    && !self.decoration_warned
                {
                    self.decoration_warned = true;
                    tracing::info!(
                        "the compositor did not grant server-side decorations; drawing the kit titlebar"
                    );
                }
                let mut status = context.controls.status;
                status.set(WindowStatus {
                    activated: configure.is_activated(),
                    maximized: configure.is_maximized(),
                    decoration,
                    fixed: context.controls.fixed,
                });
            }
            // Acknowledge the configure and let SCTK apply the decoration mode
            // before drawing the first frame. A size change is committed by the
            // redraw below, not here, to avoid sending the old buffer size.
            if !size_changed && let SurfaceRole::Window(window) = &self.surfaces[index].role {
                window.commit();
            }
            // A toplevel must attach its first buffer before it can map; unlike a
            // layer surface there is no frame callback to wait for yet.
            if self.surfaces[index].draw() && self.surfaces[index].needs_redraw {
                self.surfaces[index].request_redraw(qh);
            }
        }
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
        eprintln!("malaysia-shell lock: the compositor refused the session lock");
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

impl PopupHandler for Shell {
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        let surface = popup.wl_surface();
        let Some(index) = self.surface_for(surface) else {
            return;
        };
        let scale = self.surfaces[index].scale;
        let size = (config.width.max(0) as u32, config.height.max(0) as u32);
        if let Some(info) = self.surfaces[index].popup.as_ref() {
            info.opens_left.set(config.position.0 < 0);
        }
        self.surfaces[index].configure(size, scale);
        // Like a toplevel, a popup must attach its first buffer before it can
        // map; there is no frame callback to wait for yet.
        if self.surfaces[index].draw() && self.surfaces[index].needs_redraw {
            self.surfaces[index].request_redraw(qh);
        }
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        let surface = popup.wl_surface();
        if let Some(index) = self.surface_for(surface)
            && let Some(info) = self.surfaces[index].popup.as_ref()
        {
            let id = info.id;
            self.close_popup(id);
        }
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
        self.activation_seat = Some(seat.clone());
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
                    if let Some(manager) = &self.cursor_shape {
                        self.cursor_shape_device = Some(manager.get_shape_device(&pointer, qh));
                    }
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
            Capability::Pointer => {
                self.pointer = None;
                self.cursor_shape_device = None;
            }
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
            if let sctk::seat::pointer::PointerEventKind::Enter { serial } = event.kind {
                self.last_pointer_serial.set(serial);
                if let Some(device) = &self.cursor_shape_device {
                    device.set_shape(serial, CursorShape::Default);
                }
            }
            let position = physical_point(event.position, self.surfaces[index].scale);
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
                PointerEventKind::Press { button, serial, .. } => {
                    self.last_input_serial.set(serial);
                    let surface_id = self.surfaces[index].surface.id();
                    if let Some(context) = self
                        .window_contexts
                        .iter()
                        .find(|ctx| ctx.controls.surface_id == surface_id)
                    {
                        context.controls.pointer_serial.set(serial);
                    }
                    PlatformEvent::Mouse {
                        name: MouseEventName::MouseDown,
                        cursor: position,
                        button: Some(input::mouse_button(button)),
                    }
                }
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
        surface: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.keyboard_focus = Some(surface.clone());
        if let Some(index) = self.surface_for(surface) {
            self.surfaces[index].app.set_focused(true);
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &WlSurface,
        _: u32,
    ) {
        if self.keyboard_focus.as_ref() == Some(surface) {
            self.keyboard_focus = None;
            if let Some(index) = self.surface_for(surface) {
                self.surfaces[index].app.set_focused(false);
            }
        }
        stop_repeat(&mut self.repeat, &mut self.repeat_generation);
    }

    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.last_key_serial = serial;
        self.last_input_serial.set(serial);
        self.key_event(qh, event.clone(), KeyboardEventName::KeyDown);
        // Modifier keys and keys that xkb marks non-repeating should not
        // produce repeated KeyDown events. SCTK 0.19.2 can repeat keys itself
        // via SeatState::get_keyboard_with_repeat, but that requires a loop
        // handle at keyboard creation time and a callback that knows the
        // focused surface; keeping our own timer is the smaller change.
        // shortcut: keys other than modifiers that xkb marks non-repeating
        // are still repeated because KeyboardHandler does not expose that bit.
        if !event.keysym.is_modifier_key() {
            let (delay, rate) = self.repeat_timing();
            self.repeat_generation = self.repeat_generation.wrapping_add(1);
            self.repeat = Some(RepeatState {
                event,
                delay,
                rate,
                first: true,
                pending: false,
                generation: self.repeat_generation,
            });
        }
        self.ping.ping();
    }

    fn release_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key_event(qh, event.clone(), KeyboardEventName::KeyUp);
        // Only stop repeating if the released key is the one currently being
        // repeated. This keeps B repeating after A is released when A and B
        // were both pressed.
        if self
            .repeat
            .as_ref()
            .is_some_and(|state| state.event.keysym == event.keysym)
        {
            stop_repeat(&mut self.repeat, &mut self.repeat_generation);
        }
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
        info: RepeatInfo,
    ) {
        self.repeat_info = Some(info);
    }
}

impl Shell {
    fn key_event(&mut self, qh: &QueueHandle<Self>, event: KeyEvent, name: KeyboardEventName) {
        let (key, code) = input::key(event.keysym, event.utf8.clone());
        let modifiers = input::modifiers(self.modifiers);

        let index = self
            .keyboard_focus
            .as_ref()
            .and_then(|surface| self.surface_for(surface))
            .or_else(|| (!self.surfaces.is_empty()).then_some(0));

        let shortcut_action = if name == KeyboardEventName::KeyDown {
            index.and_then(|idx| {
                let surface = &self.surfaces[idx];
                let menu = surface
                    .menu_handle
                    .as_ref()
                    .map(|h| h.snapshot().0)
                    .or_else(|| surface.app.menu.clone())?;
                crate::app_menu::find_menu_shortcut(&menu, modifiers, &key)
                    .map(|action_id| (idx, action_id))
            })
        } else {
            None
        };

        if let Some((idx, ref action_id)) = shortcut_action
            && let Some(on_action) = self.surfaces[idx].app.on_menu_action.as_ref()
        {
            on_action(action_id);
        }

        let platform_event = PlatformEvent::Keyboard {
            name,
            key,
            code,
            modifiers,
        };
        if let Some(idx) = index {
            self.surfaces[idx].app.push(platform_event);
            self.surfaces[idx].request_redraw(qh);
            if shortcut_action.is_some() {
                self.poll_all();
                self.redraw_all();
            }
        }
    }

    fn repeat_timing(&self) -> (Duration, Duration) {
        match self.repeat_info {
            Some(RepeatInfo::Repeat { rate, delay }) => (
                Duration::from_millis(u64::from(delay)),
                Duration::from_micros(1_000_000 / u64::from(rate.get())),
            ),
            _ => (Duration::from_millis(200), Duration::from_millis(40)),
        }
    }

    fn install_repeat(&mut self, handle: &LoopHandle<Shell>) {
        let Some(state) = &mut self.repeat else {
            return;
        };
        if state.pending || self.repeat_info.is_none() {
            return;
        }
        state.pending = true;
        let delay = if state.first { state.delay } else { state.rate };
        state.first = false;
        let generation = state.generation;
        let _ = handle.insert_source(
            Timer::from_duration(delay),
            move |_, _, shell: &mut Shell| {
                if shell
                    .repeat
                    .as_ref()
                    .is_some_and(|state| state.generation == generation)
                {
                    let event = shell.repeat.as_ref().map(|state| state.event.clone());
                    if let Some(event) = event {
                        let qh = shell.qh.clone();
                        shell.key_event(&qh, event, KeyboardEventName::KeyDown);
                    }
                    if let Some(state) = &mut shell.repeat {
                        state.pending = false;
                    }
                    shell.ping.ping();
                }
                TimeoutAction::Drop
            },
        );
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
        self.touch_surfaces.insert(id, surface.clone());
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
        let Some(surface) = self.touch_surfaces.remove(&id) else {
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
        let Some(surface) = self.touch_surfaces.get(&id).cloned() else {
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

    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch) {
        self.touch_surfaces.clear();
    }
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
            location: physical_point(position, self.surfaces[index].scale),
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
        if let FtlListEvent::Toplevel { toplevel } = event
            && state._foreign_mgmt.is_none()
        {
            state.toplevel_entries.borrow_mut().push(ToplevelEntry {
                list: Some(toplevel),
                mgmt: None,
                restorable: false,
                info: ToplevelInfo::default(),
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
        if state._foreign_mgmt.is_some() {
            return;
        }
        if matches!(event, FtlHandleEvent::Closed) {
            state.toplevel_entries.borrow_mut().retain(|entry| {
                entry
                    .list
                    .as_ref()
                    .is_none_or(|list| list.id() != handle.id())
            });
            state.sync_toplevels();
            return;
        }
        let mut entries = state.toplevel_entries.borrow_mut();
        let index = entries
            .iter()
            .position(|e| e.list.as_ref().is_some_and(|list| list.id() == handle.id()));
        if let Some(index) = index {
            let mut infos: Vec<ToplevelInfo> = entries.iter().map(|e| e.info.clone()).collect();
            let info_event = match event {
                FtlHandleEvent::Title { title } => Some(InfoEvent::Title(title)),
                FtlHandleEvent::AppId { app_id } => Some(InfoEvent::AppId(app_id)),
                _ => None,
            };
            if let Some(info_event) = info_event {
                apply_info_event(&mut infos, index, info_event);
                entries[index].info = infos[index].clone();
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
        if let MgmtListEvent::Toplevel { toplevel } = event {
            state.toplevel_entries.borrow_mut().push(ToplevelEntry {
                list: None,
                mgmt: Some(toplevel),
                restorable: false,
                info: ToplevelInfo::default(),
            });
            state.sync_toplevels();
        }
    }

    event_created_child!(Shell, ZwlrForeignToplevelManagerV1, [
        sctk::reexports::protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, (), Shell> for Shell {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut entries = state.toplevel_entries.borrow_mut();
        let index = entries
            .iter()
            .position(|e| e.mgmt.as_ref().is_some_and(|mgmt| mgmt.id() == handle.id()));
        let Some(index) = index else {
            return;
        };
        if matches!(event, ZwlrForeignToplevelHandleV1Event::Closed) {
            let mut infos: Vec<ToplevelInfo> = entries.iter().map(|e| e.info.clone()).collect();
            apply_info_event(&mut infos, index, InfoEvent::Closed);
            entries.remove(index);
            drop(entries);
            state.sync_toplevels();
            return;
        }
        let info_event = match event {
            ZwlrForeignToplevelHandleV1Event::Title { title } => Some(InfoEvent::Title(title)),
            ZwlrForeignToplevelHandleV1Event::AppId { app_id } => Some(InfoEvent::AppId(app_id)),
            ZwlrForeignToplevelHandleV1Event::State { state } => Some(InfoEvent::State {
                active: state.contains(&(ZwlrForeignToplevelHandleV1State::Activated as u8)),
                maximized: state.contains(&(ZwlrForeignToplevelHandleV1State::Maximized as u8)),
                minimized: state.contains(&(ZwlrForeignToplevelHandleV1State::Minimized as u8)),
            }),
            _ => None,
        };
        if let Some(info_event) = info_event {
            let mut infos: Vec<ToplevelInfo> = entries.iter().map(|e| e.info.clone()).collect();
            apply_info_event(&mut infos, index, info_event);
            entries[index].info = infos[index].clone();
        }
        drop(entries);
        state.sync_toplevels();
    }
}

fn physical_point(position: (f64, f64), scale: f64) -> CursorPoint {
    CursorPoint::from((position.0 * scale, position.1 * scale))
}

impl Dispatch<XdgActivationV1, ()> for Shell {
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

impl Dispatch<XdgActivationTokenV1, usize> for Shell {
    fn event(
        state: &mut Self,
        _: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event
            && let Some(callback) = state
                .activation_callbacks
                .get_mut(*id)
                .and_then(Option::take)
        {
            callback(Some(token));
        }
    }
}

impl Dispatch<InfinityWindowManagerV1, ()> for Shell {
    fn event(
        state: &mut Self,
        _: &InfinityWindowManagerV1,
        event: <InfinityWindowManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use infinity::protocol::infinity_window_manager_v1::Event;
        match event {
            Event::Restorable { handle, restorable } => {
                let restorable = restorable != 0;
                for entry in state.toplevel_entries.borrow_mut().iter_mut() {
                    if entry
                        .mgmt
                        .as_ref()
                        .is_some_and(|mgmt| mgmt.id() == handle.id())
                    {
                        entry.restorable = restorable;
                        break;
                    }
                }
            }
            Event::AppMenu {
                toplevel,
                service,
                path,
            } => {
                let address = if service.is_empty() || path.is_empty() {
                    None
                } else {
                    Some(AppMenuAddress { service, path })
                };
                update_toplevel_info(
                    &mut state.toplevel_entries.borrow_mut(),
                    &toplevel,
                    InfoEvent::AppMenu(address),
                );
            }
            Event::GtkMenus {
                toplevel,
                unique_bus_name,
                application_id,
                menubar_path,
                app_menu_path,
                application_object_path,
                window_object_path,
            } => {
                let address = GtkMenusAddress::new(
                    unique_bus_name,
                    application_id,
                    menubar_path,
                    app_menu_path,
                    application_object_path,
                    window_object_path,
                );
                update_toplevel_info(
                    &mut state.toplevel_entries.borrow_mut(),
                    &toplevel,
                    InfoEvent::GtkMenus(address),
                );
            }
        }
        state.sync_toplevels();
    }
}

fn update_toplevel_info(
    entries: &mut [ToplevelEntry],
    toplevel: &ZwlrForeignToplevelHandleV1,
    event: InfoEvent,
) {
    if let Some(index) = entries.iter().position(|entry| {
        entry
            .mgmt
            .as_ref()
            .is_some_and(|mgmt| mgmt.id() == toplevel.id())
    }) {
        let mut infos: Vec<ToplevelInfo> = entries.iter().map(|entry| entry.info.clone()).collect();
        apply_info_event(&mut infos, index, event);
        entries[index].info = infos[index].clone();
    }
}

impl Dispatch<OrgKdeKwinAppmenuManager, (), Shell> for Shell {
    fn event(
        _: &mut Shell,
        _: &OrgKdeKwinAppmenuManager,
        _: org_kde_kwin_appmenu_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Shell>,
    ) {
    }
}

impl Dispatch<OrgKdeKwinAppmenu, ObjectId, Shell> for Shell {
    fn event(
        _: &mut Shell,
        _: &OrgKdeKwinAppmenu,
        _: org_kde_kwin_appmenu::Event,
        _: &ObjectId,
        _: &Connection,
        _: &QueueHandle<Shell>,
    ) {
    }
}

delegate_compositor!(Shell);
delegate_output!(Shell);
delegate_registry!(Shell);
delegate_seat!(Shell);
delegate_pointer!(Shell);
delegate_xdg_shell!(Shell);
delegate_xdg_window!(Shell);
delegate_keyboard!(Shell);
delegate_touch!(Shell);
delegate_session_lock!(Shell);
delegate_xdg_popup!(Shell);
delegate_dispatch!(Shell: [zwlr_layer_shell_v1::ZwlrLayerShellV1: sctk::globals::GlobalData] => LayerShell);
delegate_dispatch!(Shell: [zwlr_layer_surface_v1::ZwlrLayerSurfaceV1: LayerSurfaceData] => LayerShell);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn wakeup_runs_its_callback_on_the_loop_thread() {
        let (ping, source) = calloop::ping::make_ping().unwrap();
        let callbacks: WakeQueue = Rc::new(RefCell::new(Vec::new()));
        let mut event_loop = EventLoop::<()>::try_new().unwrap();
        let queue = callbacks.clone();
        event_loop
            .handle()
            .insert_source(source, move |_, _, ()| {
                for callback in queue.borrow().clone() {
                    callback();
                }
            })
            .unwrap();
        let timers = Timers {
            queue: Rc::new(RefCell::new(Vec::new())),
            ping: ping.clone(),
            wake: Wake { ping, callbacks },
        };
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = seen.clone();
        let handle = timers.wakeup(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let thread_handle = handle.clone();
        let writer = std::thread::spawn(move || thread_handle.wake());
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while seen.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            event_loop
                .dispatch(Duration::from_millis(10), &mut ())
                .unwrap();
        }
        writer.join().unwrap();
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn toplevel_size_dimension_clamps_to_unbounded() {
        assert_eq!(clamp_toplevel_size_dimension(u32::MAX), 0);
        assert_eq!(clamp_toplevel_size_dimension(i32::MAX as u32 + 1), 0);
        assert_eq!(
            clamp_toplevel_size_dimension(i32::MAX as u32),
            i32::MAX as u32
        );
        assert_eq!(clamp_toplevel_size_dimension(760), 760);
        assert_eq!(clamp_toplevel_size_dimension(0), 0);
    }

    #[test]
    fn pointer_and_touch_coordinates_become_physical() {
        assert_eq!(
            physical_point((10.0, 20.0), 1.5),
            CursorPoint::from((15.0, 30.0))
        );
        assert_eq!(
            physical_point((3.0, 4.0), 2.0),
            CursorPoint::from((6.0, 8.0))
        );
    }

    #[test]
    fn losing_focus_cancels_repeat_and_invalidates_old_timers() {
        let event = KeyEvent {
            time: 0,
            raw_code: 0,
            keysym: Keysym::new(0),
            utf8: None,
        };
        let mut repeat = Some(RepeatState {
            event,
            delay: Duration::from_millis(200),
            rate: Duration::from_millis(40),
            first: false,
            pending: true,
            generation: 7,
        });
        let mut generation = 7;
        stop_repeat(&mut repeat, &mut generation);
        assert!(repeat.is_none());
        assert_eq!(generation, 8);
    }

    #[test]
    fn interleaved_info_events_update_their_own_toplevel() {
        let mut infos = vec![ToplevelInfo::default(), ToplevelInfo::default()];
        apply_info_event(&mut infos, 0, InfoEvent::Title("Editor".into()));
        apply_info_event(&mut infos, 1, InfoEvent::Title("Navegador".into()));
        apply_info_event(&mut infos, 0, InfoEvent::AppId("editor".into()));
        apply_info_event(
            &mut infos,
            1,
            InfoEvent::State {
                active: true,
                maximized: true,
                minimized: false,
            },
        );
        apply_info_event(
            &mut infos,
            0,
            InfoEvent::State {
                active: false,
                maximized: false,
                minimized: true,
            },
        );

        assert_eq!(infos[0].title, "Editor");
        assert_eq!(infos[0].app_id, "editor");
        assert!(!infos[0].active);
        assert!(!infos[0].maximized);
        assert!(infos[0].minimized);
        assert_eq!(infos[1].title, "Navegador");
        assert!(infos[1].active);
        assert!(infos[1].maximized);
        assert!(!infos[1].minimized);

        apply_info_event(&mut infos, 0, InfoEvent::Closed);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].title, "Navegador");
    }

    #[test]
    fn app_menu_event_updates_only_its_toplevel() {
        let mut infos = vec![ToplevelInfo::default(), ToplevelInfo::default()];
        apply_info_event(
            &mut infos,
            0,
            InfoEvent::AppMenu(Some(AppMenuAddress {
                service: ":1.2".to_string(),
                path: "/MenuBar/1".to_string(),
            })),
        );
        assert_eq!(
            infos[0].app_menu.as_ref().map(|a| a.service.as_str()),
            Some(":1.2")
        );
        assert!(infos[1].app_menu.is_none());

        apply_info_event(&mut infos, 0, InfoEvent::AppMenu(None));
        assert!(infos[0].app_menu.is_none());
    }

    #[test]
    fn gtk_menus_event_updates_only_its_toplevel() {
        let mut infos = vec![ToplevelInfo::default(), ToplevelInfo::default()];
        apply_info_event(
            &mut infos,
            0,
            InfoEvent::GtkMenus(GtkMenusAddress::new(
                ":1.3".to_string(),
                "org.gtk.Demo".to_string(),
                "/menubar".to_string(),
                "/appmenu".to_string(),
                "/application".to_string(),
                "/window".to_string(),
            )),
        );
        assert_eq!(
            infos[0]
                .gtk_menus
                .as_ref()
                .map(|a| a.unique_bus_name.as_str()),
            Some(":1.3")
        );
        assert!(infos[1].gtk_menus.is_none());

        apply_info_event(&mut infos, 0, InfoEvent::GtkMenus(None));
        assert!(infos[0].gtk_menus.is_none());
    }

    #[test]
    fn gtk_menus_address_needs_a_bus_name() {
        assert!(
            GtkMenusAddress::new(
                "".to_string(),
                "org.gtk.Demo".to_string(),
                "/menubar".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
            )
            .is_none()
        );
    }

    #[test]
    fn gtk_menus_address_accepts_app_menu_only() {
        let address = GtkMenusAddress::new(
            ":1.2".to_string(),
            "org.gtk.Demo".to_string(),
            "".to_string(),
            "/appmenu".to_string(),
            "".to_string(),
            "".to_string(),
        );
        assert!(address.is_some());
        assert_eq!(address.unwrap().app_menu_path, "/appmenu");
    }

    #[test]
    fn gtk_menus_address_accepts_menubar_only() {
        let address = GtkMenusAddress::new(
            ":1.2".to_string(),
            "org.gtk.Demo".to_string(),
            "/menubar".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
        );
        assert!(address.is_some());
        assert_eq!(address.unwrap().menubar_path, "/menubar");
    }

    #[test]
    fn gtk_menus_address_rejects_both_paths_empty() {
        assert!(
            GtkMenusAddress::new(
                ":1.2".to_string(),
                "org.gtk.Demo".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
            )
            .is_none()
        );
    }

    #[test]
    fn sender_tags_user_events_with_surface_id() {
        let (ping, _) = calloop::ping::make_ping().unwrap();
        let queue: Rc<RefCell<Vec<(ObjectId, UserEvent)>>> = Rc::new(RefCell::new(Vec::new()));
        let owner = ObjectId::null();
        let sender = make_sender(queue.clone(), ping, owner.clone());
        sender(UserEvent::RequestRedraw);
        let events = drain_user_events(&queue);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, owner);
    }

    #[test]
    fn cursor_icon_maps_to_wayland_shape() {
        assert_eq!(
            cursor_shape_from_icon(CursorIcon::Default),
            CursorShape::Default
        );
        assert_eq!(cursor_shape_from_icon(CursorIcon::Text), CursorShape::Text);
        assert_eq!(
            cursor_shape_from_icon(CursorIcon::Pointer),
            CursorShape::Pointer
        );
        assert_eq!(cursor_shape_from_icon(CursorIcon::Grab), CursorShape::Grab);
        assert_eq!(
            cursor_shape_from_icon(CursorIcon::Grabbing),
            CursorShape::Grabbing
        );
        assert_eq!(
            cursor_shape_from_icon(CursorIcon::NotAllowed),
            CursorShape::NotAllowed
        );
        assert_eq!(
            cursor_shape_from_icon(CursorIcon::NResize),
            CursorShape::NResize
        );
        assert_eq!(
            cursor_shape_from_icon(CursorIcon::EResize),
            CursorShape::EResize
        );
        assert_eq!(
            cursor_shape_from_icon(CursorIcon::EwResize),
            CursorShape::EwResize
        );
        assert_eq!(
            cursor_shape_from_icon(CursorIcon::NsResize),
            CursorShape::NsResize
        );
    }
}
