//! Yuki's system defaults: the one place every visual value lives.
//!
//! These are compile-time constants, not a theme system. There is exactly one
//! look, in three modes ([`LIGHT`], [`DARK`] and [`DARK_TINT`]); the values are
//! generated at build time from the vendored Neptunia JSON. Every other crate
//! reads design values through this module and must not define its own
//! (ADR 0011).
//!
//! Colors are [`Rgba`] with `f32` components in `0.0..=1.0`, ready to hand to
//! a shader without a per-frame conversion. The literals are written with
//! [`Rgba::rgb8`] / [`Rgba::rgba8`] so they read like the hex and `rgba(...)`
//! values they were taken from. Every numeric field states its unit in its
//! documentation: lengths are logical pixels, font sizes are points, an
//! exponent and opacities are unitless.
//!
//! # Where the values come from
//!
//! The desktop design is **Neptunia** (`crates/yuki-design/neptunia`): `tokens.json` holds colors, radii, spaces, typography and
//! shadows; `components.json` holds component metrics. Its window is a 44 px
//! top titlebar with 12 px traffic lights and a 16 px corner radius; the
//! server-side chrome uses its own 32 px bar. `build.rs` turns those JSON
//! files into the constants included at the end of this module (ADR 0032).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// A color with `0.0..=1.0` components, ready for a shader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    /// Red, `0.0..=1.0`.
    pub r: f32,
    /// Green, `0.0..=1.0`.
    pub g: f32,
    /// Blue, `0.0..=1.0`.
    pub b: f32,
    /// Alpha, `0.0` transparent to `1.0` opaque.
    pub a: f32,
}

impl Rgba {
    /// Mixes toward `other` by `amount` (`0.0` keeps this color, `1.0` is the
    /// other one); alpha stays this color's.
    pub fn mix(self, other: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * amount,
            g: self.g + (other.g - self.g) * amount,
            b: self.b + (other.b - self.b) * amount,
            a: self.a,
        }
    }

    /// Mixes toward black by `amount`.
    pub fn darken(self, amount: f32) -> Self {
        self.mix(Self::rgb8(0, 0, 0), amount)
    }

    /// Mixes toward white by `amount`.
    pub fn lighten(self, amount: f32) -> Self {
        self.mix(Self::rgb8(255, 255, 255), amount)
    }

    /// Builds an opaque color from 8-bit channels.
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgba8(r, g, b, 1.0)
    }

    /// Builds a color from 8-bit channels and a `0.0..=1.0` alpha.
    pub const fn rgba8(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a,
        }
    }

    /// The 8-bit RGBA bytes the CPU rasterizers upload.
    pub fn to_rgba8(self) -> [u8; 4] {
        [
            (self.r * 255.0).round().clamp(0.0, 255.0) as u8,
            (self.g * 255.0).round().clamp(0.0, 255.0) as u8,
            (self.b * 255.0).round().clamp(0.0, 255.0) as u8,
            (self.a * 255.0).round().clamp(0.0, 255.0) as u8,
        ]
    }
}

/// One color in its focused and unfocused variants.
///
/// A window has two states, and any color that differs between them lives here
/// as a pair. The unfocused value is its own constant, never a multiplier
/// applied later in render code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusPair {
    /// The color when the window has focus.
    pub focused: Rgba,
    /// The color when the window does not have focus.
    pub unfocused: Rgba,
}

/// Which edge a titlebar occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarPosition {
    /// Along the left edge; the title reads bottom to top.
    Left,
    /// Along the right edge; the title reads bottom to top.
    Right,
    /// Along the top edge, horizontal.
    Top,
    /// Along the bottom edge, horizontal.
    Bottom,
    /// No titlebar is drawn.
    None,
}

/// How a title is oriented when the titlebar is vertical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleRotation {
    /// Ordinary horizontal text.
    Horizontal,
    /// Rotated to read bottom to top, the default for vertical titlebars.
    BottomToTop,
}

/// A font face and size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    /// Family name.
    pub family: &'static str,
    /// Size in points.
    pub size: u32,
    /// Numeric weight, as CSS/fontconfig: 400 regular, 500 medium, 600
    /// semibold, 650, 700 bold.
    pub weight: u16,
}

/// What a titlebar button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Close the window.
    Close,
    /// Minimize the window.
    Minimize,
    /// Maximize the window.
    Maximize,
}

/// A titlebar button: its function and its color when the window is focused.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Button {
    /// What the button does.
    pub kind: ButtonKind,
    /// Color when the window has focus.
    pub color: Rgba,
}

/// One straight line of a traffic-light glyph, in the glyph's canvas units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Start point.
    pub a: (f32, f32),
    /// End point.
    pub b: (f32, f32),
}

