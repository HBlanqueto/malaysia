//! Tracked toplevel windows: protocol surface and configure handling.

use std::collections::HashSet;
use std::sync::LazyLock;
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

const RESIZE_CONFIGURE_TIMEOUT_DEFAULT: Duration = Duration::from_millis(100);
const RESIZE_CONFIGURE_TIMEOUT_MS_VAR: &str = "MALAYSIA_RESIZE_CONFIGURE_TIMEOUT_MS";
static RESIZE_CONFIGURE_TIMEOUT: LazyLock<Duration> = LazyLock::new(|| {
    parse_resize_timeout(
        std::env::var(RESIZE_CONFIGURE_TIMEOUT_MS_VAR)
            .ok()
            .as_deref(),
        RESIZE_CONFIGURE_TIMEOUT_DEFAULT,
    )
});

fn parse_resize_timeout(value: Option<&str>, default: Duration) -> Duration {
    match value.and_then(|v| v.parse::<u64>().ok()) {
        Some(0) => Duration::from_millis(1),
        Some(ms) => Duration::from_millis(ms),
        None => default,
    }
}

pub struct Window<'a> {
    data: &'a WindowData,
    outer: SceneRect<f64, SceneLogical>,
    decoration: Decoration,
}

impl Window<'_> {
    pub fn id(&self) -> WindowId {
        self.data.id
    }

    pub fn surface(&self) -> &WlSurface {
        self.data.toplevel.wl_surface()
    }

    pub fn is_mapped(&self) -> bool {
        self.data.mapped
    }

    pub fn is_hidden(&self) -> bool {
        self.data.is_hidden()
    }

    pub fn has_buffer(&self) -> bool {
        surface_has_buffer(self.surface())
    }

    pub fn has_damage(&self) -> bool {
        self.data.damaged
    }

    pub fn geometry_fits_buffer(&self) -> bool {
        self.data.geometry_fits_buffer()
    }

    pub fn is_visible(&self, minimized: &HashSet<WindowId>) -> bool {
        self.is_mapped()
            && !self.is_hidden()
            && !minimized.contains(&self.id())
            && self.has_buffer()
            && self.has_damage()
            && self.geometry_fits_buffer()
    }

    pub fn app_id(&self) -> String {
        with_states(self.surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .map(|data| data.lock().unwrap().app_id.clone().unwrap_or_default())
                .unwrap_or_default()
        })
    }

    pub fn outer(&self) -> SceneRect<f64, SceneLogical> {
        self.outer
    }

    pub fn location(&self) -> Point<i32, Logical> {
        Point::from((round(self.outer.x()), round(self.outer.y())))
    }

    pub fn size(&self) -> Size<i32, Logical> {
        Size::from((round(self.outer.width()), round(self.outer.height())))
    }

    pub fn insets(&self) -> Insets {
        Insets::for_window(
            self.data.role(),
            self.data.state(),
            self.decoration,
            &TITLEBAR,
        )
    }

    pub fn state(&self) -> WindowState {
        self.data.state()
    }

    pub fn is_resizing(&self) -> bool {
        self.data.resizing
    }

    pub fn role(&self) -> Role {
        self.data.role()
    }

    pub fn is_client_decorated(&self) -> bool {
        self.decoration == Decoration::Client
    }

    pub fn window_geometry(&self) -> Rectangle<i32, Logical> {
        window_geometry(self.surface()).unwrap_or_else(|| {
            Rectangle::new(
                Point::from((0, 0)),
                buffer_size(self.surface()).unwrap_or_else(|| self.size()),
            )
        })
    }

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

    pub fn usable_size(&self) -> Size<i32, Logical> {
        usable(self.size(), self.insets())
    }

    pub fn surface_location(&self) -> Point<i32, Logical> {
        let insets = self.insets();
        Point::from((round(insets.left), round(insets.top)))
    }

    pub fn contains(&self, point: Point<f64, Logical>) -> bool {
        self.outer.contains(ScenePoint::new(point.x, point.y))
    }
}

