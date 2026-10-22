//! Runtime configuration of the compositor.
//!
//! This module is the compositor's half of the settings surface defined in ADR
//! 0021: a documented catalog of values the compositor can be told to change,
//! loadable from a TOML file today and (later) writable over the D-Bus command
//! surface (`domain`) and from a settings app. Every option here is listed
//! in `docs/configuracion/opciones.md`, which is the canonical catalog.
//!
//! The option names and values deliberately mirror macOS: the keys of
//! `com.apple.dock` (`autohide`, `orientation`, `mineffect`, ...), the System
//! Settings sections (`appearance`, `dock`, `menubar`) and its vocabulary
//! (`light`/`dark`/`auto`, `genie`/`scale`, `fill`/`minimize`/`zoom`). A setting
//! that has an exact macOS counterpart uses it verbatim.
//!
//! The structs are plain `serde` data with defaults: a config file may set any
//! subset, and every missing key falls back to the compiled default. Nothing
//! here executes on its own — handlers read these values and act on them.

use serde::Deserialize;
use std::path::PathBuf;

/// The compositor's preferred appearance, published over the appearance portal
/// (`org.freedesktop.appearance.color-scheme`). macOS calls these values
/// "Light", "Dark" and "Auto".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    /// Follow the user's preference ("Auto").
    #[default]
    Auto,
    /// Prefer the dark appearance ("Dark").
    Dark,
    /// Prefer the light appearance ("Light").
    Light,
}

/// Which windows get the compositor's server-side chrome.
///
/// `Default` is the ADR 0019/0020 policy: GTK/Libadwaita keep their client-side
/// appearance, Qt and the rest get Yuki's titlebar. The other two values force
/// the decision either way, for theming experiments and debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecorationPolicy {
    /// Honour the client negotiation (ADR 0019) — the default and recommended.
    #[default]
    Default,
    /// Draw Yuki's chrome on every window, even a GTK headerbar.
    ForceServer,
    /// Draw no Yuki chrome on any window.
    ForceClient,
}

/// When a window gains the pointer focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusPolicy {
    /// Focus only on click, like macOS.
    #[default]
    Click,
    /// Follow the pointer (sloppy focus).
    Sloppy,
}

/// The animation used when a window is minimized to the dock. macOS calls these
/// "Genie", "Scale" and (older systems) "Suck".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimizeEffect {
    /// The "suck" animation (macOS `mineffect = "suck"`).
    Suck,
    /// The "genie" animation, macOS's default.
    #[default]
    Genie,
    /// The "scale" animation.
    Scale,
}

/// The screen edge the dock sits on. Same values as macOS `orientation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockOrientation {
    /// The bottom edge, macOS's default.
    #[default]
    Bottom,
    /// The left edge.
    Left,
    /// The right edge.
    Right,
}

/// What double-clicking a window's title bar does. Same values as macOS System
/// Settings "Double-click a window's title bar to".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoubleClickAction {
    /// Expand the window to fill the area between the menu bar and the dock.
    Fill,
    /// Minimize the window to the dock.
    Minimize,
    /// Expand the window to fill the space between the dock and the menu bar.
    #[default]
    Zoom,
    /// Do nothing.
    None,
}

/// The full runtime configuration, read from one **`.domain` file per domain**
/// (ADR 0021) — like macOS and its per-domain plists.
///
/// Yuki's own preference files carry the `.domain` extension (like macOS's
/// `.plist`); their content is TOML. Like macOS, preferences live in
/// `~/.config/yuki/` (`~/Library/Preferences`) and *state/data* — the wallpaper
/// store — in `~/.local/share/yuki/` (`~/Library/Application Support`).
///
/// The sections mirror macOS System Settings (`appearance`, `window`, `dock`,
/// `menubar`); the keys mirror macOS defaults keys verbatim where one exists.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Appearance. Consumed by the appearance portal and the future shell.
    pub appearance: AppearanceConfig,
    /// Window chrome and behaviour. Consumed by the decoration handlers.
    pub window: WindowConfig,
    /// The dock surfaces of the shell (macOS `com.apple.dock` keys).
    pub dock: DockConfig,
    /// The menu bar surfaces of the shell (macOS "Menu Bar").
    pub menubar: MenubarConfig,
    /// The system wallpapers. The current selection lives in a *separate* store,
    /// like macOS keeps it in `com.apple.wallpaper` (see `docs/configuracion`).
    pub wallpaper: WallpaperConfig,
    /// Display scale (System Settings → Displays). Yuki-specific: macOS has no
    /// fractional-scale preference, it picks a mode per display.
    pub display: DisplayConfig,
    /// Screenshots (macOS Screenshot app and `com.apple.screencapture`).
    pub screenshot: ScreenshotConfig,
    /// The lock screen and automatic locking.
    pub lock: LockConfig,
    /// Compositor-side surface effects (inner border, shadow, blur). ADR 0020.
    pub effects: EffectsConfig,
    /// Development and debugging switches.
    pub debug: DebugConfig,
}

