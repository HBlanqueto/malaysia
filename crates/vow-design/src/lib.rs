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
//! The desktop design is **Neptunia** (`crates/vow-design/neptunia`): `tokens.json` holds colors, radii, spaces, typography and
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

    /// Returns premultiplied RGBA for OpenGL/Smithay blending.
    pub fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
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

/// The geometry of the close, minimize, zoom and plus traffic-light glyphs.
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
    /// The two crossed strokes of the plus glyph for fixed-width windows.
    pub plus: &'static [Segment],
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
    /// CSS blur radius, in logical pixels (`sigma = blur / 2`).
    pub blur: u32,
    /// Vertical offset, in logical pixels; positive is downward.
    pub y_offset: i32,
    /// Opacity when the window is focused, unitless `0.0..=1.0`.
    pub opacity: f32,
    /// The shadow tint; its alpha is not used.
    pub color: Rgba,
}

/// Compile-time metrics for the System Settings window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SettingsMetrics {
    /// Default settings window width, in logical pixels.
    pub window_width: u32,
    /// Default settings window height, in logical pixels.
    pub window_height: u32,
    /// Minimum settings window height, in logical pixels.
    pub window_min_height: u32,
    /// Search field height, in logical pixels.
    pub search_height: u32,
    /// Horizontal padding inside the search field, in logical pixels.
    pub search_padding_x: u32,
    /// Search icon size, in logical pixels.
    pub search_icon: u32,
    /// Account avatar size, in logical pixels.
    pub account_avatar: u32,
    /// Gap between the avatar and the account text, in logical pixels.
    pub account_gap: u32,
    /// Font for the account name.
    pub account_name_font: Font,
    /// Font for the account detail line.
    pub account_detail_font: Font,
    /// Gap between the account row and the first section, in logical pixels.
    pub section_gap: u32,
    /// Background colour of the Appearance section icon.
    pub section_icon_bg: Rgba,
    /// Group container corner radius, in logical pixels.
    pub group_radius: u32,
    /// Minimum height of a grouped row, in logical pixels.
    pub group_row_min_height: u32,
    /// Horizontal padding of a grouped row, in logical pixels.
    pub group_row_padding_x: u32,
    /// Vertical padding of a grouped row, in logical pixels.
    pub group_row_padding_y: u32,
    /// Top margin of a group title, in logical pixels.
    pub group_title_margin_top: u32,
    /// Horizontal padding inside a group container, in logical pixels.
    pub group_content_padding_x: u32,
    /// Accent color swatch size, in logical pixels.
    pub swatch_size: u32,
    /// Gap between accent color swatches, in logical pixels.
    pub swatch_gap: u32,
    /// Font used for swatch labels.
    pub swatch_name_font: Font,
    /// Accent preview card width, in logical pixels.
    pub card_width: u32,
    /// Accent preview card height, in logical pixels.
    pub card_height: u32,
    /// Accent preview card corner radius, in logical pixels.
    pub card_radius: u32,
    /// Gap between accent preview cards, in logical pixels.
    pub card_gap: u32,
    /// Selected accent card ring width, in logical pixels.
    pub card_ring: u32,
    /// Gap between the ring and the card it surrounds, in logical pixels.
    pub card_ring_gap: u32,
    /// Gap between a card preview and its caption, in logical pixels.
    pub card_caption_gap: u32,
    /// Font used for card captions.
    pub card_caption_font: Font,
    /// X offset of the theme preview window inside its card.
    pub preview_window_x: u32,
    /// Y offset of the theme preview window inside its card.
    pub preview_window_y: u32,
    /// Corner radius of the theme preview window.
    pub preview_window_radius: u32,
    /// Width of the accent bar in the theme preview.
    pub preview_bar_w: u32,
    /// Height of the accent bar in the theme preview.
    pub preview_bar_h: u32,
    /// Inset of the accent bar in the theme preview.
    pub preview_bar_inset: u32,
    /// Size of each traffic light in the theme preview.
    pub preview_light: u32,
    /// Gap between traffic lights in the theme preview.
    pub preview_light_gap: u32,
    /// X offset of the window-colour preview window.
    pub preview_tint_window_x: u32,
    /// Y offset of the window-colour preview window.
    pub preview_tint_window_y: u32,
    /// Width of the tinted side strip in the window-colour preview.
    pub preview_tint_side_w: u32,
    /// Radio button outer size, in logical pixels.
    pub radio_size: u32,
    /// Radio button inner dot size, in logical pixels.
    pub radio_dot: u32,
    /// Gap between a radio button and its label, in logical pixels.
    pub radio_gap: u32,
    /// Horizontal spacing between radio buttons, in logical pixels.
    pub radio_spacing: u32,
    /// Popup button height, in logical pixels.
    pub popup_height: u32,
    /// Popup chevron icon size, in logical pixels.
    pub popup_chevron: u32,
    /// Gap between traffic-light squares in the popup button, in logical pixels.
    pub popup_light_gap: u32,
    /// Gap between the popup button text and its chevron capsule, in logical pixels.
    pub popup_text_gap: u32,
    /// How much the resolved accent is mixed with the dark sidebar background
    /// for selected rows.
    pub selected_dark_mix: f32,
    /// Gap below the traffic lights in the settings sidebar, in logical pixels.
    pub sidebar_lights_gap: u32,
    /// Gap below the search field in the settings sidebar, in logical pixels.
    pub sidebar_search_gap: u32,
    /// Gap between the label and the accessory in a settings sidebar item, in
    /// logical pixels.
    pub sidebar_item_gap: u32,
    /// Gap between the toolbar capsule and the section title, in logical pixels.
    pub toolbar_gap: u32,
    /// Top padding of the settings content scroll view, in logical pixels.
    pub content_padding_top: u32,
    /// Bottom padding of the settings content scroll view, in logical pixels.
    pub content_padding_bottom: u32,
    /// Gap between groups in the settings content, in logical pixels.
    pub content_gap: u32,
    /// Bottom margin of a group title, in logical pixels.
    pub group_title_margin_bottom: u32,
    /// Left indent of a group title, in logical pixels.
    pub group_title_indent: u32,
    /// Gap between a row title and its description, in logical pixels.
    pub group_desc_gap: u32,
    /// Width of an accent swatch column, in logical pixels.
    pub swatch_column: u32,
    /// Gap between a swatch and its optional label, in logical pixels.
    pub swatch_name_gap: u32,
    /// Outline colour of a settings preview card.
    pub card_outline: Rgba,
}

