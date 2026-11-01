//! Settings groups: titles, rows, cards and scrollable content.

use barrier_kit::ProbeExt;
use barrier_kit::controls::{Checkbox, ScrollView, Slider};
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
    AppearanceConfig, ColorScheme, Resolved, Rgba, TintParts, TintStrength, WindowColorMode,
    WindowTintScope, card_background, load_appearance, matches_filter, palette, rgba, save_config,
    tint_scope_disabled,
};

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
                false,
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
    let text_color = scheme_palette.ink_strong;

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
        false,
    );
    let selected_window_mode = config.window_color;
    let window_help = match selected_window_mode {
        WindowColorMode::Neutral => "Grises sin tinte.",
        WindowColorMode::Accent => "Toman un poco del color de acento.",
        WindowColorMode::Wallpaper => "Toman el color del fondo de pantalla.",
    };
    let window_mode_row = row_card(
        &resolved,
        &format!("Color de las ventanas\n{window_help}"),
        rect()
            .width(Size::auto())
            .direction(Direction::Horizontal)
            .spacing(SETTINGS.card_gap as f32)
            .child(window_color_card(
                "Neutro",
                WindowColorMode::Neutral,
                selected_window_mode,
                card_bg,
                config,
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
                config,
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
                config,
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
        false,
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
        scope_disabled,
    )
    .height(Size::px(SETTINGS.group_row_min_height as f32));

    let tint_disabled = config.window_color == WindowColorMode::Neutral;
    let current_strength = match config.window_color {
        WindowColorMode::Accent => config.tint_strength_accent,
        WindowColorMode::Wallpaper => config.tint_strength_wallpaper,
        WindowColorMode::Neutral => TintStrength::default(),
    };
    let strength_row = row_card(
        &resolved,
        "Intensidad del tinte\nAcento y Fondo guardan cada uno la suya.",
        Slider::stepped(4)
            .value(strength_index(current_strength))
            .labels(&["Sutil", "Normal", "Fuerte", "Intenso"])
            .disabled(tint_disabled)
            .sub_ids(
                "group.color.intensidad.knob",
                &[
                    "group.color.intensidad.step.sutil",
                    "group.color.intensidad.step.normal",
                    "group.color.intensidad.step.fuerte",
                    "group.color.intensidad.step.intenso",
                ],
            )
            .probe("group.color.intensidad.slider")
            .on_change({
                let mut error = error;
                move |value| {
                    if !tint_disabled {
                        let mut config = load_appearance();
                        let strength = index_strength(value);
                        match config.window_color {
                            WindowColorMode::Accent => config.tint_strength_accent = strength,
                            WindowColorMode::Wallpaper => config.tint_strength_wallpaper = strength,
                            WindowColorMode::Neutral => {}
                        }
                        if let Some(msg) = save_config(&config) {
                            error.set(Some(msg));
                        }
                    }
                }
            })
            .into_element(),
        "group.color.intensidad",
        "group.color.intensidad.label",
        Some("group.color.intensidad.help"),
        tint_disabled,
    );

    let parts_row = row_card(
        &resolved,
        "Teñir en las ventanas\nLo que no marques queda en gris neutro.",
        tint_parts_grid(config.tint_parts, tint_disabled, {
            let mut error = error;
            move |parts| {
                let mut config = load_appearance();
                config.tint_parts = parts;
                if let Some(msg) = save_config(&config) {
                    error.set(Some(msg));
                }
            }
        }),
        "group.color.partes",
        "group.color.partes.label",
        Some("group.color.partes.help"),
        tint_disabled,
    );

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
        "intensidad del tinte sutil normal fuerte intenso acento fondo",
    ) {
        rows.push(strength_row.into_element());
    }
    if matches_filter(
        query,
        "tenir ventanas partes barras laterales fondo tarjetas controles menus",
    ) {
        rows.push(parts_row.into_element());
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
                false,
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
    disabled: bool,
) -> Rect {
    let scheme_palette = palette(resolved.scheme);
    let title_color = if disabled {
        scheme_palette.ink_faint
    } else {
        scheme_palette.ink_strong
    };
    let (title, desc) = label_text.split_once('\n').unwrap_or((label_text, ""));

    let mut text_col = rect()
        .width(Size::px(SETTINGS.label_column as f32))
        .height(Size::auto())
        .direction(Direction::Vertical)
        .spacing(SETTINGS.group_desc_gap as f32)
        .child(
            label()
                .text(title.to_string())
                .width(Size::fill())
                .line_height(TEXT.line_height as f32 / TEXT.size as f32)
                .font_size(TEXT.size as f32)
                .font_weight(FontWeight::from(TEXT.weight as i32))
                .color(rgba(title_color))
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
        .padding(Gaps::new(
            SETTINGS.group_row_padding_y as f32,
            SETTINGS.group_row_padding_x as f32,
            SETTINGS.group_row_padding_y as f32,
            SETTINGS.group_row_padding_x as f32,
        ))
        .spacing(SETTINGS.row_gap as f32)
        .probe(probe)
        .child(text_col)
        .child(
            rect()
                .width(Size::flex(1.0))
                .height(Size::auto())
                .direction(Direction::Horizontal)
                .main_align(Alignment::Center)
                .child(control),
        )
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
                .font_weight(FontWeight::from(TEXT.weight as i32))
                .line_height(TEXT.line_height as f32 / TEXT.size as f32)
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

fn strength_index(strength: TintStrength) -> usize {
    match strength {
        TintStrength::Subtle => 0,
        TintStrength::Normal => 1,
        TintStrength::Strong => 2,
        TintStrength::Intense => 3,
    }
}

fn index_strength(index: usize) -> TintStrength {
    match index {
        0 => TintStrength::Subtle,
        1 => TintStrength::Normal,
        2 => TintStrength::Strong,
        _ => TintStrength::Intense,
    }
}

fn tint_parts_grid(
    parts: TintParts,
    disabled: bool,
    on_change: impl FnMut(TintParts) + 'static,
) -> Element {
    use std::cell::RefCell;
    use std::rc::Rc;

    let column_gap = SETTINGS.check.column_gap as f32;
    let row_gap = SETTINGS.check.row_gap as f32;
    let control_width = SETTINGS.window_width as f32
        - (vow_design::SIDEBAR.width + 2 * vow_design::SIDEBAR.inset) as f32
        - 2.0 * SETTINGS.group_content_padding_x as f32
        - 2.0 * SETTINGS.group_row_padding_x as f32
        - SETTINGS.label_column as f32
        - SETTINGS.row_gap as f32;
    let column_width = (control_width - column_gap) / 2.0;
    let on_change = Rc::new(RefCell::new(on_change));
    let make = |label: &'static str,
                checked: bool,
                probe: &'static str,
                toggle: fn(&mut TintParts),
                on_change: Rc<RefCell<dyn FnMut(TintParts)>>| {
        Checkbox::new(label)
            .checked(checked)
            .disabled(disabled)
            .width(Size::fill())
            .probe(probe)
            .on_toggle(move |value| {
                if value != checked {
                    let mut parts = parts;
                    toggle(&mut parts);
                    on_change.borrow_mut()(parts);
                }
            })
            .into_element()
    };
    let column = |children: Vec<Element>| {
        rect()
            .width(Size::px(column_width))
            .height(Size::auto())
            .direction(Direction::Vertical)
            .spacing(row_gap)
            .children(children)
            .into_element()
    };
    rect()
        .width(Size::px(control_width))
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .spacing(column_gap)
        .child(column(vec![
            make(
                "Barras laterales",
                parts.sidebars,
                "group.color.partes.barras",
                |p| p.sidebars = !p.sidebars,
                on_change.clone(),
            ),
            make(
                "Grupos y tarjetas",
                parts.groups,
                "group.color.partes.tarjetas",
                |p| p.groups = !p.groups,
                on_change.clone(),
            ),
            make(
                "Menús y diálogos",
                parts.menus,
                "group.color.partes.menus",
                |p| p.menus = !p.menus,
                on_change.clone(),
            ),
        ]))
        .child(column(vec![
            make(
                "Fondo de la ventana",
                parts.window,
                "group.color.partes.ventana",
                |p| p.window = !p.window,
                on_change.clone(),
            ),
            make(
                "Controles",
                parts.controls,
                "group.color.partes.controles",
                |p| p.controls = !p.controls,
                on_change.clone(),
            ),
        ]))
        .into_element()
}
