//! SVG icons for the focused-app menus.
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

/// Settings gear.
pub const GEAR: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"8\" cy=\"8\" r=\"4.6\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><circle cx=\"8\" cy=\"8\" r=\"1.9\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\"/><g fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.3\" stroke-linecap=\"round\"><path d=\"M8 1.6v1.8M8 12.6v1.8M1.6 8h1.8M12.6 8h1.8M3.5 3.5l1.3 1.3M11.2 11.2l1.3 1.3M12.5 3.5l-1.3 1.3M4.8 11.2l-1.3 1.3\"/></g></svg>";

#[cfg(test)]
mod tests {
    use super::*;

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
        ] {
            assert!(icon.starts_with("<svg"), "icon does not start with <svg>");
            assert!(icon.ends_with("</svg>"), "icon does not end with </svg>");
            assert!(icon.contains("viewBox=\"0 0 16 16\""));
            assert!(icon.contains("currentColor"));
        }
    }
}
