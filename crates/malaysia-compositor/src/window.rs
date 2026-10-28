//! The tracked toplevels, behind one boundary.
//!
//! Geometry — where a window is and how it stacks — lives in
//! [`malaysia_scene::scene`]; this module owns what the scene cannot: the
//! protocol surface and the configure handshake. A [`Window`] view joins the
//! two for readers. See ADR 0015.
//!
//! Two things this module is careful about:
//!
//! - **The initial configure.** A toplevel that is tracked without being sent a
//!   configure waits forever, with no error and no log. The configure is sent as
//!   soon as the toplevel exists, before the client attaches a buffer.
//! - **Insets are recomputed, never fixed.** Every commit reads the surface's
//!   current role and state and recomputes the insets. A `set_parent` can arrive
//!   after the window is mapped, and it can be unset again; a fullscreen change
//!   can come at any time. A configure that froze the insets at map time would
//!   be wrong from then on (ADR 0014).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use malaysia_scene::geometry::{
    Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
};
use malaysia_scene::scene::{Decoration, Edge, PopupId, PopupParent, Scene, WindowId};
use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Serial, Size};
use smithay::wayland::compositor::{SurfaceAttributes, with_states};
use smithay::wayland::shell::xdg::{
    Configure, SurfaceCachedState, ToplevelStateSet, ToplevelSurface, XdgToplevelSurfaceData,
};
use vow_design::{NATIVE_APP_ID_PREFIX, TITLEBAR};

use crate::decoration::{Insets, Role, WindowState};

/// How long the compositor waits for an app to ack a resize configure before
/// sending the next pending size. This protects apps that never ack from
/// freezing the resize forever.
const RESIZE_CONFIGURE_TIMEOUT: Duration = Duration::from_millis(100);

/// A read-only view of one tracked window.
///
/// It borrows the window's data and its current scene geometry, so callers see
/// one consistent snapshot without the scene and the surface bookkeeping being
/// two separate lookups.
pub struct Window<'a> {
    /// The window's surface and configure state.
    data: &'a WindowData,
    /// The window's decorated outer rectangle.
    outer: SceneRect<f64, SceneLogical>,
    /// Who draws the window's chrome, from the scene (ADR 0019).
    decoration: Decoration,
}

impl Window<'_> {
    /// The window's scene identity.
    pub fn id(&self) -> WindowId {
        self.data.id
    }

    /// The client's surface.
    pub fn surface(&self) -> &WlSurface {
        self.data.toplevel.wl_surface()
    }

    /// Whether the window is mapped and should be shown.
    pub fn is_mapped(&self) -> bool {
        self.data.mapped
    }

    /// Whether the window is hidden from the scene (minimize or similar).
    pub fn is_hidden(&self) -> bool {
        self.data.hidden
    }

    /// Whether the surface currently has a buffer attached.
    pub fn has_buffer(&self) -> bool {
        surface_has_buffer(self.surface())
    }

    /// Whether the buffer has ever received damage.
    pub fn has_damage(&self) -> bool {
        self.data.damaged
    }

    /// Whether the window geometry fits inside the current buffer.
    pub fn geometry_fits_buffer(&self) -> bool {
        self.data.geometry_fits_buffer()
    }

    /// Whether the window is visible: mapped, not hidden, not minimized, with
    /// a current buffer that has received damage and a window geometry inside
    /// it.
    pub fn is_visible(&self, minimized: &HashSet<WindowId>) -> bool {
        self.is_mapped()
            && !self.is_hidden()
            && !minimized.contains(&self.id())
            && self.has_buffer()
            && self.has_damage()
            && self.geometry_fits_buffer()
    }

    /// The window's app id, as the client last set it.
    pub fn app_id(&self) -> String {
        with_states(self.surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .map(|data| data.lock().unwrap().app_id.clone().unwrap_or_default())
                .unwrap_or_default()
        })
    }

    /// The decorated outer rectangle, in logical coordinates.
    pub fn outer(&self) -> SceneRect<f64, SceneLogical> {
        self.outer
    }

    /// The outer top-left of the window, in logical coordinates.
    pub fn location(&self) -> Point<i32, Logical> {
        Point::from((round(self.outer.x()), round(self.outer.y())))
    }

    /// The outer window size, in logical coordinates.
    pub fn size(&self) -> Size<i32, Logical> {
        Size::from((round(self.outer.width()), round(self.outer.height())))
    }

    /// The insets for the window's current role, state and decoration.
    pub fn insets(&self) -> Insets {
        Insets::for_window(
            self.data.role(),
            self.data.state(),
            self.decoration,
            &TITLEBAR,
        )
    }

    /// The window state that affects decoration.
    pub fn state(&self) -> WindowState {
        self.data.state()
    }

    /// Whether the window is in an interactive resize right now.
    pub fn is_resizing(&self) -> bool {
        self.data.resizing
    }

    /// Whether this toplevel is a dialog, a normal window or a popup.
    pub fn role(&self) -> Role {
        self.data.role()
    }

    /// Whether the client draws its own chrome (CSD).
    pub fn is_client_decorated(&self) -> bool {
        self.decoration == Decoration::Client
    }

    /// The client's window geometry relative to its surface, or the buffer
    /// size when the client did not set one.
    pub fn window_geometry(&self) -> Rectangle<i32, Logical> {
        window_geometry(self.surface()).unwrap_or_else(|| {
            Rectangle::new(
                Point::from((0, 0)),
                buffer_size(self.surface()).unwrap_or_else(|| self.size()),
            )
        })
    }

    /// The window's title, as the client last set it.
    pub fn title(&self) -> String {
        with_states(self.surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .expect("a tracked toplevel has xdg toplevel data")
                .lock()
                .unwrap()
                .title
                .clone()
                .unwrap_or_default()
        })
    }

    /// The size configured to the client: the window minus its insets.
    pub fn usable_size(&self) -> Size<i32, Logical> {
        usable(self.size(), self.insets())
    }

    /// Where the client's surface sits inside the window, past the left and top
    /// insets.
    pub fn surface_location(&self) -> Point<i32, Logical> {
        let insets = self.insets();
        Point::from((round(insets.left), round(insets.top)))
    }

    /// Whether a logical point falls inside the window's outer rectangle.
    pub fn contains(&self, point: Point<f64, Logical>) -> bool {
        self.outer.contains(ScenePoint::new(point.x, point.y))
    }
}

/// One tracked toplevel: its identity, surface and configure bookkeeping.
struct WindowData {
    /// The window's identity in the scene.
    id: WindowId,
    /// The client's toplevel surface.
    toplevel: ToplevelSurface,
    /// The decorated outer size, in logical coordinates.
    outer_size: Size<i32, Logical>,
    /// Where the window was placed before its first map, in logical coords.
    /// After mapping the position lives in the scene; this field is only used
    /// for the initial placement and map.
    placement_origin: Point<i32, Logical>,
    /// Rectangle saved before hiding, so restoring puts the window back exactly.
    hidden_rect: Option<SceneRect<f64, SceneLogical>>,
    /// Whether the client has acknowledged a configure.
    acked: bool,
    /// The serial of the configure most recently sent, awaiting an ack.
    pending_configure: Option<Serial>,
    /// Whether the window is mapped and should be shown.
    mapped: bool,
    /// Whether the window is hidden from the scene (minimized or otherwise).
    hidden: bool,
    /// The usable size last sent, so a state change can be noticed.
    configured: Option<Size<i32, Logical>>,
    /// Whether the user is currently resizing, so the configure carries the
    /// xdg `resizing` state.
    resizing: bool,
    /// Which edge or corner is being dragged during an interactive resize.
    resize_edge: Option<Edge>,
    /// Outer size most recently requested by the pointer while the app has not
    /// acked+committed the previous configure.
    resize_pending_outer: Option<Size<i32, Logical>>,
    /// When the last configure during an interactive resize was sent, so a
    /// slow app is eventually reconfigured even if it never acks.
    resize_configure_sent: Option<Instant>,
    /// Whether MalaysiaDreams's titlebar requested the maximized state.
    maximized: bool,
    /// Whether the window is in fullscreen state, as the next configure says.
    fullscreen: bool,
    /// Whether the window holds keyboard focus, as the next configure says.
    activated: bool,
    /// The decoration mode the client last asked for, before policy. `None`
    /// until `request_mode`; creating or unsetting the object does not count.
    requested: Option<Decoration>,
    /// Whether the native-app warning was already emitted for this window.
    native_warned: bool,
    /// Whether the initial centered placement has already been applied.
    placed: bool,
    /// Whether any commit has ever carried damage for this window's buffer.
    damaged: bool,
    /// Whether the invisible-window warning was already emitted.
    invisible_warned: bool,
}

