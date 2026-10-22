//! The appearance domain and its resolved colours.
//!
//! This crate is deliberately free of Smithay and of the compositor: the
//! compositor, the shell and the portal all read `appearance.domain` and
//! `wallpaper.state` through the same rules. It only depends on serde, toml
//! and the design tokens.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use yuki_design::{
    DARK, DARK_TINT, LIGHT, PANEL_TINT_WALLPAPER, Palette, Rgba, TITLEBAR,
    TRAFFIC_CLOSE_MINIMIZE_DARKEN, TRAFFIC_ZOOM_LIGHTEN, WINDOW_TINT_ACCENT, WINDOW_TINT_WALLPAPER,
};

/// The preferred appearance, published as `org.freedesktop.appearance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    /// Follow the user's preference ("Auto").
    #[default]
    Auto,
    /// Prefer the dark appearance ("Dark").
    Dark,
    /// Prefer the dark-tinted appearance ("Dark tint").
    #[serde(rename = "dark_tint")]
    DarkTint,
    /// Prefer the light appearance ("Light").
    Light,
}

impl ColorScheme {
    /// Resolves `Auto` to the concrete appearance. There is no separate system
    /// preference source yet, so it follows the light default; the portal
    /// publishes this same value.
    pub fn resolved(self) -> Self {
        match self {
            Self::Auto => Self::Light,
            scheme => scheme,
        }
    }
}

/// Neptunia's "Color de las ventanas" (`neutral` / `accent` / `wallpaper`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowColorMode {
    /// No tint: the light/dark palette as-is.
    #[default]
    Neutral,
    /// Tint the window chrome toward the accent colour.
    Accent,
    /// Tint the window chrome toward the wallpaper's dominant colour.
    Wallpaper,
}

/// How far the wallpaper tint reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowTintScope {
    /// Only the windows.
    #[default]
    Windows,
    /// Windows plus the panels and dock.
    Desktop,
}

/// Neptunia's traffic-light styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficLightStyle {
    /// Same as the window colour mode: macOS colours for neutral, accent or
    /// wallpaper-derived colours otherwise.
    #[default]
    Auto,
    /// The fixed macOS red/amber/green.
    Macos,
    /// Derived from the accent colour.
    Accent,
    /// Derived from the wallpaper's dominant colour.
    Wallpaper,
}

/// Appearance options (macOS System Settings → Appearance).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AppearanceConfig {
    /// "Light", "Dark", "Dark tint" or "Auto".
    pub color_scheme: ColorScheme,
    /// An sRGB accent color, `[r, g, b]` in `[0, 1]`. `None` → the default.
    pub accent_color: Option<[f32; 3]>,
    /// "Increase contrast" (macOS Accessibility).
    pub contrast: bool,
    /// "Reduce motion" (macOS Accessibility).
    pub reduced_motion: bool,
    /// What tints the window chrome: nothing, the accent or the wallpaper.
    pub window_color: WindowColorMode,
    /// In wallpaper mode, whether only windows or the panels and dock too are
    /// tinted.
    pub window_tint_scope: WindowTintScope,
    /// The traffic-light palette.
    pub traffic_lights: TrafficLightStyle,
}

/// The colours and tints resolved from one appearance + wallpaper state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolved {
    /// The concrete scheme (`Auto` already resolved).
    pub scheme: ColorScheme,
    /// The accent after wallpaper mode: the dominant, or the normal accent
    /// when there is no dominant.
    pub accent: Rgba,
    /// The titlebar tint, if any, and its Neptunia factor.
    pub window_tint: Option<(Rgba, f32)>,
    /// The menubar/dock tint, only in wallpaper + desktop with a dominant.
    pub panel_tint: Option<(Rgba, f32)>,
    /// The three traffic lights.
    pub lights: [Rgba; 3],
}

/// The Neptunia palette for a concrete scheme.
pub fn palette(scheme: ColorScheme) -> Palette {
    match scheme.resolved() {
        ColorScheme::DarkTint => DARK_TINT,
        ColorScheme::Dark => DARK,
        _ => LIGHT,
    }
}

