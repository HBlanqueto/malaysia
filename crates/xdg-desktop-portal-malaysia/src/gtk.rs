//! Bridge from portal appearance settings to GTK 3/4 and libadwaita.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use vow_appearance::backdrop::Oklab;
use vow_appearance::traffic::{ButtonVisual, TrafficInput, visual};
use vow_appearance::{
    AppearanceConfig, ColorScheme, Resolved, TrafficLightShape, TrafficLightStyle, palette, resolve,
};
use vow_design::{
    ButtonKind, LAYER_FACTOR_BG_SUNKEN, LAYER_FACTOR_RAISED, LAYER_FACTOR_SURFACE, Rgba, TITLEBAR,
    TRAFFIC_GLYPHS,
};

const BEGIN: &str = "/* malaysia:gtk-bridge begin (xdg-desktop-portal-malaysia, ADR 0050) */";
const END: &str = "/* malaysia:gtk-bridge end */";
const GENERATED: &str = "malaysia.css";
const PREFER_DARK: &str = "gtk-application-prefer-dark-theme";

#[derive(Debug, Clone, Copy)]
struct AdwaitaBase {
    window: Rgba,
    view: Rgba,
    headerbar: Rgba,
    headerbar_backdrop: Rgba,
    sidebar: Rgba,
    sidebar_backdrop: Rgba,
    secondary_sidebar: Rgba,
    secondary_sidebar_backdrop: Rgba,
    card: Option<Rgba>,
    dialog: Rgba,
    popover: Rgba,
}

const ADWAITA_LIGHT: AdwaitaBase = AdwaitaBase {
    window: Rgba::rgb8(0xfa, 0xfa, 0xfb),
    view: Rgba::rgb8(0xff, 0xff, 0xff),
    headerbar: Rgba::rgb8(0xff, 0xff, 0xff),
    headerbar_backdrop: Rgba::rgb8(0xfa, 0xfa, 0xfb),
    sidebar: Rgba::rgb8(0xeb, 0xeb, 0xed),
    sidebar_backdrop: Rgba::rgb8(0xf2, 0xf2, 0xf4),
    secondary_sidebar: Rgba::rgb8(0xf3, 0xf3, 0xf5),
    secondary_sidebar_backdrop: Rgba::rgb8(0xf6, 0xf6, 0xfa),
    card: Some(Rgba::rgb8(0xff, 0xff, 0xff)),
    dialog: Rgba::rgb8(0xfa, 0xfa, 0xfb),
    popover: Rgba::rgb8(0xff, 0xff, 0xff),
};

const ADWAITA_DARK: AdwaitaBase = AdwaitaBase {
    window: Rgba::rgb8(0x22, 0x22, 0x26),
    view: Rgba::rgb8(0x1d, 0x1d, 0x20),
    headerbar: Rgba::rgb8(0x2e, 0x2e, 0x32),
    headerbar_backdrop: Rgba::rgb8(0x22, 0x22, 0x26),
    sidebar: Rgba::rgb8(0x2e, 0x2e, 0x32),
    sidebar_backdrop: Rgba::rgb8(0x28, 0x28, 0x2c),
    secondary_sidebar: Rgba::rgb8(0x28, 0x28, 0x2c),
    secondary_sidebar_backdrop: Rgba::rgb8(0x25, 0x25, 0x29),
    card: None,
    dialog: Rgba::rgb8(0x36, 0x36, 0x3a),
    popover: Rgba::rgb8(0x36, 0x36, 0x3a),
};