/// Compile-time metrics for the Neptunia sidebar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SidebarMetrics {
    /// `true` for the island default, `false` for the flat source list.
    pub default_island: bool,
    /// Sidebar width, in logical pixels.
    pub width: u32,
    /// Island inset from top/left/bottom, in logical pixels.
    pub inset: u32,
    /// Horizontal padding inside the sidebar (island and flat), in logical pixels.
    pub padding_x: u32,
    /// Corner radius of the island, in logical pixels.
    pub radius: u32,
    /// Outer rim width, in logical pixels.
    pub rim_width: f32,
    /// Inner line width, in logical pixels.
    pub inner_width: f32,
    /// Backdrop blur radius for the island, in logical pixels.
    pub blur: u32,
    /// Traffic-light inset from the island/edge left, in logical pixels.
    pub traffic_x: u32,
    /// Traffic-light inset from the island/edge top, in logical pixels.
    pub traffic_y: u32,
    /// Item row height, in logical pixels.
    pub item_height: u32,
    /// Item row corner radius, in logical pixels.
    pub item_radius: u32,
    /// Horizontal padding inside an item row, in logical pixels.
    pub item_padding_x: u32,
    /// Gap between item icon and label, in logical pixels.
    pub item_gap: u32,
    /// Item label text size, in points.
    pub item_text_size: u32,
    /// Counter text size, in points.
    pub count_size: u32,
    /// Size of a regular stroke icon, in logical pixels.
    pub item_icon: u32,
    /// Size of an app icon, in logical pixels.
    pub app_icon: u32,
    /// Section header height, in logical pixels.
    pub section_height: u32,
    /// Margin before a section header, in logical pixels.
    pub section_margin_top: u32,
    /// Section header horizontal padding, in logical pixels.
    pub section_padding_x: u32,
    /// Section header text size, in points.
    pub section_size: u32,
    /// Section header text weight.
    pub section_weight: u16,
    /// Disclosable arrow size, in logical pixels.
    pub chevron: u32,
    /// Disclosable arrow stroke width, in logical pixels.
    pub chevron_stroke: f32,
}

