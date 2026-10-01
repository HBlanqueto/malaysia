//! The tracked popups, behind one boundary.
//!
//! Geometry — where a popup sits and how it nests — lives in the scene
//! (`yuki_scene`, ADR 0015); this module owns what the scene cannot: the
//! `xdg_popup` surface and the configure handshake. This is the Bloque 5 work:
//! constraint-adjusted positioning, a configure before mapping, dismissal on an
//! outside click, and no orphans when a client dies.

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Size};
use smithay::wayland::shell::xdg::{PopupSurface, PositionerState};
use yuki_scene::geometry::{
    Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
};
use yuki_scene::scene::{PopupId, PopupParent};

use crate::state::State;
use crate::window::surface_has_buffer;

/// One tracked popup.
struct PopupData {
    popup: PopupSurface,
    id: PopupId,
    /// The surface the popup was created with as parent.
    parent_surface: WlSurface,
    /// The parent in scene terms, for the popup tree.
    parent: PopupParent,
    /// The absolute positioned rectangle, in logical pixels.
    geometry: Rectangle<i32, Logical>,
    /// Whether the client has attached a buffer.
    mapped: bool,
}

/// The tracked popups.
#[derive(Default)]
pub struct Popups {
    popups: Vec<PopupData>,
    /// The next scene id to hand out.
    next_id: u64,
}

impl Popups {
    /// Creates an empty set of tracked popups.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many popups are tracked.
    pub fn len(&self) -> usize {
        self.popups.len()
    }

    /// Whether no popups are tracked.
    pub fn is_empty(&self) -> bool {
        self.popups.is_empty()
    }

    /// The scene id of the popup for `surface`.
    pub fn id_of(&self, surface: &WlSurface) -> Option<PopupId> {
        self.popups
            .iter()
            .find(|data| data.popup.wl_surface() == surface)
            .map(|data| data.id)
    }

    /// The client surface of the popup with `id`.
    pub fn surface_of(&self, id: PopupId) -> Option<&WlSurface> {
        self.popups
            .iter()
            .find(|data| data.id == id)
            .map(|data| data.popup.wl_surface())
    }

    /// The `PopupSurface` with `id`.
    pub fn popup_of(&self, id: PopupId) -> Option<&PopupSurface> {
        self.popups
            .iter()
            .find(|data| data.id == id)
            .map(|data| &data.popup)
    }

    /// The index of the popup for `surface`.
    fn index_of(&self, surface: &WlSurface) -> Option<usize> {
        self.popups
            .iter()
            .position(|data| data.popup.wl_surface() == surface)
    }

    fn insert(&mut self, data: PopupData) {
        self.popups.push(data);
    }

    fn remove_id(&mut self, id: PopupId) {
        self.popups.retain(|data| data.id != id);
    }

    /// The absolute geometry of the popup for `surface`, if tracked.
    pub fn geometry_of(&self, surface: &WlSurface) -> Option<Rectangle<i32, Logical>> {
        self.index_of(surface).map(|i| self.popups[i].geometry)
    }
}

impl State {
    /// Handles `xdg_shell`'s `new_popup`: positions the popup against its
    /// parent, configures it before it maps, and tracks it.
    pub(crate) fn add_popup(&mut self, popup: PopupSurface, positioner: PositionerState) {
        let Some(parent_surface) = popup.get_parent_surface() else {
            return;
        };

        let parent_geometry = self.parent_geometry(&parent_surface);
        // The constraint target is the usable output area, relative to the
        // parent's geometry (the coordinate space `get_unconstrained_geometry`
        // works in). Panels will shrink this area once they exist.
        let usable = Rectangle::new(Point::from((0, 0)), self.logical_output_size());
        let target = Rectangle::new(usable.loc - parent_geometry.loc, usable.size);
        let relative = positioner.get_unconstrained_geometry(target);
        let absolute = Rectangle::new(parent_geometry.loc + relative.loc, relative.size);

        let parent = self.parent_kind(&parent_surface);

        popup.with_pending_state(|state| {
            state.geometry = relative;
        });
        if let Err(error) = popup.send_configure() {
            tracing::warn!(%error, "failed to configure a popup");
        }

        self.popups.insert(PopupData {
            popup,
            id: PopupId::new(self.popups.next_id),
            parent_surface,
            parent,
            geometry: absolute,
            mapped: false,
        });
        self.popups.next_id += 1;
        tracing::info!(
            popup = self.popups.next_id - 1,
            ?parent,
            ?absolute,
            "popup created"
        );
        self.request_redraw();
    }

    /// Handles a popup commit: maps it once the initial configure has been
    /// sent and a buffer attached.
    pub(crate) fn popup_commit(&mut self, surface: &WlSurface) {
        let Some(index) = self.popups.index_of(surface) else {
            return;
        };
        if !self.popups.popups[index].mapped && surface_has_buffer(surface) {
            let id = self.popups.popups[index].id;
            let parent = self.popups.popups[index].parent;
            let geometry = scene_rect(self.popups.popups[index].geometry);
            self.windows.insert_popup(id, parent, geometry);
            self.popups.popups[index].mapped = true;
            tracing::info!(popup = id.get(), "popup mapped");
            self.request_redraw();
        }
    }