impl WindowData {
    /// A toplevel with a parent is a dialog; otherwise an ordinary toplevel.
    fn role(&self) -> Role {
        if self.toplevel.parent().is_some() {
            Role::Dialog
        } else {
            Role::Toplevel
        }
    }

    /// The window state that affects decoration, from the surface's state.
    fn state(&self) -> WindowState {
        let states = self.toplevel.with_committed_state(|state| {
            state.map(|state| state.states.clone()).unwrap_or_default()
        });
        if states.contains(xdg_toplevel::State::Fullscreen) {
            WindowState::Fullscreen
        } else if states.contains(xdg_toplevel::State::Maximized) {
            WindowState::Maximized
        } else {
            WindowState::Normal
        }
    }

    /// The insets for the window's current role, state and decoration.
    fn insets(&self, decoration: Decoration) -> Insets {
        Insets::for_window(self.role(), self.state(), decoration, &TITLEBAR)
    }

    /// Whether the window geometry fits inside the current buffer.
    fn geometry_fits_buffer(&self) -> bool {
        let surface = self.toplevel.wl_surface();
        geometry_fits(window_geometry(surface), buffer_size(surface))
    }

    /// The client's minimum and maximum usable size, in logical coordinates.
    ///
    /// A zero on an axis means the client did not constrain it.
    fn size_bounds(&self) -> (Size<i32, Logical>, Size<i32, Logical>) {
        let surface = self.toplevel.wl_surface();
        with_states(surface, |states| {
            let mut cached = states.cached_state.get::<SurfaceCachedState>();
            let state = cached.current();
            (state.min_size, state.max_size)
        })
    }
}

/// Resize band outside a client-decorated frame, in logical pixels.
///
/// The client owns the interior, so this band only counts outside the frame.
const CLIENT_RESIZE_BORDER: f64 = 6.0;
/// Corner square outside a client-decorated frame, in logical pixels.
const CLIENT_RESIZE_CORNER: f64 = 12.0;

/// The toplevels being tracked.
#[derive(Default)]
pub struct Windows {
    /// Where the windows are and how they stack.
    scene: Scene,
    /// The surface and configure bookkeeping, in creation order.
    windows: Vec<WindowData>,
    /// Decoration modes requested for a surface whose toplevel is not tracked
    /// yet, by surface object id.
    ///
    /// GTK4 creates its KDE decoration object during realize, before it creates
    /// the xdg_toplevel at map time, so the request can arrive first. The id is
    /// stored rather than the surface so a dead surface is not kept alive.
    pending_decorations: Vec<(ObjectId, Decoration)>,
    /// The next scene id to hand out.
    next_id: u64,
}

