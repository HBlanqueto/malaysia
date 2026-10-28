//! The ordered set of windows, and the geometry queries over it.
//!
//! `malaysia-scene` cannot depend on Smithay, so a window here is only its
//! identity and its decorated outer rectangle. The compositor keeps the
//! protocol surface and the configure bookkeeping; this module owns stacking
//! order, point hit-testing and the resize edge zones. See ADR 0015.
//!
//! The rectangle is the **decorated** bounds — titlebar and border included —
//! not the client's surface: the two do not coincide (ADR 0014), and the edge
//! zones that start a resize are measured against the decorated bounds, so a
//! grab near the titlebar works even though no client surface is there.

use crate::geometry::{Logical, Point, Rect, Size};

/// Identifies one window in a [`Scene`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(u64);

impl WindowId {
    /// Creates an id from a compositor-chosen number.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number this id was created from.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Identifies one popup in a [`Scene`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PopupId(u64);

impl PopupId {
    /// Creates an id from a compositor-chosen number.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number this id was created from.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// What a popup is attached to: the surface it was created with as parent.
///
/// A popup's parent is either a toplevel window, another popup (a nested
/// submenu), or a layer surface (a menubar menu). Popups are always stacked
/// above every window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupParent {
    /// A toplevel window.
    Window(WindowId),
    /// Another popup.
    Popup(PopupId),
    /// A layer surface (menubar or dock).
    Layer,
}

/// One side or corner of a decorated window's outer rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// The top-left corner.
    TopLeft,
    /// The top-right corner.
    TopRight,
    /// The bottom-left corner.
    BottomLeft,
    /// The bottom-right corner.
    BottomRight,
}

/// Who draws a window's decorations.
///
/// This is stored with the window, keyed by [`WindowId`], and is the one value
/// the compositor reads for drawing, hit-testing and decoration insets. What a
/// window's mode *is* — negotiation, defaults, changes — is the compositor's
/// policy (ADR 0019); this crate only stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoration {
    /// The compositor draws the window's chrome.
    Server,
    /// The client draws its own chrome, or asked not to be decorated.
    Client,
}

/// A stack of windows, back to front.
///
/// The last window is on top. Every query considers windows in stacking order,
/// so what the user sees on top is also what the pointer hits.
#[derive(Debug, Default)]
pub struct Scene {
    /// Back-to-front entries; the last is topmost.
    windows: Vec<Entry>,
    /// Decoration modes, by window id.
    ///
    /// A separate table because a window negotiates its decorations *before* it
    /// maps, and a scene entry only exists once it does: the mode must be
    /// storable for an id that has no rectangle yet.
    decorations: Vec<(WindowId, Decoration)>,
    /// Popups, back to front; the last is topmost.
    ///
    /// Popups are always above every window: an open menu is above its parent
    /// window no matter how the windows are stacked (ADR 0015, Bloque 5).
    popups: Vec<PopupEntry>,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    id: WindowId,
    outer: Rect<f64, Logical>,
}

#[derive(Debug, Clone, Copy)]
struct PopupEntry {
    id: PopupId,
    parent: PopupParent,
    rect: Rect<f64, Logical>,
}

impl Scene {
    /// Creates an empty scene.
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

    /// Adds a window on top of the stack.
    pub fn insert(&mut self, id: WindowId, outer: Rect<f64, Logical>) {
        self.windows.push(Entry { id, outer });
    }

    /// Removes a window, returning whether it was present.
    ///
    /// The stored decoration goes with it.
    pub fn remove(&mut self, id: WindowId) -> bool {
        let before = self.windows.len();
        self.windows.retain(|entry| entry.id != id);
        self.decorations.retain(|(entry, _)| *entry != id);
        self.windows.len() != before
    }

    /// Stores a window's decoration mode, returning whether it changed.
    ///
    /// The window does not need a scene entry yet: a client negotiates before
    /// it maps, and the mode has to be readable from the first configure on.
    pub fn set_decoration(&mut self, id: WindowId, decoration: Decoration) -> bool {
        match self.decorations.iter_mut().find(|(entry, _)| *entry == id) {
            Some((_, stored)) if *stored == decoration => false,
            Some((_, stored)) => {
                *stored = decoration;
                true
            }
            None => {
                self.decorations.push((id, decoration));
                true
            }
        }
    }

