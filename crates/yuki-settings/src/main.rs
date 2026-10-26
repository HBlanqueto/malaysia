//! Ajustes del sistema de Yuki.
//!
//! Ventana de preferencias con sidebar, toolbar y contenido desplazable.
//! Todos los colores y medidas salen de `yuki-design`, `yuki-appearance` y los
//! componentes de `celestial-kit`; no hay literales de diseño.

use std::cell::RefCell;
use std::rc::Rc;

use celestial_kit::controls::{ScrollView, TextField, ToolButton, ToolGroup};
use celestial_kit::menu::{CheckState, MenuItem, MenuView};
use celestial_kit::platform::{
    Decorations, PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupHandle, PopupSpec,
    Popups, WindowOptions, run_window,
};
use celestial_kit::sidebar::{Sidebar, SidebarItem, SidebarLayout};
use celestial_kit::theme;
use celestial_kit::titlebar::{Titlebar, resize_edges};
use freya_components::svg_viewer::SvgViewer;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Position, Size};
use yuki_appearance::{
    AppearanceConfig, ColorScheme, Resolved, TrafficLightStyle, WindowColorMode, WindowTintScope,
    appearance_path, load_appearance, palette, read_dominant, resolve, write_appearance,
};
use yuki_design::{
    ACCENT_SWATCHES, BG_CONTROL_DARK, BG_CONTROL_DARK_TINT, BG_CONTROL_LIGHT, BG_SURFACE_DARK,
    BG_SURFACE_DARK_TINT, BG_SURFACE_LIGHT, BG_WINDOW_DARK, BG_WINDOW_DARK_TINT, BG_WINDOW_LIGHT,
    CALLOUT, HEADLINE, LAYER_FACTOR_BG_SUNKEN, Rgba, SETTINGS, SIDEBAR, TEXT, TITLE_2, TITLEBAR,
};

fn rgba(color: Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn main() {
    run_window(
        AppComponent::new(SettingsApp),
        WindowOptions::new("Ajustes del sistema", "settings")
            .decorations(Decorations::Client)
            .size((SETTINGS.window_width, SETTINGS.window_height))
            .min_size(Some((SETTINGS.window_width, SETTINGS.window_min_height)))
            .max_size(Some((SETTINGS.window_width, 0))),
    )
    .expect("yuki-settings");
}

struct SettingsApp;

impl App for SettingsApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(theme::shell);

        let search = use_state(String::new);
        let appearance_error = use_state(|| None::<String>);
        let color_error = use_state(|| None::<String>);
        let windows_error = use_state(|| None::<String>);
        let config = load_appearance();
        let dominant = read_dominant();
        let resolved = resolve(&config, dominant);
        let query = search.read().to_lowercase();

        SidebarLayout::new(
            sidebar(search, &config, &resolved),
            content(
                &config,
                dominant,
                &resolved,
                &query,
                appearance_error,
                color_error,
                windows_error,
            ),
        )
    }
}

fn sidebar(mut search: State<String>, _config: &AppearanceConfig, resolved: &Resolved) -> Sidebar {
    let resolved = *resolved;
    let sidebar_palette = resolved.sidebar.palette;
    let scheme_palette = palette(resolved.scheme);

    let search_field = rect()
        .width(Size::fill())
        .height(Size::px(SETTINGS.search_height as f32))
        .corner_radius(999.0)
        .background(rgba(bg_control(resolved.scheme)))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .padding(Gaps::new(
            0.0,
            SETTINGS.search_padding_x as f32,
            0.0,
            SETTINGS.search_padding_x as f32,
        ))
        .spacing(8.0)
        .child(svg_viewer(
            SEARCH_ICON,
            sidebar_palette.item_ink,
            SETTINGS.search_icon,
        ))
        .child(
            TextField::new(search)
                .placeholder("Buscar")
                .width(Size::fill())
                .flat(),
        )
        .child(
            rect()
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .center()
                .maybe(!search.read().is_empty(), |el| {
                    el.child(svg_viewer(CLEAR_ICON, sidebar_palette.item_ink, 14))
                        .on_press(move |_| search.set(String::new()))
                }),
        );

    let account = rect()
        .width(Size::fill())
        .direction(Direction::Horizontal)
        .spacing(SETTINGS.account_gap as f32)
        .cross_align(Alignment::Center)
        .padding(Gaps::new(
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
        ))
        .child(
            rect()
                .width(Size::px(SETTINGS.account_avatar as f32))
                .height(Size::px(SETTINGS.account_avatar as f32))
                .corner_radius((SETTINGS.account_avatar as f32 / 2.0).max(0.0))
                .background(rgba(resolved.accent))
                .center()
                .child(
                    label()
                        .text(initials(&account_name()))
                        .font_size((SETTINGS.account_avatar as f32 / 2.5).max(10.0))
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(rgba(scheme_palette.on_accent)),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .direction(Direction::Vertical)
                .child(
                    label()
                        .text(account_name())
                        .font_size(SETTINGS.account_name_font.size as f32)
                        .font_weight(FontWeight::from(SETTINGS.account_name_font.weight as i32))
                        .color(rgba(sidebar_palette.item_ink))
                        .max_lines(1),
                )
                .child(
                    label()
                        .text("Cuenta local")
                        .font_size(SETTINGS.account_detail_font.size as f32)
                        .font_weight(FontWeight::from(SETTINGS.account_detail_font.weight as i32))
                        .color(rgba(sidebar_palette.count_ink))
                        .max_lines(1),
                ),
        );

    Sidebar::new()
        .traffic_lights(true)
        .on_close(move |()| std::process::exit(0))
        .child(search_field)
        .child(account)
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(SETTINGS.section_gap as f32)),
        )
        .child(
            SidebarItem::new("Apariencia")
                .selected(true)
                .app_icon(APPEARANCE_ICON, SETTINGS.section_icon_bg)
                .on_select(move |_| {
                    // La única sección por ahora; el buscador la filtra.
                }),
        )
}

