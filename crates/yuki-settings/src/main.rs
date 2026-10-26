//! Ajustes del sistema de Yuki.
//!
//! Ventana de preferencias con sidebar, toolbar y contenido desplazable.
//! Todos los colores y medidas salen de `yuki-design`, `yuki-appearance` y los
//! componentes de `celestial-kit`; no hay literales de diseño.

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
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_appearance::{
    AppearanceConfig, ColorScheme, Resolved, TrafficLightStyle, WindowColorMode, WindowTintScope,
    appearance_path, load_appearance, palette, read_dominant, resolve, write_appearance,
};
use yuki_design::{
    ACCENT_SWATCHES, BG_WINDOW_DARK, BG_WINDOW_DARK_TINT, BG_WINDOW_LIGHT, Rgba, SETTINGS, SIDEBAR,
    TEXT, TEXT_LINE_HEIGHT, TITLE_2, TITLEBAR, WINDOW,
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
            .max_size(Some((SETTINGS.window_width, u32::MAX))),
    )
    .expect("yuki-settings");
}

struct SettingsApp;

impl App for SettingsApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(theme::shell);

        let search = use_state(String::new);
        let config = load_appearance();
        let dominant = read_dominant();
        let resolved = resolve(&config, dominant);
        let query = search.read().to_lowercase();

        SidebarLayout::new(
            sidebar(search, &config, &resolved),
            content(&config, dominant, &resolved, &query),
        )
    }
}

