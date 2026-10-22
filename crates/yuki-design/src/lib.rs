//! Yuki's system defaults: the one place every visual value lives.
//!
//! These are compile-time constants, not a theme system. There is exactly one
//! look, in two modes ([`LIGHT`] and [`DARK`]); there is no config file, no
//! generation step and no runtime override. Every other crate reads design
//! values through this module and must not define its own (ADR 0011).
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
//! The desktop design is **Neptunia** (`~/dotfiles/kits/neptunia` in the
//! author's setup): `tokens.json` holds colors, radii, spaces, typography and
//! shadows; `components.json` holds component metrics. Its window is a 44 px
//! top titlebar with 12 px traffic lights, a 16 px corner radius and the
//! `light`/`dark` palettes copied below. The titlebar lives at the top by
//! default. Until the Rust constants are generated from those JSON files, this
//! module is the approved snapshot; keep them in sync (ADR 0032).

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

/// The titlebar: 32 px at the top for server-decorated client windows, with
/// 12 px traffic lights. Neptunia's own 44 px app-window bar is not this one.
pub const TITLEBAR: Titlebar = Titlebar {
    position: TitlebarPosition::Top,
    thickness: 32,
    buttons: TitlebarButtons {
        order: [
            Button {
                kind: ButtonKind::Close,
                color: Rgba::rgb8(0xff, 0x5f, 0x57),
            },
            Button {
                kind: ButtonKind::Minimize,
                color: Rgba::rgb8(0xfe, 0xbc, 0x2e),
            },
            Button {
                kind: ButtonKind::Maximize,
                color: Rgba::rgb8(0x28, 0xc8, 0x40),
            },
        ],
        diameter: 12,
        spacing: 8,
        corner_radius: 4,
        margin_start: 16,
    },
    font: Font {
        family: "Geist",
        size: 13,
        weight: 600,
    },
    rotation: TitleRotation::Horizontal,
};

/// The window shape: Neptunia's 16 logical pixel rounded corner.
pub const WINDOW: WindowStyle = WindowStyle {
    corner_radius: 16,
    superellipse_exponent: 2.0,
};

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

/// How much the accent tints the window chrome, for light and dark modes
/// (Neptunia `appearance.modes.accent`).
pub const WINDOW_TINT_ACCENT: [f32; 2] = [0.06, 0.08];

/// How much the wallpaper's dominant colour tints the window chrome.
pub const WINDOW_TINT_WALLPAPER: [f32; 2] = [0.07, 0.09];

/// How much the wallpaper tints the panels and dock in `desktop` scope.
pub const PANEL_TINT_WALLPAPER: [f32; 2] = [0.20, 0.26];

/// The hairline rim drawn around each traffic light.
pub const TRAFFIC_RIM: Rgba = Rgba::rgba8(0, 0, 0, 0.20);

/// The traffic-light rim width, in logical pixels.
pub const TRAFFIC_RIM_WIDTH: f32 = 0.5;

/// Neptunia's layer factors: how much each surface kind picks up the window
/// tint. `bg-sunken` and the controls/lines lean into it; raised surfaces
/// barely do.
pub const LAYER_FACTOR_BG_SUNKEN: f32 = 1.4;
/// See [`LAYER_FACTOR_BG_SUNKEN`]; controls (`bg-control`, `bg-hover`).
pub const LAYER_FACTOR_CONTROL: f32 = 1.2;
/// See [`LAYER_FACTOR_BG_SUNKEN`]; lines and borders.
pub const LAYER_FACTOR_LINE: f32 = 1.2;
/// See [`LAYER_FACTOR_BG_SUNKEN`]; raised surfaces (`bg-surface`, `bg-raised`).
pub const LAYER_FACTOR_SURFACE: f32 = 0.6;

/// How much the close/minimize lights darken their source colour.
pub const TRAFFIC_CLOSE_MINIMIZE_DARKEN: f32 = 0.28;

/// How much the zoom light lightens its source colour.
pub const TRAFFIC_ZOOM_LIGHTEN: f32 = 0.58;

/// Dim over everything outside the screenshot selection.
pub const SCREENSHOT_DIM: [f32; 4] = [0.0, 0.0, 0.0, 0.35];

/// Lighter dim shown while the region can still be dragged.
pub const SCREENSHOT_IDLE_DIM: [f32; 4] = [0.0, 0.0, 0.0, 0.15];

/// The screenshot selection border.
pub const SCREENSHOT_BORDER: [f32; 4] = [1.0, 1.0, 1.0, 0.9];

/// The screenshot selection border width, in logical pixels.
pub const SCREENSHOT_BORDER_WIDTH: i32 = 1;

/// The body text face.
pub const TEXT: Font = Font {
    family: "Geist",
    size: 13,
    weight: 400,
};

/// The window shadow in light mode.
pub const SHADOW: ShadowStyle = ShadowStyle {
    ambient: ShadowLayer {
        blur: 48,
        y_offset: 18,
        opacity: 0.18,
    },
    contact: ShadowLayer {
        blur: 12,
        y_offset: 4,
        opacity: 0.08,
    },
    unfocused_scale: 0.5,
};