fn content(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
    query: &str,
    appearance_error: State<Option<String>>,
    color_error: State<Option<String>>,
    windows_error: State<Option<String>>,
) -> impl IntoElement {
    let resolved = *resolved;
    let bg = bg_window(resolved.scheme);

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(rgba(bg))
        .direction(Direction::Vertical)
        .child(
            Titlebar::new().with_sidebar(true).title("Apariencia").left(
                ToolGroup::new()
                    .child(ToolButton::new("Atrás", BACK_ICON).disabled(true))
                    .child(ToolButton::new("Adelante", FORWARD_ICON).disabled(true)),
            ),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(
                    SIDEBAR.padding_x as f32,
                    SIDEBAR.padding_x as f32,
                    SIDEBAR.padding_x as f32,
                    SIDEBAR.padding_x as f32,
                ))
                .child(
                    ScrollView::new()
                        .width(Size::fill())
                        .height(Size::flex(1.0))
                        .child(scroll_content(
                            query,
                            config,
                            dominant,
                            &resolved,
                            appearance_error,
                            color_error,
                            windows_error,
                        )),
                ),
        )
        .child(resize_edges())
}

fn scroll_content(
    query: &str,
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
    appearance_error: State<Option<String>>,
    color_error: State<Option<String>>,
    windows_error: State<Option<String>>,
) -> impl IntoElement {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);

    let mut children: Vec<Element> = Vec::new();

    let appearance_visible = matches_filter(query, "apariencia tema claro oscuro automático");
    if appearance_visible {
        children.push(group_title("", &scheme_palette));
        children.push(appearance_group(
            config,
            dominant,
            &resolved,
            appearance_error,
        ));
    }

    let color_visible = matches_filter(query, "color acento ventanas fondo escritorio tinte");
    if color_visible {
        children.push(group_title("Color", &scheme_palette));
        children.push(color_group(config, &resolved, color_error));
    }

    let windows_visible = matches_filter(query, "ventanas semáforos");
    if windows_visible {
        children.push(group_title("Ventanas", &scheme_palette));
        children.push(windows_group(config, &resolved, windows_error));
    }

    if children.is_empty() {
        return no_results(query, resolved.sidebar.palette).into_element();
    }

    rect()
        .width(Size::fill())
        .height(Size::auto())
        .direction(Direction::Vertical)
        .spacing(SIDEBAR.section_margin_top as f32)
        .children(children)
        .into_element()
}

fn group_title(label_text: &str, scheme_palette: &yuki_design::Palette) -> Element {
    if label_text.is_empty() {
        return rect()
            .width(Size::fill())
            .height(Size::px(0.0))
            .into_element();
    }
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .margin(Gaps::new(
            SETTINGS.group_title_margin_top as f32,
            0.0,
            0.0,
            0.0,
        ))
        .child(
            label()
                .text(label_text.to_string())
                .font_size(HEADLINE.size as f32)
                .font_weight(FontWeight::from(HEADLINE.weight as i32))
                .color(rgba(scheme_palette.ink_strong))
                .max_lines(1),
        )
        .into_element()
}

fn appearance_group(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Element {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);
    let card_bg = card_background(dominant, resolved.accent);

    let selected = match config.color_scheme {
        ColorScheme::Light => "light",
        ColorScheme::Auto => "auto",
        ColorScheme::Dark | ColorScheme::DarkTint => "dark",
    };

    let mut test_config = config.clone();
    test_config.color_scheme = ColorScheme::Light;
    let light_resolved = resolve(&test_config, dominant);
    test_config.color_scheme = ColorScheme::Dark;
    let dark_resolved = resolve(&test_config, dominant);
    test_config.color_scheme = ColorScheme::Auto;
    let auto_resolved = resolve(&test_config, dominant);

    card_container(
        &resolved,
        vec![
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .spacing(SETTINGS.card_gap as f32)
                .padding(Gaps::new(
                    SETTINGS.group_content_padding_x as f32,
                    SETTINGS.group_content_padding_x as f32,
                    SETTINGS.group_content_padding_x as f32,
                    SETTINGS.group_content_padding_x as f32,
                ))
                .child(theme_card(
                    "Claro",
                    "light",
                    selected,
                    card_bg,
                    light_resolved,
                    error,
                ))
                .child(theme_card(
                    "Oscuro",
                    "dark",
                    selected,
                    card_bg,
                    dark_resolved,
                    error,
                ))
                .child(theme_card(
                    "Automático",
                    "auto",
                    selected,
                    card_bg,
                    auto_resolved,
                    error,
                ))
                .into_element(),
        ],
    )
    .child(error_label(&error, &scheme_palette))
    .into_element()
}