fn sidebar(mut search: State<String>, _config: &AppearanceConfig, resolved: &Resolved) -> Sidebar {
    let resolved = *resolved;
    let sidebar_palette = resolved.sidebar.palette;
    let scheme_palette = palette(resolved.scheme);

    let search_field = rect()
        .width(Size::fill())
        .height(Size::px((TEXT.size as f32 * TEXT_LINE_HEIGHT).max(30.0)))
        .corner_radius(999.0)
        .background(rgba(resolved.toolgroup.bg))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .spacing(8.0)
        .child(svg_viewer(
            SEARCH_ICON,
            sidebar_palette.item_ink,
            SIDEBAR.item_icon,
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
        .spacing(SIDEBAR.item_gap as f32)
        .cross_align(Alignment::Center)
        .padding(Gaps::new(
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
        ))
        .child(
            rect()
                .width(Size::px(36.0))
                .height(Size::px(36.0))
                .corner_radius(18.0)
                .background(rgba(resolved.accent))
                .center()
                .child(
                    label()
                        .text(initials(&account_name()))
                        .font_size(14.0)
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
                        .font_size(SIDEBAR.item_text_size as f32)
                        .font_weight(FontWeight::MEDIUM)
                        .color(rgba(sidebar_palette.item_ink))
                        .max_lines(1),
                )
                .child(
                    label()
                        .text("Cuenta local")
                        .font_size(11.0)
                        .color(rgba(sidebar_palette.count_ink))
                        .max_lines(1),
                ),
        );

    Sidebar::new()
        .traffic_lights(true)
        .on_close(move |()| std::process::exit(0))
        .child(search_field)
        .child(account)
        .child(rect().width(Size::fill()).height(Size::flex(1.0)))
        .child(
            SidebarItem::new("Apariencia")
                .selected(true)
                .app_icon(APPEARANCE_ICON, scheme_palette.accent)
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
) -> impl IntoElement {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);
    let bg = bg_window(resolved.scheme);

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(rgba(bg))
        .direction(Direction::Vertical)
        .child(
            Titlebar::new()
                .with_sidebar(true)
                .left(
                    ToolGroup::new()
                        .child(ToolButton::new("Atrás", BACK_ICON).disabled(true))
                        .child(ToolButton::new("Adelante", FORWARD_ICON).disabled(true)),
                )
                .right(
                    label()
                        .text("Apariencia")
                        .font_size(TITLE_2.size as f32)
                        .font_weight(FontWeight::from(TITLE_2.weight as i32))
                        .color(rgba(scheme_palette.text.focused)),
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
                        .child(scroll_content(query, config, dominant, &resolved)),
                ),
        )
        .child(resize_edges())
}

fn scroll_content(
    query: &str,
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
) -> impl IntoElement {
    let resolved = *resolved;
    let palette = resolved.sidebar.palette;

    let mut children: Vec<Element> = Vec::new();

    let appearance_visible = matches_filter(query, "apariencia tema claro oscuro automático");
    if appearance_visible {
        children.push(group_title("", palette.section_ink));
        children.push(appearance_group(config, dominant, &resolved));
    }

    let color_visible = matches_filter(query, "color acento ventanas fondo escritorio tinte");
    if color_visible {
        children.push(group_title("Color", palette.section_ink));
        children.push(color_group(config, &resolved));
    }

    let windows_visible = matches_filter(query, "ventanas semáforos");
    if windows_visible {
        children.push(group_title("Ventanas", palette.section_ink));
        children.push(windows_group(config, &resolved));
    }

    if children.is_empty() {
        return no_results(query, palette).into_element();
    }

    rect()
        .width(Size::fill())
        .height(Size::auto())
        .direction(Direction::Vertical)
        .spacing(SIDEBAR.section_margin_top as f32)
        .children(children)
        .into_element()
}

fn group_title(label_text: &str, color: Rgba) -> Element {
    if label_text.is_empty() {
        return rect()
            .width(Size::fill())
            .height(Size::px(0.0))
            .into_element();
    }
    label()
        .text(label_text.to_string())
        .font_size(TEXT.size as f32)
        .font_weight(FontWeight::from(TEXT.weight as i32))
        .color(rgba(color))
        .max_lines(1)
        .into_element()
}

fn appearance_group(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
) -> Element {
    let resolved = *resolved;
    let scheme_palette = palette(resolved.scheme);

    let selected = match config.color_scheme {
        ColorScheme::Light => "light",
        ColorScheme::Auto => "auto",
        ColorScheme::Dark | ColorScheme::DarkTint => "dark",
    };

    card_container(&resolved)
        .child(
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .spacing(SETTINGS.card_gap as f32)
                .padding(Gaps::new(
                    SIDEBAR.item_padding_x as f32,
                    SIDEBAR.item_padding_x as f32,
                    SIDEBAR.item_padding_x as f32,
                    SIDEBAR.item_padding_x as f32,
                ))
                .child(theme_card(
                    "Claro",
                    ColorScheme::Light,
                    "light",
                    selected,
                    preview_color(ColorScheme::Light, dominant),
                    resolved,
                ))
                .child(theme_card(
                    "Oscuro",
                    ColorScheme::Dark,
                    "dark",
                    selected,
                    preview_color(ColorScheme::Dark, dominant),
                    resolved,
                ))
                .child(theme_card(
                    "Automático",
                    ColorScheme::Auto,
                    "auto",
                    selected,
                    preview_color(ColorScheme::Auto, dominant),
                    resolved,
                )),
        )
        .child(
            label()
                .text("Elige el aspecto general del escritorio.")
                .font_size(11.0)
                .color(rgba(scheme_palette.text.focused))
                .max_lines(2)
                .padding(Gaps::new(
                    SIDEBAR.item_padding_x as f32,
                    SIDEBAR.item_padding_x as f32,
                    SIDEBAR.item_padding_x as f32,
                    SIDEBAR.item_padding_x as f32,
                )),
        )
        .into_element()
}

fn preview_color(scheme: ColorScheme, dominant: Option<Rgba>) -> Rgba {
    match scheme {
        ColorScheme::Light => BG_WINDOW_LIGHT,
        ColorScheme::Dark | ColorScheme::DarkTint => BG_WINDOW_DARK,
        ColorScheme::Auto => dominant.unwrap_or(yuki_design::LIGHT.accent),
    }
}

fn theme_card(
    caption: &'static str,
    scheme: ColorScheme,
    id: &'static str,
    selected: &str,
    preview: Rgba,
    resolved: Resolved,
) -> Rect {
    let is_selected = selected == id;
    let accent = resolved.accent;
    let scheme_palette = palette(resolved.scheme);
    let preview_fg = if scheme == ColorScheme::Light {
        scheme_palette.text.focused
    } else {
        scheme_palette.title.focused
    };

    let on_select = move |_| {
        let mut config = load_appearance();
        config.color_scheme = scheme;
        apply_change(config);
    };

    let mut card = rect()
        .width(Size::px(SETTINGS.card_width as f32))
        .height(Size::px(SETTINGS.card_height as f32))
        .corner_radius(SETTINGS.card_radius as f32)
        .background(rgba(preview))
        .direction(Direction::Vertical)
        .padding(Gaps::new(4.0, 4.0, 4.0, 4.0))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .corner_radius((SETTINGS.card_radius as f32 - 2.0).max(0.0))
                .background(rgba(preview))
                .child(window_preview_mini(resolved, scheme, preview_fg))
                .on_press(on_select),
        )
        .child(
            label()
                .text(caption)
                .font_size(11.0)
                .font_weight(if is_selected {
                    FontWeight::SEMI_BOLD
                } else {
                    FontWeight::MEDIUM
                })
                .color(rgba(if is_selected {
                    accent
                } else {
                    scheme_palette.text.focused
                }))
                .max_lines(1)
                .on_press(on_select),
        );

    if is_selected {
        card = card.border(
            Border::new()
                .width(SETTINGS.card_ring as f32)
                .alignment(BorderAlignment::Outer)
                .fill(rgba(accent)),
        );
    }

    card
}

fn color_group(config: &AppearanceConfig, resolved: &Resolved) -> Element {
    let resolved = *resolved;
    let wallpaper_mode = config.window_color == WindowColorMode::Wallpaper;

    let swatches = rect()
        .width(Size::fill())
        .direction(Direction::Horizontal)
        .spacing(SETTINGS.swatch_gap as f32)
        .children(ACCENT_SWATCHES.iter().map(|(name, color)| {
            let selected = config.accent_color == Some([color.r, color.g, color.b]);
            accent_swatch(name, *color, selected, wallpaper_mode).into_element()
        }));

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
                resolved,
            ))
            .child(window_color_card(
                "Acento",
                WindowColorMode::Accent,
                selected_window_mode,
                resolved,
            ))
            .child(window_color_card(
                "Fondo",
                WindowColorMode::Wallpaper,
                selected_window_mode,
                resolved,
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
            .spacing(SIDEBAR.item_gap as f32)
            .child(radio(
                "Solo las ventanas",
                config.window_tint_scope == WindowTintScope::Windows,
                scope_disabled,
                move |_| {
                    if !scope_disabled {
                        let mut config = load_appearance();
                        config.window_tint_scope = WindowTintScope::Windows;
                        apply_change(config);
                    }
                },
            ))
            .child(radio(
                "Todo el escritorio",
                config.window_tint_scope == WindowTintScope::Desktop,
                scope_disabled,
                move |_| {
                    if !scope_disabled {
                        let mut config = load_appearance();
                        config.window_tint_scope = WindowTintScope::Desktop;
                        apply_change(config);
                    }
                },
            ))
            .into_element(),
    );

    let note: Element = if wallpaper_mode {
        label()
            .text("Lo pone el fondo de pantalla.")
            .font_size(11.0)
            .color(rgba(resolved.sidebar.palette.count_ink))
            .max_lines(1)
            .into_element()
    } else {
        rect().into_element()
    };

    card_container(&resolved)
        .child(accent_row)
        .child(window_mode_row)
        .child(scope_row)
        .child(note)
        .into_element()
}