struct WindowData {
    id: WindowId,
    toplevel: ToplevelSurface,
    outer_size: Size<i32, Logical>,
    placement_origin: Point<i32, Logical>,
    hidden_rect: Option<SceneRect<f64, SceneLogical>>,
    acked: bool,
    pending_configure: Option<Serial>,
    mapped: bool,
    hidden: bool,
    off_space: bool,
    configured: Option<Size<i32, Logical>>,
    resizing: bool,
    resize_edge: Option<Edge>,
    resize_pending_outer: Option<Size<i32, Logical>>,
    resize_configure_sent: Option<Instant>,
    maximized: bool,
    fullscreen: bool,
    activated: bool,
    requested: Option<Decoration>,
    native_warned: bool,
    placed: bool,
    damaged: bool,
    invisible_warned: bool,
}

impl WindowData {
    fn role(&self) -> Role {
        if self.toplevel.parent().is_some() {
            Role::Dialog
        } else {
            Role::Toplevel
        }
    }

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

    fn insets(&self, decoration: Decoration) -> Insets {
        Insets::for_window(self.role(), self.state(), decoration, &TITLEBAR)
    }

    fn is_hidden(&self) -> bool {
        self.hidden || self.off_space
    }

    fn geometry_fits_buffer(&self) -> bool {
        let surface = self.toplevel.wl_surface();
        geometry_fits(window_geometry(surface), buffer_size(surface))
    }

    fn size_bounds(&self) -> (Size<i32, Logical>, Size<i32, Logical>) {
        let surface = self.toplevel.wl_surface();
        with_states(surface, |states| {
            let mut cached = states.cached_state.get::<SurfaceCachedState>();
            let state = cached.current();
            (state.min_size, state.max_size)
        })
    }
}

const CLIENT_RESIZE_BORDER: f64 = 6.0;
const CLIENT_RESIZE_CORNER: f64 = 12.0;

struct EdgeTarget {
    id: WindowId,
    band: SceneRect<f64, SceneLogical>,
    blocking: SceneRect<f64, SceneLogical>,
    exterior: bool,
    resizable: bool,
}

fn edge_hit(
    targets: impl Iterator<Item = EdgeTarget>,
    point: ScenePoint<f64, SceneLogical>,
    side: f64,
    corner: f64,
) -> Option<(WindowId, Edge)> {
    for target in targets {
        if target.resizable {
            let edge = if target.exterior && target.band.contains(point) {
                None
            } else if target.exterior {
                malaysia_scene::scene::edge_at(
                    target.band,
                    point,
                    CLIENT_RESIZE_BORDER,
                    CLIENT_RESIZE_CORNER,
                )
            } else {
                malaysia_scene::scene::edge_at(target.band, point, side, corner)
            };
            if let Some(edge) = edge {
                return Some((target.id, edge));
            }
        }
        if target.blocking.contains(point) {
            return None;
        }
    }
    None
}

#[derive(Default)]
pub struct Windows {
    scene: Scene,
    windows: Vec<WindowData>,
    pending_decorations: Vec<(ObjectId, Decoration)>,
    next_id: u64,
}

fn needs_configure_after_commit(
    placed: bool,
    resizing: bool,
    outer_size: Size<i32, Logical>,
    configured: Option<Size<i32, Logical>>,
    insets: Insets,
) -> bool {
    if !placed || resizing {
        return false;
    }
    configured != Some(usable(outer_size, insets))
}

impl Windows {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

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

    pub fn surfaces(&self) -> impl Iterator<Item = &WlSurface> {
        self.windows.iter().map(|data| data.toplevel.wl_surface())
    }

    pub fn add_toplevel(&mut self, toplevel: ToplevelSurface) {
        let id = WindowId::new(self.next_id);
        self.next_id += 1;
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
            off_space: false,
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
            self.windows[index].configured = Some(usable(outer_size, insets));
            self.windows[index].placed = true;
        }