fn card_background(dominant: Option<Rgba>, accent: Rgba) -> Rgba {
    dominant.unwrap_or(accent)
}

fn theme_card(
    caption: &'static str,
    id: &'static str,
    selected: &str,
    card_bg: Rgba,
    preview_resolved: Resolved,
    error: State<Option<String>>,
) -> Element {
    let is_selected = selected == id;
    let accent = preview_resolved.accent;
    let scheme_palette = palette(preview_resolved.scheme);
    let scheme = preview_resolved.scheme;

    let mut error = error;
    let on_select = move |_| {
        let mut config = load_appearance();
        if scheme == ColorScheme::Dark && config.color_scheme == ColorScheme::DarkTint {
            // Keep dark-tint when the user presses "Dark".
        } else {
            config.color_scheme = scheme;
        }
        if let Err(err) = write_appearance(&appearance_path(), &config) {
            error.set(Some(err.to_string()));
        }
    };

    selectable_card(
        caption,
        card_bg,
        theme_preview(scheme, &preview_resolved),
        is_selected,
        accent,
        scheme_palette,
        on_select,
    )
}

fn theme_preview(scheme: ColorScheme, preview_resolved: &Resolved) -> Rect {
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

fn tinted_bg_window(preview_resolved: &Resolved) -> Rgba {
    let base = match preview_resolved.scheme {
        ColorScheme::DarkTint => BG_WINDOW_DARK_TINT,
        ColorScheme::Dark => BG_WINDOW_DARK,
        _ => BG_WINDOW_LIGHT,
    };
    match preview_resolved.window_tint {
        Some((tint, factor)) => base.mix(tint, factor),
        None => base,
    }
}

fn tinted_bg_sunken(preview_resolved: &Resolved) -> Rgba {
    let base = palette(preview_resolved.scheme).titlebar_bg.unfocused;
    match preview_resolved.window_tint {
        Some((tint, factor)) => base.mix(tint, factor * LAYER_FACTOR_BG_SUNKEN),
        None => base,
    }
}

fn selectable_card(
    caption: &'static str,
    card_bg: Rgba,
    preview: Rect,
    is_selected: bool,
    accent: Rgba,
    scheme_palette: yuki_design::Palette,
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
                .fill(rgba(scheme_palette.line)),
        )
        .overflow(Overflow::Clip)
        .child(preview);

    let wrapper = rect()
        .width(Size::auto())
        .height(Size::auto())
        .padding(SETTINGS.card_ring_gap as f32)
        .corner_radius((SETTINGS.card_radius + SETTINGS.card_ring_gap) as f32)
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

fn color_group(
    config: &AppearanceConfig,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Element {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);
    let wallpaper_mode = config.window_color == WindowColorMode::Wallpaper;
    let card_bg = card_background(read_dominant(), resolved.accent);

    let swatches = rect()
        .width(Size::fill())
        .direction(Direction::Horizontal)
        .spacing(SETTINGS.swatch_gap as f32)
        .children(
            ACCENT_SWATCHES
                .iter()
                .enumerate()
                .map(|(index, (name, color))| {
                    let selected = config.accent_color == Some([color.r, color.g, color.b])
                        || (config.accent_color.is_none() && index == 0);
                    accent_swatch(name, *color, selected, wallpaper_mode, error).into_element()
                }),
        );

    let accent_row = row_card(&resolved, "Color de acento", swatches.into_element());

    let selected_window_mode = config.window_color;
    let window_mode_row = row_card(
        &resolved,
        "Color de las ventanas",
        rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .spacing(SETTINGS.card_gap as f32)
            .child(window_color_card(
                "Neutro",
                WindowColorMode::Neutral,
                selected_window_mode,
                card_bg,
                resolved,
                error,
            ))
            .child(window_color_card(
                "Acento",
                WindowColorMode::Accent,
                selected_window_mode,
                card_bg,
                resolved,
                error,
            ))
            .child(window_color_card(
                "Fondo",
                WindowColorMode::Wallpaper,
                selected_window_mode,
                card_bg,
                resolved,
                error,
            ))
            .into_element(),
    );

    let scope_disabled = !wallpaper_mode;
    let scope_row = row_card(
        &resolved,
        "Aplicar el tinte a",
        rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .spacing(SETTINGS.radio_spacing as f32)
            .child(radio(
                "Solo las ventanas",
                config.window_tint_scope == WindowTintScope::Windows,
                scope_disabled,
                resolved,
                {
                    let mut error = error;
                    move |_| {
                        if !scope_disabled {
                            let mut config = load_appearance();
                            config.window_tint_scope = WindowTintScope::Windows;
                            if let Err(err) = write_appearance(&appearance_path(), &config) {
                                error.set(Some(err.to_string()));
                            }
                        }
                    }
                },
            ))
            .child(radio(
                "Todo el escritorio",
                config.window_tint_scope == WindowTintScope::Desktop,
                scope_disabled,
                resolved,
                {
                    let mut error = error;
                    move |_| {
                        if !scope_disabled {
                            let mut config = load_appearance();
                            config.window_tint_scope = WindowTintScope::Desktop;
                            if let Err(err) = write_appearance(&appearance_path(), &config) {
                                error.set(Some(err.to_string()));
                            }
                        }
                    }
                },
            ))
            .into_element(),
    );

    let note: Element = if wallpaper_mode {
        label()
            .text("Lo pone el fondo de pantalla.")
            .font_size(CALLOUT.size as f32)
            .font_weight(FontWeight::from(CALLOUT.weight as i32))
            .color(rgba(resolved.sidebar.palette.count_ink))
            .max_lines(1)
            .into_element()
    } else {
        rect().into_element()
    };

    card_container(
        &resolved,
        vec![
            accent_row.into_element(),
            window_mode_row.into_element(),
            scope_row.into_element(),
            note,
        ],
    )
    .child(error_label(&error, &scheme_palette))
    .into_element()
}

fn accent_swatch(
    name: &'static str,
    color: Rgba,
    selected: bool,
    disabled: bool,
    error: State<Option<String>>,
) -> Rect {
    let swatch_size = SETTINGS.swatch_size as f32;
    let ring_gap = SETTINGS.swatch_gap as f32; // reused as ring gap; swatch has no separate ring gap token
    let ring = 1.5f32;

    let inner = rect()
        .width(Size::px(swatch_size))
        .height(Size::px(swatch_size))
        .corner_radius((swatch_size / 2.0).max(0.0))
        .background(rgba(color));

    let wrapper = rect()
        .width(Size::auto())
        .height(Size::auto())
        .padding(ring_gap)
        .corner_radius(((swatch_size + ring_gap * 2.0) / 2.0).max(0.0))
        .border(
            Border::new()
                .width(if selected { ring } else { 0.0 })
                .alignment(BorderAlignment::Outer)
                .fill(rgba(yuki_design::LIGHT.text.focused)),
        )
        .child(inner);

    let mut column = rect()
        .width(Size::auto())
        .height(Size::auto())
        .direction(Direction::Vertical)
        .spacing(2.0)
        .cross_align(Alignment::Center)
        .child(wrapper);

    if selected {
        column = column.child(
            label()
                .text(name)
                .font_size(SETTINGS.swatch_name_font.size as f32)
                .font_weight(FontWeight::from(SETTINGS.swatch_name_font.weight as i32))
                .color(rgba(yuki_design::LIGHT.text.focused))
                .max_lines(1),
        );
    }

    if disabled {
        column.opacity(0.4)
    } else {
        let mut error = error;
        column.on_press(move |_| {
            let mut config = load_appearance();
            config.accent_color = Some([color.r, color.g, color.b]);
            if let Err(err) = write_appearance(&appearance_path(), &config) {
                error.set(Some(err.to_string()));
            }
        })
    }
}

fn window_color_card(
    caption: &'static str,
    mode: WindowColorMode,
    selected: WindowColorMode,
    card_bg: Rgba,
    resolved: Resolved,
    error: State<Option<String>>,
) -> Element {
    let mut test_config = load_appearance();
    test_config.window_color = mode;
    let preview_resolved = resolve(&test_config, read_dominant());
    let is_selected = selected == mode;
    let scheme_palette = palette(resolved.scheme);

    let mut error = error;
    let on_select = move |_| {
        let mut config = load_appearance();
        config.window_color = mode;
        if let Err(err) = write_appearance(&appearance_path(), &config) {
            error.set(Some(err.to_string()));
        }
    };

    selectable_card(
        caption,
        card_bg,
        window_color_preview(&preview_resolved),
        is_selected,
        resolved.accent,
        scheme_palette,
        on_select,
    )
}

fn window_color_preview(preview_resolved: &Resolved) -> Rect {
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

fn error_label(error: &State<Option<String>>, scheme_palette: &yuki_design::Palette) -> Element {
    if let Some(ref msg) = *error.read() {
        label()
            .text(msg.clone())
            .font_size(CALLOUT.size as f32)
            .font_weight(FontWeight::from(CALLOUT.weight as i32))
            .color(rgba(scheme_palette.danger))
            .max_lines(2)
            .padding(Gaps::new(
                SETTINGS.group_content_padding_x as f32,
                SETTINGS.group_content_padding_x as f32,
                SETTINGS.group_content_padding_x as f32,
                SETTINGS.group_content_padding_x as f32,
            ))
            .into_element()
    } else {
        rect().into_element()
    }
}

fn windows_group(
    config: &AppearanceConfig,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Element {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);
    let palette = resolved.sidebar.palette;
    let current = config.traffic_lights;

    let traffic_button = rect()
        .width(Size::auto())
        .height(Size::px(SETTINGS.popup_height as f32))
        .corner_radius(SETTINGS.card_radius as f32)
        .background(rgba(resolved.toolgroup.bg))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .spacing(8.0)
        .child(
            label()
                .text(traffic_label(current))
                .font_size(TEXT.size as f32)
                .color(rgba(palette.item_ink)),
        )
        .child(traffic_squares(resolved.lights))
        .child(svg_viewer(
            CHEVRON_UP_DOWN_ICON,
            palette.item_ink,
            SETTINGS.popup_chevron,
        ))
        .on_press(move |_| {
            open_traffic_menu(current, resolved, error);
        });

    card_container(
        &resolved,
        vec![row_card(&resolved, "Semáforos", traffic_button.into_element()).into_element()],
    )
    .child(error_label(&error, &scheme_palette))
    .into_element()
}

fn traffic_squares(lights: [Rgba; 3]) -> Rect {
    rect()
        .width(Size::auto())
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .spacing(2.0)
        .children(lights.iter().map(|color| {
            rect()
                .width(Size::px(12.0))
                .height(Size::px(12.0))
                .corner_radius(TITLEBAR.buttons.corner_radius as f32)
                .background(rgba(*color))
                .into_element()
        }))
}

fn traffic_label(style: TrafficLightStyle) -> &'static str {
    match style {
        TrafficLightStyle::Auto => "Automático",
        TrafficLightStyle::Standard => "macOS",
        TrafficLightStyle::Accent => "Acento",
        TrafficLightStyle::Wallpaper => "Fondo de pantalla",
    }
}

fn open_traffic_menu(current: TrafficLightStyle, resolved: Resolved, error: State<Option<String>>) {
    let items = vec![
        traffic_menu_item("auto", "Automático", current == TrafficLightStyle::Auto),
        traffic_menu_item("standard", "macOS", current == TrafficLightStyle::Standard),
        traffic_menu_item("accent", "Acento", current == TrafficLightStyle::Accent),
        traffic_menu_item(
            "wallpaper",
            "Fondo de pantalla",
            current == TrafficLightStyle::Wallpaper,
        ),
    ];
    let error = Rc::new(RefCell::new(error));
    let view = MenuView::new(
        items,
        move |id| {
            let style = match id.as_str() {
                "auto" => TrafficLightStyle::Auto,
                "standard" => TrafficLightStyle::Standard,
                "accent" => TrafficLightStyle::Accent,
                "wallpaper" => TrafficLightStyle::Wallpaper,
                _ => return,
            };
            let mut config = load_appearance();
            config.traffic_lights = style;
            if let Err(err) = write_appearance(&appearance_path(), &config) {
                error.borrow_mut().set(Some(err.to_string()));
            }
            PopupHandle::get().close();
        },
        || {
            PopupHandle::get().close();
        },
        resolved.menu,
    );

    let spec = PopupSpec {
        anchor_rect: (0, 0, 1, 1),
        anchor: PopupAnchor::TopLeft,
        gravity: PopupGravity::BottomRight,
        offset: (0, 0),
        size: (200, 140),
        constraint_adjustment: PopupConstraintAdjustment {
            flip_x: true,
            flip_y: true,
            slide_x: true,
            slide_y: true,
        },
        grab: true,
        on_dismissed: None,
    };
    Popups::get().open(spec, AppComponent::new(view));
}

fn traffic_menu_item(id: &'static str, label_text: &str, checked: bool) -> MenuItem {
    MenuItem::Action {
        id: id.to_string(),
        label: label_text.to_string(),
        shortcut: None,
        enabled: true,
        check: CheckState::Radio(checked),
        icon: None,
    }
}

fn row_card(resolved: &Resolved, label_text: &str, control: Element) -> Rect {
    let scheme_palette = palette(resolved.scheme);
    rect()
        .width(Size::fill())
        .height(Size::px(SETTINGS.group_row_min_height as f32))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .padding(Gaps::new(
            SETTINGS.group_row_padding_y as f32,
            SETTINGS.group_row_padding_x as f32,
            SETTINGS.group_row_padding_y as f32,
            SETTINGS.group_row_padding_x as f32,
        ))
        .spacing(SETTINGS.group_row_padding_x as f32)
        .child(
            label()
                .text(label_text.to_string())
                .font_size(TEXT.size as f32)
                .color(rgba(scheme_palette.text.focused))
                .max_lines(1)
                .width(Size::flex(1.0)),
        )
        .child(control)
}

fn card_container(resolved: &Resolved, rows: Vec<Element>) -> Rect {
    let scheme_palette = palette(resolved.scheme);
    let mut children: Vec<Element> = Vec::with_capacity(rows.len() * 2);
    let last = rows.len().saturating_sub(1);
    for (i, row) in rows.into_iter().enumerate() {
        children.push(row);
        if i != last {
            children.push(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.0))
                    .margin(Gaps::new(
                        0.0,
                        SETTINGS.group_row_padding_x as f32,
                        0.0,
                        SETTINGS.group_row_padding_x as f32,
                    ))
                    .background(rgba(scheme_palette.line))
                    .into_element(),
            );
        }
    }
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .corner_radius(SETTINGS.group_radius as f32)
        .background(rgba(bg_surface(resolved.scheme)))
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