    /// The decoration mode stored for a window, if one was set.
    pub fn decoration(&self, id: WindowId) -> Option<Decoration> {
        self.decorations
            .iter()
            .find(|(entry, _)| *entry == id)
            .map(|(_, decoration)| *decoration)
    }

    /// Moves a window to the top of the stack.
    pub fn raise(&mut self, id: WindowId) -> bool {
        let Some(index) = self.windows.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let entry = self.windows.remove(index);
        self.windows.push(entry);
        true
    }

    /// The decorated outer rectangle of a window.
    pub fn outer(&self, id: WindowId) -> Option<Rect<f64, Logical>> {
        self.windows
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.outer)
    }

    /// Replaces a window's decorated outer rectangle.
    pub fn set_outer(&mut self, id: WindowId, outer: Rect<f64, Logical>) -> bool {
        let Some(entry) = self.windows.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.outer = outer;
        true
    }

    /// Every window, back to front, so the last is drawn on top.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (WindowId, Rect<f64, Logical>)> + '_ {
        self.windows.iter().map(|entry| (entry.id, entry.outer))
    }

    /// The topmost window whose decorated rectangle contains `point`.
    pub fn topmost_at(&self, point: Point<f64, Logical>) -> Option<WindowId> {
        self.windows
            .iter()
            .rev()
            .find(|entry| entry.outer.contains(point))
            .map(|entry| entry.id)
    }

    /// The topmost window whose resize zone contains `point`, if any.
    ///
    /// `side` is the exterior band along each edge and `corner` is the length
    /// of the exterior L at each corner, both in logical pixels. A point in a
    /// corner L returns the corner, not one side.
    ///
    /// The search is top-to-bottom. A point inside a window's outer rectangle
    /// but not in its 2 px corner inset is content: it stops the search, so the
    /// exterior band of a lower window never steals input from the content of
    /// one above. A point outside every outer rectangle may still hit the
    /// exterior band of the topmost window whose band reaches it.
    pub fn edge_at(
        &self,
        point: Point<f64, Logical>,
        side: f64,
        corner: f64,
        eligible: impl Fn(WindowId) -> bool,
    ) -> Option<(WindowId, Edge)> {
        self.edge_at_with(point, |id| eligible(id).then_some((side, corner, false)))
    }

    /// Like [`Self::edge_at`], but each eligible window chooses its own band.
    ///
    /// `eligible` returns `(side, corner, exterior_only)`. An exterior-only
    /// band never matches inside the window rectangle, so a client that owns
    /// its interior keeps every click there.
    pub fn edge_at_with(
        &self,
        point: Point<f64, Logical>,
        eligible: impl Fn(WindowId) -> Option<(f64, f64, bool)>,
    ) -> Option<(WindowId, Edge)> {
        for entry in self.windows.iter().rev() {
            if let Some((side, corner, exterior_only)) = eligible(entry.id)
                && (!exterior_only || !entry.outer.contains(point))
                && let Some(edge) = edge_in(entry.outer, point, side, corner)
            {
                return Some((entry.id, edge));
            }
            if entry.outer.contains(point) {
                return None;
            }
        }
        None
    }

    /// The topmost window, if any.
    pub fn topmost(&self) -> Option<WindowId> {
        self.windows.last().map(|entry| entry.id)
    }

    /// Adds a popup on top of the popup stack.
    pub fn insert_popup(&mut self, id: PopupId, parent: PopupParent, rect: Rect<f64, Logical>) {
        self.popups.push(PopupEntry { id, parent, rect });
    }

    /// Removes a popup, returning whether it was present.
    pub fn remove_popup(&mut self, id: PopupId) -> bool {
        let before = self.popups.len();
        self.popups.retain(|entry| entry.id != id);
        self.popups.len() != before
    }

    /// A popup's positioned rectangle.
    pub fn popup_rect(&self, id: PopupId) -> Option<Rect<f64, Logical>> {
        self.popups
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.rect)
    }

    /// Replaces a popup's positioned rectangle.
    pub fn set_popup_rect(&mut self, id: PopupId, rect: Rect<f64, Logical>) -> bool {
        let Some(entry) = self.popups.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.rect = rect;
        true
    }

    /// A popup's parent, if it is tracked.
    pub fn popup_parent(&self, id: PopupId) -> Option<PopupParent> {
        self.popups
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.parent)
    }

    /// Every popup, back to front, so the last is drawn on top.
    pub fn iter_popups(
        &self,
    ) -> impl DoubleEndedIterator<Item = (PopupId, PopupParent, Rect<f64, Logical>)> + '_ {
        self.popups
            .iter()
            .map(|entry| (entry.id, entry.parent, entry.rect))
    }

    /// The topmost popup, if any.
    pub fn topmost_popup(&self) -> Option<PopupId> {
        self.popups.last().map(|entry| entry.id)
    }

    /// The topmost popup whose rectangle contains `point`.
    ///
    /// Popups are above windows, so a point over an open menu is over the menu
    /// even when a window sits underneath.
    pub fn popup_under(&self, point: Point<f64, Logical>) -> Option<PopupId> {
        self.popups
            .iter()
            .rev()
            .find(|entry| entry.rect.contains(point))
            .map(|entry| entry.id)
    }

    /// The popup ancestors of `id`, from the immediate parent upwards.
    ///
    /// `id` itself is not included. Used to dismiss a whole menu tree: closing
    /// a popup's parent closes the popup, so the ancestors of a popup are the
    /// popups that must go with it.
    pub fn popup_ancestors(&self, id: PopupId) -> Vec<PopupId> {
        let mut chain = Vec::new();
        let mut current = id;
        while let Some(parent) = self.popup_parent(current) {
            match parent {
                PopupParent::Popup(parent) => {
                    chain.push(parent);
                    current = parent;
                }
                PopupParent::Window(_) | PopupParent::Layer => break,
            }
        }
        chain
    }

    /// Every popup whose ancestry leads to `id` (including `id` itself).
    ///
    /// Used to dismiss a whole subtree: dismissing a popup takes its nested
    /// submenus with it.
    pub fn popup_subtree(&self, id: PopupId) -> Vec<PopupId> {
        let mut subtree = vec![id];
        loop {
            let before = subtree.len();
            for entry in &self.popups {
                if let PopupParent::Popup(parent) = entry.parent
                    && subtree.contains(&parent)
                    && !subtree.contains(&entry.id)
                {
                    subtree.push(entry.id);
                }
            }
            if subtree.len() == before {
                break;
            }
        }
        subtree
    }
}

