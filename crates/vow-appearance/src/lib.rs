//! The appearance domain and its resolved colours.
//!
//! This crate is deliberately free of Smithay and of the compositor: the
//! compositor, the shell and the portal all read `appearance.domain` and
//! `wallpaper.state` through the same rules. It only depends on serde, toml
//! and the design tokens.

use std::path::{Path, PathBuf};

use serde::Deserialize;
pub mod traffic;
pub mod watch;

use vow_design::{
    BG_CONTROL_DARK, BG_CONTROL_DARK_TINT, BG_CONTROL_LIGHT, BG_HOVER_DARK, BG_HOVER_DARK_TINT,
    BG_HOVER_LIGHT, BG_RAISED_DARK, BG_RAISED_DARK_TINT, BG_RAISED_LIGHT, BG_SUNKEN_DARK,
    BG_SUNKEN_DARK_TINT, BG_SUNKEN_LIGHT, BG_SURFACE_DARK, BG_SURFACE_DARK_TINT, BG_SURFACE_LIGHT,
    BG_WINDOW_DARK, BG_WINDOW_DARK_TINT, BG_WINDOW_LIGHT, DARK, DARK_TINT, LAYER_FACTOR_BG_SUNKEN,
    LAYER_FACTOR_CONTROL, LAYER_FACTOR_LINE, LAYER_FACTOR_SURFACE, LIGHT, MENU_DARK,
    MENU_DARK_OPAQUE, MENU_DARK_TINT, MENU_DARK_TINT_OPAQUE, MENU_LIGHT, MENU_LIGHT_OPAQUE,
    MenuPalette, PANEL_TINT_WALLPAPER, Palette, Rgba, SETTINGS, SHELL_DARK, SHELL_DARK_OPAQUE,
    SHELL_DARK_TINT, SHELL_DARK_TINT_OPAQUE, SHELL_LIGHT, SHELL_LIGHT_OPAQUE, SIDEBAR_DARK,
    SIDEBAR_DARK_OPAQUE, SIDEBAR_DARK_TINT, SIDEBAR_DARK_TINT_OPAQUE, SIDEBAR_ISLAND_DARK,
    SIDEBAR_ISLAND_DARK_OPAQUE, SIDEBAR_ISLAND_DARK_TINT, SIDEBAR_ISLAND_DARK_TINT_OPAQUE,
    SIDEBAR_ISLAND_INNER_DARK, SIDEBAR_ISLAND_INNER_DARK_TINT, SIDEBAR_ISLAND_INNER_LIGHT,
    SIDEBAR_ISLAND_LIGHT, SIDEBAR_ISLAND_LIGHT_OPAQUE, SIDEBAR_ISLAND_RIM_DARK,
    SIDEBAR_ISLAND_RIM_DARK_TINT, SIDEBAR_ISLAND_RIM_LIGHT, SIDEBAR_LIGHT, SIDEBAR_LIGHT_OPAQUE,
    SIDEBAR_PALETTE_DARK, SIDEBAR_PALETTE_DARK_TINT, SIDEBAR_PALETTE_LIGHT, ShellPalette,
    SidebarPalette, TITLEBAR, TOOLGROUP_BG_DARK, TOOLGROUP_BG_DARK_OPAQUE, TOOLGROUP_BG_DARK_TINT,
    TOOLGROUP_BG_DARK_TINT_OPAQUE, TOOLGROUP_BG_LIGHT, TOOLGROUP_BG_LIGHT_OPAQUE,
    TOOLGROUP_PALETTE_DARK, TOOLGROUP_PALETTE_DARK_TINT, TOOLGROUP_PALETTE_LIGHT,
    TRAFFIC_CLOSE_MINIMIZE_DARKEN, TRAFFIC_ZOOM_LIGHTEN, ToolgroupPalette, WINDOW_TINT_ACCENT,
    WINDOW_TINT_WALLPAPER,
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

/// The kit sidebar style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarStyle {
    /// Floating island with vibrancy, rim and inner line.
    #[default]
    Island,
    /// Flat source list that blends into the window background.
    Flat,
}

/// Neptunia's traffic-light styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficLightStyle {
    /// Same as the window colour mode: standard colours for neutral, accent
    /// or wallpaper-derived colours otherwise.
    #[default]
    Auto,
    /// The fixed standard red/amber/green.
    #[serde(alias = "macos")]
    Standard,
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
    /// "Reduce transparency" (macOS Accessibility). Solid shell surfaces when
    /// true; the default. The value is kept so blur can be enabled later.
    #[serde(default = "default_reduce_transparency")]
    pub reduce_transparency: bool,
    /// What tints the window chrome: nothing, the accent or the wallpaper.
    pub window_color: WindowColorMode,
    /// In wallpaper mode, whether only windows or the panels and dock too are
    /// tinted.
    pub window_tint_scope: WindowTintScope,
    /// The traffic-light palette.
    pub traffic_lights: TrafficLightStyle,
    /// The kit sidebar style.
    pub sidebar_style: SidebarStyle,
}

fn default_reduce_transparency() -> bool {
    true
}

/// The resolved kit sidebar surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SidebarSurface {
    /// The configured style (island or flat).
    pub style: SidebarStyle,
    /// Background fill.
    pub bg: Rgba,
    /// Outer rim colour (transparent in flat mode).
    pub rim: Rgba,
    /// Inner highlight line (transparent in flat mode).
    pub inner: Rgba,
    /// Item/section colours for this theme.
    pub palette: SidebarPalette,
}

/// The resolved toolbar glass capsule surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolgroupSurface {
    /// Capsule background fill.
    pub bg: Rgba,
    /// Button and separator colours for this theme.
    pub palette: ToolgroupPalette,
}