/// Resolves an appearance configuration with the current wallpaper dominant.
pub fn resolve(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> Resolved {
    let scheme = appearance.color_scheme.resolved();
    let base = palette(scheme);
    let dark = matches!(scheme, ColorScheme::Dark | ColorScheme::DarkTint);
    let index = usize::from(dark);

    let normal_accent = appearance
        .accent_color
        .map(|[r, g, b]| Rgba {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: 1.0,
        })
        .unwrap_or(base.accent);

    let accent = if appearance.window_color == WindowColorMode::Wallpaper {
        dominant.unwrap_or(normal_accent)
    } else {
        normal_accent
    };

    let window_tint = match appearance.window_color {
        WindowColorMode::Neutral => None,
        WindowColorMode::Accent => Some((normal_accent, WINDOW_TINT_ACCENT[index])),
        WindowColorMode::Wallpaper => Some((accent, WINDOW_TINT_WALLPAPER[index])),
    };

    let panel_tint = if appearance.window_color == WindowColorMode::Wallpaper
        && appearance.window_tint_scope == WindowTintScope::Desktop
    {
        dominant.map(|color| (color, PANEL_TINT_WALLPAPER[index]))
    } else {
        None
    };

    let style = match appearance.traffic_lights {
        TrafficLightStyle::Auto => match appearance.window_color {
            WindowColorMode::Neutral => TrafficLightStyle::Macos,
            WindowColorMode::Accent => TrafficLightStyle::Accent,
            WindowColorMode::Wallpaper => TrafficLightStyle::Wallpaper,
        },
        style => style,
    };
    let lights = match style {
        TrafficLightStyle::Auto | TrafficLightStyle::Macos => [
            TITLEBAR.buttons.order[0].color,
            TITLEBAR.buttons.order[1].color,
            TITLEBAR.buttons.order[2].color,
        ],
        TrafficLightStyle::Accent => derived_lights(normal_accent),
        TrafficLightStyle::Wallpaper => derived_lights(dominant.unwrap_or(normal_accent)),
    };

    Resolved {
        scheme,
        accent,
        window_tint,
        panel_tint,
        lights,
    }
}

fn derived_lights(source: Rgba) -> [Rgba; 3] {
    [
        source.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN),
        source.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN),
        source.lighten(TRAFFIC_ZOOM_LIGHTEN),
    ]
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

/// `~/.config/yuki/appearance.domain`.
pub fn appearance_path() -> PathBuf {
    config_dir().join("appearance.domain")
}

/// `~/.local/share/yuki/wallpaper.state`.
pub fn wallpaper_state_path() -> PathBuf {
    data_dir().join("wallpaper.state")
}

/// Reads `appearance.domain`; `Ok(None)` when it does not exist.
pub fn read_appearance() -> Result<Option<AppearanceConfig>, toml::de::Error> {
    read_appearance_from(&appearance_path())
}

/// Reads an appearance file at `path`.
pub fn read_appearance_from(path: &Path) -> Result<Option<AppearanceConfig>, toml::de::Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map(Some),
        Err(_) => Ok(None),
    }
}

/// Reads `appearance.domain`, falling back to the defaults.
pub fn load_appearance() -> AppearanceConfig {
    read_appearance().ok().flatten().unwrap_or_default()
}

#[derive(Debug, Deserialize)]
struct WallpaperState {
    dominant: Option<[f32; 3]>,
}

/// Reads the dominant colour from `wallpaper.state`, if present.
pub fn load_dominant() -> Option<Rgba> {
    read_dominant()
}

/// Reads the dominant colour from `wallpaper.state`, if present.
pub fn read_dominant() -> Option<Rgba> {
    read_dominant_from(&wallpaper_state_path())
}

