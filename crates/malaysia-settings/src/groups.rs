//! Grupos de Ajustes: títulos, filas, tarjetas y contenido desplazable.

use barrier_kit::ProbeExt;
use barrier_kit::controls::ScrollView;
use freya_components::svg_viewer::SvgViewer;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Content, Direction, Gaps, Position, Size};
use vow_design::{CALLOUT, HEADLINE, SETTINGS, TEXT};

use crate::cards::{theme_card, window_color_card};
use crate::controls::{accent_swatch, radio, traffic_dropdown};

const SWATCH_PROBES: [&str; 8] = [
    "group.color.swatch.azul",
    "group.color.swatch.morado",
    "group.color.swatch.rosa",
    "group.color.swatch.rojo",
    "group.color.swatch.naranja",
    "group.color.swatch.amarillo",
    "group.color.swatch.verde",
    "group.color.swatch.gris",
];
use crate::state::{
    AppearanceConfig, ColorScheme, Resolved, Rgba, WindowColorMode, WindowTintScope,
    card_background, load_appearance, matches_filter, palette, rgba, save_config,
    tint_scope_disabled,
};

/// Contenido principal desplazable con los grupos filtrados por búsqueda.
pub fn content_scroll(
    query: &str,
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
    appearance_error: State<Option<String>>,
    color_error: State<Option<String>>,
    windows_error: State<Option<String>>,
) -> Element {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);
    let mut children: Vec<Element> = Vec::new();

    let appearance_visible = matches_filter(query, "apariencia tema claro oscuro automático");
    if appearance_visible {
        children.push(appearance_group(
            query,
            config,
            dominant,
            &resolved,
            appearance_error,
        ));
    }

    let color_visible = matches_filter(
        query,
        "color acento ventanas fondo escritorio tinte azul morado rosa rojo naranja amarillo verde gris neutro",
    );
    if color_visible {
        children.push(group_title(
            "Color",
            &scheme_palette,
            Some("group.color.title"),
        ));
        children.push(color_group(query, config, dominant, &resolved, color_error));
    }

    let windows_visible = matches_filter(
        query,
        "ventanas semaforos botones cerrar minimizar ampliar clasico acento fondo",
    );
    if windows_visible {
        children.push(group_title(
            "Ventanas",
            &scheme_palette,
            Some("group.ventanas.title"),
        ));
        children.push(windows_group(query, config, &resolved, windows_error));
    }

    if children.is_empty() {
        return rect()
            .width(Size::fill())
            .height(Size::fill())
            .probe("content")
            .padding(Gaps::new(
                SETTINGS.content_padding_top as f32,
                SETTINGS.group_content_padding_x as f32,
                0.0,
                SETTINGS.group_content_padding_x as f32,
            ))
            .child(no_results(query, resolved.sidebar.palette))
            .into_element();
    }

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .probe("content")
        .padding(Gaps::new(
            SETTINGS.content_padding_top as f32,
            SETTINGS.group_content_padding_x as f32,
            SETTINGS.content_padding_bottom as f32,
            SETTINGS.group_content_padding_x as f32,
        ))
        .child(
            ScrollView::new()
                .width(Size::fill())
                .height(Size::fill())
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::auto())
                        .direction(Direction::Vertical)
                        .spacing(SETTINGS.content_gap as f32)
                        .children(children),
                ),
        )
        .into_element()
}

fn group_title(
    label_text: &str,
    scheme_palette: &vow_design::Palette,
    probe: Option<&'static str>,
) -> Element {
    if label_text.is_empty() {
        return rect()
            .width(Size::fill())
            .height(Size::px(0.0))
            .into_element();
    }
    let mut title = label()
        .text(label_text.to_string())
        .width(Size::flex(1.0))
        .line_height(HEADLINE.line_height as f32 / HEADLINE.size as f32)
        .font_size(HEADLINE.size as f32)
        .font_weight(FontWeight::from(HEADLINE.weight as i32))
        .color(rgba(scheme_palette.ink_strong))
        .max_lines(1);
    if let Some(id) = probe {
        title = title.a11y_builder(move |node| node.set_author_id(id));
    }
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .margin(Gaps::new(
            SETTINGS.group_title_margin_top as f32 - SETTINGS.content_gap as f32,
            0.0,
            SETTINGS.group_title_margin_bottom as f32 - SETTINGS.content_gap as f32,
            0.0,
        ))
        .direction(Direction::Horizontal)
        .content(Content::flex())
        .child(
            rect()
                .width(Size::px(SETTINGS.group_title_indent as f32))
                .height(Size::auto()),
        )
        .child(title)
        .into_element()
}