/// The resolved tinted surface stack shared by the compositor and the kit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surfaces {
    /// Window background (`bg-window`).
    pub window: Rgba,
    /// Sunken background (`bg-sunken`).
    pub sunken: Rgba,
    /// Surface background (`bg-surface`).
    pub surface: Rgba,
    /// Raised surface background (`bg-raised`).
    pub raised: Rgba,
    /// Control background (`bg-control`).
    pub control: Rgba,
    /// Hover background (`bg-hover`).
    pub hover: Rgba,
    /// Separator and outline colour (`line`).
    pub line: Rgba,
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
    /// How far the window tint reaches: only the windows or the desktop too.
    pub window_tint_scope: WindowTintScope,
    /// The menubar/dock tint, only in wallpaper + desktop with a dominant.
    pub panel_tint: Option<(Rgba, f32)>,
    /// The three traffic lights.
    pub lights: [Rgba; 3],
    /// The resolved traffic-light style (`Auto` already expanded).
    pub traffic_style: TrafficLightStyle,
    /// Whether the shell and kit must use opaque surfaces.
    pub reduce_transparency: bool,
    /// Whether motion should be minimised.
    pub reduced_motion: bool,
    /// The menubar/dock palette for this appearance.
    pub shell: ShellPalette,
    /// The menu popup palette for this appearance.
    pub menu: MenuPalette,
    /// The kit sidebar surface for this appearance.
    pub sidebar: SidebarSurface,
    /// The kit toolbar capsule surface for this appearance.
    pub toolgroup: ToolgroupSurface,
    /// The tinted surface stack for window backgrounds, controls and lines.
    pub surfaces: Surfaces,
}

/// The Neptunia palette for a concrete scheme.
pub fn palette(scheme: ColorScheme) -> Palette {
    match scheme.resolved() {
        ColorScheme::DarkTint => DARK_TINT,
        ColorScheme::Dark => DARK,
        _ => LIGHT,
    }
}

/// The shell, menu and sidebar surfaces for a concrete theme, choosing the
/// opaque variants when transparency is reduced.
fn chrome_surfaces(
    scheme: ColorScheme,
    style: SidebarStyle,
    opaque: bool,
) -> (ShellPalette, MenuPalette, SidebarSurface, ToolgroupSurface) {
    let sidebar_palette = match scheme {
        ColorScheme::Dark => SIDEBAR_PALETTE_DARK,
        ColorScheme::DarkTint => SIDEBAR_PALETTE_DARK_TINT,
        _ => SIDEBAR_PALETTE_LIGHT,
    };
    let toolgroup_palette = match scheme {
        ColorScheme::Dark => TOOLGROUP_PALETTE_DARK,
        ColorScheme::DarkTint => TOOLGROUP_PALETTE_DARK_TINT,
        _ => TOOLGROUP_PALETTE_LIGHT,
    };
    (
        match (scheme, opaque) {
            (ColorScheme::Dark, false) => SHELL_DARK,
            (ColorScheme::Dark, true) => SHELL_DARK_OPAQUE,
            (ColorScheme::DarkTint, false) => SHELL_DARK_TINT,
            (ColorScheme::DarkTint, true) => SHELL_DARK_TINT_OPAQUE,
            (_, false) => SHELL_LIGHT,
            (_, true) => SHELL_LIGHT_OPAQUE,
        },
        match (scheme, opaque) {
            (ColorScheme::Dark, false) => MENU_DARK,
            (ColorScheme::Dark, true) => MENU_DARK_OPAQUE,
            (ColorScheme::DarkTint, false) => MENU_DARK_TINT,
            (ColorScheme::DarkTint, true) => MENU_DARK_TINT_OPAQUE,
            (_, false) => MENU_LIGHT,
            (_, true) => MENU_LIGHT_OPAQUE,
        },
        sidebar_surface(scheme, style, opaque, sidebar_palette),
        toolgroup_surface(scheme, opaque, toolgroup_palette),
    )
}

/// Resolves the kit sidebar surface for a concrete theme and style.
fn sidebar_surface(
    scheme: ColorScheme,
    style: SidebarStyle,
    opaque: bool,
    palette: SidebarPalette,
) -> SidebarSurface {
    let transparent = Rgba::rgba8(0, 0, 0, 0.0);

    if style == SidebarStyle::Flat {
        let bg = match (scheme, opaque) {
            (ColorScheme::Dark, false) => SIDEBAR_DARK,
            (ColorScheme::Dark, true) => SIDEBAR_DARK_OPAQUE,
            (ColorScheme::DarkTint, false) => SIDEBAR_DARK_TINT,
            (ColorScheme::DarkTint, true) => SIDEBAR_DARK_TINT_OPAQUE,
            (_, false) => SIDEBAR_LIGHT,
            (_, true) => SIDEBAR_LIGHT_OPAQUE,
        };
        return SidebarSurface {
            style,
            bg,
            rim: transparent,
            inner: transparent,
            palette,
        };
    }

    let (bg, rim, inner) = match (scheme, opaque) {
        (ColorScheme::Dark, false) => (
            SIDEBAR_ISLAND_DARK,
            SIDEBAR_ISLAND_RIM_DARK,
            SIDEBAR_ISLAND_INNER_DARK,
        ),
        (ColorScheme::Dark, true) => (
            SIDEBAR_ISLAND_DARK_OPAQUE,
            SIDEBAR_ISLAND_RIM_DARK,
            SIDEBAR_ISLAND_INNER_DARK,
        ),
        (ColorScheme::DarkTint, false) => (
            SIDEBAR_ISLAND_DARK_TINT,
            SIDEBAR_ISLAND_RIM_DARK_TINT,
            SIDEBAR_ISLAND_INNER_DARK_TINT,
        ),
        (ColorScheme::DarkTint, true) => (
            SIDEBAR_ISLAND_DARK_TINT_OPAQUE,
            SIDEBAR_ISLAND_RIM_DARK_TINT,
            SIDEBAR_ISLAND_INNER_DARK_TINT,
        ),
        (_, false) => (
            SIDEBAR_ISLAND_LIGHT,
            SIDEBAR_ISLAND_RIM_LIGHT,
            SIDEBAR_ISLAND_INNER_LIGHT,
        ),
        (_, true) => (
            SIDEBAR_ISLAND_LIGHT_OPAQUE,
            SIDEBAR_ISLAND_RIM_LIGHT,
            SIDEBAR_ISLAND_INNER_LIGHT,
        ),
    };
    SidebarSurface {
        style,
        bg,
        rim,
        inner,
        palette,
    }
}

