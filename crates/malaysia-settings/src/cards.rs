//! Tarjetas de previsualización de Ajustes (tema y color de ventanas).

use barrier_kit::ProbeExt;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Position, Size};
use vow_design::SETTINGS;

use crate::state::{
    AppearanceConfig, ColorScheme, Resolved, Rgba, WindowColorMode, load_appearance, resolve, rgba,
};

/// Resolves the preview for a theme card while keeping the rest of the
/// current config (window colour mode, accent, etc.).
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

/// Construye una tarjeta de tema (Claro/Oscuro/Automático).
#[allow(clippy::too_many_arguments)]
pub fn theme_card(
    caption: &'static str,
    scheme: ColorScheme,
    selected_scheme: ColorScheme,
    card_bg: Rgba,
    dominant: Option<Rgba>,
    accent: Rgba,
    scheme_palette: vow_design::Palette,
    base_config: &AppearanceConfig,
    probe: &'static str,
    preview_probe: &'static str,
    on_select: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let is_selected = selected_scheme == scheme;
    let preview_resolved = preview_for_theme(base_config, scheme, dominant);

    selectable_card(
        caption,
        card_bg,
        theme_preview(scheme, base_config, dominant, &preview_resolved),
        is_selected,
        accent,
        preview_resolved.surfaces.surface,
        scheme_palette,
        probe,
        preview_probe,
        on_select,
    )
}

/// Miniatura de tema: degradado del acento con panel, menú y mini app.
pub fn theme_preview(
    scheme: ColorScheme,
    base_config: &AppearanceConfig,
    dominant: Option<Rgba>,
    preview_resolved: &Resolved,
) -> Rect {
    let accent = preview_resolved.accent;
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
        .background(gradient)
        .overflow(Overflow::Clip)
}

fn preview_panel(resolved: &Resolved, align: Alignment) -> Rect {
    let width = SETTINGS.card_width as f32 / 2.0;
    let left = if align == Alignment::End { width } else { 0.0 };
    rect()
        .position(Position::new_absolute().top(0.0).left(left))
        .width(Size::px(width))
        .height(Size::px(SETTINGS.preview_panel_h as f32))
        .background(rgba(resolved.surfaces.window))
}

fn preview_menu(resolved: &Resolved, accent: Rgba) -> Rect {
    let padding = SETTINGS.preview_panel_h as f32 / 2.0;
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
                .corner_radius(SETTINGS.preview_radius as f32 / 2.0)
                .background(rgba(accent)),
        )
        .child(preview_menu_line(resolved))
        .child(preview_menu_line(resolved))
}

fn preview_menu_line(resolved: &Resolved) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::px(SETTINGS.preview_panel_h as f32 / 2.0))
        .corner_radius(SETTINGS.preview_radius as f32 / 3.0)
        .background(rgba(resolved.surfaces.control))
}

fn preview_app(resolved: &Resolved) -> Rect {
    let inset = SETTINGS.preview_app_inset as f32;
    let dot = SETTINGS.preview_light as f32;
    let dot_gap = SETTINGS.preview_panel_h as f32 / 4.0;
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
                        .left(SETTINGS.preview_panel_h as f32 / 2.0)
                        .top(SETTINGS.preview_panel_h as f32 / 2.0),
                )
                .direction(Direction::Horizontal)
                .spacing(dot_gap)
                .children(resolved.lights.iter().map(|color| {
                    rect()
                        .width(Size::px(dot))
                        .height(Size::px(dot))
                        .corner_radius(SETTINGS.preview_radius as f32 / 3.0)
                        .background(rgba(*color))
                        .into_element()
                })),
        )
}

/// Tarjeta de color de ventanas (Neutro/Acento/Fondo).
#[allow(clippy::too_many_arguments)]
pub fn window_color_card(
    caption: &'static str,
    mode: WindowColorMode,
    selected: WindowColorMode,
    card_bg: Rgba,
    resolved: Resolved,
    dominant: Option<Rgba>,
    probe: &'static str,
    preview_probe: &'static str,
    on_select: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let base_config = load_appearance();
    let test_config = AppearanceConfig {
        window_color: mode,
        ..base_config
    };
    let preview_resolved = resolve(&test_config, dominant);
    let is_selected = selected == mode;
    let scheme_palette = crate::state::palette(resolved.scheme);

    selectable_card(
        caption,
        card_bg,
        window_color_preview(&preview_resolved),
        is_selected,
        resolved.accent,
        resolved.surfaces.surface,
        scheme_palette,
        probe,
        preview_probe,
        on_select,
    )
}

