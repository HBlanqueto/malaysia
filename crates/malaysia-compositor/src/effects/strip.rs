use std::f32::consts::FRAC_1_SQRT_2;

use ctdesign::Rgba;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::utils::CropRenderElement;
use smithay::backend::renderer::element::{Id, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::CommitCounter;
use smithay::desktop::Window;
use smithay::utils::{FrameExtents, Physical, Point, Rectangle};

use super::label::{Name, centred};
use super::{Backdrop, Badge, Clipped, Effects, Lines, Placement, Shadow, pixels, shadow, solid};
use crate::state::Element;
use crate::workspaces::desktops::MAX_DESKTOPS;

pub struct Chrome {
    bar: Id,
    line: Id,
    shadow: Id,
    pill: Id,
    add: [Id; 2],
    ring: Id,
    pub hover_ring: Id,
    pub ring_color: Rgba,
    close: Id,
    backdrops: [Id; MAX_DESKTOPS + 1],
    placeholder: Id,
    placeholder_plus: [Id; 2],
}

impl Chrome {
    pub fn new() -> Chrome {
        Chrome {
            bar: Id::new(),
            line: Id::new(),
            shadow: Id::new(),
            pill: Id::new(),
            add: [Id::new(), Id::new()],
            ring: Id::new(),
            hover_ring: Id::new(),
            ring_color: ctdesign::ACCENTS[0].colors(false, false).ring,
            close: Id::new(),
            backdrops: std::array::from_fn(|_| Id::new()),
            placeholder: Id::new(),
            placeholder_plus: [Id::new(), Id::new()],
        }
    }
}

fn plus_bars(centre: Point<f64, Physical>, scale: f64) -> [Rectangle<i32, Physical>; 2] {
    let arm = pixels(f64::from(ctdesign::SPACES_ADD_SIZE), scale);
    let stroke = f64::from(ctdesign::SPACES_ADD_STROKE) * scale;
    let inset = ((f64::from(arm) - stroke) / 2.0).round() as i32;
    let stroke = arm - 2 * inset;
    let half = f64::from(arm) / 2.0;
    let corner = (centre - Point::from((half, half))).to_i32_round();
    [
        Rectangle::new(corner + Point::from((0, inset)), (arm, stroke).into()),
        Rectangle::new(corner + Point::from((inset, 0)), (stroke, arm).into()),
    ]
}

impl Effects {
    pub fn push_strip_controls(
        &self,
        ring: Option<Rectangle<i32, Physical>>,
        close: Option<(Point<f64, Physical>, bool)>,
        scale: f64,
        frame: &mut Vec<Element>,
    ) {
        if let Some((centre, pressed)) = close {
            let color = if pressed {
                ctdesign::SPACES_CLOSE_PRESSED_COLOR
            } else {
                ctdesign::SPACES_CLOSE_COLOR
            };
            let size = pixels(f64::from(ctdesign::SPACES_CLOSE_SIZE), scale);
            let half = f64::from(size) / 2.0;
            let bounds = Rectangle::new(
                (centre - Point::from((half, half))).to_i32_round(),
                (size, size).into(),
            );
            frame.push(Element::Badge(Badge::new(
                self.strip.close.clone(),
                self.shape(bounds, half as f32),
                color.premultiplied(),
                ctdesign::SPACES_CLOSE_CROSS_COLOR.premultiplied(),
                ctdesign::SPACES_CLOSE_CROSS_SIZE as f32 * scale as f32 * FRAC_1_SQRT_2,
                ctdesign::SPACES_CLOSE_CROSS_STROKE * scale as f32,
                self.badge.clone(),
            )));
        }
        if let Some(thumbnail) = ring {
            let gap = pixels(f64::from(ctdesign::SPACES_RING_GAP), scale);
            let bounds = thumbnail + FrameExtents::new(gap, gap, gap, gap);
            frame.push(Element::Lines(Lines::new(
                self.strip.ring.clone(),
                self.style,
                self.shape(bounds, 0.0),
                pixels(f64::from(ctdesign::SPACES_RING_WIDTH), scale),
                self.strip.ring_color,
                Rgba::TRANSPARENT,
                self.lines.clone(),
            )));
        }
    }

    pub fn push_thumbnail(
        &self,
        renderer: &mut GlesRenderer,
        window: &Window,
        placement: Placement,
        thumbnail: Rectangle<i32, Physical>,
        scale: f64,
        frame: &mut Vec<Element>,
    ) {
        let Placement {
            geometry,
            desktop_offset,
            expansion,
            ..
        } = placement;
        let surface = window.toplevel().unwrap().wl_surface();
        let location = (geometry.loc - window.geometry().loc).to_physical_precise_round(scale)
            + desktop_offset;
        let shape = self.shape(thumbnail, 0.0);
        let tree = render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<_>>(
            renderer,
            surface,
            location,
            scale,
            1.0,
            Kind::Unspecified,
        );
        frame.extend(tree.into_iter().filter_map(|surface| {
            let clipped = Clipped {
                surface,
                shape,
                origin: desktop_offset,
                factor: expansion,
                program: self.clip.clone(),
            };
            CropRenderElement::from_element(clipped, scale, thumbnail).map(Element::Thumbnail)
        }));
    }

    pub fn push_desktop_names<'a>(
        &self,
        renderer: &mut GlesRenderer,
        cells: impl Iterator<Item = (usize, Rectangle<i32, Physical>, Name<'a>, f32)>,
        grown: bool,
        bar: Rectangle<i32, Physical>,
        scale: f64,
        frame: &mut Vec<Element>,
    ) {
        let gap = pixels(f64::from(ctdesign::SPACES_LABEL_GAP), scale);
        let inset = pixels(f64::from(ctdesign::SPACES_LABEL_PILL_RADIUS), scale);
        let mut labels = self.labels.borrow_mut();
        for (slot, cell, name, opacity) in cells {
            let limit = cell.size.w - 2 * inset;
            let Some(text) = labels.desktop_name(renderer, slot, name, grown, limit, scale) else {
                continue;
            };
            let mut location = centred(text.pixels, cell);
            if grown {
                location.y = cell.loc.y + cell.size.h + gap;
            }
            frame.extend(text.element(location, bar, opacity, scale));
        }
    }

    pub fn push_backdrop(
        &self,
        index: usize,
        area: Rectangle<i32, Physical>,
        full_screen: bool,
        frame: &mut Vec<Element>,
    ) {
        let color = if full_screen {
            ctdesign::FULL_SCREEN_BACKGROUND
        } else {
            ctdesign::DESKTOP_BACKGROUND
        };
        frame.push(Element::Solid(SolidColorRenderElement::new(
            self.strip.backdrops[index].clone(),
            area,
            usize::from(full_screen),
            color.premultiplied(),
            Kind::Unspecified,
        )));
    }

    pub fn push_split_placeholder(
        &self,
        share: Rectangle<i32, Physical>,
        clip: Rectangle<i32, Physical>,
        scale: f64,
        frame: &mut Vec<Element>,
    ) {
        let centre = share.loc.to_f64() + share.size.to_f64().downscale(2.0).to_point();
        for (id, bar) in self
            .strip
            .placeholder_plus
            .iter()
            .zip(plus_bars(centre, scale))
        {
            if let Some(bar) = bar.intersection(clip) {
                frame.push(solid(id, bar, ctdesign::SPACES_SPLIT_PLACEHOLDER_PLUS));
            }
        }
        let Some(share) = share.intersection(clip) else {
            return;
        };
        let mut placeholder = Badge::new(
            self.strip.placeholder.clone(),
            self.shape(share, 0.0),
            ctdesign::SPACES_SPLIT_PLACEHOLDER_LEFT.premultiplied(),
            Rgba::TRANSPARENT.premultiplied(),
            0.0,
            0.0,
            self.badge.clone(),
        );
        placeholder.fade_to(ctdesign::SPACES_SPLIT_PLACEHOLDER_RIGHT.premultiplied());
        frame.push(Element::Badge(placeholder));
    }

    pub fn push_strip_marks(
        &self,
        add: Point<f64, Physical>,
        preview: Option<Rectangle<i32, Physical>>,
        pill: Option<Rectangle<i32, Physical>>,
        scale: f64,
        frame: &mut Vec<Element>,
    ) {
        for (id, bar) in self.strip.add.iter().zip(plus_bars(add, scale)) {
            frame.push(solid(id, bar, ctdesign::SPACES_ADD_COLOR));
        }
        if let Some(preview) = preview {
            self.push_backdrop(MAX_DESKTOPS, preview, false, frame);
        }
        if let Some(pill) = pill {
            let lift = ctdesign::SPACES_LABEL_PILL_LIFT;
            frame.push(Element::Badge(Badge::new(
                self.strip.pill.clone(),
                self.shape(pill, ctdesign::SPACES_LABEL_PILL_RADIUS * scale as f32),
                [lift, lift, lift, 0.0],
                Rgba::TRANSPARENT.premultiplied(),
                0.0,
                0.0,
                self.badge.clone(),
            )));
        }
    }

    pub fn push_strip_bar(
        &self,
        strip: Rectangle<i32, Physical>,
        open: bool,
        scale: f64,
        frame: &mut Vec<Element>,
    ) {
        let line = pixels(f64::from(ctdesign::FRAME_LINE_WIDTH), scale);
        if open {
            frame.push(solid(
                &self.strip.line,
                Rectangle::new(
                    strip.loc + Point::from((0, strip.size.h)),
                    (strip.size.w, line).into(),
                ),
                ctdesign::SPACES_STRIP_LINE,
            ));
        }
        frame.push(match &self.backdrop {
            Some(programs) => Element::Backdrop(Backdrop::new(
                self.strip.bar.clone(),
                strip,
                scale,
                programs.clone(),
            )),
            None => solid(&self.strip.bar, strip, ctdesign::SPACES_STRIP_TINT),
        });
        if !open {
            return;
        }
        let wider = strip + FrameExtents::new(strip.size.w, strip.size.w, 0, line);
        frame.push(Element::Shadow(Shadow::new(
            self.strip.shadow.clone(),
            CommitCounter::default(),
            self.shape(wider, 0.0),
            0,
            shadow::scaled(ctdesign::SPACES_STRIP_SHADOW, scale),
            0.0,
            self.shadow.clone(),
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bars_of_the_plus_cross_at_their_middles_with_the_stroke_within_a_pixel_at_every_scale() {
        for scale in [1.0, 1.25, 1.5, 5.0 / 3.0, 1.75, 2.0, 2.25, 2.5, 3.0] {
            let [across, down] = plus_bars(Point::from((1000.3, 19.5 * scale)), scale);
            assert_eq!(
                2 * across.loc.x + across.size.w,
                2 * down.loc.x + down.size.w
            );
            assert_eq!(
                2 * across.loc.y + across.size.h,
                2 * down.loc.y + down.size.h
            );
            let stroke = f64::from(ctdesign::SPACES_ADD_STROKE) * scale;
            assert!((f64::from(across.size.h) - stroke).abs() <= 1.0, "{scale}");
        }
    }
}