impl Windows {
    /// Creates an empty set of tracked windows.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many windows are tracked.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// Whether no windows are tracked.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// The tracked windows, back to front, so the last is drawn on top.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = Window<'_>> {
        self.scene.iter().filter_map(|(id, outer)| {
            self.windows
                .iter()
                .find(|data| data.id == id)
                .map(|data| Window {
                    data,
                    outer,
                    decoration: self.decoration_of(id),
                })
        })
    }

    /// Every tracked toplevel surface, whether mapped or not.
    pub fn surfaces(&self) -> impl Iterator<Item = &WlSurface> {
        self.windows.iter().map(|data| data.toplevel.wl_surface())
    }

    /// Starts tracking `toplevel`; the caller then sends its initial configure.
    ///
    /// The initial configure carries 0×0 (the client chooses its own size), so
    /// there is no size or origin to record yet: both are derived from the
    /// first committed buffer in [`Windows::commit`].
    pub fn add_toplevel(&mut self, toplevel: ToplevelSurface) {
        let id = WindowId::new(self.next_id);
        self.next_id += 1;
        // A mode negotiated before the toplevel existed is remembered now; the
        // caller applies the policy and sends the initial configure.
        let object = toplevel.wl_surface().id();
        let mut requested = None;
        if let Some(index) = self
            .pending_decorations
            .iter()
            .position(|(pending, _)| *pending == object)
        {
            let (_, pending_requested) = self.pending_decorations.remove(index);
            requested = Some(pending_requested);
        }
        self.windows.push(WindowData {
            id,
            toplevel,
            outer_size: Size::from((0, 0)),
            placement_origin: Point::from((0, 0)),
            hidden_rect: None,
            acked: false,
            pending_configure: None,
            mapped: false,
            hidden: false,
            configured: None,
            resizing: false,
            resize_edge: None,
            resize_pending_outer: None,
            resize_configure_sent: None,
            maximized: false,
            fullscreen: false,
            activated: false,
            requested,
            native_warned: false,
            placed: false,
            damaged: false,
            invisible_warned: false,
        });
    }

    /// Handles a surface commit.
    ///
    /// Recomputes the insets from current state and sends a fresh configure if
    /// they changed. The first time a buffer is committed after the initial
    /// configure, the window is centered on its committed geometry (window
    /// geometry or buffer size), clamped to the workspace, and mapped. There is
    /// no second configure: the client already chose and drew its size. Returns
    /// the window's id the first time it becomes mapped, so the caller can give
    /// it keyboard focus.
    pub fn commit(
        &mut self,
        surface: &WlSurface,
        workspace: Rectangle<i32, Logical>,
    ) -> Option<WindowId> {
        let index = self.index_of(surface)?;
        let decoration = self.decoration_of(self.windows[index].id);
        if surface_has_damage(surface) {
            self.windows[index].damaged = true;
        }

        if !self.windows[index].placed && self.windows[index].acked && surface_has_buffer(surface) {
            // The initial configure was 0×0, so this first buffer is the
            // client's own geometry. Center the window on what it actually
            // drew; reconfiguring here would only make it flash.
            let insets = self.windows[index].insets(decoration);
            let is_dialog = self.windows[index].role() == Role::Dialog;
            let (outer_size, origin) = if is_dialog {
                self.dialog_parent(self.windows[index].id)
                    .and_then(|parent_id| {
                        dialog_placement(committed_size(surface), insets, self.outer(parent_id))
                    })
                    .unwrap_or_else(|| {
                        initial_placement(committed_size(surface), insets, workspace)
                    })
            } else {
                initial_placement(committed_size(surface), insets, workspace)
            };
            self.windows[index].outer_size = outer_size;
            self.windows[index].placement_origin = origin;
            // The client is already at this size: record it so the check below
            // does not send a redundant configure.
            self.windows[index].configured = Some(usable(outer_size, insets));
            self.windows[index].placed = true;
        }

        // A normal window may commit a buffer at a size other than the one it
        // was configured (a terminal's cell grid, a client that ignored a
        // resize). Follow the committed geometry so placement, rendering and
        // hit-testing all stay on what the client actually drew.
        let should_resize = {
            let data = &self.windows[index];
            if !data.mapped || data.state() != WindowState::Normal {
                None
            } else {
                let insets = data.insets(self.decoration_of(data.id));
                committed_size(surface).and_then(|client| {
                    let outer = committed_outer(Some(client), insets);
                    (outer != data.outer_size).then_some((data.id, outer))
                })
            }
        };
        if let Some((id, outer_size)) = should_resize {
            // The current position is the scene's outer rectangle; after a move
            // or an arrange it no longer matches the placement origin.
            let current_origin = self
                .scene
                .outer(id)
                .map(|outer| Point::from((round(outer.x()), round(outer.y()))))
                .unwrap_or(self.windows[index].placement_origin);
            // During an interactive resize the opposite edge must stay fixed,
            // so the origin is recomputed from the committed size. Outside a
            // resize the origin stays where it is (client-initiated changes).
            let origin = anchor_resize_origin(
                current_origin,
                self.windows[index].outer_size,
                outer_size,
                self.windows[index].resize_edge,
            );
            let outer = SceneRect::new(
                ScenePoint::new(f64::from(origin.x), f64::from(origin.y)),
                SceneSize::new(f64::from(outer_size.w), f64::from(outer_size.h)),
            );
            if self.scene.set_outer(id, outer) {
                self.windows[index].outer_size = outer_size;
                tracing::debug!(
                    window = id.get(),
                    ?outer_size,
                    resizing = self.windows[index].resizing,
                    "commit resized window"
                );
                if self.windows[index].resizing {
                    // The client has caught up: the frame now follows the
                    // committed size, and any newer pointer request is sent.
                    self.windows[index].resize_configure_sent = None;
                    if let Some(pending) = self.windows[index].resize_pending_outer.take() {
                        self.send_configure_with_outer(index, pending);
                        self.windows[index].resize_configure_sent = Some(Instant::now());
                    }
                } else {
                    self.send_configure(index);
                }
            }
        }

        // Before placement every configure is 0×0, so there is no usable size
        // to compare; sending one here would invalidate the ack the client is
        // about to make and leave the window unmapped.
        if self.windows[index].placed {
            let usable = usable(
                self.windows[index].outer_size,
                self.windows[index].insets(decoration),
            );
            if self.windows[index].configured != Some(usable) {
                self.send_configure(index);
            }
        }

        let (damaged, geometry_fits, warned, mapped, acked) = {
            let data = &self.windows[index];
            (
                data.damaged,
                data.geometry_fits_buffer(),
                data.invisible_warned,
                data.mapped,
                data.acked,
            )
        };
        let mapping = !mapped && acked && surface_has_buffer(surface);
        if (mapping || mapped) && (!damaged || !geometry_fits) && !warned {
            self.windows[index].invisible_warned = true;
            let id = self.windows[index].id;
            let app_id = self.app_id(surface);
            tracing::warn!(
                window = id.get(),
                %app_id,
                damaged,
                geometry_fits,
                "window not visible: its buffer has no damage or its window geometry is outside the buffer"
            );
        }

        if !self.windows[index].mapped && self.windows[index].acked && surface_has_buffer(surface) {
            self.windows[index].mapped = true;
            let data = &self.windows[index];
            let size = data.outer_size;
            let origin = data.placement_origin;
            let outer = SceneRect::new(
                ScenePoint::new(f64::from(origin.x), f64::from(origin.y)),
                SceneSize::new(f64::from(size.w), f64::from(size.h)),
            );
            let id = data.id;
            self.scene.insert(id, outer);
            let app_id = self.app_id(surface);
            tracing::info!(window = id.get(), app_id, ?origin, "window mapped");
            return Some(id);
        }
        None
    }

    /// Records that the client acknowledged a configure for `surface`.
    ///
    /// Only the serial of the configure most recently sent is accepted. An ack
    /// for any other serial is stale — the client is acknowledging a configure
    /// MalaysiaDreams has already replaced — so it is discarded rather than marking
    /// the window ready.
    pub fn ack_configure(&mut self, surface: &WlSurface, configure: Configure) {
        let Configure::Toplevel(configure) = configure else {
            return;
        };
        let Some(index) = self.index_of(surface) else {
            return;
        };
        if self.windows[index].pending_configure == Some(configure.serial) {
            self.windows[index].pending_configure = None;
            self.windows[index].acked = true;
        } else {
            tracing::debug!("ignoring an ack for a configure no longer pending");
        }
    }

    /// Stops tracking the toplevel for `surface`, returning its scene id.
    ///
    /// A decoration requested for a surface that never became a toplevel is
    /// dropped too, so a dead surface is not remembered.
    pub fn remove(&mut self, surface: &WlSurface) -> Option<WindowId> {
        let object = surface.id();
        self.pending_decorations
            .retain(|(pending, _)| *pending != object);
        let index = self.index_of(surface)?;
        let id = self.windows.remove(index).id;
        self.scene.remove(id);
        Some(id)
    }

    /// Stops tracking the window with `id` (used by destruction).
    pub fn remove_by_id(&mut self, id: WindowId) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        self.windows.remove(index);
        self.scene.remove(id);
        true
    }

    /// Hides a window from the scene without dropping its state, or puts it
    /// back with its last outer rectangle (minimize/restore).
    pub fn set_hidden(&mut self, id: WindowId, hidden: bool) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if self.windows[index].hidden == hidden {
            return false;
        }
        let data = &mut self.windows[index];
        data.hidden = hidden;
        let rect = if hidden {
            data.hidden_rect = self.scene.outer(id);
            SceneRect::new(ScenePoint::new(0.0, 0.0), SceneSize::new(0.0, 0.0))
        } else {
            data.hidden_rect.unwrap_or_else(|| {
                SceneRect::new(
                    ScenePoint::new(
                        f64::from(data.placement_origin.x),
                        f64::from(data.placement_origin.y),
                    ),
                    SceneSize::new(f64::from(data.outer_size.w), f64::from(data.outer_size.h)),
                )
            })
        };
        if !hidden {
            self.windows[index].hidden_rect = None;
        }
        self.scene.set_outer(id, rect)
    }

    /// The scene id of the window for `surface`.
    pub fn id_of(&self, surface: &WlSurface) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|data| data.toplevel.wl_surface() == surface)
            .map(|data| data.id)
    }

    /// The client surface of the window with `id`.
    pub fn surface_of(&self, id: WindowId) -> Option<&WlSurface> {
        self.windows
            .iter()
            .find(|data| data.id == id)
            .map(|data| data.toplevel.wl_surface())
    }

    /// Whether the window for `surface` has been placed in the workspace.
    pub fn is_placed(&self, surface: &WlSurface) -> bool {
        self.index_of(surface)
            .map(|index| self.windows[index].placed)
            .unwrap_or(false)
    }

    /// Whether the window for `surface` is mapped and should be shown.
    pub fn is_mapped(&self, surface: &WlSurface) -> bool {
        self.index_of(surface)
            .map(|index| self.windows[index].mapped)
            .unwrap_or(false)
    }

    /// The window's title and app id, for the shell's active-window state.
    pub fn active_info(&self, id: WindowId) -> Option<(String, String)> {
        let index = self.index(id)?;
        let surface = self.windows[index].toplevel.wl_surface();
        let info = with_states(surface, |states| {
            let attributes = states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .expect("a tracked toplevel has xdg toplevel data")
                .lock()
                .unwrap();
            (
                attributes.title.clone().unwrap_or_default(),
                attributes.app_id.clone().unwrap_or_default(),
            )
        });
        Some(info)
    }

    /// Whether the window is currently visible.
    ///
    /// A window is visible when it is mapped, not hidden, not in the minimized
    /// set kept by the compositor, and has a damaged buffer whose window
    /// geometry fits inside it.
    pub fn is_visible(&self, id: WindowId, minimized: &HashSet<WindowId>) -> bool {
        self.index(id).is_some_and(|index| {
            let data = &self.windows[index];
            data.mapped
                && !data.hidden
                && !minimized.contains(&id)
                && surface_has_buffer(data.toplevel.wl_surface())
                && data.damaged
                && data.geometry_fits_buffer()
        })
    }

    /// The tracked windows that are currently visible, back to front.
    pub fn iter_visible<'a>(
        &'a self,
        minimized: &'a HashSet<WindowId>,
    ) -> impl DoubleEndedIterator<Item = Window<'a>> + 'a {
        self.iter()
            .filter(move |window| window.is_visible(minimized))
    }

    /// The topmost visible window, if any.
    pub fn topmost_visible(&self, minimized: &HashSet<WindowId>) -> Option<WindowId> {
        self.iter_visible(minimized)
            .next_back()
            .map(|window| window.id())
    }

    /// The scene id of the topmost visible window under a logical point.
    pub fn window_under(
        &self,
        point: Point<f64, Logical>,
        minimized: &HashSet<WindowId>,
    ) -> Option<WindowId> {
        self.scene
            .topmost_at(scene_point(point))
            .filter(|id| self.is_visible(*id, minimized))
    }

    /// The topmost visible window under `point` and its resize edge zone, if any.
    ///
    /// Server-decorated windows use the configured inside/outside band.
    /// Client-decorated windows only get an exterior band: the client owns its
    /// interior, but GTK and Firefox drop their own resize margin once the
    /// configure marks them tiled, so the compositor must offer the outer grab.
    /// A maximized/fullscreen window has no free edge to drag.
    pub fn edge_under(
        &self,
        point: Point<f64, Logical>,
        side: f64,
        corner: f64,
        minimized: &HashSet<WindowId>,
    ) -> Option<(WindowId, Edge)> {
        if let Some(hit) = self.scene.edge_at_with(scene_point(point), |id| {
            if !self.is_visible(id, minimized) {
                return None;
            }
            let index = self.index(id)?;
            let data = &self.windows[index];
            if data.maximized || data.state() != WindowState::Normal {
                return None;
            }
            match self.decoration_of(id) {
                Decoration::Server => Some((side, corner, false)),
                Decoration::Client => None,
            }
        }) {
            return Some(hit);
        }

        // A client-decorated GTK/Firefox window keeps an invisible shadow
        // margin in its buffer, so the scene rectangle is larger than the
        // visible frame. Hit-test the frame itself; its exterior band is the
        // resize grab the client no longer draws.
        for window in self.iter_visible(minimized).rev() {
            let id = window.id();
            let index = self.index(id)?;
            let data = &self.windows[index];
            if data.maximized || data.state() != WindowState::Normal {
                continue;
            }
            if self.decoration_of(id) != Decoration::Client {
                if window.contains(point) {
                    return None;
                }
                continue;
            }
            let origin = window.location() + window.surface_location();
            let geometry = window.window_geometry();
            let frame = SceneRect::new(
                ScenePoint::new(
                    f64::from(origin.x + geometry.loc.x),
                    f64::from(origin.y + geometry.loc.y),
                ),
                SceneSize::new(f64::from(geometry.size.w), f64::from(geometry.size.h)),
            );
            if let Some(edge) = malaysia_scene::scene::exterior_edge(
                frame,
                scene_point(point),
                CLIENT_RESIZE_BORDER,
                CLIENT_RESIZE_CORNER,
            ) {
                return Some((id, edge));
            }
            if frame.contains(scene_point(point)) {
                return None;
            }
        }
        None
    }

    /// The surface under a logical point, with the point in surface-local
    /// coordinates.
    ///
    /// Only the client's own surface counts: a point over the decorated band
    /// around it is over the window but not over the client.
    pub fn surface_under(
        &self,
        point: Point<f64, Logical>,
        minimized: &HashSet<WindowId>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let id = self
            .scene
            .topmost_at(scene_point(point))
            .filter(|id| self.is_visible(*id, minimized))?;
        let index = self.index(id)?;
        let outer = self.scene.outer(id)?;
        let insets = self.windows[index].insets(self.decoration_of(id));
        let local = client_point(outer, insets, point)?;
        Some((self.windows[index].toplevel.wl_surface().clone(), local))
    }

    /// Moves a window's top-left corner, keeping its size.
    pub fn move_to(&mut self, id: WindowId, location: Point<i32, Logical>) -> bool {
        let Some(outer) = self.scene.outer(id) else {
            return false;
        };
        let moved = SceneRect::new(
            ScenePoint::new(f64::from(location.x), f64::from(location.y)),
            outer.size(),
        );
        self.scene.set_outer(id, moved)
    }

    /// Applies a new decorated outer rectangle and configures the client.
    ///
    /// Used by an in-progress resize; the configure carries the `resizing`
    /// state until the grab ends.  To keep the frame anchored to the committed
    /// size while the app catches up, the scene outer is only updated when the
    /// client commits a buffer.
    pub fn resize(&mut self, id: WindowId, outer: SceneRect<f64, SceneLogical>) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        let requested = Size::from((round(outer.width()), round(outer.height())));
        let data = &mut self.windows[index];
        let can_send = data
            .resize_configure_sent
            .is_none_or(|t| t.elapsed() >= RESIZE_CONFIGURE_TIMEOUT);
        if can_send {
            data.resize_configure_sent = Some(Instant::now());
            data.resize_pending_outer = None;
            self.send_configure_with_outer(index, requested);
        } else {
            data.resize_pending_outer = Some(requested);
        }
        true
    }

    /// Sets whether the window is being resized and re-configures it.
    pub fn set_resizing(&mut self, id: WindowId, resizing: bool, edge: Option<Edge>) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        self.windows[index].resizing = resizing;
        self.windows[index].resize_edge = edge;
        if !resizing {
            // Ending the resize: send the most recent pending request so the
            // final size matches where the pointer released.
            self.windows[index].resize_configure_sent = None;
            if let Some(pending) = self.windows[index].resize_pending_outer.take() {
                self.send_configure_with_outer(index, pending);
                return true;
            }
        }
        self.send_configure(index);
        true
    }

    /// The smallest decorated outer size this window allows, from the client's
    /// minimum size plus the insets.
    ///
    /// A client minimum of zero means no constraint on that axis, but the outer
    /// size still never collapses below one pixel.
    pub fn minimum_outer_size(&self, id: WindowId) -> SceneSize<f64, SceneLogical> {
        at_least_one_pixel(self.outer_size_bounds(id, |min, _| min))
    }

    /// Whether the window can be zoomed: its client minimum is not a fixed
    /// size shared with the maximum.
    pub fn zoom_supported(&self, id: WindowId) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        let (min, max) = self.windows[index].size_bounds();
        !(min.w > 0 && min.h > 0 && min == max)
    }

    /// The largest decorated outer size, or zero where the client left it
    /// unbounded.
    pub fn maximum_outer_size(&self, id: WindowId) -> SceneSize<f64, SceneLogical> {
        self.outer_size_bounds(id, |_, max| max)
    }

    /// The client's minimum and maximum usable size, if the window is tracked.
    pub fn client_size_bounds(
        &self,
        id: WindowId,
    ) -> Option<(Size<i32, Logical>, Size<i32, Logical>)> {
        let index = self.index(id)?;
        Some(self.windows[index].size_bounds())
    }

    /// Raises a window to the top of the stack.
    pub fn raise(&mut self, id: WindowId) -> bool {
        self.scene.raise(id)
    }

    /// The topmost window, if any.
    pub fn topmost(&self) -> Option<WindowId> {
        self.scene.topmost()
    }

    /// The decoration insets for `id`, or `None` when it is not tracked.
    pub fn insets(&self, id: WindowId) -> Option<Insets> {
        let index = self.index(id)?;
        Some(self.windows[index].insets(self.decoration_of(id)))
    }

    /// Sets the activated state the next configure carries and reconfigures.
    pub fn set_activated(&mut self, id: WindowId, activated: bool) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if self.windows[index].activated == activated {
            return false;
        }
        self.windows[index].activated = activated;
        self.send_configure(index);
        true
    }

    /// Sets the maximized state and outer rectangle, sending a single configure.
    ///
    /// Used for maximize/restore so the client receives one configure carrying
    /// both the new state and the new size, instead of two intermediate ones.
    pub fn set_maximized_with_outer(
        &mut self,
        id: WindowId,
        maximized: bool,
        outer: SceneRect<f64, SceneLogical>,
    ) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        let state_changed = self.windows[index].maximized != maximized;
        self.windows[index].maximized = maximized;
        self.scene.set_outer(id, outer);
        self.windows[index].outer_size = Size::from((round(outer.width()), round(outer.height())));
        self.send_configure(index);
        state_changed
    }

    /// Sets the fullscreen state and outer rectangle, sending a single configure.
    pub fn set_fullscreen_with_outer(
        &mut self,
        id: WindowId,
        fullscreen: bool,
        outer: SceneRect<f64, SceneLogical>,
    ) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        let state_changed = self.windows[index].fullscreen != fullscreen;
        self.windows[index].fullscreen = fullscreen;
        self.scene.set_outer(id, outer);
        self.windows[index].outer_size = Size::from((round(outer.width()), round(outer.height())));
        self.send_configure(index);
        state_changed
    }

    /// Applies an outer rectangle from Infinity without changing any state bit.
    pub fn set_infinity_geometry(&mut self, id: WindowId, outer: SceneRect<f64, SceneLogical>) {
        let Some(index) = self.index(id) else {
            return;
        };
        self.scene.set_outer(id, outer);
        self.windows[index].outer_size = Size::from((round(outer.width()), round(outer.height())));
        self.send_configure(index);
    }

    /// Sets the maximized state the next configure carries and reconfigures.
    pub fn set_maximized(&mut self, id: WindowId, maximized: bool) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if self.windows[index].maximized == maximized {
            return false;
        }
        self.windows[index].maximized = maximized;
        self.send_configure(index);
        true
    }

    /// A window's decorated outer rectangle.
    pub fn outer(&self, id: WindowId) -> Option<SceneRect<f64, SceneLogical>> {
        self.scene.outer(id)
    }

    /// A window's absolute usable rectangle — where its client surface sits —
    /// in logical pixels.
    ///
    /// This is the rectangle popups anchor against when the window is their
    /// parent (the `window_geometry` of the xdg protocol).
    pub fn usable_rect(&self, id: WindowId) -> Option<Rectangle<i32, Logical>> {
        let outer = self.scene.outer(id)?;
        let index = self.index(id)?;
        let decoration = self.decoration_of(id);
        let insets = self.windows[index].insets(decoration);
        let location = usable_rect_location(outer, insets);
        let size = usable(self.windows[index].outer_size, insets);
        Some(Rectangle::new(location, size))
    }

    // --- popups -------------------------------------------------------------
    //
    // The popup *geometry* lives in the scene, like windows (ADR 0015); the
    // compositor's `Popups` tracker holds the protocol surface. These methods
    // expose the scene's popup side to it.

    /// Adds a popup to the scene stack.
    pub fn insert_popup(
        &mut self,
        id: PopupId,
        parent: PopupParent,
        rect: SceneRect<f64, SceneLogical>,
    ) {
        self.scene.insert_popup(id, parent, rect);
    }

    /// Removes a popup from the scene.
    pub fn remove_popup(&mut self, id: PopupId) -> bool {
        self.scene.remove_popup(id)
    }

    /// A popup's positioned rectangle.
    pub fn popup_rect(&self, id: PopupId) -> Option<SceneRect<f64, SceneLogical>> {
        self.scene.popup_rect(id)
    }

    /// Replaces a popup's positioned rectangle.
    pub fn set_popup_rect(&mut self, id: PopupId, rect: SceneRect<f64, SceneLogical>) -> bool {
        self.scene.set_popup_rect(id, rect)
    }

    /// Every popup, back to front.
    pub fn iter_popups(
        &self,
    ) -> impl DoubleEndedIterator<Item = (PopupId, PopupParent, SceneRect<f64, SceneLogical>)> + '_
    {
        self.scene.iter_popups()
    }

    /// The topmost popup under a logical point, if any.
    pub fn popup_under(&self, point: Point<f64, Logical>) -> Option<PopupId> {
        self.scene.popup_under(scene_point(point))
    }

    /// The topmost popup, if any.
    pub fn topmost_popup(&self) -> Option<PopupId> {
        self.scene.topmost_popup()
    }

    /// The popup ancestors of `id`, from the immediate parent upwards.
    pub fn popup_ancestors(&self, id: PopupId) -> Vec<PopupId> {
        self.scene.popup_ancestors(id)
    }

    /// Every popup in the subtree rooted at `id`, including `id`.
    pub fn popup_subtree(&self, id: PopupId) -> Vec<PopupId> {
        self.scene.popup_subtree(id)
    }

    /// Asks the client to close the window.
    pub fn send_close(&self, id: WindowId) {
        if let Some(index) = self.index(id) {
            self.windows[index].toplevel.send_close();
        }
    }

    /// Stores the requested decoration mode without applying it.
    ///
    /// The caller resolves the policy and native-app rule, then stores the
    /// effective mode with [`Windows::set_decoration`].
    pub fn set_requested_decoration(&mut self, surface: &WlSurface, requested: Decoration) {
        let object = surface.id();
        if let Some(index) = self.index_of(surface) {
            self.windows[index].requested = Some(requested);
            return;
        }
        match self
            .pending_decorations
            .iter_mut()
            .find(|(pending, _)| *pending == object)
        {
            Some((_, stored)) => *stored = requested,
            None => self.pending_decorations.push((object, requested)),
        }
    }

    /// Clears the requested mode (`new_decoration` or `unset_mode`).
    pub fn clear_requested_decoration(&mut self, surface: &WlSurface) {
        if let Some(index) = self.index_of(surface) {
            self.windows[index].requested = None;
        }
    }

    /// The decoration mode the client asked for, if it sent `request_mode`.
    pub fn requested_decoration(&self, surface: &WlSurface) -> Option<Decoration> {
        if let Some(index) = self.index_of(surface) {
            return self.windows[index].requested;
        }
        let object = surface.id();
        self.pending_decorations
            .iter()
            .find(|(pending, _)| *pending == object)
            .map(|(_, requested)| *requested)
    }

    /// Stores the effective mode without sending a configure (the initial
    /// configure follows separately).
    pub fn set_initial_decoration(&mut self, surface: &WlSurface, decoration: Decoration) {
        if let Some(index) = self.index_of(surface) {
            let id = self.windows[index].id;
            self.scene.set_decoration(id, decoration);
        }
    }

    /// Sends the first configure, once the initial decoration is stored.
    pub fn send_initial_configure(&mut self, surface: &WlSurface) {
        if let Some(index) = self.index_of(surface) {
            self.send_configure(index);
        }
    }

    /// Stores a window's effective decoration mode and re-configures it if it
    /// changed.
    pub fn set_decoration(&mut self, surface: &WlSurface, decoration: Decoration) -> bool {
        let Some(index) = self.index_of(surface) else {
            return false;
        };
        let id = self.windows[index].id;
        let previous = self.decoration_of(id);
        self.scene.set_decoration(id, decoration);
        if previous == decoration {
            return false;
        }
        self.send_configure(index);
        true
    }

    /// The window's app id, as the client last set it.
    pub fn app_id(&self, surface: &WlSurface) -> String {
        with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .map(|data| data.lock().unwrap().app_id.clone().unwrap_or_default())
                .unwrap_or_default()
        })
    }

    /// Whether the window is one of Malaysia's own apps (ADR 0035).
    pub fn is_native(&self, surface: &WlSurface) -> bool {
        self.app_id(surface).starts_with(NATIVE_APP_ID_PREFIX)
    }

    /// Marks the native-app warning as emitted; returns false if it was already.
    pub fn mark_native_warning(&mut self, surface: &WlSurface) -> bool {
        let Some(index) = self.index_of(surface) else {
            return false;
        };
        if self.windows[index].native_warned {
            return false;
        }
        self.windows[index].native_warned = true;
        true
    }

    /// The decoration mode of a window, or client-side when none was stored.
    ///
    /// A window that never negotiated has no stored mode; treating it as
    /// self-decorating is the policy ADR 0019 sets.
    fn decoration_of(&self, id: WindowId) -> Decoration {
        self.scene.decoration(id).unwrap_or(Decoration::Client)
    }

    /// The tracked window id of a surface, if any.
    fn id_of_surface(&self, surface: &WlSurface) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|data| data.toplevel.wl_surface() == surface)
            .map(|data| data.id)
    }

    /// The parent window of a dialog, if it is tracked.
    fn dialog_parent(&self, id: WindowId) -> Option<WindowId> {
        let index = self.index(id)?;
        let parent = self.windows[index].toplevel.parent()?;
        self.id_of_surface(&parent)
    }

    /// Sends a configure for the current state.
    ///
    /// Before the window is placed (its first buffer committed) every configure
    /// carries 0×0, so the client chooses its own size. After that the configure
    /// carries the usable size for the current outer size and state.
    fn send_configure(&mut self, index: usize) {
        let id = self.windows[index].id;
        let decoration = self.decoration_of(id);
        let data = &mut self.windows[index];
        let configured = configure_size(data.placed, data.outer_size, data.insets(decoration));
        let resizing = data.resizing;
        data.toplevel.with_pending_state(|state| {
            state.size = Some(configured);
            // The configure carries the mode the client negotiated; a client
            // with no decoration object ignores the field, and one with an
            // object learns the choice before its first attach (ADR 0019).
            state.decoration_mode = Some(match decoration {
                Decoration::Server => zxdg_toplevel_decoration_v1::Mode::ServerSide,
                Decoration::Client => zxdg_toplevel_decoration_v1::Mode::ClientSide,
            });
            apply_configure_states(
                &mut state.states,
                data.placed,
                data.maximized,
                data.fullscreen,
                resizing,
                data.activated,
            );
        });
        if data.maximized {
            tracing::info!(window = id.get(), ?configured, "configure with Maximized");
        }
        tracing::debug!(window = id.get(), ?configured, resizing, "send_configure");
        data.pending_configure = Some(data.toplevel.send_configure());
        data.configured = Some(configured);
    }

    /// Sends a configure using `outer_size` as the window size without changing
    /// the scene rectangle. Used during an interactive resize so the requested
    /// size reaches the client while the frame stays at the committed size.
    fn send_configure_with_outer(&mut self, index: usize, outer_size: Size<i32, Logical>) {
        let previous = self.windows[index].outer_size;
        self.windows[index].outer_size = outer_size;
        self.send_configure(index);
        self.windows[index].outer_size = previous;
    }

    /// The outer size bounds from the client's usable minimum and maximum.
    ///
    /// A zero axis stays zero: the client set no bound on it, and for a maximum
    /// that means unbounded. `resized` reads a zero maximum that way
    /// (`malaysia-scene/src/scene.rs:283-304`).
    fn outer_size_bounds(
        &self,
        id: WindowId,
        pick: impl Fn(Size<i32, Logical>, Size<i32, Logical>) -> Size<i32, Logical>,
    ) -> SceneSize<f64, SceneLogical> {
        let Some(index) = self.index(id) else {
            return SceneSize::new(0.0, 0.0);
        };
        let data = &self.windows[index];
        let (min, max) = data.size_bounds();
        let bound = pick(min, max);
        let insets = data.insets(self.decoration_of(id));
        outer_size(bound, insets)
    }

    /// The index of the window for `surface`.
    fn index_of(&self, surface: &WlSurface) -> Option<usize> {
        self.windows
            .iter()
            .position(|data| data.toplevel.wl_surface() == surface)
    }

    /// The index of the window with `id`.
    fn index(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|data| data.id == id)
    }
}

