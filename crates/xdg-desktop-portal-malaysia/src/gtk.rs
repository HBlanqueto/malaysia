use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ctappearance::backdrop::{Oklab, WallpaperTinting, lift};
use ctappearance::traffic::{ButtonVisual, Glyph, TrafficInput, glyph_svg, inset, visual};
use ctappearance::{AppearanceConfig, ColorScheme, Resolved, TintParts, ini, palette, resolve};
use ctdesign::{
    ButtonKind, LAYER_FACTOR_BG_SUNKEN, LAYER_FACTOR_RAISED, LAYER_FACTOR_SURFACE, Rgba, TITLEBAR,
    TITLEBAR_CLIENT_HEIGHT, TRAFFIC_RING_WIDTH, TrafficHalo, TrafficLooks,
};

const BEGIN: &str = "/* malaysia:gtk-bridge begin */";
const END: &str = "/* malaysia:gtk-bridge end */";
const GENERATED: &str = "malaysia.css";
const PREFER_DARK: &str = "gtk-application-prefer-dark-theme";
const THEME_KEY: &str = "gtk-theme";
const THEME_NAME: &str = "gtk-theme-name";
const DEFAULT_THEME: &str = "Adwaita";
const LIVE_THEMES: [&str; 2] = ["Malaysia-Live-A", "Malaysia-Live-B"];
const ADWAITA_GTK3: &str = "resource:///org/gtk/libgtk/theme/Adwaita/gtk-contained";
const FIREFOX_XDG_PROFILES: &str = "mozilla/firefox";
const FIREFOX_LEGACY_PROFILES: &str = ".mozilla/firefox";
const FIREFOX_MAC_TRAFFIC_START: f64 = 12.0;

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
    behind: Vec<(&'static str, String)>,
    traffic: TrafficLooks,
}

const BEHIND: &str = "--malaysia-behind";

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
        let dark = resolved.scheme.is_dark();
        let on_accent = palette(resolved.scheme).on_accent;
        let traffic = resolved.traffic_lights;
        let mut colors = vec![
            ("accent_bg_color", resolved.accent),
            ("accent_fg_color", on_accent),
            ("accent_color", standalone_accent(resolved.accent, dark)),
        ];
        let mut behind = Vec::new();
        if !appearance.contrast {
            colors.extend(tinted_backgrounds(&resolved, appearance, dark));
            behind = behind_backgrounds(&resolved, appearance, dark);
        }
        Self {
            dark,
            colors,
            behind,
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
    tint_layers(dark, appearance.tint_parts)
        .into_iter()
        .map(|(name, color, layer)| {
            let share = match layer {
                Layer::Window(share) => share,
                Layer::Sidebar => LAYER_FACTOR_BG_SUNKEN,
            };
            (name, color.mix(tint, factor * share))
        })
        .collect()
}

fn behind_backgrounds(
    resolved: &Resolved,
    appearance: &AppearanceConfig,
    dark: bool,
) -> Vec<(&'static str, String)> {
    let Some(tinting) = resolved
        .wallpaper_tinting
        .filter(|_| resolved.window_tint.is_none())
    else {
        return Vec::new();
    };
    let window = if dark { ADWAITA_DARK } else { ADWAITA_LIGHT }.window;
    tint_layers(dark, appearance.tint_parts)
        .into_iter()
        .map(|(name, color, layer)| {
            let css = match layer {
                Layer::Window(_) => behind_css(color, window, tinting),
                Layer::Sidebar => behind_css(color, color, tinting.sidebar()),
            };
            (name, css)
        })
        .collect()
}

fn behind_css(color: Rgba, reference: Rgba, tinting: WallpaperTinting) -> String {
    let (own, base) = (Oklab::from_rgba(color), Oklab::from_rgba(reference));
    let k = tinting.chroma.clamp(0.0, 1.0);
    let lightness = own.l + Oklab::from_rgba(lift(reference, tinting.lift)).l - base.l;
    let channel = |name: &str, own: f32, base: f32| {
        let rest = own - base * k;
        let sign = if rest < 0.0 { '-' } else { '+' };
        format!("calc({name} * {k:.4} {sign} {:.4})", rest.abs())
    };
    format!(
        "oklab(from var({BEHIND}, {}) {lightness:.4} {} {})",
        css_color(reference),
        channel("a", own.a, base.a),
        channel("b", own.b, base.b)
    )
}

#[derive(Debug, Clone, Copy)]
enum Layer {
    Window(f32),
    Sidebar,
}