/// Compile-time metrics for a toolbar capsule group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolgroupMetrics {
    /// Capsule height, in logical pixels.
    pub height: u32,
    /// Horizontal padding inside the capsule, in logical pixels.
    pub padding_x: u32,
    /// Button width inside the capsule, in logical pixels.
    pub button_w: u32,
    /// Button height inside the capsule, in logical pixels.
    pub button_h: u32,
    /// Separator line width, in logical pixels.
    pub separator_w: u32,
    /// Separator line height, in logical pixels.
    pub separator_h: u32,
    /// Flat variant button width, in logical pixels.
    pub flat_button_w: u32,
    /// Flat variant button height, in logical pixels.
    pub flat_button_h: u32,
    /// Flat variant corner radius, in logical pixels.
    pub flat_radius: u32,
}

/// The two-layer window shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowStyle {
    /// The wide, soft layer.
    pub ambient: ShadowLayer,
    /// The tight, dark layer that anchors the window to its surface.
    pub contact: ShadowLayer,
    /// Blur multiplier for an unfocused window, unitless.
    pub unfocused_blur_scale: f32,
    /// Offset multiplier for an unfocused window, unitless.
    pub unfocused_offset_scale: f32,
    /// Opacity multiplier for an unfocused window, unitless.
    pub unfocused_opacity_scale: f32,
}

impl ShadowStyle {
    /// The two layers, scaled like macOS when the window is not focused.
    pub fn layers(self, focused: bool) -> [ShadowLayer; 2] {
        if focused {
            return [self.ambient, self.contact];
        }
        let unfocused = |layer: ShadowLayer| ShadowLayer {
            blur: (layer.blur as f32 * self.unfocused_blur_scale).round() as u32,
            y_offset: (layer.y_offset as f32 * self.unfocused_offset_scale).round() as i32,
            opacity: layer.opacity * self.unfocused_opacity_scale,
            ..layer
        };
        [unfocused(self.ambient), unfocused(self.contact)]
    }
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
    /// Outer border drawn around the whole window.
    pub outer_border: Rgba,
    /// Window title text, focused and unfocused.
    pub title: FocusPair,
    /// Body text, focused and unfocused.
    pub text: FocusPair,
    /// Strong text colour (`ink-strong`).
    pub ink_strong: Rgba,
    /// Separator and outline colour (`line`).
    pub line: Rgba,
    /// The accent color.
    pub accent: Rgba,
    /// Text or glyphs drawn on top of the accent color.
    pub on_accent: Rgba,
    /// Danger/error colour.
    pub danger: Rgba,
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
    /// Open menubar-title pill background (`shell.menubar.title-open`).
    pub bar_title_open_bg: Rgba,
    /// Open menubar-title pill corner radius (`radius.sm`).
    pub bar_title_open_radius: f32,
    /// Open menubar-title pill height, centred on the bar.
    pub bar_title_open_height: f32,
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
    /// Leading icon size in the mark column.
    pub icon_size: f32,
    /// Group header font size.
    pub header_font_size: f32,
    /// Group header font weight.
    pub header_weight: i32,
    /// Group header row height.
    pub header_height: f32,
    /// Search field height.
    pub search_height: f32,
    /// Vertical margin around the search field.
    pub search_margin_y: f32,
    /// Search field accent ring width.
    pub search_ring_width: f32,
    /// Search field corner radius.
    pub search_radius: f32,
    /// Search field background (`bg-control`).
    pub search_bg: Rgba,
    /// Search field accent ring.
    pub search_ring: Rgba,
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

/// The colours of a Neptunia sidebar for one mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SidebarPalette {
    /// Item text colour (`ink`).
    pub item_ink: Rgba,
    /// Item icon stroke colour (`accent-strong`).
    pub item_icon: Rgba,
    /// Selected item background (`accent-soft`).
    pub selected_bg: Rgba,
    /// Selected item text and icon colour (`accent-strong`).
    pub selected_ink: Rgba,
    /// Hover background (`bg-hover`).
    pub hover_bg: Rgba,
    /// Section header text colour (`ink-muted`).
    pub section_ink: Rgba,
    /// Counter text colour (`ink-muted`).
    pub count_ink: Rgba,
    /// Flat mode separator line (`line`).
    pub line: Rgba,
}