/// How a wallpaper image is scaled to the output. Same words as macOS
/// (`desktoppr scale`): `fill` is macOS's default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperScale {
    /// Scale to cover the output, cropping the excess (macOS default).
    #[default]
    Fill,
    /// Stretch to the output, distorting the image.
    Stretch,
    /// Scale to fit entirely inside the output.
    Fit,
    /// Centre the image at its native size.
    Center,
}

/// Display options (System Settings → Displays).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// The output scale, `1.0..=4.0`. Fractional values go to clients through
    /// `wp_fractional_scale_v1`; the integer part is what `wl_output` carries.
    /// This is Yuki-specific: macOS chooses a display mode instead.
    pub scale: f64,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

/// What the screenshot hotkeys do with the capture.
///
/// The words follow the macOS Screenshot app ("Save to: Desktop / Clipboard /
/// Mail / ...") and `com.apple.screencapture` (`type`, `location`,
/// `include-date`, `disable-shadow`). The defaults are macOS's: save to the
/// Desktop as PNG with the date, pointer hidden, floating thumbnail shown.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ScreenshotConfig {
    /// Show the mouse pointer in full-screen and region captures. macOS hides
    /// it by default. Window captures never include it.
    pub show_pointer: bool,
    /// Show the floating thumbnail after a capture (Screenshot app option).
    pub show_thumbnail: bool,
    /// Where the capture goes ("Save to").
    pub target: ScreenshotTarget,
    /// A custom folder for `target = "other"` or to override the desktop.
    pub location: Option<PathBuf>,
    /// File type (`com.apple.screencapture type`).
    #[serde(rename = "type")]
    pub format: ScreenshotFormat,
    /// Include the date and time in the file name (`include-date`).
    pub include_date: bool,
    /// Leave the window's shadow out of window captures (`disable-shadow`).
    pub disable_shadow: bool,
    /// Seconds to wait before capturing (Screenshot app "Timer"): 0, 5 or 10.
    pub timer: u32,
    /// Remember the last region or window selection (Screenshot app).
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

/// The Screenshot app's "Save to" choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenshotTarget {
    /// The XDG Desktop directory (macOS's default).
    #[default]
    Desktop,
    /// Copy to the clipboard instead of writing a file.
    Clipboard,
    /// Open the capture in Preview.
    Preview,
    /// Attach it to a new Mail message.
    Mail,
    /// Attach it to a new Messages conversation.
    Messages,
    /// The folder in [`ScreenshotConfig::location`].
    Other,
}

/// `com.apple.screencapture type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotFormat {
    /// PNG (macOS's default).
    #[default]
    Png,
    /// JPEG; `jpg` is what macOS writes in `type`, `jpeg` is accepted too.
    #[serde(rename = "jpg", alias = "jpeg")]
    Jpg,
    /// HEIF.
    Heif,
    /// TIFF.
    Tiff,
    /// PDF.
    Pdf,
}

/// Lock screen and idle options.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LockConfig {
    /// Seconds without input before the session locks itself. `0` disables
    /// auto-lock. The default is five minutes.
    pub idle_timeout_seconds: u64,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            idle_timeout_seconds: 300,
        }
    }
}

