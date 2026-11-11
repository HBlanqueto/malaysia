use malaysia_scene::geometry::{
    Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
};
use std::time::{Duration, Instant};

use malaysia_scene::scene::{PopupId, PopupParent, WindowId};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Serial, Size};
use smithay::wayland::shell::wlr_layer::KeyboardInteractivity;
use smithay::wayland::shell::xdg::{PopupSurface, PositionerState};

use crate::state::State;
use crate::window::surface_has_buffer;

struct PopupData {
    popup: PopupSurface,
    id: PopupId,
    parent_surface: WlSurface,
    parent: PopupParent,
    geometry: Rectangle<i32, Logical>,
    mapped: bool,
}

#[derive(Debug)]
pub(crate) enum FocusRestore {
    Popup(WlSurface),
    Layer(WlSurface),
    Previous(Option<WlSurface>, Option<WindowId>),
}

struct PopupFocusFrame {
    surface: WlSurface,
    grab_serial: Serial,
    root_layer: Option<(WlSurface, KeyboardInteractivity)>,
    previous_focus: Option<WlSurface>,
    previous_window: Option<WindowId>,
}

struct ClosedMenu {
    surface: WlSurface,
    grab_serial: Serial,
    closed_at: Instant,
}

const MENU_SWITCH_WINDOW: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct Popups {
    popups: Vec<PopupData>,
    next_id: u64,
    focus_stack: Vec<PopupFocusFrame>,
    closed_menu: Option<ClosedMenu>,
}

impl Popups {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id_of(&self, surface: &WlSurface) -> Option<PopupId> {
        self.popups
            .iter()
            .find(|data| data.popup.wl_surface() == surface)
            .map(|data| data.id)
    }

    pub fn surface_of(&self, id: PopupId) -> Option<&WlSurface> {
        self.popups
            .iter()
            .find(|data| data.id == id)
            .map(|data| data.popup.wl_surface())
    }

    pub fn popup_of(&self, id: PopupId) -> Option<&PopupSurface> {
        self.popups
            .iter()
            .find(|data| data.id == id)
            .map(|data| &data.popup)
    }

    pub fn surfaces(&self) -> impl Iterator<Item = &WlSurface> {
        self.popups.iter().map(|data| data.popup.wl_surface())
    }

    pub(crate) fn is_popup_surface(&self, surface: &WlSurface) -> bool {
        self.id_of(surface).is_some()
    }

    pub(crate) fn current_root_layer(&self) -> Option<(WlSurface, KeyboardInteractivity)> {
        self.focus_stack.last().and_then(|f| f.root_layer.clone())
    }

    pub(crate) fn push_focus_frame(
        &mut self,
        surface: WlSurface,
        grab_serial: Serial,
        root_layer: Option<(WlSurface, KeyboardInteractivity)>,
        previous_focus: Option<WlSurface>,
        previous_window: Option<WindowId>,
    ) {
        self.focus_stack.push(PopupFocusFrame {
            surface,
            grab_serial,
            root_layer,
            previous_focus,
            previous_window,
        });
    }

    fn note_closed_menu(&mut self, surfaces: &[WlSurface]) {
        self.closed_menu = self
            .focus_stack
            .first()
            .filter(|root| surfaces.contains(&root.surface))
            .map(|root| ClosedMenu {
                surface: root.surface.clone(),
                grab_serial: root.grab_serial,
                closed_at: Instant::now(),
            });
    }

    pub(crate) fn take_menu_switch(
        &mut self,
        surface: &WlSurface,
        serial: Serial,
        last_delivered: Option<Serial>,
    ) -> bool {
        let Some(closed) = self.closed_menu.take() else {
            return false;
        };
        closed.surface.id().same_client_as(&surface.id())
            && menu_switch_serial_ok(
                closed.grab_serial,
                closed.closed_at.elapsed(),
                serial,
                last_delivered,
            )
    }

    pub(crate) fn grab_serial_of(&self, surface: &WlSurface) -> Option<Serial> {
        self.focus_stack
            .iter()
            .rev()
            .find(|frame| frame.surface == *surface)
            .map(|frame| frame.grab_serial)
    }

