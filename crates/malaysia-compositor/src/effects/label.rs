use std::thread::{self, JoinHandle};

use cosmic_text::{
    Align, Attrs, Buffer, Color, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, Metrics,
    Shaping, SwashCache, Weight, Wrap,
};
use ctdesign::{Font, Rgba};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::utils::CropRenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size, Transform};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;

use super::{Badge, Effects, pixels};
use crate::state::Element;
use crate::workspaces::desktops::MAX_DESKTOPS;

const DESKTOP: &str = "Desktop";

pub struct Text {
    texture: TextureBuffer<GlesTexture>,
    size: Size<i32, Logical>,
    pub pixels: Size<i32, Physical>,
}

#[derive(Default)]
struct Label {
    text: String,
    made_for: Option<(Option<i32>, f64)>,
    drawn: Option<Text>,
}

struct Fonts {
    loading: Option<JoinHandle<FontSystem>>,
    system: Option<FontSystem>,
    cache: SwashCache,
}

pub struct Labels {
    fonts: Fonts,
    names: [[Label; MAX_DESKTOPS]; 2],
    title: Label,
}

fn fit(pixels: i32, scale: f64) -> (i32, i32) {
    let logical = (f64::from(pixels) / scale).ceil() as i32;
    (logical, (f64::from(logical) * scale).round() as i32)
}

pub fn centred(size: Size<i32, Physical>, area: Rectangle<i32, Physical>) -> Point<i32, Physical> {
    area.loc + Point::from(((area.size.w - size.w) / 2, (area.size.h - size.h) / 2))
}

fn blend(pixels: &mut [u8], width: i32, height: i32, (x, y): (i32, i32), color: Color) {
    if !(0..width).contains(&x) || !(0..height).contains(&y) {
        return;
    }
    let alpha = u32::from(color.a());
    let at = 4 * (y * width + x) as usize;
    for (channel, source) in
        pixels[at..at + 4]
            .iter_mut()
            .zip([color.r(), color.g(), color.b(), 255])
    {
        let source = u32::from(source) * alpha / 255;
        *channel = (source + u32::from(*channel) * (255 - alpha) / 255) as u8;
    }
}

impl Fonts {
    fn draw(
        &mut self,
        renderer: &mut GlesRenderer,
        text: &str,
        font: Font,
        color: Rgba,
        limit: Option<i32>,
        scale: f64,
    ) -> Option<Text> {
        let system = self
            .system
            .get_or_insert_with(|| self.loading.take().unwrap().join().unwrap());
        if system.db().is_empty() {
            return None;
        }
        let line_height = font.line_height as f32 * scale as f32;
        let mut buffer = Buffer::new(
            system,
            Metrics::new(font.size as f32 * scale as f32, line_height),
        );
        buffer.set_wrap(Wrap::None);
        buffer.set_ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)));
        buffer.set_size(limit.map(|limit| limit as f32), None);
        let attrs = Attrs::new()
            .family(Family::Name(font.family))
            .weight(Weight(font.weight));
        buffer.set_text(text, &attrs, Shaping::Advanced, Some(Align::Left));
        buffer.shape_until_scroll(system, false);
        let width = buffer.layout_runs().next()?.line_w.ceil() as i32;
        if width <= 0 {
            return None;
        }
        let (logical_w, pixel_w) = fit(width, scale);
        let (logical_h, pixel_h) = fit(line_height.ceil() as i32, scale);
        let offset = (pixel_w - width) / 2;
        let mut pixels = vec![0; 4 * (pixel_w * pixel_h) as usize];
        let [r, g, b, _] = color.to_rgba8();
        buffer.draw(
            system,
            &mut self.cache,
            Color::rgb(r, g, b),
            |x, y, _, _, color| blend(&mut pixels, pixel_w, pixel_h, (x + offset, y), color),
        );
        let texture = TextureBuffer::from_memory(
            renderer,
            &pixels,
            Fourcc::Abgr8888,
            (pixel_w, pixel_h),
            false,
            1,
            Transform::Normal,
            None,
        )
        .inspect_err(|error| tracing::warn!(%error, "label upload failed"))
        .ok()?;
        Some(Text {
            texture,
            size: (logical_w, logical_h).into(),
            pixels: (pixel_w, pixel_h).into(),
        })
    }

    fn refresh<'a>(
        &mut self,
        label: &'a mut Label,
        renderer: &mut GlesRenderer,
        font: Font,
        color: Rgba,
        limit: Option<i32>,
        scale: f64,
    ) -> Option<&'a Text> {
        let wanted = Some((limit, scale));
        if label.made_for != wanted {
            label.made_for = wanted;
            label.drawn = self.draw(renderer, &label.text, font, color, limit, scale);
        }
        label.drawn.as_ref()
    }
}