fn appearance_group(
    _query: &str,
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Element {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);
    let card_bg = card_background(dominant, resolved.accent);

    let selected = config.color_scheme;

    card_container(
        &resolved,
        vec![
            row_card(
                &resolved,
                "Apariencia",
                rect()
                    .width(Size::auto())
                    .direction(Direction::Horizontal)
                    .a11y_alt("appearance-theme-row")
                    .spacing(SETTINGS.card_gap as f32)
                    .child(theme_card(
                        "Automático",
                        ColorScheme::Auto,
                        selected,
                        card_bg,
                        dominant,
                        resolved.accent,
                        scheme_palette,
                        config,
                        "group.apariencia.card.automatico",
                        "group.apariencia.card.automatico.preview",
                        {
                            let mut error = error;
                            move |_| {
                                let mut config = load_appearance();
                                config.color_scheme = ColorScheme::Auto;
                                if let Some(msg) = save_config(&config) {
                                    error.set(Some(msg));
                                }
                            }
                        },
                    ))
                    .child(theme_card(
                        "Claro",
                        ColorScheme::Light,
                        selected,
                        card_bg,
                        dominant,
                        resolved.accent,
                        scheme_palette,
                        config,
                        "group.apariencia.card.claro",
                        "group.apariencia.card.claro.preview",
                        {
                            let mut error = error;
                            move |_| {
                                let mut config = load_appearance();
                                config.color_scheme = ColorScheme::Light;
                                if let Some(msg) = save_config(&config) {
                                    error.set(Some(msg));
                                }
                            }
                        },
                    ))
                    .child(theme_card(
                        "Oscuro",
                        ColorScheme::Dark,
                        selected,
                        card_bg,
                        dominant,
                        resolved.accent,
                        scheme_palette,
                        config,
                        "group.apariencia.card.oscuro",
                        "group.apariencia.card.oscuro.preview",
                        {
                            let mut error = error;
                            move |_| {
                                let mut config = load_appearance();
                                if config.color_scheme == ColorScheme::DarkTint {
                                    // Keep dark-tint when the user presses "Dark".
                                } else {
                                    config.color_scheme = ColorScheme::Dark;
                                }
                                if let Some(msg) = save_config(&config) {
                                    error.set(Some(msg));
                                }
                            }
                        },
                    ))
                    .into_element(),
                "group.apariencia.row",
                "group.apariencia.label",
                None,
            )
            .into_element(),
        ],
        "group.apariencia",
    )
    .child(error_label(&error, &scheme_palette))
    .into_element()
}