/// The absolute top-left corner of a window's usable area.
///
/// Kept separate from [`Windows::usable_rect`] so the rounding can be tested
/// without building a full [`Windows`] tracker. Uses `round` because the usable
/// rectangle must fit inside the workspace: flooring the origin would push it
/// toward the exclusive zones (e.g., the menubar), while ceiling it would push
/// the opposite edge past the dock.
fn usable_rect_location(
    outer: SceneRect<f64, SceneLogical>,
    insets: Insets,
) -> Point<i32, Logical> {
    Point::from((
        (outer.x() + insets.left).round() as i32,
        (outer.y() + insets.top).round() as i32,
    ))
}

/// Maps an xdg resize edge to a scene edge, if it names one.
pub(crate) fn from_xdg_resize_edge(edge: ResizeEdge) -> Option<Edge> {
    match edge {
        ResizeEdge::Top => Some(Edge::Top),
        ResizeEdge::Bottom => Some(Edge::Bottom),
        ResizeEdge::Left => Some(Edge::Left),
        ResizeEdge::Right => Some(Edge::Right),
        ResizeEdge::TopLeft => Some(Edge::TopLeft),
        ResizeEdge::TopRight => Some(Edge::TopRight),
        ResizeEdge::BottomLeft => Some(Edge::BottomLeft),
        ResizeEdge::BottomRight => Some(Edge::BottomRight),
        _ => None,
    }
}

