mod clip;
mod corner;
mod lines;
mod shadow;

use std::cell::{RefCell, RefMut};

use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::{Id, Kind};
use smithay::backend::renderer::gles::{GlesError, GlesPixelProgram, GlesRenderer, GlesTexProgram};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::desktop::{PopupManager, Window};
use smithay::utils::{Logical, Physical, Rectangle, Size};

pub use clip::Clipped;
pub use lines::Lines;
pub use shadow::Shadow;

use crate::state::Element;
use crate::window_style::corner_radius;
use corner::Curve;

pub struct Effects {
    clip: GlesTexProgram,
    lines: GlesPixelProgram,
    shadow: GlesPixelProgram,
    curve: Curve,
    lines_scheme: ctdesign::FrameLines,
    shadow_scheme: ctdesign::WindowShadow,
}

#[derive(Clone, Copy)]
pub struct Shape {
    bounds: Rectangle<i32, Physical>,
    radius: f32,
    curve: Curve,
}

#[derive(Clone, Copy, PartialEq)]
struct Look {
    size: Size<i32, Physical>,
    radius: f32,
    main: bool,
}

pub struct Drawn {
    lines: Id,
    shadow: Id,
    commit: CommitCounter,
    look: Option<Look>,
}

fn drawn(window: &Window, look: Look) -> RefMut<'_, Drawn> {
    let mut drawn = window
        .user_data()
        .get_or_insert(|| {
            RefCell::new(Drawn {
                lines: Id::new(),
                shadow: Id::new(),
                commit: CommitCounter::default(),
                look: None,
            })
        })
        .borrow_mut();
    if drawn.look != Some(look) {
        drawn.look = Some(look);
        drawn.commit.increment();
    }
    drawn
}

impl Effects {
    pub fn new(renderer: &mut GlesRenderer, dark: bool) -> Result<Effects, GlesError> {
        Ok(Effects {
            clip: clip::compile(renderer)?,
            lines: lines::compile(renderer)?,
            shadow: shadow::compile(renderer)?,
            curve: Curve::new(ctdesign::CORNER_WINDOW.smoothing),
            lines_scheme: if dark {
                ctdesign::FRAME_LINES_DARK
            } else {
                ctdesign::FRAME_LINES_LIGHT
            },
            shadow_scheme: if dark {
                ctdesign::WINDOW_SHADOW_DARK
            } else {
                ctdesign::WINDOW_SHADOW_LIGHT
            },
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
        let bounds: Rectangle<i32, Physical> = geometry.to_physical_precise_round(scale);
        let (radius, curve) = self.curve.fit(
            corner_radius(surface) * scale as f32,
            bounds.size.w.min(bounds.size.h) as f32 / 2.0,
        );
        let shape = Shape {
            bounds,
            radius,
            curve,
        };
        let width = (f64::from(ctdesign::FRAME_LINE_WIDTH) * scale)
            .round()
            .max(1.0) as i32;
        let drawn = drawn(
            window,
            Look {
                size: shape.bounds.size,
                radius: shape.radius,
                main,
            },
        );
        frame.push(Element::Lines(Lines::new(
            &drawn,
            shape,
            width,
            main,
            self.lines_scheme,
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
        frame.extend(tree.into_iter().map(|surface| {
            Element::Clipped(Clipped {
                surface,
                shape,
                program: self.clip.clone(),
            })
        }));
        frame.push(Element::Shadow(Shadow::new(
            &drawn,
            shape,
            width,
            scale,
            main,
            self.shadow_scheme,
            self.shadow.clone(),
        )));
    }
}
