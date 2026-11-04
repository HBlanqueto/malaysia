//! Runtime configuration of the compositor, loaded from `.domain` TOML files.

use serde::Deserialize;

pub use vow_appearance::{AppearanceConfig, config_dir};

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
    pub menubar: MenubarConfig,
    pub display: DisplayConfig,
    pub screenshot: ScreenshotConfig,
    pub lock: LockConfig,
    pub debug: DebugConfig,
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ScreenshotConfig {
    pub show_pointer: bool,
    pub target: ScreenshotTarget,
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LockConfig {
    pub idle_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub decorations: DecorationPolicy,
    pub double_click_titlebar: DoubleClickAction,
    pub green_button: GreenButtonAction,
    pub resize_border: i32,
    pub resize_corner: i32,
    pub qt_server_decorations: bool,
    pub focus_titlebar_apps: Vec<String>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            decorations: DecorationPolicy::default(),
            double_click_titlebar: DoubleClickAction::default(),
            green_button: GreenButtonAction::default(),
            resize_border: 10,
            resize_corner: 20,
            qt_server_decorations: true,
            focus_titlebar_apps: ["firefox", "brave-browser", "chromium-browser"]
                .map(String::from)
                .to_vec(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MenubarConfig {
    pub app_menus: bool,
}

impl Default for MenubarConfig {
    fn default() -> Self {
        Self { app_menus: true }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct DebugConfig {
    pub overlay_planes: bool,
}

impl Config {
    pub fn load() -> Self {
        Self::load_from_dir(&config_dir())
    }

    pub fn load_from_dir(config_dir: &std::path::Path) -> Self {
        Self {
            appearance: load_section(config_dir, "appearance"),
            window: load_section(config_dir, "window"),
            menubar: load_section(config_dir, "menubar"),
            display: load_section(config_dir, "display"),
            screenshot: load_section(config_dir, "screenshot"),
            lock: load_section(config_dir, "lock"),
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
    use vow_appearance::{ColorScheme, TrafficLightStyle, WindowColorMode, WindowTintScope};

    #[test]
    fn per_domain_files_are_loaded_separately() {
        let config_dir =
            std::env::temp_dir().join(format!("malaysia-cfg-test-{}", std::process::id()));
        std::fs::create_dir_all(&config_dir).expect("create the temp config dir");
        std::fs::write(
            config_dir.join("appearance.domain"),
            "color_scheme = \"dark\"\nwindow_color = \"wallpaper\"\nwindow_tint_scope = \"desktop\"\ntraffic_lights = \"accent\"\n",
        )
        .expect("write appearance.domain");
        std::fs::write(config_dir.join("display.domain"), "scale = 1.5\n")
            .expect("write display.domain");
        std::fs::write(
            config_dir.join("screenshot.domain"),
            "show_pointer = true\ntarget = \"clipboard\"\n",
        )
        .expect("write screenshot.domain");

        let config = Config::load_from_dir(&config_dir);
        assert_eq!(config.appearance.color_scheme, ColorScheme::Dark);
        assert_eq!(config.appearance.window_color, WindowColorMode::Wallpaper);
        assert_eq!(
            config.appearance.window_tint_scope,
            WindowTintScope::Desktop
        );
        assert_eq!(config.appearance.traffic_lights, TrafficLightStyle::Accent);
        assert_eq!(config.window.decorations, DecorationPolicy::Default);
        assert_eq!(config.display.scale, 1.5);
        assert!(config.screenshot.show_pointer);
        assert_eq!(config.screenshot.target, ScreenshotTarget::Clipboard);

        let _ = std::fs::remove_dir_all(&config_dir);
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
        assert!(config.menubar.app_menus);
        assert_eq!(config.display.scale, 1.0);
        assert!(!config.screenshot.show_pointer);
        assert_eq!(config.screenshot.target, ScreenshotTarget::Desktop);
        assert_eq!(config.lock.idle_timeout_seconds, 0);
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
    }

    #[test]
    fn documented_macos_keys_round_trip() {
        let text = r#"
            [appearance]
            color_scheme = "dark"
            accent_color = [0.208, 0.518, 0.894]

            [window]
            decorations = "force_server"
            double_click_titlebar = "minimize"
            green_button = "fullscreen"
            resize_border = 8
            resize_corner = 20

            [menubar]
            app_menus = false

            [lock]
            idle_timeout_seconds = 15

            [debug]
            overlay_planes = true
        "#;
        let parsed: Config = toml::from_str(text).expect("the documented keys parse");
        assert_eq!(parsed.appearance.color_scheme, ColorScheme::Dark);
        assert_eq!(parsed.appearance.accent_color, Some([0.208, 0.518, 0.894]));
        assert_eq!(parsed.window.decorations, DecorationPolicy::ForceServer);
        assert_eq!(
            parsed.window.double_click_titlebar,
            DoubleClickAction::Minimize
        );
        assert_eq!(parsed.window.green_button, GreenButtonAction::Fullscreen);
        assert_eq!(parsed.window.resize_border, 8);
        assert_eq!(parsed.window.resize_corner, 20);
        assert!(!parsed.menubar.app_menus);
        assert_eq!(parsed.lock.idle_timeout_seconds, 15);
        assert!(parsed.debug.overlay_planes);
    }
}