/// Resolves the kit toolbar glass capsule surface for a concrete theme.
fn toolgroup_surface(
    scheme: ColorScheme,
    opaque: bool,
    palette: ToolgroupPalette,
) -> ToolgroupSurface {
    let bg = match (scheme, opaque) {
        (ColorScheme::Dark, false) => TOOLGROUP_BG_DARK,
        (ColorScheme::Dark, true) => TOOLGROUP_BG_DARK_OPAQUE,
        (ColorScheme::DarkTint, false) => TOOLGROUP_BG_DARK_TINT,
        (ColorScheme::DarkTint, true) => TOOLGROUP_BG_DARK_TINT_OPAQUE,
        (_, false) => TOOLGROUP_BG_LIGHT,
        (_, true) => TOOLGROUP_BG_LIGHT_OPAQUE,
    };
    ToolgroupSurface { bg, palette }
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
            WindowColorMode::Neutral => TrafficLightStyle::Standard,
            WindowColorMode::Accent => TrafficLightStyle::Accent,
            WindowColorMode::Wallpaper => TrafficLightStyle::Wallpaper,
        },
        style => style,
    };
    let lights = match style {
        TrafficLightStyle::Auto | TrafficLightStyle::Standard => [
            TITLEBAR.buttons.order[0].color,
            TITLEBAR.buttons.order[1].color,
            TITLEBAR.buttons.order[2].color,
        ],
        TrafficLightStyle::Accent => derived_lights(normal_accent),
        TrafficLightStyle::Wallpaper => derived_lights(dominant.unwrap_or(normal_accent)),
    };

    let (mut shell, menu, mut sidebar, mut toolgroup) = chrome_surfaces(
        scheme,
        appearance.sidebar_style,
        appearance.reduce_transparency,
    );
    let surfaces = tinted_surfaces(scheme, window_tint, &base);
    if dark {
        // In dark modes the selected row blends the accent into the sidebar
        // background so it is readable on bg-sunken. The ink stays white.
        sidebar.palette.selected_bg = accent.mix(sidebar.bg, SETTINGS.selected_dark_mix);
        sidebar.palette.selected_ink = base.on_accent;
    }
    if let Some((tint, factor)) = panel_tint {
        shell.bar = shell.bar.mix(tint, factor);
        shell.tile = shell.tile.mix(tint, factor);
    }
    if let Some((tint, factor)) = window_tint {
        sidebar.bg = sidebar.bg.mix(tint, factor * LAYER_FACTOR_BG_SUNKEN);
        toolgroup.bg = toolgroup.bg.mix(tint, factor * LAYER_FACTOR_CONTROL);
    }
    shell.tile_active = accent;

    Resolved {
        scheme,
        accent,
        window_tint,
        window_tint_scope: appearance.window_tint_scope,
        panel_tint,
        lights,
        traffic_style: style,
        reduce_transparency: appearance.reduce_transparency,
        reduced_motion: appearance.reduced_motion,
        shell,
        menu,
        sidebar,
        toolgroup,
        surfaces,
    }
}

/// Resolves the tinted surface stack for a concrete scheme.
fn tinted_surfaces(
    scheme: ColorScheme,
    window_tint: Option<(Rgba, f32)>,
    base: &Palette,
) -> Surfaces {
    let (window, sunken, surface, raised, control, hover) = match scheme {
        ColorScheme::DarkTint => (
            BG_WINDOW_DARK_TINT,
            BG_SUNKEN_DARK_TINT,
            BG_SURFACE_DARK_TINT,
            BG_RAISED_DARK_TINT,
            BG_CONTROL_DARK_TINT,
            BG_HOVER_DARK_TINT,
        ),
        ColorScheme::Dark => (
            BG_WINDOW_DARK,
            BG_SUNKEN_DARK,
            BG_SURFACE_DARK,
            BG_RAISED_DARK,
            BG_CONTROL_DARK,
            BG_HOVER_DARK,
        ),
        _ => (
            BG_WINDOW_LIGHT,
            BG_SUNKEN_LIGHT,
            BG_SURFACE_LIGHT,
            BG_RAISED_LIGHT,
            BG_CONTROL_LIGHT,
            BG_HOVER_LIGHT,
        ),
    };
    let line = base.line;
    let mix = |color: Rgba, factor: f32| {
        window_tint
            .map(|(tint, tint_factor)| color.mix(tint, tint_factor * factor))
            .unwrap_or(color)
    };
    Surfaces {
        window: mix(window, 1.0),
        sunken: mix(sunken, LAYER_FACTOR_BG_SUNKEN),
        surface: mix(surface, LAYER_FACTOR_SURFACE),
        raised: mix(raised, LAYER_FACTOR_SURFACE),
        control: mix(control, LAYER_FACTOR_CONTROL),
        hover: mix(hover, LAYER_FACTOR_CONTROL),
        line: mix(line, LAYER_FACTOR_LINE),
    }
}

