//! Tarjetas de previsualización de Ajustes (tema y color de ventanas).

use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Position, Size};
use vow_design::{BG_WINDOW_DARK, BG_WINDOW_LIGHT, SETTINGS};

use crate::state::{
    AppearanceConfig, ColorScheme, Resolved, Rgba, WindowColorMode, bg_surface, load_appearance,
    read_dominant, resolve, rgba, tinted_bg_sunken, tinted_bg_window,
};

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
    on_select: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let is_selected = selected_scheme == scheme;
    let test_config = AppearanceConfig {
        color_scheme: scheme,
        ..Default::default()
    };
    let preview_resolved = resolve(&test_config, dominant);

    selectable_card(
        caption,
        card_bg,
        theme_preview(scheme, &preview_resolved),
        is_selected,
        accent,
        bg_surface(preview_resolved.scheme),
        scheme_palette,
        on_select,
    )
}

/// Miniatura de tema: ventana con barra de acento y semáforos.
pub fn theme_preview(scheme: ColorScheme, preview_resolved: &Resolved) -> Rect {
    let window_bg = tinted_bg_window(preview_resolved);
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
        window = window
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0).bottom(0.0))
                    .width(Size::px(card_w / 2.0))
                    .background(rgba(BG_WINDOW_LIGHT)),
            )
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).right(0.0).bottom(0.0))
                    .width(Size::px(card_w / 2.0))
                    .background(rgba(BG_WINDOW_DARK)),
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
    on_select: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let base_config = load_appearance();
    let test_config = AppearanceConfig {
        window_color: mode,
        ..base_config
    };
    let preview_resolved = resolve(&test_config, read_dominant());
    let is_selected = selected == mode;
    let scheme_palette = crate::state::palette(resolved.scheme);

    selectable_card(
        caption,
        card_bg,
        window_color_preview(&preview_resolved),
        is_selected,
        resolved.accent,
        bg_surface(resolved.scheme),
        scheme_palette,
        on_select,
    )
}

/// Miniatura de color de ventanas.
pub fn window_color_preview(preview_resolved: &Resolved) -> Rect {
    let window_bg = tinted_bg_window(preview_resolved);
    let sunken = tinted_bg_sunken(preview_resolved);
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
