//! The pointer cursor: the client's surface, or a compositor-drawn one.
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
//! - a named cursor — the default arrow, a `wp_cursor_shape_v1` shape, or the
//!   resize shape over an edge zone — is loaded once from the Xcursor theme and
//!   drawn. A shape the theme does not have falls back to the default arrow,
//!   and if no theme can be read at all a built-in arrow is used.
//!
//! [`State::cursor_status`]: crate::state::State::cursor_status
//! [`SERIAL_COUNTER`]: smithay::utils::SERIAL_COUNTER
//! [`SeatHandler::cursor_image`]: smithay::input::SeatHandler::cursor_image

use std::io::Read;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::input::pointer::CursorIcon;
use smithay::utils::{Logical, Point, Rectangle, Size, Transform};
use xcursor::CursorTheme;
use xcursor::parser::{Image, parse_xcursor};

/// The compositor-drawn cursors, one per shape the compositor uses itself.
///
/// Client cursor surfaces do not pass through here; this is only what MalaysiaDreams
/// draws when no client has set a cursor, including the resize shapes it shows
/// over its own edge zones.
pub struct Cursors {
    /// The shapes the theme provided, other than the default.
    icons: Vec<(CursorIcon, LoadedCursor)>,
    /// The default arrow, used for every shape the theme did not provide.
    fallback: LoadedCursor,
    /// The same shapes at the shake size, so the enlargement samples a
    /// higher-resolution image instead of magnifying the normal one.
    boosted_icons: Vec<(CursorIcon, LoadedCursor)>,
    /// The boosted default arrow.
    boosted_fallback: LoadedCursor,
}

/// One decoded cursor image and its hotspot.
pub struct LoadedCursor {
    /// The image, ready to upload as a render element.
    buffer: MemoryRenderBuffer,
    /// The pixel inside the image that sits on the pointer position, in the
    /// image's logical coordinates.
    hotspot: Point<i32, Logical>,
    /// The image's logical size, which is the source rectangle.
    source: Size<i32, Logical>,
    /// The image size at rest, before any shake enlargement.
    size: Size<i32, Logical>,
}

/// Every shape a client may request through `wp_cursor_shape_v1`.
const SHAPES: [CursorIcon; 35] = [
    CursorIcon::ContextMenu,
    CursorIcon::Help,
    CursorIcon::Pointer,
    CursorIcon::Progress,
    CursorIcon::Wait,
    CursorIcon::Cell,
    CursorIcon::Crosshair,
    CursorIcon::Text,
    CursorIcon::VerticalText,
    CursorIcon::Alias,
    CursorIcon::Copy,
    CursorIcon::Move,
    CursorIcon::NoDrop,
    CursorIcon::NotAllowed,
    CursorIcon::Grab,
    CursorIcon::Grabbing,
    CursorIcon::EResize,
    CursorIcon::NResize,
    CursorIcon::NeResize,
    CursorIcon::NwResize,
    CursorIcon::SResize,
    CursorIcon::SeResize,
    CursorIcon::SwResize,
    CursorIcon::WResize,
    CursorIcon::EwResize,
    CursorIcon::NsResize,
    CursorIcon::NeswResize,
    CursorIcon::NwseResize,
    CursorIcon::ColResize,
    CursorIcon::RowResize,
    CursorIcon::AllScroll,
    CursorIcon::ZoomIn,
    CursorIcon::ZoomOut,
    CursorIcon::DndAsk,
    CursorIcon::AllResize,
];

impl Cursors {
    /// Loads the cursor shapes from the Xcursor theme, with built-ins as a
    /// fallback.
    ///
    /// `scale` is the output's integer scale, so a HiDPI output gets a larger
    /// pointer. The theme is `XCURSOR_THEME`, or the first of `Adwaita`,
    /// `breeze` and `default` that exists; the size is `XCURSOR_SIZE` or 24. A
    /// shape that cannot be read falls back to the default arrow, and if no
    /// theme can be read at all the built-in arrow is used.
    pub fn load(scale: i32) -> Self {
        let target = cursor_target(scale);
        let fallback = load_icon(CursorIcon::Default, scale, target, None).unwrap_or_else(|| {
            tracing::warn!("no usable Xcursor theme; drawing the built-in pointer");
            builtin_cursor(scale, None)
        });

        let mut icons = Vec::new();
        for icon in SHAPES {
            if let Some(cursor) = load_icon(icon, scale, target, None) {
                icons.push((icon, cursor));
            }
        }

        let boost = vow_design::CURSOR_SHAKE_SCALE.ceil() as u32;
        let boosted_target = target.saturating_mul(boost);
        let boosted_fallback = load_icon(
            CursorIcon::Default,
            scale,
            boosted_target,
            Some(fallback.size()),
        )
        .unwrap_or_else(|| builtin_cursor(scale, Some(fallback.size())));
        let boosted_icons = SHAPES
            .iter()
            .filter_map(|&icon| {
                let nominal = icons
                    .iter()
                    .find(|(name, _)| *name == icon)
                    .map_or_else(|| fallback.size(), |(_, cursor)| cursor.size());
                load_icon(icon, scale, boosted_target, Some(nominal)).map(|cursor| (icon, cursor))
            })
            .collect();

        Self {
            icons,
            fallback,
            boosted_icons,
            boosted_fallback,
        }
    }

