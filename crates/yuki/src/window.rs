//! The tracked toplevels, behind one boundary.
//!
//! Geometry — where a window is and how it stacks — lives in
//! [`yuki_scene::scene`]; this module owns what the scene cannot: the
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

use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Serial, Size};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{
    Configure, SurfaceCachedState, ToplevelSurface, XdgToplevelSurfaceData,
};
use yuki_design::{NATIVE_APP_ID_PREFIX, TITLEBAR};
use yuki_scene::geometry::{
    Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
};
use yuki_scene::scene::{Decoration, Edge, PopupId, PopupParent, Scene, WindowId};

use crate::decoration::{Insets, Role, WindowState};

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
    /// Where the window was placed (the workspace origin), in logical coords.
    origin: Point<i32, Logical>,
    /// Whether the client has acknowledged a configure.
    acked: bool,
    /// The serial of the configure most recently sent, awaiting an ack.
    pending_configure: Option<Serial>,
    /// Whether the window is mapped and should be shown.
    mapped: bool,
    /// The usable size last sent, so a state change can be noticed.
    configured: Option<Size<i32, Logical>>,
    /// Whether the user is currently resizing, so the configure carries the
    /// xdg `resizing` state.
    resizing: bool,
    /// Whether Yuki's titlebar requested the maximized state.
    maximized: bool,
    /// The decoration mode the client last asked for, before policy.
    requested: Decoration,
    /// Whether the client explicitly asked for a mode (not just created the
    /// decoration object).
    mode_requested: bool,
    /// Whether the native-app warning was already emitted for this window.
    native_warned: bool,
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

/// The toplevels being tracked.
#[derive(Default)]
pub struct Windows {
    /// Where the windows are and how they stack.
    scene: Scene,
    /// The surface and configure bookkeeping, in creation order.
    windows: Vec<WindowData>,
    /// Decoration modes requested for a surface whose toplevel is not tracked
    /// yet, by surface object id. The bool says whether the client explicitly
    /// asked for the mode.
    ///
    /// GTK4 creates its KDE decoration object during realize, before it creates
    /// the xdg_toplevel at map time, so the request can arrive first. The id is
    /// stored rather than the surface so a dead surface is not kept alive.
    pending_decorations: Vec<(ObjectId, Decoration, bool)>,
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

    /// Starts tracking `toplevel` at `size` and sends its initial configure.
    ///
    /// `size` is the outer window size; the configure carries it minus the
    /// insets, so the client can size its surface for the decorated window from
    /// the start. `origin` is where the window lands (the workspace origin,
    /// below the menu bar).
    pub fn add_toplevel(
        &mut self,
        toplevel: ToplevelSurface,
        size: Size<i32, Logical>,
        origin: Point<i32, Logical>,
    ) {
        let id = WindowId::new(self.next_id);
        self.next_id += 1;
        // A mode negotiated before the toplevel existed is remembered now; the
        // caller applies the policy and sends the initial configure.
        let object = toplevel.wl_surface().id();
        let (mut requested, mut mode_requested) = (Decoration::Server, false);
        if let Some(index) = self
            .pending_decorations
            .iter()
            .position(|(pending, _, _)| *pending == object)
        {
            let (_, pending_requested, explicit) = self.pending_decorations.remove(index);
            requested = pending_requested;
            mode_requested = explicit;
        }
        self.windows.push(WindowData {
            id,
            toplevel,
            outer_size: size,
            origin,
            acked: false,
            pending_configure: None,
            mapped: false,
            configured: None,
            resizing: false,
            maximized: false,
            requested,
            mode_requested,
            native_warned: false,
        });
    }

    /// Handles a surface commit.
    ///
    /// Recomputes the insets from current state and sends a fresh configure if
    /// they changed, then maps the window once the initial configure has been
    /// acked and a buffer has been attached. Returns the window's id the first
    /// time it becomes mapped, so the caller can give it keyboard focus.
    pub fn commit(&mut self, surface: &WlSurface) -> Option<WindowId> {
        let index = self.index_of(surface)?;
        let decoration = self.decoration_of(self.windows[index].id);

        let usable = usable(
            self.windows[index].outer_size,
            self.windows[index].insets(decoration),
        );
        if self.windows[index].configured != Some(usable) {
            self.send_configure(index);
        }

        if !self.windows[index].mapped && self.windows[index].acked && surface_has_buffer(surface) {
            self.windows[index].mapped = true;
            let data = &self.windows[index];
            let size = data.outer_size;
            let origin = data.origin;
            let outer = SceneRect::new(
                ScenePoint::new(f64::from(origin.x), f64::from(origin.y)),
                SceneSize::new(f64::from(size.w), f64::from(size.h)),
            );
            let id = data.id;
            self.scene.insert(id, outer);
            tracing::info!(window = id.get(), "window mapped");
            return Some(id);
        }
        None
    }

