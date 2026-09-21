//! The ordered set of windows, and the geometry queries over it.
//!
//! `neptune-scene` cannot depend on Smithay, so a window here is only its
//! identity and its decorated outer rectangle. The compositor keeps the
//! protocol surface and the configure bookkeeping; this module owns stacking
//! order, point hit-testing and the resize edge zones. See ADR 0015.
//!
//! The rectangle is the **decorated** bounds — titlebar and border included —
//! not the client's surface: the two do not coincide (ADR 0014), and the edge
//! zones that start a resize are measured against the decorated bounds, so a
//! grab near the titlebar works even though no client surface is there.

use crate::geometry::{Logical, Point, Rect};

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

/// A stack of windows, back to front.
///
/// The last window is on top. Every query considers windows in stacking order,
/// so what the user sees on top is also what the pointer hits.
#[derive(Debug, Default)]
pub struct Scene {
    /// Back-to-front entries; the last is topmost.
    windows: Vec<Entry>,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    id: WindowId,
    outer: Rect<f64, Logical>,
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
    pub fn remove(&mut self, id: WindowId) -> bool {
        let before = self.windows.len();
        self.windows.retain(|entry| entry.id != id);
        self.windows.len() != before
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

    /// The topmost window under `point` and its resize edge zone, if any.
    ///
    /// `border` is the width of the grab band along each edge, in logical
    /// pixels. A point in a corner's band returns the corner, not one side.
    pub fn edge_at(&self, point: Point<f64, Logical>, border: f64) -> Option<(WindowId, Edge)> {
        let entry = self
            .windows
            .iter()
            .rev()
            .find(|entry| entry.outer.contains(point))?;
        edge_in(entry.outer, point, border).map(|edge| (entry.id, edge))
    }

    /// The topmost window, if any.
    pub fn topmost(&self) -> Option<WindowId> {
        self.windows.last().map(|entry| entry.id)
    }
}

/// Which edge band of `outer` contains `point`, if any.
fn edge_in(outer: Rect<f64, Logical>, point: Point<f64, Logical>, border: f64) -> Option<Edge> {
    let left = point.x <= outer.x() + border;
    let right = point.x >= outer.right() - border;
    let top = point.y <= outer.y() + border;
    let bottom = point.y >= outer.bottom() - border;
    match (top, bottom, left, right) {
        (true, _, true, _) => Some(Edge::TopLeft),
        (true, _, _, true) => Some(Edge::TopRight),
        (_, true, true, _) => Some(Edge::BottomLeft),
        (_, true, _, true) => Some(Edge::BottomRight),
        (true, _, _, _) => Some(Edge::Top),
        (_, true, _, _) => Some(Edge::Bottom),
        (_, _, true, _) => Some(Edge::Left),
        (_, _, _, true) => Some(Edge::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::{Logical, Point, Rect, Size};

    use super::{Edge, Scene, WindowId};

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
    fn edge_zones_are_measured_against_the_decorated_bounds() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        scene.insert(window, rect(100.0, 100.0, 200.0, 150.0));
        let border = 8.0;

        assert_eq!(
            scene.edge_at(point(150.0, 104.0), border),
            Some((window, Edge::Top))
        );
        assert_eq!(
            scene.edge_at(point(150.0, 246.0), border),
            Some((window, Edge::Bottom))
        );
        assert_eq!(
            scene.edge_at(point(104.0, 150.0), border),
            Some((window, Edge::Left))
        );
        assert_eq!(
            scene.edge_at(point(296.0, 150.0), border),
            Some((window, Edge::Right))
        );
        assert_eq!(
            scene.edge_at(point(104.0, 104.0), border),
            Some((window, Edge::TopLeft))
        );
        assert_eq!(
            scene.edge_at(point(296.0, 246.0), border),
            Some((window, Edge::BottomRight))
        );
        assert_eq!(scene.edge_at(point(200.0, 175.0), border), None);
        assert_eq!(scene.edge_at(point(99.0, 175.0), border), None);
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
    fn setting_the_outer_rectangle_moves_the_hit_area() {
        let mut scene = Scene::new();
        let window = WindowId::new(1);
        scene.insert(window, rect(0.0, 0.0, 100.0, 100.0));
        assert!(scene.set_outer(window, rect(200.0, 200.0, 100.0, 100.0)));
        assert_eq!(scene.topmost_at(point(50.0, 50.0)), None);
        assert_eq!(scene.topmost_at(point(250.0, 250.0)), Some(window));
    }
}