/// How far a corner L may enter the window, in logical pixels.
const CORNER_INSET: f64 = 2.0;

/// Which exterior resize zone of `outer` contains `point`, if any.
///
/// Sides are an exterior band of `side`; corners are an exterior L of length
/// `corner` along each side, with a 2 px inward tolerance. There are no large
/// squares inside the content: a point 30 px inside a corner is client area.
fn edge_in(
    outer: Rect<f64, Logical>,
    point: Point<f64, Logical>,
    side: f64,
    corner: f64,
) -> Option<Edge> {
    let top_arm = point.y >= outer.y() - side && point.y <= outer.y() + CORNER_INSET;
    let bottom_arm = point.y >= outer.bottom() - CORNER_INSET && point.y <= outer.bottom() + side;
    let left_arm = point.x >= outer.x() - side && point.x <= outer.x() + CORNER_INSET;
    let right_arm = point.x >= outer.right() - CORNER_INSET && point.x <= outer.right() + side;

    if (top_arm && point.x >= outer.x() - side && point.x <= outer.x() + corner)
        || (left_arm && point.y >= outer.y() - side && point.y <= outer.y() + corner)
    {
        return Some(Edge::TopLeft);
    }
    if (top_arm && point.x >= outer.right() - corner && point.x <= outer.right() + side)
        || (right_arm && point.y >= outer.y() - side && point.y <= outer.y() + corner)
    {
        return Some(Edge::TopRight);
    }
    if (bottom_arm && point.x >= outer.x() - side && point.x <= outer.x() + corner)
        || (left_arm && point.y >= outer.bottom() - corner && point.y <= outer.bottom() + side)
    {
        return Some(Edge::BottomLeft);
    }
    if (bottom_arm && point.x >= outer.right() - corner && point.x <= outer.right() + side)
        || (right_arm && point.y >= outer.bottom() - corner && point.y <= outer.bottom() + side)
    {
        return Some(Edge::BottomRight);
    }

    let top = point.y >= outer.y() - side && point.y <= outer.y() + CORNER_INSET;
    let bottom = point.y >= outer.bottom() - CORNER_INSET && point.y <= outer.bottom() + side;
    let left = point.x >= outer.x() - side && point.x <= outer.x() + CORNER_INSET;
    let right = point.x >= outer.right() - CORNER_INSET && point.x <= outer.right() + side;

    if top && point.x >= outer.x() + corner && point.x <= outer.right() - corner {
        return Some(Edge::Top);
    }
    if bottom && point.x >= outer.x() + corner && point.x <= outer.right() - corner {
        return Some(Edge::Bottom);
    }
    if left && point.y >= outer.y() + corner && point.y <= outer.bottom() - corner {
        return Some(Edge::Left);
    }
    if right && point.y >= outer.y() + corner && point.y <= outer.bottom() - corner {
        return Some(Edge::Right);
    }
    None
}