/// The system wallpaper catalog (System Settings → Wallpaper).
///
/// This section describes *which* wallpapers exist and how they are drawn; the
/// current selection per desktop is kept separately, like macOS's
/// `com.apple.wallpaper`, and read by the shell (`yuki-shell wallpaper`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    /// The folder the system wallpapers live in (e.g. the macOS "Solid Colors"
    /// set). The shell builds the current image path from this + the store.
    pub dir: String,
    /// How a wallpaper image is scaled to the output (see [`WallpaperScale`]).
    pub scale: WallpaperScale,
    /// Rotate the wallpapers in `dir` on an interval (macOS "Shuffle").
    pub shuffle: bool,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            dir: yuki_design::WALLPAPER_DIR.to_owned(),
            scale: WallpaperScale::default(),
            shuffle: false,
        }
    }
}

/// Appearance options (macOS System Settings → Appearance).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AppearanceConfig {
    /// "Light", "Dark" or "Auto". `None` → `ColorScheme::Auto`.
    pub color_scheme: ColorScheme,
    /// An sRGB accent color, `[r, g, b]` in `[0, 1]`. `None` → the default.
    pub accent_color: Option<[f32; 3]>,
}

/// Window chrome and behaviour (macOS System Settings → Windows & Apps).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Which windows get server-side chrome (see [`DecorationPolicy`]).
    pub decorations: DecorationPolicy,
    /// Whether a server-decorated window gets a titlebar at all. A window can
    /// be server-decorated with only an inner border, like niri.
    pub titlebar: bool,
    /// What a double click on the title bar does (see [`DoubleClickAction`]).
    pub double_click_titlebar: DoubleClickAction,
    /// When the pointer focus changes the focused window (see [`FocusPolicy`]).
    pub focus: FocusPolicy,
    /// The thickness, in logical pixels, of the resize edge band. Today a
    /// constant in `window::RESIZE_BORDER`; this is where it becomes configurable.
    pub resize_border: i32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            decorations: DecorationPolicy::default(),
            titlebar: true,
            double_click_titlebar: DoubleClickAction::default(),
            focus: FocusPolicy::default(),
            resize_border: 6,
        }
    }
}

/// Dock options, with macOS `com.apple.dock` keys verbatim.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DockConfig {
    /// `autohide` — automatically hide and show the dock.
    pub autohide: bool,
    /// `orientation` — the screen edge the dock sits on.
    pub orientation: DockOrientation,
    /// `magnification` — magnify dock icons on hover.
    pub magnification: bool,
    /// `tilesize` — the dock icon size in pixels.
    pub tilesize: i32,
    /// `largesize` — the dock icon size when magnified, in pixels.
    pub largesize: i32,
    /// `mineffect` — the minimize animation (`genie`, `scale`, `suck`).
    pub mineffect: MinimizeEffect,
    /// `show-process-indicators` — a dot under running apps.
    #[serde(rename = "show-process-indicators")]
    pub show_process_indicators: bool,
    /// `static-only` — show only open apps.
    #[serde(rename = "static-only")]
    pub static_only: bool,
    /// `minimize-to-application` — minimize into the app's icon.
    #[serde(rename = "minimize-to-application")]
    pub minimize_to_application: bool,
    /// `show-recents` — show suggested and recent apps.
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

/// Menu bar options (macOS System Settings → Menu Bar).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct MenubarConfig {
    /// `autohide` — automatically hide and show the menu bar.
    pub autohide: bool,
}

/// Compositor-side surface effects (ADR 0020). These apply to *every* window,
/// client- or server-decorated. Not rendered yet; they are the design contract.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EffectsConfig {
    /// The macOS-style inner border around the window content.
    pub inner_border: InnerBorderConfig,
    /// The window drop shadow.
    pub shadow: ShadowConfig,
    /// The backdrop blur behind translucent surfaces (a transparent theme).
    pub blur: BlurConfig,
    /// The window corner radius, if the compositor clips client surfaces.
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

/// The macOS-style inner border.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct InnerBorderConfig {
    /// Whether the compositor draws the inner border.
    pub enabled: bool,
    /// Its thickness in logical pixels.
    pub width: f32,
    /// Its colour as sRGB `[r, g, b]` in `[0, 1]`.
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

/// The window drop shadow.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ShadowConfig {
    /// Whether the compositor draws the shadow.
    pub enabled: bool,
    /// The blur radius of the shadow in logical pixels.
    pub radius: f32,
    /// The shadow offset, `[x, y]`, in logical pixels.
    pub offset: [f32; 2],
    /// The shadow opacity in `[0, 1]`.
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