#[derive(Debug, Clone, PartialEq)]
struct SchemeColors {
    dark: bool,
    colors: Vec<(&'static str, Rgba)>,
    traffic: Traffic,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Traffic {
    lights: [Rgba; 3],
    style: TrafficLightStyle,
    shape: TrafficLightShape,
    inactive: Rgba,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Toolkit {
    Gtk3,
    Gtk4,
}

impl SchemeColors {
    fn new(appearance: &AppearanceConfig, dominant: Option<Rgba>, scheme: ColorScheme) -> Self {
        let concrete = AppearanceConfig {
            color_scheme: scheme,
            ..appearance.clone()
        };
        let resolved = resolve(&concrete, dominant);
        let dark = matches!(resolved.scheme, ColorScheme::Dark | ColorScheme::DarkTint);
        let on_accent = palette(resolved.scheme).on_accent;
        let traffic = Traffic {
            lights: resolved.lights,
            style: resolved.traffic_style,
            shape: resolved.traffic_shape,
            inactive: palette(resolved.scheme).traffic_lights_inactive,
        };
        let mut colors = vec![
            ("accent_bg_color", resolved.accent),
            ("accent_fg_color", on_accent),
            ("accent_color", standalone_accent(resolved.accent, dark)),
        ];
        if !appearance.contrast {
            colors.extend(tinted_backgrounds(&resolved, appearance, dark));
        }
        Self {
            dark,
            colors,
            traffic,
        }
    }

    fn get(&self, name: &str) -> Option<Rgba> {
        self.colors
            .iter()
            .find_map(|(key, color)| (*key == name).then_some(*color))
    }
}

fn tinted_backgrounds(
    resolved: &Resolved,
    appearance: &AppearanceConfig,
    dark: bool,
) -> Vec<(&'static str, Rgba)> {
    let Some((tint, factor)) = resolved.window_tint else {
        return Vec::new();
    };
    let base = if dark { ADWAITA_DARK } else { ADWAITA_LIGHT };
    let parts = appearance.tint_parts;
    let mix = |color: Rgba, layer: f32| color.mix(tint, factor * layer);
    let mut colors = Vec::new();
    if parts.window {
        colors.extend([
            ("window_bg_color", mix(base.window, 1.0)),
            ("headerbar_bg_color", mix(base.headerbar, 1.0)),
            (
                "headerbar_backdrop_color",
                mix(base.headerbar_backdrop, 1.0),
            ),
            ("view_bg_color", mix(base.view, LAYER_FACTOR_SURFACE)),
        ]);
    }
    if parts.sidebars {
        colors.extend([
            (
                "sidebar_bg_color",
                mix(base.sidebar, LAYER_FACTOR_BG_SUNKEN),
            ),
            (
                "sidebar_backdrop_color",
                mix(base.sidebar_backdrop, LAYER_FACTOR_BG_SUNKEN),
            ),
            (
                "secondary_sidebar_bg_color",
                mix(base.secondary_sidebar, LAYER_FACTOR_BG_SUNKEN),
            ),
            (
                "secondary_sidebar_backdrop_color",
                mix(base.secondary_sidebar_backdrop, LAYER_FACTOR_BG_SUNKEN),
            ),
        ]);
    }
    if parts.groups
        && let Some(card) = base.card
    {
        colors.push(("card_bg_color", mix(card, LAYER_FACTOR_SURFACE)));
    }
    if parts.menus {
        colors.extend([
            ("dialog_bg_color", mix(base.dialog, LAYER_FACTOR_RAISED)),
            ("popover_bg_color", mix(base.popover, LAYER_FACTOR_RAISED)),
        ]);
    }
    colors
}

const ADWAITA_STANDALONE_MAX_L_LIGHT: f32 = 0.5;
const ADWAITA_STANDALONE_MIN_L_DARK: f32 = 0.85;

fn standalone_accent(accent: Rgba, dark: bool) -> Rgba {
    let mut lab = Oklab::from_rgba(accent);
    lab.l = if dark {
        lab.l.max(ADWAITA_STANDALONE_MIN_L_DARK)
    } else {
        lab.l.min(ADWAITA_STANDALONE_MAX_L_LIGHT)
    };
    Rgba {
        a: accent.a,
        ..lab.to_rgba()
    }
}

fn schemes(appearance: &AppearanceConfig) -> (ColorScheme, ColorScheme) {
    let dark = if appearance.color_scheme == ColorScheme::DarkTint {
        ColorScheme::DarkTint
    } else {
        ColorScheme::Dark
    };
    (ColorScheme::Light, dark)
}

fn css_color(color: Rgba) -> String {
    let [r, g, b, _] = color.to_rgba8();
    if color.a >= 0.999 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r}, {g}, {b}, {:.3})", color.a)
    }
}