fn accent_swatch(name: &'static str, color: Rgba, selected: bool, disabled: bool) -> Rect {
    let mut swatch = rect()
        .width(Size::px(SETTINGS.swatch_size as f32))
        .height(Size::px(SETTINGS.swatch_size as f32))
        .corner_radius((SETTINGS.swatch_size as f32 / 2.0).max(0.0))
        .background(rgba(color))
        .center();

    if selected {
        swatch = swatch.child(
            rect()
                .width(Size::px(8.0))
                .height(Size::px(8.0))
                .corner_radius(4.0)
                .background(rgba(yuki_design::LIGHT.text.focused)),
        );
    }

    let row = rect()
        .width(Size::auto())
        .height(Size::auto())
        .direction(Direction::Vertical)
        .spacing(2.0)
        .cross_align(Alignment::Center)
        .child(swatch)
        .child(
            label()
                .text(name)
                .font_size(10.0)
                .color(rgba(yuki_design::LIGHT.text.focused))
                .max_lines(1),
        );

    if disabled {
        row.opacity(0.4)
    } else {
        row.on_press(move |_| {
            let mut config = load_appearance();
            config.accent_color = Some([color.r, color.g, color.b]);
            apply_change(config);
        })
    }
}

fn window_color_card(
    caption: &'static str,
    mode: WindowColorMode,
    selected: WindowColorMode,
    resolved: Resolved,
) -> Rect {
    let mut test_config = load_appearance();
    test_config.window_color = mode;
    let preview_resolved = resolve(&test_config, read_dominant());
    let bg = preview_resolved.toolgroup.bg;
    let is_selected = selected == mode;
    let scheme_palette = palette(resolved.scheme);

    let on_select = move |_| {
        let mut config = load_appearance();
        config.window_color = mode;
        apply_change(config);
    };

    let mut card = rect()
        .width(Size::px(SETTINGS.card_width as f32))
        .height(Size::px(SETTINGS.card_height as f32))
        .corner_radius(SETTINGS.card_radius as f32)
        .background(rgba(bg))
        .direction(Direction::Vertical)
        .padding(Gaps::new(4.0, 4.0, 4.0, 4.0))
        .child(
            window_preview_mini(
                preview_resolved,
                resolved.scheme,
                scheme_palette.text.focused,
            )
            .on_press(on_select),
        )
        .child(
            label()
                .text(caption)
                .font_size(11.0)
                .font_weight(if is_selected {
                    FontWeight::SEMI_BOLD
                } else {
                    FontWeight::MEDIUM
                })
                .color(rgba(if is_selected {
                    resolved.accent
                } else {
                    scheme_palette.text.focused
                }))
                .max_lines(1)
                .on_press(on_select),
        );

    if is_selected {
        card = card.border(
            Border::new()
                .width(SETTINGS.card_ring as f32)
                .alignment(BorderAlignment::Outer)
                .fill(rgba(resolved.accent)),
        );
    }

    card
}

