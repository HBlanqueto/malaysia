use ctdesign::KNOB_LENS;
use malaysia_scene::scene::WindowId;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::Uniform;
use smithay::utils::{Logical, Physical, Point, Rectangle};

use crate::blur::{Glass, backdrop_ids, material};
use crate::chrome::FrameOwner;
use crate::cursor::OutputRenderElement;
use crate::effects::{RoundedTextureElement, Shape};
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::physical_rect;
use crate::session::physical_point;
use crate::state::State;
use crate::window_style::Knob;

const RING_ROOM: i32 = 4;

fn lens_of(state: &State, kind: Knob) -> [f32; 2] {
    let [depth, strength] = if kind == Knob::Switch {
        KNOB_LENS
    } else {
        material(&state.chrome_theme.resolved, Glass::Dock).lens
    };
    [depth, -strength]
}

pub(crate) fn append<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    id: WindowId,
    origin: Point<i32, Logical>,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let Some(surface) = state.windows.surface_of(id).cloned() else {
        return;
    };
    let (Some(knobs), Some(program)) = (
        crate::window_style::knobs(&surface),
        state.knob_glass.clone(),
    ) else {
        return;
    };
    let lens = lens_of(state, knobs.kind);
    let theme = state.chrome_theme;
    let [red, green, blue, alpha] = knobs.tint;
    let tint = [red * alpha, green * alpha, blue * alpha, alpha];
    for (index, rect) in knobs.rects.iter().enumerate() {
        let knob = physical_rect(Rectangle::new(origin + rect.loc, rect.size), scale);
        let room: Rectangle<i32, Physical> = Rectangle::new(
            (knob.loc.x - RING_ROOM, knob.loc.y - RING_ROOM).into(),
            (knob.size.w + 2 * RING_ROOM, knob.size.h + 2 * RING_ROOM).into(),
        );
        let shape = Shape {
            rect: knob,
            radius: knob.size.w.min(knob.size.h) / 2,
            exponent: ctdesign::CIRCLE_EXPONENT,
        };
        let surface_elements = render_elements_from_surface_tree::<R, WaylandSurfaceRenderElement<R>>(
            renderer,
            &surface,
            physical_point(origin, scale),
            scale,
            1.0,
            Kind::Unspecified,
        );
        let ids = backdrop_ids(&surface, usize::MAX - index, surface_elements.len());
        let glass: Vec<OutputRenderElement<R>> = surface_elements
            .into_iter()
            .zip(ids)
            .filter_map(|(element, element_id)| {
                RoundedTextureElement::from_surface_crop(
                    element,
                    room,
                    element_id,
                    shape,
                    scale,
                    program.clone(),
                )
            })
            .map(|element| {
                element
                    .with_uniforms(vec![Uniform::new("lens", lens), Uniform::new("tint", tint)])
                    .into()
            })
            .collect();
        elements.extend(glass);
        if let Some(shadow) = state.shadow.as_ref() {
            elements.push(state.chrome.knob_shadow(
                FrameOwner::Knob(id, index),
                shape,
                shadow,
                scale,
                &theme,
            ));
        }
    }
}