/// One filled triangle of a traffic-light glyph, in the glyph's canvas units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle(pub [(f32, f32); 3]);

/// The geometry of the close, minimize and zoom traffic-light glyphs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrafficGlyphs {
    /// Side of the square glyph canvas, centred in the traffic light.
    pub box_size: f32,
    /// Stroke width in canvas units; ends are rounded.
    pub stroke: f32,
    /// The two diagonal strokes of the close glyph.
    pub close: &'static [Segment],
    /// The horizontal stroke of the minimize glyph.
    pub minimize: &'static [Segment],
    /// The two triangles of the zoom glyph.
    pub zoom: &'static [Triangle],
}

/// The titlebar double-click gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoubleClick {
    /// Maximum time between clicks, in milliseconds.
    pub interval_ms: u32,
    /// Maximum pointer movement between clicks, in logical pixels.
    pub slop: f64,
}

/// The titlebar buttons, their order along the bar and their shared metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TitlebarButtons {
    /// Buttons in order from the start of the titlebar; top to bottom when the
    /// titlebar is vertical.
    pub order: [Button; 3],
    /// Button diameter, in logical pixels.
    pub diameter: u32,
    /// Gap between adjacent buttons, in logical pixels.
    pub spacing: u32,
    /// Corner radius of each button, in logical pixels.
    pub corner_radius: u32,
    /// Gap before the first button, in logical pixels.
    pub margin_start: u32,
}

/// Titlebar placement, metrics and title typography.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Titlebar {
    /// Which edge the titlebar occupies.
    pub position: TitlebarPosition,
    /// Thickness of the bar, in logical pixels.
    pub thickness: u32,
    /// The buttons and their metrics.
    pub buttons: TitlebarButtons,
    /// The typeface used for the title.
    pub font: Font,
    /// How the title is oriented.
    pub rotation: TitleRotation,
}

/// A window's shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowStyle {
    /// Corner radius, in logical pixels.
    pub corner_radius: u32,
    /// Superellipse exponent, unitless. `2.0` is an ellipse; larger values are
    /// squarer.
    pub superellipse_exponent: f32,
}

/// One layer of a window shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayer {
    /// Blur radius, in logical pixels.
    pub blur: u32,
    /// Vertical offset, in logical pixels; positive is downward.
    pub y_offset: i32,
    /// Opacity, unitless `0.0..=1.0`.
    pub opacity: f32,
}

/// The two-layer window shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowStyle {
    /// The wide, soft layer.
    pub ambient: ShadowLayer,
    /// The tight, dark layer that anchors the window to its surface.
    pub contact: ShadowLayer,
    /// Scale applied to the shadow when the window is unfocused, unitless.
    pub unfocused_scale: f32,
}

/// The colors of one mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// Titlebar background, focused and unfocused.
    pub titlebar_bg: FocusPair,
    /// Hairline border along the titlebar.
    pub hairline: Rgba,
    /// Inner line. Present in dark mode, absent in light mode: it is an
    /// [`Option`] rather than a transparent color so the renderer can skip the
    /// pass entirely.
    pub inner_line: Option<Rgba>,
    /// Window title text, focused and unfocused.
    pub title: FocusPair,
    /// Body text, focused and unfocused.
    pub text: FocusPair,
    /// The accent color.
    pub accent: Rgba,
    /// The traffic lights when the window does not have focus. Neptunia uses a
    /// different grey per theme, so it belongs to the palette, not the bar.
    pub traffic_lights_inactive: Rgba,
}
/// The menubar and dock colours for one appearance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShellPalette {
    /// The bar surface, translucent like macOS vibrancy.
    pub bar: Rgba,
    /// The bar text.
    pub text: Rgba,
    /// A dock tile.
    pub tile: Rgba,
    /// An active dock tile.
    pub tile_active: Rgba,
    /// Text on an active dock tile.
    pub tile_text_active: Rgba,
}