fn windows_group(config: &AppearanceConfig, resolved: &Resolved) -> Element {
    let resolved = *resolved;
    let palette = resolved.sidebar.palette;
    let current = config.traffic_lights;

    let mut test_config = config.clone();
    test_config.window_color = WindowColorMode::Neutral;
    let neutral_lights = resolve(&test_config, read_dominant()).lights;
    test_config.window_color = WindowColorMode::Accent;
    let accent_lights = resolve(&test_config, read_dominant()).lights;
    test_config.window_color = WindowColorMode::Wallpaper;
    let wallpaper_lights = resolve(&test_config, read_dominant()).lights;

    let traffic_button = rect()
        .width(Size::auto())
        .height(Size::px(30.0))
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
        .child(
            rect()
                .width(Size::auto())
                .height(Size::auto())
                .direction(Direction::Horizontal)
                .spacing(4.0)
                .child(traffic_squares(neutral_lights))
                .child(traffic_squares(accent_lights))
                .child(traffic_squares(wallpaper_lights)),
        )
        .child(svg_viewer(
            CHEVRON_UP_DOWN_ICON,
            palette.item_ink,
            SIDEBAR.item_icon,
        ))
        .on_press(move |_| {
            open_traffic_menu(current, resolved);
        });

    row_card(&resolved, "Semáforos", traffic_button.into_element()).into_element()
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

fn open_traffic_menu(current: TrafficLightStyle, resolved: Resolved) {
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
            let _ = write_appearance(&appearance_path(), &config);
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
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .main_align(Alignment::SpaceBetween)
        .padding(Gaps::new(
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
            SIDEBAR.item_padding_x as f32,
        ))
        .child(
            label()
                .text(label_text.to_string())
                .font_size(TEXT.size as f32)
                .color(rgba(scheme_palette.text.focused))
                .max_lines(1),
        )
        .child(control)
}

fn card_container(resolved: &Resolved) -> Rect {
    let scheme_palette = palette(resolved.scheme);
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .corner_radius(WINDOW.corner_radius as f32)
        .background(rgba(scheme_palette.titlebar_bg.focused))
        .direction(Direction::Vertical)
        .spacing(1.0)
}

fn window_preview_mini(resolved: Resolved, scheme: ColorScheme, text: Rgba) -> Rect {
    let dark = matches!(scheme, ColorScheme::Dark | ColorScheme::DarkTint);
    let titlebar_bg = if dark {
        BG_WINDOW_DARK
    } else {
        BG_WINDOW_LIGHT
    };
    rect()
        .width(Size::fill())
        .height(Size::flex(1.0))
        .corner_radius((SETTINGS.card_radius as f32 - 4.0).max(0.0))
        .background(rgba(titlebar_bg))
        .direction(Direction::Vertical)
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(14.0))
                .background(rgba(resolved.toolgroup.bg))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
                .spacing(3.0)
                .children(resolved.lights.iter().map(|color| {
                    rect()
                        .width(Size::px(8.0))
                        .height(Size::px(8.0))
                        .corner_radius(TITLEBAR.buttons.corner_radius as f32)
                        .background(rgba(*color))
                        .into_element()
                })),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .center()
                .child(
                    label()
                        .text("Aa")
                        .font_size(10.0)
                        .color(rgba(text))
                        .max_lines(1),
                ),
        )
}