fn color_group(
    query: &str,
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Element {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);
    let wallpaper_mode = config.window_color == WindowColorMode::Wallpaper;
    let card_bg = card_background(dominant, resolved.accent);
    let control_bg = resolved.surfaces.control;
    let text_color = scheme_palette.text.focused;

    let swatches =
        rect()
            .width(Size::auto())
            .direction(Direction::Horizontal)
            .spacing(SETTINGS.swatch_gap as f32)
            .children(vow_design::ACCENT_SWATCHES.iter().enumerate().map(
                |(index, (name, color))| {
                    let selected = config.accent_color == Some([color.r, color.g, color.b])
                        || (config.accent_color.is_none() && index == 0);
                    let mut error = error;
                    let probe = SWATCH_PROBES[index];
                    accent_swatch(name, *color, selected, wallpaper_mode, probe, move |_| {
                        let mut config = load_appearance();
                        config.accent_color = Some([color.r, color.g, color.b]);
                        if let Some(msg) = save_config(&config) {
                            error.set(Some(msg));
                        }
                    })
                    .into_element()
                },
            ));

    let accent_row = row_card(
        &resolved,
        "Color de acento",
        swatches.into_element(),
        "group.color.acento",
        "group.color.acento.label",
        None,
    );
    let selected_window_mode = config.window_color;
    let window_mode_row = row_card(
        &resolved,
        "Color de las ventanas\nGrises sin tinte.",
        rect()
            .width(Size::auto())
            .direction(Direction::Horizontal)
            .spacing(SETTINGS.card_gap as f32)
            .child(window_color_card(
                "Neutro",
                WindowColorMode::Neutral,
                selected_window_mode,
                card_bg,
                resolved,
                dominant,
                "group.color.ventanas.card.neutro",
                "group.color.ventanas.card.neutro.preview",
                {
                    let mut error = error;
                    move |_| {
                        let mut config = load_appearance();
                        config.window_color = WindowColorMode::Neutral;
                        if let Some(msg) = save_config(&config) {
                            error.set(Some(msg));
                        }
                    }
                },
            ))
            .child(window_color_card(
                "Acento",
                WindowColorMode::Accent,
                selected_window_mode,
                card_bg,
                resolved,
                dominant,
                "group.color.ventanas.card.acento",
                "group.color.ventanas.card.acento.preview",
                {
                    let mut error = error;
                    move |_| {
                        let mut config = load_appearance();
                        config.window_color = WindowColorMode::Accent;
                        if let Some(msg) = save_config(&config) {
                            error.set(Some(msg));
                        }
                    }
                },
            ))
            .child(window_color_card(
                "Fondo",
                WindowColorMode::Wallpaper,
                selected_window_mode,
                card_bg,
                resolved,
                dominant,
                "group.color.ventanas.card.fondo",
                "group.color.ventanas.card.fondo.preview",
                {
                    let mut error = error;
                    move |_| {
                        let mut config = load_appearance();
                        config.window_color = WindowColorMode::Wallpaper;
                        if let Some(msg) = save_config(&config) {
                            error.set(Some(msg));
                        }
                    }
                },
            ))
            .into_element(),
        "group.color.ventanas",
        "group.color.ventanas.label",
        Some("group.color.ventanas.help"),
    );

    let scope_disabled = tint_scope_disabled(config.window_color);
    let scope_row = row_card(
        &resolved,
        "Aplicar el tinte a",
        rect()
            .width(Size::auto())
            .direction(Direction::Horizontal)
            .spacing(SETTINGS.radio_spacing as f32)
            .child(radio(
                "Solo las ventanas",
                config.window_tint_scope == WindowTintScope::Windows,
                scope_disabled,
                control_bg,
                resolved.accent,
                text_color,
                "group.color.alcance.ventanas",
                {
                    let mut error = error;
                    move |_| {
                        if !scope_disabled {
                            let mut config = load_appearance();
                            config.window_tint_scope = WindowTintScope::Windows;
                            if let Some(msg) = save_config(&config) {
                                error.set(Some(msg));
                            }
                        }
                    }
                },
            ))
            .child(radio(
                "Todo el escritorio",
                config.window_tint_scope == WindowTintScope::Desktop,
                scope_disabled,
                control_bg,
                resolved.accent,
                text_color,
                "group.color.alcance.escritorio",
                {
                    let mut error = error;
                    move |_| {
                        if !scope_disabled {
                            let mut config = load_appearance();
                            config.window_tint_scope = WindowTintScope::Desktop;
                            if let Some(msg) = save_config(&config) {
                                error.set(Some(msg));
                            }
                        }
                    }
                },
            ))
            .into_element(),
        "group.color.alcance",
        "group.color.alcance.label",
        None,
    )
    .height(Size::px(SETTINGS.group_row_min_height as f32));

    let mut rows: Vec<Element> = Vec::new();
    if matches_filter(
        query,
        "color de acento azul morado rosa rojo naranja amarillo verde gris",
    ) {
        rows.push(accent_row.into_element());
    }
    if matches_filter(
        query,
        "color de las ventanas tinte neutro acento fondo de pantalla",
    ) {
        rows.push(window_mode_row.into_element());
    }
    if matches_filter(
        query,
        "aplicar el tinte escritorio barra de menus dock fondo de pantalla",
    ) {
        rows.push(scope_row.into_element());
    }
    card_container(&resolved, rows, "group.color")
        .child(error_label(&error, &scheme_palette))
        .into_element()
}

fn windows_group(
    _query: &str,
    config: &AppearanceConfig,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Element {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);
    let current = config.traffic_lights;
    let control_bg = resolved.surfaces.control;
    let text_color = resolved.sidebar.palette.item_ink;

    let mut error = error;
    let dropdown = traffic_dropdown(
        current,
        resolved.lights,
        text_color,
        control_bg,
        "group.ventanas.semaforos.dropdown",
        move |style| {
            let mut config = load_appearance();
            config.traffic_lights = style;
            if let Some(msg) = save_config(&config) {
                error.set(Some(msg));
            }
        },
    );

    card_container(
        &resolved,
        vec![
            row_card(
                &resolved,
                "Semáforos",
                dropdown.into_element(),
                "group.ventanas.semaforos",
                "group.ventanas.semaforos.label",
                None,
            )
            .into_element(),
        ],
        "group.ventanas",
    )
    .child(error_label(&error, &scheme_palette))
    .into_element()
}