    pub(crate) fn remove_focus_frames(
        &mut self,
        surfaces: &[WlSurface],
        current_focus: Option<&WlSurface>,
    ) -> Option<FocusRestore> {
        let focus_in_removed =
            current_focus.is_some_and(|focus| surfaces.iter().any(|surface| surface == focus));

        let mut removed = Vec::new();
        let mut i = 0;
        while i < self.focus_stack.len() {
            if surfaces
                .iter()
                .any(|surface| *surface == self.focus_stack[i].surface)
            {
                removed.push(self.focus_stack.remove(i));
            } else {
                i += 1;
            }
        }

        if !focus_in_removed || removed.is_empty() {
            return None;
        }

        if let Some(parent) = self.focus_stack.last() {
            return Some(FocusRestore::Popup(parent.surface.clone()));
        }

        let root = removed
            .iter()
            .find(|frame| frame.root_layer.is_some())
            .or_else(|| removed.first())?;
        if let Some((layer_surface, mode)) = &root.root_layer
            && matches!(
                mode,
                KeyboardInteractivity::Exclusive | KeyboardInteractivity::OnDemand
            )
        {
            return Some(FocusRestore::Layer(layer_surface.clone()));
        }
        Some(FocusRestore::Previous(
            root.previous_focus.clone(),
            root.previous_window,
        ))
    }

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
}

// xdg_popup.grab: a menu bar that moves to the next menu reuses the press serial of the menu it closed.
fn menu_switch_serial_ok(
    closed_grab: Serial,
    since_closed: Duration,
    serial: Serial,
    last_delivered: Option<Serial>,
) -> bool {
    since_closed <= MENU_SWITCH_WINDOW
        && serial.is_no_older_than(&closed_grab)
        && last_delivered.is_some_and(|last| serial <= last)
}

fn popup_root(topmost: PopupId, ancestors: &[PopupId]) -> PopupId {
    ancestors.last().copied().unwrap_or(topmost)
}

impl State {
    pub(crate) fn popup_has_kit_frame(&self, id: PopupId) -> bool {
        let Some(mut surface) = self.popups.surface_of(id).cloned() else {
            return false;
        };
        while let Some(parent) = self
            .popups
            .popups
            .iter()
            .find(|data| data.popup.wl_surface() == &surface)
            .map(|data| data.parent_surface.clone())
        {
            surface = parent;
        }
        crate::window_style::is_kit_client(&surface) || self.layers.entry_of(&surface).is_some()
    }