/// Reads a wallpaper state file at `path`.
pub fn read_dominant_from(path: &Path) -> Option<Rgba> {
    let text = std::fs::read_to_string(path).ok()?;
    let state: WallpaperState = toml::from_str(&text).ok()?;
    let [r, g, b] = state.dominant?;
    Some(Rgba {
        r: r.clamp(0.0, 1.0),
        g: g.clamp(0.0, 1.0),
        b: b.clamp(0.0, 1.0),
        a: 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use yuki_design::{
        PANEL_TINT_WALLPAPER, TRAFFIC_CLOSE_MINIMIZE_DARKEN, TRAFFIC_ZOOM_LIGHTEN,
        WINDOW_TINT_ACCENT, WINDOW_TINT_WALLPAPER,
    };

    fn appearance(
        scheme: ColorScheme,
        mode: WindowColorMode,
        scope: WindowTintScope,
        lights: TrafficLightStyle,
        accent: Option<[f32; 3]>,
    ) -> AppearanceConfig {
        AppearanceConfig {
            color_scheme: scheme,
            accent_color: accent,
            contrast: false,
            reduced_motion: false,
            window_color: mode,
            window_tint_scope: scope,
            traffic_lights: lights,
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
    fn auto_resolves_to_light() {
        let resolved = resolve(
            &appearance(
                ColorScheme::Auto,
                WindowColorMode::Neutral,
                WindowTintScope::Windows,
                TrafficLightStyle::Auto,
                None,
            ),
            None,
        );
        assert_eq!(resolved.scheme, ColorScheme::Light);
    }

    #[test]
    fn neutral_desktop_does_not_tint() {
        let resolved = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Neutral,
                WindowTintScope::Desktop,
                TrafficLightStyle::Auto,
                None,
            ),
            Some(dominant()),
        );
        assert_eq!(resolved.window_tint, None);
        assert_eq!(resolved.panel_tint, None);
        assert_eq!(resolved.accent, LIGHT.accent);
    }

    #[test]
    fn neutral_windows_does_not_tint() {
        let resolved = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Neutral,
                WindowTintScope::Windows,
                TrafficLightStyle::Auto,
                None,
            ),
            Some(dominant()),
        );
        assert_eq!(resolved.window_tint, None);
        assert_eq!(resolved.panel_tint, None);
        assert_eq!(resolved.accent, LIGHT.accent);
    }

    #[test]
    fn accent_windows_tints_only_the_window_chrome() {
        let resolved = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Accent,
                WindowTintScope::Windows,
                TrafficLightStyle::Auto,
                Some([0.9, 0.3, 0.1]),
            ),
            Some(dominant()),
        );
        assert!(resolved.window_tint.is_some());
        assert_eq!(resolved.panel_tint, None);
    }

    #[test]
    fn accent_tints_windows_but_not_panels() {
        let accent = [0.9, 0.3, 0.1];
        let resolved = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Accent,
                WindowTintScope::Desktop,
                TrafficLightStyle::Auto,
                Some(accent),
            ),
            Some(dominant()),
        );
        let expected = Rgba {
            r: accent[0],
            g: accent[1],
            b: accent[2],
            a: 1.0,
        };
        assert_eq!(resolved.accent, expected);
        assert_eq!(
            resolved.window_tint,
            Some((expected, WINDOW_TINT_ACCENT[0]))
        );
        assert_eq!(resolved.panel_tint, None);
    }

    #[test]
    fn wallpaper_desktop_tints_panels_and_uses_the_dominant_accent() {
        let resolved = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Wallpaper,
                WindowTintScope::Desktop,
                TrafficLightStyle::Auto,
                Some([0.9, 0.3, 0.1]),
            ),
            Some(dominant()),
        );
        assert_eq!(resolved.accent, dominant());
        assert_eq!(
            resolved.window_tint,
            Some((dominant(), WINDOW_TINT_WALLPAPER[0]))
        );
        assert_eq!(
            resolved.panel_tint,
            Some((dominant(), PANEL_TINT_WALLPAPER[0]))
        );
    }

    #[test]
    fn wallpaper_windows_never_tints_panels() {
        let resolved = resolve(
            &appearance(
                ColorScheme::Dark,
                WindowColorMode::Wallpaper,
                WindowTintScope::Windows,
                TrafficLightStyle::Auto,
                None,
            ),
            Some(dominant()),
        );
        assert_eq!(resolved.panel_tint, None);
        assert_eq!(resolved.accent, dominant());
        assert_eq!(
            resolved.window_tint,
            Some((dominant(), WINDOW_TINT_WALLPAPER[1]))
        );
    }

    #[test]
    fn wallpaper_without_dominant_falls_back_to_the_normal_accent() {
        let resolved = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Wallpaper,
                WindowTintScope::Desktop,
                TrafficLightStyle::Auto,
                Some([0.9, 0.3, 0.1]),
            ),
            None,
        );
        let normal = Rgba {
            r: 0.9,
            g: 0.3,
            b: 0.1,
            a: 1.0,
        };
        assert_eq!(resolved.accent, normal);
        assert_eq!(resolved.panel_tint, None);
        assert_eq!(
            resolved.window_tint,
            Some((normal, WINDOW_TINT_WALLPAPER[0]))
        );
    }

    #[test]
    fn dark_and_dark_tint_use_the_dark_factors() {
        for scheme in [ColorScheme::Dark, ColorScheme::DarkTint] {
            let resolved = resolve(
                &appearance(
                    scheme,
                    WindowColorMode::Wallpaper,
                    WindowTintScope::Desktop,
                    TrafficLightStyle::Auto,
                    None,
                ),
                Some(dominant()),
            );
            assert_eq!(
                resolved.window_tint,
                Some((dominant(), WINDOW_TINT_WALLPAPER[1]))
            );
            assert_eq!(
                resolved.panel_tint,
                Some((dominant(), PANEL_TINT_WALLPAPER[1]))
            );
        }
    }

    #[test]
    fn wallpaper_traffic_lights_follow_the_dominant_even_in_accent_mode() {
        let accent = [0.9, 0.3, 0.1];
        let with_dominant = resolve(&appearance(
            ColorScheme::Light,
            WindowColorMode::Accent,
            WindowTintScope::Windows,
            TrafficLightStyle::Wallpaper,
            Some(accent),
        ), Some(dominant()));
        assert_eq!(
            with_dominant.lights[0],
            dominant().darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN)
        );
        assert_eq!(
            with_dominant.lights[2],
            dominant().lighten(TRAFFIC_ZOOM_LIGHTEN)
        );

        let without_dominant = resolve(&appearance(
            ColorScheme::Light,
            WindowColorMode::Accent,
            WindowTintScope::Windows,
            TrafficLightStyle::Wallpaper,
            Some(accent),
        ), None);
        let normal = Rgba {
            r: accent[0],
            g: accent[1],
            b: accent[2],
            a: 1.0,
        };
        assert_eq!(
            without_dominant.lights[0],
            normal.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN)
        );
        assert_eq!(
            without_dominant.lights[2],
            normal.lighten(TRAFFIC_ZOOM_LIGHTEN)
        );
    }

    #[test]
    fn traffic_light_styles_are_resolved() {
        let macos = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Accent,
                WindowTintScope::Windows,
                TrafficLightStyle::Macos,
                None,
            ),
            None,
        );
        assert_eq!(
            macos.lights,
            [
                TITLEBAR.buttons.order[0].color,
                TITLEBAR.buttons.order[1].color,
                TITLEBAR.buttons.order[2].color,
            ]
        );

        let accent = Rgba {
            r: 0.9,
            g: 0.3,
            b: 0.1,
            a: 1.0,
        };
        let derived = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Accent,
                WindowTintScope::Windows,
                TrafficLightStyle::Accent,
                Some([0.9, 0.3, 0.1]),
            ),
            None,
        );
        assert_eq!(
            derived.lights[0],
            accent.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN)
        );
        assert_eq!(derived.lights[2], accent.lighten(TRAFFIC_ZOOM_LIGHTEN));
    }
}