fn derived_lights(source: Rgba) -> [Rgba; 3] {
    [
        source.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN),
        source.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN),
        source.lighten(TRAFFIC_ZOOM_LIGHTEN),
    ]
}

/// The preferences directory: `MALAYSIA_CONFIG_DIR`, else `$XDG_CONFIG_HOME/malaysia`.
pub fn config_dir() -> PathBuf {
    std::env::var_os("MALAYSIA_CONFIG_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|home| home.join(".config"))
                })
                .unwrap_or_else(|| PathBuf::from(".config"));
            base.join("malaysia")
        },
        PathBuf::from,
    )
}

/// The state/data directory: `MALAYSIA_DATA_DIR`, else `$XDG_DATA_HOME/malaysia`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("MALAYSIA_DATA_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|home| home.join(".local/share"))
                })
                .unwrap_or_else(|| PathBuf::from(".local/share"));
            base.join("malaysia")
        },
        PathBuf::from,
    )
}

/// `~/.config/malaysia/appearance.domain`.
pub fn appearance_path() -> PathBuf {
    config_dir().join("appearance.domain")
}

/// `~/.local/share/malaysia/wallpaper.state`.
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
        Ok(text) => {
            warn_legacy_traffic_light_name(&text);
            toml::from_str(&text).map(Some)
        }
        Err(_) => Ok(None),
    }
}

/// Writes `config` to `path` atomically, preserving unknown keys and comments.
///
/// The file is written to a sibling `.tmp` file and then renamed so readers
/// never see a partial domain. Parent directories are created as needed.
pub fn write_appearance(path: &Path, config: &AppearanceConfig) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let parsed = text.parse::<toml_edit::DocumentMut>();
    if !text.is_empty() && parsed.is_err() {
        let backup = path.with_extension("domain.bak");
        std::fs::copy(path, &backup)?;
        tracing::warn!(
            path = %path.display(),
            backup = %backup.display(),
            "unreadable appearance domain backed up before overwrite"
        );
    }
    let mut doc = parsed.unwrap_or_else(|_| toml_edit::DocumentMut::new());

    doc["color_scheme"] = toml_edit::value(color_scheme_str(config.color_scheme));
    if let Some(accent) = config.accent_color {
        doc["accent_color"] = toml_edit::value(accent_array(accent));
    } else {
        doc.remove("accent_color");
    }
    doc["contrast"] = toml_edit::value(config.contrast);
    doc["reduced_motion"] = toml_edit::value(config.reduced_motion);
    doc["reduce_transparency"] = toml_edit::value(config.reduce_transparency);
    doc["window_color"] = toml_edit::value(window_color_str(config.window_color));
    doc["window_tint_scope"] = toml_edit::value(window_tint_scope_str(config.window_tint_scope));
    doc["traffic_lights"] = toml_edit::value(traffic_lights_str(config.traffic_lights));
    doc["sidebar_style"] = toml_edit::value(sidebar_style_str(config.sidebar_style));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, doc.to_string())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Writes the default appearance config to [`appearance_path()`].
pub fn write_appearance_default() -> std::io::Result<()> {
    write_appearance(&appearance_path(), &AppearanceConfig::default())
}

fn color_scheme_str(scheme: ColorScheme) -> &'static str {
    match scheme {
        ColorScheme::Auto => "auto",
        ColorScheme::Dark => "dark",
        ColorScheme::DarkTint => "dark_tint",
        ColorScheme::Light => "light",
    }
}

fn window_color_str(mode: WindowColorMode) -> &'static str {
    match mode {
        WindowColorMode::Neutral => "neutral",
        WindowColorMode::Accent => "accent",
        WindowColorMode::Wallpaper => "wallpaper",
    }
}

fn window_tint_scope_str(scope: WindowTintScope) -> &'static str {
    match scope {
        WindowTintScope::Windows => "windows",
        WindowTintScope::Desktop => "desktop",
    }
}

fn traffic_lights_str(style: TrafficLightStyle) -> &'static str {
    match style {
        TrafficLightStyle::Auto => "auto",
        TrafficLightStyle::Standard => "standard",
        TrafficLightStyle::Accent => "accent",
        TrafficLightStyle::Wallpaper => "wallpaper",
    }
}

fn sidebar_style_str(style: SidebarStyle) -> &'static str {
    match style {
        SidebarStyle::Island => "island",
        SidebarStyle::Flat => "flat",
    }
}

