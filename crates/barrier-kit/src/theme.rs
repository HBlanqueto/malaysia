//! The Freya theme built from `vow-design` and `vow-appearance` (ADR 0034).
//!
//! No colour or measurement is written here: the sheet is a projection of the
//! resolved Neptunia palette, so the compositor, the shell and the apps keep
//! sharing one source.

use freya_components::theming::component_themes::{ColorsSheet, Theme};
use freya_components::theming::themes::{dark_theme, light_theme};
use freya_core::prelude::{Color, State, try_consume_root_context};
use vow_appearance::{
    ColorScheme, Resolved, WindowTintScope, load_appearance, load_dominant, resolve,
};
use vow_design::{
    BG_WINDOW_DARK, BG_WINDOW_DARK_TINT, BG_WINDOW_LIGHT, LockPalette, MenuPalette, Rgba,
    SHELL_DARK_OPAQUE, SHELL_DARK_TINT_OPAQUE, SHELL_LIGHT_OPAQUE, ShellPalette,
};

pub(crate) fn color(c: Rgba) -> Color {
    let [r, g, b, a] = c.to_rgba8();
    Color::from_argb(a, r, g, b)
}

/// What kind of surface the kit is painting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// A normal xdg_toplevel owned by a kit app.
    Toplevel,
    /// A shell layer surface (menubar, dock).
    LayerShell,
}

