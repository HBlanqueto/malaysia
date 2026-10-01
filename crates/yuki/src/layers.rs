//! Minimal `wlr_layer_shell_v1` support.
//!
//! Yuki serves layer-shell for its shell (`yuki-shell`, ADR 0023). This module
//! tracks the layer surfaces a client creates, computes their geometry from
//! anchors/margins/size, sends the configures, and exposes them for rendering
//! and hit-testing.
//!
//! The wallpaper lives on the [`Layer::Background`] layer — the lowest layer,
//! behind every window — exactly as macOS draws its desktop picture behind the
//! whole window stack.

use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Logical, Point, Rectangle, Size};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::wlr_layer::{
    Anchor, ExclusiveZone, KeyboardInteractivity, Layer, LayerSurface, LayerSurfaceCachedState,
};

/// A layer surface Yuki has placed on the output.
#[derive(Debug)]
pub struct LayerEntry {
    /// The layer surface.
    pub surface: LayerSurface,
    /// Which stacking layer it sits in (background, bottom, top, overlay).
    pub layer: Layer,
    /// The edges it is anchored to.
    pub anchors: Anchor,
    /// Where it is on the output, in logical coordinates.
    pub geometry: Rectangle<i32, Logical>,
    /// The exclusive zone it reserves, in logical pixels.
    pub exclusive_zone: i32,
    /// Whether it may take keyboard focus.
    pub keyboard_interactivity: KeyboardInteractivity,
}

/// The compositor's registry of layer surfaces.
#[derive(Debug, Default)]
pub struct Layers {
    /// Surfaces created but not yet configured. On their first commit we
    /// compute geometry and send the initial configure.
    unmapped: Vec<LayerSurface>,
    /// Surfaces that are placed and rendered.
    mapped: Vec<LayerEntry>,
}

impl Layers {
    /// Creates an empty layer registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a new layer surface (`WlrLayerShellHandler::new_layer_surface`).
    pub fn add(&mut self, surface: LayerSurface) {
        self.unmapped.push(surface);
    }

    /// Forgets a destroyed layer surface.
    pub fn destroy(&mut self, surface: &WlSurface) {
        self.unmapped.retain(|s| s.wl_surface() != surface);
        self.mapped.retain(|e| e.surface.wl_surface() != surface);
    }

    /// Handles a surface commit: configures a new layer surface, or re-places a
    /// mapped one whose anchors/size changed.
    pub fn commit(&mut self, surface: &WlSurface, output_size: Size<i32, Logical>) {
        // Prune any layer surface whose wl_surface died.
        self.mapped.retain(|e| e.surface.wl_surface().alive());
        self.unmapped.retain(|s| s.wl_surface().alive());

        if let Some(pos) = self.unmapped.iter().position(|s| s.wl_surface() == surface) {
            let layer_surface = self.unmapped.remove(pos);
            self.configure(&layer_surface, output_size);
            return;
        }

        if let Some(entry) = self
            .mapped
            .iter_mut()
            .find(|e| e.surface.wl_surface() == surface)
        {
            let state = cached_state_of(surface);
            let geometry = geometry_of(&state, output_size);
            let exclusive_zone = exclusive_zone_of(&state);
            if geometry != entry.geometry || exclusive_zone != entry.exclusive_zone {
                // Tell the client the new size, then place it.
                entry.surface.with_pending_state(|pending| {
                    pending.size = Some(geometry.size);
                });
                entry.surface.send_pending_configure();
                entry.geometry = geometry;
                entry.exclusive_zone = exclusive_zone;
            }
        }
    }