impl Labels {
    pub fn new() -> Labels {
        Labels {
            fonts: Fonts {
                loading: Some(thread::spawn(FontSystem::new)),
                system: None,
                cache: SwashCache::new(),
            },
            names: Default::default(),
            title: Label::default(),
        }
    }

    pub fn desktop_name(
        &mut self,
        renderer: &mut GlesRenderer,
        index: usize,
        grown: bool,
        scale: f64,
    ) -> Option<&Text> {
        let font = if grown {
            ctdesign::SPACES_EXPANDED_LABEL_FONT
        } else {
            ctdesign::SPACES_LABEL_FONT
        };
        let label = &mut self.names[usize::from(grown)][index];
        if label.text.is_empty() {
            label.text = format!("{DESKTOP} {}", index + 1);
        }
        self.fonts.refresh(
            label,
            renderer,
            font,
            ctdesign::SPACES_LABEL_COLOR,
            None,
            scale,
        )
    }

    fn title(
        &mut self,
        renderer: &mut GlesRenderer,
        surface: &WlSurface,
        limit: i32,
        scale: f64,
    ) -> Option<&Text> {
        let label = &mut self.title;
        with_states(surface, |states| {
            let data = states
                .data_map
                .get::<XdgToplevelSurfaceData>()?
                .lock()
                .unwrap();
            let title = data.title.as_deref().filter(|title| !title.is_empty())?;
            if label.text != title {
                label.text.clear();
                label.text.push_str(title);
                label.made_for = None;
            }
            Some(())
        })?;
        self.fonts.refresh(
            label,
            renderer,
            ctdesign::EXPANSION_TITLE_FONT,
            ctdesign::EXPANSION_TITLE_COLOR,
            Some(limit),
            scale,
        )
    }
}

impl Text {
    pub fn element(
        &self,
        location: Point<i32, Physical>,
        clip: Rectangle<i32, Physical>,
        scale: f64,
    ) -> Option<Element> {
        let source = Rectangle::from_size(Size::from((self.pixels.w, self.pixels.h)).to_f64());
        let texture = TextureRenderElement::from_texture_buffer(
            location.to_f64(),
            &self.texture,
            None,
            Some(source),
            Some(self.size),
            Kind::Unspecified,
        );
        CropRenderElement::from_element(texture, scale, clip).map(Element::Text)
    }
}

impl Effects {
    pub fn push_title(
        &self,
        renderer: &mut GlesRenderer,
        surface: &WlSurface,
        window: Rectangle<i32, Physical>,
        scale: f64,
        frame: &mut Vec<Element>,
    ) {
        let padding = pixels(f64::from(ctdesign::EXPANSION_TITLE_PADDING), scale);
        let limit = window.size.w - 2 * padding;
        if limit <= 0 {
            return;
        }
        let mut labels = self.labels.borrow_mut();
        let Some(text) = labels.title(renderer, surface, limit, scale) else {
            return;
        };
        let height = pixels(f64::from(ctdesign::EXPANSION_TITLE_HEIGHT), scale);
        let size = Size::from(((text.pixels.w + 2 * padding).min(window.size.w), height));
        let capsule = Rectangle::new(centred(size, window), size);
        frame.extend(text.element(centred(text.pixels, capsule), window, scale));
        frame.push(Element::Badge(Badge::new(
            self.capsule.clone(),
            self.shape(capsule, height as f32 / 2.0),
            ctdesign::EXPANSION_TITLE_FILL.premultiplied(),
            Rgba::TRANSPARENT.premultiplied(),
            0.0,
            0.0,
            self.badge.clone(),
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_texture_holds_its_text_and_maps_one_to_one_onto_physical_pixels_at_every_scale() {
        for scale in [1.0, 1.25, 1.5, 5.0 / 3.0, 1.75, 2.0, 2.25, 2.5, 3.0] {
            for needed in 1..300 {
                let (logical, physical) = fit(needed, scale);
                assert!(physical >= needed, "{scale} {needed}");
                assert!(
                    f64::from(physical - needed) < scale + 0.5,
                    "{scale} {needed}"
                );
                let drawn = (f64::from(logical) * scale + 7.0).round() as i32 - 7;
                assert_eq!(drawn, physical, "{scale} {needed}");
            }
        }
    }
}
