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

/// A font weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// Regular weight.
    Regular,
    /// Bold weight.
    Bold,
}

/// A font face and size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    /// Family name.
    pub family: &'static str,
    /// Size in points.
    pub size: u32,
    /// Weight.
    pub weight: FontWeight,
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

/// The titlebar: Neptunia's 44 px bar at the top, with 12 px traffic lights.
pub const TITLEBAR: Titlebar = Titlebar {
    position: TitlebarPosition::Top,
    thickness: 44,
    buttons: TitlebarButtons {
        order: [
            Button {
                kind: ButtonKind::Close,
                color: Rgba::rgb8(0xe0, 0x62, 0x5d),
            },
            Button {
                kind: ButtonKind::Minimize,
                color: Rgba::rgb8(0xd9, 0xa6, 0x3a),
            },
            Button {
                kind: ButtonKind::Maximize,
                color: Rgba::rgb8(0x4f, 0xae, 0x5f),
            },
        ],
        diameter: 12,
        spacing: 8,
        corner_radius: 4,
        margin_start: 16,
    },
    font: Font {
        family: "Inter",
        size: 13,
        weight: FontWeight::Bold,
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
    family: "Inter",
    size: 10,
    weight: FontWeight::Regular,
};

/// The window shadow, the same in both modes.
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
        unfocused: Rgba::rgb8(0x55, 0x65, 0x7e),
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

/// The menubar/dock surface: Neptunia `bg-window` with a slight transparency.
pub const SHELL_BAR: [u8; 4] = [0xf2, 0xf4, 0xf7, 0xe6];

/// The menubar/dock text: Neptunia `ink-strong`.
pub const SHELL_TEXT: [u8; 4] = [0x0d, 0x12, 0x19, 0xff];

/// A dock tile: Neptunia `bg-control`.
pub const SHELL_TILE: [u8; 4] = [0xe3, 0xe8, 0xef, 0xff];

/// An active dock tile: Neptunia `accent`.
pub const SHELL_TILE_ACTIVE: [u8; 4] = [0x2f, 0x6f, 0xe4, 0xff];

/// Text on an active dock tile: Neptunia `on-accent`.
pub const SHELL_TILE_TEXT_ACTIVE: [u8; 4] = [0xff, 0xff, 0xff, 0xff];

/// Where the wallpapers Yuki ships are installed, like GNOME's
/// `/usr/share/backgrounds`. The flake installs `share/wallpapers/` under
/// `/run/current-system/sw/share/wallpapers`.
pub const WALLPAPER_DIR: &str = "/run/current-system/sw/share/wallpapers/solid-colors";

/// The wallpaper a session shows when the store selects none: a fresh
/// account's desktop, like macOS ships one default picture. A file name inside
/// [`WALLPAPER_DIR`] (or inside a custom `wallpaper.dir`).
pub const WALLPAPER: &str = "Solid Aqua Blue.png";