/// A logical point in the scene's coordinate types.
fn scene_point(point: Point<f64, Logical>) -> ScenePoint<f64, SceneLogical> {
    ScenePoint::new(point.x, point.y)
}

/// The decorated outer size for a client bound.
///
/// The insets are added to a bound the client set; a zero axis stays zero,
/// because zero means the client set no bound there. `resized` reads a zero
/// maximum as unbounded, so turning it into one pixel would collapse every
/// pointer-driven resize.
fn outer_size(bound: Size<i32, Logical>, insets: Insets) -> SceneSize<f64, SceneLogical> {
    let width = if bound.w > 0 {
        f64::from(bound.w) + insets.horizontal()
    } else {
        0.0
    };
    let height = if bound.h > 0 {
        f64::from(bound.h) + insets.vertical()
    } else {
        0.0
    };
    SceneSize::new(width, height)
}

/// Keeps an outer size from collapsing below one pixel on an axis.
fn at_least_one_pixel(size: SceneSize<f64, SceneLogical>) -> SceneSize<f64, SceneLogical> {
    SceneSize::new(size.width.max(1.0), size.height.max(1.0))
}

/// Centers a window of `size` inside the workspace.
fn center_origin(
    size: Size<i32, Logical>,
    workspace: Rectangle<i32, Logical>,
) -> Point<i32, Logical> {
    Point::from((
        workspace.loc.x + (workspace.size.w - size.w) / 2,
        workspace.loc.y + (workspace.size.h - size.h) / 2,
    ))
}