    /// The cursor for a shape, or the default arrow when the theme lacks it.
    pub fn get(&self, icon: CursorIcon) -> &LoadedCursor {
        self.icons
            .iter()
            .find(|(name, _)| *name == icon)
            .map(|(_, cursor)| cursor)
            .unwrap_or(&self.fallback)
    }

    /// The high-resolution cursor used while the shake enlarges the pointer.
    pub fn get_boosted(&self, icon: CursorIcon) -> &LoadedCursor {
        self.boosted_icons
            .iter()
            .find(|(name, _)| *name == icon)
            .map(|(_, cursor)| cursor)
            .unwrap_or(&self.boosted_fallback)
    }
}

impl LoadedCursor {
    /// The image to upload.
    pub fn buffer(&self) -> &MemoryRenderBuffer {
        &self.buffer
    }

    /// The image size, in logical coordinates before a shake enlargement.
    pub fn size(&self) -> Size<i32, Logical> {
        self.size
    }

    /// The source rectangle, destination size and hotspot for `boost`.
    ///
    /// The destination grows with the shake but the source stays the full
    /// image, so the boosted set (loaded at a larger size) is sampled instead
    /// of magnified; the hotspot scales with the destination.
    pub fn geometry(
        &self,
        boost: f64,
    ) -> (
        Rectangle<f64, Logical>,
        Size<i32, Logical>,
        Point<f64, Logical>,
    ) {
        let target = Size::from((
            (f64::from(self.size.w) * boost).round().max(1.0) as i32,
            (f64::from(self.size.h) * boost).round().max(1.0) as i32,
        ));
        let ratio_x = f64::from(target.w) / f64::from(self.source.w.max(1));
        let ratio_y = f64::from(target.h) / f64::from(self.source.h.max(1));
        let hotspot = Point::from((
            f64::from(self.hotspot.x) * ratio_x,
            f64::from(self.hotspot.y) * ratio_y,
        ));
        (Rectangle::from_size(self.source.to_f64()), target, hotspot)
    }
}

/// One decoded cursor image and its hotspot, before it becomes a buffer.
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

/// Reads a named icon from the Xcursor theme, if it can be read.
///
/// The theme is `XCURSOR_THEME` when set; otherwise the common desktop
/// defaults are tried in order, because an unset variable would otherwise make
/// every shape fall back to the built-in triangle. The search path itself is
/// the XDG one `xcursor` implements (`XCURSOR_PATH`, then `XDG_DATA_DIRS`),
/// which on NixOS is set by the session.
fn theme_image(name: &str, target: u32) -> Option<ImageData> {
    for theme_name in theme_names() {
        let theme = CursorTheme::load(&theme_name);
        let Some(path) = theme.load_icon(name) else {
            continue;
        };
        let mut data = Vec::new();
        if std::fs::File::open(path)
            .ok()
            .and_then(|mut file| file.read_to_end(&mut data).ok())
            .is_none()
        {
            continue;
        }
        let Some(images) = parse_xcursor(&data) else {
            continue;
        };
        let Some(image) = nearest_image(&images, target) else {
            continue;
        };

        return Some(ImageData {
            pixels: image.pixels_rgba.clone(),
            width: image.width as i32,
            height: image.height as i32,
            hot_x: image.xhot as i32,
            hot_y: image.yhot as i32,
        });
    }
    None
}

/// The cursor theme names to try, in order: the session's choice first, then
/// the usual GNOME and KDE defaults.
fn theme_names() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(name) = std::env::var("XCURSOR_THEME")
        && !name.is_empty()
    {
        names.push(name);
    }
    for fallback in ["Adwaita", "breeze", "default"] {
        if !names.iter().any(|name| name == fallback) {
            names.push(fallback.to_string());
        }
    }
    names
}