fn push_define_colors(out: &mut String, colors: &SchemeColors, indent: &str) {
    for (name, color) in &colors.colors {
        out.push_str(&format!(
            "{indent}@define-color {name} {};\n",
            css_color(*color)
        ));
    }
}

fn push_variables(out: &mut String, colors: &SchemeColors, indent: &str) {
    out.push_str(&format!("{indent}:root {{\n"));
    for (name, color) in &colors.colors {
        if *name == "accent_color" {
            continue;
        }
        let variable = name.replace('_', "-");
        out.push_str(&format!("{indent}  --{variable}: {};\n", css_color(*color)));
    }
    out.push_str(&format!("{indent}}}\n"));
}

const HEADER: &str = "/* Generado por xdg-desktop-portal-malaysia desde appearance.domain\n \
                      * (ADR 0050). Se reescribe en cada cambio: no lo edites; tus reglas\n \
                      * van en gtk.css, debajo del bloque malaysia:gtk-bridge. */\n";

fn push_media_blocks(out: &mut String, light: &SchemeColors, dark: &SchemeColors) {
    for (query, colors) in [("light", light), ("dark", dark)] {
        out.push_str(&format!("\n@media (prefers-color-scheme: {query}) {{\n"));
        push_variables(out, colors, "  ");
        push_define_colors(out, colors, "  ");
        push_window_controls(out, &colors.traffic, Toolkit::Gtk4, "  ");
        out.push_str("}\n");
    }
}

pub fn button_layout() -> String {
    let names: Vec<&str> = TITLEBAR
        .buttons
        .order
        .iter()
        .map(|button| gtk_button_name(button.kind))
        .collect();
    format!("{}:", names.join(","))
}

fn gtk_button_name(kind: ButtonKind) -> &'static str {
    match kind {
        ButtonKind::Close => "close",
        ButtonKind::Minimize => "minimize",
        ButtonKind::Maximize => "maximize",
    }
}

fn traffic_states(traffic: &Traffic, focused: bool, group_hovered: bool) -> [ButtonVisual; 3] {
    visual(
        traffic.lights,
        traffic.style,
        traffic.inactive,
        TrafficInput {
            focused,
            group_hovered,
            pressed: None,
            zoom_supported: true,
        },
    )
}

// libadwaita pads the headerbar box 7 px inside a -1 px margin (_header-bar.scss, 1.9.3).
const ADWAITA_HEADERBAR_START: u32 = 6;

// libadwaita dims the whole headerbar in backdrop; the traffic lights carry their own unfocused colour.
const ADWAITA_BACKDROP_DIM: &str = "filter: opacity(0.5);";

