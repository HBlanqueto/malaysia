//! The Freya theme built from `vow-design` and `vow-appearance` (ADR 0034).
//!
//! No colour or measurement is written here: the sheet is a projection of the
//! resolved Neptunia palette, so the compositor, the shell and the apps keep
//! sharing one source.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use freya_components::theming::component_themes::{ColorsSheet, Theme};
use freya_components::theming::themes::{dark_theme, light_theme};
use freya_core::prelude::Color;
use vow_appearance::{
    ColorScheme, Resolved, config_dir, data_dir, load_appearance, load_dominant, resolve,
};
use vow_design::{LockPalette, MenuPalette, Rgba, ShellPalette};

static RESOLVED: OnceLock<Mutex<HashMap<(PathBuf, PathBuf), Resolved>>> = OnceLock::new();

fn load_resolved() -> Resolved {
    resolve(&load_appearance(), load_dominant())
}

fn cache_key() -> (PathBuf, PathBuf) {
    (config_dir(), data_dir())
}

fn cache() -> &'static Mutex<HashMap<(PathBuf, PathBuf), Resolved>> {
    RESOLVED.get_or_init(|| Mutex::new(HashMap::new()))
}

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
///
/// The first call for a given config/data directory pair loads and caches the
/// value; later calls return the cached value until [`refresh`] updates it.
pub fn resolved() -> Resolved {
    let key = cache_key();
    let mut cache = cache().lock().expect("theme cache is not poisoned");
    cache.get(&key).copied().unwrap_or_else(|| {
        let resolved = load_resolved();
        cache.insert(key, resolved);
        resolved
    })
}

/// Reloads the appearance from disk and updates the cache. Returns `true` when
/// the resolved values changed.
pub fn refresh() -> bool {
    let key = cache_key();
    let resolved = load_resolved();
    let mut cache = cache().lock().expect("theme cache is not poisoned");
    let changed = cache.get(&key) != Some(&resolved);
    if changed {
        cache.insert(key, resolved);
    }
    changed
}

/// The shell palette for the current appearance (respects the desktop tint).
///
/// `Resolved` already picked the opaque or translucent surfaces from
/// `reduce_transparency`.
// TODO(ADR 0024): with reduce_transparency = false the compositor should blur
// the wallpaper behind these surfaces; until then the translucent token simply
// blends with whatever is behind it.
pub fn shell_palette(resolved: &Resolved) -> ShellPalette {
    resolved.shell
}

/// The shell theme for an explicit scheme (tests and previews).
pub fn for_scheme(dark: bool) -> Theme {
    let appearance = vow_appearance::AppearanceConfig {
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
    resolved.menu
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
    let mut palette = vow_design::LOCK;
    palette.accent = resolved.accent;
    palette
}
