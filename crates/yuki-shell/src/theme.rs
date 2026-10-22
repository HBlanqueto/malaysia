//! The shell palette: Neptunia light/dark/dark-tint, with the optional
//! desktop wallpaper tint.
//!
//! The shell writes `wallpaper.state` (it decodes the wallpaper) and reads
//! `appearance.domain`; the compositor only reads both. `window_tint_scope =
//! "desktop"` mixes the bar and tiles with the wallpaper's dominant colour.

use std::time::SystemTime;

use yuki_appearance::{
    AppearanceConfig, ColorScheme, appearance_path, load_appearance, load_dominant, resolve,
    wallpaper_state_path,
};
use yuki_design::{SHELL_DARK, SHELL_DARK_TINT, SHELL_LIGHT, ShellPalette};

pub use yuki_appearance::data_dir;

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
        let sources = (
            modified(&appearance_path()),
            modified(&wallpaper_state_path()),
        );
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
        let appearance = load_appearance();
        let dominant = load_dominant();
        self.palette = palette_for(&appearance, dominant);
    }
}

/// Chooses the Neptunia palette and applies the desktop tint.
fn palette_for(appearance: &AppearanceConfig, dominant: Option<yuki_design::Rgba>) -> ShellPalette {
    let resolved = resolve(appearance, dominant);
    let base = match resolved.scheme {
        ColorScheme::Dark => SHELL_DARK,
        ColorScheme::DarkTint => SHELL_DARK_TINT,
        ColorScheme::Auto | ColorScheme::Light => SHELL_LIGHT,
    };
    let mut palette = base;
    if let Some((color, factor)) = resolved.panel_tint {
        palette.bar = base.bar.mix(color, factor);
        palette.tile = base.tile.mix(color, factor);
    }
    palette.tile_active = resolved.accent;
    palette
}

fn modified(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use yuki_appearance::{TrafficLightStyle, WindowColorMode, WindowTintScope};
    use yuki_design::{PANEL_TINT_WALLPAPER, Rgba};

    fn appearance(mode: WindowColorMode, scope: WindowTintScope) -> AppearanceConfig {
        AppearanceConfig {
            window_color: mode,
            window_tint_scope: scope,
            traffic_lights: TrafficLightStyle::Auto,
            ..AppearanceConfig::default()
        }
    }

    fn dominant() -> Rgba {
        Rgba {
            r: 0.8,
            g: 0.2,
            b: 0.1,
            a: 1.0,
        }
    }

    #[test]
    fn auto_uses_the_light_shell_palette() {
        let palette = palette_for(&AppearanceConfig::default(), None);
        assert_eq!(palette, SHELL_LIGHT);
    }

    #[test]
    fn neutral_desktop_does_not_tint() {
        let palette = palette_for(
            &appearance(WindowColorMode::Neutral, WindowTintScope::Desktop),
            Some(dominant()),
        );
        assert_eq!(palette, SHELL_LIGHT);
    }

    #[test]
    fn desktop_scope_mixes_bar_and_tile_but_not_the_active_tile() {
        let palette = palette_for(
            &appearance(WindowColorMode::Wallpaper, WindowTintScope::Desktop),
            Some(dominant()),
        );
        assert_eq!(
            palette.bar,
            SHELL_LIGHT.bar.mix(dominant(), PANEL_TINT_WALLPAPER[0])
        );
        assert_eq!(
            palette.tile,
            SHELL_LIGHT.tile.mix(dominant(), PANEL_TINT_WALLPAPER[0])
        );
        assert_eq!(palette.tile_active, dominant());
        assert_ne!(
            palette.tile_active,
            SHELL_LIGHT
                .tile_active
                .mix(dominant(), PANEL_TINT_WALLPAPER[0])
        );
        assert_eq!(palette.text, SHELL_LIGHT.text);
    }

    #[test]
    fn windows_scope_leaves_the_bar_and_tile_alone() {
        let palette = palette_for(
            &appearance(WindowColorMode::Wallpaper, WindowTintScope::Windows),
            Some(dominant()),
        );
        assert_eq!(palette.bar, SHELL_LIGHT.bar);
        assert_eq!(palette.tile, SHELL_LIGHT.tile);
        assert_eq!(palette.tile_active, dominant());
    }
}
