//! The shell palette: Neptunia light/dark/dark-tint, with the optional
//! desktop wallpaper tint.
//!
//! The compositor owns `appearance.domain` and writes `wallpaper.state`; the
//! shell only reads them, like the other state stores. `window_tint_scope =
//! "desktop"` mixes the bar and dock surfaces with the wallpaper's dominant
//! colour.

use std::path::PathBuf;
use std::time::SystemTime;

use serde::Deserialize;
use yuki_design::{
    PANEL_TINT_WALLPAPER, Rgba, SHELL_DARK, SHELL_DARK_TINT, SHELL_LIGHT, ShellPalette,
};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Appearance {
    color_scheme: ColorScheme,
    window_tint_scope: WindowTintScope,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ColorScheme {
    #[default]
    Auto,
    Dark,
    #[serde(rename = "dark_tint")]
    DarkTint,
    Light,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WindowTintScope {
    #[default]
    Windows,
    Desktop,
}

#[derive(Debug, Deserialize)]
struct WallpaperState {
    dominant: Option<[f32; 3]>,
}

/// The current shell palette and the files it was resolved from.
pub struct ShellTheme {
    palette: ShellPalette,
    sources: Option<(Option<SystemTime>, Option<SystemTime>)>,
}

impl ShellTheme {
    /// Loads the palette from `appearance.domain` and `wallpaper.state`.
    pub fn new() -> Self {
        let mut theme = Self {
            palette: SHELL_LIGHT,
            sources: None,
        };
        theme.reload();
        theme
    }

    /// Re-resolves the palette when either source file changed.
    pub fn refresh(&mut self) -> bool {
        let sources = (modified(&appearance_path()), modified(&wallpaper_path()));
        if self.sources == Some(sources) {
            return false;
        }
        self.sources = Some(sources);
        self.reload();
        true
    }

    /// The resolved palette.
    pub fn palette(&self) -> ShellPalette {
        self.palette
    }

    fn reload(&mut self) {
        let appearance = read_appearance();
        let dominant = if appearance.window_tint_scope == WindowTintScope::Desktop {
            read_dominant()
        } else {
            None
        };
        self.palette = palette_for(&appearance, dominant);
    }
}

/// Chooses the Neptunia palette and applies the desktop tint.
fn palette_for(appearance: &Appearance, dominant: Option<Rgba>) -> ShellPalette {
    let dark = matches!(
        appearance.color_scheme,
        ColorScheme::Dark | ColorScheme::DarkTint
    );
    let base = match appearance.color_scheme {
        ColorScheme::Dark => SHELL_DARK,
        ColorScheme::DarkTint => SHELL_DARK_TINT,
        ColorScheme::Auto | ColorScheme::Light => SHELL_LIGHT,
    };
    if appearance.window_tint_scope != WindowTintScope::Desktop {
        return base;
    }
    let Some(dominant) = dominant else {
        return base;
    };
    let factor = PANEL_TINT_WALLPAPER[usize::from(dark)];
    ShellPalette {
        bar: base.bar.mix(dominant, factor),
        text: base.text,
        tile: base.tile.mix(dominant, factor),
        tile_active: base.tile_active.mix(dominant, factor),
        tile_text_active: base.tile_text_active,
    }
}

fn read_appearance() -> Appearance {
    let Ok(text) = std::fs::read_to_string(appearance_path()) else {
        return Appearance::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

fn read_dominant() -> Option<Rgba> {
    let text = std::fs::read_to_string(wallpaper_path()).ok()?;
    let state: WallpaperState = toml::from_str(&text).ok()?;
    let [r, g, b] = state.dominant?;
    Some(Rgba {
        r: r.clamp(0.0, 1.0),
        g: g.clamp(0.0, 1.0),
        b: b.clamp(0.0, 1.0),
        a: 1.0,
    })
}

fn appearance_path() -> PathBuf {
    config_dir().join("appearance.domain")
}

fn wallpaper_path() -> PathBuf {
    data_dir().join("wallpaper.state")
}

/// The preferences directory: `YUKI_CONFIG_DIR`, else `$XDG_CONFIG_HOME/yuki`.
pub fn config_dir() -> PathBuf {
    std::env::var_os("YUKI_CONFIG_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|home| home.join(".config"))
                })
                .unwrap_or_else(|| PathBuf::from(".config"));
            base.join("yuki")
        },
        PathBuf::from,
    )
}

/// The state/data directory: `YUKI_DATA_DIR`, else `$XDG_DATA_HOME/yuki`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("YUKI_DATA_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|home| home.join(".local/share"))
                })
                .unwrap_or_else(|| PathBuf::from(".local/share"));
            base.join("yuki")
        },
        PathBuf::from,
    )
}

fn modified(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appearance(scheme: ColorScheme, scope: WindowTintScope) -> Appearance {
        Appearance {
            color_scheme: scheme,
            window_tint_scope: scope,
        }
    }

    #[test]
    fn auto_uses_the_light_shell_palette() {
        let palette = palette_for(
            &appearance(ColorScheme::Auto, WindowTintScope::Windows),
            None,
        );
        assert_eq!(palette, SHELL_LIGHT);
    }

    #[test]
    fn each_scheme_picks_its_palette() {
        assert_eq!(
            palette_for(
                &appearance(ColorScheme::Dark, WindowTintScope::Windows),
                None
            ),
            SHELL_DARK
        );
        assert_eq!(
            palette_for(
                &appearance(ColorScheme::DarkTint, WindowTintScope::Windows),
                None
            ),
            SHELL_DARK_TINT
        );
        assert_eq!(
            palette_for(
                &appearance(ColorScheme::Light, WindowTintScope::Windows),
                None
            ),
            SHELL_LIGHT
        );
    }

    #[test]
    fn desktop_scope_mixes_the_bar_and_tiles_with_the_dominant() {
        let dominant = Rgba {
            r: 0.8,
            g: 0.2,
            b: 0.1,
            a: 1.0,
        };
        let light = palette_for(
            &appearance(ColorScheme::Light, WindowTintScope::Desktop),
            Some(dominant),
        );
        assert_eq!(
            light.bar,
            SHELL_LIGHT.bar.mix(dominant, PANEL_TINT_WALLPAPER[0])
        );
        assert_eq!(
            light.tile,
            SHELL_LIGHT.tile.mix(dominant, PANEL_TINT_WALLPAPER[0])
        );
        assert_eq!(light.text, SHELL_LIGHT.text);

        let dark = palette_for(
            &appearance(ColorScheme::Dark, WindowTintScope::Desktop),
            Some(dominant),
        );
        assert_eq!(
            dark.bar,
            SHELL_DARK.bar.mix(dominant, PANEL_TINT_WALLPAPER[1])
        );
    }

    #[test]
    fn windows_scope_ignores_the_dominant() {
        let dominant = Rgba {
            r: 0.8,
            g: 0.2,
            b: 0.1,
            a: 1.0,
        };
        assert_eq!(
            palette_for(
                &appearance(ColorScheme::Light, WindowTintScope::Windows),
                Some(dominant)
            ),
            SHELL_LIGHT
        );
    }
}