fn radio(
    label_text: &str,
    selected: bool,
    disabled: bool,
    resolved: Resolved,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Rect {
    let scheme_palette = palette(resolved.scheme);
    let fg = if disabled {
        resolved.sidebar.palette.count_ink
    } else {
        scheme_palette.text.focused
    };
    let radio_size = SETTINGS.radio_size as f32;
    let dot_size = SETTINGS.radio_dot as f32;

    let mut dot = rect()
        .width(Size::px(radio_size))
        .height(Size::px(radio_size))
        .corner_radius((radio_size / 2.0).max(0.0))
        .background(rgba(bg_control(resolved.scheme)))
        .center();

    if selected {
        dot = dot.child(
            rect()
                .width(Size::px(dot_size))
                .height(Size::px(dot_size))
                .corner_radius((dot_size / 2.0).max(0.0))
                .background(rgba(Rgba::rgb8(255, 255, 255))),
        );
    }

    let mut row = rect()
        .width(Size::auto())
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .spacing(SETTINGS.radio_gap as f32)
        .cross_align(Alignment::Center)
        .child(dot)
        .child(
            label()
                .text(label_text.to_string())
                .font_size(TEXT.size as f32)
                .color(rgba(fg))
                .max_lines(1),
        );

    if disabled {
        row = row.opacity(0.5);
    } else {
        row = row.on_press(on_press);
    }

    row
}

fn no_results(query: &str, palette: yuki_design::SidebarPalette) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .direction(Direction::Vertical)
        .spacing(SIDEBAR.item_gap as f32)
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .child(svg_viewer(SEARCH_ICON, palette.item_ink, 32))
        .child(
            label()
                .text("Sin resultados")
                .font_size(TITLE_2.size as f32)
                .font_weight(FontWeight::from(TITLE_2.weight as i32))
                .color(rgba(palette.item_ink))
                .max_lines(1),
        )
        .child(
            label()
                .text(format!("No hay ninguna opción que coincida con «{query}»."))
                .font_size(TEXT.size as f32)
                .color(rgba(palette.count_ink))
                .max_lines(2),
        )
}

