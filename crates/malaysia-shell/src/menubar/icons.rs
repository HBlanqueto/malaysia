//! SVG icons for the menubar menus.
//!
//! Layout icons share one 16 px frame: a rounded rectangle from (1.25, 2.25)
//! to (14.75, 13.75) with a 1.3 px stroke. Filled zones stay inside it, in the
//! x 2..14, y 3..13 box. Every colour is `currentColor`, so the menu decides
//! muted, white or faint.

/// Left half filled.
pub const HALF_LEFT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"3\" width=\"6\" height=\"10\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Right half filled.
pub const HALF_RIGHT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"8\" y=\"3\" width=\"6\" height=\"10\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Top half filled.
pub const HALF_TOP: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"3\" width=\"12\" height=\"5\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Bottom half filled.
pub const HALF_BOTTOM: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"8\" width=\"12\" height=\"5\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Top-left quarter filled.
pub const QUARTER_TOP_LEFT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"3\" width=\"6\" height=\"5\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Top-right quarter filled.
pub const QUARTER_TOP_RIGHT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"8\" y=\"3\" width=\"6\" height=\"5\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Bottom-left quarter filled.
pub const QUARTER_BOTTOM_LEFT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"8\" width=\"6\" height=\"5\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Bottom-right quarter filled.
pub const QUARTER_BOTTOM_RIGHT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"8\" y=\"8\" width=\"6\" height=\"5\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Two halves side by side: left opaque, right 45 %.
pub const HALVES_LEFT_RIGHT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"3\" width=\"5.5\" height=\"10\" fill=\"currentColor\"/><rect x=\"8.5\" y=\"3\" width=\"5.5\" height=\"10\" fill=\"currentColor\" fill-opacity=\"0.45\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Two halves side by side: right opaque, left 45 %.
pub const HALVES_RIGHT_LEFT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"3\" width=\"5.5\" height=\"10\" fill=\"currentColor\" fill-opacity=\"0.45\"/><rect x=\"8.5\" y=\"3\" width=\"5.5\" height=\"10\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Two halves stacked: top opaque, bottom 45 %.
pub const HALVES_TOP_BOTTOM: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"3\" width=\"12\" height=\"4.5\" fill=\"currentColor\"/><rect x=\"2\" y=\"8.5\" width=\"12\" height=\"4.5\" fill=\"currentColor\" fill-opacity=\"0.45\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Two halves stacked: bottom opaque, top 45 %.
pub const HALVES_BOTTOM_TOP: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"3\" width=\"12\" height=\"4.5\" fill=\"currentColor\" fill-opacity=\"0.45\"/><rect x=\"2\" y=\"8.5\" width=\"12\" height=\"4.5\" fill=\"currentColor\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Layout grid: the frame with a cross.
pub const GRID: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M8 3v10M2 8h12\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Vertical split: the frame with one vertical line.
pub const SPLIT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M8 3v10\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/><rect x=\"1.25\" y=\"2.25\" width=\"13.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Information: a circle with an i.
pub const INFO: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"8\" cy=\"8\" r=\"6.25\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><circle cx=\"8\" cy=\"5\" r=\"0.9\" fill=\"currentColor\"/><path d=\"M8 7.4v4.6\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/></svg>";

/// Help topic: a circle with a question mark (trazo 1.4, currentColor).
pub const HELP: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"8\" cy=\"8\" r=\"6.3\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\"/><path d=\"M8 11.2V10.5q0-.75.45-1.2l.6-.6q.45-.45.45-1.05 0-.75-.525-1.125T8 6.15q-.9 0-1.425.525T6.05 8\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\"/><circle cx=\"8\" cy=\"12.8\" r=\"0.85\" fill=\"currentColor\"/></svg>";

/// Menu item search result.
///
/// Template: replace `{{text}}` with the text colour and `{{accent}}` with the
/// accent colour. Both become `rgb(r,g,b)`; opacity travels in its own
/// attribute because SVG readers reject eight-digit hex colours.
pub const MENU_ITEM: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"14\" viewBox=\"0 0 16 14\"><rect x=\"0.75\" y=\"1.25\" width=\"14.5\" height=\"11.5\" rx=\"2.2\" fill=\"none\" stroke=\"{{text}}\" stroke-width=\"1.3\"/><rect x=\"3\" y=\"4\" width=\"10\" height=\"3.2\" rx=\"1\" fill=\"{{accent}}\"/><line x1=\"3.5\" y1=\"9.6\" x2=\"8.5\" y2=\"9.6\" stroke=\"{{text}}\" stroke-width=\"1.1\" stroke-linecap=\"round\" stroke-opacity=\"0.6\"/></svg>";

