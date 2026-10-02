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

    /// The measured size of the current text, after shaping.
    ///
    /// [`Text::size`] reports the buffer size the caller configured (often
    /// `None`, i.e. `0.0`); this shapes the pending text first and returns the
    /// real laid-out width and height from the glyph hitboxes, so a caller can
    /// position or damage-exactly the text it is about to draw.
    pub fn measure(&mut self) -> (f32, f32) {
        self.buffer.shape_until_scroll(&mut self.font_system, false);
        let mut width = 0.0f32;
        let mut height = 0.0f32;
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs {
                width = width.max(glyph.x + glyph.w);
            }
            height = height.max(run.line_height);
        }
        (width, height)
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
        // `Buffer::layout_runs` only iterates already-shaped lines; shape
        // first or the iterator yields nothing (this was the shell-text bug of
        // Bloque 7.1).
        self.buffer.shape_until_scroll(&mut self.font_system, false);
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
                    matches!(
                        image.content,
                        cosmic_text::SwashContent::Color | cosmic_text::SwashContent::SubpixelMask
                    ),
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
    /// The glyph bitmap: one coverage byte per pixel (swash `Alpha`) or
    /// premultiplied RGBA (swash `Color`, for colour glyphs/emoji).
    data: &'a [u8],
    color: (u8, u8, u8),
}

/// Blends one glyph bitmap into a top-down RGBA frame.
///
/// The bitmap is a swash image in `Alpha` format (1 byte of coverage per
/// pixel) or `Color` format (premultiplied RGBA). Either way the text colour
/// is composited premultiplied src-over: the RGB is scaled by the coverage
/// before blending, so glyph edges never show a dark halo.
fn blit_glyph(frame: Frame<'_>, glyph: Glyph<'_>, color_format: bool) {
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
            let gi = row as usize * glyph.w as usize + col as usize;
            let coverage = if color_format {
                // A colour glyph carries its own premultiplied colour; use its
                // alpha for coverage and keep the glyph's RGB.
                let c = &glyph.data[gi * 4..gi * 4 + 4];
                let src = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                let pi = y as usize * frame.stride + x as usize * 4;
                composite(frame.pixels, pi, src);
                continue;
            } else {
                glyph.data[gi]
            };
            if coverage == 0 {
                continue;
            }
            let pi = y as usize * frame.stride + x as usize * 4;
            // src-over with the text colour modulated by the glyph coverage.
            let a = coverage as f32 / 255.0;
            let src = [r as f32 * a, g as f32 * a, b as f32 * a, coverage as f32];
            composite(frame.pixels, pi, src);
        }
    }
}

/// Premultiplied src-over of `src` (RGB already premultiplied, `src[3]` alpha)
/// into an unpremultiplied RGBA pixel at `pi`.
fn composite(pixels: &mut [u8], pi: usize, src: [f32; 4]) {
    let a = src[3] / 255.0;
    let dst = [
        pixels[pi] as f32 / 255.0,
        pixels[pi + 1] as f32 / 255.0,
        pixels[pi + 2] as f32 / 255.0,
        pixels[pi + 3] as f32 / 255.0,
    ];
    // Premultiply the destination by its own alpha.
    let dst_premul = [dst[0] * dst[3], dst[1] * dst[3], dst[2] * dst[3], dst[3]];
    let out_a = src[3] / 255.0 + dst_premul[3] * (1.0 - a);
    if out_a <= 0.0 {
        pixels[pi + 3] = 0;
        return;
    }
    for k in 0..3 {
        let value = (src[k] / 255.0 + dst_premul[k] * (1.0 - a)) / out_a;
        pixels[pi + k] = (value * 255.0).clamp(0.0, 255.0) as u8;
    }
    pixels[pi + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
}

/// A colour in cosmic-text's `ARGB` layout.
pub fn argb(color: [u8; 4]) -> Color {
    Color::rgba(color[0], color[1], color[2], color[3])
}

#[cfg(test)]
mod tests {
    use super::Text;
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

    /// Rasterizes a string and checks that glyphs actually cover pixels.
    ///
    /// This is the regression test for the Bloque 7.1 bug: `layout_runs`
    /// without a preceding `shape_until_scroll` yields nothing, so text was
    /// never drawn. The test also pins the premultiplied src-over blend (no
    /// dark halo around glyph edges).
    #[test]
    fn text_rasterizes_non_transparent_pixels() {
        let mut text = Text::new(13.0);
        text.set_text("18:38", 13.0, None);
        let (w, h) = text.measure();
        assert!(w > 0.0, "measured text width should be > 0, got {w}");
        assert!(h > 0.0, "measured text height should be > 0, got {h}");

        let (fw, fh) = (w.ceil() as usize + 2, h.ceil() as usize + 2);
        let stride = fw * 4;
        let mut frame = vec![0u8; stride * fh];
        text.draw(
            &mut frame,
            stride,
            fw as i32,
            fh as i32,
            (1, 1),
            [0xff, 0xff, 0xff, 0xff],
        );

        let covered = frame.chunks(4).filter(|px| px[3] != 0).count();
        assert!(
            covered > 0,
            "rasterized text produced no non-transparent pixels"
        );

        // No dark halo: the text colour is white on transparent, so every
        // covered pixel must have equal RGB (premultiplied blend, no channel
        // ever darkened below the coverage).
        for px in frame.chunks(4) {
            if px[3] == 0 {
                continue;
            }
            assert!(
                px[0] == px[1] && px[1] == px[2],
                "channel mismatch {:?} (premultiplied halo)",
                px
            );
        }
    }
}