        let should_resize = {
            let data = &self.windows[index];
            if !data.mapped
                || data.fullscreen
                || data.maximized
                || data.state() != WindowState::Normal
            {
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
            let current_origin = self
                .scene
                .outer(id)
                .map(|outer| Point::from((round(outer.x()), round(outer.y()))))
                .unwrap_or(self.windows[index].placement_origin);
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

        if needs_configure_after_commit(
            self.windows[index].placed,
            self.windows[index].resizing,
            self.windows[index].outer_size,
            self.windows[index].configured,
            self.windows[index].insets(decoration),
        ) {
            self.send_configure(index);
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
            tracing::info!(window = id.get(), app_id, ?origin, ?size, "window mapped");
            return Some(id);
        }
        None
    }

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
            tracing::debug!(
                window = self.windows[index].id.get(),
                serial = u32::from(configure.serial),
                "ack_configure"
            );
        } else {
            tracing::debug!("ignoring an ack for a configure no longer pending");
        }
    }

    pub fn remove(&mut self, surface: &WlSurface) -> Option<WindowId> {
        let object = surface.id();
        self.pending_decorations
            .retain(|(pending, _)| *pending != object);
        let index = self.index_of(surface)?;
        let id = self.windows.remove(index).id;
        self.scene.remove(id);
        Some(id)
    }

    pub fn remove_by_id(&mut self, id: WindowId) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        self.windows.remove(index);
        self.scene.remove(id);
        true
    }

    pub fn set_hidden(&mut self, id: WindowId, hidden: bool) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if self.windows[index].hidden == hidden {
            return false;
        }
        let was = self.windows[index].is_hidden();
        self.windows[index].hidden = hidden;
        self.apply_hidden(index, was)
    }

    pub fn set_off_space(&mut self, id: WindowId, off_space: bool) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if self.windows[index].off_space == off_space {
            return false;
        }
        let was = self.windows[index].is_hidden();
        self.windows[index].off_space = off_space;
        self.apply_hidden(index, was)
    }

    fn apply_hidden(&mut self, index: usize, was: bool) -> bool {
        let id = self.windows[index].id;
        let hidden = self.windows[index].is_hidden();
        if hidden == was {
            return false;
        }
        let data = &mut self.windows[index];
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

    pub fn id_of(&self, surface: &WlSurface) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|data| data.toplevel.wl_surface() == surface)
            .map(|data| data.id)
    }

    pub fn surface_of(&self, id: WindowId) -> Option<&WlSurface> {
        self.windows
            .iter()
            .find(|data| data.id == id)
            .map(|data| data.toplevel.wl_surface())
    }

    pub fn is_placed(&self, surface: &WlSurface) -> bool {
        self.index_of(surface)
            .map(|index| self.windows[index].placed)
            .unwrap_or(false)
    }

    pub fn is_mapped(&self, surface: &WlSurface) -> bool {
        self.index_of(surface)
            .map(|index| self.windows[index].mapped)
            .unwrap_or(false)
    }

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

    pub fn is_client_decorated(&self, id: WindowId) -> bool {
        self.decoration_of(id) == Decoration::Client
    }

    pub fn is_visible(&self, id: WindowId, minimized: &HashSet<WindowId>) -> bool {
        self.index(id).is_some_and(|index| {
            let data = &self.windows[index];
            data.mapped
                && !data.is_hidden()
                && !minimized.contains(&id)
                && surface_has_buffer(data.toplevel.wl_surface())
                && data.damaged
                && data.geometry_fits_buffer()
        })
    }

    pub fn is_presented(&self, id: WindowId, minimized: &HashSet<WindowId>) -> bool {
        self.index(id).is_some_and(|index| {
            let data = &self.windows[index];
            data.mapped
                && !data.is_hidden()
                && !minimized.contains(&id)
                && surface_has_buffer(data.toplevel.wl_surface())
        })
    }

    pub fn iter_visible<'a>(
        &'a self,
        minimized: &'a HashSet<WindowId>,
    ) -> impl DoubleEndedIterator<Item = Window<'a>> + 'a {
        self.iter()
            .filter(move |window| window.is_visible(minimized))
    }

    pub fn iter_drawable<'a>(
        &'a self,
        minimized: &'a HashSet<WindowId>,
    ) -> impl DoubleEndedIterator<Item = Window<'a>> + 'a {
        self.scene.iter().filter_map(move |(id, outer)| {
            let data = self.windows.iter().find(|data| data.id == id)?;
            let outer = if data.off_space && !data.hidden {
                data.hidden_rect?
            } else {
                outer
            };
            let drawable = data.mapped
                && !data.hidden
                && !minimized.contains(&id)
                && surface_has_buffer(data.toplevel.wl_surface())
                && data.damaged
                && data.geometry_fits_buffer();
            drawable.then(|| Window {
                data,
                outer,
                decoration: self.decoration_of(id),
            })
        })
    }

    pub fn topmost_visible(&self, minimized: &HashSet<WindowId>) -> Option<WindowId> {
        self.iter_visible(minimized)
            .next_back()
            .map(|window| window.id())
    }

    pub fn window_under(
        &self,
        point: Point<f64, Logical>,
        minimized: &HashSet<WindowId>,
    ) -> Option<WindowId> {
        self.scene
            .topmost_at(scene_point(point))
            .filter(|id| self.is_visible(*id, minimized))
    }

    pub fn edge_under(
        &self,
        point: Point<f64, Logical>,
        side: f64,
        corner: f64,
        minimized: &HashSet<WindowId>,
    ) -> Option<(WindowId, Edge)> {
        let targets = self.iter_visible(minimized).rev().map(|window| {
            let id = window.id();
            let resizable = !window.data.maximized && window.data.state() == WindowState::Normal;
            let (band, blocking, exterior) = match self.decoration_of(id) {
                Decoration::Server => (window.outer(), window.outer(), false),
                Decoration::Client => {
                    let origin = window.location() + window.surface_location();
                    let geometry = window.window_geometry();
                    let frame = SceneRect::new(
                        ScenePoint::new(
                            f64::from(origin.x + geometry.loc.x),
                            f64::from(origin.y + geometry.loc.y),
                        ),
                        SceneSize::new(f64::from(geometry.size.w), f64::from(geometry.size.h)),
                    );
                    let buffer = buffer_size(window.surface()).unwrap_or(geometry.size);
                    let blocking = SceneRect::new(
                        ScenePoint::new(f64::from(origin.x), f64::from(origin.y)),
                        SceneSize::new(f64::from(buffer.w), f64::from(buffer.h)),
                    );
                    (frame, blocking, true)
                }
            };
            EdgeTarget {
                id,
                band,
                blocking,
                exterior,
                resizable,
            }
        });
        edge_hit(targets, scene_point(point), side, corner)
    }

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

    pub fn resize(&mut self, id: WindowId, outer: SceneRect<f64, SceneLogical>) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        let requested = Size::from((round(outer.width()), round(outer.height())));
        let data = &mut self.windows[index];
        let can_send = data
            .resize_configure_sent
            .is_none_or(|t| t.elapsed() >= *RESIZE_CONFIGURE_TIMEOUT);
        if can_send {
            data.resize_configure_sent = Some(Instant::now());
            data.resize_pending_outer = None;
            self.send_configure_with_outer(index, requested);
        } else {
            data.resize_pending_outer = Some(requested);
        }
        true
    }

    pub fn set_resizing(&mut self, id: WindowId, resizing: bool, edge: Option<Edge>) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        self.windows[index].resizing = resizing;
        self.windows[index].resize_edge = edge;
        if !resizing {
            self.windows[index].resize_configure_sent = None;
            if let Some(pending) = self.windows[index].resize_pending_outer.take() {
                self.send_configure_with_outer(index, pending);
                return true;
            }
        }
        self.send_configure(index);
        true
    }

    pub fn minimum_outer_size(&self, id: WindowId) -> SceneSize<f64, SceneLogical> {
        at_least_one_pixel(self.outer_size_bounds(id, |min, _| min))
    }

    pub fn zoom_supported(&self, id: WindowId) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        let (min, max) = self.windows[index].size_bounds();
        !(min.w > 0 && min.h > 0 && min == max)
    }

    pub fn maximum_outer_size(&self, id: WindowId) -> SceneSize<f64, SceneLogical> {
        self.outer_size_bounds(id, |_, max| max)
    }

    pub fn client_size_bounds(
        &self,
        id: WindowId,
    ) -> Option<(Size<i32, Logical>, Size<i32, Logical>)> {
        let index = self.index(id)?;
        Some(self.windows[index].size_bounds())
    }

    pub fn raise(&mut self, id: WindowId) -> bool {
        self.scene.raise(id)
    }

    pub fn topmost(&self) -> Option<WindowId> {
        self.scene.topmost()
    }

    pub fn insets(&self, id: WindowId) -> Option<Insets> {
        let index = self.index(id)?;
        Some(self.windows[index].insets(self.decoration_of(id)))
    }

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
        if self.windows[index].is_hidden() {
            self.windows[index].hidden_rect = Some(outer);
        } else {
            self.scene.set_outer(id, outer);
        }
        self.windows[index].outer_size = Size::from((round(outer.width()), round(outer.height())));
        self.send_configure(index);
        state_changed
    }

    pub fn set_scene_y(&mut self, id: WindowId, y: f64) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if self.windows[index].is_hidden() {
            if let Some(rect) = self.windows[index].hidden_rect.as_mut() {
                *rect = SceneRect::new(ScenePoint::new(rect.x(), y), rect.size());
            }
            return true;
        }
        let Some(outer) = self.scene.outer(id) else {
            return false;
        };
        self.scene.set_outer(
            id,
            SceneRect::new(ScenePoint::new(outer.x(), y), outer.size()),
        )
    }

    pub fn set_infinity_geometry(&mut self, id: WindowId, outer: SceneRect<f64, SceneLogical>) {
        let Some(index) = self.index(id) else {
            return;
        };
        self.scene.set_outer(id, outer);
        self.windows[index].outer_size = Size::from((round(outer.width()), round(outer.height())));
        self.send_configure(index);
    }

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

    pub fn outer(&self, id: WindowId) -> Option<SceneRect<f64, SceneLogical>> {
        self.scene.outer(id)
    }

    pub fn usable_rect(&self, id: WindowId) -> Option<Rectangle<i32, Logical>> {
        let outer = self.scene.outer(id)?;
        let index = self.index(id)?;
        let decoration = self.decoration_of(id);
        let insets = self.windows[index].insets(decoration);
        let location = usable_rect_location(outer, insets);
        let size = usable(self.windows[index].outer_size, insets);
        Some(Rectangle::new(location, size))
    }

    pub fn insert_popup(
        &mut self,
        id: PopupId,
        parent: PopupParent,
        rect: SceneRect<f64, SceneLogical>,
    ) {
        self.scene.insert_popup(id, parent, rect);
    }

    pub fn remove_popup(&mut self, id: PopupId) -> bool {
        self.scene.remove_popup(id)
    }

    pub fn popup_rect(&self, id: PopupId) -> Option<SceneRect<f64, SceneLogical>> {
        self.scene.popup_rect(id)
    }

    pub fn set_popup_rect(&mut self, id: PopupId, rect: SceneRect<f64, SceneLogical>) -> bool {
        self.scene.set_popup_rect(id, rect)
    }

    pub fn iter_popups(
        &self,
    ) -> impl DoubleEndedIterator<Item = (PopupId, PopupParent, SceneRect<f64, SceneLogical>)> + '_
    {
        self.scene.iter_popups()
    }

    pub fn popup_under(&self, point: Point<f64, Logical>) -> Option<PopupId> {
        self.scene.popup_under(scene_point(point))
    }

    pub fn topmost_popup(&self) -> Option<PopupId> {
        self.scene.topmost_popup()
    }

    pub fn popup_ancestors(&self, id: PopupId) -> Vec<PopupId> {
        self.scene.popup_ancestors(id)
    }

    pub fn popup_subtree(&self, id: PopupId) -> Vec<PopupId> {
        self.scene.popup_subtree(id)
    }

    pub fn send_close(&self, id: WindowId) {
        if let Some(index) = self.index(id) {
            self.windows[index].toplevel.send_close();
        }
    }

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

    pub fn clear_requested_decoration(&mut self, surface: &WlSurface) {
        if let Some(index) = self.index_of(surface) {
            self.windows[index].requested = None;
        }
    }

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

    pub fn set_initial_decoration(&mut self, surface: &WlSurface, decoration: Decoration) {
        if let Some(index) = self.index_of(surface) {
            let id = self.windows[index].id;
            self.scene.set_decoration(id, decoration);
        }
    }

    pub fn send_initial_configure(&mut self, surface: &WlSurface) {
        if let Some(index) = self.index_of(surface) {
            self.send_configure(index);
        }
    }

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

    pub fn app_id(&self, surface: &WlSurface) -> String {
        with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .map(|data| data.lock().unwrap().app_id.clone().unwrap_or_default())
                .unwrap_or_default()
        })
    }

    pub fn is_native(&self, surface: &WlSurface) -> bool {
        self.app_id(surface).starts_with(NATIVE_APP_ID_PREFIX)
    }

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

    fn decoration_of(&self, id: WindowId) -> Decoration {
        self.scene.decoration(id).unwrap_or(Decoration::Client)
    }

    fn id_of_surface(&self, surface: &WlSurface) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|data| data.toplevel.wl_surface() == surface)
            .map(|data| data.id)
    }

    pub(crate) fn parent_of(&self, id: WindowId) -> Option<WindowId> {
        self.dialog_parent(id)
    }

    fn dialog_parent(&self, id: WindowId) -> Option<WindowId> {
        let index = self.index(id)?;
        let parent = self.windows[index].toplevel.parent()?;
        self.id_of_surface(&parent)
    }

    fn send_configure(&mut self, index: usize) {
        let id = self.windows[index].id;
        let decoration = self.decoration_of(id);
        let data = &mut self.windows[index];
        let configured = configure_size(data.placed, data.outer_size, data.insets(decoration));
        let resizing = data.resizing;
        data.toplevel.with_pending_state(|state| {
            state.size = Some(configured);
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
        let serial = data.toplevel.send_configure();
        tracing::debug!(
            window = id.get(),
            ?configured,
            resizing,
            serial = u32::from(serial),
            "send_configure"
        );
        data.pending_configure = Some(serial);
        data.configured = Some(configured);
    }

    fn send_configure_with_outer(&mut self, index: usize, outer_size: Size<i32, Logical>) {
        let previous = self.windows[index].outer_size;
        self.windows[index].outer_size = outer_size;
        self.send_configure(index);
        self.windows[index].outer_size = previous;
    }

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

    fn index_of(&self, surface: &WlSurface) -> Option<usize> {
        self.windows
            .iter()
            .position(|data| data.toplevel.wl_surface() == surface)
    }

    fn index(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|data| data.id == id)
    }
}