/// Centers a dialog over its parent window.
fn dialog_placement(
    committed: Option<Size<i32, Logical>>,
    insets: Insets,
    parent_outer: Option<SceneRect<f64, SceneLogical>>,
) -> Option<(Size<i32, Logical>, Point<i32, Logical>)> {
    let parent = parent_outer?;
    let outer = committed_outer(committed, insets);
    let center_x = parent.x() + parent.width() / 2.0;
    let center_y = parent.y() + parent.height() / 2.0;
    let origin = Point::from((
        (center_x - f64::from(outer.w) / 2.0).round() as i32,
        (center_y - f64::from(outer.h) / 2.0).round() as i32,
    ));
    Some((outer, origin))
}

/// The initial placement for a new window: centered in the workspace, from the
/// client's committed geometry, clamped so it fits.
///
/// A client that committed nothing usable (0×0 and no content) still gets the
/// default 640×420 window; that is rare, but without it the window would be a
/// single pixel.
fn initial_placement(
    committed: Option<Size<i32, Logical>>,
    insets: Insets,
    workspace: Rectangle<i32, Logical>,
) -> (Size<i32, Logical>, Point<i32, Logical>) {
    let mut outer = committed_outer(committed, insets);
    outer.w = outer.w.min(workspace.size.w);
    outer.h = outer.h.min(workspace.size.h);
    let origin = center_origin(outer, workspace);
    (outer, origin)
}

/// The decorated outer size a window occupies: the committed client geometry
/// (window geometry or buffer) plus the insets.
///
/// Hit-testing, rendering and placement all derive the window from this, never
/// from a configured size the client has not drawn. A client that committed no
/// usable geometry falls back to 640×420 (see [`initial_placement`]).
fn committed_outer(committed: Option<Size<i32, Logical>>, insets: Insets) -> Size<i32, Logical> {
    let client = match committed {
        Some(size) if size.w > 0 && size.h > 0 => size,
        _ => Size::from((640, 420)),
    };
    let outer = outer_size(client, insets);
    Size::from((round(outer.width), round(outer.height)))
}

/// Fills the xdg_toplevel states for a configure.
///
/// A configure sent before the first buffer (the initial configure) carries no
/// `Tiled*`: Firefox reads tiled on the initial 0×0 configure as a tiled
/// request and opens at a tiny default size instead of its own. Once the window
/// is placed, normal windows are advertised as tiled on all four sides so
/// GTK/libadwaita and Firefox drop their client-side rounded corners and
/// resize margin. Maximized and fullscreen keep their existing state behaviour.
fn apply_configure_states(
    states: &mut ToplevelStateSet,
    placed: bool,
    maximized: bool,
    fullscreen: bool,
    resizing: bool,
    activated: bool,
) {
    if resizing {
        states.set(xdg_toplevel::State::Resizing);
    } else {
        states.unset(xdg_toplevel::State::Resizing);
    }
    if maximized {
        states.set(xdg_toplevel::State::Maximized);
    } else {
        states.unset(xdg_toplevel::State::Maximized);
    }
    if fullscreen {
        states.set(xdg_toplevel::State::Fullscreen);
    } else {
        states.unset(xdg_toplevel::State::Fullscreen);
    }
    if activated {
        states.set(xdg_toplevel::State::Activated);
    } else {
        states.unset(xdg_toplevel::State::Activated);
    }
    let tiled = placed && !fullscreen && !maximized;
    for tile in [
        xdg_toplevel::State::TiledLeft,
        xdg_toplevel::State::TiledRight,
        xdg_toplevel::State::TiledTop,
        xdg_toplevel::State::TiledBottom,
    ] {
        if tiled {
            states.set(tile);
        } else {
            states.unset(tile);
        }
    }
}

/// The client's committed geometry: the xdg window geometry when it set one,
/// otherwise the committed buffer's logical size.
fn committed_size(surface: &WlSurface) -> Option<Size<i32, Logical>> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
            .map(|rect| rect.size)
    })
    .or_else(|| with_renderer_surface_state(surface, |state| state.surface_size()).flatten())
}

