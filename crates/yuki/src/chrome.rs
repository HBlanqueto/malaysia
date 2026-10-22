//! Yuki's server-side window chrome: the Neptunia titlebar.
//!
//! Only windows that negotiated server-side decoration and reserve a titlebar
//! inset get one. The bar is rasterized on the CPU into an RGBA buffer and
//! cached per window; it is uploaded as a memory texture when the title,
//! focus, width or scale changes. Client-side decorated windows (GTK) never
//! come through here.

use std::collections::HashMap;

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size, Transform};
use yuki_design::{ButtonKind, Font, Palette, TITLEBAR, TitlebarPosition};
use yuki_scene::geometry::{Logical as SceneLogical, Rect as SceneRect};
use yuki_scene::scene::WindowId;

use crate::cursor::OutputRenderElement;
use crate::decoration::Insets;

/// The titlebar texture cache, one entry per decorated window.
pub(crate) struct ChromeCache {
    entries: HashMap<WindowId, Cached>,
    text: TextEngine,
}

struct Cached {
    key: Key,
    buffer: MemoryRenderBuffer,
    size: (i32, i32),
}

#[derive(PartialEq, Eq)]
struct Key {
    title: String,
    focused: bool,
    width: i32,
    scale: u64,
    dark: bool,
}

impl ChromeCache {
    /// Creates an empty cache with its own text engine.
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            text: TextEngine::new(),
        }
    }

    /// Drops the cached texture of a destroyed window.
    pub(crate) fn remove(&mut self, id: WindowId) {
        self.entries.remove(&id);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        id: WindowId,
        title: &str,
        focused: bool,
        width: i32,
        location: Point<i32, Logical>,
        scale: f64,
        palette: &Palette,
        dark: bool,
    ) -> Option<OutputRenderElement<GlesRenderer>> {
        let key = Key {
            title: title.to_string(),
            focused,
            width,
            scale: scale.to_bits(),
            dark,
        };
        let stale = self.entries.get(&id).is_none_or(|entry| entry.key != key);
        if stale {
            let (pixels, w, h) = self.text.rasterize(title, focused, width, scale, palette);
            let buffer = MemoryRenderBuffer::from_slice(
                &pixels,
                Fourcc::Abgr8888,
                (w, h),
                1,
                Transform::Normal,
                None,
            );
            self.entries.insert(
                id,
                Cached {
                    key,
                    buffer,
                    size: (w, h),
                },
            );
        }
        let entry = self.entries.get(&id)?;
        let (w, h) = entry.size;
        let source = Rectangle::from_size(Size::from((f64::from(w), f64::from(h))));
        let size = Size::from((w, h));
        let location = physical_point(location, scale).to_f64();
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            &entry.buffer,
            None,
            Some(source),
            Some(size),
            Kind::Unspecified,
        )
        .ok()
        .map(OutputRenderElement::from)
    }
}

/// The titlebar button at `location`, if any.
///
/// Only top titlebars take part; a window with no titlebar (client-side chrome,
/// dialog, fullscreen) never matches.
pub(crate) fn button_at(
    outer: SceneRect<f64, SceneLogical>,
    insets: Insets,
    location: Point<f64, Logical>,
) -> Option<ButtonKind> {
    let titlebar = TITLEBAR;
    if titlebar.position != TitlebarPosition::Top || insets.top < f64::from(titlebar.thickness) {
        return None;
    }
    let (x, y) = (outer.x(), outer.y());
    let thickness = f64::from(titlebar.thickness);
    if location.y < y || location.y >= y + thickness {
        return None;
    }
    let center_y = y + thickness / 2.0;
    let diameter = f64::from(titlebar.buttons.diameter);
    let gap = f64::from(titlebar.buttons.spacing);
    let start = x + f64::from(titlebar.buttons.margin_start) + diameter / 2.0;
    for (index, button) in titlebar.buttons.order.iter().enumerate() {
        let center_x = start + index as f64 * (diameter + gap);
        let half = diameter / 2.0;
        let radius = f64::from(titlebar.buttons.corner_radius).min(half);
        let dx = (location.x - center_x).abs() - (half - radius);
        let dy = (location.y - center_y).abs() - (half - radius);
        let distance = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - radius;
        if distance <= 0.0 {
            return Some(button.kind);
        }
    }
    None
}

fn physical_point(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
    point.to_f64().to_physical(scale).to_i32_round()
}

struct TextEngine {
    font_system: FontSystem,
    cache: SwashCache,
    buffer: Buffer,
    family: String,
}

impl TextEngine {
    fn new() -> Self {
        let mut font_system = FontSystem::new();
        let family = font_system
            .db_mut()
            .faces()
            .find(|face| !face.monospaced && !face.post_script_name.contains("Emoji"))
            .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
            .unwrap_or_else(|| "sans-serif".to_string());
        let cache = SwashCache::new();
        let buffer = Buffer::new(&mut font_system, Metrics::new(13.0, 17.0));
        Self {
            font_system,
            cache,
            buffer,
            family,
        }
    }

