//! The ordered set of windows and geometry queries over it.

use crate::geometry::{Logical, Point, Rect, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(u64);

impl WindowId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PopupId(u64);

impl PopupId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupParent {
    Window(WindowId),
    Popup(PopupId),
    Layer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoration {
    Server,
    Client,
}

#[derive(Debug, Default)]
pub struct Scene {
    windows: Vec<Entry>,
    decorations: Vec<(WindowId, Decoration)>,
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn insert(&mut self, id: WindowId, outer: Rect<f64, Logical>) {
        self.windows.push(Entry { id, outer });
    }

    pub fn remove(&mut self, id: WindowId) -> bool {
        let before = self.windows.len();
        self.windows.retain(|entry| entry.id != id);
        self.decorations.retain(|(entry, _)| *entry != id);
        self.windows.len() != before
    }

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

    pub fn decoration(&self, id: WindowId) -> Option<Decoration> {
        self.decorations
            .iter()
            .find(|(entry, _)| *entry == id)
            .map(|(_, decoration)| *decoration)
    }

    pub fn raise(&mut self, id: WindowId) -> bool {
        let Some(index) = self.windows.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let entry = self.windows.remove(index);
        self.windows.push(entry);
        true
    }

    pub fn outer(&self, id: WindowId) -> Option<Rect<f64, Logical>> {
        self.windows
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.outer)
    }

    pub fn set_outer(&mut self, id: WindowId, outer: Rect<f64, Logical>) -> bool {
        let Some(entry) = self.windows.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.outer = outer;
        true
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (WindowId, Rect<f64, Logical>)> + '_ {
        self.windows.iter().map(|entry| (entry.id, entry.outer))
    }

    pub fn topmost_at(&self, point: Point<f64, Logical>) -> Option<WindowId> {
        self.windows
            .iter()
            .rev()
            .find(|entry| entry.outer.contains(point))
            .map(|entry| entry.id)
    }

    pub fn topmost(&self) -> Option<WindowId> {
        self.windows.last().map(|entry| entry.id)
    }

    pub fn insert_popup(&mut self, id: PopupId, parent: PopupParent, rect: Rect<f64, Logical>) {
        self.popups.push(PopupEntry { id, parent, rect });
    }

    pub fn remove_popup(&mut self, id: PopupId) -> bool {
        let before = self.popups.len();
        self.popups.retain(|entry| entry.id != id);
        self.popups.len() != before
    }

    pub fn popup_rect(&self, id: PopupId) -> Option<Rect<f64, Logical>> {
        self.popups
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.rect)
    }

    pub fn set_popup_rect(&mut self, id: PopupId, rect: Rect<f64, Logical>) -> bool {
        let Some(entry) = self.popups.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.rect = rect;
        true
    }

    pub fn popup_parent(&self, id: PopupId) -> Option<PopupParent> {
        self.popups
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.parent)
    }

    pub fn iter_popups(
        &self,
    ) -> impl DoubleEndedIterator<Item = (PopupId, PopupParent, Rect<f64, Logical>)> + '_ {
        self.popups
            .iter()
            .map(|entry| (entry.id, entry.parent, entry.rect))
    }

    pub fn topmost_popup(&self) -> Option<PopupId> {
        self.popups.last().map(|entry| entry.id)
    }

    pub fn popup_under(&self, point: Point<f64, Logical>) -> Option<PopupId> {
        self.popups
            .iter()
            .rev()
            .find(|entry| entry.rect.contains(point))
            .map(|entry| entry.id)
    }

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

const CORNER_INSET: f64 = 2.0;

pub fn edge_at(
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

    use super::{Decoration, Edge, PopupId, PopupParent, Scene, WindowId, edge_at, resized};

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
        let outer = rect(100.0, 100.0, 200.0, 150.0);
        let side = 10.0;
        let corner = 20.0;

        assert_eq!(
            edge_at(outer, point(299.0, 150.0), side, corner),
            Some(Edge::Right)
        );
        assert_eq!(edge_at(outer, point(297.0, 150.0), side, corner), None);
        assert_eq!(
            edge_at(outer, point(306.0, 150.0), side, corner),
            Some(Edge::Right)
        );
        assert_eq!(
            edge_at(outer, point(94.0, 94.0), side, corner),
            Some(Edge::TopLeft)
        );
        assert_eq!(edge_at(outer, point(130.0, 130.0), side, corner), None);
        assert_eq!(
            edge_at(outer, point(150.0, 94.0), side, corner),
            Some(Edge::Top)
        );
        assert_eq!(
            edge_at(outer, point(94.0, 150.0), side, corner),
            Some(Edge::Left)
        );
        assert_eq!(
            edge_at(outer, point(306.0, 244.0), side, corner),
            Some(Edge::BottomRight)
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
        assert_eq!(scene.popup_under(point(80.0, 80.0)), Some(popup));
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

        let order: Vec<WindowId> = scene.iter().map(|(id, _)| id).collect();
        assert_eq!(order, vec![bottom, middle, top]);
        assert_eq!(scene.topmost_at(point(50.0, 50.0)), Some(top));

        scene.remove(top);
        let order: Vec<WindowId> = scene.iter().map(|(id, _)| id).collect();
        assert_eq!(order, vec![bottom, middle]);
        assert_eq!(scene.topmost_at(point(50.0, 50.0)), Some(middle));
    }
}