/// The colours of a Neptunia toolbar capsule for one mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolgroupPalette {
    /// Enabled button icon/text colour (`ink-strong`).
    pub ink: Rgba,
    /// Disabled button icon/text colour (`ink-faint`).
    pub disabled_ink: Rgba,
    /// Vertical separator line (`line`).
    pub separator: Rgba,
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

/// The app-id prefix of Malaysia's own BarrierKit applications (ADR 0035).
///
/// It only decides whether the compositor draws a titlebar; it is not a
/// security boundary. Infinity will replace it when it exists.
pub const NATIVE_APP_ID_PREFIX: &str = "io.github.hblanqueto.malaysia.";

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
        assert_eq!(super::TRAFFIC_GLYPHS.plus.len(), 2);
    }

    #[test]
    fn generated_shadows_match_the_tokens() {
        let tokens: serde_json::Value =
            serde_json::from_str(include_str!("../neptunia/tokens.json"))
                .expect("tokens.json should parse");

        for (style, theme) in [(super::SHADOW, "light"), (super::SHADOW_DARK, "dark")] {
            for (layer, name) in [(style.ambient, "ambient"), (style.contact, "contact")] {
                let json = &tokens["shadow"]["layers"][theme][name];
                assert_eq!(u64::from(layer.blur), json["blur"].as_u64().unwrap());
                assert_eq!(
                    i64::from(layer.y_offset),
                    json["y-offset"].as_i64().unwrap()
                );
                assert!(
                    (f64::from(layer.opacity) - json["opacity"].as_f64().unwrap()).abs() < 1e-6
                );
                let hex = json["color"].as_str().unwrap().trim_start_matches('#');
                let expected = [
                    u8::from_str_radix(&hex[0..2], 16).unwrap(),
                    u8::from_str_radix(&hex[2..4], 16).unwrap(),
                    u8::from_str_radix(&hex[4..6], 16).unwrap(),
                ];
                assert_eq!(layer.color.to_rgba8()[..3], expected);
            }
        }

        let unfocused = &tokens["shadow"]["unfocused"];
        let scales = [
            (
                f64::from(super::SHADOW.unfocused_blur_scale),
                unfocused["blur"].as_f64().unwrap(),
            ),
            (
                f64::from(super::SHADOW.unfocused_offset_scale),
                unfocused["offset"].as_f64().unwrap(),
            ),
            (
                f64::from(super::SHADOW.unfocused_opacity_scale),
                unfocused["opacity"].as_f64().unwrap(),
            ),
        ];
        for (generated, json) in scales {
            assert!((generated - json).abs() < 1e-6);
        }

        let [ambient, contact] = super::SHADOW.layers(false);
        assert_eq!(
            ambient.blur,
            (super::SHADOW.ambient.blur as f32 * super::SHADOW.unfocused_blur_scale).round() as u32
        );
        assert_eq!(
            contact.y_offset,
            (super::SHADOW.contact.y_offset as f32 * super::SHADOW.unfocused_offset_scale).round()
                as i32
        );
    }

    #[test]
    fn premultiplied_opaque_is_unchanged() {
        let c = super::Rgba::rgb8(128, 64, 255);
        let [r, g, b, a] = c.premultiplied();
        assert!((r - c.r).abs() < 0.004);
        assert!((g - c.g).abs() < 0.004);
        assert!((b - c.b).abs() < 0.004);
        assert!((a - c.a).abs() < 0.001);
    }

    #[test]
    fn premultiplied_honours_alpha() {
        let c = super::Rgba::rgba8(255, 128, 0, 0.5);
        let [r, g, b, a] = c.premultiplied();
        assert!((r - 0.5).abs() < 0.004, "red should be halved");
        assert!((g - 0.25).abs() < 0.004, "green should be halved");
        assert!(b < 0.004);
        assert!((a - 0.5).abs() < 0.001);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn sidebar_metrics_come_from_tokens() {
        assert_eq!(super::SIDEBAR.inset, 8);
        assert_eq!(super::SIDEBAR.padding_x, 10);
        assert_eq!(super::SIDEBAR.radius, 12);
        assert_eq!(super::SIDEBAR.width, 220);
        assert!(super::SIDEBAR.default_island);
        assert_eq!(super::SIDEBAR.item_padding_x, 8);
        assert_eq!(super::SIDEBAR.item_gap, 8);
        assert_eq!(super::SIDEBAR.item_text_size, 13);
        assert_eq!(super::SIDEBAR.section_padding_x, 8);
        assert!((super::SIDEBAR.chevron_stroke - 1.8).abs() < 1e-6);
        assert_eq!(super::TOOLGROUP.height, 34);
        assert_eq!(super::TOOLGROUP.padding_x, 3);
        assert_eq!(super::TOOLGROUP.flat_button_w, 30);
        assert_eq!(super::TOOLGROUP.flat_button_h, 28);
        assert_eq!(super::TOOLGROUP.flat_radius, 8);
    }

    #[test]
    fn settings_metrics_and_swatches_match_components() {
        assert_eq!(super::SETTINGS.window_width, 760);
        assert_eq!(super::SETTINGS.window_height, 620);
        assert_eq!(super::SETTINGS.window_min_height, 480);
        assert_eq!(super::SETTINGS.search_height, 30);
        assert_eq!(super::SETTINGS.search_padding_x, 10);
        assert_eq!(super::SETTINGS.search_icon, 16);
        assert_eq!(super::SETTINGS.account_avatar, 36);
        assert_eq!(super::SETTINGS.account_gap, 10);
        assert_eq!(super::SETTINGS.account_name_font, super::HEADLINE);
        assert_eq!(super::SETTINGS.account_detail_font, super::CALLOUT);
        assert_eq!(super::SETTINGS.section_gap, 14);
        assert_eq!(
            super::SETTINGS.section_icon_bg,
            super::Rgba::rgb8(0x2b, 0x2b, 0x2b)
        );
        assert_eq!(super::SETTINGS.group_radius, 16);
        assert_eq!(super::SETTINGS.group_row_min_height, 44);
        assert_eq!(super::SETTINGS.group_row_padding_x, 14);
        assert_eq!(super::SETTINGS.group_row_padding_y, 10);
        assert_eq!(super::SETTINGS.group_title_margin_top, 16);
        assert_eq!(super::SETTINGS.group_content_padding_x, 16);
        assert_eq!(super::SETTINGS.swatch_size, 22);
        assert_eq!(super::SETTINGS.swatch_gap, 9);
        assert_eq!(super::SETTINGS.swatch_name_font, super::CAPTION);
        assert_eq!(super::SETTINGS.card_width, 70);
        assert_eq!(super::SETTINGS.card_height, 46);
        assert_eq!(super::SETTINGS.card_radius, 8);
        assert_eq!(super::SETTINGS.card_gap, 10);
        assert_eq!(super::SETTINGS.card_ring, 2);
        assert_eq!(super::SETTINGS.card_ring_gap, 2);
        assert_eq!(super::SETTINGS.card_caption_gap, 5);
        assert_eq!(super::SETTINGS.card_caption_font, super::CALLOUT);
        assert_eq!(super::SETTINGS.preview_window_x, 14);
        assert_eq!(super::SETTINGS.preview_window_y, 12);
        assert_eq!(super::SETTINGS.preview_window_radius, 5);
        assert_eq!(super::SETTINGS.preview_bar_w, 22);
        assert_eq!(super::SETTINGS.preview_bar_h, 4);
        assert_eq!(super::SETTINGS.preview_bar_inset, 5);
        assert_eq!(super::SETTINGS.preview_light, 5);
        assert_eq!(super::SETTINGS.preview_light_gap, 3);
        assert_eq!(super::SETTINGS.preview_tint_window_x, 10);
        assert_eq!(super::SETTINGS.preview_tint_window_y, 9);
        assert_eq!(super::SETTINGS.preview_tint_side_w, 14);
        assert_eq!(super::SETTINGS.radio_size, 16);
        assert_eq!(super::SETTINGS.radio_dot, 6);
        assert_eq!(super::SETTINGS.radio_gap, 7);
        assert_eq!(super::SETTINGS.radio_spacing, 16);
        assert_eq!(super::SETTINGS.popup_height, 26);
        assert_eq!(super::SETTINGS.popup_chevron, 18);
        assert_eq!(super::SETTINGS.popup_light_gap, 5);
        assert_eq!(super::SETTINGS.popup_text_gap, 6);
        assert!((super::SETTINGS.selected_dark_mix - 0.5).abs() < 1e-6);
        assert_eq!(super::SETTINGS.sidebar_lights_gap, 18);
        assert_eq!(super::SETTINGS.sidebar_search_gap, 12);
        assert_eq!(super::SETTINGS.sidebar_item_gap, 2);
        assert_eq!(super::SETTINGS.toolbar_gap, 12);
        assert_eq!(super::SETTINGS.content_padding_top, 4);
        assert_eq!(super::SETTINGS.content_padding_bottom, 24);
        assert_eq!(super::SETTINGS.content_gap, 8);
        assert_eq!(super::SETTINGS.group_title_margin_bottom, 2);
        assert_eq!(super::SETTINGS.group_title_indent, 12);
        assert_eq!(super::SETTINGS.group_desc_gap, 2);
        assert_eq!(super::SETTINGS.swatch_column, 24);
        assert_eq!(super::SETTINGS.swatch_name_gap, 5);
        assert_eq!(super::SETTINGS.card_outline.to_rgba8(), [0, 0, 0, 0x40]);
        assert_eq!(super::ACCENT_SWATCHES.len(), 8);
        assert_eq!(super::ACCENT_SWATCHES[0].0, "Azul");
        assert_eq!(
            super::ACCENT_SWATCHES[1].1,
            super::Rgba::rgb8(0x8e, 0x5b, 0xd8)
        );
        assert_eq!(super::HEADLINE.size, 13);
        assert_eq!(super::HEADLINE.weight, 600);
        assert_eq!(super::CALLOUT.size, 12);
        assert_eq!(super::CALLOUT.weight, 400);
        assert_eq!(super::CAPTION.size, 11);
        assert_eq!(super::CAPTION.weight, 500);
    }

    #[test]
    fn sidebar_island_opaque_matches_bg_sunken() {
        // The opaque island background is now the same as the flat sidebar
        // background (bg-sunken) so it reads darker than the window content.
        assert_eq!(super::SIDEBAR_ISLAND_LIGHT_OPAQUE, super::SIDEBAR_LIGHT);
        assert_eq!(super::SIDEBAR_ISLAND_DARK_OPAQUE, super::SIDEBAR_DARK);
        assert_eq!(
            super::SIDEBAR_ISLAND_DARK_TINT_OPAQUE,
            super::SIDEBAR_DARK_TINT
        );
        assert_eq!(super::SIDEBAR_ISLAND_LIGHT_OPAQUE.a, 1.0);
        assert_eq!(super::SIDEBAR.rim_width, 1.0);
    }

    #[test]
    fn sidebar_island_light_has_no_rim_inner_or_shadow() {
        // In light mode the island sits flush against the window; the rim,
        // inner line and drop shadow are fully transparent.
        assert_eq!(super::SIDEBAR_ISLAND_RIM_LIGHT.a, 0.0);
        assert_eq!(super::SIDEBAR_ISLAND_INNER_LIGHT.a, 0.0);
        assert_eq!(super::SIDEBAR_ISLAND_SHADOW[0].opacity, 0.0);
        assert_eq!(super::SIDEBAR_ISLAND_SHADOW[1].opacity, 0.0);
    }

    #[test]
    fn sidebar_island_dark_has_gray_rim() {
        // In dark modes the island keeps a subtle gray rim (#ffffff21).
        assert_eq!(
            super::SIDEBAR_ISLAND_RIM_DARK.to_rgba8(),
            [0xff, 0xff, 0xff, 0x21]
        );
        assert_eq!(
            super::SIDEBAR_ISLAND_RIM_DARK_TINT.to_rgba8(),
            [0xff, 0xff, 0xff, 0x21]
        );
        assert!(super::SIDEBAR_ISLAND_SHADOW_DARK[0].opacity > 0.0);
        assert_eq!(super::SIDEBAR_ISLAND_SHADOW_DARK[1].opacity, 0.0);
    }
}