    pub(crate) fn add_popup(&mut self, popup: PopupSurface, positioner: PositionerState) {
        let Some(parent_surface) = popup.get_parent_surface() else {
            return;
        };

        let parent_geometry = self.parent_geometry(&parent_surface);
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

    pub(crate) fn popup_destroyed(&mut self, surface: &WlSurface) {
        let Some(id) = self.popups.id_of(surface) else {
            return;
        };
        self.remove_popup_subtree(id);
    }

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

    pub(crate) fn dismiss_popups(&mut self, seed: PopupId) {
        let subtree = self.windows.popup_subtree(seed);
        tracing::info!(seed = seed.get(), ?subtree, "dismissing popups");
        let surfaces: Vec<WlSurface> = subtree
            .iter()
            .filter_map(|id| self.popups.surface_of(*id).cloned())
            .collect();
        for id in &subtree {
            if let Some(popup) = self.popups.popup_of(*id) {
                popup.send_popup_done();
            }
        }
        for id in subtree {
            self.forget_popup(id);
        }
        self.popups.closed_menu = None;
        self.restore_popup_focus(&surfaces);
        self.request_redraw();
    }

    fn forget_popup(&mut self, id: PopupId) {
        self.windows.remove_popup(id);
        self.popups.remove_id(id);
        self.chrome.remove(crate::chrome::FrameOwner::Popup(id));
    }

    pub(crate) fn root_popups_of(&self, window: WindowId) -> Vec<PopupId> {
        self.windows
            .iter_popups()
            .filter(|(_, parent, _)| *parent == PopupParent::Window(window))
            .map(|(id, _, _)| id)
            .collect()
    }

    pub(crate) fn fade_out_popups_of(&mut self, window: WindowId) {
        if self.root_popups_of(window).is_empty() {
            return;
        }
        if self.config.appearance.reduced_motion {
            for root in self.root_popups_of(window) {
                self.dismiss_popups(root);
            }
            return;
        }
        self.popup_fade_request = Some(window);
        self.request_redraw();
    }

    pub(crate) fn dismiss_all_popups(&mut self) {
        let Some(topmost) = self.windows.topmost_popup() else {
            return;
        };
        let ancestors = self.windows.popup_ancestors(topmost);
        self.dismiss_popups(popup_root(topmost, &ancestors));
    }

    pub(crate) fn dismiss_topmost_popup(&mut self) {
        let Some(topmost) = self.windows.topmost_popup() else {
            return;
        };
        self.dismiss_popups(topmost);
    }

    fn remove_popup_subtree(&mut self, id: PopupId) {
        let subtree = self.windows.popup_subtree(id);
        tracing::info!(root = id.get(), ?subtree, "popup subtree removed");
        let surfaces: Vec<WlSurface> = subtree
            .iter()
            .filter_map(|id| self.popups.surface_of(*id).cloned())
            .collect();
        for id in subtree {
            self.forget_popup(id);
        }
        self.popups.note_closed_menu(&surfaces);
        self.restore_popup_focus(&surfaces);
        self.request_redraw();
    }

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

    fn restore_popup_focus(&mut self, surfaces: &[WlSurface]) {
        let current_focus = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus());
        if let Some(restore) = self
            .popups
            .remove_focus_frames(surfaces, current_focus.as_ref())
        {
            self.apply_focus_restore(restore);
        }
    }

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

fn scene_rect(rect: Rectangle<i32, Logical>) -> SceneRect<f64, SceneLogical> {
    SceneRect::new(
        ScenePoint::new(rect.loc.x as f64, rect.loc.y as f64),
        SceneSize::new(rect.size.w as f64, rect.size.h as f64),
    )
}

fn smithay_rect(rect: SceneRect<f64, SceneLogical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((rect.x().round() as i32, rect.y().round() as i32)),
        Size::from((rect.width().round() as i32, rect.height().round() as i32)),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{MENU_SWITCH_WINDOW, menu_switch_serial_ok, popup_root, scene_rect, smithay_rect};
    use malaysia_scene::geometry::{Point as ScenePoint, Rect, Size as SceneSize};
    use malaysia_scene::scene::PopupId;
    use smithay::utils::{Point, Rectangle, Serial, Size};

    #[test]
    fn a_menu_switch_grabs_with_the_closed_menu_serial_or_a_newer_delivered_one() {
        let (closed, key, last) = (Serial::from(10), Serial::from(12), Some(Serial::from(12)));
        let soon = Duration::from_millis(5);
        assert!(menu_switch_serial_ok(closed, soon, closed, last));
        assert!(menu_switch_serial_ok(closed, soon, key, last));
        assert!(!menu_switch_serial_ok(closed, soon, Serial::from(9), last));
        assert!(!menu_switch_serial_ok(closed, soon, Serial::from(13), last));
        assert!(!menu_switch_serial_ok(
            closed,
            MENU_SWITCH_WINDOW + soon,
            closed,
            last
        ));
    }

    #[test]
    fn scene_rect_round_trips_whole_pixels() {
        let smithay = Rectangle::new(Point::from((10, 20)), Size::from((300, 200)));
        let scene = scene_rect(smithay);
        assert_eq!(
            scene,
            Rect::new(ScenePoint::new(10.0, 20.0), SceneSize::new(300.0, 200.0))
        );
    }

    #[test]
    fn a_click_outside_dismisses_the_whole_cascade_from_its_root() {
        let topmost = PopupId::new(2);
        let ancestors = [PopupId::new(1), PopupId::new(0)];
        assert_eq!(popup_root(topmost, &ancestors), PopupId::new(0));
        assert_eq!(popup_root(topmost, &[]), topmost);
    }

    #[test]
    fn smithay_rect_rounds_fractional_protocol_geometry() {
        let rect = Rect::new(ScenePoint::new(10.4, 20.6), SceneSize::new(100.4, 100.6));
        let smithay = smithay_rect(rect);
        assert_eq!(smithay.loc, Point::from((10, 21)));
        assert_eq!(smithay.size, Size::from((100, 101)));
    }
}