fn row_card(
    resolved: &Resolved,
    label_text: &str,
    control: Element,
    probe: &'static str,
    label_probe: &'static str,
    desc_probe: Option<&'static str>,
) -> Rect {
    let scheme_palette = palette(resolved.scheme);
    let (title, desc) = label_text.split_once('\n').unwrap_or((label_text, ""));

    let text_height = TEXT.line_height as f32
        + if desc.is_empty() {
            0.0
        } else {
            SETTINGS.group_desc_gap as f32 + CALLOUT.line_height as f32
        };
    let mut text_col = rect()
        .width(Size::flex(1.0))
        .height(Size::px(text_height))
        .direction(Direction::Vertical)
        .spacing(SETTINGS.group_desc_gap as f32)
        .child(
            label()
                .text(title.to_string())
                .width(Size::fill())
                .line_height(TEXT.line_height as f32 / TEXT.size as f32)
                .font_size(TEXT.size as f32)
                .color(rgba(scheme_palette.text.focused))
                .a11y_builder(move |node| node.set_author_id(label_probe))
                .max_lines(1),
        );

    if !desc.is_empty()
        && let Some(desc_probe) = desc_probe
    {
        text_col = text_col.child(
            label()
                .text(desc.to_string())
                .width(Size::fill())
                .line_height(CALLOUT.line_height as f32 / CALLOUT.size as f32)
                .font_size(CALLOUT.size as f32)
                .font_weight(FontWeight::from(CALLOUT.weight as i32))
                .color(rgba(scheme_palette.text.unfocused))
                .a11y_builder(move |node| node.set_author_id(desc_probe))
                .max_lines(2),
        );
    }

    rect()
        .width(Size::fill())
        .height(Size::auto())
        .min_height(Size::px(SETTINGS.group_row_min_height as f32))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .content(Content::flex())
        .padding(Gaps::new(
            SETTINGS.group_row_padding_y as f32,
            SETTINGS.group_row_padding_x as f32,
            SETTINGS.group_row_padding_y as f32,
            SETTINGS.group_row_padding_x as f32,
        ))
        .spacing(SETTINGS.row_gap as f32)
        .probe(probe)
        .child(text_col)
        .child(control)
}

fn card_container(resolved: &Resolved, rows: Vec<Element>, probe: &'static str) -> Rect {
    let scheme_palette = palette(resolved.scheme);
    let children: Vec<Element> = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let mut cell = rect().width(Size::fill()).height(Size::auto()).child(row);
            if index > 0 {
                cell = cell.child(
                    rect()
                        .position(
                            Position::new_absolute()
                                .top(0.0)
                                .left(SETTINGS.group_row_padding_x as f32)
                                .right(SETTINGS.group_row_padding_x as f32),
                        )
                        .height(Size::px(1.0))
                        .background(rgba(scheme_palette.line)),
                );
            }
            cell.into_element()
        })
        .collect();
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .probe(probe)
        .corner_radius(SETTINGS.group_radius as f32)
        .background(rgba(resolved.surfaces.surface))
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(rgba(scheme_palette.line)),
        )
        .direction(Direction::Vertical)
        .overflow(Overflow::Clip)
        .children(children)
}

fn error_label(error: &State<Option<String>>, scheme_palette: &vow_design::Palette) -> Element {
    if let Some(ref msg) = *error.read() {
        label()
            .text(msg.clone())
            .font_size(CALLOUT.size as f32)
            .font_weight(FontWeight::from(CALLOUT.weight as i32))
            .color(rgba(scheme_palette.danger))
            .max_lines(2)
            .padding(Gaps::new(
                SETTINGS.group_row_padding_x as f32,
                SETTINGS.group_row_padding_x as f32,
                SETTINGS.group_row_padding_x as f32,
                SETTINGS.group_row_padding_x as f32,
            ))
            .into_element()
    } else {
        rect().into_element()
    }
}

fn no_results(query: &str, palette: vow_design::SidebarPalette) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .probe("empty")
        .padding(Gaps::new(
            SETTINGS.empty_padding_y as f32,
            0.0,
            SETTINGS.empty_padding_y as f32,
            0.0,
        ))
        .direction(Direction::Vertical)
        .spacing(SETTINGS.empty_gap as f32)
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .child(svg_viewer(
            SEARCH_ICON,
            palette.item_ink,
            SETTINGS.empty_icon,
        ))
        .child(
            label()
                .text("Sin resultados")
                .line_height(HEADLINE.line_height as f32 / HEADLINE.size as f32)
                .font_size(HEADLINE.size as f32)
                .font_weight(FontWeight::from(HEADLINE.weight as i32))
                .color(rgba(palette.item_ink))
                .max_lines(1),
        )
        .child(
            label()
                .text(format!("No hay ninguna opción que coincida con «{query}»."))
                .line_height(TEXT.line_height as f32 / TEXT.size as f32)
                .font_size(TEXT.size as f32)
                .color(rgba(palette.count_ink))
                .a11y_builder(|node| node.set_author_id("empty.text"))
                .max_lines(2),
        )
}

fn svg_viewer(template: &str, color: Rgba, size: u32) -> SvgViewer {
    let [r, g, b, _] = color.to_rgba8();
    SvgViewer::new((
        format!("settings-icon-{size}-{}", template.len()),
        Bytes::from(template.replace("currentColor", &format!("rgb({r},{g},{b})"))),
    ))
    .width(Size::px(size as f32))
    .height(Size::px(size as f32))
    .show_loader(false)
}

const SEARCH_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"6.8\" cy=\"6.8\" r=\"4.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\"/><path d=\"M10 10l3.2 3.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\"/></svg>";