/// Settings gear.
pub const GEAR: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"8\" cy=\"8\" r=\"4.6\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><circle cx=\"8\" cy=\"8\" r=\"1.9\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><g fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"><path d=\"M8 1.6v1.8M8 12.6v1.8M1.6 8h1.8M12.6 8h1.8M3.5 3.5l1.3 1.3M11.2 11.2l1.3 1.3M12.5 3.5l-1.3 1.3M4.8 11.2l-1.3 1.3\"/></g></svg>";

/// Magnifying glass.
pub const SEARCH: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"6.8\" cy=\"6.8\" r=\"4.4\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><line x1=\"9.9\" y1=\"9.9\" x2=\"13.5\" y2=\"13.5\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/></svg>";

/// Trash can.
pub const TRASH: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M3.5 4.5h9 M5.5 4.5V3.2h5v1.3\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/><path d=\"M4.5 6v6a1 1 0 0 0 1 1h5a1 1 0 0 0 1-1V6\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><line x1=\"6.5\" y1=\"7.5\" x2=\"6.5\" y2=\"11\" stroke=\"currentColor\" stroke-width=\"1.1\"/><line x1=\"9.5\" y1=\"7.5\" x2=\"9.5\" y2=\"11\" stroke=\"currentColor\" stroke-width=\"1.1\"/></svg>";

/// House: a simple home shape.
pub const HOUSE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M8 2.5L2.5 7.5h1.5v5.5h8V7.5h1.5z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linejoin=\"round\"/></svg>";

/// Desktop: a monitor standing on a small foot.
pub const DESKTOP: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2.5\" y=\"3\" width=\"11\" height=\"7.5\" rx=\"1.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><line x1=\"6.5\" y1=\"12.5\" x2=\"9.5\" y2=\"12.5\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/><line x1=\"8\" y1=\"10.5\" x2=\"8\" y2=\"12.5\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Documents folder with two lines inside.
pub const DOCUMENTS: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M2 4.5h4l1.5 1.5h6.5v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><line x1=\"5\" y1=\"8.5\" x2=\"11\" y2=\"8.5\" stroke=\"currentColor\" stroke-width=\"1.1\"/><line x1=\"5\" y1=\"10.5\" x2=\"9\" y2=\"10.5\" stroke=\"currentColor\" stroke-width=\"1.1\"/></svg>";

/// Downloads folder with a downward arrow.
pub const DOWNLOADS: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M2 4.5h4l1.5 1.5h6.5v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><path d=\"M8 7.5v3.5 M6 9.5l2 2 2-2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";

/// Pictures folder with a tiny mountain scene.
pub const PICTURES: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M2 4.5h4l1.5 1.5h6.5v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><circle cx=\"5.5\" cy=\"7.5\" r=\"0.8\" fill=\"currentColor\"/><path d=\"M4 11.5l2-2.5 1.5 1.5 2.5-3 2.5 4\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/></svg>";

/// Music folder with an eighth note.
pub const MUSIC: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M2 4.5h4l1.5 1.5h6.5v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><path d=\"M6.5 11.5V8l3-0.8v2.8\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/><circle cx=\"5.8\" cy=\"11.5\" r=\"0.9\" fill=\"currentColor\"/><circle cx=\"8.8\" cy=\"10.7\" r=\"0.9\" fill=\"currentColor\"/></svg>";

/// Videos folder with a play triangle.
pub const VIDEOS: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M2 4.5h4l1.5 1.5h6.5v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><path d=\"M6.5 8l3 1.5-3 1.5z\" fill=\"currentColor\"/></svg>";

/// Share sheet: a rectangle with an upward arrow.
pub const SHARE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M8 3v7\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/><path d=\"M5 6l3-3 3 3\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/><rect x=\"2.5\" y=\"8\" width=\"11\" height=\"5\" rx=\"1.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// Eject: an upward triangle above a line.
pub const EJECT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M4 9l4-5 4 5z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linejoin=\"round\"/><line x1=\"3\" y1=\"11\" x2=\"13\" y2=\"11\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/></svg>";

/// Printer: a sheet feeding into a body.
pub const PRINTER: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"3\" y=\"5\" width=\"10\" height=\"7\" rx=\"1.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><rect x=\"4.5\" y=\"2.5\" width=\"7\" height=\"4\" rx=\"1\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><line x1=\"5.5\" y1=\"8\" x2=\"10.5\" y2=\"8\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/><line x1=\"5.5\" y1=\"10\" x2=\"9.5\" y2=\"10\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/></svg>";