fn push_window_controls(out: &mut String, traffic: &Traffic, toolkit: Toolkit, indent: &str) {
    let buttons = &TITLEBAR.buttons;
    let radius = match traffic.shape {
        TrafficLightShape::Square => buttons.corner_radius,
        TrafficLightShape::Round => buttons.diameter / 2,
    };
    let focused = traffic_states(traffic, true, false);
    let backdrop = traffic_states(traffic, false, false);
    let hovered = traffic_states(traffic, true, true);
    let mut rule = |selector: &str, body: &str| {
        out.push_str(&format!("{indent}{selector} {{ {body} }}\n"));
    };
    if toolkit == Toolkit::Gtk4 {
        rule(
            "windowcontrols",
            &format!("border-spacing: {}px;", buttons.spacing),
        );
        rule(
            "windowcontrols.start:dir(ltr)",
            &format!(
                "margin-left: {}px;",
                buttons.margin_start.saturating_sub(ADWAITA_HEADERBAR_START)
            ),
        );
        rule("headerbar:backdrop > windowhandle", "filter: none;");
        for parent in ["windowhandle > box", "windowhandle > box > widget"] {
            rule(
                &format!("headerbar:backdrop > {parent} > :not(box)"),
                ADWAITA_BACKDROP_DIM,
            );
            rule(
                &format!("headerbar:backdrop > {parent} > box > :not(windowcontrols)"),
                ADWAITA_BACKDROP_DIM,
            );
        }
    }
    for (index, button) in buttons.order.iter().enumerate() {
        let name = gtk_button_name(button.kind);
        let pressed = hovered[index]
            .fill
            .darken(vow_design::TRAFFIC_PRESSED_DARKEN);
        let glyph = hovered[index]
            .glyph
            .map_or("transparent".to_string(), css_color);
        let (button, light, hover) = match toolkit {
            Toolkit::Gtk4 => (
                format!("windowcontrols > button.{name}"),
                format!("windowcontrols > button.{name} > image"),
                format!("windowcontrols:hover > button.{name} > image"),
            ),
            Toolkit::Gtk3 => (
                format!("button.titlebutton.{name}"),
                format!("button.titlebutton.{name}"),
                format!("button.titlebutton.{name}:hover"),
            ),
        };
        let shape = format!(
            "border: none; border-radius: {radius}px; box-shadow: none; background: {};",
            css_color(focused[index].fill)
        );
        match toolkit {
            Toolkit::Gtk4 => {
                let padding = (buttons.diameter as f32 - TRAFFIC_GLYPHS.box_size) / 2.0;
                rule(
                    &button,
                    "min-width: 0; min-height: 0; padding: 0; margin: 0; border: none; \
                     box-shadow: none; background: none;",
                );
                rule(
                    &light,
                    &format!(
                        "min-width: 0; min-height: 0; padding: {padding}px; \
                         -gtk-icon-size: {}px; color: transparent; {shape}",
                        TRAFFIC_GLYPHS.box_size
                    ),
                );
            }
            Toolkit::Gtk3 => {
                let margin = if index > 0 { buttons.spacing } else { 0 };
                rule(
                    &button,
                    &format!(
                        "min-width: {d}px; min-height: {d}px; padding: 0; \
                         margin: 0 0 0 {margin}px; {shape}",
                        d = buttons.diameter
                    ),
                );
                rule(&format!("{button} image"), "opacity: 0;");
                rule(&format!("{hover} image"), "opacity: 1;");
            }
        }
        let active = match toolkit {
            Toolkit::Gtk4 => format!("windowcontrols > button.{name}:active > image"),
            Toolkit::Gtk3 => format!("{button}:active"),
        };
        let backdrop_light = match toolkit {
            Toolkit::Gtk4 => format!("windowcontrols > button.{name}:backdrop > image"),
            Toolkit::Gtk3 => format!("{button}:backdrop"),
        };
        rule(
            &backdrop_light,
            &format!("background: {};", css_color(backdrop[index].fill)),
        );
        rule(
            &hover,
            &format!(
                "background: {}; color: {glyph};",
                css_color(hovered[index].fill)
            ),
        );
        rule(&active, &format!("background: {};", css_color(pressed)));
    }
}