fn tint_layers(dark: bool, parts: TintParts) -> Vec<(&'static str, Rgba, Layer)> {
    let base = if dark { ADWAITA_DARK } else { ADWAITA_LIGHT };
    let mut layers = Vec::new();
    if parts.window {
        layers.extend([
            ("window_bg_color", base.window, Layer::Window(1.0)),
            ("headerbar_bg_color", base.headerbar, Layer::Window(1.0)),
            (
                "headerbar_backdrop_color",
                base.headerbar_backdrop,
                Layer::Window(1.0),
            ),
            (
                "view_bg_color",
                base.view,
                Layer::Window(LAYER_FACTOR_SURFACE),
            ),
        ]);
    }
    if parts.sidebars {
        layers.extend([
            ("sidebar_bg_color", base.sidebar, Layer::Sidebar),
            (
                "sidebar_backdrop_color",
                base.sidebar_backdrop,
                Layer::Sidebar,
            ),
            (
                "secondary_sidebar_bg_color",
                base.secondary_sidebar,
                Layer::Sidebar,
            ),
            (
                "secondary_sidebar_backdrop_color",
                base.secondary_sidebar_backdrop,
                Layer::Sidebar,
            ),
        ]);
    }
    if parts.groups
        && let Some(card) = base.card
    {
        layers.push(("card_bg_color", card, Layer::Window(LAYER_FACTOR_SURFACE)));
    }
    if parts.menus {
        layers.extend([
            (
                "dialog_bg_color",
                base.dialog,
                Layer::Window(LAYER_FACTOR_RAISED),
            ),
            (
                "popover_bg_color",
                base.popover,
                Layer::Window(LAYER_FACTOR_RAISED),
            ),
        ]);
    }
    layers
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
    for (name, value) in &colors.behind {
        let variable = name.replace('_', "-");
        out.push_str(&format!("{indent}  --{variable}: {value};\n"));
    }
    out.push_str(&format!("{indent}}}\n"));
}

const HEADER: &str = "/* Generated by xdg-desktop-portal-malaysia from appearance.domain.\n \
                      * Rewritten on every change: do not edit it; your rules\n \
                      * go in gtk.css, below the malaysia:gtk-bridge block. */\n";

fn push_media_blocks(out: &mut String, light: &SchemeColors, dark: &SchemeColors) {
    for (query, colors) in [("light", light), ("dark", dark)] {
        out.push_str(&format!("\n@media (prefers-color-scheme: {query}) {{\n"));
        push_variables(out, colors, "  ");
        push_define_colors(out, colors, "  ");
        push_window_controls(out, &colors.traffic, Toolkit::Gtk4, "  ");
        out.push_str("}\n");
    }
}