/// The window shadow in dark and dark-tint modes: Neptunia's deeper,
/// tighter dark shadow.
pub const SHADOW_DARK: ShadowStyle = ShadowStyle {
    ambient: ShadowLayer {
        blur: 56,
        y_offset: 22,
        opacity: 0.55,
    },
    contact: ShadowLayer {
        blur: 16,
        y_offset: 6,
        opacity: 0.35,
    },
    unfocused_scale: 0.5,
};

/// The window outer border: light first, dark second. Painted by the effects
/// painter (ADR 0020), not by the titlebar.
pub const WINDOW_OUTER: [Rgba; 2] = [
    Rgba::rgba8(0x0d, 0x12, 0x19, 0.12),
    Rgba::rgba8(0, 0, 0, 0.90),
];

/// The light mode palette: Neptunia `light`.
pub const LIGHT: Palette = Palette {
    titlebar_bg: FocusPair {
        focused: Rgba::rgb8(0xf2, 0xf4, 0xf7),
        unfocused: Rgba::rgb8(0xe7, 0xeb, 0xf1),
    },
    hairline: Rgba::rgb8(0xd6, 0xdc, 0xe5),
    inner_line: None,
    title: FocusPair {
        focused: Rgba::rgb8(0x0d, 0x12, 0x19),
        unfocused: Rgba::rgb8(0x8a, 0x96, 0xa8),
    },
    text: FocusPair {
        focused: Rgba::rgb8(0x1b, 0x23, 0x30),
        unfocused: Rgba::rgb8(0x55, 0x65, 0x7e),
    },
    accent: Rgba::rgb8(0x2f, 0x6f, 0xe4),
    traffic_lights_inactive: Rgba::rgb8(0xd3, 0xd3, 0xd5),
};

/// The dark mode palette: Neptunia `dark`.
pub const DARK: Palette = Palette {
    titlebar_bg: FocusPair {
        focused: Rgba::rgb8(0x1e, 0x1e, 0x1e),
        unfocused: Rgba::rgb8(0x16, 0x16, 0x16),
    },
    hairline: Rgba::rgb8(0x33, 0x33, 0x33),
    inner_line: Some(Rgba::rgba8(0xff, 0xff, 0xff, 0.14)),
    title: FocusPair {
        focused: Rgba::rgb8(0xf5, 0xf5, 0xf5),
        unfocused: Rgba::rgb8(0x9b, 0x9b, 0x9b),
    },
    text: FocusPair {
        focused: Rgba::rgb8(0xdc, 0xdc, 0xdc),
        unfocused: Rgba::rgb8(0x9b, 0x9b, 0x9b),
    },
    accent: Rgba::rgb8(0x2f, 0x6f, 0xe4),
    traffic_lights_inactive: Rgba::rgb8(0x4e, 0x4e, 0x50),
};

/// The dark-tint mode palette: Neptunia `dark-tint`.
pub const DARK_TINT: Palette = Palette {
    titlebar_bg: FocusPair {
        focused: Rgba::rgb8(0x1d, 0x1e, 0x20),
        unfocused: Rgba::rgb8(0x15, 0x16, 0x17),
    },
    hairline: Rgba::rgb8(0x32, 0x33, 0x36),
    inner_line: Some(Rgba::rgba8(0xff, 0xff, 0xff, 0.14)),
    title: FocusPair {
        focused: Rgba::rgb8(0xf4, 0xf5, 0xf6),
        unfocused: Rgba::rgb8(0x99, 0x9c, 0xa3),
    },
    text: FocusPair {
        focused: Rgba::rgb8(0xda, 0xdc, 0xdf),
        unfocused: Rgba::rgb8(0x99, 0x9c, 0xa3),
    },
    accent: Rgba::rgb8(0x2f, 0x6f, 0xe4),
    traffic_lights_inactive: Rgba::rgb8(0x4e, 0x4e, 0x50),
};

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

/// The light shell palette: Neptunia `light`.
pub const SHELL_LIGHT: ShellPalette = ShellPalette {
    bar: Rgba::rgba8(0xf2, 0xf4, 0xf7, 230.0 / 255.0),
    text: Rgba::rgb8(0x0d, 0x12, 0x19),
    tile: Rgba::rgb8(0xe3, 0xe8, 0xef),
    tile_active: Rgba::rgb8(0x2f, 0x6f, 0xe4),
    tile_text_active: Rgba::rgb8(0xff, 0xff, 0xff),
};

/// The dark shell palette: Neptunia `dark`.
pub const SHELL_DARK: ShellPalette = ShellPalette {
    bar: Rgba::rgba8(0x1e, 0x1e, 0x1e, 230.0 / 255.0),
    text: Rgba::rgb8(0xf5, 0xf5, 0xf5),
    tile: Rgba::rgb8(0x3a, 0x3a, 0x3a),
    tile_active: Rgba::rgb8(0x2f, 0x6f, 0xe4),
    tile_text_active: Rgba::rgb8(0xff, 0xff, 0xff),
};

