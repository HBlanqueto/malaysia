use bk::prelude::*;
use ctdesign::SETTINGS;
use torin::prelude::Position;

use bk::theme::{SurfaceKind, color as rgba, surface_background};
use ctappearance::menubar::{MenubarConfig, MenubarHeight, MenubarStyle, classic_bar, colors_over};
use ctappearance::{AppearanceConfig, ColorScheme, Resolved, resolve};
use ctdesign::{BACKDROP, MENUBAR_HEIGHT, Rgba};
use torin::prelude::Gaps;

use crate::wallpaper::{Picture, appearance, over_artwork, preview};

fn preview_for_theme(
    base_config: &AppearanceConfig,
    scheme: ColorScheme,
    dominant: Option<Rgba>,
) -> Resolved {
    let test_config = AppearanceConfig {
        color_scheme: scheme,
        ..base_config.clone()
    };
    resolve(&test_config, dominant)
}

pub fn theme_preview(
    scheme: ColorScheme,
    base_config: &AppearanceConfig,
    dominant: Option<Rgba>,
) -> Rect {
    let theme = preview_for_theme(base_config, scheme, dominant);
    wallpaper_background(scheme.is_dark()).child(
        over_artwork(Position::new_absolute())
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .child(preview_menubar_dot(&theme))
            .child(preview_menu(&theme))
            .child(preview_window(&theme)),
    )
}

pub fn automatic_preview(base_config: &AppearanceConfig, dominant: Option<Rgba>) -> Rect {
    let half = SETTINGS.card_width as f32 / 2.0;
    let side = |scheme: ColorScheme, left: f32| {
        rect()
            .position(Position::new_absolute().left(left))
            .width(Size::px(half))
            .height(Size::percent(100.0))
            .overflow(Overflow::Clip)
            .child(
                rect()
                    .position(Position::new_absolute().left(-left))
                    .width(Size::px(SETTINGS.card_width as f32))
                    .height(Size::px(SETTINGS.card_height as f32))
                    .child(theme_preview(scheme, base_config, dominant)),
            )
    };
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .child(side(ColorScheme::Light, 0.0))
        .child(side(ColorScheme::Dark, half))
}

pub(crate) fn wallpaper_background(dark: bool) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .corner_radius(SETTINGS.card_radius as f32)
        .overflow(Overflow::Clip)
        .maybe_child(preview(None, appearance(dark)).map(|path| Picture {
            path,
            aspect: AspectRatio::Max,
        }))
}

fn preview_menubar_dot(resolved: &Resolved) -> Rect {
    let inset = SETTINGS.preview_menubar_dot_inset as f32;
    let dot = SETTINGS.preview_menubar_dot as f32;
    rect()
        .position(Position::new_absolute().left(inset).top(inset))
        .width(Size::px(dot))
        .height(Size::px(dot))
        .corner_radius(dot / 2.0)
        .background(rgba(colors_over(resolved.accent).ink))
}

fn preview_menu(resolved: &Resolved) -> VStack {
    let padding = SETTINGS.preview_menu_padding as f32;
    VStack::new()
        .main_align(Alignment::Start)
        .cross_align(Alignment::Start)
        .position(
            Position::new_absolute()
                .left(SETTINGS.preview_menu_x as f32)
                .top(SETTINGS.preview_menu_y as f32),
        )
        .width(Size::px(SETTINGS.preview_menu_w as f32))
        .height(Size::px(SETTINGS.preview_menu_h as f32))
        .padding(padding)
        .spacing(padding)
        .corner_radius(SETTINGS.preview_radius as f32)
        .background(rgba(Rgba {
            a: BACKDROP.island,
            ..resolved.surfaces.window
        }))
        .child(preview_menu_line(
            SETTINGS.preview_menu_accent_h,
            resolved.accent,
        ))
        .child(preview_menu_line(
            SETTINGS.preview_menu_line_h,
            resolved.surfaces.control,
        ))
}

fn preview_menu_line(height: u32, color: Rgba) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::px(height as f32))
        .corner_radius(SETTINGS.preview_menu_line_radius as f32)
        .background(rgba(color))
}