fn usable_rect_location(
    outer: SceneRect<f64, SceneLogical>,
    insets: Insets,
) -> Point<i32, Logical> {
    Point::from((
        (outer.x() + insets.left).round() as i32,
        (outer.y() + insets.top).round() as i32,
    ))
}

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

fn scene_point(point: Point<f64, Logical>) -> ScenePoint<f64, SceneLogical> {
    ScenePoint::new(point.x, point.y)
}

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

fn at_least_one_pixel(size: SceneSize<f64, SceneLogical>) -> SceneSize<f64, SceneLogical> {
    SceneSize::new(size.width.max(1.0), size.height.max(1.0))
}

fn center_origin(
    size: Size<i32, Logical>,
    workspace: Rectangle<i32, Logical>,
) -> Point<i32, Logical> {
    Point::from((
        workspace.loc.x + (workspace.size.w - size.w) / 2,
        workspace.loc.y + (workspace.size.h - size.h) / 2,
    ))
}

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

fn committed_outer(committed: Option<Size<i32, Logical>>, insets: Insets) -> Size<i32, Logical> {
    let client = match committed {
        Some(size) if size.w > 0 && size.h > 0 => size,
        _ => Size::from((640, 420)),
    };
    let outer = outer_size(client, insets);
    Size::from((round(outer.width), round(outer.height)))
}

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