pub(crate) fn button_layout() -> String {
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

fn traffic_states(
    looks: &TrafficLooks,
    focused: bool,
    group_hovered: bool,
    pressed: Option<ButtonKind>,
) -> [ButtonVisual; 3] {
    visual(
        looks,
        TrafficInput {
            focused,
            group_hovered,
            pressed,
            zoom_supported: true,
            fullscreen: false,
        },
    )
}

const ADWAITA_HEADERBAR_START: u32 = 6;

const GTK3_HEADER_SPACING: u32 = 6;
const GTK3_TITLEBUTTON_ICON: u32 = 16;

const ADWAITA_BACKDROP_DIM: &str = "filter: opacity(0.5);";

const ADWAITA_BACKDROP_BARS: &str =
    "headerbar:backdrop > windowhandle, toolbarview > .top-bar:backdrop > windowhandle";

const BACKDROP_DIMMED: [&str; 5] = [
    "toolbarview > .top-bar:backdrop > windowhandle > box > :not(headerbar)",
    "headerbar:backdrop > windowhandle > box > :not(box):not(widget)",
    "headerbar:backdrop > windowhandle > box > box > :not(windowcontrols)",
    "headerbar:backdrop > windowhandle > box > widget > :not(box)",
    "headerbar:backdrop > windowhandle > box > widget > box > :not(windowcontrols)",
];

fn glyph_image(index: usize, color: Rgba) -> String {
    let svg = glyph_svg(Glyph::of(index, false, false), color);
    let encoded: String = svg
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("url(\"data:image/svg+xml,{encoded}\")")
}

fn gradient(colors: &[Rgba]) -> String {
    let last = (colors.len() - 1) as f32;
    let stops: Vec<String> = colors
        .iter()
        .enumerate()
        .map(|(index, color)| format!("{} {:.2}%", css_color(*color), index as f32 / last * 100.0))
        .collect();
    format!("linear-gradient(to bottom, {})", stops.join(", "))
}

fn light_images(index: usize, visual: &ButtonVisual, fill: &[Rgba]) -> String {
    match visual.glyph {
        Some(color) => format!("{}, {}", glyph_image(index, color), gradient(fill)),
        None => gradient(fill),
    }
}

fn pressed_fill(visual: &ButtonVisual, backdrop: Rgba) -> [Rgba; 4] {
    visual.fill.map(|color| {
        let lift = |channel: f32, ground: f32| {
            ((channel * color.a + ground * (1.0 - color.a)) / (1.0 - visual.dodge)).min(1.0)
        };
        Rgba {
            r: lift(color.r, backdrop.r),
            g: lift(color.g, backdrop.g),
            b: lift(color.b, backdrop.b),
            a: 1.0,
        }
    })
}

fn light_shadow(ring: &[Rgba; 3], halo: TrafficHalo) -> String {
    let width = TRAFFIC_RING_WIDTH;
    format!(
        "inset 0 {width}px 0 0 {}, inset 0 -{width}px 0 0 {}, inset 0 0 0 {width}px {}, 0 {}px {}px {}",
        css_color(ring[0]),
        css_color(ring[2]),
        css_color(ring[1]),
        halo.dy,
        halo.blur * 2.0,
        css_color(halo.color)
    )
}

fn push_window_controls(out: &mut String, looks: &TrafficLooks, toolkit: Toolkit, indent: &str) {
    let buttons = &TITLEBAR.buttons;
    let radius = buttons.diameter as f32 / 2.0;
    let focused = traffic_states(looks, true, false, None);
    let backdrop = traffic_states(looks, false, false, None);
    let hovered = traffic_states(looks, true, true, None);
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
                (inset(TITLEBAR_CLIENT_HEIGHT) as u32).saturating_sub(ADWAITA_HEADERBAR_START)
            ),
        );
        rule(ADWAITA_BACKDROP_BARS, "filter: none;");
        rule(&BACKDROP_DIMMED.join(", "), ADWAITA_BACKDROP_DIM);
    }
    for (index, button) in buttons.order.iter().enumerate() {
        let name = gtk_button_name(button.kind);
        let pressed = traffic_states(looks, true, true, Some(button.kind))[index];
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
            "border: none; border-radius: {radius}px; box-shadow: {}; background: {};",
            light_shadow(&focused[index].ring, looks.halo),
            gradient(&focused[index].fill)
        );
        match toolkit {
            Toolkit::Gtk4 => {
                rule(
                    &button,
                    "min-width: 0; min-height: 0; padding: 0; margin: 0; border: none; \
                     box-shadow: none; background: none;",
                );
                rule(
                    &light,
                    &format!(
                        "min-width: 0; min-height: 0; padding: 0; \
                         -gtk-icon-size: {}px; color: transparent; {shape}",
                        buttons.diameter
                    ),
                );
            }
            Toolkit::Gtk3 => {
                let margin = if index > 0 {
                    buttons.spacing - GTK3_HEADER_SPACING
                } else {
                    (inset(TITLEBAR_CLIENT_HEIGHT) as u32).saturating_sub(ADWAITA_HEADERBAR_START)
                };
                rule(
                    &button,
                    &format!(
                        "min-width: {d}px; min-height: {d}px; padding: 0; \
                         margin: 0 0 0 {margin}px; {shape}",
                        d = buttons.diameter
                    ),
                );
                rule(
                    &format!("{button} image"),
                    &format!(
                        "opacity: 0; color: transparent; margin: {}px;",
                        (f64::from(buttons.diameter) - f64::from(GTK3_TITLEBUTTON_ICON)) / 2.0
                    ),
                );
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
            &format!(
                "background: {}; box-shadow: {};",
                gradient(&backdrop[index].fill),
                light_shadow(&backdrop[index].ring, looks.halo)
            ),
        );
        rule(
            &hover,
            &format!(
                "background-image: {}; background-position: center; \
                 background-repeat: no-repeat; background-size: {d}px {d}px; \
                 box-shadow: {};",
                light_images(index, &hovered[index], &hovered[index].fill),
                light_shadow(&hovered[index].ring, looks.halo),
                d = buttons.diameter
            ),
        );
        rule(
            &active,
            &format!(
                "background-image: {};",
                light_images(index, &pressed, &pressed_fill(&pressed, looks.backdrop))
            ),
        );
    }
}

fn firefox_classes(kind: ButtonKind) -> &'static [&'static str] {
    match kind {
        ButtonKind::Close => &["titlebar-close"],
        ButtonKind::Minimize => &["titlebar-min"],
        ButtonKind::Maximize => &["titlebar-max", "titlebar-restore"],
    }
}