fn radio(
    label_text: &str,
    selected: bool,
    disabled: bool,
    on_press: impl Fn(Event<PressEventData>) + 'static,
) -> Rect {
    let resolved = resolve(&load_appearance(), read_dominant());
    let scheme_palette = palette(resolved.scheme);
    let fg = if disabled {
        resolved.sidebar.palette.count_ink
    } else {
        scheme_palette.text.focused
    };
    let accent = resolved.accent;

    let mut dot = rect()
        .width(Size::px(16.0))
        .height(Size::px(16.0))
        .corner_radius(8.0)
        .background(rgba(resolved.toolgroup.bg))
        .center();

    if selected {
        dot = dot.child(
            rect()
                .width(Size::px(8.0))
                .height(Size::px(8.0))
                .corner_radius(4.0)
                .background(rgba(accent)),
        );
    }

    let mut row = rect()
        .width(Size::auto())
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .spacing(SIDEBAR.item_gap as f32)
        .cross_align(Alignment::Center)
        .child(dot)
        .child(
            label()
                .text(label_text.to_string())
                .font_size(TEXT.size as f32)
                .color(rgba(fg))
                .max_lines(1),
        );

    if !disabled {
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

fn apply_change(config: AppearanceConfig) {
    let _ = write_appearance(&appearance_path(), &config);
}

fn bg_window(scheme: ColorScheme) -> Rgba {
    match scheme {
        ColorScheme::DarkTint => BG_WINDOW_DARK_TINT,
        ColorScheme::Dark => BG_WINDOW_DARK,
        _ => BG_WINDOW_LIGHT,
    }
}

fn account_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "Usuario".into())
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
const CHEVRON_UP_DOWN_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"12\" viewBox=\"0 0 12 12\"><path d=\"M2 4l4 4 4-4\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::sync::Mutex;

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
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .child(scroll_content(
                    &self.query,
                    &self.config,
                    self.dominant,
                    &resolved,
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
            let initial = AppearanceConfig::default();
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
}