    /// Places a newly committed layer surface: computes its geometry, sends the
    /// initial configure, and marks it mapped.
    fn configure(&mut self, surface: &LayerSurface, output_size: Size<i32, Logical>) {
        let state = cached_state_of(surface.wl_surface());
        let geometry = geometry_of(&state, output_size);
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
        });
    }

    /// The surface, if any, under `point` among layers that accept input.
    ///
    /// `Background` and `Bottom` layers never take input; `Top`/`Overlay`
    /// surfaces with keyboard interactivity do. Returns the surface and the
    /// surface-local coordinates, matching `Windows::surface_under`.
    pub fn surface_under(
        &self,
        point: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.mapped
            .iter()
            .rev()
            .find(|entry| {
                entry.keyboard_interactivity != KeyboardInteractivity::None
                    && matches!(entry.layer, Layer::Top | Layer::Overlay)
                    && entry.geometry.to_f64().contains(point)
            })
            .map(|entry| {
                let local = point - entry.geometry.to_f64().loc;
                (entry.surface.wl_surface().clone(), local)
            })
    }

    /// The layer surfaces on a given layer, most recently added last.
    pub fn on_layer(&self, layer: Layer) -> impl Iterator<Item = &LayerEntry> {
        self.mapped.iter().filter(move |entry| entry.layer == layer)
    }

    /// The geometry of a mapped layer surface, if it is tracked.
    ///
    /// Popups created through `zwlr_layer_surface_v1.get_popup` anchor against
    /// this (the menubar's menus will do exactly that).
    pub fn geometry_of(&self, surface: &WlSurface) -> Option<Rectangle<i32, Logical>> {
        self.mapped
            .iter()
            .find(|entry| entry.surface.wl_surface() == surface)
            .map(|entry| entry.geometry)
    }

    /// The workspace space reserved at the top edge, in logical pixels.
    ///
    /// Top/Overlay layers anchored to the top with an exclusive zone claim it
    /// (the menu bar); windows start below it.
    pub fn exclusive_top(&self) -> i32 {
        self.mapped
            .iter()
            .filter(|entry| {
                matches!(entry.layer, Layer::Top | Layer::Overlay)
                    && entry.anchors.contains(Anchor::TOP)
            })
            .map(|entry| entry.exclusive_zone)
            .sum()
    }

    /// The workspace space reserved at the bottom edge, in logical pixels.
    pub fn exclusive_bottom(&self) -> i32 {
        self.mapped
            .iter()
            .filter(|entry| {
                matches!(entry.layer, Layer::Top | Layer::Overlay)
                    && entry.anchors.contains(Anchor::BOTTOM)
            })
            .map(|entry| entry.exclusive_zone)
            .sum()
    }

    /// All configured layer surfaces, for frame callbacks.
    pub fn all(&self) -> impl Iterator<Item = &LayerEntry> {
        self.mapped.iter()
    }

    /// Whether there are any configured layer surfaces.
    pub fn is_empty(&self) -> bool {
        self.mapped.is_empty()
    }
}

/// Reads the client-committed layer state from the surface's cached state.
fn cached_state_of(surface: &WlSurface) -> LayerSurfaceCachedState {
    with_states(surface, |states| {
        *states
            .cached_state
            .get::<LayerSurfaceCachedState>()
            .current()
    })
}

/// Computes where a layer surface sits, from its anchors, margins and size.
fn geometry_of(
    state: &LayerSurfaceCachedState,
    output_size: Size<i32, Logical>,
) -> Rectangle<i32, Logical> {
    let margins = state.margin;
    let anchors = state.anchor;

    let width = if anchors.anchored_horizontally() {
        output_size.w - margins.left - margins.right
    } else {
        state.size.w
    };
    let height = if anchors.anchored_vertically() {
        output_size.h - margins.top - margins.bottom
    } else {
        state.size.h
    };

    let x = if anchors.contains(Anchor::RIGHT) {
        output_size.w - margins.right - width
    } else {
        margins.left
    };
    let y = if anchors.contains(Anchor::BOTTOM) {
        output_size.h - margins.bottom - height
    } else {
        margins.top
    };

    Rectangle::new(
        Point::from((x.max(0), y.max(0))),
        Size::from((width.max(0), height.max(0))),
    )
}

/// The exclusive zone a layer surface reserves, as plain pixels.
fn exclusive_zone_of(state: &LayerSurfaceCachedState) -> i32 {
    match state.exclusive_zone {
        ExclusiveZone::Exclusive(value) => value as i32,
        _ => 0,
    }
}