    fn rasterize(
        &mut self,
        title: &str,
        focused: bool,
        width: i32,
        scale: f64,
        palette: &Palette,
    ) -> (Vec<u8>, i32, i32) {
        let w = (f64::from(width) * scale).round().max(1.0) as i32;
        let h = (f64::from(TITLEBAR.thickness) * scale).round().max(1.0) as i32;
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let background = if focused {
            palette.titlebar_bg.focused
        } else {
            palette.titlebar_bg.unfocused
        };
        fill_rect(&mut pixels, w, h, 0, 0, w, h, background);

        let diameter = (f64::from(TITLEBAR.buttons.diameter) * scale)
            .round()
            .max(1.0) as i32;
        let gap = (f64::from(TITLEBAR.buttons.spacing) * scale).round() as i32;
        let margin = (f64::from(TITLEBAR.buttons.margin_start) * scale).round() as i32;
        let center_y = h / 2;
        for (index, button) in TITLEBAR.buttons.order.iter().enumerate() {
            let center_x = margin + diameter / 2 + index as i32 * (diameter + gap);
            let color = if focused {
                button.color
            } else {
                palette.traffic_lights_inactive
            };
            fill_rounded_square(
                &mut pixels,
                w,
                h,
                center_x,
                center_y,
                diameter,
                (f64::from(TITLEBAR.buttons.corner_radius) * scale).round() as i32,
                color,
            );
        }

        let text_color = if focused {
            palette.title.focused
        } else {
            palette.title.unfocused
        };
        let buttons_width = margin * 2 + 3 * diameter + 2 * gap;
        let max_width = (w - buttons_width).max(w / 2);
        self.draw_title(
            &mut pixels,
            w,
            h,
            title,
            text_color,
            TITLEBAR.font,
            scale,
            max_width,
        );

        let line = (scale.round() as i32).max(1);
        fill_rect(&mut pixels, w, h, 0, h - line, w, line, palette.hairline);
        (pixels, w, h)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_title(
        &mut self,
        pixels: &mut [u8],
        w: i32,
        h: i32,
        title: &str,
        color: yuki_design::Rgba,
        font: Font,
        scale: f64,
        max_width: i32,
    ) {
        if title.is_empty() {
            return;
        }
        let size = (f64::from(font.size) * scale).max(1.0) as f32;
        self.buffer.set_metrics(Metrics::new(size, size * 1.3));
        self.buffer
            .set_size(Some(f64::from(max_width) as f32), Some(h as f32));
        self.buffer.set_text(
            title,
            &Attrs::new().family(Family::Name(&self.family)),
            Shaping::Advanced,
            None,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);

        let mut text_width = 0.0f32;
        let text_height = self
            .buffer
            .layout_runs()
            .map(|run| run.line_height)
            .fold(0.0f32, f32::max);
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                text_width = text_width.max(glyph.x + glyph.w);
            }
        }
        let x = ((w as f32 - text_width) / 2.0).max(0.0);
        let line_y = self
            .buffer
            .layout_runs()
            .next()
            .map_or(0.0, |run| run.line_y);
        let y = ((h as f32 - text_height) / 2.0).max(0.0) + line_y;
        let color = [
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            (color.a * 255.0) as u8,
        ];
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((x, y), 1.0);
                let Some(image) = self
                    .cache
                    .get_image(&mut self.font_system, physical.cache_key)
                else {
                    continue;
                };
                blit_glyph(
                    pixels,
                    w,
                    h,
                    physical.x + image.placement.left,
                    physical.y - image.placement.top,
                    image.placement.width as i32,
                    image.placement.height as i32,
                    &image.data,
                    color,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rect(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: yuki_design::Rgba,
) {
    let rgba = rgba8(color);
    for row in y.max(0)..(y + h).min(height) {
        for col in x.max(0)..(x + w).min(width) {
            let index = (row * width + col) as usize * 4;
            pixels[index..index + 4].copy_from_slice(&rgba);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rounded_square(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    cx: i32,
    cy: i32,
    size: i32,
    radius: i32,
    color: yuki_design::Rgba,
) {
    let half = size as f32 / 2.0;
    let radius = (radius as f32).min(half);
    let inner = half - radius;
    for row in (cy - size).max(0)..(cy + size).min(height) {
        for col in (cx - size).max(0)..(cx + size).min(width) {
            let dx = ((col as f32 + 0.5) - cx as f32).abs() - inner;
            let dy = ((row as f32 + 0.5) - cy as f32).abs() - inner;
            let distance = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - radius;
            let coverage = (0.5 - distance).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let index = (row * width + col) as usize * 4;
            blend(&mut pixels[index..index + 4], color, coverage);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_glyph(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    data: &[u8],
    color: [u8; 4],
) {
    if w == 0 || h == 0 {
        return;
    }
    let alpha_color = yuki_design::Rgba {
        r: f32::from(color[0]) / 255.0,
        g: f32::from(color[1]) / 255.0,
        b: f32::from(color[2]) / 255.0,
        a: f32::from(color[3]) / 255.0,
    };
    for row in 0..h {
        let py = y + row;
        if py < 0 || py >= height {
            continue;
        }
        for col in 0..w {
            let px = x + col;
            if px < 0 || px >= width {
                continue;
            }
            let coverage = if data.len() == (w * h * 4) as usize {
                f32::from(data[((row * w + col) * 4 + 3) as usize]) / 255.0
            } else {
                f32::from(data[(row * w + col) as usize]) / 255.0
            };
            if coverage == 0.0 {
                continue;
            }
            let index = (py * width + px) as usize * 4;
            blend(&mut pixels[index..index + 4], alpha_color, coverage);
        }
    }
}

fn blend(dst: &mut [u8], src: yuki_design::Rgba, coverage: f32) {
    let a = (src.a * coverage).clamp(0.0, 1.0);
    let out = [
        src.r * a + (f32::from(dst[0]) / 255.0) * (1.0 - a),
        src.g * a + (f32::from(dst[1]) / 255.0) * (1.0 - a),
        src.b * a + (f32::from(dst[2]) / 255.0) * (1.0 - a),
    ];
    for (index, value) in out.iter().enumerate() {
        dst[index] = (value * 255.0).clamp(0.0, 255.0) as u8;
    }
    dst[3] = 255;
}

fn rgba8(color: yuki_design::Rgba) -> [u8; 4] {
    [
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
        (color.a * 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use yuki_design::ButtonKind;
    use yuki_scene::geometry::Rect;

    fn outer() -> SceneRect<f64, SceneLogical> {
        Rect::new(
            yuki_scene::geometry::Point::new(100.0, 50.0),
            yuki_scene::geometry::Size::new(800.0, 600.0),
        )
    }

    fn insets() -> Insets {
        Insets {
            top: f64::from(TITLEBAR.thickness),
            ..Insets::ZERO
        }
    }

    #[test]
    fn the_titlebar_uses_neptunia_metrics() {
        assert_eq!(TITLEBAR.position, TitlebarPosition::Top);
        assert_eq!(TITLEBAR.thickness, 44);
        assert_eq!(TITLEBAR.buttons.diameter, 12);
        assert_eq!(TITLEBAR.buttons.spacing, 8);
        assert_eq!(TITLEBAR.buttons.margin_start, 16);
        assert_eq!(yuki_design::WINDOW.corner_radius, 16);
    }

    #[test]
    fn the_close_button_is_at_the_left() {
        let y = 50.0 + f64::from(TITLEBAR.thickness) / 2.0;
        let x = 100.0
            + f64::from(TITLEBAR.buttons.margin_start)
            + f64::from(TITLEBAR.buttons.diameter) / 2.0;
        assert_eq!(
            button_at(outer(), insets(), Point::from((x, y))),
            Some(ButtonKind::Close)
        );
    }

    #[test]
    fn the_three_buttons_are_in_neptunia_order() {
        let y = 50.0 + f64::from(TITLEBAR.thickness) / 2.0;
        let start = 100.0
            + f64::from(TITLEBAR.buttons.margin_start)
            + f64::from(TITLEBAR.buttons.diameter) / 2.0;
        let step = f64::from(TITLEBAR.buttons.diameter + TITLEBAR.buttons.spacing);
        for (index, expected) in [
            ButtonKind::Close,
            ButtonKind::Minimize,
            ButtonKind::Maximize,
        ]
        .into_iter()
        .enumerate()
        {
            let x = start + index as f64 * step;
            assert_eq!(
                button_at(outer(), insets(), Point::from((x, y))),
                Some(expected)
            );
        }
    }

    #[test]
    fn the_buttons_are_rounded_squares() {
        // The top-left corner of the bounding box is outside the rounded
        // square; its centre and edge midpoints are inside.
        let y = 50.0 + f64::from(TITLEBAR.thickness) / 2.0;
        let x = 100.0 + f64::from(TITLEBAR.buttons.margin_start);
        assert_eq!(
            button_at(outer(), insets(), Point::from((x + 0.1, y - 5.9))),
            None
        );
        assert_eq!(
            button_at(outer(), insets(), Point::from((x + 6.0, y))),
            Some(ButtonKind::Close)
        );
    }

    #[test]
    fn the_title_area_is_not_a_button() {
        assert_eq!(
            button_at(outer(), insets(), Point::from((500.0, 70.0))),
            None
        );
        assert_eq!(
            button_at(outer(), insets(), Point::from((500.0, 100.0))),
            None
        );
    }
}
