//! Tarjetas de previsualización de Ajustes (tema y color de ventanas).

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
        on_select,
    )
}

/// Miniatura de tema: ventana con barra de acento y semáforos.
pub fn theme_preview(
    scheme: ColorScheme,
    base_config: &AppearanceConfig,
    dominant: Option<Rgba>,
    preview_resolved: &Resolved,
) -> Rect {
    let window_bg = preview_resolved.surfaces.window;
    let accent = preview_resolved.accent;
    let card_w = SETTINGS.card_width as f32;
    let card_h = SETTINGS.card_height as f32;
    let radius = SETTINGS.preview_window_radius as f32;
    let x = SETTINGS.preview_window_x as f32;
    let y = SETTINGS.preview_window_y as f32;
    let bar_w = SETTINGS.preview_bar_w as f32;
    let bar_h = SETTINGS.preview_bar_h as f32;
    let inset = SETTINGS.preview_bar_inset as f32;
    let light = SETTINGS.preview_light as f32;
    let light_gap = SETTINGS.preview_light_gap as f32;

    let mut window = rect()
        .width(Size::px(card_w))
        .height(Size::px(card_h))
        .corner_radius(radius)
        .background(rgba(window_bg))
        .overflow(Overflow::Clip)
        .position(Position::new_absolute().top(y).left(x));

    if scheme == ColorScheme::Auto {
        let light = preview_for_theme(base_config, ColorScheme::Light, dominant)
            .surfaces
            .window;
        let dark = preview_for_theme(base_config, ColorScheme::Dark, dominant)
            .surfaces
            .window;
        window = window
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0).bottom(0.0))
                    .width(Size::px(card_w / 2.0))
                    .background(rgba(light)),
            )
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).right(0.0).bottom(0.0))
                    .width(Size::px(card_w / 2.0))
                    .background(rgba(dark)),
            );
    }

    window
        .child(
            rect()
                .position(Position::new_absolute().top(inset).left(inset))
                .width(Size::px(bar_w))
                .height(Size::px(bar_h))
                .corner_radius((bar_h / 2.0).max(0.0))
                .background(rgba(accent)),
        )
        .child(
            rect()
                .position(Position::new_absolute().bottom(inset).left(inset))
                .width(Size::auto())
                .height(Size::px(light))
                .direction(Direction::Horizontal)
                .spacing(light_gap)
                .children(preview_resolved.lights.iter().map(|color| {
                    rect()
                        .width(Size::px(light))
                        .height(Size::px(light))
                        .corner_radius((light / 2.0).max(0.0))
                        .background(rgba(*color))
                        .into_element()
                })),
        )
}

/// Tarjeta de color de ventanas (Neutro/Acento/Fondo).
pub fn window_color_card(
    caption: &'static str,
    mode: WindowColorMode,
    selected: WindowColorMode,
    card_bg: Rgba,
    resolved: Resolved,
    dominant: Option<Rgba>,
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
        on_select,
    )
}

/// Miniatura de color de ventanas.
pub fn window_color_preview(preview_resolved: &Resolved) -> Rect {
    let window_bg = preview_resolved.surfaces.window;
    let sunken = preview_resolved.surfaces.sunken;
    let x = SETTINGS.preview_tint_window_x as f32;
    let y = SETTINGS.preview_tint_window_y as f32;
    let side_w = SETTINGS.preview_tint_side_w as f32;
    let card_w = SETTINGS.card_width as f32;
    let card_h = SETTINGS.card_height as f32;
    let radius = SETTINGS.preview_window_radius as f32;
    let light = SETTINGS.preview_light as f32;
    let light_gap = SETTINGS.preview_light_gap as f32;
    let inset = SETTINGS.preview_bar_inset as f32;

    rect()
        .width(Size::px(card_w))
        .height(Size::px(card_h))
        .corner_radius(radius)
        .background(rgba(window_bg))
        .overflow(Overflow::Clip)
        .position(Position::new_absolute().top(y).left(x))
        .child(
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0).bottom(0.0))
                .width(Size::px(side_w))
                .background(rgba(sunken)),
        )
        .child(
            rect()
                .position(Position::new_absolute().top(inset).left(inset))
                .width(Size::auto())
                .height(Size::px(light))
                .direction(Direction::Horizontal)
                .spacing(light_gap)
                .children(preview_resolved.lights.iter().map(|color| {
                    rect()
                        .width(Size::px(light))
                        .height(Size::px(light))
                        .corner_radius((light / 2.0).max(0.0))
                        .background(rgba(*color))
                        .into_element()
                })),
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
        .child(preview);

    let wrapper = rect()
        .width(Size::auto())
        .height(Size::auto())
        .padding(SETTINGS.card_ring_gap as f32)
        .corner_radius((SETTINGS.card_radius + SETTINGS.card_ring_gap) as f32)
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
                .font_size(SETTINGS.card_caption_font.size as f32)
                .font_weight(caption_weight)
                .color(rgba(caption_color))
                .max_lines(1),
        )
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