fn gtk4_css(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> String {
    let (light, dark) = schemes(appearance);
    let light = SchemeColors::new(appearance, dominant, light);
    let dark = SchemeColors::new(appearance, dominant, dark);
    let current = if matches!(
        appearance.color_scheme.resolved(),
        ColorScheme::Dark | ColorScheme::DarkTint
    ) {
        &dark
    } else {
        &light
    };
    let mut out = String::from(HEADER);
    out.push('\n');
    push_define_colors(&mut out, current, "");
    push_media_blocks(&mut out, &light, &dark);
    out
}

pub fn live_css(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> String {
    let (light, dark) = schemes(appearance);
    let light = SchemeColors::new(appearance, dominant, light);
    let dark = SchemeColors::new(appearance, dominant, dark);
    let mut out = String::new();
    push_media_blocks(&mut out, &light, &dark);
    out
}

fn gtk3_css(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> String {
    let (light, dark) = schemes(appearance);
    let scheme = match appearance.color_scheme.resolved() {
        ColorScheme::Dark | ColorScheme::DarkTint => dark,
        _ => light,
    };
    let colors = SchemeColors::new(appearance, dominant, scheme);
    let mut out = String::from(HEADER);
    out.push('\n');
    push_define_colors(&mut out, &colors, "");
    let mut alias = |name: &str, source: &str| {
        if let Some(color) = colors.get(source) {
            out.push_str(&format!("@define-color {name} {};\n", css_color(color)));
        }
    };
    alias("theme_selected_bg_color", "accent_bg_color");
    alias("theme_selected_fg_color", "accent_fg_color");
    alias("theme_bg_color", "window_bg_color");
    alias("theme_base_color", "view_bg_color");
    push_window_controls(&mut out, &colors.traffic, Toolkit::Gtk3, "");
    out
}

fn strip_block(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut inside = false;
    for line in css.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed == BEGIN {
            inside = true;
            continue;
        }
        if inside {
            if trimmed == END {
                inside = false;
            }
            continue;
        }
        out.push_str(line);
    }
    out
}

fn install_block(css: &str) -> String {
    let rest = strip_block(css);
    let rest = rest.trim_start_matches('\n');
    let mut out = format!("{BEGIN}\n@import url(\"{GENERATED}\");\n{END}\n");
    if !rest.is_empty() {
        out.push('\n');
        out.push_str(rest);
    }
    out
}

fn set_ini_key(ini: &str, key: &str, value: &str) -> String {
    let entry = format!("{key}={value}");
    let mut out = Vec::new();
    let mut in_settings = false;
    let mut seen_settings = false;
    let mut written = false;
    for line in ini.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_settings && !written {
                out.push(entry.clone());
                written = true;
            }
            in_settings = trimmed == "[Settings]";
            seen_settings |= in_settings;
            out.push(line.to_string());
            continue;
        }
        if in_settings
            && let Some((name, _)) = trimmed.split_once('=')
            && name.trim() == key
        {
            if !written {
                out.push(entry.clone());
                written = true;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if !written {
        if !seen_settings {
            if out.last().is_some_and(|line| !line.trim().is_empty()) {
                out.push(String::new());
            }
            out.push("[Settings]".to_string());
        }
        out.push(entry);
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn write_if_changed(path: &Path, contents: &str) -> io::Result<()> {
    if fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("malaysia-tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct BridgeOptions {
    gtk_bridge: bool,
}

impl Default for BridgeOptions {
    fn default() -> Self {
        Self { gtk_bridge: true }
    }
}

pub fn enabled() -> bool {
    let path = vow_appearance::appearance_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return true;
    };
    match toml::from_str::<BridgeOptions>(&text) {
        Ok(options) => options.gtk_bridge,
        Err(_) => true,
    }
}

pub struct Bridge {
    config_home: PathBuf,
}

impl Bridge {
    pub fn from_env() -> Option<Self> {
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(Self { config_home })
    }

    #[cfg(test)]
    fn at(config_home: PathBuf) -> Self {
        Self { config_home }
    }

    fn dir(&self, version: &str) -> PathBuf {
        self.config_home.join(version)
    }

    pub fn apply(&self, appearance: &AppearanceConfig, dominant: Option<Rgba>) -> io::Result<()> {
        let dark = matches!(
            appearance.color_scheme.resolved(),
            ColorScheme::Dark | ColorScheme::DarkTint
        );
        for (version, css) in [
            ("gtk-4.0", gtk4_css(appearance, dominant)),
            ("gtk-3.0", gtk3_css(appearance, dominant)),
        ] {
            let dir = self.dir(version);
            let main = dir.join("gtk.css");
            if is_symlink(&main) {
                tracing::warn!(path = %main.display(), "gtk.css is a symlink; the GTK bridge leaves it alone");
                continue;
            }
            write_if_changed(&dir.join(GENERATED), &css)?;
            let current = fs::read_to_string(&main).unwrap_or_default();
            write_if_changed(&main, &install_block(&current))?;
        }
        let ini = self.dir("gtk-3.0").join("settings.ini");
        if is_symlink(&ini) {
            tracing::warn!(path = %ini.display(), "settings.ini is a symlink; GTK 3 dark mode not set");
        } else {
            let current = fs::read_to_string(&ini).unwrap_or_default();
            let value = if dark { "true" } else { "false" };
            write_if_changed(&ini, &set_ini_key(&current, PREFER_DARK, value))?;
        }
        Ok(())
    }

    pub fn remove(&self) -> io::Result<()> {
        for version in ["gtk-4.0", "gtk-3.0"] {
            let dir = self.dir(version);
            let main = dir.join("gtk.css");
            if !is_symlink(&main)
                && let Ok(current) = fs::read_to_string(&main)
            {
                let stripped = strip_block(&current);
                if stripped != current {
                    if stripped.trim().is_empty() {
                        fs::remove_file(&main)?;
                    } else {
                        write_if_changed(&main, stripped.trim_start_matches('\n'))?;
                    }
                }
            }
            match fs::remove_file(dir.join(GENERATED)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }

    pub fn sync(&self, appearance: Option<&AppearanceConfig>, dominant: Option<Rgba>) {
        let result = if enabled() {
            let default = AppearanceConfig::default();
            self.apply(appearance.unwrap_or(&default), dominant)
        } else {
            self.remove()
        };
        if let Err(error) = result {
            tracing::warn!(%error, "GTK bridge could not update the GTK user CSS");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vow_appearance::WindowColorMode;

    fn temp_home(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("malaysia-gtk-bridge-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_neutral_desktop_only_overrides_the_accent() {
        let css = gtk4_css(&AppearanceConfig::default(), None);
        assert!(css.contains("@define-color accent_bg_color"));
        assert!(css.contains("--accent-bg-color"));
        assert!(!css.contains("window_bg_color"));
        assert!(!css.contains("--accent-color:"));
    }

    #[test]
    fn gtk4_carries_both_schemes_behind_media_queries() {
        let css = gtk4_css(&AppearanceConfig::default(), None);
        assert!(css.contains("@media (prefers-color-scheme: light)"));
        assert!(css.contains("@media (prefers-color-scheme: dark)"));
    }

    #[test]
    fn the_window_controls_sit_on_the_left_in_the_titlebar_order() {
        assert_eq!(button_layout(), "close,minimize,maximize:");
    }

    #[test]
    fn every_toolkit_gets_the_traffic_light_rules_for_each_scheme() {
        let gtk4 = gtk4_css(&AppearanceConfig::default(), None);
        assert_eq!(gtk4.matches("windowcontrols > button.close {").count(), 2);
        assert!(gtk4.contains("windowcontrols:hover > button.close > image { background:"));
        let gtk3 = gtk3_css(&AppearanceConfig::default(), None);
        assert!(gtk3.contains("button.titlebutton.close:backdrop"));
        assert!(live_css(&AppearanceConfig::default(), None).contains("windowcontrols"));
    }

    #[test]
    fn the_wallpaper_mode_tints_the_adwaita_backgrounds() {
        let appearance = AppearanceConfig {
            window_color: WindowColorMode::Wallpaper,
            ..AppearanceConfig::default()
        };
        let dominant = Rgba::rgb8(0x20, 0x60, 0xd0);
        let light = SchemeColors::new(&appearance, Some(dominant), ColorScheme::Light);
        let window = light.get("window_bg_color").expect("tinted window");
        assert_ne!(window, ADWAITA_LIGHT.window);
        assert!(window.b > window.r, "the blue dominant shows in the window");
        let dark = SchemeColors::new(&appearance, Some(dominant), ColorScheme::Dark);
        assert!(dark.dark);
        assert!(dark.get("card_bg_color").is_none());
    }

    #[test]
    fn increased_contrast_drops_the_tint() {
        let appearance = AppearanceConfig {
            window_color: WindowColorMode::Accent,
            contrast: true,
            ..AppearanceConfig::default()
        };
        let colors = SchemeColors::new(&appearance, None, ColorScheme::Light);
        assert!(colors.get("window_bg_color").is_none());
        assert!(colors.get("accent_bg_color").is_some());
    }

    #[test]
    fn gtk3_follows_the_current_scheme_only() {
        let appearance = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            window_color: WindowColorMode::Accent,
            ..AppearanceConfig::default()
        };
        let css = gtk3_css(&appearance, None);
        assert!(!css.contains("@media"));
        assert!(css.contains("@define-color theme_selected_bg_color"));
        assert!(css.contains("@define-color theme_bg_color"));
    }

    #[test]
    fn the_live_rules_carry_only_the_media_blocks() {
        let appearance = AppearanceConfig {
            window_color: WindowColorMode::Accent,
            ..AppearanceConfig::default()
        };
        let live = live_css(&appearance, None);
        assert!(
            live.trim_start()
                .starts_with("@media (prefers-color-scheme: light)")
        );
        assert!(live.contains("@media (prefers-color-scheme: dark)"));
        assert!(live.contains("window_bg_color"));
        assert!(!live.contains("Generado"));
        assert!(gtk4_css(&appearance, None).ends_with(&live));
    }

    #[test]
    fn the_standalone_accent_keeps_libadwaitas_contrast() {
        let light_accent = Rgba::rgb8(0x9a, 0xc8, 0xff);
        let light = standalone_accent(light_accent, false);
        assert!(Oklab::from_rgba(light).l <= ADWAITA_STANDALONE_MAX_L_LIGHT + 1e-3);
        let dark_accent = Rgba::rgb8(0x50, 0x58, 0x68);
        let dark = standalone_accent(dark_accent, true);
        assert!(Oklab::from_rgba(dark).l >= ADWAITA_STANDALONE_MIN_L_DARK - 1e-3);
        let navy = Rgba::rgb8(0x1a, 0x3a, 0x80);
        assert!(Oklab::from_rgba(standalone_accent(navy, true)).l > Oklab::from_rgba(navy).l + 0.3);
        let blue = Rgba::rgb8(0x0b, 0x3d, 0x91);
        let kept = standalone_accent(blue, false);
        assert!((kept.r - blue.r).abs() < 2e-3 && (kept.b - blue.b).abs() < 2e-3);
    }

    #[test]
    fn translucent_colours_use_rgba() {
        assert_eq!(css_color(Rgba::rgb8(255, 0, 16)), "#ff0010");
        assert_eq!(
            css_color(Rgba::rgba8(255, 255, 255, 0.5)),
            "rgba(255, 255, 255, 0.500)"
        );
    }

    #[test]
    fn the_block_goes_first_and_is_idempotent() {
        let user = "window { opacity: 1; }\n";
        let once = install_block(user);
        assert!(once.starts_with(BEGIN));
        assert!(once.ends_with(user));
        assert_eq!(install_block(&once), once);
        assert_eq!(strip_block(&once).trim_start(), user);
    }

    #[test]
    fn the_ini_key_is_set_in_place() {
        assert_eq!(
            set_ini_key("", PREFER_DARK, "true"),
            "[Settings]\ngtk-application-prefer-dark-theme=true\n"
        );
        let ini = "[Settings]\ngtk-theme-name=adw-gtk3\ngtk-application-prefer-dark-theme=0\n";
        assert_eq!(
            set_ini_key(ini, PREFER_DARK, "true"),
            "[Settings]\ngtk-theme-name=adw-gtk3\ngtk-application-prefer-dark-theme=true\n"
        );
        let other = "[Other]\na=1\n";
        assert_eq!(
            set_ini_key(other, PREFER_DARK, "false"),
            "[Other]\na=1\n\n[Settings]\ngtk-application-prefer-dark-theme=false\n"
        );
        let two = "[Settings]\na=1\n[Other]\nb=2\n";
        assert_eq!(
            set_ini_key(two, PREFER_DARK, "true"),
            "[Settings]\na=1\ngtk-application-prefer-dark-theme=true\n[Other]\nb=2\n"
        );
    }

    #[test]
    fn apply_and_remove_round_trip() {
        let home = temp_home("round-trip");
        let gtk4 = home.join("gtk-4.0");
        fs::create_dir_all(&gtk4).unwrap();
        fs::write(gtk4.join("gtk.css"), "label { color: red; }\n").unwrap();
        let bridge = Bridge::at(home.clone());
        let appearance = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            ..AppearanceConfig::default()
        };
        bridge.apply(&appearance, None).unwrap();
        assert!(gtk4.join(GENERATED).exists());
        assert!(home.join("gtk-3.0").join(GENERATED).exists());
        let ini = fs::read_to_string(home.join("gtk-3.0/settings.ini")).unwrap();
        assert!(ini.contains("gtk-application-prefer-dark-theme=true"));

        bridge.remove().unwrap();
        assert!(!gtk4.join(GENERATED).exists());
        assert_eq!(
            fs::read_to_string(gtk4.join("gtk.css")).unwrap(),
            "label { color: red; }\n"
        );
        assert!(!home.join("gtk-3.0/gtk.css").exists());
        let _ = fs::remove_dir_all(&home);
    }
}