/// The physical pixels to ask the theme for, per logical cursor size.
fn cursor_target(scale: i32) -> u32 {
    let size = std::env::var("XCURSOR_SIZE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(24);
    size.saturating_mul(scale.max(1) as u32)
}

/// Loads one named cursor at `target` physical pixels into an uploadable
/// buffer. `nominal` is the logical size it should be drawn at; without it the
/// image's own logical size is used.
fn load_icon(
    icon: CursorIcon,
    scale: i32,
    target: u32,
    nominal: Option<Size<i32, Logical>>,
) -> Option<LoadedCursor> {
    // Xcursor themes commonly use legacy names (`left_ptr` for `default`);
    // the standard name comes first, then the crate's alternatives.
    let data = std::iter::once(icon.name())
        .chain(icon.alt_names().iter().copied())
        .find_map(|name| theme_image(name, target))?;
    Some(loaded_cursor(&data, scale, nominal))
}

/// The built-in arrow, as an uploadable buffer.
fn builtin_cursor(scale: i32, nominal: Option<Size<i32, Logical>>) -> LoadedCursor {
    loaded_cursor(&fallback_image(), scale, nominal)
}

/// Turns a theme image loaded for `scale` physical pixels per logical pixel
/// into a buffer plus logical hotspot and size.
fn loaded_cursor(
    data: &ImageData,
    scale: i32,
    nominal: Option<Size<i32, Logical>>,
) -> LoadedCursor {
    let source = Size::from((data.width / scale, data.height / scale));
    LoadedCursor {
        buffer: memory_buffer(data, scale),
        // The theme stores the hotspot in the loaded image's pixels; the rest
        // of the compositor works in logical coordinates.
        hotspot: Point::from((data.hot_x / scale, data.hot_y / scale)),
        source,
        size: nominal.unwrap_or(source),
    }
}

/// Uploads decoded pixels as a memory buffer at `scale`.
fn memory_buffer(data: &ImageData, scale: i32) -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        &data.pixels,
        Fourcc::Argb8888,
        (data.width, data.height),
        scale,
        Transform::Normal,
        None,
    )
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
    /// Any element the output's frame can contain: a window, a client cursor,
    /// a compositor-drawn cursor, a GPU shader effect or a full-screen texture
    /// overlay (theme fade).
    pub(crate) OutputRenderElement<=GlesRenderer>;
    Surface = WaylandSurfaceRenderElement<GlesRenderer>,
    Memory = MemoryRenderBufferRenderElement<GlesRenderer>,
    Solid = SolidColorRenderElement,
    PixelShader = smithay::backend::renderer::gles::element::PixelShaderElement,
    Rounded = crate::effects::RoundedTextureElement,
    Texture = smithay::backend::renderer::element::texture::TextureRenderElement<GlesTexture>,
}

#[cfg(test)]
mod tests {
    use super::{ImageData, fallback_image, loaded_cursor};
    use smithay::utils::{Point, Rectangle, Size};

    #[test]
    fn the_fallback_image_is_a_full_rgba_buffer() {
        let image = fallback_image();
        assert_eq!(
            image.pixels.len(),
            (image.width * image.height * 4) as usize
        );
    }

    #[test]
    fn the_shake_uses_a_higher_resolution_source() {
        let data = ImageData {
            pixels: vec![0; 128 * 128 * 4],
            width: 128,
            height: 128,
            hot_x: 16,
            hot_y: 16,
        };
        let cursor = loaded_cursor(&data, 2, Some(Size::from((24, 24))));
        assert_eq!(cursor.source, Size::from((64, 64)));
        let (source, target, hotspot) = cursor.geometry(3.0);
        assert_eq!(source, Rectangle::from_size(Size::from((64.0, 64.0))));
        assert_eq!(target, Size::from((72, 72)));
        // 8 logical inside the source, scaled by 72/64.
        assert_eq!(hotspot, Point::from((9.0, 9.0)));
    }

    #[test]
    fn a_cursor_loaded_at_2x_has_a_logical_hotspot_and_size() {
        let data = ImageData {
            pixels: vec![0; 48 * 48 * 4],
            width: 48,
            height: 48,
            hot_x: 18,
            hot_y: 12,
        };
        let cursor = loaded_cursor(&data, 2, None);
        assert_eq!(cursor.hotspot, Point::from((9, 6)));
        assert_eq!(cursor.source, Size::from((24, 24)));
        assert_eq!(cursor.size, Size::from((24, 24)));
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
