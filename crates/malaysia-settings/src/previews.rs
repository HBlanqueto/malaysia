use ctdesign::{SETTINGS, TITLEBAR};
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Position, Size};

use bk::theme::color as rgba;
use ctappearance::{
    AppearanceConfig, ColorScheme, Resolved, TrafficLightShape, WindowColorMode, resolve,
};
use ctdesign::Rgba;

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
    let accent = preview_for_theme(base_config, scheme, dominant).accent;
    let light = preview_for_theme(base_config, ColorScheme::Light, dominant);
    let dark = preview_for_theme(base_config, ColorScheme::Dark, dominant);

    let mut preview = gradient_background(accent);
    match scheme {
        ColorScheme::Auto => {
            preview = preview
                .child(preview_panel(&light, Alignment::Start))
                .child(preview_panel(&dark, Alignment::End))
                .child(preview_menu(&light, accent))
                .child(preview_app(&dark));
        }
        ColorScheme::Light => {
            preview = preview
                .child(preview_panel(&light, Alignment::Start))
                .child(preview_menu(&light, accent))
                .child(preview_app(&light));
        }
        ColorScheme::Dark | ColorScheme::DarkTint => {
            preview = preview
                .child(preview_panel(&dark, Alignment::Start))
                .child(preview_menu(&dark, accent))
                .child(preview_app(&dark));
        }
    }
    preview
}

fn gradient_background(accent: Rgba) -> Rect {
    let gradient = LinearGradient::new()
        .angle(SETTINGS.preview_wall_angle as f32)
        .stop((rgba(accent.lighten(SETTINGS.preview_wall_light)), 0.0))
        .stop((rgba(accent), 50.0))
        .stop((rgba(accent.darken(SETTINGS.preview_wall_dark)), 100.0));
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .corner_radius(SETTINGS.card_radius as f32)
        .background(gradient)
        .overflow(Overflow::Clip)
}

fn preview_panel(resolved: &Resolved, align: Alignment) -> Rect {
    let width = SETTINGS.preview_panel_w as f32;
    let left = if align == Alignment::End { width } else { 0.0 };
    rect()
        .position(Position::new_absolute().top(0.0).left(left))
        .width(Size::px(width))
        .height(Size::px(SETTINGS.preview_panel_h as f32))
        .background(rgba(resolved.surfaces.window))
}

fn preview_menu(resolved: &Resolved, accent: Rgba) -> Rect {
    let padding = SETTINGS.preview_menu_padding as f32;
    rect()
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
        .background(rgba(resolved.surfaces.window))
        .direction(Direction::Vertical)
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(SETTINGS.preview_panel_h as f32))
                .corner_radius(SETTINGS.preview_menu_line_radius as f32)
                .background(rgba(accent)),
        )
        .child(preview_menu_line(resolved))
        .child(preview_menu_line(resolved))
}

fn preview_menu_line(resolved: &Resolved) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::px(SETTINGS.preview_menu_line_h as f32))
        .corner_radius(SETTINGS.preview_menu_line_radius as f32)
        .background(rgba(resolved.surfaces.control))
}

fn preview_app(resolved: &Resolved) -> Rect {
    let inset = SETTINGS.preview_app_inset as f32;
    let dot = SETTINGS.preview_light as f32;
    let dot_gap = SETTINGS.preview_dot_gap as f32;
    rect()
        .position(Position::new_absolute().right(inset).bottom(inset))
        .width(Size::px(SETTINGS.preview_app_w as f32))
        .height(Size::px(SETTINGS.preview_app_h as f32))
        .corner_radius(SETTINGS.preview_radius as f32)
        .background(rgba(resolved.surfaces.window))
        .overflow(Overflow::Clip)
        .child(
            rect()
                .position(Position::new_absolute().top(0.0).bottom(0.0).left(0.0))
                .width(Size::px(SETTINGS.preview_app_side_w as f32))
                .background(rgba(resolved.surfaces.sunken)),
        )
        .child(
            rect()
                .position(
                    Position::new_absolute()
                        .left(SETTINGS.preview_dots_inset as f32)
                        .top(SETTINGS.preview_dots_inset as f32),
                )
                .direction(Direction::Horizontal)
                .spacing(dot_gap)
                .children(resolved.lights.iter().map(|color| {
                    rect()
                        .width(Size::px(dot))
                        .height(Size::px(dot))
                        .corner_radius(SETTINGS.preview_dot_radius as f32)
                        .background(rgba(*color))
                        .into_element()
                })),
        )
}

pub fn window_color_preview(
    base_config: &AppearanceConfig,
    mode: WindowColorMode,
    dominant: Option<Rgba>,
) -> Rect {
    let preview_resolved = resolve(
        &AppearanceConfig {
            window_color: mode,
            ..base_config.clone()
        },
        dominant,
    );
    let window_bg = preview_resolved.surfaces.window;
    let sunken = preview_resolved.surfaces.sunken;
    let x = SETTINGS.preview_tint_window_x as f32;
    let y = SETTINGS.preview_tint_window_y as f32;
    let side_w = SETTINGS.preview_tint_side_w as f32;
    let padding = SETTINGS.preview_radius as f32;
    let width = SETTINGS.card_width as f32 - 2.0 * x;
    let height = SETTINGS.card_height as f32 - y + SETTINGS.preview_panel_h as f32;

    gradient_background(preview_resolved.accent).child(
        rect()
            .position(Position::new_absolute().top(y).left(x))
            .width(Size::px(width))
            .height(Size::px(height))
            .corner_radius(SETTINGS.preview_window_radius as f32)
            .background(rgba(window_bg))
            .overflow(Overflow::Clip)
            .child(
                rect()
                    .position(Position::new_absolute().top(padding).left(padding))
                    .width(Size::px(side_w))
                    .height(Size::px(height - 2.0 * padding))
                    .corner_radius(SETTINGS.preview_radius as f32)
                    .background(rgba(sunken)),
            ),
    )
}

pub fn traffic_shape_preview(resolved: &Resolved, shape: TrafficLightShape) -> Rect {
    let diameter = TITLEBAR.buttons.diameter as f32;
    let radius = shape.corner_radius(diameter);
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .corner_radius(SETTINGS.card_radius as f32)
        .background(rgba(resolved.surfaces.sunken))
        .direction(Direction::Horizontal)
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .spacing(TITLEBAR.buttons.spacing as f32)
        .children(resolved.lights.map(|light| {
            rect()
                .width(Size::px(diameter))
                .height(Size::px(diameter))
                .corner_radius(radius)
                .background(rgba(light))
                .into_element()
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctappearance::{AppearanceConfig, ColorScheme, WindowColorMode};
    use ctdesign::Rgba;

    #[test]
    fn auto_theme_preview_keeps_window_color_and_uses_dominant() {
        let dominant = Rgba {
            r: 0.8,
            g: 0.2,
            b: 0.1,
            a: 1.0,
        };
        let config = AppearanceConfig {
            window_color: WindowColorMode::Wallpaper,
            ..AppearanceConfig::default()
        };
        let preview = preview_for_theme(&config, ColorScheme::Auto, Some(dominant));
        assert_ne!(preview.accent, dominant);
        assert!(preview.window_tint.is_some());
    }

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
        let preview = preview_for_theme(&config, ColorScheme::Auto, None);
        assert_eq!(preview.accent, accent);
        assert!(preview.window_tint.is_none());
    }
}