/// A resize edge strictly outside `outer`, never inside the client content.
pub fn exterior_edge(
    outer: Rect<f64, Logical>,
    point: Point<f64, Logical>,
    side: f64,
    corner: f64,
) -> Option<Edge> {
    if outer.contains(point) {
        None
    } else {
        edge_in(outer, point, side, corner)
    }
}

/// The outer rectangle produced by dragging one edge or corner of `start`.
///
/// `dx` and `dy` are the pointer's movement since the drag began, in logical
/// pixels. Dragging a left or top edge moves the origin and keeps the opposite
/// edge fixed. The size is clamped to `minimum` on both axes and to `maximum`
/// where a maximum axis is non-zero (zero means unbounded); clamping a left or
/// top edge again keeps the opposite edge fixed.
pub fn resized(
    start: Rect<f64, Logical>,
    edge: Edge,
    dx: f64,
    dy: f64,
    minimum: Size<f64, Logical>,
    maximum: Size<f64, Logical>,
) -> Rect<f64, Logical> {
    let left = matches!(edge, Edge::Left | Edge::TopLeft | Edge::BottomLeft);
    let right = matches!(edge, Edge::Right | Edge::TopRight | Edge::BottomRight);
    let top = matches!(edge, Edge::Top | Edge::TopLeft | Edge::TopRight);
    let bottom = matches!(edge, Edge::Bottom | Edge::BottomLeft | Edge::BottomRight);

    let mut x = start.x();
    let mut y = start.y();
    let mut width = start.width();
    let mut height = start.height();
    if left {
        x += dx;
        width -= dx;
    }
    if right {
        width += dx;
    }
    if top {
        y += dy;
        height -= dy;
    }
    if bottom {
        height += dy;
    }

    width = clamp_axis(
        width,
        minimum.width,
        maximum.width,
        left,
        start.right(),
        &mut x,
    );
    height = clamp_axis(
        height,
        minimum.height,
        maximum.height,
        top,
        start.bottom(),
        &mut y,
    );

    Rect::new(Point::new(x, y), Size::new(width, height))
}

/// Clamps one axis' extent, moving `origin` so a leading-edge drag keeps the
/// opposite edge fixed.
fn clamp_axis(
    extent: f64,
    minimum: f64,
    maximum: f64,
    leading: bool,
    far_edge: f64,
    origin: &mut f64,
) -> f64 {
    let extent = if extent < minimum {
        minimum
    } else if maximum > 0.0 && extent > maximum {
        maximum
    } else {
        extent
    };
    if leading {
        *origin = far_edge - extent;
    }
    extent
}

#[cfg(test)]
mod tests {
    use crate::geometry::{Logical, Point, Rect, Size};

    use super::{Decoration, Edge, PopupId, PopupParent, Scene, WindowId, resized};

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect<f64, Logical> {
        Rect::new(Point::new(x, y), Size::new(w, h))
    }

    fn point(x: f64, y: f64) -> Point<f64, Logical> {
        Point::new(x, y)
    }

    #[test]
    fn the_last_inserted_window_is_on_top() {
        let mut scene = Scene::new();
        let first = WindowId::new(1);
        let second = WindowId::new(2);
        scene.insert(first, rect(0.0, 0.0, 100.0, 100.0));
        scene.insert(second, rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(scene.topmost(), Some(second));
        assert_eq!(scene.topmost_at(point(50.0, 50.0)), Some(second));
    }

    #[test]
    fn raise_moves_a_window_to_the_top() {
        let mut scene = Scene::new();
        let first = WindowId::new(1);
        let second = WindowId::new(2);
        scene.insert(first, rect(0.0, 0.0, 100.0, 100.0));
        scene.insert(second, rect(0.0, 0.0, 100.0, 100.0));
        assert!(scene.raise(first));
        assert_eq!(scene.topmost(), Some(first));
        let order: Vec<WindowId> = scene.iter().map(|(id, _)| id).collect();
        assert_eq!(order, vec![second, first]);
    }

    #[test]
    fn a_point_only_hits_the_window_that_contains_it() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        scene.insert(window, rect(10.0, 10.0, 40.0, 40.0));
        assert_eq!(scene.topmost_at(point(30.0, 30.0)), Some(window));
        assert_eq!(scene.topmost_at(point(5.0, 5.0)), None);
    }