fn preview_window(resolved: &Resolved) -> Rect {
    let radius = SETTINGS.preview_window_radius as f32;
    let (x, y) = (
        SETTINGS.preview_window_x as f32,
        SETTINGS.preview_window_y as f32,
    );
    let light = SETTINGS.preview_light as f32;
    let corner = resolved.traffic_shape.corner_radius(light);
    rect()
        .position(Position::new_absolute().left(x).top(y))
        .width(Size::px(SETTINGS.card_width as f32 - x + radius))
        .height(Size::px(SETTINGS.card_height as f32 - y + radius))
        .corner_radius(radius)
        .background(rgba(resolved.surfaces.window))
        .child(
            HStack::new()
                .main_align(Alignment::Start)
                .cross_align(Alignment::Start)
                .position(
                    Position::new_absolute()
                        .left(SETTINGS.preview_dots_x as f32)
                        .top(SETTINGS.preview_dots_y as f32),
                )
                .spacing(SETTINGS.preview_dot_gap as f32)
                .children(resolved.lights.iter().map(|color| {
                    rect()
                        .width(Size::px(light))
                        .height(Size::px(light))
                        .corner_radius(corner)
                        .background(rgba(*color))
                        .into_element()
                })),
        )
}

pub fn menubar_preview(
    style: MenubarStyle,
    height: MenubarHeight,
    with_window: bool,
    resolved: &Resolved,
) -> Rect {
    let config = MenubarConfig {
        bar_height: height,
        ..MenubarConfig::default()
    };
    let bar_height = SETTINGS.preview_bar_h as f32 * config.height() as f32 / MENUBAR_HEIGHT as f32;
    let solid = surface_background(resolved, SurfaceKind::LayerShell);
    let over = colors_over(resolved.accent);
    let (background, ink) = match style {
        MenubarStyle::Solid => (
            classic_bar(solid, resolved.reduce_transparency),
            resolved.shell.text,
        ),
        MenubarStyle::Translucent => (over.glass, over.ink),
        MenubarStyle::Floating => (
            Rgba {
                a: 0.0,
                ..over.glass
            },
            over.ink,
        ),
    };
    let padding = SETTINGS.preview_bar_padding as f32;
    let inset = SETTINGS.preview_app_inset as f32;
    let window = rect()
        .position(Position::new_absolute().top(bar_height + inset).left(inset))
        .width(Size::px(SETTINGS.card_width as f32 - 2.0 * inset))
        .height(Size::px(SETTINGS.card_height as f32))
        .corner_radius(SETTINGS.preview_window_radius as f32)
        .background(rgba(resolved.surfaces.window));
    let bar = HStack::new()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::fill())
        .height(Size::px(bar_height))
        .padding(Gaps::new(0.0, padding, 0.0, padding))
        .main_align(Alignment::SpaceBetween)
        .cross_align(Alignment::Center)
        .background(rgba(background))
        .child(preview_bar_items(4, ink))
        .child(preview_bar_items(2, ink));
    wallpaper_background(resolved.scheme.is_dark()).child(
        over_artwork(Position::new_absolute())
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .maybe_child(with_window.then_some(window))
            .child(bar),
    )
}

fn preview_bar_items(count: usize, ink: Rgba) -> HStack {
    HStack::new()
        .main_align(Alignment::Start)
        .cross_align(Alignment::Center)
        .spacing(SETTINGS.preview_bar_item_gap as f32)
        .children((0..count).map(|_| {
            rect()
                .width(Size::px(SETTINGS.preview_bar_item_w as f32))
                .height(Size::px(SETTINGS.preview_menu_line_h as f32))
                .corner_radius(SETTINGS.preview_menu_line_radius as f32)
                .background(rgba(ink))
                .into_element()
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctappearance::{AppearanceConfig, ColorScheme, WindowColorMode};
    use ctdesign::Rgba;

    #[test]
    fn auto_theme_preview_keeps_accent_when_window_color_is_neutral() {
        let accent = Rgba {
            r: 0.0,
            g: 0.3,
            b: 0.8,
            a: 1.0,
        };
        let config = AppearanceConfig {
            accent_color: Some([accent.r, accent.g, accent.b]),
            window_color: WindowColorMode::Neutral,
            ..AppearanceConfig::default()
        };
        let preview = preview_for_theme(&config, ColorScheme::Light, None);
        assert_eq!(preview.accent, accent);
        assert!(preview.window_tint.is_none());
    }
}