    /// Handles a destroyed popup surface, removing its whole subtree.
    pub(crate) fn popup_destroyed(&mut self, surface: &WlSurface) {
        let Some(id) = self.popups.id_of(surface) else {
            return;
        };
        self.remove_popup_subtree(id);
    }

    /// Applies a `xdg_popup.reposition`: recomputes the geometry with the new
    /// positioner and confirms with `repositioned`.
    pub(crate) fn reposition_popup(
        &mut self,
        popup: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        let Some(index) = self.popups.index_of(popup.wl_surface()) else {
            return;
        };
        let parent_surface = self.popups.popups[index].parent_surface.clone();
        let parent_geometry = self.parent_geometry(&parent_surface);
        let usable = Rectangle::new(Point::from((0, 0)), self.logical_output_size());
        let target = Rectangle::new(usable.loc - parent_geometry.loc, usable.size);
        let relative = positioner.get_unconstrained_geometry(target);
        let absolute = Rectangle::new(parent_geometry.loc + relative.loc, relative.size);

        popup.with_pending_state(|state| {
            state.geometry = relative;
        });
        popup.send_repositioned(token);
        let _ = popup.send_configure();

        let id = self.popups.popups[index].id;
        self.popups.popups[index].geometry = absolute;
        if self.popups.popups[index].mapped {
            self.windows.set_popup_rect(id, scene_rect(absolute));
        }
        self.request_redraw();
    }

    /// Dismisses the popup `seed` and its whole subtree: `popup_done` to each,
    /// then they are untracked.
    pub(crate) fn dismiss_popups(&mut self, seed: PopupId) {
        let subtree = self.windows.popup_subtree(seed);
        tracing::info!(seed = seed.get(), ?subtree, "dismissing popups");
        for id in &subtree {
            if let Some(popup) = self.popups.popup_of(*id) {
                popup.send_popup_done();
            }
        }
        for id in subtree {
            self.windows.remove_popup(id);
            self.popups.remove_id(id);
        }
        self.request_redraw();
    }

    /// Dismisses every open popup.
    pub(crate) fn dismiss_all_popups(&mut self) {
        self.dismiss_topmost_popup();
    }

    /// Dismisses the topmost popup (and, with it, any submenus nested under it).
    ///
    /// This is what Esc does: close one menu level.
    pub(crate) fn dismiss_topmost_popup(&mut self) {
        let Some(topmost) = self.windows.topmost_popup() else {
            return;
        };
        self.dismiss_popups(topmost);
    }

    /// Removes `id` and its popup descendants from tracking, without sending
    /// `popup_done` (used when the client destroyed the popups itself).
    fn remove_popup_subtree(&mut self, id: PopupId) {
        let subtree = self.windows.popup_subtree(id);
        tracing::info!(root = id.get(), ?subtree, "popup subtree removed");
        for id in subtree {
            self.windows.remove_popup(id);
            self.popups.remove_id(id);
        }
        self.request_redraw();
    }

    /// The absolute geometry of a parent surface a popup anchors against.
    ///
    /// The parent may be a toplevel window (its usable rectangle — the xdg
    /// `window_geometry`), another popup, or a layer surface (a menubar menu,
    /// via `get_popup`).
    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        if let Some(window) = self.windows.id_of(parent)
            && let Some(rect) = self.windows.usable_rect(window)
        {
            return rect;
        }
        if let Some(popup) = self.popups.id_of(parent)
            && let Some(rect) = self.windows.popup_rect(popup)
        {
            return smithay_rect(rect);
        }
        if let Some(rect) = self.layers.geometry_of(parent) {
            return rect;
        }
        Rectangle::new(Point::from((0, 0)), Size::from((1, 1)))
    }

    /// The popup's parent in scene terms.
    fn parent_kind(&self, parent: &WlSurface) -> PopupParent {
        if let Some(window) = self.windows.id_of(parent) {
            PopupParent::Window(window)
        } else if let Some(popup) = self.popups.id_of(parent) {
            PopupParent::Popup(popup)
        } else {
            PopupParent::Layer
        }
    }
}

/// A scene rectangle from the compositor's logical rectangle.
fn scene_rect(rect: Rectangle<i32, Logical>) -> SceneRect<f64, SceneLogical> {
    SceneRect::new(
        ScenePoint::new(rect.loc.x as f64, rect.loc.y as f64),
        SceneSize::new(rect.size.w as f64, rect.size.h as f64),
    )
}

/// The compositor's logical rectangle from a scene rectangle.
fn smithay_rect(rect: SceneRect<f64, SceneLogical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((rect.x().round() as i32, rect.y().round() as i32)),
        Size::from((rect.width().round() as i32, rect.height().round() as i32)),
    )
}

#[cfg(test)]
mod tests {
    use super::scene_rect;
    use smithay::utils::{Point, Rectangle, Size};
    use yuki_scene::geometry::{Point as ScenePoint, Rect, Size as SceneSize};

    #[test]
    fn scene_rect_round_trips_whole_pixels() {
        let smithay = Rectangle::new(Point::from((10, 20)), Size::from((300, 200)));
        let scene = scene_rect(smithay);
        assert_eq!(
            scene,
            Rect::new(ScenePoint::new(10.0, 20.0), SceneSize::new(300.0, 200.0))
        );
    }
}