/// Emoji picker: a smiley face.
pub const EMOJI: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"8\" cy=\"8\" r=\"6.25\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><circle cx=\"5.7\" cy=\"7\" r=\"0.8\" fill=\"currentColor\"/><circle cx=\"10.3\" cy=\"7\" r=\"0.8\" fill=\"currentColor\"/><path d=\"M5.5 9.5a4.5 4.5 0 0 0 5 0\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/></svg>";

/// View as list: three bulleted rows.
pub const VIEW_LIST: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"3\" y=\"4\" width=\"2\" height=\"2\" rx=\"0.5\" fill=\"currentColor\"/><line x1=\"6.5\" y1=\"5\" x2=\"13\" y2=\"5\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/><rect x=\"3\" y=\"7\" width=\"2\" height=\"2\" rx=\"0.5\" fill=\"currentColor\"/><line x1=\"6.5\" y1=\"8\" x2=\"13\" y2=\"8\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/><rect x=\"3\" y=\"10\" width=\"2\" height=\"2\" rx=\"0.5\" fill=\"currentColor\"/><line x1=\"6.5\" y1=\"11\" x2=\"13\" y2=\"11\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"/></svg>";

/// View as columns: two tall rectangles side by side.
pub const VIEW_COLUMNS: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2.5\" y=\"3\" width=\"4.5\" height=\"10\" rx=\"1.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><rect x=\"9\" y=\"3\" width=\"4.5\" height=\"10\" rx=\"1.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/></svg>";

/// View as gallery: a picture frame with a sun and mountains.
pub const VIEW_GALLERY: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2.5\" y=\"4\" width=\"11\" height=\"8\" rx=\"1.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><circle cx=\"6\" cy=\"7.5\" r=\"0.9\" fill=\"currentColor\"/><path d=\"M3 12l3-3.5 2 2 3-3.5 3 5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/></svg>";

/// Applications folder: a folder with a small grid of app tiles.
pub const APPS: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M2 4.5h4l1.5 1.5h6.5v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><rect x=\"6\" y=\"7\" width=\"2\" height=\"2\" rx=\"0.5\" fill=\"currentColor\"/><rect x=\"9\" y=\"7\" width=\"2\" height=\"2\" rx=\"0.5\" fill=\"currentColor\"/><rect x=\"6\" y=\"10\" width=\"2\" height=\"2\" rx=\"0.5\" fill=\"currentColor\"/><rect x=\"9\" y=\"10\" width=\"2\" height=\"2\" rx=\"0.5\" fill=\"currentColor\"/></svg>";

/// Book: a closed book with lines on the cover.
pub const BOOK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"3\" y=\"2.5\" width=\"10\" height=\"11\" rx=\"1.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><line x1=\"3\" y1=\"5.5\" x2=\"13\" y2=\"5.5\" stroke=\"currentColor\" stroke-width=\"1.1\"/><line x1=\"5\" y1=\"8\" x2=\"11\" y2=\"8\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/><line x1=\"5\" y1=\"10\" x2=\"10\" y2=\"10\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/></svg>";

/// Keyboard: a small keyboard with a few keys.
pub const KEYBOARD: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"5\" width=\"12\" height=\"6\" rx=\"1.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><rect x=\"4\" y=\"7\" width=\"2\" height=\"1.5\" rx=\"0.3\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\"/><rect x=\"7\" y=\"7\" width=\"2\" height=\"1.5\" rx=\"0.3\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\"/><rect x=\"10\" y=\"7\" width=\"2\" height=\"1.5\" rx=\"0.3\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.1\"/><line x1=\"4\" y1=\"10\" x2=\"12\" y2=\"10\" stroke=\"currentColor\" stroke-width=\"1.1\" stroke-linecap=\"round\"/></svg>";

#[cfg(test)]
mod tests {
    use super::*;
    use barrier_kit::menu::search_icon_svg;
    use freya_engine::prelude::{AlphaType, ColorType, FontMgr, ImageInfo, raster_n32_premul, svg};
    use vow_design::{MENU_DARK, MENU_LIGHT, MenuPalette};