/// The backdrop blur.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BlurConfig {
    /// Whether the compositor blurs what is behind translucent surfaces.
    pub enabled: bool,
    /// The blur radius in logical pixels.
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

/// Development and debugging switches.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct DebugConfig {
    /// Highlight the damaged regions each frame, for render debugging.
    pub damage_highlight: bool,
}

impl Config {
    /// Loads the configuration from the filesystem, falling back to defaults.
    ///
    /// Like macOS, the configuration is split **one `.domain` file per domain**
    /// instead of a single monolithic file. Preference domains (`appearance`,
    /// `window`, `dock`, `menubar`, `screenshot`, `display`, `lock`, `effects`,
    /// `debug`) load from
    /// `~/.config/yuki/` — the `~/Library/Preferences` equivalent. The wallpaper
    /// domain — catalog *and* current selection — loads from
    /// `~/.local/share/yuki/`, the `~/Library/Application Support` equivalent,
    /// exactly as macOS keeps `com.apple.wallpaper` there.
    ///
    /// Each domain file is optional: a missing file, or an unparsable one, falls
    /// back to that domain's default. `YUKI_CONFIG_DIR`/`YUKI_DATA_DIR` override
    /// the two directories.
    pub fn load() -> Self {
        Self::load_from_dirs(&config_dir(), &data_dir())
    }

    /// Loads the per-domain `.domain` files from the two directories.
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

/// The preferences directory: `YUKI_CONFIG_DIR`, else `$XDG_CONFIG_HOME/yuki`
/// (`~/.config/yuki` by default) — the `~/Library/Preferences` equivalent.
fn config_dir() -> PathBuf {
    std::env::var_os("YUKI_CONFIG_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|h| h.join(".config"))
                })
                .unwrap_or_else(|| PathBuf::from(".config"));
            base.join("yuki")
        },
        PathBuf::from,
    )
}

/// The state/data directory: `YUKI_DATA_DIR`, else `$XDG_DATA_HOME/yuki`
/// (`~/.local/share/yuki` by default) — the `~/Library/Application Support`
/// equivalent, where the wallpaper store lives.
fn data_dir() -> PathBuf {
    std::env::var_os("YUKI_DATA_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|h| h.join(".local/share"))
                })
                .unwrap_or_else(|| PathBuf::from(".local/share"));
            base.join("yuki")
        },
        PathBuf::from,
    )
}

/// Reads one domain file (`<dir>/<name>.domain`) as a typed section, falling back
/// to the domain's default when the file is missing or unparsable.
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
        // Like macOS: one .domain file per domain, defaults for the rest;
        // preferences in the config dir, wallpaper (state) in the data dir.
        let base = std::env::temp_dir().join(format!("yuki-cfg-test-{}", std::process::id()));
        let config_dir = base.join("config");
        let data_dir = base.join("data");
        std::fs::create_dir_all(&config_dir).expect("create the temp config dir");
        std::fs::create_dir_all(&data_dir).expect("create the temp data dir");
        std::fs::write(
            config_dir.join("appearance.domain"),
            "color_scheme = \"dark\"\n",
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
        assert!(config.dock.autohide);
        assert_eq!(config.dock.orientation, DockOrientation::Left);
        // The wallpaper domain comes from the data dir (Application Support).
        assert_eq!(config.wallpaper.scale, WallpaperScale::Fit);
        // Domains without a file keep their defaults.
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
        assert_eq!(config.window.decorations, DecorationPolicy::Default);
        assert!(config.window.titlebar);
        assert_eq!(config.window.focus, FocusPolicy::Click);
        assert!(!config.dock.autohide);
        assert_eq!(config.dock.orientation, DockOrientation::Bottom);
        assert_eq!(config.dock.tilesize, 48);
        assert_eq!(config.dock.mineffect, MinimizeEffect::Genie);
        assert!(config.dock.show_process_indicators);
        assert!(!config.menubar.autohide);
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
            focus = "sloppy"
            resize_border = 8

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
        assert_eq!(parsed.window.focus, FocusPolicy::Sloppy);
        assert_eq!(parsed.window.resize_border, 8);
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