/// The default background for `kind`, from the appearance the kit already watches.
pub fn surface_background(resolved: &Resolved, kind: SurfaceKind) -> Rgba {
    match kind {
        SurfaceKind::Toplevel => resolved.surfaces.window,
        SurfaceKind::LayerShell => {
            let mut bar = resolved.shell.bar;
            // Wallpaper + desktop already mixed shell.bar with panel_tint in resolve().
            if resolved.window_tint_scope == WindowTintScope::Desktop
                && resolved.panel_tint.is_none()
                && let Some((tint, factor)) = resolved.window_tint
            {
                bar = bar.mix(tint, factor);
            }
            bar
        }
    }
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
/// Inside a Freya tree this reads the process-wide reactive state provided by
/// the runner, so components that call it re-execute when the theme changes.
/// Outside a runner (tests without context, previews) it loads from disk.
pub fn resolved() -> Resolved {
    if let Some(state) = try_consume_root_context::<State<Resolved>>() {
        *state.read()
    } else {
        // shortcut: previews and tests that do not provide the runner state
        // resolve from disk each time. Replace with a root context if reactive
        // updates become needed.
        resolve(&load_appearance(), load_dominant())
    }
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

/// The neutral background a menu is raised above for `kind`.
fn neutral_background(scheme: ColorScheme, kind: SurfaceKind) -> Rgba {
    match (scheme, kind) {
        (ColorScheme::Dark, SurfaceKind::Toplevel) => BG_WINDOW_DARK,
        (ColorScheme::DarkTint, SurfaceKind::Toplevel) => BG_WINDOW_DARK_TINT,
        (_, SurfaceKind::Toplevel) => BG_WINDOW_LIGHT,
        (ColorScheme::Dark, SurfaceKind::LayerShell) => SHELL_DARK_OPAQUE.bar,
        (ColorScheme::DarkTint, SurfaceKind::LayerShell) => SHELL_DARK_TINT_OPAQUE.bar,
        (_, SurfaceKind::LayerShell) => SHELL_LIGHT_OPAQUE.bar,
    }
}

/// The menu background over `kind`, keeping the menu's raised offset.
fn menu_background(resolved: &Resolved, kind: SurfaceKind) -> Rgba {
    let parent = surface_background(resolved, kind);
    let neutral = neutral_background(resolved.scheme, kind);
    let menu = resolved.menu.bg;
    // With no tint, keep MENU_* exactly.
    if parent.r == neutral.r && parent.g == neutral.g && parent.b == neutral.b {
        return menu;
    }
    // MENU_* sits `menu - neutral` above its parent; shift by that same offset
    // so the parent's tint survives and the menu stays raised.
    let lift = |parent: f32, menu: f32, neutral: f32| (parent + menu - neutral).clamp(0.0, 1.0);
    Rgba {
        r: lift(parent.r, menu.r, neutral.r),
        g: lift(parent.g, menu.g, neutral.g),
        b: lift(parent.b, menu.b, neutral.b),
        a: menu.a,
    }
}

/// The menu palette for the current appearance and opening surface.
pub fn menu_palette(resolved: &Resolved, kind: SurfaceKind) -> MenuPalette {
    let mut palette = resolved.menu;
    palette.bg = menu_background(resolved, kind);
    palette.separator_color = resolved.menu_separator(palette.bg);
    palette
}

/// Applies the settings popover metrics over `palette`; colours are untouched.
pub fn apply_popover_metrics(mut palette: MenuPalette) -> MenuPalette {
    palette.radius = vow_design::MENU_POPOVER.radius;
    palette.padding = vow_design::MENU_POPOVER.padding;
    palette.min_width = vow_design::MENU_POPOVER.min_width;
    palette.item_height = vow_design::MENU_POPOVER.item_height;
    palette.item_radius = vow_design::MENU_POPOVER.item_radius;
    palette.item_padding_x = vow_design::MENU_POPOVER.item_padding_start;
    palette.mark_width = vow_design::MENU_POPOVER.check;
    palette.mark_check = vow_design::MENU_POPOVER.check;
    palette.separator_margin_y = vow_design::MENU_POPOVER.separator_margin;
    palette
}

/// The palette of a settings dropdown menu: live colours plus popover metrics.
pub fn popover_palette(resolved: &Resolved, kind: SurfaceKind) -> MenuPalette {
    apply_popover_metrics(menu_palette(resolved, kind))
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

#[cfg(test)]
mod tests {
    use super::*;
    use freya_core::prelude::*;
    use freya_testing::TestingRunner;
    use std::sync::Mutex;
    use torin::prelude::{Size, Size2D};
    use vow_appearance::{AppearanceConfig, ColorScheme, resolve};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_dirs<F: FnOnce() -> R, R>(f: F) -> R {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir =
            std::env::temp_dir().join(format!("barrier-kit-theme-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old_config = std::env::var_os("MALAYSIA_CONFIG_DIR");
        let old_data = std::env::var_os("MALAYSIA_DATA_DIR");
        unsafe {
            std::env::set_var("MALAYSIA_CONFIG_DIR", &dir);
            std::env::set_var("MALAYSIA_DATA_DIR", &dir);
        }
        let result = f();
        match old_config {
            Some(v) => unsafe { std::env::set_var("MALAYSIA_CONFIG_DIR", v) },
            None => unsafe { std::env::remove_var("MALAYSIA_CONFIG_DIR") },
        }
        match old_data {
            Some(v) => unsafe { std::env::set_var("MALAYSIA_DATA_DIR", v) },
            None => unsafe { std::env::remove_var("MALAYSIA_DATA_DIR") },
        }
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    fn decode_png(data: &[u8]) -> (u32, u32, Vec<u8>) {
        let decoder = png::Decoder::new(data);
        let mut reader = decoder.read_info().expect("png info");
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("png frame");
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    fn pixel_at(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * width + x) * 4) as usize;
        [data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]
    }

    /// The testing runner renders on a white background, so a translucent fill
    /// is alpha-composited over white.
    fn over_white([r, g, b, a]: [u8; 4]) -> [u8; 4] {
        if a == 255 {
            return [r, g, b, a];
        }
        let alpha = a as f32 / 255.0;
        let blend = |c| (c as f32 * alpha + 255.0 * (1.0 - alpha)).round() as u8;
        [blend(r), blend(g), blend(b), 255]
    }

    fn light_and_dark() -> (Resolved, Resolved) {
        let light = resolve(
            &AppearanceConfig {
                color_scheme: ColorScheme::Light,
                ..Default::default()
            },
            None,
        );
        let dark = resolve(
            &AppearanceConfig {
                color_scheme: ColorScheme::Dark,
                ..Default::default()
            },
            None,
        );
        (light, dark)
    }

    fn appearance_app() -> impl IntoElement {
        let resolved = resolved();
        let palette = shell_palette(&resolved);
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(color(palette.bar))
    }

    #[test]
    fn appearance_state_recolors_component() {
        with_temp_dirs(|| {
            let (light, dark) = light_and_dark();
            let (mut test, mut state) = TestingRunner::new(
                appearance_app,
                Size2D::new(64.0, 64.0),
                |runner| runner.provide_root_context(|| State::create(light)),
                1.0,
            );
            test.sync_and_update();
            let (w, h, data) = decode_png(&test.render());
            assert_eq!((w, h), (64, 64));
            let actual = pixel_at(&data, w, 32, 32);
            let expected = over_white(shell_palette(&light).bar.to_rgba8());
            assert_eq!(actual, expected);

            state.set(dark);
            test.sync_and_update();
            let (w, h, data) = decode_png(&test.render());
            assert_eq!((w, h), (64, 64));
            let actual = pixel_at(&data, w, 32, 32);
            let expected = over_white(shell_palette(&dark).bar.to_rgba8());
            assert_eq!(actual, expected);
        })
    }

    #[test]
    fn surface_backgrounds_follow_the_window_colour_mode_and_scope() {
        use vow_appearance::{WindowColorMode, WindowTintScope};
        use vow_design::{
            BG_WINDOW_LIGHT, PANEL_TINT_WALLPAPER, SHELL_LIGHT_OPAQUE, WINDOW_TINT_ACCENT,
            WINDOW_TINT_WALLPAPER,
        };

        let accent = Rgba {
            r: 0.2,
            g: 0.4,
            b: 0.9,
            a: 1.0,
        };
        let dominant = Rgba {
            r: 0.8,
            g: 0.2,
            b: 0.1,
            a: 1.0,
        };
        let config = |window_color, window_tint_scope| AppearanceConfig {
            color_scheme: ColorScheme::Light,
            accent_color: Some([accent.r, accent.g, accent.b]),
            reduce_transparency: true,
            window_color,
            window_tint_scope,
            ..Default::default()
        };

        let cases: [(&str, WindowColorMode, WindowTintScope, Rgba, Rgba); 6] = [
            (
                "neutral/windows",
                WindowColorMode::Neutral,
                WindowTintScope::Windows,
                BG_WINDOW_LIGHT,
                SHELL_LIGHT_OPAQUE.bar,
            ),
            (
                "neutral/desktop",
                WindowColorMode::Neutral,
                WindowTintScope::Desktop,
                BG_WINDOW_LIGHT,
                SHELL_LIGHT_OPAQUE.bar,
            ),
            (
                "accent/windows",
                WindowColorMode::Accent,
                WindowTintScope::Windows,
                BG_WINDOW_LIGHT.mix(accent, WINDOW_TINT_ACCENT[0]),
                SHELL_LIGHT_OPAQUE.bar,
            ),
            (
                "accent/desktop",
                WindowColorMode::Accent,
                WindowTintScope::Desktop,
                BG_WINDOW_LIGHT.mix(accent, WINDOW_TINT_ACCENT[0]),
                SHELL_LIGHT_OPAQUE.bar.mix(accent, WINDOW_TINT_ACCENT[0]),
            ),
            (
                "wallpaper/windows",
                WindowColorMode::Wallpaper,
                WindowTintScope::Windows,
                BG_WINDOW_LIGHT.mix(dominant, WINDOW_TINT_WALLPAPER[0]),
                SHELL_LIGHT_OPAQUE.bar,
            ),
            (
                "wallpaper/desktop",
                WindowColorMode::Wallpaper,
                WindowTintScope::Desktop,
                BG_WINDOW_LIGHT.mix(dominant, WINDOW_TINT_WALLPAPER[0]),
                SHELL_LIGHT_OPAQUE
                    .bar
                    .mix(dominant, PANEL_TINT_WALLPAPER[0]),
            ),
        ];

        for (name, window_color, window_tint_scope, toplevel, layer) in cases {
            let resolved = resolve(&config(window_color, window_tint_scope), Some(dominant));
            assert_eq!(
                surface_background(&resolved, SurfaceKind::Toplevel),
                toplevel,
                "{name}: toplevel background"
            );
            assert_eq!(
                surface_background(&resolved, SurfaceKind::LayerShell),
                layer,
                "{name}: layer-shell background"
            );
        }
    }

    #[test]
    fn selection_colours_follow_the_accent() {
        let orange = Rgba {
            r: 0.95,
            g: 0.5,
            b: 0.1,
            a: 1.0,
        };
        let config = |scheme| AppearanceConfig {
            color_scheme: scheme,
            accent_color: Some([orange.r, orange.g, orange.b]),
            ..Default::default()
        };

        let light = resolve(&config(ColorScheme::Light), None);
        assert_eq!(light.menu.hover_bg, orange, "light menu selection");
        assert_eq!(light.menu.search_ring, orange, "light focus ring");
        assert_eq!(
            light.sidebar.palette.selected_bg, orange,
            "light sidebar selection"
        );
        assert_eq!(
            light.sidebar.palette.item_icon, orange,
            "light sidebar icon"
        );
        assert_eq!(
            light.sidebar.palette.selected_ink,
            vow_design::LIGHT.on_accent
        );
        assert_eq!(
            crate::sidebar::item_icon_color(true, light.sidebar.palette),
            vow_design::LIGHT.on_accent,
            "light selected sidebar icon"
        );
        assert_eq!(
            crate::sidebar::item_icon_color(false, light.sidebar.palette),
            orange,
            "light unselected sidebar icon"
        );
        assert_ne!(light.menu.hover_bg, vow_design::LIGHT.accent);

        let dark = resolve(&config(ColorScheme::Dark), None);
        assert_eq!(dark.menu.hover_bg, orange, "dark menu selection");
        assert_eq!(dark.menu.search_ring, orange, "dark focus ring");
        assert_eq!(
            dark.sidebar.palette.selected_bg,
            orange.mix(dark.sidebar.bg, vow_design::SETTINGS.selected_dark_mix),
            "dark sidebar selection"
        );
        assert_eq!(dark.sidebar.palette.item_icon, orange, "dark sidebar icon");
        assert_eq!(
            dark.sidebar.palette.selected_ink,
            vow_design::DARK.on_accent
        );
        assert_eq!(
            crate::sidebar::item_icon_color(true, dark.sidebar.palette),
            vow_design::DARK.on_accent,
            "dark selected sidebar icon"
        );
        assert_eq!(
            crate::sidebar::item_icon_color(false, dark.sidebar.palette),
            orange,
            "dark unselected sidebar icon"
        );
    }

    #[test]
    fn menu_background_follows_the_opening_surface() {
        use vow_appearance::{WindowColorMode, WindowTintScope};

        let accent = Rgba {
            r: 0.9,
            g: 0.1,
            b: 0.1,
            a: 1.0,
        };
        let config = |scheme, accent: Rgba, scope| AppearanceConfig {
            color_scheme: scheme,
            accent_color: Some([accent.r, accent.g, accent.b]),
            reduce_transparency: true,
            window_color: WindowColorMode::Accent,
            window_tint_scope: scope,
            ..Default::default()
        };
        let light = resolve(
            &config(ColorScheme::Light, accent, WindowTintScope::Desktop),
            None,
        );
        let windows = resolve(
            &config(ColorScheme::Light, accent, WindowTintScope::Windows),
            None,
        );
        let dark = resolve(
            &config(ColorScheme::Dark, accent, WindowTintScope::Desktop),
            None,
        );
        let dark_tint = resolve(
            &config(ColorScheme::DarkTint, accent, WindowTintScope::Desktop),
            None,
        );

        let expected = |resolved: &Resolved, kind: SurfaceKind, neutral: Rgba| {
            let parent = surface_background(resolved, kind);
            let menu = resolved.menu.bg;
            let lift =
                |parent: f32, menu: f32, neutral: f32| (parent + menu - neutral).clamp(0.0, 1.0);
            Rgba {
                r: lift(parent.r, menu.r, neutral.r),
                g: lift(parent.g, menu.g, neutral.g),
                b: lift(parent.b, menu.b, neutral.b),
                a: menu.a,
            }
        };

        for (name, resolved, kind, neutral) in [
            (
                "light/toplevel",
                light,
                SurfaceKind::Toplevel,
                BG_WINDOW_LIGHT,
            ),
            (
                "light/layer",
                light,
                SurfaceKind::LayerShell,
                SHELL_LIGHT_OPAQUE.bar,
            ),
            ("dark/toplevel", dark, SurfaceKind::Toplevel, BG_WINDOW_DARK),
            (
                "dark/layer",
                dark,
                SurfaceKind::LayerShell,
                SHELL_DARK_OPAQUE.bar,
            ),
            (
                "dark-tint/toplevel",
                dark_tint,
                SurfaceKind::Toplevel,
                BG_WINDOW_DARK_TINT,
            ),
            (
                "dark-tint/layer",
                dark_tint,
                SurfaceKind::LayerShell,
                SHELL_DARK_TINT_OPAQUE.bar,
            ),
        ] {
            assert_eq!(
                menu_palette(&resolved, kind).bg,
                expected(&resolved, kind, neutral),
                "{name}: menu must keep its raised offset and inherit the tint"
            );
        }

        assert_eq!(
            menu_palette(&windows, SurfaceKind::LayerShell).bg,
            windows.menu.bg,
            "a layer-shell menu must stay neutral when only windows are tinted"
        );

        let white = Rgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        let pale = resolve(
            &config(ColorScheme::Light, white, WindowTintScope::Desktop),
            None,
        );
        let pale_menu = menu_palette(&pale, SurfaceKind::LayerShell).bg;
        assert_eq!(
            pale_menu,
            expected(&pale, SurfaceKind::LayerShell, SHELL_LIGHT_OPAQUE.bar),
            "a light accent must clamp every channel to 1"
        );
        assert!(pale_menu.r <= 1.0 && pale_menu.g <= 1.0 && pale_menu.b <= 1.0);
    }

    #[test]
    fn tinted_menu_separators_stay_visible() {
        use vow_appearance::WindowColorMode;
        use vow_design::ACCENT_SWATCHES;

        fn linear(channel: f32) -> f32 {
            if channel <= 0.03928 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        }

        fn luminance(color: Rgba) -> f32 {
            0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
        }

        fn contrast(a: Rgba, b: Rgba) -> f32 {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        }

        let wallpapers = [
            ("dark", Rgba::rgb8(13, 13, 13)),
            ("mid", Rgba::rgb8(128, 115, 102)),
            ("light", Rgba::rgb8(235, 230, 219)),
        ];
        let config = |scheme, mode, accent: Option<[f32; 3]>| AppearanceConfig {
            color_scheme: scheme,
            accent_color: accent,
            reduce_transparency: true,
            window_color: mode,
            window_tint_scope: WindowTintScope::Desktop,
            ..Default::default()
        };

        for scheme in [ColorScheme::Light, ColorScheme::Dark, ColorScheme::DarkTint] {
            for (name, accent) in ACCENT_SWATCHES {
                let resolved = resolve(
                    &config(
                        scheme,
                        WindowColorMode::Accent,
                        Some([accent.r, accent.g, accent.b]),
                    ),
                    None,
                );
                for kind in [SurfaceKind::Toplevel, SurfaceKind::LayerShell] {
                    let palette = menu_palette(&resolved, kind);
                    let ratio = contrast(palette.separator_color, palette.bg);
                    assert!(
                        ratio >= 1.25,
                        "{scheme:?}/acento {name}/{kind:?}: {ratio:.3}:1"
                    );
                }
            }

            for (name, tint) in wallpapers {
                let resolved = resolve(
                    &config(scheme, WindowColorMode::Wallpaper, None),
                    Some(tint),
                );
                for kind in [SurfaceKind::Toplevel, SurfaceKind::LayerShell] {
                    let palette = menu_palette(&resolved, kind);
                    let ratio = contrast(palette.separator_color, palette.bg);
                    assert!(
                        ratio >= 1.25,
                        "{scheme:?}/fondo {name}/{kind:?}: {ratio:.3}:1"
                    );
                }
            }

            let neutral = resolve(&config(scheme, WindowColorMode::Neutral, None), None);
            for kind in [SurfaceKind::Toplevel, SurfaceKind::LayerShell] {
                assert_eq!(
                    menu_palette(&neutral, kind).separator_color,
                    neutral.menu.separator_color,
                    "{scheme:?}/{kind:?}: neutro keeps today's colour"
                );
            }
        }
    }

    #[test]
    fn accent_state_recolors_sidebar_selection() {
        with_temp_dirs(|| {
            let mut config = AppearanceConfig {
                color_scheme: ColorScheme::Light,
                accent_color: Some([0.95, 0.5, 0.1]),
                ..Default::default()
            };
            let orange = resolve(&config, None);
            config.accent_color = Some([0.5, 0.0, 1.0]);
            let purple = resolve(&config, None);

            assert_eq!(orange.sidebar.palette.selected_bg, orange.accent);
            assert_eq!(purple.sidebar.palette.selected_bg, purple.accent);

            fn sidebar_selection_app() -> impl IntoElement {
                let resolved = resolved();
                let palette = resolved.sidebar.palette;
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .background(color(palette.selected_bg))
            }

            let (mut test, mut state) = TestingRunner::new(
                sidebar_selection_app,
                Size2D::new(64.0, 64.0),
                |runner| runner.provide_root_context(|| State::create(orange)),
                1.0,
            );
            test.sync_and_update();
            let (w, h, data) = decode_png(&test.render());
            assert_eq!((w, h), (64, 64));
            let actual = pixel_at(&data, w, 32, 32);
            let expected = over_white(orange.sidebar.palette.selected_bg.to_rgba8());
            assert_eq!(actual, expected);

            state.set(purple);
            test.sync_and_update();
            let (w, h, data) = decode_png(&test.render());
            assert_eq!((w, h), (64, 64));
            let actual = pixel_at(&data, w, 32, 32);
            let expected = over_white(purple.sidebar.palette.selected_bg.to_rgba8());
            assert_eq!(actual, expected);
        })
    }
}
