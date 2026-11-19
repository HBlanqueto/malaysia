use malaysia_scene::scene::WindowId;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Logical, Point, Rectangle, Size};
use smithay::wayland::compositor::{SurfaceAttributes, with_states};
use smithay::wayland::shell::wlr_layer::{
    Anchor, ExclusiveZone, KeyboardInteractivity, Layer, LayerSurface, LayerSurfaceCachedState,
};

#[derive(Debug)]
pub(crate) struct LayerEntry {
    pub(crate) surface: LayerSurface,
    pub(crate) layer: Layer,
    pub(crate) anchors: Anchor,
    pub(crate) geometry: Rectangle<i32, Logical>,
    exclusive_zone: i32,
    pub(crate) keyboard_interactivity: KeyboardInteractivity,
    moved: bool,
}

impl LayerEntry {
    pub(crate) fn is_desktop_bar(&self) -> bool {
        matches!(self.layer, Layer::Top | Layer::Overlay)
            && self
                .anchors
                .contains(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT)
            && !self.anchors.contains(Anchor::BOTTOM)
            && self.exclusive_zone > 0
    }

    pub(crate) fn is_dock(&self) -> bool {
        crate::window_style::is_dock_layer(self.surface.wl_surface())
    }

    pub(crate) fn is_shell_chrome(&self) -> bool {
        let side_dock = self.anchors.contains(Anchor::TOP | Anchor::BOTTOM)
            && self.anchors.contains(Anchor::LEFT) != self.anchors.contains(Anchor::RIGHT)
            && self.exclusive_zone > 0;
        self.is_desktop_bar()
            || (matches!(self.layer, Layer::Top | Layer::Overlay)
                && (side_dock
                    || self.anchors.contains(Anchor::BOTTOM)
                        && !self.anchors.contains(Anchor::TOP)))
    }