/// Miniatura de color de ventanas sobre el degradado del acento.
pub fn window_color_preview(preview_resolved: &Resolved) -> Rect {
    let window_bg = preview_resolved.surfaces.window;
    let sunken = preview_resolved.surfaces.sunken;
    let x = SETTINGS.preview_tint_window_x as f32;
    let y = SETTINGS.preview_tint_window_y as f32;
    let side_w = SETTINGS.preview_tint_side_w as f32;
    let padding = SETTINGS.preview_radius as f32;
    let bottom = -(SETTINGS.preview_panel_h as f32);

    gradient_background(preview_resolved.accent).child(
        rect()
            .position(
                Position::new_absolute()
                    .top(y)
                    .left(x)
                    .right(x)
                    .bottom(bottom),
            )
            .corner_radius(SETTINGS.preview_window_radius as f32)
            .background(rgba(window_bg))
            .overflow(Overflow::Clip)
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .top(padding)
                            .left(padding)
                            .bottom(padding),
                    )
                    .width(Size::px(side_w))
                    .corner_radius(SETTINGS.preview_radius as f32)
                    .background(rgba(sunken)),
            ),
    )
}

/// Tarjeta seleccionable con miniatura, anillo de selección y pie.
#[allow(clippy::too_many_arguments)]
pub fn selectable_card(
    caption: &'static str,
    card_bg: Rgba,
    preview: Rect,
    is_selected: bool,
    accent: Rgba,
    bg_surface: Rgba,
    scheme_palette: vow_design::Palette,
    probe: &'static str,
    preview_probe: &'static str,
    on_select: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let card = rect()
        .width(Size::px(SETTINGS.card_width as f32))
        .height(Size::px(SETTINGS.card_height as f32))
        .corner_radius(SETTINGS.card_radius as f32)
        .background(rgba(card_bg))
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(rgba(SETTINGS.card_outline)),
        )
        .overflow(Overflow::Clip)
        .child(preview.probe(preview_probe));

    let wrapper = rect()
        .width(Size::auto())
        .height(Size::auto())
        .corner_radius(SETTINGS.card_radius as f32)
        .background(rgba(bg_surface))
        .border(
            Border::new()
                .width(if is_selected {
                    SETTINGS.card_ring as f32
                } else {
                    0.0
                })
                .alignment(BorderAlignment::Outer)
                .fill(rgba(accent)),
        )
        .child(card);

    let caption_weight = if is_selected {
        FontWeight::SEMI_BOLD
    } else {
        FontWeight::from(SETTINGS.card_caption_font.weight as i32)
    };
    let caption_color = if is_selected {
        scheme_palette.ink_strong
    } else {
        scheme_palette.text.unfocused
    };

    rect()
        .width(Size::auto())
        .height(Size::auto())
        .direction(Direction::Vertical)
        .spacing(SETTINGS.card_caption_gap as f32)
        .cross_align(Alignment::Center)
        .child(wrapper)
        .child(
            label()
                .text(caption)
                .line_height(
                    SETTINGS.card_caption_font.line_height as f32
                        / SETTINGS.card_caption_font.size as f32,
                )
                .font_size(SETTINGS.card_caption_font.size as f32)
                .font_weight(caption_weight)
                .color(rgba(caption_color))
                .max_lines(1),
        )
        .probe(probe)
        .on_press(on_select)
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vow_appearance::{AppearanceConfig, ColorScheme, WindowColorMode};
    use vow_design::Rgba;

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
        assert_eq!(preview.accent, dominant);
        assert!(preview.window_tint.is_some());
    }

    #[test]
    fn auto_theme_preview_keeps_accent_when_window_color_is_neutral() {
        let accent = Rgba {
            r: 0.2,
            g: 0.5,
            b: 0.9,
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
