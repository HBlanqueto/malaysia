mod clip;
mod corner;
mod lines;

use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::{GlesError, GlesPixelProgram, GlesRenderer, GlesTexProgram};
use smithay::desktop::{PopupManager, Window};
use smithay::utils::{Logical, Physical, Rectangle};

pub use clip::Clipped;
pub use lines::Lines;

use crate::state::Element;
use crate::window_style::corner_radius;
use corner::Curve;

pub struct Effects {
    clip: GlesTexProgram,
    lines: GlesPixelProgram,
    curve: Curve,
    dark: bool,
}

#[derive(Clone, Copy)]
pub struct Shape {
    bounds: Rectangle<i32, Physical>,
    radius: f32,
    curve: Curve,
}

impl Effects {
    pub fn new(renderer: &mut GlesRenderer, dark: bool) -> Result<Effects, GlesError> {
        Ok(Effects {
            clip: clip::compile(renderer)?,
            lines: lines::compile(renderer)?,
            curve: Curve::new(ctdesign::CORNER_WINDOW.smoothing),
            dark,
        })
    }

    pub fn push_window(
        &self,
        renderer: &mut GlesRenderer,
        window: &Window,
        geometry: Rectangle<i32, Logical>,
        scale: f64,
        main: bool,
        frame: &mut Vec<Element>,
    ) {
        let surface = window.toplevel().unwrap().wl_surface();
        let location = (geometry.loc - window.geometry().loc).to_physical_precise_round(scale);
        for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
            let offset = (window.geometry().loc + popup_offset - popup.geometry().loc)
                .to_physical_precise_round(scale);
            frame.extend(render_elements_from_surface_tree::<_, Element>(
                renderer,
                popup.wl_surface(),
                location + offset,
                scale,
                1.0,
                Kind::Unspecified,
            ));
        }
        let shape = Shape {
            bounds: geometry.to_physical_precise_round(scale),
            radius: corner_radius(surface) * scale as f32,
            curve: self.curve,
        };
        let width = (f64::from(ctdesign::FRAME_LINE_WIDTH) * scale)
            .round()
            .max(1.0) as i32;
        frame.push(Element::Lines(Lines::new(
            window,
            shape,
            width,
            main,
            self.dark,
            self.lines.clone(),
        )));
        let tree = render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<_>>(
            renderer,
            surface,
            location,
            scale,
            1.0,
            Kind::Unspecified,
        );
        frame.extend(
            tree.into_iter()
                .map(|surface| Element::Clipped(Clipped::new(surface, shape, self.clip.clone()))),
        );
    }
}