    #[test]
    fn edge_zones_sit_outside_the_window() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        scene.insert(window, rect(100.0, 100.0, 200.0, 150.0));
        let side = 10.0;
        let corner = 20.0;

        // The side band enters 2 px: one pixel inside is still Right.
        assert_eq!(
            scene.edge_at(point(299.0, 150.0), side, corner, |_| true),
            Some((window, Edge::Right))
        );
        // Three pixels inside is client area.
        assert_eq!(
            scene.edge_at(point(297.0, 150.0), side, corner, |_| true),
            None
        );
        // Six pixels outside the right edge: the right band.
        assert_eq!(
            scene.edge_at(point(306.0, 150.0), side, corner, |_| true),
            Some((window, Edge::Right))
        );
        // Six pixels outside diagonally from the top-left corner: the corner L.
        assert_eq!(
            scene.edge_at(point(94.0, 94.0), side, corner, |_| true),
            Some((window, Edge::TopLeft))
        );
        // Thirty pixels inside a corner is content, not a 24 px square.
        assert_eq!(
            scene.edge_at(point(130.0, 130.0), side, corner, |_| true),
            None
        );
        // The sides still work away from the corners.
        assert_eq!(
            scene.edge_at(point(150.0, 94.0), side, corner, |_| true),
            Some((window, Edge::Top))
        );
        assert_eq!(
            scene.edge_at(point(94.0, 150.0), side, corner, |_| true),
            Some((window, Edge::Left))
        );
        assert_eq!(
            scene.edge_at(point(306.0, 244.0), side, corner, |_| true),
            Some((window, Edge::BottomRight))
        );
    }

    #[test]
    fn an_exterior_band_only_hits_outside_the_frame() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        scene.insert(window, rect(100.0, 100.0, 200.0, 150.0));
        let band = |_| Some((6.0, 12.0, true));

        assert_eq!(
            scene.edge_at_with(point(303.0, 175.0), band),
            Some((window, Edge::Right))
        );
        assert_eq!(
            scene.edge_at_with(point(303.0, 253.0), band),
            Some((window, Edge::BottomRight))
        );
        assert_eq!(scene.edge_at_with(point(310.0, 175.0), band), None);
        assert_eq!(scene.edge_at_with(point(298.0, 175.0), band), None);
    }

    #[test]
    fn content_of_a_window_above_blocks_the_band_of_one_below() {
        let mut scene = Scene::new();
        let below = WindowId::new(1);
        let above = WindowId::new(2);
        scene.insert(below, rect(100.0, 100.0, 200.0, 150.0));
        scene.insert(above, rect(250.0, 100.0, 200.0, 150.0));

        // The point is in the below window's right exterior band, but also in
        // the above window's content. Content wins.
        assert_eq!(
            scene.edge_at(point(306.0, 150.0), 10.0, 20.0, |_| true),
            None
        );
    }

    #[test]
    fn the_top_window_band_wins_over_lower_content() {
        let mut scene = Scene::new();
        let below = WindowId::new(1);
        let above = WindowId::new(2);
        scene.insert(below, rect(400.0, 100.0, 200.0, 150.0));
        scene.insert(above, rect(250.0, 100.0, 200.0, 150.0));

        // The point is in the above window's right exterior band and over the
        // below window's content. The top window wins.
        assert_eq!(
            scene.edge_at(point(456.0, 150.0), 10.0, 20.0, |_| true),
            Some((above, Edge::Right))
        );
    }

    #[test]
    fn an_exterior_band_outside_the_visible_rect_still_belongs_to_its_window() {
        let mut scene = Scene::new();
        let below = WindowId::new(1);
        let above = WindowId::new(2);
        // The above window sits completely inside the below one, but its left
        // exterior band reaches outside the below window's visible rectangle.
        scene.insert(below, rect(100.0, 100.0, 300.0, 200.0));
        scene.insert(above, rect(200.0, 150.0, 100.0, 80.0));

        // The point is outside the above window's rectangle, in its left band,
        // and also outside the below window. It belongs to the above window.
        assert_eq!(
            scene.edge_at(point(194.0, 190.0), 10.0, 20.0, |_| true),
            Some((above, Edge::Left))
        );
    }

    #[test]
    fn an_ineligible_band_does_not_count_but_its_content_still_blocks() {
        let mut scene = Scene::new();
        let lower = WindowId::new(1);
        let upper = WindowId::new(2);
        scene.insert(lower, rect(100.0, 100.0, 200.0, 150.0));
        scene.insert(upper, rect(250.0, 100.0, 200.0, 150.0));

        // The point is in the upper window's band and the lower window's
        // content. If the upper window is ineligible, its content blocks the
        // lower band; its own band is skipped.
        assert_eq!(
            scene.edge_at(point(306.0, 150.0), 10.0, 20.0, |id| id != upper),
            None
        );
    }

    #[test]
    fn an_ineligible_window_still_blocks_a_lower_band_with_its_content() {
        let mut scene = Scene::new();
        let lower = WindowId::new(1);
        let upper = WindowId::new(2);
        scene.insert(lower, rect(400.0, 100.0, 200.0, 150.0));
        scene.insert(upper, rect(250.0, 100.0, 200.0, 150.0));

        // The upper window's right band is skipped, but its outer content
        // still stops the search before the lower window.
        assert_eq!(
            scene.edge_at(point(456.0, 150.0), 10.0, 20.0, |id| id != upper),
            None
        );
    }

    #[test]
    fn removing_a_window_takes_it_out_of_the_order() {
        let mut scene = Scene::new();
        let first = WindowId::new(1);
        let second = WindowId::new(2);
        scene.insert(first, rect(0.0, 0.0, 10.0, 10.0));
        scene.insert(second, rect(0.0, 0.0, 10.0, 10.0));
        assert!(scene.remove(first));
        assert!(!scene.remove(first));
        assert_eq!(scene.len(), 1);
        assert_eq!(scene.topmost(), Some(second));
        assert_eq!(scene.outer(first), None);
    }

    #[test]
    fn a_decoration_can_be_stored_before_the_window_is_inserted() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        assert_eq!(scene.decoration(window), None);
        assert!(scene.set_decoration(window, Decoration::Server));
        assert_eq!(scene.decoration(window), Some(Decoration::Server));
        scene.insert(window, rect(0.0, 0.0, 10.0, 10.0));
        assert_eq!(scene.decoration(window), Some(Decoration::Server));
    }

    #[test]
    fn setting_the_same_decoration_reports_no_change() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        assert!(scene.set_decoration(window, Decoration::Client));
        assert!(!scene.set_decoration(window, Decoration::Client));
        assert!(scene.set_decoration(window, Decoration::Server));
        assert_eq!(scene.decoration(window), Some(Decoration::Server));
    }

    #[test]
    fn removing_a_window_drops_its_decoration() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        scene.insert(window, rect(0.0, 0.0, 10.0, 10.0));
        scene.set_decoration(window, Decoration::Server);
        assert!(scene.remove(window));
        assert_eq!(scene.decoration(window), None);
    }

    #[test]
    fn setting_the_outer_rectangle_moves_the_hit_area() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        scene.insert(window, rect(0.0, 0.0, 100.0, 100.0));
        assert!(scene.set_outer(window, rect(200.0, 200.0, 100.0, 100.0)));
        assert_eq!(scene.topmost_at(point(50.0, 50.0)), None);
        assert_eq!(scene.topmost_at(point(250.0, 250.0)), Some(window));
    }

    #[test]
    fn dragging_a_right_edge_grows_the_width_from_a_fixed_left() {
        let start = rect(10.0, 10.0, 100.0, 100.0);
        let new = resized(
            start,
            Edge::Right,
            20.0,
            0.0,
            Size::new(1.0, 1.0),
            Size::new(0.0, 0.0),
        );
        assert_eq!(new, rect(10.0, 10.0, 120.0, 100.0));
    }

    #[test]
    fn dragging_a_left_edge_keeps_the_right_edge_fixed() {
        let start = rect(10.0, 10.0, 100.0, 100.0);
        let new = resized(
            start,
            Edge::Left,
            20.0,
            0.0,
            Size::new(1.0, 1.0),
            Size::new(0.0, 0.0),
        );
        assert_eq!(new, rect(30.0, 10.0, 80.0, 100.0));
    }

    #[test]
    fn a_corner_drag_changes_both_axes() {
        let start = rect(10.0, 10.0, 100.0, 100.0);
        let new = resized(
            start,
            Edge::BottomRight,
            10.0,
            5.0,
            Size::new(1.0, 1.0),
            Size::new(0.0, 0.0),
        );
        assert_eq!(new, rect(10.0, 10.0, 110.0, 105.0));
    }

    #[test]
    fn resizing_respects_a_minimum_and_keeps_the_opposite_edge_fixed() {
        let start = rect(10.0, 10.0, 100.0, 100.0);
        let new = resized(
            start,
            Edge::Left,
            200.0,
            0.0,
            Size::new(50.0, 50.0),
            Size::new(0.0, 0.0),
        );
        assert_eq!(new, rect(60.0, 10.0, 50.0, 100.0));
    }

    #[test]
    fn resizing_respects_a_maximum() {
        let start = rect(10.0, 10.0, 100.0, 100.0);
        let new = resized(
            start,
            Edge::Right,
            500.0,
            0.0,
            Size::new(1.0, 1.0),
            Size::new(200.0, 0.0),
        );
        assert_eq!(new, rect(10.0, 10.0, 200.0, 100.0));
    }

    #[test]
    fn popups_stack_above_windows_and_hit_first() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        let popup = PopupId::new(2);
        scene.insert(window, rect(0.0, 0.0, 200.0, 200.0));
        scene.insert_popup(
            popup,
            PopupParent::Window(window),
            rect(50.0, 50.0, 100.0, 80.0),
        );
        // A point over both the window and the popup hits the popup.
        assert_eq!(scene.popup_under(point(80.0, 80.0)), Some(popup));
        // A point over only the window hits no popup.
        assert_eq!(scene.popup_under(point(180.0, 180.0)), None);
        assert_eq!(scene.topmost_popup(), Some(popup));
    }

    #[test]
    fn popup_ancestors_walk_the_parent_chain() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        let root = PopupId::new(2);
        let sub = PopupId::new(3);
        scene.insert(window, rect(0.0, 0.0, 200.0, 200.0));
        scene.insert_popup(
            root,
            PopupParent::Window(window),
            rect(50.0, 50.0, 100.0, 80.0),
        );
        scene.insert_popup(
            sub,
            PopupParent::Popup(root),
            rect(120.0, 50.0, 100.0, 80.0),
        );
        assert_eq!(scene.popup_ancestors(sub), vec![root]);
        assert_eq!(scene.popup_ancestors(root), Vec::<PopupId>::new());
        // Dismissing the root takes the submenu with it.
        assert_eq!(scene.popup_subtree(root), vec![root, sub]);
    }

    #[test]
    fn removing_a_popup_drops_its_geometry_and_parentage() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        let popup = PopupId::new(2);
        scene.insert(window, rect(0.0, 0.0, 200.0, 200.0));
        scene.insert_popup(
            popup,
            PopupParent::Window(window),
            rect(50.0, 50.0, 100.0, 80.0),
        );
        assert!(scene.remove_popup(popup));
        assert_eq!(scene.popup_rect(popup), None);
        assert_eq!(scene.popup_parent(popup), None);
        assert_eq!(scene.popup_under(point(80.0, 80.0)), None);
    }

    #[test]
    fn render_and_hit_test_visit_windows_in_the_same_stacking_order() {
        let mut scene = Scene::new();
        let bottom = WindowId::new(1);
        let middle = WindowId::new(2);
        let top = WindowId::new(3);
        scene.insert(bottom, rect(0.0, 0.0, 100.0, 100.0));
        scene.insert(middle, rect(0.0, 0.0, 100.0, 100.0));
        scene.insert(top, rect(0.0, 0.0, 100.0, 100.0));

        // `iter` produces back-to-front order; the renderer reverses it to
        // draw front-to-back. `topmost_at` also walks the same vector in
        // reverse, so the same window is considered on top by both paths.
        let order: Vec<WindowId> = scene.iter().map(|(id, _)| id).collect();
        assert_eq!(order, vec![bottom, middle, top]);
        assert_eq!(scene.topmost_at(point(50.0, 50.0)), Some(top));

        scene.remove(top);
        let order: Vec<WindowId> = scene.iter().map(|(id, _)| id).collect();
        assert_eq!(order, vec![bottom, middle]);
        assert_eq!(scene.topmost_at(point(50.0, 50.0)), Some(middle));
    }
}