fn firefox_buttons(kind: ButtonKind) -> String {
    firefox_classes(kind)
        .iter()
        .map(|class| format!(".{class}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn firefox_icons(kind: ButtonKind, scope: &str, state: &str) -> String {
    firefox_classes(kind)
        .iter()
        .map(|class| format!("{scope}.{class}{state} > .toolbarbutton-icon"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn push_firefox_controls(out: &mut String, looks: &TrafficLooks, indent: &str) {
    let buttons = &TITLEBAR.buttons;
    let radius = buttons.diameter as f32 / 2.0;
    let focused = traffic_states(looks, true, false, None);
    let backdrop = traffic_states(looks, false, false, None);
    let hovered = traffic_states(looks, true, true, None);
    let paint = |visual: &ButtonVisual, images: String| {
        format!(
            "background-image: {images} !important; box-shadow: {} !important;",
            light_shadow(&visual.ring, looks.halo)
        )
    };
    let mut rule = |selector: &str, body: &str| {
        out.push_str(&format!("{indent}{selector} {{ {body} }}\n"));
    };
    let half_gap = f64::from(buttons.spacing) / 2.0;
    rule(
        ".titlebar-buttonbox",
        &format!("--titlebar-button-spacing: {half_gap}px !important;"),
    );
    rule(
        ".titlebar-buttonbox-container",
        &format!(
            "padding-inline-start: {}px !important;",
            FIREFOX_MAC_TRAFFIC_START - half_gap
        ),
    );
    rule(
        &firefox_buttons(buttons.order[buttons.order.len() - 1].kind),
        "padding-inline-end: 0 !important;",
    );
    rule(
        ".titlebar-button > .toolbarbutton-icon",
        &format!(
            "width: {d}px !important; height: {d}px !important; \
             border-radius: {radius}px !important; background-image: none !important; \
             background-position: center !important; background-repeat: no-repeat !important; \
             background-size: {d}px {d}px !important; transition: none !important;",
            d = buttons.diameter
        ),
    );
    for (index, button) in buttons.order.iter().enumerate() {
        let kind = button.kind;
        let pressed = traffic_states(looks, true, true, Some(kind))[index];
        rule(
            &firefox_icons(kind, "", ""),
            &paint(&focused[index], gradient(&focused[index].fill)),
        );
        rule(
            &firefox_icons(kind, "", ":-moz-window-inactive"),
            &paint(&backdrop[index], gradient(&backdrop[index].fill)),
        );
        rule(
            &firefox_icons(kind, ".titlebar-buttonbox:hover ", ""),
            &paint(
                &hovered[index],
                light_images(index, &hovered[index], &hovered[index].fill),
            ),
        );
        rule(
            &firefox_icons(kind, "", ":hover:active"),
            &format!(
                "background-image: {} !important;",
                light_images(index, &pressed, &pressed_fill(&pressed, looks.backdrop))
            ),
        );
    }
}

fn controls_blocks(
    appearance: &AppearanceConfig,
    dominant: Option<Rgba>,
    push_controls: impl Fn(&mut String, &TrafficLooks, &str),
) -> String {
    let (light, dark) = schemes(appearance);
    let mut out = String::new();
    for (query, scheme) in [("light", light), ("dark", dark)] {
        let colors = SchemeColors::new(appearance, dominant, scheme);
        out.push_str(&format!("@media (prefers-color-scheme: {query}) {{\n"));
        push_controls(&mut out, &colors.traffic, "  ");
        out.push_str("}\n");
    }
    out
}

fn firefox_css(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> String {
    controls_blocks(appearance, dominant, push_firefox_controls)
}

fn gtk4_controls_css(appearance: &AppearanceConfig) -> String {
    let blocks = controls_blocks(appearance, None, |out, traffic, indent| {
        push_window_controls(out, traffic, Toolkit::Gtk4, indent)
    });
    format!("{HEADER}\n{blocks}")
}

fn gtk3_controls_css(appearance: &AppearanceConfig) -> String {
    let (light, dark) = schemes(appearance);
    let scheme = if appearance.color_scheme.is_dark() {
        dark
    } else {
        light
    };
    let colors = SchemeColors::new(appearance, None, scheme);
    let mut out = String::from(HEADER);
    out.push('\n');
    push_window_controls(&mut out, &colors.traffic, Toolkit::Gtk3, "");
    out
}

fn gtk4_css(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> String {
    let (light, dark) = schemes(appearance);
    let light = SchemeColors::new(appearance, dominant, light);
    let dark = SchemeColors::new(appearance, dominant, dark);
    let current = if appearance.color_scheme.is_dark() {
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

pub(crate) fn live_css(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> String {
    let (light, dark) = schemes(appearance);
    let light = SchemeColors::new(appearance, dominant, light);
    let dark = SchemeColors::new(appearance, dominant, dark);
    let mut out = String::new();
    push_media_blocks(&mut out, &light, &dark);
    out
}

fn gtk3_css(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> String {
    let (light, dark) = schemes(appearance);
    let scheme = if appearance.color_scheme.is_dark() {
        dark
    } else {
        light
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
        if trimmed.starts_with(BEGIN.trim_end_matches(" */")) {
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

pub(crate) fn write_if_changed(path: &Path, contents: &str) -> io::Result<()> {
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

fn install_import(dir: &Path, file: &str, css: &str) -> io::Result<()> {
    let main = dir.join(file);
    if main.is_symlink() {
        tracing::warn!(path = %main.display(), "{file} is a symlink; the GTK bridge leaves it alone");
        return Ok(());
    }
    write_if_changed(&dir.join(GENERATED), css)?;
    let current = fs::read_to_string(&main).unwrap_or_default();
    write_if_changed(&main, &install_block(&current))
}

fn remove_import(dir: &Path, file: &str) -> io::Result<()> {
    let main = dir.join(file);
    if !main.is_symlink()
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
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn next_live_theme(current: &str, css: &str, themes: &Path) -> Option<&'static str> {
    let slot = LIVE_THEMES.iter().position(|name| *name == current);
    let next = match slot {
        Some(index) if live_css_of(&themes.join(current)).is_some_and(|old| old == css) => index,
        Some(index) => 1 - index,
        None if current == DEFAULT_THEME => 0,
        None => return None,
    };
    Some(LIVE_THEMES[next])
}

fn live_css_of(theme: &Path) -> Option<String> {
    fs::read_to_string(theme.join("gtk-3.0").join(GENERATED)).ok()
}

fn write_live_theme(theme: &Path, css: &str) -> io::Result<()> {
    let dir = theme.join("gtk-3.0");
    write_if_changed(&dir.join(GENERATED), css)?;
    for (file, variant) in [("gtk.css", ""), ("gtk-dark.css", "-dark")] {
        write_if_changed(
            &dir.join(file),
            &format!(
                "@import url(\"{ADWAITA_GTK3}{variant}.css\");\n@import url(\"{GENERATED}\");\n"
            ),
        )?;
    }
    Ok(())
}

pub(crate) struct Bridge {
    config_home: PathBuf,
    themes: PathBuf,
    firefox_roots: Vec<PathBuf>,
}

impl Bridge {
    pub(crate) fn from_env() -> Option<Self> {
        let config_home = ctappearance::xdg::config_home()?;
        let mut firefox_roots = vec![config_home.join(FIREFOX_XDG_PROFILES)];
        firefox_roots.extend(
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(FIREFOX_LEGACY_PROFILES)),
        );
        Some(Self {
            config_home,
            themes: ctappearance::xdg::data_home()?.join("themes"),
            firefox_roots,
        })
    }

    #[cfg(test)]
    fn at(config_home: PathBuf) -> Self {
        Self {
            firefox_roots: vec![
                config_home.join(FIREFOX_XDG_PROFILES),
                config_home.join(FIREFOX_LEGACY_PROFILES),
            ],
            themes: config_home.join("themes"),
            config_home,
        }
    }

    fn dir(&self, version: &str) -> PathBuf {
        self.config_home.join(version)
    }

    fn firefox_chrome_dirs(&self) -> Vec<PathBuf> {
        self.firefox_roots
            .iter()
            .filter_map(|root| fs::read_dir(root).ok())
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|profile| profile.join("prefs.js").is_file())
            .map(|profile| profile.join("chrome"))
            .collect()
    }

    fn apply(&self, appearance: &AppearanceConfig, dominant: Option<Rgba>) -> io::Result<()> {
        let dark = appearance.color_scheme.is_dark();
        install_import(
            &self.dir("gtk-4.0"),
            "gtk.css",
            &gtk4_css(appearance, dominant),
        )?;
        let css = gtk3_css(appearance, dominant);
        let current = crate::settings::gnome_setting(THEME_KEY);
        let live = current
            .as_deref()
            .and_then(|current| next_live_theme(current, &css, &self.themes));
        match live {
            Some(theme) => {
                write_live_theme(&self.themes.join(theme), &css)?;
                remove_import(&self.dir("gtk-3.0"), "gtk.css")?;
                if current.as_deref() != Some(theme) {
                    crate::settings::set_gnome_setting(THEME_KEY, Some(theme));
                }
            }
            None => install_import(&self.dir("gtk-3.0"), "gtk.css", &css)?,
        }
        let ini = self.dir("gtk-3.0").join("settings.ini");
        if ini.is_symlink() {
            tracing::warn!(path = %ini.display(), "settings.ini is a symlink; GTK 3 dark mode not set");
        } else {
            let current = fs::read_to_string(&ini).unwrap_or_default();
            let value = if dark { "true" } else { "false" };
            let current = ini::set(&current, "Settings", PREFER_DARK, Some(value));
            let current = match live {
                Some(theme) => ini::set(&current, "Settings", THEME_NAME, Some(theme)),
                None => current,
            };
            write_if_changed(&ini, &current)?;
        }
        let firefox = firefox_css(appearance, dominant);
        for dir in self.firefox_chrome_dirs() {
            install_import(&dir, "userChrome.css", &firefox)?;
        }
        Ok(())
    }

    fn apply_controls(&self, appearance: &AppearanceConfig) -> io::Result<()> {
        install_import(
            &self.dir("gtk-4.0"),
            "gtk.css",
            &gtk4_controls_css(appearance),
        )?;
        install_import(
            &self.dir("gtk-3.0"),
            "gtk.css",
            &gtk3_controls_css(appearance),
        )?;
        let firefox = firefox_css(appearance, None);
        for dir in self.firefox_chrome_dirs() {
            install_import(&dir, "userChrome.css", &firefox)?;
        }
        Ok(())
    }

    pub(crate) fn remove(&self) -> io::Result<()> {
        for version in ["gtk-4.0", "gtk-3.0"] {
            remove_import(&self.dir(version), "gtk.css")?;
        }
        if crate::settings::gnome_setting(THEME_KEY)
            .is_some_and(|current| LIVE_THEMES.contains(&current.as_str()))
        {
            crate::settings::set_gnome_setting(THEME_KEY, None);
        }
        let ini = self.dir("gtk-3.0").join("settings.ini");
        if let Ok(current) = fs::read_to_string(&ini)
            && ini::value(&current, "Settings", THEME_NAME)
                .is_some_and(|name| LIVE_THEMES.contains(&name))
            && !ini.is_symlink()
        {
            write_if_changed(&ini, &ini::set(&current, "Settings", THEME_NAME, None))?;
        }
        for theme in LIVE_THEMES {
            match fs::remove_dir_all(self.themes.join(theme)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        for dir in self.firefox_chrome_dirs() {
            remove_import(&dir, "userChrome.css")?;
        }
        Ok(())
    }

    pub(crate) fn sync(&self, appearance: Option<&AppearanceConfig>, dominant: Option<Rgba>) {
        let default = AppearanceConfig::default();
        let appearance = appearance.unwrap_or(&default);
        let result = if appearance.gtk_bridge {
            self.apply(appearance, dominant)
        } else {
            self.remove().and_then(|()| self.apply_controls(appearance))
        };
        if let Err(error) = result {
            tracing::warn!(%error, "GTK bridge could not update the GTK user CSS");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctappearance::WindowColorMode;

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
        assert!(gtk4.contains("windowcontrols:hover > button.close > image { background-image:"));
        let gtk3 = gtk3_css(&AppearanceConfig::default(), None);
        assert!(gtk3.contains("button.titlebutton.close:backdrop"));
        assert!(live_css(&AppearanceConfig::default(), None).contains("windowcontrols"));
    }

    #[test]
    fn the_accent_mode_tints_the_adwaita_backgrounds() {
        let appearance = AppearanceConfig {
            window_color: WindowColorMode::Accent,
            accent_color: Some([0.125, 0.376, 0.816]),
            ..AppearanceConfig::default()
        };
        let light = SchemeColors::new(&appearance, None, ColorScheme::Light);
        let window = light.get("window_bg_color").expect("tinted window");
        assert_ne!(window, ADWAITA_LIGHT.window);
        assert!(window.b > window.r, "the blue accent shows in the window");
        let dark = SchemeColors::new(&appearance, None, ColorScheme::Dark);
        assert!(dark.dark);
        assert!(dark.get("card_bg_color").is_none());
    }

    #[test]
    fn the_wallpaper_source_gives_libadwaita_the_color_behind_each_window() {
        let dark = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            ..AppearanceConfig::default()
        };
        let live = live_css(&dark, None);
        let (light_block, dark_block) = live.split_once("prefers-color-scheme: dark").unwrap();
        assert!(
            dark_block.contains("--window-bg-color: oklab(from var(--malaysia-behind, #222226)")
        );
        assert!(dark_block.contains("--sidebar-bg-color: oklab(from var(--malaysia-behind"));
        assert!(
            !light_block.contains(BEHIND),
            "light mode needs wallpaper_tinting_in_light"
        );
        let accent = AppearanceConfig {
            window_color: WindowColorMode::Accent,
            ..dark
        };
        assert!(!live_css(&accent, None).contains(BEHIND));
    }

    fn behind_dark(name: &str, wall: Rgba) -> Oklab {
        let colors = SchemeColors::new(
            &AppearanceConfig {
                color_scheme: ColorScheme::Dark,
                ..AppearanceConfig::default()
            },
            None,
            ColorScheme::Dark,
        );
        let (_, css) = colors
            .behind
            .iter()
            .find(|(layer, _)| *layer == name)
            .unwrap();
        let numbers: Vec<f32> = css
            .split([' ', '(', ')'])
            .filter_map(|token| token.parse().ok())
            .collect();
        let [l, ka, ca, kb, cb] = numbers[..] else {
            panic!("unexpected expression {css}");
        };
        let sign = |channel: &str| {
            if css.contains(&format!("{channel} * {ka:.4} -")) {
                -1.0
            } else {
                1.0
            }
        };
        let w = Oklab::from_rgba(wall);
        Oklab {
            l,
            a: w.a * ka + sign("a") * ca,
            b: w.b * kb + sign("b") * cb,
        }
    }

    #[test]
    fn the_css_behind_a_window_matches_the_barrierkit_mix() {
        let wall = Rgba::rgb8(146, 197, 41);
        let tinting = WallpaperTinting {
            chroma: ctdesign::WALLPAPER_TINTING[1],
            lift: ctdesign::WALLPAPER_TINTING_LIFT[1],
        };
        let window = behind_dark("window_bg_color", wall);
        let expected = Oklab::from_rgba(tinting.take(ADWAITA_DARK.window, wall));
        assert!((window.l - expected.l).abs() < 0.002);
        assert!((window.a - expected.a).abs() < 0.002);
        assert!((window.b - expected.b).abs() < 0.002);
        let sidebar = behind_dark("sidebar_bg_color", wall);
        let expected = Oklab::from_rgba(tinting.sidebar().take(ADWAITA_DARK.sidebar, wall));
        assert!((sidebar.l - expected.l).abs() < 0.002);
        assert!((sidebar.a - expected.a).abs() < 0.002);
        assert!((sidebar.b - expected.b).abs() < 0.002);
    }

    #[test]
    fn every_libadwaita_layer_shifts_as_much_as_the_window_behind_the_wallpaper() {
        let wall = Rgba::rgb8(70, 65, 155);
        let shift = |name: &str, color: Rgba| {
            let (after, before) = (behind_dark(name, wall), Oklab::from_rgba(color));
            [after.l - before.l, after.a - before.a, after.b - before.b]
        };
        let window = shift("window_bg_color", ADWAITA_DARK.window);
        for (name, color) in [
            ("view_bg_color", ADWAITA_DARK.view),
            ("headerbar_bg_color", ADWAITA_DARK.headerbar),
            ("dialog_bg_color", ADWAITA_DARK.dialog),
            ("popover_bg_color", ADWAITA_DARK.popover),
        ] {
            for (taken, expected) in shift(name, color).iter().zip(window) {
                assert!((taken - expected).abs() < 0.002, "{name}");
            }
        }
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
        assert!(!live.contains("Generated"));
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
    fn a_begin_marker_with_a_suffix_is_replaced() {
        let user = "window { opacity: 1; }\n";
        let named = format!("/* malaysia:gtk-bridge begin (portal) */\n{END}\n{user}");
        assert_eq!(install_block(&named), install_block(user));
    }

    #[test]
    fn firefox_gets_the_traffic_light_rules_for_each_scheme() {
        let css = firefox_css(&AppearanceConfig::default(), None);
        assert_eq!(css.matches("@media (prefers-color-scheme:").count(), 2);
        assert_eq!(
            css.matches("\n  .titlebar-close > .toolbarbutton-icon {")
                .count(),
            2
        );
        assert!(css.contains(".titlebar-max > .toolbarbutton-icon, .titlebar-restore >"));
        assert!(css.contains(
            ".titlebar-buttonbox:hover .titlebar-min > .toolbarbutton-icon { background-image:"
        ));
        assert!(css.contains(".titlebar-close:-moz-window-inactive > .toolbarbutton-icon"));
    }

    #[test]
    fn firefox_lights_sit_where_firefox_puts_them_on_macos() {
        let half_gap = f64::from(TITLEBAR.buttons.spacing) / 2.0;
        let container = FIREFOX_MAC_TRAFFIC_START - half_gap;
        let css = firefox_css(&AppearanceConfig::default(), None);
        assert!(css.contains(&format!(
            ".titlebar-buttonbox-container {{ padding-inline-start: {container}px !important; }}"
        )));
        assert!(css.contains(&format!(
            ".titlebar-buttonbox {{ --titlebar-button-spacing: {half_gap}px !important; }}"
        )));
        assert!(
            css.contains(".titlebar-max, .titlebar-restore { padding-inline-end: 0 !important; }")
        );
    }

    #[test]
    fn firefox_profiles_in_both_folders_get_the_bridge_and_keep_their_user_chrome() {
        let home = temp_home("firefox");
        let chromes: Vec<PathBuf> = [FIREFOX_XDG_PROFILES, FIREFOX_LEGACY_PROFILES]
            .iter()
            .map(|root| {
                let profile = home.join(root).join("abc.default");
                fs::create_dir_all(profile.join("chrome")).unwrap();
                fs::write(profile.join("prefs.js"), "").unwrap();
                fs::write(
                    profile.join("chrome/userChrome.css"),
                    "#nav-bar { order: 1; }\n",
                )
                .unwrap();
                profile.join("chrome")
            })
            .collect();
        let unused = home.join(FIREFOX_XDG_PROFILES).join("Crash Reports");
        fs::create_dir_all(&unused).unwrap();
        let bridge = Bridge::at(home.clone());

        bridge.apply(&AppearanceConfig::default(), None).unwrap();
        for chrome in &chromes {
            assert!(chrome.join(GENERATED).exists());
            let user_chrome = fs::read_to_string(chrome.join("userChrome.css")).unwrap();
            assert_eq!(user_chrome, install_block("#nav-bar { order: 1; }\n"));
        }
        assert!(!unused.join("chrome").exists());

        bridge.remove().unwrap();
        for chrome in &chromes {
            assert!(!chrome.join(GENERATED).exists());
            assert_eq!(
                fs::read_to_string(chrome.join("userChrome.css")).unwrap(),
                "#nav-bar { order: 1; }\n"
            );
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn gtk3_takes_over_adwaita_and_switches_live_theme_only_when_the_css_changes() {
        let themes = temp_home("live-themes");
        assert_eq!(next_live_theme("Yaru", "a", &themes), None);
        assert_eq!(
            next_live_theme(DEFAULT_THEME, "a", &themes),
            Some(LIVE_THEMES[0])
        );
        write_live_theme(&themes.join(LIVE_THEMES[0]), "a").unwrap();
        assert_eq!(
            next_live_theme(LIVE_THEMES[0], "a", &themes),
            Some(LIVE_THEMES[0])
        );
        assert_eq!(
            next_live_theme(LIVE_THEMES[0], "b", &themes),
            Some(LIVE_THEMES[1])
        );
        assert_eq!(
            next_live_theme(LIVE_THEMES[1], "b", &themes),
            Some(LIVE_THEMES[0])
        );
        let dark =
            fs::read_to_string(themes.join(LIVE_THEMES[0]).join("gtk-3.0/gtk-dark.css")).unwrap();
        assert!(dark.starts_with(&format!("@import url(\"{ADWAITA_GTK3}-dark.css\");")));
        assert!(dark.ends_with(&format!("@import url(\"{GENERATED}\");\n")));
        let _ = fs::remove_dir_all(&themes);
    }

    #[test]
    fn with_the_bridge_off_only_the_traffic_lights_stay() {
        let home = temp_home("controls-only");
        let bridge = Bridge::at(home.clone());
        let appearance = AppearanceConfig {
            gtk_bridge: false,
            ..AppearanceConfig::default()
        };
        bridge.sync(Some(&appearance), None);
        for (version, lights) in [
            ("gtk-4.0", "windowcontrols"),
            ("gtk-3.0", "button.titlebutton"),
        ] {
            let css = fs::read_to_string(home.join(version).join(GENERATED)).unwrap();
            assert!(css.contains(lights), "{version} keeps the lights");
            assert!(!css.contains("@define-color"), "{version} drops the colors");
        }
        let _ = fs::remove_dir_all(&home);
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