/// The colours and metrics of a Neptunia menu popup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuPalette {
    /// Panel background (`bg-raised`).
    pub bg: Rgba,
    /// Panel corner radius.
    pub radius: f32,
    /// Panel padding.
    pub padding: f32,
    /// Panel border width (outer + inner line).
    pub border_width: f32,
    /// Panel outer border colour (`window-outer`).
    pub outer_border: Rgba,
    /// Panel inner line colour (`window-inner`, transparent in light mode).
    pub inner_line: Rgba,
    /// Panel shadow colour.
    pub shadow: Rgba,
    /// Panel minimum logical width.
    pub min_width: f32,
    /// Panel maximum logical width.
    pub max_width: f32,
    /// Item height.
    pub item_height: f32,
    /// Item corner radius.
    pub item_radius: f32,
    /// Item horizontal padding.
    pub item_padding_x: f32,
    /// Item font size.
    pub font_size: f32,
    /// Gap between label and shortcut columns.
    pub shortcut_gap: f32,
    /// Horizontal offset of a child submenu from the parent edge.
    pub submenu_offset: f32,
    /// Item text colour (`ink`).
    pub fg: Rgba,
    /// Shortcut text colour (`ink-muted`).
    pub shortcut_color: Rgba,
    /// Item hover background (`accent`).
    pub hover_bg: Rgba,
    /// Item hover foreground (`on-accent`).
    pub hover_fg: Rgba,
    /// Open menubar-title background (`bg-hover`).
    pub bar_hover_bg: Rgba,
    /// Open menubar-title foreground (`ink-strong`).
    pub bar_hover_fg: Rgba,
    /// Disabled item foreground (`ink-faint`).
    pub disabled_fg: Rgba,
    /// Separator line colour (`line`).
    pub separator_color: Rgba,
    /// Separator line height.
    pub separator_height: f32,
    /// Separator horizontal margin.
    pub separator_margin_x: f32,
    /// Separator vertical margin.
    pub separator_margin_y: f32,
    /// Width reserved for the check/radio mark column.
    pub mark_width: f32,
    /// Radio dot diameter.
    pub mark_dot: f32,
    /// Check glyph font size.
    pub mark_check: f32,
    /// Submenu chevron size.
    pub chevron_size: f32,
    /// Submenu chevron colour (`ink-muted`).
    pub chevron_color: Rgba,
}

/// The lock screen colours: Neptunia `bg-surface`, `ink`, `ink-muted`,
/// `danger` and `accent` for the focused field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LockPalette {
    /// The screen and field surface (`bg-surface`).
    pub bg_surface: Rgba,
    /// Title and masked input (`ink`).
    pub ink: Rgba,
    /// Labels and status (`ink-muted`).
    pub ink_muted: Rgba,
    /// A refused attempt (`danger`).
    pub danger: Rgba,
    /// The focused field line (`accent`).
    pub accent: Rgba,
}

/// The pointer scale while the user shakes the mouse to locate it, like
/// macOS Accessibility > Pointer Control. macOS grows the pointer to roughly
/// three times its normal size; normal drawing uses `1.0`.
pub const CURSOR_SHAKE_SCALE: f32 = 3.0;

/// How long the shake enlargement takes to grow, in milliseconds.
pub const CURSOR_SHAKE_GROW_MS: u64 = 180;

/// How long the pointer takes to shrink back, in milliseconds.
pub const CURSOR_SHAKE_SHRINK_MS: u64 = 220;

/// The interval between animation frames, in milliseconds (roughly 60 Hz).
pub const CURSOR_SHAKE_TICK_MS: u64 = 16;

/// Dim over everything outside the screenshot selection.
pub const SCREENSHOT_DIM: [f32; 4] = [0.0, 0.0, 0.0, 0.35];

/// Lighter dim shown while the region can still be dragged.
pub const SCREENSHOT_IDLE_DIM: [f32; 4] = [0.0, 0.0, 0.0, 0.15];

/// The screenshot selection border.
pub const SCREENSHOT_BORDER: [f32; 4] = [1.0, 1.0, 1.0, 0.9];

/// The screenshot selection border width, in logical pixels.
pub const SCREENSHOT_BORDER_WIDTH: i32 = 1;

/// Where the wallpapers Yuki ships are installed, like GNOME's
/// `/usr/share/backgrounds`. The flake installs `share/wallpapers/` under
/// `/run/current-system/sw/share/wallpapers`.
pub const WALLPAPER_DIR: &str = "/run/current-system/sw/share/wallpapers/solid-colors";

/// The app-id prefix of Yuki's own CelestialKit applications (ADR 0035).
///
/// It only decides whether the compositor draws a titlebar; it is not a
/// security boundary. Infinity will replace it when it exists.
pub const NATIVE_APP_ID_PREFIX: &str = "io.github.hblanqueto.yuki.";

/// The wallpaper a session shows when the store selects none: a fresh
/// account's desktop, like macOS ships one default picture. A file name inside
/// [`WALLPAPER_DIR`] (or inside a custom `wallpaper.dir`).
pub const WALLPAPER: &str = "Solid Aqua Blue.png";

include!(concat!(env!("OUT_DIR"), "/neptunia.rs"));

#[cfg(test)]
mod tests {
    #[test]
    fn traffic_glyphs_have_the_expected_shapes() {
        assert_eq!(super::TRAFFIC_GLYPHS.close.len(), 2);
        assert_eq!(super::TRAFFIC_GLYPHS.minimize.len(), 1);
        assert_eq!(super::TRAFFIC_GLYPHS.zoom.len(), 2);
    }
}
