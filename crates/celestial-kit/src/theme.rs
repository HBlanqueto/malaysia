//! The Freya theme built from `yuki-design` and `yuki-appearance` (ADR 0034).
//!
//! No colour or measurement is written here: the sheet is a projection of the
//! resolved Neptunia palette, so the compositor, the shell and the apps keep
//! sharing one source.

use freya_components::theming::component_themes::{ColorsSheet, Theme};
use freya_components::theming::themes::{dark_theme, light_theme};
use freya_core::prelude::Color;
use yuki_appearance::{ColorScheme, Resolved, load_appearance, load_dominant, resolve};
use yuki_design::{
    LockPalette, MENU_DARK, MENU_DARK_TINT, MENU_LIGHT, MenuPalette, Rgba, SHELL_DARK,
    SHELL_DARK_TINT, SHELL_LIGHT, ShellPalette,
};

fn color(c: Rgba) -> Color {
    let [r, g, b, a] = c.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn themed(name: &'static str, dark: bool, colors: ColorsSheet) -> Theme {
    let mut theme = if dark { dark_theme() } else { light_theme() };
    theme.name = name;
    theme.colors = colors;
    theme
}

fn sheet(background: Rgba, surface: Rgba, text: Rgba, accent: Rgba, border: Rgba) -> ColorsSheet {
    ColorsSheet {
        primary: color(accent),
        secondary: color(accent),
        tertiary: color(accent),
        success: color(accent),
        warning: color(accent),
        error: color(accent),
        info: color(accent),
        background: color(background),
        surface_primary: color(surface),
        surface_secondary: color(surface),
        surface_tertiary: color(background),
        surface_inverse: color(text),
        surface_inverse_secondary: color(text),
        surface_inverse_tertiary: color(text),
        border: color(border),
        border_focus: color(accent),
        border_disabled: color(border),
        text_primary: color(text),
        text_secondary: color(text),
        text_placeholder: color(text),
        text_inverse: color(background),
        text_highlight: color(accent),
        focus: color(accent),
        active: color(accent),
        disabled: color(border),
        overlay: color(background),
        shadow: color(Rgba { a: 0.25, ..text }),
    }
}

/// Resolves the current appearance and its shell palette.
pub fn resolved() -> Resolved {
    resolve(&load_appearance(), load_dominant())
}

/// The shell palette for the current appearance (respects the desktop tint).
pub fn shell_palette(resolved: &Resolved) -> ShellPalette {
    let base = match resolved.scheme {
        ColorScheme::Dark => SHELL_DARK,
        ColorScheme::DarkTint => SHELL_DARK_TINT,
        ColorScheme::Auto | ColorScheme::Light => SHELL_LIGHT,
    };
    let mut palette = base;
    if let Some((tint, factor)) = resolved.panel_tint {
        palette.bar = base.bar.mix(tint, factor);
        palette.tile = base.tile.mix(tint, factor);
    }
    palette.tile_active = resolved.accent;
    palette
}

/// The shell theme for an explicit scheme (tests and previews).
pub fn for_scheme(dark: bool) -> Theme {
    let appearance = yuki_appearance::AppearanceConfig {
        color_scheme: if dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        },
        ..Default::default()
    };
    let resolved = resolve(&appearance, None);
    let palette = shell_palette(&resolved);
    let text = palette.text;
    let border = if dark { palette.tile } else { palette.bar };
    themed(
        if dark {
            "neptunia-shell-dark"
        } else {
            "neptunia-shell-light"
        },
        dark,
        sheet(palette.bar, palette.tile, text, palette.tile_active, border),
    )
}

/// The menu palette for the current appearance.
pub fn menu_palette(resolved: &Resolved) -> MenuPalette {
    match resolved.scheme {
        ColorScheme::Dark => MENU_DARK,
        ColorScheme::DarkTint => MENU_DARK_TINT,
        ColorScheme::Auto | ColorScheme::Light => MENU_LIGHT,
    }
}

/// The theme for menubar/dock-style surfaces.
pub fn shell() -> Theme {
    let resolved = resolved();
    let palette = shell_palette(&resolved);
    let dark = matches!(resolved.scheme, ColorScheme::Dark | ColorScheme::DarkTint);
    let text = palette.text;
    let border = if dark { palette.tile } else { palette.bar };
    themed(
        "neptunia-shell",
        dark,
        sheet(palette.bar, palette.tile, text, palette.tile_active, border),
    )
}

/// The theme for the lock screen.
pub fn lock() -> Theme {
    let resolved = resolved();
    let palette = lock_palette(&resolved);
    themed(
        "neptunia-lock",
        true,
        sheet(
            palette.bg_surface,
            palette.bg_surface,
            palette.ink,
            palette.accent,
            palette.ink_muted,
        ),
    )
}

pub fn lock_palette(resolved: &Resolved) -> LockPalette {
    let mut palette = yuki_design::LOCK;
    palette.accent = resolved.accent;
    palette
}