    /// Records that the client acknowledged a configure for `surface`.
    ///
    /// Only the serial of the configure most recently sent is accepted. An ack
    /// for any other serial is stale — the client is acknowledging a configure
    /// Yuki has already replaced — so it is discarded rather than marking
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
            .retain(|(pending, _, _)| *pending != object);
        let index = self.index_of(surface)?;
        let id = self.windows.remove(index).id;
        self.scene.remove(id);
        Some(id)
    }

    /// Stops tracking the window with `id` (used by minimize and destruction).
    pub fn remove_by_id(&mut self, id: WindowId) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        self.windows.remove(index);
        self.scene.remove(id);
        true
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

    /// The scene id of the topmost window under a logical point.
    pub fn window_under(&self, point: Point<f64, Logical>) -> Option<WindowId> {
        self.scene.topmost_at(scene_point(point))
    }

    /// The topmost window under `point` and its resize edge zone, if any.
    ///
    /// Only server-decorated windows that are not maximized or fullscreen
    /// offer compositor-side resize. Client-decorated windows own their edges
    /// (ADR 0019), and a maximized/fullscreen window has no free edge to drag.
    pub fn edge_under(
        &self,
        point: Point<f64, Logical>,
        side: f64,
        corner: f64,
    ) -> Option<(WindowId, Edge)> {
        self.scene.edge_at(scene_point(point), side, corner, |id| {
            self.index(id).is_some_and(|index| {
                let data = &self.windows[index];
                self.decoration_of(id) == Decoration::Server
                    && !data.maximized
                    && data.state() == WindowState::Normal
            })
        })
    }

    /// The surface under a logical point, with the point in surface-local
    /// coordinates.
    ///
    /// Only the client's own surface counts: a point over the decorated band
    /// around it is over the window but not over the client.
    pub fn surface_under(
        &self,
        point: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let id = self.scene.topmost_at(scene_point(point))?;
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
    /// state until the grab ends.
    pub fn resize(&mut self, id: WindowId, outer: SceneRect<f64, SceneLogical>) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if !self.scene.set_outer(id, outer) {
            return false;
        }
        self.windows[index].outer_size = Size::from((round(outer.width()), round(outer.height())));
        self.send_configure(index);
        true
    }

    /// Sets whether the window is being resized and re-configures it.
    pub fn set_resizing(&mut self, id: WindowId, resizing: bool) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        self.windows[index].resizing = resizing;
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
        let location = Point::from((
            (outer.x() + insets.left).round() as i32,
            (outer.y() + insets.top).round() as i32,
        ));
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
    pub fn set_requested_decoration(
        &mut self,
        surface: &WlSurface,
        requested: Decoration,
        explicit: bool,
    ) {
        let object = surface.id();
        if let Some(index) = self.index_of(surface) {
            self.windows[index].requested = requested;
            self.windows[index].mode_requested |= explicit;
            return;
        }
        match self
            .pending_decorations
            .iter_mut()
            .find(|(pending, _, _)| *pending == object)
        {
            Some((_, stored, stored_explicit)) => {
                *stored = requested;
                *stored_explicit |= explicit;
            }
            None => self.pending_decorations.push((object, requested, explicit)),
        }
    }

    /// The decoration mode the client asked for, if known.
    pub fn requested_decoration(&self, surface: &WlSurface) -> Option<Decoration> {
        if let Some(index) = self.index_of(surface) {
            return Some(self.windows[index].requested);
        }
        let object = surface.id();
        self.pending_decorations
            .iter()
            .find(|(pending, _, _)| *pending == object)
            .map(|(_, requested, _)| *requested)
    }

    /// Whether the client explicitly requested a decoration mode.
    pub fn decoration_was_requested(&self, surface: &WlSurface) -> bool {
        self.index_of(surface)
            .is_some_and(|index| self.windows[index].mode_requested)
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

    /// Whether the window is one of Yuki's own apps (ADR 0035).
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

    /// Sends a configure carrying the usable size for the current state.
    fn send_configure(&mut self, index: usize) {
        let id = self.windows[index].id;
        let decoration = self.decoration_of(id);
        let data = &mut self.windows[index];
        let usable = usable(data.outer_size, data.insets(decoration));
        let resizing = data.resizing;
        data.toplevel.with_pending_state(|state| {
            state.size = Some(usable);
            // The configure carries the mode the client negotiated; a client
            // with no decoration object ignores the field, and one with an
            // object learns the choice before its first attach (ADR 0019).
            state.decoration_mode = Some(match decoration {
                Decoration::Server => zxdg_toplevel_decoration_v1::Mode::ServerSide,
                Decoration::Client => zxdg_toplevel_decoration_v1::Mode::ClientSide,
            });
            if resizing {
                state.states.set(xdg_toplevel::State::Resizing);
            } else {
                state.states.unset(xdg_toplevel::State::Resizing);
            }
            if data.maximized {
                state.states.set(xdg_toplevel::State::Maximized);
            } else {
                state.states.unset(xdg_toplevel::State::Maximized);
            }
        });
        data.pending_configure = Some(data.toplevel.send_configure());
        data.configured = Some(usable);
    }

    /// The outer size bounds from the client's usable minimum and maximum.
    ///
    /// A zero axis stays zero: the client set no bound on it, and for a maximum
    /// that means unbounded. `resized` reads a zero maximum that way
    /// (`yuki-scene/src/scene.rs:283-304`).
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

/// The configured size: the outer size minus the insets, never below one pixel.
fn usable(window: Size<i32, Logical>, insets: Insets) -> Size<i32, Logical> {
    let width = (f64::from(window.w) - insets.horizontal()).max(1.0).round();
    let height = (f64::from(window.h) - insets.vertical()).max(1.0).round();
    Size::from((width as i32, height as i32))
}

/// Rounds a logical length to whole pixels.
fn round(value: f64) -> i32 {
    value.round() as i32
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
    use smithay::utils::{Logical, Point, Size};

    use super::{at_least_one_pixel, client_point, outer_size, usable};
    use crate::decoration::Insets;
    use yuki_scene::geometry::Size as SceneSize;

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

    #[test]
    fn a_zero_minimum_still_leaves_a_pixel() {
        assert_eq!(
            at_least_one_pixel(outer_size(size(0, 0), Insets::ZERO)),
            SceneSize::new(1.0, 1.0)
        );
    }
    #[test]
    fn the_titlebar_band_is_not_the_client_surface() {
        use yuki_scene::geometry::{Point as ScenePoint, Rect, Size as SceneSize};

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
}