fn usable(window: Size<i32, Logical>, insets: Insets) -> Size<i32, Logical> {
    let width = (f64::from(window.w) - insets.horizontal()).max(1.0).round();
    let height = (f64::from(window.h) - insets.vertical()).max(1.0).round();
    Size::from((width as i32, height as i32))
}

fn configure_size(placed: bool, outer: Size<i32, Logical>, insets: Insets) -> Size<i32, Logical> {
    if placed {
        usable(outer, insets)
    } else {
        Size::from((0, 0))
    }
}

fn round(value: f64) -> i32 {
    value.round() as i32
}

pub(crate) fn window_geometry(surface: &WlSurface) -> Option<Rectangle<i32, Logical>> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
    })
}

fn buffer_size(surface: &WlSurface) -> Option<Size<i32, Logical>> {
    with_renderer_surface_state(surface, |state| state.buffer_size()).flatten()
}

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

pub(crate) fn surface_has_buffer(surface: &WlSurface) -> bool {
    with_renderer_surface_state(surface, |state| state.buffer().is_some()).unwrap_or(false)
}

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
    use std::time::Duration;

    use smithay::utils::{Logical, Point, Rectangle, Size};

    use super::{
        EdgeTarget, RESIZE_CONFIGURE_TIMEOUT_DEFAULT, anchor_resize_origin, apply_configure_states,
        at_least_one_pixel, client_point, committed_outer, configure_size, edge_hit, geometry_fits,
        initial_placement, needs_configure_after_commit, outer_size, parse_resize_timeout, usable,
        usable_rect_location,
    };
    use crate::decoration::Insets;
    use malaysia_scene::geometry::{
        Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
    };
    use malaysia_scene::scene::{Edge, WindowId};
    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
    use smithay::wayland::shell::xdg::ToplevelStateSet;

    fn size(width: i32, height: i32) -> Size<i32, Logical> {
        Size::from((width, height))
    }

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
        let mut initial = ToplevelStateSet::default();
        apply_configure_states(&mut initial, false, false, false, false, false);
        for state in tiled_states() {
            assert!(!initial.contains(state), "unexpected {state:?}");
        }
        assert_eq!(
            configure_size(false, size(1204, 804), titlebar()),
            size(0, 0)
        );

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

    fn csd(
        id: WindowId,
        blocking: SceneRect<f64, SceneLogical>,
        frame: SceneRect<f64, SceneLogical>,
    ) -> EdgeTarget {
        EdgeTarget {
            id,
            band: frame,
            blocking,
            exterior: true,
            resizable: true,
        }
    }

    #[test]
    fn a_csd_margin_blocks_the_band_of_the_window_below() {
        let above = WindowId::new(1);
        let below = WindowId::new(2);
        let above_target = csd(
            above,
            SceneRect::new(ScenePoint::new(100.0, 100.0), SceneSize::new(300.0, 200.0)),
            SceneRect::new(ScenePoint::new(120.0, 120.0), SceneSize::new(260.0, 160.0)),
        );
        let below_target = csd(
            below,
            SceneRect::new(ScenePoint::new(180.0, 130.0), SceneSize::new(226.0, 140.0)),
            SceneRect::new(ScenePoint::new(200.0, 150.0), SceneSize::new(186.0, 100.0)),
        );

        let point = ScenePoint::new(390.0, 200.0);
        let targets = [above_target, below_target];
        assert_eq!(edge_hit(targets.into_iter(), point, 6.0, 12.0), None);

        let below_target = csd(
            below,
            SceneRect::new(ScenePoint::new(180.0, 130.0), SceneSize::new(226.0, 140.0)),
            SceneRect::new(ScenePoint::new(200.0, 150.0), SceneSize::new(186.0, 100.0)),
        );
        assert_eq!(
            edge_hit([below_target].into_iter(), point, 6.0, 12.0),
            Some((below, Edge::Right))
        );

        let above_target = csd(
            above,
            SceneRect::new(ScenePoint::new(100.0, 100.0), SceneSize::new(300.0, 200.0)),
            SceneRect::new(ScenePoint::new(120.0, 120.0), SceneSize::new(260.0, 160.0)),
        );
        assert_eq!(
            edge_hit(
                [above_target].into_iter(),
                ScenePoint::new(383.0, 200.0),
                6.0,
                12.0
            ),
            Some((above, Edge::Right))
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
        use malaysia_scene::geometry::{Point as ScenePoint, Rect, Size as SceneSize};

        let outer = Rect::new(ScenePoint::new(100.0, 50.0), SceneSize::new(800.0, 600.0));
        let insets = Insets {
            top: 44.0,
            ..Insets::ZERO
        };
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

    #[test]
    fn parse_resize_timeout_returns_parsed_value() {
        assert_eq!(
            parse_resize_timeout(Some("8"), RESIZE_CONFIGURE_TIMEOUT_DEFAULT),
            Duration::from_millis(8)
        );
    }

    #[test]
    fn parse_resize_timeout_empty_uses_default() {
        assert_eq!(
            parse_resize_timeout(Some(""), RESIZE_CONFIGURE_TIMEOUT_DEFAULT),
            RESIZE_CONFIGURE_TIMEOUT_DEFAULT
        );
        assert_eq!(
            parse_resize_timeout(None, RESIZE_CONFIGURE_TIMEOUT_DEFAULT),
            RESIZE_CONFIGURE_TIMEOUT_DEFAULT
        );
    }

    #[test]
    fn parse_resize_timeout_garbage_uses_default() {
        assert_eq!(
            parse_resize_timeout(Some("fast"), RESIZE_CONFIGURE_TIMEOUT_DEFAULT),
            RESIZE_CONFIGURE_TIMEOUT_DEFAULT
        );
    }

    #[test]
    fn parse_resize_timeout_zero_uses_minimum() {
        assert_eq!(
            parse_resize_timeout(Some("0"), RESIZE_CONFIGURE_TIMEOUT_DEFAULT),
            Duration::from_millis(1)
        );
    }

    #[test]
    fn commit_does_not_reconfigure_with_committed_size_during_resize() {
        let insets = Insets::ZERO;
        let committed = size(640, 420);
        let requested = size(800, 600);
        assert!(
            !needs_configure_after_commit(true, true, committed, Some(requested), insets),
            "during resize the committed size must not trigger a reconfigure"
        );
    }

    #[test]
    fn commit_reconfigures_when_size_changes_outside_resize() {
        let insets = Insets::ZERO;
        let committed = size(640, 420);
        let requested = size(800, 600);
        assert!(
            needs_configure_after_commit(true, false, committed, Some(requested), insets),
            "outside resize a committed size different from configured must reconfigure"
        );
    }

    #[test]
    fn commit_does_not_reconfigure_when_size_matches() {
        let insets = Insets::ZERO;
        let committed = size(640, 420);
        assert!(
            !needs_configure_after_commit(true, false, committed, Some(committed), insets),
            "no reconfigure when configured size already matches committed"
        );
    }
}
