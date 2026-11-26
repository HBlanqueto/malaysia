mod badge;
mod clip;
mod corner;
mod lines;
mod shaded;
mod shadow;
mod strip;

use std::cell::{RefCell, RefMut};

use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::{Id, Kind};
use smithay::backend::renderer::gles::{GlesError, GlesPixelProgram, GlesRenderer, GlesTexProgram};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::desktop::{PopupManager, Window};
use smithay::utils::{Logical, Physical, Point, Rectangle, Size};

pub use badge::Badge;
pub use clip::{Clipped, round_edges};
pub use lines::Lines;
pub use shadow::Shadow;

use crate::state::Element;
use crate::window_style::corner_radius;
use corner::Curve;
use strip::Chrome;

pub struct Placement {
    pub geometry: Rectangle<i32, Logical>,
    pub desktop_offset: Point<i32, Physical>,
    pub expansion: f64,
}

pub struct Effects {
    clip: GlesTexProgram,
    lines: GlesPixelProgram,
    shadow: GlesPixelProgram,
    badge: GlesPixelProgram,
    curve: Curve,
    gap: Id,
    strip: Chrome,
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
    expansion: f64,
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

fn pixels(points: f64, scale: f64) -> i32 {
    (points * scale).round().max(1.0) as i32
}

fn solid(id: &Id, geometry: Rectangle<i32, Physical>, color: ctdesign::Rgba) -> Element {
    Element::Solid(SolidColorRenderElement::new(
        id.clone(),
        geometry,
        CommitCounter::default(),
        color.premultiplied(),
        Kind::Unspecified,
    ))
}

impl Effects {
    pub fn new(renderer: &mut GlesRenderer, dark: bool) -> Result<Effects, GlesError> {
        Ok(Effects {
            clip: clip::compile(renderer)?,
            lines: lines::compile(renderer)?,
            shadow: shadow::compile(renderer)?,
            badge: badge::compile(renderer)?,
            curve: Curve::new(ctdesign::CORNER_WINDOW.smoothing),
            gap: Id::new(),
            strip: Chrome::new(dark),
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

    fn shape(&self, bounds: Rectangle<i32, Physical>, radius: f32) -> Shape {
        let (radius, curve) = self
            .curve
            .fit(radius, bounds.size.w.min(bounds.size.h) as f32 / 2.0);
        Shape {
            bounds,
            radius,
            curve,
        }
    }

    pub fn push_gap(&self, geometry: Rectangle<i32, Physical>, frame: &mut Vec<Element>) {
        frame.push(solid(&self.gap, geometry, ctdesign::DESKTOP_GAP_COLOR));
    }

    pub fn push_window(
        &self,
        renderer: &mut GlesRenderer,
        window: &Window,
        placement: Placement,
        scale: f64,
        main: bool,
        frame: &mut Vec<Element>,
    ) {
        let Placement {
            geometry,
            desktop_offset,
            expansion,
        } = placement;
        let surface = window.toplevel().unwrap().wl_surface();
        let location = (geometry.loc - window.geometry().loc).to_physical_precise_round(scale)
            + desktop_offset;
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
        let mut bounds: Rectangle<i32, Physical> = geometry.to_physical_precise_round(scale);
        bounds.loc += desktop_offset;
        bounds.size = bounds.size.to_f64().upscale(expansion).to_i32_round();
        let shape = self.shape(bounds, corner_radius(surface) * (scale * expansion) as f32);
        let width = pixels(f64::from(ctdesign::FRAME_LINE_WIDTH), scale);
        let drawn = drawn(
            window,
            Look {
                size: shape.bounds.size,
                radius: shape.radius,
                main,
                expansion,
            },
        );
        let (outline, inner_line, window_shadow) = if main {
            (
                self.lines_scheme.outer,
                self.lines_scheme.inner,
                self.shadow_scheme.main,
            )
        } else {
            (
                self.lines_scheme.outer_inactive,
                self.lines_scheme.inner_inactive,
                self.shadow_scheme.inactive,
            )
        };
        frame.push(Element::Lines(Lines::new(
            drawn.lines.clone(),
            drawn.commit,
            shape,
            width,
            outline,
            inner_line,
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
                origin: shape.bounds.loc,
                factor: expansion,
                program: self.clip.clone(),
            })
        }));
        let shadow_scale = scale * expansion;
        frame.push(Element::Shadow(Shadow::new(
            drawn.shadow.clone(),
            drawn.commit,
            shape,
            width,
            shadow::scaled(window_shadow, shadow_scale),
            ctdesign::WINDOW_SHADOW_SPREAD * shadow_scale as f32,
            self.shadow.clone(),
        )));
    }
}
