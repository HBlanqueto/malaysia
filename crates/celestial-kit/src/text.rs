//! Text rendering with `cosmic-text` (ADR 0024).
//!
//! This is the Core Text part of Grade Graphics' client side: shaping and
//! layout with `cosmic-text` (harfrust), glyph rasterization with swash, and a
//! `draw` that composites the glyphs into a caller-owned RGBA frame. The atlas
//! cache lives here too, so re-drawing the same text (a clock) re-rasterizes
//! nothing.

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};

/// The text engine: a font system, a glyph cache and one laid-out buffer.
pub struct Text {
    font_system: FontSystem,
    cache: SwashCache,
    buffer: Buffer,
    /// The family actually used, picked from the fonts the system found.
    ///
    /// `FontSystem::new()` defaults the sans-serif fallback to "Open Sans",
    /// which is not installed here; querying it yields no font and no glyphs.
    /// The first non-monospace, non-emoji family available is used instead.
    family: String,
}

impl Text {
    /// Creates an engine with the system fonts and the given metrics.
    pub fn new(font_size: f32) -> Self {
        let mut font_system = FontSystem::new();
        let family = font_system
            .db_mut()
            .faces()
            .find(|face| !face.monospaced && !face.post_script_name.contains("Emoji"))
            .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
            .unwrap_or_else(|| "sans-serif".to_string());
        let cache = SwashCache::new();
        let metrics = Metrics::new(font_size, font_size * 1.3);
        let buffer = Buffer::new(&mut font_system, metrics);
        Self {
            font_system,
            cache,
            buffer,
            family,
        }
    }

    /// Sets the text, wrapping at `max_width` logical pixels when given.
    pub fn set_text(&mut self, text: &str, font_size: f32, max_width: Option<f32>) {
        let metrics = Metrics::new(font_size, font_size * 1.3);
        self.buffer.set_metrics(metrics);
        self.buffer.set_size(max_width, None);
        self.buffer.set_text(
            text,
            &Attrs::new().family(Family::Name(&self.family)),
            Shaping::Advanced,
            None,
        );
    }

    /// The laid-out size of the current text, in logical pixels.
    pub fn size(&self) -> (f32, f32) {
        let (w, h) = self.buffer.size();
        (w.unwrap_or(0.0), h.unwrap_or(0.0))
    }

    /// Composites the current text into `pixels` at `origin`, tinted `color`.
    ///
    /// `pixels` is a top-down RGBA8 frame of `frame_w`×`frame_h` with `stride`
    /// bytes per row. `origin` is the text baseline start in frame coordinates.
    /// Glyphs are blended src-over with the glyph coverage modulating `color`.
    pub fn draw(
        &mut self,
        pixels: &mut [u8],
        stride: usize,
        frame_w: i32,
        frame_h: i32,
        origin: (i32, i32),
        color: [u8; 4],
    ) {
        let (r, g, b) = (color[0], color[1], color[2]);
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let Some(image) = self
                    .cache
                    .get_image(&mut self.font_system, physical.cache_key)
                else {
                    continue;
                };
                let w = image.placement.width;
                let h = image.placement.height;
                if w == 0 || h == 0 {
                    continue;
                }
                blit_glyph(
                    Frame {
                        pixels,
                        stride,
                        w: frame_w,
                        h: frame_h,
                    },
                    Glyph {
                        x: origin.0 + physical.x + image.placement.left,
                        y: origin.1 + physical.y - image.placement.top,
                        w,
                        h,
                        data: &image.data,
                        color: (r, g, b),
                    },
                );
            }
        }
    }
}

/// A top-down RGBA8 frame to composite into.
struct Frame<'a> {
    pixels: &'a mut [u8],
    stride: usize,
    w: i32,
    h: i32,
}

/// One glyph bitmap and where it goes in the frame.
struct Glyph<'a> {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    data: &'a [u8],
    color: (u8, u8, u8),
}

/// Blends one glyph bitmap (premultiplied RGBA) into a top-down RGBA frame.
fn blit_glyph(frame: Frame<'_>, glyph: Glyph<'_>) {
    let (r, g, b) = glyph.color;
    for row in 0..glyph.h as i32 {
        let y = glyph.y + row;
        if y < 0 || y >= frame.h {
            continue;
        }
        for col in 0..glyph.w as i32 {
            let x = glyph.x + col;
            if x < 0 || x >= frame.w {
                continue;
            }
            let gi = (row as usize * glyph.w as usize + col as usize) * 4;
            let coverage = glyph.data[gi + 3];
            if coverage == 0 {
                continue;
            }
            let pi = y as usize * frame.stride + x as usize * 4;
            // src-over with the text colour modulated by the glyph coverage.
            let a = coverage as f32 / 255.0;
            let src = [r as f32 * a, g as f32 * a, b as f32 * a, coverage as f32];
            let dst = [
                frame.pixels[pi] as f32,
                frame.pixels[pi + 1] as f32,
                frame.pixels[pi + 2] as f32,
                frame.pixels[pi + 3] as f32,
            ];
            let out_a = src[3] + dst[3] * (1.0 - a);
            for k in 0..3 {
                let value = (src[k] + dst[k] * (1.0 - a)) / out_a.max(1.0);
                frame.pixels[pi + k] = value.clamp(0.0, 255.0) as u8;
            }
            frame.pixels[pi + 3] = out_a.clamp(0.0, 255.0) as u8;
        }
    }
}

/// A colour in cosmic-text's `ARGB` layout.
pub fn argb(color: [u8; 4]) -> Color {
    Color::rgba(color[0], color[1], color[2], color[3])
}

#[cfg(test)]
mod tests {
    use super::argb;

    #[test]
    fn argb_encodes_in_cosmic_order() {
        // cosmic-text packs `a<<24 | r<<16 | g<<8 | b`; our API takes [r, g, b, a].
        let color = argb([0x12, 0x34, 0x56, 0x78]);
        assert_eq!((color.0 >> 24) & 0xff, 0x78); // alpha
        assert_eq!((color.0 >> 16) & 0xff, 0x12); // red
        assert_eq!((color.0 >> 8) & 0xff, 0x34); // green
        assert_eq!(color.0 & 0xff, 0x56); // blue
    }
}