    fn reconfigure(&mut self, output_size: Size<i32, Logical>, reserved: (i32, i32)) {
        let state = cached_state_of(self.surface.wl_surface());
        let reserved = reserved_for(self.surface.wl_surface(), &state, reserved);
        let mut geometry = geometry_of(&state, output_size, reserved);
        if self.moved {
            geometry.loc = self.geometry.loc;
        }
        self.exclusive_zone = exclusive_zone_of(&state);
        self.anchors = state.anchor;
        self.keyboard_interactivity = state.keyboard_interactivity;
        self.surface.with_pending_state(|pending| {
            pending.size = Some(geometry.size);
        });
        self.surface.send_pending_configure();
        let acked = self
            .surface
            .with_committed_state(|acked| acked.and_then(|acked| acked.size));
        if acked == Some(geometry.size) {
            self.geometry = geometry;
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Layers {
    unmapped: Vec<LayerSurface>,
    mapped: Vec<LayerEntry>,
    chrome_hidden: bool,
    bar_revealed: bool,
    dock_revealed: bool,
}

impl Layers {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&mut self, surface: LayerSurface) {
        self.unmapped.push(surface);
    }

    pub(crate) fn surfaces(&self) -> impl Iterator<Item = &LayerSurface> {
        self.unmapped
            .iter()
            .chain(self.mapped.iter().map(|e| &e.surface))
    }

    pub(crate) fn destroy(&mut self, surface: &WlSurface) {
        self.unmapped.retain(|s| s.wl_surface() != surface);
        self.mapped.retain(|e| e.surface.wl_surface() != surface);
    }

    pub(crate) fn commit(
        &mut self,
        surface: &WlSurface,
        output_size: Size<i32, Logical>,
    ) -> Option<LayerSurface> {
        self.mapped.retain(|e| e.surface.wl_surface().alive());
        self.unmapped.retain(|s| s.wl_surface().alive());

        if let Some(pos) = self.unmapped.iter().position(|s| s.wl_surface() == surface) {
            let layer_surface = self.unmapped.remove(pos);
            self.configure(&layer_surface, output_size);
            return Some(layer_surface);
        }

        let reserved = self.reserved();
        if let Some(entry) = self
            .mapped
            .iter_mut()
            .find(|e| e.surface.wl_surface() == surface)
        {
            entry.reconfigure(output_size, reserved);
        }
        None
    }

    pub(crate) fn output_resized(&mut self, output_size: Size<i32, Logical>) {
        self.mapped.retain(|e| e.surface.wl_surface().alive());
        let reserved = self.reserved();
        for entry in &mut self.mapped {
            entry.reconfigure(output_size, reserved);
        }
    }

    pub(crate) fn entry_of(&self, surface: &WlSurface) -> Option<&LayerEntry> {
        self.mapped
            .iter()
            .find(|entry| entry.surface.wl_surface() == surface)
    }

    pub(crate) fn mapped(&self) -> &[LayerEntry] {
        &self.mapped
    }

    fn configure(&mut self, surface: &LayerSurface, output_size: Size<i32, Logical>) {
        let state = cached_state_of(surface.wl_surface());
        let reserved = reserved_for(surface.wl_surface(), &state, self.reserved());
        let geometry = geometry_of(&state, output_size, reserved);
        surface.with_pending_state(|pending| {
            pending.size = Some(geometry.size);
        });
        surface.send_configure();
        tracing::info!(
            ?geometry,
            layer = ?state.layer,
            surface = ?surface.wl_surface().id(),
            "layer surface configured"
        );
        self.mapped.push(LayerEntry {
            surface: surface.clone(),
            layer: state.layer,
            anchors: state.anchor,
            geometry,
            exclusive_zone: exclusive_zone_of(&state),
            keyboard_interactivity: state.keyboard_interactivity,
            moved: false,
        });
    }

    pub(crate) fn surface_under(
        &self,
        point: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.mapped
            .iter()
            .rev()
            .find(|entry| {
                if !matches!(entry.layer, Layer::Top | Layer::Overlay)
                    || (self.chrome_hidden
                        && entry.is_shell_chrome()
                        && !(self.bar_revealed && entry.is_desktop_bar())
                        && !(entry.is_dock() && self.dock_shown()))
                    || !entry.surface.wl_surface().alive()
                    || !entry.geometry.to_f64().contains(point)
                    || (crate::notification_panel::is_open_panel(
                        entry.anchors,
                        entry.surface.wl_surface(),
                    ) && point.y < f64::from(self.exclusive_top()))
                {
                    return false;
                }
                let local = point - entry.geometry.to_f64().loc;
                accepts_input(input_region_of(entry.surface.wl_surface()).as_ref(), local)
            })
            .map(|entry| {
                let local = point - entry.geometry.to_f64().loc;
                (entry.surface.wl_surface().clone(), local)
            })
    }

    pub(crate) fn set_chrome_hidden(&mut self, hidden: bool) {
        self.chrome_hidden = hidden;
    }

    pub(crate) fn set_bar_revealed(&mut self, revealed: bool) {
        self.bar_revealed = revealed;
    }

    pub(crate) fn chrome_hidden(&self) -> bool {
        self.chrome_hidden
    }

    pub(crate) fn set_dock_revealed(&mut self, revealed: bool) -> bool {
        std::mem::replace(&mut self.dock_revealed, revealed) != revealed
    }

    pub(crate) fn dock_shown(&self) -> bool {
        self.dock_revealed
            || self
                .dock()
                .is_some_and(|dock| !crate::blur::blur_region(dock.surface.wl_surface()).is_empty())
    }

    pub(crate) fn dock(&self) -> Option<&LayerEntry> {
        self.mapped.iter().find(|entry| entry.is_dock())
    }

    pub(crate) fn dock_takes_pointer_at(&self, point: Point<f64, Logical>) -> bool {
        self.dock().is_some_and(|dock| {
            let geometry = dock.geometry.to_f64();
            geometry.contains(point)
                && accepts_input(
                    input_region_of(dock.surface.wl_surface()).as_ref(),
                    point - geometry.loc,
                )
        })
    }

    pub(crate) fn on_layer(&self, layer: Layer) -> impl Iterator<Item = &LayerEntry> {
        self.mapped.iter().filter(move |entry| entry.layer == layer)
    }

    pub(crate) fn geometry_of(&self, surface: &WlSurface) -> Option<Rectangle<i32, Logical>> {
        self.mapped
            .iter()
            .find(|entry| entry.surface.wl_surface() == surface)
            .map(|entry| entry.geometry)
    }

    pub(crate) fn exclusive_top(&self) -> i32 {
        self.mapped
            .iter()
            .filter(|entry| {
                matches!(entry.layer, Layer::Top | Layer::Overlay)
                    && entry.anchors.contains(Anchor::TOP)
                    && !entry.anchors.contains(Anchor::BOTTOM)
            })
            .map(|entry| entry.exclusive_zone)
            .sum()
    }

    pub(crate) fn exclusive_bottom(&self) -> i32 {
        self.mapped
            .iter()
            .filter(|entry| {
                matches!(entry.layer, Layer::Top | Layer::Overlay)
                    && entry.anchors.contains(Anchor::BOTTOM)
                    && !entry.anchors.contains(Anchor::TOP)
            })
            .map(|entry| entry.exclusive_zone)
            .sum()
    }

    pub(crate) fn alert_location(&self, alert: u64) -> Option<Point<i32, Logical>> {
        self.mapped
            .iter()
            .find(|entry| is_alert(entry, alert))
            .map(|entry| entry.geometry.loc)
    }

    pub(crate) fn move_alert(&mut self, alert: u64, location: Point<i32, Logical>) {
        if let Some(entry) = self.mapped.iter_mut().find(|entry| is_alert(entry, alert)) {
            entry.geometry.loc = location;
            entry.moved = true;
        }
    }

    fn reserved(&self) -> (i32, i32) {
        (self.exclusive_top(), self.exclusive_bottom())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LayerUnfocus<S> {
    None,
    Layer(S),
    Window(Option<WindowId>),
}

#[derive(Debug)]
pub(crate) struct LayerFocus<S> {
    exclusive: Vec<S>,
    on_demand: Option<S>,
    focused: Option<S>,
    previous_window: Option<WindowId>,
}

impl<S> Default for LayerFocus<S> {
    fn default() -> Self {
        Self {
            exclusive: Vec::new(),
            on_demand: None,
            focused: None,
            previous_window: None,
        }
    }
}

impl<S: Clone + Eq> LayerFocus<S> {
    fn remember_window(&mut self, focused_window: Option<WindowId>) {
        if self.previous_window.is_none() {
            self.previous_window = focused_window;
        }
    }

    fn grab_exclusive(&mut self, surface: &S, focused_window: Option<WindowId>) {
        if !self.exclusive.contains(surface) {
            self.exclusive.push(surface.clone());
        }
        self.remember_window(focused_window);
    }

    pub(crate) fn mapped(
        &mut self,
        surface: S,
        mode: KeyboardInteractivity,
        focused_window: Option<WindowId>,
    ) -> bool {
        match mode {
            KeyboardInteractivity::Exclusive => {
                self.grab_exclusive(&surface, focused_window);
                self.on_demand = None;
                self.focused = Some(surface);
                true
            }
            KeyboardInteractivity::OnDemand | KeyboardInteractivity::None => false,
        }
    }

    pub(crate) fn clicked(
        &mut self,
        surface: S,
        mode: KeyboardInteractivity,
        focused_window: Option<WindowId>,
    ) -> bool {
        match mode {
            KeyboardInteractivity::Exclusive => {
                self.grab_exclusive(&surface, focused_window);
                let changed = self.focused.as_ref() != Some(&surface);
                self.focused = Some(surface);
                changed
            }
            KeyboardInteractivity::OnDemand => {
                if self.focused.as_ref() == Some(&surface) {
                    return false;
                }
                self.remember_window(focused_window);
                self.on_demand = Some(surface.clone());
                self.focused = Some(surface);
                true
            }
            KeyboardInteractivity::None => false,
        }
    }

    pub(crate) fn clicked_outside(&mut self) -> bool {
        let Some(focused) = self.focused.as_ref() else {
            return false;
        };
        if self.exclusive.contains(focused) {
            return false;
        }
        self.focused = None;
        self.on_demand = None;
        self.previous_window = None;
        true
    }

    pub(crate) fn unmapped(&mut self, surface: &S) -> LayerUnfocus<S> {
        self.exclusive.retain(|entry| entry != surface);
        if self.on_demand.as_ref() == Some(surface) {
            self.on_demand = None;
        }
        if self.focused.as_ref() != Some(surface) {
            return LayerUnfocus::None;
        }
        self.focused = None;
        if let Some(next) = self.exclusive.last().cloned() {
            self.focused = Some(next.clone());
            return LayerUnfocus::Layer(next);
        }
        LayerUnfocus::Window(self.previous_window.take())
    }

    pub(crate) fn blocks_window_focus(&self) -> bool {
        !self.exclusive.is_empty()
    }

    pub(crate) fn focus_window_after(&mut self, id: WindowId) {
        self.previous_window = Some(id);
    }
}

fn input_region_of(surface: &WlSurface) -> Option<smithay::wayland::compositor::RegionAttributes> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceAttributes>()
            .current()
            .input_region
            .clone()
    })
}

pub(crate) fn surface_accepts_input(surface: &WlSurface, local: Point<f64, Logical>) -> bool {
    accepts_input(input_region_of(surface).as_ref(), local)
}

pub(crate) fn accepts_input(
    region: Option<&smithay::wayland::compositor::RegionAttributes>,
    local: Point<f64, Logical>,
) -> bool {
    region.is_none_or(|region| region.contains(local.to_i32_floor()))
}

fn cached_state_of(surface: &WlSurface) -> LayerSurfaceCachedState {
    with_states(surface, |states| {
        *states
            .cached_state
            .get::<LayerSurfaceCachedState>()
            .current()
    })
}

fn is_alert(entry: &LayerEntry, alert: u64) -> bool {
    crate::window_style::alert_layer(entry.surface.wl_surface()) == Some(alert)
}

fn reserved_for(
    surface: &WlSurface,
    state: &LayerSurfaceCachedState,
    (top, bottom): (i32, i32),
) -> (i32, i32) {
    let under_the_bar = crate::notification_panel::is_open_panel(state.anchor, surface);
    (if under_the_bar { 0 } else { top }, bottom)
}

fn geometry_of(
    state: &LayerSurfaceCachedState,
    output_size: Size<i32, Logical>,
    (reserved_top, reserved_bottom): (i32, i32),
) -> Rectangle<i32, Logical> {
    let margins = state.margin;
    let anchors = state.anchor;
    let (avoided_top, avoided_bottom) = if matches!(state.exclusive_zone, ExclusiveZone::Neutral) {
        (reserved_top, reserved_bottom)
    } else {
        (0, 0)
    };

    let room = (
        output_size.w - margins.left - margins.right,
        output_size.h - avoided_top - avoided_bottom - margins.top - margins.bottom,
    );
    let width = if anchors.anchored_horizontally() {
        room.0
    } else {
        state.size.w.min(room.0)
    };
    let height = if anchors.anchored_vertically() {
        room.1
    } else {
        state.size.h.min(room.1)
    };

    let x = if anchors.contains(Anchor::LEFT) {
        margins.left
    } else if anchors.contains(Anchor::RIGHT) {
        output_size.w - margins.right - width
    } else {
        (output_size.w - width) / 2
    };
    let y = if anchors.contains(Anchor::TOP) {
        avoided_top + margins.top
    } else if anchors.contains(Anchor::BOTTOM) {
        output_size.h - avoided_bottom - margins.bottom - height
    } else {
        reserved_top + (output_size.h - reserved_top - reserved_bottom - height) / 2
    };

    Rectangle::new(
        Point::from((x.max(0), y.max(0))),
        Size::from((width.max(0), height.max(0))),
    )
}

fn exclusive_zone_of(state: &LayerSurfaceCachedState) -> i32 {
    match state.exclusive_zone {
        ExclusiveZone::Exclusive(value) => value as i32,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_never_outgrows_its_output() {
        let state = LayerSurfaceCachedState {
            size: Size::from((0, 65535)),
            anchor: Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: ExclusiveZone::Exclusive(80),
            ..LayerSurfaceCachedState::default()
        };
        let geometry = geometry_of(&state, Size::from((1920, 1080)), (30, 0));
        assert_eq!(
            geometry,
            Rectangle::new(Point::from((0, 0)), Size::from((1920, 1080)))
        );
    }

    #[test]
    fn a_layer_without_an_input_region_accepts_the_pointer() {
        use smithay::wayland::compositor::RectangleKind;

        let region = smithay::wayland::compositor::RegionAttributes {
            rects: vec![(
                RectangleKind::Add,
                Rectangle::new(Point::from((0, 0)), Size::from((10, 10))),
            )],
        };
        assert!(accepts_input(None, Point::from((5.0, 5.0))));
        assert!(accepts_input(Some(&region), Point::from((5.0, 5.0))));
        assert!(!accepts_input(Some(&region), Point::from((15.0, 5.0))));
    }

    #[test]
    fn the_pointer_pinned_to_the_screen_edge_is_inside_a_strip_along_that_edge() {
        use smithay::wayland::compositor::RectangleKind;

        let strip = smithay::wayland::compositor::RegionAttributes {
            rects: vec![(
                RectangleKind::Add,
                Rectangle::new(Point::from((0, 198)), Size::from((1000, 2))),
            )],
        };
        let corner =
            crate::input::clamp_pointer(Point::from((5000.0, 5000.0)), Size::from((1000, 200)));
        assert!(accepts_input(Some(&strip), corner));
        assert!(!accepts_input(Some(&strip), Point::from((500.0, 197.99))));
    }

    #[test]
    fn an_empty_input_region_rejects_the_pointer() {
        let region = smithay::wayland::compositor::RegionAttributes::default();
        assert!(!accepts_input(Some(&region), Point::from((5.0, 5.0))));
    }

    #[test]
    fn a_none_layer_click_does_not_take_keyboard_focus() {
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        assert!(!focus.clicked(3, KeyboardInteractivity::None, Some(WindowId::new(1))));
        assert_eq!(focus.focused.as_ref(), None);
        assert!(!focus.blocks_window_focus());
    }

    #[test]
    fn exclusive_takes_focus_on_map_and_blocks_window_focus() {
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        assert!(focus.mapped(7, KeyboardInteractivity::Exclusive, Some(WindowId::new(3))));
        assert_eq!(focus.focused.as_ref(), Some(&7));
        assert!(focus.blocks_window_focus());
        assert!(!focus.clicked(7, KeyboardInteractivity::Exclusive, None));
    }

    #[test]
    fn on_demand_takes_focus_on_click_and_loses_it_outside() {
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        assert!(!focus.mapped(4, KeyboardInteractivity::OnDemand, Some(WindowId::new(2))));
        assert_eq!(focus.focused.as_ref(), None);
        assert!(focus.clicked(4, KeyboardInteractivity::OnDemand, Some(WindowId::new(2))));
        assert_eq!(focus.focused.as_ref(), Some(&4));
        assert!(!focus.blocks_window_focus());
        assert!(focus.clicked_outside());
        assert_eq!(focus.focused.as_ref(), None);
        assert!(!focus.clicked_outside());
    }

    #[test]
    fn exclusive_keeps_focus_when_the_user_clicks_outside() {
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        focus.mapped(9, KeyboardInteractivity::Exclusive, None);
        assert!(!focus.clicked_outside());
        assert_eq!(focus.focused.as_ref(), Some(&9));
    }

    #[test]
    fn unmapping_restores_the_window_that_had_focus() {
        let window = WindowId::new(5);
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        focus.mapped(1, KeyboardInteractivity::Exclusive, Some(window));
        assert_eq!(focus.unmapped(&1), LayerUnfocus::Window(Some(window)));
        assert_eq!(focus.focused.as_ref(), None);
        assert!(!focus.blocks_window_focus());
    }

    #[test]
    fn unmapping_focuses_the_window_that_asked_for_focus_while_blocked() {
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        focus.mapped(1, KeyboardInteractivity::Exclusive, Some(WindowId::new(5)));
        focus.focus_window_after(WindowId::new(6));
        assert_eq!(
            focus.unmapped(&1),
            LayerUnfocus::Window(Some(WindowId::new(6)))
        );
    }

    #[test]
    fn unmapping_does_not_restore_a_window_that_is_gone() {
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        focus.mapped(2, KeyboardInteractivity::OnDemand, None);
        focus.clicked(2, KeyboardInteractivity::OnDemand, None);
        assert_eq!(focus.unmapped(&2), LayerUnfocus::Window(None));
    }

    #[test]
    fn the_next_exclusive_layer_takes_over_when_one_unmaps() {
        let mut focus: LayerFocus<u32> = LayerFocus::default();
        focus.mapped(1, KeyboardInteractivity::Exclusive, None);
        focus.mapped(2, KeyboardInteractivity::Exclusive, None);
        assert_eq!(focus.unmapped(&2), LayerUnfocus::Layer(1));
        assert_eq!(focus.focused.as_ref(), Some(&1));
    }
}
