//! Runtime configuration of the compositor, loaded from `.domain` TOML files.

use serde::Deserialize;
use std::path::PathBuf;

pub use vow_appearance::{
    AppearanceConfig, ColorScheme, TrafficLightStyle, WindowColorMode, WindowTintScope, config_dir,
    data_dir,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecorationPolicy {
    #[default]
    Default,
    ForceServer,
    ForceClient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusPolicy {
    #[default]
    Click,
    Sloppy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimizeEffect {
    Suck,
    #[default]
    Genie,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockOrientation {
    #[default]
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoubleClickAction {
    Fill,
    Minimize,
    #[default]
    Zoom,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GreenButtonAction {
    #[default]
    Focus,
    Zoom,
    Fullscreen,
    None,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub appearance: AppearanceConfig,
    pub window: WindowConfig,
    pub dock: DockConfig,
    pub menubar: MenubarConfig,
    pub wallpaper: WallpaperConfig,
    pub display: DisplayConfig,
    pub screenshot: ScreenshotConfig,
    pub lock: LockConfig,
    pub effects: EffectsConfig,
    pub debug: DebugConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperScale {
    #[default]
    Fill,
    Stretch,
    Fit,
    Center,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub scale: f64,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ScreenshotConfig {
    pub show_pointer: bool,
    pub show_thumbnail: bool,
    pub target: ScreenshotTarget,
    pub location: Option<PathBuf>,
    #[serde(rename = "type")]
    pub format: ScreenshotFormat,
    pub include_date: bool,
    pub disable_shadow: bool,
    pub timer: u32,
    pub remember_selection: bool,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            show_pointer: false,
            show_thumbnail: true,
            target: ScreenshotTarget::Desktop,
            location: None,
            format: ScreenshotFormat::Png,
            include_date: true,
            disable_shadow: false,
            timer: 0,
            remember_selection: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenshotTarget {
    #[default]
    Desktop,
    Clipboard,
    Preview,
    Mail,
    Messages,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotFormat {
    #[default]
    Png,
    #[serde(rename = "jpg", alias = "jpeg")]
    Jpg,
    Heif,
    Tiff,
    Pdf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LockConfig {
    pub idle_timeout_seconds: u64,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            idle_timeout_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub dir: String,
    pub scale: WallpaperScale,
    pub shuffle: bool,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            dir: vow_design::WALLPAPER_DIR.to_owned(),
            scale: WallpaperScale::default(),
            shuffle: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub decorations: DecorationPolicy,
    pub titlebar: bool,
    pub double_click_titlebar: DoubleClickAction,
    pub green_button: GreenButtonAction,
    pub focus: FocusPolicy,
    pub resize_border: i32,
    pub resize_corner: i32,
    pub qt_server_decorations: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            decorations: DecorationPolicy::default(),
            titlebar: true,
            double_click_titlebar: DoubleClickAction::default(),
            green_button: GreenButtonAction::default(),
            focus: FocusPolicy::default(),
            resize_border: 10,
            resize_corner: 20,
            qt_server_decorations: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DockConfig {
    pub autohide: bool,
    pub orientation: DockOrientation,
    pub magnification: bool,
    pub tilesize: i32,
    pub largesize: i32,
    pub mineffect: MinimizeEffect,
    #[serde(rename = "show-process-indicators")]
    pub show_process_indicators: bool,
    #[serde(rename = "static-only")]
    pub static_only: bool,
    #[serde(rename = "minimize-to-application")]
    pub minimize_to_application: bool,
    #[serde(rename = "show-recents")]
    pub show_recents: bool,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            autohide: false,
            orientation: DockOrientation::default(),
            magnification: false,
            tilesize: 48,
            largesize: 64,
            mineffect: MinimizeEffect::default(),
            show_process_indicators: true,
            static_only: false,
            minimize_to_application: false,
            show_recents: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MenubarConfig {
    pub autohide: bool,
    pub app_menus: bool,
}

impl Default for MenubarConfig {
    fn default() -> Self {
        Self {
            autohide: false,
            app_menus: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EffectsConfig {
    pub inner_border: InnerBorderConfig,
    pub shadow: ShadowConfig,
    pub blur: BlurConfig,
    pub corner_radius: f32,
}

impl Default for EffectsConfig {
    fn default() -> Self {
        Self {
            inner_border: InnerBorderConfig::default(),
            shadow: ShadowConfig::default(),
            blur: BlurConfig::default(),
            corner_radius: 0.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct InnerBorderConfig {
    pub enabled: bool,
    pub width: f32,
    pub color: [f32; 3],
}

impl Default for InnerBorderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            width: 1.0,
            color: [0.5, 0.5, 0.5],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ShadowConfig {
    pub enabled: bool,
    pub radius: f32,
    pub offset: [f32; 2],
    pub opacity: f32,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            radius: 24.0,
            offset: [0.0, 6.0],
            opacity: 0.35,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BlurConfig {
    pub enabled: bool,
    pub radius: f32,
}

impl Default for BlurConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            radius: 20.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct DebugConfig {
    pub damage_highlight: bool,
}

impl Config {
    pub fn load() -> Self {
        Self::load_from_dirs(&config_dir(), &data_dir())
    }

    pub fn load_from_dirs(config_dir: &std::path::Path, data_dir: &std::path::Path) -> Self {
        Self {
            appearance: load_section(config_dir, "appearance"),
            window: load_section(config_dir, "window"),
            dock: load_section(config_dir, "dock"),
            menubar: load_section(config_dir, "menubar"),
            wallpaper: load_section(data_dir, "wallpaper"),
            display: load_section(config_dir, "display"),
            screenshot: load_section(config_dir, "screenshot"),
            lock: load_section(config_dir, "lock"),
            effects: load_section(config_dir, "effects"),
            debug: load_section(config_dir, "debug"),
        }
    }
}

fn load_section<T>(dir: &std::path::Path, name: &str) -> T
where
    T: Default + serde::de::DeserializeOwned,
{
    let path = dir.join(format!("{name}.domain"));
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|error| {
            tracing::warn!(path = %path.display(), %error, "config file ignored");
            T::default()
        }),
        Err(_) => T::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_domain_files_are_loaded_separately() {
        let base = std::env::temp_dir().join(format!("malaysia-cfg-test-{}", std::process::id()));
        let config_dir = base.join("config");
        let data_dir = base.join("data");
        std::fs::create_dir_all(&config_dir).expect("create the temp config dir");
        std::fs::create_dir_all(&data_dir).expect("create the temp data dir");
        std::fs::write(
            config_dir.join("appearance.domain"),
            "color_scheme = \"dark\"\nwindow_color = \"wallpaper\"\nwindow_tint_scope = \"desktop\"\ntraffic_lights = \"accent\"\n",
        )
        .expect("write appearance.domain");
        std::fs::write(
            config_dir.join("dock.domain"),
            "autohide = true\norientation = \"left\"\n",
        )
        .expect("write dock.domain");
        std::fs::write(config_dir.join("display.domain"), "scale = 1.5\n")
            .expect("write display.domain");
        std::fs::write(
            config_dir.join("screenshot.domain"),
            "show_pointer = true\nshow_thumbnail = false\ntarget = \"clipboard\"\nlocation = \"/tmp/shots\"\ntype = \"jpg\"\ninclude_date = false\ndisable_shadow = true\ntimer = 5\nremember_selection = true\n",
        )
        .expect("write screenshot.domain");
        std::fs::write(
            data_dir.join("wallpaper.domain"),
            "dir = \"/run/current-system/sw/share/wallpapers/solid-colors\"\nscale = \"fit\"\n\n[desktops]\ndefault = \"file:///w/Solid Mint.png\"\n",
        )
        .expect("write wallpaper.domain");

        let config = Config::load_from_dirs(&config_dir, &data_dir);
        assert_eq!(config.appearance.color_scheme, ColorScheme::Dark);
        assert_eq!(config.appearance.window_color, WindowColorMode::Wallpaper);
        assert_eq!(
            config.appearance.window_tint_scope,
            WindowTintScope::Desktop
        );
        assert_eq!(config.appearance.traffic_lights, TrafficLightStyle::Accent);
        assert!(config.dock.autohide);
        assert_eq!(config.dock.orientation, DockOrientation::Left);
        assert_eq!(config.wallpaper.scale, WallpaperScale::Fit);
        assert_eq!(config.window.decorations, DecorationPolicy::Default);
        assert!(!config.effects.blur.enabled);
        assert_eq!(config.display.scale, 1.5);
        assert!(config.screenshot.show_pointer);
        assert!(!config.screenshot.show_thumbnail);
        assert_eq!(config.screenshot.target, ScreenshotTarget::Clipboard);
        assert_eq!(
            config.screenshot.location.as_deref(),
            Some(std::path::Path::new("/tmp/shots"))
        );
        assert_eq!(config.screenshot.format, ScreenshotFormat::Jpg);
        assert!(!config.screenshot.include_date);
        assert!(config.screenshot.disable_shadow);
        assert_eq!(config.screenshot.timer, 5);
        assert!(config.screenshot.remember_selection);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn defaults_are_complete_and_sane() {
        let config = Config::default();
        assert_eq!(config.appearance.color_scheme, ColorScheme::Auto);
        assert!(!config.appearance.contrast);
        assert!(!config.appearance.reduced_motion);
        assert_eq!(config.appearance.window_color, WindowColorMode::Neutral);
        assert_eq!(
            config.appearance.window_tint_scope,
            WindowTintScope::Windows
        );
        assert_eq!(config.appearance.traffic_lights, TrafficLightStyle::Auto);
        assert_eq!(config.window.decorations, DecorationPolicy::Default);
        assert!(config.window.titlebar);
        assert_eq!(config.window.focus, FocusPolicy::Click);
        assert!(!config.dock.autohide);
        assert_eq!(config.dock.orientation, DockOrientation::Bottom);
        assert_eq!(config.dock.tilesize, 48);
        assert_eq!(config.dock.mineffect, MinimizeEffect::Genie);
        assert!(config.dock.show_process_indicators);
        assert!(!config.menubar.autohide);
        assert!(config.menubar.app_menus);
        assert_eq!(config.wallpaper.scale, WallpaperScale::Fill);
        assert!(!config.wallpaper.shuffle);
        assert!(!config.effects.inner_border.enabled);
        assert!(!config.effects.shadow.enabled);
        assert!(!config.effects.blur.enabled);
        assert_eq!(config.display.scale, 1.0);
        assert!(!config.screenshot.show_pointer);
        assert!(config.screenshot.show_thumbnail);
        assert_eq!(config.screenshot.target, ScreenshotTarget::Desktop);
        assert_eq!(config.screenshot.format, ScreenshotFormat::Png);
        assert!(config.screenshot.include_date);
        assert!(!config.screenshot.disable_shadow);
        assert_eq!(config.screenshot.timer, 0);
        assert!(!config.screenshot.remember_selection);
        assert_eq!(config.lock.idle_timeout_seconds, 300);
    }

    #[test]
    fn the_green_button_defaults_to_focus_and_parses_lowercase() {
        assert_eq!(
            WindowConfig::default().green_button,
            GreenButtonAction::Focus
        );
        for (text, expected) in [
            ("focus", GreenButtonAction::Focus),
            ("zoom", GreenButtonAction::Zoom),
            ("fullscreen", GreenButtonAction::Fullscreen),
            ("none", GreenButtonAction::None),
        ] {
            let parsed: Config = toml::from_str(&format!("[window]\ngreen_button = \"{text}\"\n"))
                .expect("green_button parses");
            assert_eq!(parsed.window.green_button, expected);
        }
    }

    #[test]
    fn the_resize_band_defaults_are_10_and_20() {
        let window = WindowConfig::default();
        assert_eq!(window.resize_border, 10);
        assert_eq!(window.resize_corner, 20);
    }

    #[test]
    fn auto_resolves_to_the_light_default() {
        assert_eq!(ColorScheme::Auto.resolved(), ColorScheme::Light);
        assert_eq!(ColorScheme::Dark.resolved(), ColorScheme::Dark);
        assert_eq!(ColorScheme::DarkTint.resolved(), ColorScheme::DarkTint);
    }

    #[test]
    fn dark_tint_is_a_color_scheme_value() {
        let parsed: Config = toml::from_str("[appearance]\ncolor_scheme = \"dark_tint\"\n")
            .expect("dark_tint parses");
        assert_eq!(parsed.appearance.color_scheme, ColorScheme::DarkTint);
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_missing_keys() {
        let parsed: Config = toml::from_str("").expect("an empty file is a valid config");
        assert_eq!(parsed.appearance.color_scheme, ColorScheme::Auto);
        assert!(parsed.window.titlebar);
        assert!(!parsed.effects.blur.enabled);
    }

    #[test]
    fn documented_macos_keys_round_trip() {
        let text = r#"
            [appearance]
            color_scheme = "dark"
            accent_color = [0.208, 0.518, 0.894]

            [window]
            decorations = "force_server"
            titlebar = false
            double_click_titlebar = "minimize"
            green_button = "fullscreen"
            focus = "sloppy"
            resize_border = 8
            resize_corner = 20

            [dock]
            autohide = true
            orientation = "left"
            magnification = true
            tilesize = 64
            largesize = 96
            mineffect = "scale"
            show-process-indicators = false
            static-only = true
            minimize-to-application = true
            show-recents = false

            [menubar]
            autohide = true

            [wallpaper]
            dir = "/home/user/Pictures/Solid Colors"
            scale = "fit"
            shuffle = true

            [lock]
            idle_timeout_seconds = 15

            [effects]
            corner_radius = 12.0
            [effects.inner_border]
            enabled = true
            width = 1.5
            color = [1.0, 1.0, 1.0]
            [effects.shadow]
            enabled = true
            radius = 30.0
            offset = [0.0, 8.0]
            opacity = 0.4
            [effects.blur]
            enabled = true
            radius = 25.0

            [debug]
            damage_highlight = true
        "#;
        let parsed: Config = toml::from_str(text).expect("the documented keys parse");
        assert_eq!(parsed.appearance.color_scheme, ColorScheme::Dark);
        assert_eq!(parsed.appearance.accent_color, Some([0.208, 0.518, 0.894]));
        assert_eq!(parsed.window.decorations, DecorationPolicy::ForceServer);
        assert!(!parsed.window.titlebar);
        assert_eq!(
            parsed.window.double_click_titlebar,
            DoubleClickAction::Minimize
        );
        assert_eq!(parsed.window.green_button, GreenButtonAction::Fullscreen);
        assert_eq!(parsed.window.focus, FocusPolicy::Sloppy);
        assert_eq!(parsed.window.resize_border, 8);
        assert_eq!(parsed.window.resize_corner, 20);
        assert!(parsed.dock.autohide);
        assert_eq!(parsed.dock.orientation, DockOrientation::Left);
        assert!(parsed.dock.magnification);
        assert_eq!(parsed.dock.tilesize, 64);
        assert_eq!(parsed.dock.mineffect, MinimizeEffect::Scale);
        assert!(!parsed.dock.show_process_indicators);
        assert!(parsed.dock.static_only);
        assert!(parsed.dock.minimize_to_application);
        assert!(!parsed.dock.show_recents);
        assert!(parsed.menubar.autohide);
        assert_eq!(parsed.wallpaper.dir, "/home/user/Pictures/Solid Colors");
        assert_eq!(parsed.wallpaper.scale, WallpaperScale::Fit);
        assert!(parsed.wallpaper.shuffle);
        assert_eq!(parsed.lock.idle_timeout_seconds, 15);
        assert!(parsed.effects.inner_border.enabled);
        assert_eq!(parsed.effects.inner_border.color, [1.0, 1.0, 1.0]);
        assert!(parsed.effects.shadow.enabled);
        assert!(parsed.effects.blur.enabled);
        assert!(parsed.debug.damage_highlight);
    }
}
