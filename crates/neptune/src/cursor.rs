//! The pointer cursor: the client's surface, or a default arrow.
//!
//! A client chooses its pointer with `wl_pointer.set_cursor`. Smithay handles
//! that request and, before it calls [`SeatHandler::cursor_image`], validates
//! the request's serial against the pointer's **last enter serial** — the one
//! this compositor generated from its single [`SERIAL_COUNTER`]. A request with
//! any other serial never reaches this module, so no second serial counter
//! exists and no fabricated serial is accepted.
//!
//! The status Smithay passes is kept in [`State::cursor_status`]. Drawing it is
//! this module's job:
//!
//! - a client surface is drawn at the pointer position minus its hotspot;
//! - `Hidden` (a null surface) draws nothing;
//! - the default arrow, loaded from the Xcursor theme once at startup, is drawn
//!   when no client surface is active. If no theme can be read, a built-in
//!   arrow is used instead.
//!
//! [`State::cursor_status`]: crate::state::State::cursor_status
//! [`SERIAL_COUNTER`]: smithay::utils::SERIAL_COUNTER
//! [`SeatHandler::cursor_image`]: smithay::input::SeatHandler::cursor_image

use std::io::Read;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::ImportAll;
use smithay::backend::renderer::ImportMem;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::utils::{Logical, Point, Transform};
use xcursor::CursorTheme;
use xcursor::parser::{Image, parse_xcursor};

/// The compositor's own pointer, used when no client has set one.
pub struct DefaultCursor {
    /// The arrow's pixels, ready to upload as a render element.
    buffer: MemoryRenderBuffer,
    /// The pixel inside the arrow that sits on the pointer position.
    hotspot: Point<i32, Logical>,
}

impl DefaultCursor {
    /// Loads the default arrow from the Xcursor theme, or a built-in one.
    ///
    /// `scale` is the output's integer scale, so a HiDPI output gets a larger
    /// arrow. The theme is `XCURSOR_THEME` or `default`, and the size is
    /// `XCURSOR_SIZE` or 24. Any failure — no theme, no `default` icon, an
    /// unreadable or unparsable file — is not fatal: the built-in arrow is used
    /// and the reason is logged.
    pub fn load(scale: i32) -> Self {
        let data = theme_image(scale).unwrap_or_else(|| {
            tracing::warn!("no usable Xcursor theme; drawing the built-in pointer");
            fallback_image()
        });
        let buffer = MemoryRenderBuffer::from_slice(
            &data.pixels,
            Fourcc::Argb8888,
            (data.width, data.height),
            scale,
            Transform::Normal,
            None,
        );
        Self {
            buffer,
            hotspot: Point::from((data.hot_x, data.hot_y)),
        }
    }

    /// The arrow's pixels.
    pub fn buffer(&self) -> &MemoryRenderBuffer {
        &self.buffer
    }

    /// The arrow's hotspot, in logical coordinates relative to its top-left.
    pub fn hotspot(&self) -> Point<i32, Logical> {
        self.hotspot
    }
}

/// One decoded cursor image and its hotspot.
struct ImageData {
    /// Pixels in the byte order Xcursor and `Fourcc::Argb8888` share.
    pixels: Vec<u8>,
    /// Image width in pixels.
    width: i32,
    /// Image height in pixels.
    height: i32,
    /// Hotspot x, relative to the top-left.
    hot_x: i32,
    /// Hotspot y, relative to the top-left.
    hot_y: i32,
}

/// Reads the `default` icon from the Xcursor theme, if it can be read.
fn theme_image(scale: i32) -> Option<ImageData> {
    let name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".to_string());
    let size = std::env::var("XCURSOR_SIZE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(24);
    let target = size.saturating_mul(scale.max(1) as u32);

    let theme = CursorTheme::load(&name);
    let path = theme.load_icon("default")?;
    let mut data = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .read_to_end(&mut data)
        .ok()?;
    let images = parse_xcursor(&data)?;
    let image = nearest_image(&images, target)?;

    Some(ImageData {
        pixels: image.pixels_rgba.clone(),
        width: image.width as i32,
        height: image.height as i32,
        hot_x: image.xhot as i32,
        hot_y: image.yhot as i32,
    })
}

/// The image whose nominal size is closest to `target`.
fn nearest_image(images: &[Image], target: u32) -> Option<&Image> {
    images.iter().min_by_key(|image| {
        let size = i64::from(image.size);
        (size - i64::from(target)).abs()
    })
}

/// The arrow drawn when no theme can be read.
///
/// It is a filled lower-left triangle whose tip is the hotspot, with a one-pixel
/// dark outline so it is visible on both light and dark backgrounds. Building it
/// here keeps the fallback testable and avoids committing a binary blob.
fn fallback_image() -> ImageData {
    const WIDTH: i32 = 12;
    const HEIGHT: i32 = 12;
    let inside = |x: i32, y: i32| (0..WIDTH).contains(&x) && (0..HEIGHT).contains(&y) && x <= y;

    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let outline = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .iter()
                .any(|&(nx, ny)| !inside(nx, ny));
            let (red, green, blue, alpha) = if !inside(x, y) {
                (0, 0, 0, 0)
            } else if outline {
                (0, 0, 0, 255)
            } else {
                (255, 255, 255, 255)
            };
            pixels.extend_from_slice(&[red, green, blue, alpha]);
        }
    }

    ImageData {
        pixels,
        width: WIDTH,
        height: HEIGHT,
        hot_x: 0,
        hot_y: 0,
    }
}

smithay::render_elements! {
    /// Any element the output's frame can contain: a window, a client cursor or
    /// the default arrow.
    pub(crate) OutputRenderElement<R> where R: ImportAll + ImportMem;
    Surface = WaylandSurfaceRenderElement<R>,
    Memory = MemoryRenderBufferRenderElement<R>,
}

#[cfg(test)]
mod tests {
    use super::fallback_image;

    #[test]
    fn the_fallback_image_is_a_full_rgba_buffer() {
        let image = fallback_image();
        assert_eq!(
            image.pixels.len(),
            (image.width * image.height * 4) as usize
        );
    }

    #[test]
    fn the_fallback_hotspot_is_the_tip() {
        let image = fallback_image();
        assert_eq!((image.hot_x, image.hot_y), (0, 0));
    }

    #[test]
    fn the_fallback_arrow_is_visible() {
        let image = fallback_image();
        let opaque = image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(
            opaque > image.pixels.len() / 8,
            "the fallback should be more than a sliver, got {opaque} opaque pixels"
        );
    }
}