    fn raster(source: &str, width: i32, height: i32) -> Vec<u8> {
        let mut dom = svg::Dom::from_bytes(source.as_bytes(), FontMgr::empty())
            .expect("the icon SVG must parse");
        dom.set_container_size((width, height));
        let mut root = dom.root();
        root.set_width(svg::Length::new(width as f32, svg::LengthUnit::PX));
        root.set_height(svg::Length::new(height as f32, svg::LengthUnit::PX));

        let mut surface = raster_n32_premul((width, height)).expect("raster surface");
        dom.render(surface.canvas());

        let mut pixels = vec![0; (width * height * 4) as usize];
        let info = ImageInfo::new(
            (width, height),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        assert!(
            surface.read_pixels(&info, &mut pixels, (width * 4) as usize, (0, 0)),
            "could not read the rasterized icon"
        );
        pixels
    }

    fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> [u8; 4] {
        let offset = ((y * width + x) * 4) as usize;
        pixels[offset..offset + 4].try_into().unwrap()
    }

    fn assert_close(actual: [u8; 4], expected: [u8; 4], label: &str, theme: &str) {
        const TOLERANCE: u8 = 12;
        assert!(
            actual[3] > 200,
            "{theme}: {label} is transparent: {actual:?}"
        );
        for (a, e) in actual[..3].iter().zip(&expected[..3]) {
            assert!(
                a.abs_diff(*e) <= TOLERANCE,
                "{theme}: {label} is {actual:?}, expected ≈{expected:?}"
            );
        }
    }

    fn themes() -> [(&'static str, MenuPalette); 2] {
        [("claro", MENU_LIGHT), ("oscuro", MENU_DARK)]
    }

    #[test]
    fn every_icon_is_a_16px_svg() {
        for icon in [
            HALF_LEFT,
            HALF_RIGHT,
            HALF_TOP,
            HALF_BOTTOM,
            QUARTER_TOP_LEFT,
            QUARTER_TOP_RIGHT,
            QUARTER_BOTTOM_LEFT,
            QUARTER_BOTTOM_RIGHT,
            HALVES_LEFT_RIGHT,
            HALVES_RIGHT_LEFT,
            HALVES_TOP_BOTTOM,
            HALVES_BOTTOM_TOP,
            GRID,
            SPLIT,
            INFO,
            GEAR,
            HELP,
            SEARCH,
            TRASH,
            HOUSE,
            DESKTOP,
            DOCUMENTS,
            DOWNLOADS,
            PICTURES,
            MUSIC,
            VIDEOS,
            SHARE,
            EJECT,
            PRINTER,
            EMOJI,
            VIEW_LIST,
            VIEW_COLUMNS,
            VIEW_GALLERY,
            APPS,
            BOOK,
            KEYBOARD,
        ] {
            assert!(icon.starts_with("<svg"), "icon does not start with <svg>");
            assert!(icon.ends_with("</svg>"), "icon does not end with </svg>");
            assert!(icon.contains("viewBox=\"0 0 16 16\""));
            assert!(icon.contains("currentColor"));
        }
    }

    #[test]
    fn menu_item_icon_paints_frame_text_and_accent_bar() {
        for (theme, m) in themes() {
            let svg = search_icon_svg(MENU_ITEM, m.fg, m.hover_bg);
            assert!(svg.contains("rgb("), "{theme}: no rgb() colour in {svg}");
            assert!(
                !svg.contains("#00000000"),
                "{theme}: eight-digit hex leaked into the icon"
            );
            let pixels = raster(&svg, 16, 14);

            assert_close(pixel(&pixels, 16, 0, 7), m.fg.to_rgba8(), "frame", theme);
            assert_close(
                pixel(&pixels, 16, 8, 5),
                m.hover_bg.to_rgba8(),
                "accent bar",
                theme,
            );
            assert_eq!(
                pixel(&pixels, 16, 8, 3)[3],
                0,
                "{theme}: the icon background is not transparent"
            );
        }
    }

    #[test]
    fn menu_item_icon_uses_white_when_focused_and_faint_when_disabled() {
        for (theme, m) in themes() {
            let focused = search_icon_svg(MENU_ITEM, m.hover_fg, m.hover_fg);
            let pixels = raster(&focused, 16, 14);
            assert_close(
                pixel(&pixels, 16, 0, 7),
                m.hover_fg.to_rgba8(),
                "focused frame",
                theme,
            );
            assert_close(
                pixel(&pixels, 16, 8, 5),
                m.hover_fg.to_rgba8(),
                "focused bar",
                theme,
            );

            let disabled = search_icon_svg(MENU_ITEM, m.disabled_fg, m.disabled_fg);
            let pixels = raster(&disabled, 16, 14);
            assert_close(
                pixel(&pixels, 16, 8, 5),
                m.disabled_fg.to_rgba8(),
                "disabled bar",
                theme,
            );
        }
    }

    #[test]
    fn help_icon_paints_the_text_colour() {
        for (theme, m) in themes() {
            let [r, g, b, _]: [u8; 4] = m.fg.to_rgba8();
            let svg = HELP.replace("currentColor", &format!("rgb({r},{g},{b})"));
            let pixels = raster(&svg, 16, 16);
            assert_close(
                pixel(&pixels, 16, 1, 8),
                m.fg.to_rgba8(),
                "help ring",
                theme,
            );
            assert_eq!(
                pixel(&pixels, 16, 8, 3)[3],
                0,
                "{theme}: the help background is not transparent"
            );
        }
    }
}