fn bg_window(scheme: ColorScheme) -> Rgba {
    match scheme {
        ColorScheme::DarkTint => BG_WINDOW_DARK_TINT,
        ColorScheme::Dark => BG_WINDOW_DARK,
        _ => BG_WINDOW_LIGHT,
    }
}

fn bg_control(scheme: ColorScheme) -> Rgba {
    match scheme {
        ColorScheme::DarkTint => BG_CONTROL_DARK_TINT,
        ColorScheme::Dark => BG_CONTROL_DARK,
        _ => BG_CONTROL_LIGHT,
    }
}

fn bg_surface(scheme: ColorScheme) -> Rgba {
    match scheme {
        ColorScheme::DarkTint => BG_SURFACE_DARK_TINT,
        ColorScheme::Dark => BG_SURFACE_DARK,
        _ => BG_SURFACE_LIGHT,
    }
}

fn account_name() -> String {
    full_account_name().unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| "Usuario".into())
    })
}

/// Returns the user's full name from `passwd` (the `pw_gecos` field), or `None`
/// when the lookup fails. The first comma-separated entry is the display name.
fn full_account_name() -> Option<String> {
    // SAFETY: `getpwuid` returns a pointer to static libc data; we only read
    // it and copy the resulting C string before the next call.
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    if pw.is_null() {
        return None;
    }
    let gecos = unsafe { std::ffi::CStr::from_ptr((*pw).pw_gecos) };
    gecos
        .to_str()
        .ok()?
        .split(',')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

fn matches_filter(query: &str, haystack: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let normalized = normalize(query);
    normalize(haystack)
        .split_whitespace()
        .any(|word| word.contains(&normalized))
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .replace('á', "a")
        .replace('é', "e")
        .replace('í', "i")
        .replace('ó', "o")
        .replace('ú', "u")
        .replace('ñ', "n")
}

fn svg_viewer(template: &str, color: Rgba, size: u32) -> SvgViewer {
    let [r, g, b, _] = color.to_rgba8();
    SvgViewer::new((
        format!(
            "settings-icon-{size}-{template_len}",
            template_len = template.len()
        ),
        Bytes::from(template.replace("currentColor", &format!("rgb({r},{g},{b})"))),
    ))
    .width(Size::px(size as f32))
    .height(Size::px(size as f32))
    .show_loader(false)
}

const SEARCH_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"6.8\" cy=\"6.8\" r=\"4.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\"/><path d=\"M10 10l3.2 3.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\"/></svg>";
const CLEAR_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M4 4l8 8M12 4l-8 8\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\"/></svg>";
const BACK_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M10 3L5 8l5 5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const FORWARD_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M6 3l5 5-5 5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const APPEARANCE_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"22\" height=\"22\" viewBox=\"0 0 22 22\"><circle cx=\"11\" cy=\"11\" r=\"9\" fill=\"currentColor\"/><path d=\"M11 2v18\" stroke=\"white\" stroke-width=\"2\"/><path d=\"M2 11h18\" stroke=\"white\" stroke-width=\"2\"/></svg>";
const CHEVRON_UP_DOWN_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"12\" viewBox=\"0 0 12 12\"><path d=\"M2 8l4-4 4 4M2 4l4 4 4-4\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::sync::Mutex;

    use celestial_kit::controls::{ToolButton, ToolGroup};
    use celestial_kit::platform::{DecorationResponse, WindowStatus};
    use celestial_kit::titlebar::Titlebar;
    use freya_core::prelude::Label;
    use freya_testing::TestingRunner;
    use torin::prelude::Size2D;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_config<F: FnOnce(std::path::PathBuf) -> R, R>(f: F) -> R {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "yuki-settings-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let old = env::var_os("YUKI_CONFIG_DIR");
        unsafe { env::set_var("YUKI_CONFIG_DIR", &dir) };
        let result = f(dir.clone());
        match old {
            Some(v) => unsafe { env::set_var("YUKI_CONFIG_DIR", v) },
            None => unsafe { env::remove_var("YUKI_CONFIG_DIR") },
        }
        let _ = fs::remove_dir_all(&dir);
        result
    }

    struct ContentApp {
        query: String,
        config: AppearanceConfig,
        dominant: Option<Rgba>,
    }

    impl App for ContentApp {
        fn render(&self) -> impl IntoElement {
            use_init_theme(|| theme::for_scheme(false));
            let resolved = resolve(&self.config, self.dominant);
            let appearance_error = use_state(|| None::<String>);
            let color_error = use_state(|| None::<String>);
            let windows_error = use_state(|| None::<String>);
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .child(scroll_content(
                    &self.query,
                    &self.config,
                    self.dominant,
                    &resolved,
                    appearance_error,
                    color_error,
                    windows_error,
                ))
        }
    }

    fn run_content(query: &str, config: AppearanceConfig, dominant: Option<Rgba>) -> TestingRunner {
        let app = AppComponent::new(ContentApp {
            query: query.to_string(),
            config,
            dominant,
        });
        let (mut test, _) = TestingRunner::new(app, Size2D::new(600.0, 800.0), |_| {}, 1.0);
        test.sync_and_update();
        test
    }

    fn find_label(test: &TestingRunner, text: &str) -> Option<freya_testing::TestingNode> {
        test.find_many(|node, el| {
            Label::try_downcast(el).and_then(|label| {
                if label.text.contains(text) {
                    Some(node)
                } else {
                    None
                }
            })
        })
        .into_iter()
        .next()
    }

    fn click_node(test: &mut TestingRunner, node: freya_testing::TestingNode) {
        let layout = node.layout();
        let area = layout.area;
        let x = area.origin.x + area.size.width / 2.0;
        let y = area.origin.y + area.size.height / 2.0;
        test.click_cursor((x as f64, y as f64));
        test.sync_and_update();
    }

    #[test]
    fn search_filter_matches_translated_terms() {
        assert!(matches_filter(
            "claro",
            "apariencia tema claro oscuro automático"
        ));
        assert!(matches_filter(
            "acento",
            "color acento ventanas fondo escritorio tinte"
        ));
        assert!(!matches_filter("foo", "apariencia tema claro"));
    }

    #[test]
    fn empty_query_matches_everything() {
        assert!(matches_filter("", "cualquier cosa"));
    }

    #[test]
    fn filter_normalizes_accents() {
        assert!(matches_filter("oscuro", "tema oscuro claro"));
    }

    #[test]
    fn initials_takes_first_two_words() {
        assert_eq!(initials("ana lopez"), "AL");
        assert_eq!(initials("pedro"), "P");
    }

    #[test]
    fn search_without_matches_shows_no_results_label() {
        let test = run_content("xyz", AppearanceConfig::default(), None);
        assert!(
            find_label(&test, "Sin resultados").is_some(),
            "no-results label must be visible"
        );
        assert!(
            find_label(&test, "xyz").is_some(),
            "the empty-state must mention the query"
        );
    }

    #[test]
    fn tint_scope_radios_are_disabled_in_neutral_mode() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig {
                window_color: WindowColorMode::Neutral,
                window_tint_scope: WindowTintScope::Windows,
                ..Default::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_content("ventanas", initial, None);
            let node = find_label(&test, "Todo el escritorio").expect("desktop radio label exists");
            click_node(&mut test, node);

            let text = fs::read_to_string(&path).unwrap();
            assert!(
                text.contains("window_tint_scope = \"windows\""),
                "clicking a disabled radio must not change the scope: {text}"
            );
        });
    }

    #[test]
    fn choosing_dark_writes_color_scheme_dark() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig::default();
            write_appearance(&path, &initial).unwrap();

            let mut test = run_content("", initial, None);
            let node = find_label(&test, "Oscuro").expect("dark theme card label exists");
            click_node(&mut test, node);

            let text = fs::read_to_string(&path).unwrap();
            assert!(
                text.contains("color_scheme = \"dark\""),
                "choosing dark must write the dark scheme: {text}"
            );
        });
    }

    #[test]
    fn choosing_purple_accent_writes_three_decimals() {
        with_temp_config(|_dir| {
            let purple = ACCENT_SWATCHES
                .iter()
                .find(|(name, _)| *name == "Morado")
                .map(|(_, color)| color)
                .expect("purple swatch exists");
            let initial = AppearanceConfig {
                accent_color: Some([purple.r, purple.g, purple.b]),
                ..Default::default()
            };
            write_appearance(&appearance_path(), &initial).unwrap();

            let mut test = run_content("color", initial, None);
            let node = find_label(&test, "Morado").expect("purple accent swatch label exists");
            click_node(&mut test, node);

            let config = load_appearance();
            let accent = config.accent_color.expect("accent color must be written");
            assert!(
                (accent[0] - 0.557).abs() < 0.001
                    && (accent[1] - 0.357).abs() < 0.001
                    && (accent[2] - 0.847).abs() < 0.001,
                "choosing purple must round to three decimals: {accent:?}"
            );
        });
    }

    #[test]
    fn only_selected_swatch_shows_its_name() {
        let purple = ACCENT_SWATCHES
            .iter()
            .find(|(name, _)| *name == "Morado")
            .map(|(_, color)| color)
            .expect("purple swatch exists");
        let config = AppearanceConfig {
            accent_color: Some([purple.r, purple.g, purple.b]),
            ..Default::default()
        };

        let test = run_content("color", config, None);
        let names: Vec<&str> = ACCENT_SWATCHES.iter().map(|(name, _)| *name).collect();
        let visible: Vec<&str> = names
            .iter()
            .filter(|name| find_label(&test, name).is_some())
            .copied()
            .collect();
        assert_eq!(
            visible,
            vec!["Morado"],
            "only the selected accent swatch should display its name: {visible:?}"
        );
    }

    #[test]
    fn pressing_dark_preserves_dark_tint() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig {
                color_scheme: ColorScheme::DarkTint,
                ..Default::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_content("", initial, None);
            let node = find_label(&test, "Oscuro").expect("dark theme card label exists");
            click_node(&mut test, node);

            let config = load_appearance();
            assert_eq!(
                config.color_scheme,
                ColorScheme::DarkTint,
                "pressing Dark while in dark-tint must keep dark-tint"
            );
        });
    }

    #[test]
    fn write_error_is_displayed() {
        with_temp_config(|dir| {
            use std::os::unix::fs::PermissionsExt;

            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig::default();
            write_appearance(&path, &initial).unwrap();

            // Make the config directory read-only so subsequent writes fail.
            let mut permissions = fs::metadata(&dir).unwrap().permissions();
            permissions.set_mode(0o555);
            fs::set_permissions(&dir, permissions).unwrap();

            let mut test = run_content("", initial, None);
            let node = find_label(&test, "Oscuro").expect("dark theme card label exists");
            click_node(&mut test, node);

            assert!(
                find_label(&test, "No se pudo guardar").is_some()
                    || find_label(&test, "Permission denied").is_some()
                    || find_label(&test, "denegado").is_some(),
                "the write error message must be rendered"
            );
        });
    }

    #[test]
    fn default_accent_selects_blue_swatch() {
        let config = AppearanceConfig::default();
        assert!(
            config.accent_color.is_none(),
            "default config has no accent"
        );

        let test = run_content("color", config, None);
        let names: Vec<&str> = ACCENT_SWATCHES.iter().map(|(name, _)| *name).collect();
        let visible: Vec<&str> = names
            .iter()
            .filter(|name| find_label(&test, name).is_some())
            .copied()
            .collect();
        assert_eq!(
            visible,
            vec!["Azul"],
            "without an accent_color the first swatch (Azul) must be selected: {visible:?}"
        );
    }

    #[test]
    fn traffic_button_shows_exactly_three_lights() {
        let test = run_content("ventanas", AppearanceConfig::default(), None);
        let squares = test.find_many(|node, _el| {
            let size = node.layout().area.size;
            if (size.width - 12.0).abs() < 0.5 && (size.height - 12.0).abs() < 0.5 {
                Some(())
            } else {
                None
            }
        });
        assert_eq!(
            squares.len(),
            3,
            "the traffic-light button must show exactly three 12x12 squares: {squares:?}"
        );
    }

    struct TitlebarCase;

    impl App for TitlebarCase {
        fn render(&self) -> impl IntoElement {
            use_init_theme(|| theme::for_scheme(false));
            Titlebar::preview(
                WindowStatus {
                    activated: true,
                    maximized: false,
                    decoration: DecorationResponse::Client,
                    fixed: false,
                },
                "Ajustes del sistema",
                false,
            )
            .with_sidebar(true)
            .title("Apariencia")
            .left(
                ToolGroup::new()
                    .child(ToolButton::new("Atrás", BACK_ICON).disabled(true))
                    .child(ToolButton::new("Adelante", FORWARD_ICON).disabled(true)),
            )
        }
    }

    #[test]
    fn sidebar_titlebar_shows_appearance_title() {
        let app = AppComponent::new(TitlebarCase);
        let (test, _) = TestingRunner::new(app, Size2D::new(400.0, 100.0), |_| {}, 1.0);
        assert!(
            find_label(&test, "Apariencia").is_some(),
            "the sidebar titlebar must show the section title 'Apariencia'"
        );
    }
}