fn accent_array(accent: [f32; 3]) -> toml_edit::Array {
    let mut arr = toml_edit::Array::new();
    for channel in accent {
        let rounded = (channel.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
        arr.push(rounded as f64);
    }
    arr
}

/// Warns once when an existing `appearance.domain` still uses the legacy name.
fn warn_legacy_traffic_light_name(text: &str) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    let legacy = text.lines().any(|line| {
        let line = line.split('#').next().unwrap_or(line).trim();
        line.starts_with("traffic_lights")
            && line
                .split_once('=')
                .map(|(_, value)| value.trim().trim_matches('"') == "macos")
                .unwrap_or(false)
    });
    if legacy {
        WARNED.call_once(|| {
            tracing::warn!(
                "traffic_lights = \"macos\" is deprecated; the style is named \"standard\""
            );
        });
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
    use vow_design::{
        PANEL_TINT_WALLPAPER, TRAFFIC_CLOSE_MINIMIZE_DARKEN, TRAFFIC_ZOOM_LIGHTEN,
        WINDOW_TINT_ACCENT, WINDOW_TINT_WALLPAPER,
    };

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
            reduce_transparency: true,
            window_color: mode,
            window_tint_scope: scope,
            traffic_lights: lights,
            sidebar_style: SidebarStyle::Island,
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
        let with_dominant = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Accent,
                WindowTintScope::Windows,
                TrafficLightStyle::Wallpaper,
                Some(accent),
            ),
            Some(dominant()),
        );
        assert_eq!(
            with_dominant.lights[0],
            dominant().darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN)
        );
        assert_eq!(
            with_dominant.lights[2],
            dominant().lighten(TRAFFIC_ZOOM_LIGHTEN)
        );

        let without_dominant = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Accent,
                WindowTintScope::Windows,
                TrafficLightStyle::Wallpaper,
                Some(accent),
            ),
            None,
        );
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
    fn the_legacy_style_name_deserializes_to_standard() {
        let config: AppearanceConfig = toml::from_str("traffic_lights = \"macos\"\n").unwrap();
        assert_eq!(config.traffic_lights, TrafficLightStyle::Standard);
        let config: AppearanceConfig = toml::from_str("traffic_lights = \"standard\"\n").unwrap();
        assert_eq!(config.traffic_lights, TrafficLightStyle::Standard);
    }

    #[test]
    fn reduce_transparency_defaults_to_true() {
        let without_key: AppearanceConfig = toml::from_str(
            r#"
color_scheme = "dark"
accent_color = [0.2, 0.5, 0.8]
contrast = false
reduced_motion = false
window_color = "neutral"
window_tint_scope = "windows"
traffic_lights = "auto"
"#,
        )
        .unwrap();
        assert!(without_key.reduce_transparency);

        let with_false: AppearanceConfig = toml::from_str("reduce_transparency = false\n").unwrap();
        assert!(!with_false.reduce_transparency);
    }

    #[test]
    fn reduced_transparency_makes_shell_menu_and_sidebar_opaque() {
        for scheme in [ColorScheme::Light, ColorScheme::Dark, ColorScheme::DarkTint] {
            let resolved = resolve(
                &appearance(
                    scheme,
                    WindowColorMode::Neutral,
                    WindowTintScope::Windows,
                    TrafficLightStyle::Auto,
                    None,
                ),
                Some(dominant()),
            );
            assert!(resolved.reduce_transparency);
            for (name, color) in [
                ("shell bar", resolved.shell.bar),
                ("menu panel", resolved.menu.bg),
                ("sidebar", resolved.sidebar.bg),
            ] {
                assert_eq!(color.a, 1.0, "{scheme:?} {name} is still translucent");
            }
        }
    }

    #[test]
    fn transparency_keeps_the_translucent_bar_when_disabled() {
        let mut config = appearance(
            ColorScheme::Light,
            WindowColorMode::Neutral,
            WindowTintScope::Windows,
            TrafficLightStyle::Auto,
            None,
        );
        config.reduce_transparency = false;
        let resolved = resolve(&config, None);
        assert!(!resolved.reduce_transparency);
        assert!(resolved.shell.bar.a < 1.0);
    }

    #[test]
    fn traffic_light_styles_are_resolved() {
        let standard = resolve(
            &appearance(
                ColorScheme::Light,
                WindowColorMode::Accent,
                WindowTintScope::Windows,
                TrafficLightStyle::Standard,
                None,
            ),
            None,
        );
        assert_eq!(
            standard.lights,
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

    #[test]
    fn sidebar_style_defaults_to_island() {
        let config: AppearanceConfig = toml::from_str("color_scheme = \"light\"\n").unwrap();
        assert_eq!(config.sidebar_style, SidebarStyle::Island);
    }

    #[test]
    fn flat_sidebar_style_deserializes() {
        let config: AppearanceConfig = toml::from_str(
            r#"
color_scheme = "light"
sidebar_style = "flat"
"#,
        )
        .unwrap();
        assert_eq!(config.sidebar_style, SidebarStyle::Flat);
    }

    #[test]
    fn sidebar_style_chooses_the_right_background_per_scheme_and_transparency() {
        let check = |scheme, style, opaque, expected: Rgba| {
            let mut config = appearance(
                scheme,
                WindowColorMode::Neutral,
                WindowTintScope::Windows,
                TrafficLightStyle::Auto,
                None,
            );
            config.sidebar_style = style;
            config.reduce_transparency = opaque;
            let resolved = resolve(&config, None);
            assert_eq!(resolved.sidebar.style, style);
            assert_eq!(resolved.sidebar.bg, expected);
            if opaque {
                assert_eq!(
                    resolved.sidebar.bg.a, 1.0,
                    "{scheme:?} {style:?} must be opaque"
                );
            }
        };

        check(
            ColorScheme::Light,
            SidebarStyle::Island,
            true,
            SIDEBAR_ISLAND_LIGHT_OPAQUE,
        );
        check(
            ColorScheme::Light,
            SidebarStyle::Flat,
            true,
            SIDEBAR_LIGHT_OPAQUE,
        );
        check(
            ColorScheme::Dark,
            SidebarStyle::Island,
            true,
            SIDEBAR_ISLAND_DARK_OPAQUE,
        );
        check(
            ColorScheme::Dark,
            SidebarStyle::Flat,
            true,
            SIDEBAR_DARK_OPAQUE,
        );
        check(
            ColorScheme::DarkTint,
            SidebarStyle::Island,
            true,
            SIDEBAR_ISLAND_DARK_TINT_OPAQUE,
        );
        check(
            ColorScheme::DarkTint,
            SidebarStyle::Flat,
            true,
            SIDEBAR_DARK_TINT_OPAQUE,
        );

        check(
            ColorScheme::Light,
            SidebarStyle::Island,
            false,
            SIDEBAR_ISLAND_LIGHT,
        );
        check(ColorScheme::Light, SidebarStyle::Flat, false, SIDEBAR_LIGHT);
    }

    #[test]
    fn flat_neutral_matches_the_legacy_sidebar_colour() {
        for scheme in [ColorScheme::Light, ColorScheme::Dark, ColorScheme::DarkTint] {
            for opaque in [false, true] {
                let mut config = appearance(
                    scheme,
                    WindowColorMode::Neutral,
                    WindowTintScope::Windows,
                    TrafficLightStyle::Auto,
                    None,
                );
                config.sidebar_style = SidebarStyle::Flat;
                config.reduce_transparency = opaque;
                let resolved = resolve(&config, None);
                let legacy = match (scheme, opaque) {
                    (ColorScheme::Dark, false) => SIDEBAR_DARK,
                    (ColorScheme::Dark, true) => SIDEBAR_DARK_OPAQUE,
                    (ColorScheme::DarkTint, false) => SIDEBAR_DARK_TINT,
                    (ColorScheme::DarkTint, true) => SIDEBAR_DARK_TINT_OPAQUE,
                    (_, false) => SIDEBAR_LIGHT,
                    (_, true) => SIDEBAR_LIGHT_OPAQUE,
                };
                assert_eq!(
                    resolved.sidebar.bg, legacy,
                    "{scheme:?} opaque={opaque} flat sidebar must match legacy value"
                );
                assert_eq!(resolved.sidebar.rim.a, 0.0);
                assert_eq!(resolved.sidebar.inner.a, 0.0);
            }
        }
    }

    #[test]
    fn accent_and_wallpaper_tint_sidebar_and_toolgroup_but_neutral_does_not() {
        let accent = Rgba {
            r: 0.9,
            g: 0.3,
            b: 0.1,
            a: 1.0,
        };
        let mut neutral = appearance(
            ColorScheme::Light,
            WindowColorMode::Neutral,
            WindowTintScope::Windows,
            TrafficLightStyle::Auto,
            Some([accent.r, accent.g, accent.b]),
        );
        neutral.reduce_transparency = true;
        let neutral = resolve(&neutral, None);

        let mut accent_mode = appearance(
            ColorScheme::Light,
            WindowColorMode::Accent,
            WindowTintScope::Windows,
            TrafficLightStyle::Auto,
            Some([accent.r, accent.g, accent.b]),
        );
        accent_mode.reduce_transparency = true;
        let accent_resolved = resolve(&accent_mode, None);

        let expected_factor = WINDOW_TINT_ACCENT[0] * LAYER_FACTOR_BG_SUNKEN;
        let expected_sidebar_bg = neutral.sidebar.bg.mix(accent, expected_factor);
        assert_eq!(accent_resolved.sidebar.bg, expected_sidebar_bg);

        let expected_toolgroup_factor = WINDOW_TINT_ACCENT[0] * LAYER_FACTOR_CONTROL;
        let expected_toolgroup_bg = neutral.toolgroup.bg.mix(accent, expected_toolgroup_factor);
        assert_eq!(accent_resolved.toolgroup.bg, expected_toolgroup_bg);

        let mut wallpaper = accent_mode.clone();
        wallpaper.window_color = WindowColorMode::Wallpaper;
        let wallpaper_resolved = resolve(&wallpaper, Some(dominant()));
        let tint = dominant();
        let wallpaper_factor = WINDOW_TINT_WALLPAPER[0] * LAYER_FACTOR_BG_SUNKEN;
        assert_eq!(
            wallpaper_resolved.sidebar.bg,
            neutral.sidebar.bg.mix(tint, wallpaper_factor)
        );

        assert_eq!(neutral.sidebar.bg, SIDEBAR_ISLAND_LIGHT_OPAQUE);
        assert_eq!(neutral.toolgroup.bg, TOOLGROUP_BG_LIGHT_OPAQUE);
    }

    #[test]
    fn neutral_surfaces_match_the_base_colours() {
        for scheme in [ColorScheme::Light, ColorScheme::Dark, ColorScheme::DarkTint] {
            let resolved = resolve(
                &appearance(
                    scheme,
                    WindowColorMode::Neutral,
                    WindowTintScope::Windows,
                    TrafficLightStyle::Auto,
                    None,
                ),
                None,
            );
            let expected = match scheme {
                ColorScheme::DarkTint => (
                    BG_WINDOW_DARK_TINT,
                    BG_SUNKEN_DARK_TINT,
                    BG_SURFACE_DARK_TINT,
                    BG_RAISED_DARK_TINT,
                    BG_CONTROL_DARK_TINT,
                    BG_HOVER_DARK_TINT,
                ),
                ColorScheme::Dark => (
                    BG_WINDOW_DARK,
                    BG_SUNKEN_DARK,
                    BG_SURFACE_DARK,
                    BG_RAISED_DARK,
                    BG_CONTROL_DARK,
                    BG_HOVER_DARK,
                ),
                _ => (
                    BG_WINDOW_LIGHT,
                    BG_SUNKEN_LIGHT,
                    BG_SURFACE_LIGHT,
                    BG_RAISED_LIGHT,
                    BG_CONTROL_LIGHT,
                    BG_HOVER_LIGHT,
                ),
            };
            assert_eq!(resolved.surfaces.window, expected.0, "{scheme:?} window");
            assert_eq!(resolved.surfaces.sunken, expected.1, "{scheme:?} sunken");
            assert_eq!(resolved.surfaces.surface, expected.2, "{scheme:?} surface");
            assert_eq!(resolved.surfaces.raised, expected.3, "{scheme:?} raised");
            assert_eq!(resolved.surfaces.control, expected.4, "{scheme:?} control");
            assert_eq!(resolved.surfaces.hover, expected.5, "{scheme:?} hover");
            assert_eq!(
                resolved.surfaces.line,
                palette(scheme).line,
                "{scheme:?} line"
            );
        }
    }

    #[test]
    fn accent_tints_each_surface_by_its_layer_factor() {
        let purple = Rgba {
            r: 0.557,
            g: 0.357,
            b: 0.847,
            a: 1.0,
        };
        let resolved = resolve(
            &appearance(
                ColorScheme::Dark,
                WindowColorMode::Accent,
                WindowTintScope::Windows,
                TrafficLightStyle::Auto,
                Some([purple.r, purple.g, purple.b]),
            ),
            None,
        );
        let factor = WINDOW_TINT_ACCENT[1];
        assert_eq!(resolved.surfaces.window, BG_WINDOW_DARK.mix(purple, factor));
        assert_eq!(
            resolved.surfaces.sunken,
            BG_SUNKEN_DARK.mix(purple, factor * LAYER_FACTOR_BG_SUNKEN)
        );
        assert_eq!(
            resolved.surfaces.surface,
            BG_SURFACE_DARK.mix(purple, factor * LAYER_FACTOR_SURFACE)
        );
        assert_eq!(
            resolved.surfaces.raised,
            BG_RAISED_DARK.mix(purple, factor * LAYER_FACTOR_SURFACE)
        );
        assert_eq!(
            resolved.surfaces.control,
            BG_CONTROL_DARK.mix(purple, factor * LAYER_FACTOR_CONTROL)
        );
        assert_eq!(
            resolved.surfaces.hover,
            BG_HOVER_DARK.mix(purple, factor * LAYER_FACTOR_CONTROL)
        );
        assert_eq!(
            resolved.surfaces.line,
            palette(ColorScheme::Dark)
                .line
                .mix(purple, factor * LAYER_FACTOR_LINE)
        );
    }

    #[test]
    fn sidebar_and_toolgroup_palettes_come_from_the_theme() {
        for (scheme, expected_sidebar, expected_toolgroup) in [
            (
                ColorScheme::Light,
                SIDEBAR_PALETTE_LIGHT,
                TOOLGROUP_PALETTE_LIGHT,
            ),
            (
                ColorScheme::Dark,
                SIDEBAR_PALETTE_DARK,
                TOOLGROUP_PALETTE_DARK,
            ),
            (
                ColorScheme::DarkTint,
                SIDEBAR_PALETTE_DARK_TINT,
                TOOLGROUP_PALETTE_DARK_TINT,
            ),
        ] {
            let resolved = resolve(
                &appearance(
                    scheme,
                    WindowColorMode::Neutral,
                    WindowTintScope::Windows,
                    TrafficLightStyle::Auto,
                    None,
                ),
                None,
            );
            assert_eq!(
                resolved.toolgroup.palette, expected_toolgroup,
                "{scheme:?} toolgroup palette"
            );

            if matches!(scheme, ColorScheme::Dark | ColorScheme::DarkTint) {
                // Dark modes blend the accent into the sidebar background for
                // selected rows instead of using the light-mode accent-soft.
                let accent = resolved.accent;
                assert_eq!(
                    resolved.sidebar.palette.selected_bg,
                    accent.mix(resolved.sidebar.bg, SETTINGS.selected_dark_mix),
                    "{scheme:?} selected_bg"
                );
                assert_eq!(
                    resolved.sidebar.palette.selected_ink,
                    palette(scheme).on_accent,
                    "{scheme:?} selected_ink"
                );
            } else {
                assert_eq!(resolved.sidebar.palette, expected_sidebar);
            }
        }
    }

    #[test]
    fn reduced_motion_is_passed_through() {
        let mut config = appearance(
            ColorScheme::Light,
            WindowColorMode::Neutral,
            WindowTintScope::Windows,
            TrafficLightStyle::Auto,
            None,
        );
        config.reduced_motion = true;
        let resolved = resolve(&config, None);
        assert!(resolved.reduced_motion);
    }

    #[test]
    fn dark_selected_row_mixes_accent_with_sidebar_bg() {
        let check = |scheme, accent: [f32; 3]| {
            let mut config = appearance(
                scheme,
                WindowColorMode::Neutral,
                WindowTintScope::Windows,
                TrafficLightStyle::Auto,
                Some(accent),
            );
            config.reduce_transparency = true;
            let resolved = resolve(&config, None);
            let accent_rgba = Rgba {
                r: accent[0],
                g: accent[1],
                b: accent[2],
                a: 1.0,
            };
            assert_eq!(
                resolved.sidebar.palette.selected_bg,
                accent_rgba.mix(resolved.sidebar.bg, SETTINGS.selected_dark_mix),
                "{scheme:?} selected_bg should mix the accent into the sidebar background"
            );
            assert_eq!(
                resolved.sidebar.palette.selected_ink,
                palette(scheme).on_accent,
                "{scheme:?} selected_ink should be on-accent"
            );
        };
        check(ColorScheme::Dark, [0.184, 0.435, 0.894]); // default Azul
        check(ColorScheme::DarkTint, [0.557, 0.357, 0.847]); // Morado
        check(ColorScheme::Dark, [0.898, 0.271, 0.561]); // Rosa
    }

    fn assert_configs_equal(a: &AppearanceConfig, b: &AppearanceConfig) {
        assert_eq!(a.color_scheme, b.color_scheme);
        assert_eq!(a.accent_color, b.accent_color);
        assert_eq!(a.contrast, b.contrast);
        assert_eq!(a.reduced_motion, b.reduced_motion);
        assert_eq!(a.reduce_transparency, b.reduce_transparency);
        assert_eq!(a.window_color, b.window_color);
        assert_eq!(a.window_tint_scope, b.window_tint_scope);
        assert_eq!(a.traffic_lights, b.traffic_lights);
        assert_eq!(a.sidebar_style, b.sidebar_style);
    }

    #[test]
    fn write_appearance_roundtrips() {
        let dir = std::env::temp_dir().join(format!("vow-appearance-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("appearance.domain");
        let config = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            accent_color: Some([0.184, 0.435, 0.894]),
            contrast: false,
            reduced_motion: false,
            reduce_transparency: true,
            window_color: WindowColorMode::Accent,
            window_tint_scope: WindowTintScope::Desktop,
            traffic_lights: TrafficLightStyle::Accent,
            sidebar_style: SidebarStyle::Flat,
        };
        write_appearance(&path, &config).unwrap();
        let read = read_appearance_from(&path).unwrap().unwrap();
        assert_configs_equal(&config, &read);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_appearance_preserves_unknown_keys_and_comments() {
        let dir =
            std::env::temp_dir().join(format!("vow-appearance-preserve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("appearance.domain");
        std::fs::write(
            &path,
            "# legacy comment preserved\nlegacy_key = true\ncolor_scheme = \"light\"\n",
        )
        .unwrap();
        let config = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            ..AppearanceConfig::default()
        };
        write_appearance(&path, &config).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("legacy comment preserved"));
        assert!(text.contains("legacy_key = true"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_appearance_backs_up_an_unreadable_domain() {
        let dir =
            std::env::temp_dir().join(format!("vow-appearance-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("appearance.domain");
        let original = "color_scheme = ";
        std::fs::write(&path, original).unwrap();
        let config = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            ..AppearanceConfig::default()
        };

        write_appearance(&path, &config).unwrap();

        let backup = dir.join("appearance.domain.bak");
        assert_eq!(std::fs::read(&backup).unwrap(), original.as_bytes());
        let read = read_appearance_from(&path).unwrap().unwrap();
        assert_eq!(read.color_scheme, ColorScheme::Dark);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_appearance_does_not_back_up_a_readable_domain() {
        let dir =
            std::env::temp_dir().join(format!("vow-appearance-nobackup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("appearance.domain");
        std::fs::write(&path, "color_scheme = \"light\"\n").unwrap();

        write_appearance(&path, &AppearanceConfig::default()).unwrap();

        assert!(!dir.join("appearance.domain.bak").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_appearance_creates_missing_file_and_leaves_no_temp() {
        let dir =
            std::env::temp_dir().join(format!("vow-appearance-create-{}", std::process::id()));
        let path = dir.join("appearance.domain");
        assert!(!path.exists());
        write_appearance(&path, &AppearanceConfig::default()).unwrap();
        assert!(path.exists());
        assert!(!dir.join("appearance.tmp").exists());
        read_appearance_from(&path)
            .unwrap()
            .expect("created file parses");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_appearance_default_uses_config_dir() {
        let _env = ENV_LOCK.lock().unwrap();
        let dir =
            std::env::temp_dir().join(format!("vow-appearance-default-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = std::env::var_os("MALAYSIA_CONFIG_DIR");
        unsafe { std::env::set_var("MALAYSIA_CONFIG_DIR", &dir) };
        write_appearance_default().unwrap();
        let path = dir.join("appearance.domain");
        assert!(path.exists());
        let read = read_appearance_from(&path).unwrap().unwrap();
        assert_configs_equal(&AppearanceConfig::default(), &read);
        match old {
            Some(v) => unsafe { std::env::set_var("MALAYSIA_CONFIG_DIR", v) },
            None => unsafe { std::env::remove_var("MALAYSIA_CONFIG_DIR") },
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_and_data_dirs_use_the_documented_malaysia_directory() {
        let _env = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("vow-appearance-dirs-{}", std::process::id()));
        let config_home = dir.join("config");
        let data_home = dir.join("data");
        let old_config = std::env::var_os("MALAYSIA_CONFIG_DIR");
        let old_data = std::env::var_os("MALAYSIA_DATA_DIR");
        let old_xdg_config = std::env::var_os("XDG_CONFIG_HOME");
        let old_xdg_data = std::env::var_os("XDG_DATA_HOME");
        let restore = |name: &str, value: Option<std::ffi::OsString>| unsafe {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        };

        unsafe {
            std::env::remove_var("MALAYSIA_CONFIG_DIR");
            std::env::remove_var("MALAYSIA_DATA_DIR");
            std::env::set_var("XDG_CONFIG_HOME", &config_home);
            std::env::set_var("XDG_DATA_HOME", &data_home);
        }
        assert_eq!(config_dir(), config_home.join("malaysia"));
        assert_eq!(data_dir(), data_home.join("malaysia"));

        restore("MALAYSIA_CONFIG_DIR", old_config);
        restore("MALAYSIA_DATA_DIR", old_data);
        restore("XDG_CONFIG_HOME", old_xdg_config);
        restore("XDG_DATA_HOME", old_xdg_data);
    }
}