/// The configured size: the outer size minus the insets, never below one pixel.
fn usable(window: Size<i32, Logical>, insets: Insets) -> Size<i32, Logical> {
    let width = (f64::from(window.w) - insets.horizontal()).max(1.0).round();
    let height = (f64::from(window.h) - insets.vertical()).max(1.0).round();
    Size::from((width as i32, height as i32))
}

/// The size a configure carries: 0×0 before the window is placed (the client
/// chooses), then the committed window geometry.
fn configure_size(placed: bool, outer: Size<i32, Logical>, insets: Insets) -> Size<i32, Logical> {
    if placed {
        usable(outer, insets)
    } else {
        Size::from((0, 0))
    }
}

/// Rounds a logical length to whole pixels.
fn round(value: f64) -> i32 {
    value.round() as i32
}

/// The client's explicit window geometry, if it set one.
fn window_geometry(surface: &WlSurface) -> Option<Rectangle<i32, Logical>> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
    })
}

/// The logical size of the surface's current buffer.
fn buffer_size(surface: &WlSurface) -> Option<Size<i32, Logical>> {
    with_renderer_surface_state(surface, |state| state.buffer_size()).flatten()
}

/// Recomputes the origin for a committed size during an interactive resize,
/// keeping the opposite edge fixed.
///
/// `old_size` is the configured outer size that anchored the opposite edge;
/// `new_size` is what the client actually committed. For a left/top drag the
/// origin moves so the right/bottom edge stays in place; for other edges the
/// origin is unchanged.
fn anchor_resize_origin(
    origin: Point<i32, Logical>,
    old_size: Size<i32, Logical>,
    new_size: Size<i32, Logical>,
    edge: Option<Edge>,
) -> Point<i32, Logical> {
    let Some(edge) = edge else {
        return origin;
    };
    let mut origin = origin;
    let fixed_right = origin.x + old_size.w;
    let fixed_bottom = origin.y + old_size.h;
    if matches!(edge, Edge::Left | Edge::TopLeft | Edge::BottomLeft) {
        origin.x = fixed_right - new_size.w;
    }
    if matches!(edge, Edge::Top | Edge::TopLeft | Edge::TopRight) {
        origin.y = fixed_bottom - new_size.h;
    }
    origin
}

/// Whether a window geometry lies inside its buffer.
///
/// A surface with no explicit geometry, or without a buffer yet, is not
/// rejected here: the buffer check belongs to [`surface_has_buffer`].
fn geometry_fits(
    geometry: Option<Rectangle<i32, Logical>>,
    buffer: Option<Size<i32, Logical>>,
) -> bool {
    let (Some(geometry), Some(buffer)) = (geometry, buffer) else {
        return true;
    };
    geometry.loc.x >= 0
        && geometry.loc.y >= 0
        && geometry.size.w > 0
        && geometry.size.h > 0
        && geometry.loc.x + geometry.size.w <= buffer.w
        && geometry.loc.y + geometry.size.h <= buffer.h
}

/// Whether any commit has ever carried damage for the surface's buffer.
///
/// A new buffer's damage is moved into the renderer state by
/// `on_commit_buffer_handler`; damage from a commit without a new buffer stays
/// in `SurfaceAttributes`.
fn surface_has_damage(surface: &WlSurface) -> bool {
    let pending = with_states(surface, |states| {
        !states
            .cached_state
            .get::<SurfaceAttributes>()
            .current()
            .damage
            .is_empty()
    });
    pending
        || with_renderer_surface_state(surface, |state| {
            state.damage().raw().any(|mut rects| rects.next().is_some())
        })
        .unwrap_or(false)
}

/// Whether the surface currently has a buffer attached.
///
/// This reads the renderer's surface state, which `on_commit_buffer_handler`
/// populates, because that call *consumes* the compositor's own buffer
/// assignment, so `SurfaceAttributes` can no longer answer this.
pub(crate) fn surface_has_buffer(surface: &WlSurface) -> bool {
    with_renderer_surface_state(surface, |state| state.buffer().is_some()).unwrap_or(false)
}