/// The dark-tint shell palette: Neptunia `dark-tint`.
pub const SHELL_DARK_TINT: ShellPalette = ShellPalette {
    bar: Rgba::rgba8(0x1d, 0x1e, 0x20, 230.0 / 255.0),
    text: Rgba::rgb8(0xf4, 0xf5, 0xf6),
    tile: Rgba::rgb8(0x38, 0x39, 0x3d),
    tile_active: Rgba::rgb8(0x2f, 0x6f, 0xe4),
    tile_text_active: Rgba::rgb8(0xff, 0xff, 0xff),
};

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

/// The lock palette (Neptunia `dark-tint`).
pub const LOCK: LockPalette = LockPalette {
    bg_surface: Rgba::rgb8(0x25, 0x26, 0x28),
    ink: Rgba::rgb8(0xda, 0xdc, 0xdf),
    ink_muted: Rgba::rgb8(0x99, 0x9c, 0xa3),
    danger: Rgba::rgb8(0xff, 0x6b, 0x6b),
    accent: Rgba::rgb8(0x2f, 0x6f, 0xe4),
};

/// The menubar height, in logical pixels.
pub const MENUBAR_HEIGHT: i32 = 28;
/// The menubar font size, in points.
pub const MENUBAR_FONT_SIZE: f32 = 13.0;
/// The menubar edge margin, in logical pixels.
pub const MENUBAR_MARGIN: i32 = 10;
/// The maximum width of the active app title, in logical pixels.
pub const MENUBAR_TITLE_MAX_WIDTH: f32 = 240.0;

/// The default dock tile size, in logical pixels (`dock.domain` may override).
pub const DOCK_TILESIZE: i32 = 48;
/// The gap between dock tiles, in logical pixels.
pub const DOCK_GAP: i32 = 6;
/// The dock padding around the tiles, in logical pixels.
pub const DOCK_PADDING: i32 = 8;
/// The dock bar height, in logical pixels.
pub const DOCK_BAR_HEIGHT: i32 = DOCK_TILESIZE + DOCK_PADDING * 2;
/// The dock label font size, in points.
pub const DOCK_FONT_SIZE: f32 = 11.0;
/// The dock label baseline offset inside a tile, in logical pixels.
pub const DOCK_LABEL_OFFSET: i32 = 8;
/// The dock label's horizontal inset, in logical pixels.
pub const DOCK_TILE_TEXT_INSET: i32 = 2;
/// The dock label's width padding inside a tile, in logical pixels.
pub const DOCK_TILE_TEXT_PADDING: i32 = 4;

/// The lock title font size, in points.
pub const LOCK_FONT_SIZE: f32 = 24.0;
/// The title is this many times the prompt font size.
pub const LOCK_TITLE_SCALE: f32 = 1.5;
/// The title's vertical position is the height divided by this.
pub const LOCK_TITLE_Y_DIVISOR: i32 = 3;
/// The password field width, in logical pixels.
pub const LOCK_FIELD_WIDTH: i32 = 360;
/// The password field height, in logical pixels.
pub const LOCK_FIELD_HEIGHT: i32 = 44;
/// The password field's minimum width, in logical pixels.
pub const LOCK_FIELD_MIN_WIDTH: i32 = 120;
/// The horizontal margin that bounds the field and status, in logical pixels.
pub const LOCK_FIELD_MARGIN: i32 = 80;
/// The focused field line height, in logical pixels.
pub const LOCK_FIELD_LINE: i32 = 2;
/// The label offset above the field, in logical pixels.
pub const LOCK_LABEL_OFFSET: i32 = 10;
/// The masked input offset below the field top, in logical pixels.
pub const LOCK_MASK_OFFSET: i32 = 30;
/// The status offset below the field, in logical pixels.
pub const LOCK_STATUS_OFFSET: i32 = 22;
/// The label font size, in points.
pub const LOCK_LABEL_FONT_SIZE: f32 = 16.0;
/// The masked input font size, in points.
pub const LOCK_MASK_FONT_SIZE: f32 = 22.0;
/// The status font size, in points.
pub const LOCK_STATUS_FONT_SIZE: f32 = 14.0;

/// The line-height multiplier used by the text engines.
pub const TEXT_LINE_HEIGHT: f32 = 1.3;

/// Where the wallpapers Yuki ships are installed, like GNOME's
/// `/usr/share/backgrounds`. The flake installs `share/wallpapers/` under
/// `/run/current-system/sw/share/wallpapers`.
pub const WALLPAPER_DIR: &str = "/run/current-system/sw/share/wallpapers/solid-colors";

/// The wallpaper a session shows when the store selects none: a fresh
/// account's desktop, like macOS ships one default picture. A file name inside
/// [`WALLPAPER_DIR`] (or inside a custom `wallpaper.dir`).
pub const WALLPAPER: &str = "Solid Aqua Blue.png";
