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
//! # Where the accent came from
//!
//! The accent is extracted from the host's somewm configuration, which is the
//! desktop Yuki is being built on:
//!
//! - `~/.config/somewm/settings.json` selects it: `theme.accent = "blue"`.
//! - `~/.config/somewm/themes/accents.lua` line 13 defines it, in the original
//!   format of two lowercase `#rrggbb` hex strings:
//!   `light = "#007aff"`, `dark = "#0a84ff"`.
//!
//! That file provides a light and a dark variant, so [`LIGHT`] and [`DARK`]
//! each carry the accent that belongs to them. This is the only value in the
//! module taken from the host rather than decided here.

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
    /// The color of every button when the window does not have focus.
    pub unfocused: Rgba,
    /// Button diameter, in logical pixels.
    pub diameter: u32,
    /// Gap between adjacent buttons, in logical pixels.
    pub spacing: u32,
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
}

/// The titlebar: a 32 logical pixel bar on the left, with macOS-style buttons.
pub const TITLEBAR: Titlebar = Titlebar {
    position: TitlebarPosition::Left,
    thickness: 32,
    buttons: TitlebarButtons {
        order: [
            Button {
                kind: ButtonKind::Close,
                color: Rgba::rgb8(255, 95, 87),
            },
            Button {
                kind: ButtonKind::Minimize,
                color: Rgba::rgb8(254, 188, 46),
            },
            Button {
                kind: ButtonKind::Maximize,
                color: Rgba::rgb8(40, 200, 64),
            },
        ],
        unfocused: Rgba::rgb8(142, 142, 147),
        diameter: 12,
        spacing: 8,
        margin_start: 16,
    },
    font: Font {
        family: "Inter",
        size: 10,
        weight: FontWeight::Bold,
    },
    rotation: TitleRotation::BottomToTop,
};

/// The window shape: a 12 logical pixel rounded superellipse.
pub const WINDOW: WindowStyle = WindowStyle {
    corner_radius: 12,
    superellipse_exponent: 5.0,
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

/// The body text face.
pub const TEXT: Font = Font {
    family: "Inter",
    size: 10,
    weight: FontWeight::Regular,
};

/// The window shadow, the same in both modes.
pub const SHADOW: ShadowStyle = ShadowStyle {
    ambient: ShadowLayer {
        blur: 40,
        y_offset: 12,
        opacity: 0.18,
    },
    contact: ShadowLayer {
        blur: 6,
        y_offset: 2,
        opacity: 0.28,
    },
    unfocused_scale: 0.5,
};

/// The light mode palette. Its inner line is absent.
pub const LIGHT: Palette = Palette {
    titlebar_bg: FocusPair {
        focused: Rgba::rgb8(236, 236, 236),
        unfocused: Rgba::rgb8(246, 246, 246),
    },
    hairline: Rgba::rgba8(0, 0, 0, 0.25),
    inner_line: None,
    title: FocusPair {
        focused: Rgba::rgba8(0, 0, 0, 0.80),
        unfocused: Rgba::rgba8(0, 0, 0, 0.45),
    },
    text: FocusPair {
        focused: Rgba::rgba8(0, 0, 0, 0.80),
        unfocused: Rgba::rgba8(0, 0, 0, 0.45),
    },
    accent: Rgba::rgb8(0x00, 0x7a, 0xff),
};

/// The dark mode palette. Its inner line is present.
pub const DARK: Palette = Palette {
    titlebar_bg: FocusPair {
        focused: Rgba::rgb8(42, 42, 44),
        unfocused: Rgba::rgb8(31, 31, 33),
    },
    hairline: Rgba::rgba8(0, 0, 0, 0.60),
    inner_line: Some(Rgba::rgba8(255, 255, 255, 0.12)),
    title: FocusPair {
        focused: Rgba::rgba8(255, 255, 255, 0.80),
        unfocused: Rgba::rgba8(255, 255, 255, 0.45),
    },
    text: FocusPair {
        focused: Rgba::rgba8(255, 255, 255, 0.80),
        unfocused: Rgba::rgba8(255, 255, 255, 0.45),
    },
    accent: Rgba::rgb8(0x0a, 0x84, 0xff),
};

/// Where the wallpapers Yuki ships are installed, like GNOME's
/// `/usr/share/backgrounds`. The flake installs `share/wallpapers/` under
/// `/run/current-system/sw/share/wallpapers`.
pub const WALLPAPER_DIR: &str = "/run/current-system/sw/share/wallpapers/solid-colors";

/// The wallpaper a session shows when the store selects none: a fresh
/// account's desktop, like macOS ships one default picture. A file name inside
/// [`WALLPAPER_DIR`] (or inside a custom `wallpaper.dir`).
pub const WALLPAPER: &str = "Solid Aqua Blue.png";