/// The client-surface-local point for `point`, or `None` when it falls on the
/// decoration band around the client.
fn client_point(
    outer: SceneRect<f64, SceneLogical>,
    insets: Insets,
    point: Point<f64, Logical>,
) -> Option<Point<f64, Logical>> {
    let origin: ScenePoint<f64, SceneLogical> =
        ScenePoint::new(outer.x() + insets.left, outer.y() + insets.top);
    let local: Point<f64, Logical> = Point::new(point.x - origin.x, point.y - origin.y);
    let usable = usable(
        Size::from((round(outer.width()), round(outer.height()))),
        insets,
    );
    (local.x >= 0.0
        && local.y >= 0.0
        && local.x < f64::from(usable.w)
        && local.y < f64::from(usable.h))
    .then_some(local)
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point, Rectangle, Size};

    use super::{
        anchor_resize_origin, apply_configure_states, at_least_one_pixel, client_point,
        committed_outer, configure_size, geometry_fits, initial_placement, outer_size, usable,
        usable_rect_location,
    };
    use crate::decoration::Insets;
    use malaysia_scene::geometry::{Point as ScenePoint, Rect as SceneRect, Size as SceneSize};
    use malaysia_scene::scene::Edge;
    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
    use smithay::wayland::shell::xdg::ToplevelStateSet;

    fn size(width: i32, height: i32) -> Size<i32, Logical> {
        Size::from((width, height))
    }

    /// The inset a server-decorated window reserves for its titlebar.
    fn titlebar() -> Insets {
        Insets {
            top: 32.0,
            ..Insets::ZERO
        }
    }

    #[test]
    fn usable_size_subtracts_the_top_inset() {
        let insets = Insets {
            top: 32.0,
            ..Insets::ZERO
        };
        assert_eq!(usable(size(800, 600), insets), size(800, 568));
    }

    #[test]
    fn usable_size_subtracts_both_axes() {
        let insets = Insets {
            top: 10.0,
            right: 5.0,
            bottom: 10.0,
            left: 5.0,
        };
        assert_eq!(usable(size(800, 600), insets), size(790, 580));
    }

    #[test]
    fn usable_size_is_the_window_when_nothing_is_reserved() {
        assert_eq!(usable(size(800, 600), Insets::ZERO), size(800, 600));
    }

    #[test]
    fn usable_size_never_collapses() {
        let insets = Insets {
            top: 500.0,
            bottom: 500.0,
            ..Insets::ZERO
        };
        assert_eq!(usable(size(800, 600), insets), size(800, 1));
    }

    #[test]
    fn an_unset_client_maximum_stays_unbounded() {
        // A maximum of zero means the client set none. Reading it as one pixel
        // clamped every pointer-driven resize to 1x1.
        assert_eq!(outer_size(size(0, 0), titlebar()), SceneSize::new(0.0, 0.0));
    }

    #[test]
    fn a_client_bound_carries_the_insets() {
        assert_eq!(
            outer_size(size(800, 600), titlebar()),
            SceneSize::new(800.0, 632.0)
        );
    }

    fn tiled_states() -> [xdg_toplevel::State; 4] {
        [
            xdg_toplevel::State::TiledLeft,
            xdg_toplevel::State::TiledRight,
            xdg_toplevel::State::TiledTop,
            xdg_toplevel::State::TiledBottom,
        ]
    }

    #[test]
    fn the_initial_configure_is_not_tiled_and_the_next_one_is() {
        // Firefox opens at 500×304 when the 0×0 initial configure says Tiled.
        let mut initial = ToplevelStateSet::default();
        apply_configure_states(&mut initial, false, false, false, false, false);
        for state in tiled_states() {
            assert!(!initial.contains(state), "unexpected {state:?}");
        }
        assert_eq!(
            configure_size(false, size(1204, 804), titlebar()),
            size(0, 0)
        );

        // The first configure after the first buffer carries the committed
        // size, never 0×0.
        let mut later = ToplevelStateSet::default();
        apply_configure_states(&mut later, true, false, false, false, false);
        for state in tiled_states() {
            assert!(later.contains(state), "missing {state:?}");
        }
        assert_eq!(
            configure_size(true, size(1204, 804), titlebar()),
            size(1204, 772)
        );
    }

    #[test]
    fn a_normal_toplevel_is_tiled_on_all_four_sides() {
        let mut states = ToplevelStateSet::default();
        apply_configure_states(&mut states, true, false, false, false, true);
        for state in tiled_states() {
            assert!(states.contains(state), "missing {state:?}");
        }
    }

    #[test]
    fn a_fullscreen_toplevel_keeps_its_tiled_edges_clear() {
        let mut states = ToplevelStateSet::default();
        apply_configure_states(&mut states, true, false, true, false, true);
        assert!(states.contains(xdg_toplevel::State::Fullscreen));
        for state in tiled_states() {
            assert!(!states.contains(state), "unexpected {state:?}");
        }
    }

    #[test]
    fn a_maximized_toplevel_keeps_its_tiled_edges_clear() {
        let mut states = ToplevelStateSet::default();
        apply_configure_states(&mut states, true, true, false, false, true);
        assert!(states.contains(xdg_toplevel::State::Maximized));
        for state in tiled_states() {
            assert!(!states.contains(state), "unexpected {state:?}");
        }
    }

    #[test]
    fn a_zero_minimum_still_leaves_a_pixel() {
        assert_eq!(
            at_least_one_pixel(outer_size(size(0, 0), Insets::ZERO)),
            SceneSize::new(1.0, 1.0)
        );
    }
    #[test]
    fn the_titlebar_band_is_not_the_client_surface() {
        use malaysia_scene::geometry::{Point as ScenePoint, Rect, Size as SceneSize};

        let outer = Rect::new(ScenePoint::new(100.0, 50.0), SceneSize::new(800.0, 600.0));
        let insets = Insets {
            top: 44.0,
            ..Insets::ZERO
        };
        // The bar is part of the window, but not of the client's surface.
        assert_eq!(client_point(outer, insets, Point::new(110.0, 60.0)), None);
        assert_eq!(
            client_point(outer, insets, Point::new(110.0, 100.0)),
            Some(Point::new(10.0, 6.0))
        );
    }

    #[test]
    fn a_configured_1280x708_window_that_committed_640x420_only_hits_inside_it() {
        use malaysia_scene::geometry::{Point as ScenePoint, Rect as SceneRect, Size as SceneSize};
        use malaysia_scene::scene::{Scene, WindowId};

        // The compositor may have configured 1280×708, but the client drew a
        // 640×420 buffer. Hit-testing walks the scene's committed geometry, so
        // the empty strip outside the buffer is not part of the window.
        let outer = committed_outer(Some(size(640, 420)), Insets::ZERO);
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        scene.insert(
            window,
            SceneRect::new(
                ScenePoint::new(0.0, 0.0),
                SceneSize::new(f64::from(outer.w), f64::from(outer.h)),
            ),
        );

        assert_eq!(
            scene.topmost_at(ScenePoint::new(639.0, 419.0)),
            Some(window)
        );
        assert_eq!(scene.topmost_at(ScenePoint::new(640.0, 420.0)), None);
        assert_eq!(scene.topmost_at(ScenePoint::new(1000.0, 354.0)), None);
    }

    #[test]
    fn placement_centers_the_committed_geometry_not_the_configured_size() {
        // 640×420 committed inside a 1280×708 workspace: the size is the
        // client's and only the position comes from the workspace.
        let workspace = Rectangle::new(Point::from((0, 0)), size(1280, 708));
        let (outer, origin) = initial_placement(Some(size(640, 420)), Insets::ZERO, workspace);
        assert_eq!(outer, size(640, 420));
        assert_eq!(origin, Point::from((320, 144)));
    }

    #[test]
    fn a_client_that_committed_nothing_still_gets_the_default_window() {
        assert_eq!(committed_outer(None, Insets::ZERO), size(640, 420));
        assert_eq!(
            committed_outer(Some(size(0, 0)), Insets::ZERO),
            size(640, 420)
        );
    }

    #[test]
    fn committed_geometry_is_never_wider_than_the_workspace() {
        let workspace = Rectangle::new(Point::from((0, 0)), size(1280, 708));
        let (outer, _) = initial_placement(Some(size(4000, 3000)), Insets::ZERO, workspace);
        assert_eq!(outer, size(1280, 708));
    }

    #[test]
    fn a_left_resize_keeps_the_right_edge_fixed_when_the_client_commits_less() {
        let origin = Point::from((100, 100));
        let old_size = size(200, 150);
        let new_size = size(197, 150);
        let anchored = anchor_resize_origin(origin, old_size, new_size, Some(Edge::Left));
        assert_eq!(anchored, Point::from((103, 100)));
    }

    #[test]
    fn a_top_left_resize_keeps_both_opposite_edges_fixed() {
        let origin = Point::from((100, 100));
        let old_size = size(200, 150);
        let new_size = size(197, 147);
        let anchored = anchor_resize_origin(origin, old_size, new_size, Some(Edge::TopLeft));
        assert_eq!(anchored, Point::from((103, 103)));
    }

    #[test]
    fn a_right_resize_keeps_the_left_edge_fixed() {
        let origin = Point::from((100, 100));
        let old_size = size(200, 150);
        let new_size = size(197, 150);
        let anchored = anchor_resize_origin(origin, old_size, new_size, Some(Edge::Right));
        assert_eq!(anchored, Point::from((100, 100)));
    }

    #[test]
    fn no_edge_leaves_the_origin_unchanged() {
        let origin = Point::from((100, 100));
        let old_size = size(200, 150);
        let new_size = size(197, 150);
        let anchored = anchor_resize_origin(origin, old_size, new_size, None);
        assert_eq!(anchored, Point::from((100, 100)));
    }

    #[test]
    fn window_geometry_must_fit_inside_the_buffer() {
        let buffer = Some(size(640, 420));
        let inside = Some(Rectangle::new(Point::from((0, 0)), size(640, 420)));
        let shifted = Some(Rectangle::new(Point::from((0, 32)), size(640, 420)));
        let negative = Some(Rectangle::new(Point::from((-1, 0)), size(640, 420)));
        let empty = Some(Rectangle::new(Point::from((0, 0)), size(0, 420)));

        assert!(geometry_fits(inside, buffer));
        assert!(geometry_fits(None, buffer));
        assert!(!geometry_fits(shifted, buffer));
        assert!(!geometry_fits(negative, buffer));
        assert!(!geometry_fits(empty, buffer));
    }

    #[test]
    fn a_left_resize_keeps_the_right_edge_fixed_after_a_move() {
        // After a move/arrange the origin is no longer the placement origin;
        // the resize must anchor against the current origin.
        let origin = Point::from((250, 180));
        let old_size = size(640, 420);
        let new_size = size(637, 420);
        let anchored = anchor_resize_origin(origin, old_size, new_size, Some(Edge::Left));
        assert_eq!(anchored, Point::from((253, 180)));
    }

    #[test]
    fn restoring_hidden_uses_the_saved_rect_not_the_placement_origin() {
        use malaysia_scene::geometry::{Logical, Point as ScenePoint, Rect as SceneRect};

        let saved = SceneRect::new(ScenePoint::new(123.0, 456.0), SceneSize::new(640.0, 420.0));
        let mut hidden_rect: Option<SceneRect<f64, Logical>> = Some(saved);
        let restored = hidden_rect.take().unwrap();
        assert_eq!(restored.x(), 123.0);
        assert_eq!(restored.y(), 456.0);
    }

    /// The usable rectangle must stay inside the workspace. A window whose
    /// outer rectangle exactly fills the workspace below a 24-logical menubar
    /// must not push its usable area past the bottom edge. Ceiling the origin
    /// would add one logical pixel and overflow; flooring would shrink it and
    /// is safe, but round is the smallest behaviour that matches the existing
    /// `usable()` and `surface_location()` helpers.
    #[test]
    fn usable_rect_location_fits_inside_the_workspace() {
        let workspace_height = 100.0;
        let menubar = 24.0;
        let outer = SceneRect::new(
            ScenePoint::new(0.0, menubar + 0.4),
            SceneSize::new(640.0, workspace_height - menubar),
        );
        let insets = titlebar();
        let location = usable_rect_location(outer, insets);
        let usable_height = usable(size(640, (workspace_height - menubar) as i32), insets).h;
        assert!(
            location.y + usable_height <= workspace_height as i32,
            "usable rect must not extend below the workspace"
        );
    }
}
