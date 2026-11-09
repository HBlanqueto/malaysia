use bk::choices::{ChoiceCards, ColorWell, RadioGroup};
use bk::menu::MenuIcon;
use bk::patterns::language::system_language_row;
use bk::patterns::settings::{Pane, PaneGroup, SettingsWindow};
use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::styles::{Axis, RadioGroupStyle};
use bk::views::feedback::Alert;
use bk::views::form::{Form, Row, Section};
use ctappearance::{
    AppearanceConfig, ColorFilter, ColorScheme, MenuBarSize, Resolved, ScrollBars, TintParts,
    TintStrength, Toolkit, TrafficLightShape, TrafficLightStyle, WindowColorMode, WindowTintScope,
    lights_for, load_appearance, palette,
};
use ctdesign::{
    ACCENT_SWATCHES, COLOR_FILTER_INTENSITY, DISPLAY_CONTRAST, MENU_POPOVER, POINTER_SIZES, Rgba,
    SETTINGS, SPACE_3, TEXT_SIZES,
};
use freya_components::svg_viewer::SvgViewer;
use torin::prelude::Gaps;

use crate::bluetooth::{bluetooth_pane, pairing_sheet, use_pairing_prompt};
use crate::bluez::BlueZ;
use crate::bus::use_service;
use crate::energy::{PowerProfiles, UPower, energy_pane};
use crate::localize::{tr, tr_list};
use crate::network::{network_pane, vpn_pane};
use crate::networkmanager::NetworkManager;
use crate::previews::{theme_preview, traffic_shape_preview, window_color_preview};
use crate::state::{account_name, initials, save_config};
use crate::wifi::{Joining, password_sheet, wifi_pane};

const SCHEMES: [ColorScheme; 3] = [ColorScheme::Auto, ColorScheme::Light, ColorScheme::Dark];
const WINDOW_COLORS: [WindowColorMode; 2] = [WindowColorMode::Neutral, WindowColorMode::Accent];
const STRENGTHS: [TintStrength; 4] = [
    TintStrength::Subtle,
    TintStrength::Normal,
    TintStrength::Strong,
    TintStrength::Intense,
];
const SCOPES: [WindowTintScope; 2] = [WindowTintScope::Windows, WindowTintScope::Desktop];
const SCROLL_BARS: [ScrollBars; 2] = [ScrollBars::WhenScrolling, ScrollBars::Always];
const MENU_BAR_SIZES: [MenuBarSize; 2] = [MenuBarSize::Default, MenuBarSize::Large];
const COLOR_FILTERS: [ColorFilter; 4] = [
    ColorFilter::Grayscale,
    ColorFilter::RedGreen,
    ColorFilter::GreenRed,
    ColorFilter::BlueYellow,
];
const ACCESSIBILITY_TINT: Rgba = ACCENT_SWATCHES[0].1;
const COLOR_FILTER_PENCILS: &[u8] = include_bytes!("../assets/color-filters.svg");
const COLOR_FILTER_PENCILS_SIZE: (f32, f32) = (349.0, 130.0);
type Text = fn() -> &'static str;

const SHAPES: [(TrafficLightShape, Text); 2] = [
    (TrafficLightShape::Square, || tr!("traffic-shape-square")),
    (TrafficLightShape::Round, || tr!("traffic-shape-round")),
];
const TRAFFIC: [(TrafficLightStyle, Text); 4] = [
    (TrafficLightStyle::Auto, || tr!("lights-auto")),
    (TrafficLightStyle::Standard, || tr!("lights-standard")),
    (TrafficLightStyle::Accent, || tr!("lights-accent")),
    (TrafficLightStyle::Wallpaper, || tr!("lights-wallpaper")),
];
type Part = fn(&mut TintParts) -> &mut bool;

const PARTS: [(Text, Part); 5] = [
    (|| tr!("tint-part-sidebars"), |p| &mut p.sidebars),
    (|| tr!("tint-part-groups"), |p| &mut p.groups),
    (|| tr!("tint-part-menus"), |p| &mut p.menus),
    (|| tr!("tint-part-window"), |p| &mut p.window),
    (|| tr!("tint-part-controls"), |p| &mut p.controls),
];

pub fn settings_window(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
) -> Rect {
    let mut error = use_state(|| None::<String>);
    let power = use_service::<UPower>();
    let profiles = use_service::<PowerProfiles>();
    let network = use_service::<NetworkManager>();
    let bluetooth = use_service::<BlueZ>();
    let pairing = use_pairing_prompt();
    let joining = use_state(|| None::<Joining>);
    let password = use_state(bk::password::PasswordField::default);
    let name = account_name();
    let window = SettingsWindow::new()
        .account(initials(&name), name, tr!("local-account"))
        .sidebar_toggle(false)
        .sidebar_resizable(false)
        .group(
            [
                wifi_pane(&network, joining),
                bluetooth_pane(&bluetooth),
                network_pane(&network),
                vpn_pane(&network),
                Some(energy_pane(&power, &profiles)),
            ]
            .into_iter()
            .flatten()
            .fold(PaneGroup::new(None), PaneGroup::pane),
        );
    let window = window.group(
        PaneGroup::new(None)
            .pane(
                Pane::new(
                    "accessibility",
                    tr!("accessibility"),
                    Symbol::Accessibility,
                    ACCESSIBILITY_TINT,
                    accessibility(),
                )
                .page(
                    Pane::new(
                        "display",
                        tr!("display"),
                        Symbol::Display,
                        ACCESSIBILITY_TINT,
                        display(config, error),
                    )
                    .page(Pane::new(
                        "text-size",
                        tr!("text-size"),
                        Symbol::Display,
                        ACCESSIBILITY_TINT,
                        text_size(config, error),
                    )),
                ),
            )
            .pane(Pane::new(
                "appearance",
                tr!("appearance"),
                Symbol::CircleHalf,
                SETTINGS.section_icon_bg,
                appearance(config, dominant, resolved, error),
            ))
            .pane(crate::notifications::notifications_pane(error))
            .pane(crate::notifications::focus_pane(error)),
    );
    let message = error.read().clone();
    let failures = [
        power.failure,
        profiles.failure,
        network.failure,
        bluetooth.failure,
    ];
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .child(window)
        .maybe_child(message.map(|message| {
            Alert::new(tr!("save-error-title"), message)
                .confirm(tr!("save-error-ok"), move |_| error.set(None))
                .cancel(tr!("save-error-close"), move |_| error.set(None))
        }))
        .maybe_child(password_sheet(joining, password, &network.remote))
        .maybe_child(pairing_sheet(pairing))
        .children(failures.into_iter().filter_map(failure_alert))
}

fn failure_alert(mut failure: State<Option<String>>) -> Option<Element> {
    let message = failure.read().clone()?;
    Some(
        Alert::new(tr!("action-failed"), message)
            .confirm(tr!("save-error-ok"), move |_| failure.set(None))
            .into_element(),
    )
}

fn edit<T: 'static>(
    error: State<Option<String>>,
    apply: impl Fn(&mut AppearanceConfig, T) + 'static,
) -> Callback<T, ()> {
    Callback::new(move |value| {
        let mut config = load_appearance();
        apply(&mut config, value);
        if let Some(message) = save_config(&config) {
            let mut error = error;
            error.set(Some(message));
        }
    })
}

fn appearance(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Form {
    let tint_disabled = config.window_color == WindowColorMode::Neutral;
    Form::new()
        .section(
            Section::new()
                .row(
                    Row::new(tr!("appearance"))
                        .keywords(tr_list!("appearance-keywords"))
                        .control(scheme_cards(config, dominant, error)),
                )
                .row(system_language_row()),
        )
        .section(
            Section::titled(tr!("color"))
                .row(
                    Row::new(tr!("accent-color"))
                        .keywords(tr_list!("accent-color-keywords"))
                        .control(
                            ColorWell::new(tr!("accent-color"), &ACCENT_SWATCHES)
                                .selected(accent_index(config))
                                .on_change(edit(error, |config, index: usize| {
                                    let color = ACCENT_SWATCHES[index].1;
                                    config.accent_color = Some([color.r, color.g, color.b]);
                                })),
                        ),
                )
                .row(
                    Row::new(tr!("window-color"))
                        .detail(window_color_help(config.window_color))
                        .keywords(tr_list!("window-color-keywords"))
                        .control(window_color_cards(config, dominant, error)),
                )
                .row(
                    Row::new(tr!("tint-strength"))
                        .keywords(tr_list!("tint-strength-keywords"))
                        .control(
                            Slider::stepped(tr!("tint-strength"), STRENGTHS.len())
                                .value(strength_index(config))
                                .labels(tr_list!(
                                    "tint-strength-subtle",
                                    "tint-strength-normal",
                                    "tint-strength-strong",
                                    "tint-strength-intense"
                                ))
                                .disabled(tint_disabled)
                                .on_change(edit(error, |config, index: usize| {
                                    config.tint_strength_accent = STRENGTHS[index];
                                })),
                        ),
                )
                .row(
                    Row::new(tr!("tint-parts"))
                        .detail(tr!("tint-parts-help"))
                        .keywords(tr_list!("tint-parts-keywords"))
                        .control(option_column(tint_parts(
                            config.tint_parts,
                            tint_disabled,
                            error,
                        ))),
                )
                .row(
                    Row::new(tr!("tint-scope"))
                        .keywords(tr_list!("tint-scope-keywords"))
                        .control(option_column(
                            RadioGroup::new(tr!("tint-scope"))
                                .option(tr!("tint-scope-windows"))
                                .option(tr!("tint-scope-desktop"))
                                .selected(position(&SCOPES, config.window_tint_scope))
                                .disabled(tint_disabled)
                                .on_change(edit(error, |config, index: usize| {
                                    config.window_tint_scope = SCOPES[index];
                                })),
                        )),
                ),
        )
        .section(
            Section::titled(tr!("windows"))
                .row(
                    Row::new(tr!("traffic-lights"))
                        .keywords(tr_list!("traffic-lights-keywords"))
                        .control(traffic_popup(config, dominant, resolved, error)),
                )
                .row(
                    Row::new(tr!("traffic-shape"))
                        .keywords(tr_list!("traffic-shape-keywords"))
                        .control(traffic_shape_cards(config, resolved, error)),
                )
                .row(
                    Row::new(tr!("scroll-bars"))
                        .keywords(tr_list!("scroll-bars-keywords"))
                        .control(option_column(
                            RadioGroup::new(tr!("scroll-bars"))
                                .option(tr!("scroll-bars-when-scrolling"))
                                .option(tr!("scroll-bars-always"))
                                .selected(position(&SCROLL_BARS, config.scroll_bars))
                                .on_change(edit(error, |config, index: usize| {
                                    config.scroll_bars = SCROLL_BARS[index];
                                })),
                        )),
                )
                .row(wallpaper_tinting_row(config, error)),
        )
}

fn wallpaper_tinting_row(config: &AppearanceConfig, error: State<Option<String>>) -> Row {
    let accent_tints = config.window_color == WindowColorMode::Accent;
    let row = Row::new(tr!("wallpaper-tinting"))
        .keywords(tr_list!("wallpaper-tinting-keywords"))
        .control(
            Switch::new(tr!("wallpaper-tinting"))
                .on(config.wallpaper_tinting)
                .disabled(accent_tints)
                .on_toggle(edit(error, |config, on: bool| {
                    config.wallpaper_tinting = on;
                })),
        );
    if accent_tints {
        row.detail(tr!("wallpaper-tinting-accent-help"))
    } else {
        row
    }
}

fn accessibility() -> Form {
    Form::new()
        .section(
            Section::new().row(
                Row::new(tr!("accessibility"))
                    .symbol(Symbol::Accessibility, ACCESSIBILITY_TINT)
                    .detail(tr!("accessibility-about")),
            ),
        )
        .section(
            Section::titled(tr!("vision")).row(
                Row::page(tr!("display"), "display").symbol(Symbol::Display, ACCESSIBILITY_TINT),
            ),
        )
}

fn display(config: &AppearanceConfig, error: State<Option<String>>) -> Form {
    let switch = |title: &'static str, on: bool, apply: fn(&mut AppearanceConfig, bool)| {
        Row::new(title).control(
            Switch::new(title)
                .on(on)
                .on_toggle(edit(error, move |config, on: bool| apply(config, on))),
        )
    };
    let reduce_transparency = Row::new(tr!("reduce-transparency")).control(
        Switch::new(tr!("reduce-transparency"))
            .on(config.transparency_reduced())
            .disabled(config.contrast)
            .on_toggle(edit(error, |config, on: bool| {
                config.reduce_transparency = on;
            })),
    );
    let filters_off = !config.color_filters;
    Form::new()
        .section(
            Section::new()
                .row(switch(
                    tr!("invert-colors"),
                    config.invert_colors,
                    |config, on| {
                        config.invert_colors = on;
                        if on && config.color_filter != ColorFilter::Grayscale {
                            config.color_filters = false;
                        }
                    },
                ))
                .row(switch(
                    tr!("increase-contrast"),
                    config.contrast,
                    |config, on| {
                        config.contrast = on;
                    },
                ))
                .row(if config.contrast {
                    reduce_transparency.detail(tr!("reduce-transparency-contrast-help"))
                } else {
                    reduce_transparency
                })
                .row(
                    Row::new(tr!("wallpaper-tinting-in-light"))
                        .detail(tr!("wallpaper-tinting-in-light-help"))
                        .keywords(tr_list!("wallpaper-tinting-in-light-keywords"))
                        .control(
                            Switch::new(tr!("wallpaper-tinting-in-light"))
                                .on(config.wallpaper_tinting_in_light)
                                .disabled(!config.wallpaper_tinting)
                                .on_toggle(edit(error, |config, on: bool| {
                                    config.wallpaper_tinting_in_light = on;
                                })),
                        ),
                )
                .row(switch(
                    tr!("show-borders"),
                    config.show_borders,
                    |config, on| {
                        config.show_borders = on;
                    },
                ))
                .row(switch(
                    tr!("differentiate-without-color"),
                    config.differentiate_without_color,
                    |config, on| config.differentiate_without_color = on,
                ))
                .row(
                    Row::new(tr!("display-contrast")).control(option_column(
                        Slider::stepped(tr!("display-contrast"), DISPLAY_CONTRAST.len())
                            .value(nearest(&DISPLAY_CONTRAST, config.display_contrast))
                            .labels(tr_list!(
                                "display-contrast-normal",
                                "step-blank",
                                "step-blank",
                                "step-blank",
                                "step-blank",
                                "step-blank",
                                "display-contrast-maximum"
                            ))
                            .on_change(edit(error, |config, index: usize| {
                                config.display_contrast = DISPLAY_CONTRAST[index];
                            })),
                    )),
                ),
        )
        .section(
            Section::titled(tr!("text"))
                .row(Row::page(tr!("text-size"), "text-size").detail(tr!("text-size-help")))
                .row(
                    Row::new(tr!("menu-bar-size")).control(
                        RadioGroup::new(tr!("menu-bar-size"))
                            .style(|style| RadioGroupStyle {
                                axis: Axis::Horizontal,
                                ..style
                            })
                            .option(tr!("menu-bar-size-default"))
                            .option(tr!("menu-bar-size-large"))
                            .selected(position(&MENU_BAR_SIZES, config.menu_bar_size))
                            .on_change(edit(error, |config, index: usize| {
                                config.menu_bar_size = MENU_BAR_SIZES[index];
                            })),
                    ),
                ),
        )
        .section(
            Section::titled(tr!("pointer"))
                .row(
                    switch(
                        tr!("shake-to-locate"),
                        config.shake_to_locate,
                        |config, on| {
                            config.shake_to_locate = on;
                        },
                    )
                    .detail(tr!("shake-to-locate-help")),
                )
                .row(
                    Row::new(tr!("pointer-size")).control(option_column(
                        Slider::stepped(tr!("pointer-size"), POINTER_SIZES.len())
                            .value(nearest(&POINTER_SIZES, config.pointer_size))
                            .labels(tr_list!(
                                "pointer-size-normal",
                                "step-blank",
                                "step-blank",
                                "step-blank",
                                "step-blank",
                                "step-blank",
                                "pointer-size-large"
                            ))
                            .on_change(edit(error, |config, index: usize| {
                                config.pointer_size = POINTER_SIZES[index];
                            })),
                    )),
                ),
        )
        .section(
            Section::titled(tr!("color-filters"))
                .illustration(color_filter_pencils())
                .row(switch(
                    tr!("color-filters"),
                    config.color_filters,
                    |config, on| {
                        config.color_filters = on;
                        if on && config.color_filter != ColorFilter::Grayscale {
                            config.invert_colors = false;
                        }
                    },
                ))
                .row(
                    Row::new(tr!("color-filter-type")).control(
                        COLOR_FILTER_NAMES
                            .iter()
                            .fold(PopUpButton::new(tr!("color-filter-type")), |popup, name| {
                                popup.item(name())
                            })
                            .selected(position(&COLOR_FILTERS, config.color_filter))
                            .on_change(edit(error, |config, index: usize| {
                                config.color_filter = COLOR_FILTERS[index];
                                if COLOR_FILTERS[index] != ColorFilter::Grayscale {
                                    config.invert_colors = false;
                                }
                            })),
                    ),
                )
                .row(
                    Row::new(tr!("color-filter-intensity")).control(option_column(
                        Slider::stepped(
                            tr!("color-filter-intensity"),
                            COLOR_FILTER_INTENSITY.len(),
                        )
                        .value(nearest(
                            &COLOR_FILTER_INTENSITY,
                            config.color_filter_intensity,
                        ))
                        .labels(tr_list!(
                            "color-filter-intensity-low",
                            "step-blank",
                            "step-blank",
                            "step-blank",
                            "step-blank",
                            "step-blank",
                            "color-filter-intensity-high"
                        ))
                        .disabled(filters_off)
                        .on_change(edit(error, |config, index: usize| {
                            config.color_filter_intensity = COLOR_FILTER_INTENSITY[index];
                        })),
                    )),
                ),
        )
}

const TOOLKITS: [(Toolkit, Text, Text); 3] = [
    (
        Toolkit::BarrierKit,
        || tr!("text-size-barrierkit"),
        || tr!("text-size-barrierkit-help"),
    ),
    (
        Toolkit::Gtk,
        || tr!("text-size-gtk"),
        || tr!("text-size-gtk-help"),
    ),
    (
        Toolkit::Qt,
        || tr!("text-size-qt"),
        || tr!("text-size-qt-help"),
    ),
];

fn text_size(config: &AppearanceConfig, error: State<Option<String>>) -> Form {
    let apps = TOOLKITS.iter().fold(
        Section::titled(tr!("text-size-apps")),
        |section, &(toolkit, title, help)| {
            let mut sizes = config.text_sizes;
            let selected = sizes
                .get_mut(toolkit)
                .map_or(0, |size| 1 + nearest(&TEXT_SIZES, size));
            section.row(
                Row::new(title()).detail(help()).control(
                    text_size_names()
                        .iter()
                        .fold(PopUpButton::new(title()), |popup, name| popup.item(name))
                        .selected(selected)
                        .on_change(edit(error, move |config, index: usize| {
                            *config.text_sizes.get_mut(toolkit) =
                                index.checked_sub(1).map(|step| TEXT_SIZES[step]);
                        })),
                ),
            )
        },
    );
    Form::new()
        .section(
            Section::new().row(
                Row::new(tr!("text-size-preferred"))
                    .detail(tr!("text-size-preferred-help"))
                    .control(option_column(
                        Slider::stepped(tr!("text-size-preferred"), TEXT_SIZES.len())
                            .value(nearest(&TEXT_SIZES, config.text_size))
                            .labels(tr_list!(
                                "text-size-default",
                                "step-blank",
                                "step-blank",
                                "step-blank",
                                "step-blank",
                                "text-size-largest"
                            ))
                            .on_change(edit(error, |config, index: usize| {
                                config.text_size = TEXT_SIZES[index];
                            })),
                    )),
            ),
        )
        .section(apps)
}

fn text_size_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        std::iter::once(tr!("text-size-use-preferred"))
            .chain(
                TEXT_SIZES
                    .iter()
                    .map(|size| &*Box::leak(format!("{:.0} %", size * 100.0).into_boxed_str())),
            )
            .collect()
    })
}

const COLOR_FILTER_NAMES: [Text; 4] = [
    || tr!("color-filter-grayscale"),
    || tr!("color-filter-red-green"),
    || tr!("color-filter-green-red"),
    || tr!("color-filter-blue-yellow"),
];

fn color_filter_pencils() -> Rect {
    let (width, height) = COLOR_FILTER_PENCILS_SIZE;
    rect()
        .width(Size::fill())
        .padding(Gaps::new(SPACE_3, 0.0, 0.0, 0.0))
        .main_align(Alignment::Center)
        .direction(Direction::Horizontal)
        .child(
            SvgViewer::new((
                "color-filter-pencils",
                Bytes::from_static(COLOR_FILTER_PENCILS),
            ))
            .width(Size::px(width))
            .height(Size::px(height))
            .show_loader(false),
        )
}

fn nearest(steps: &[f32], value: f32) -> usize {
    (0..steps.len())
        .min_by(|&a, &b| {
            (steps[a] - value)
                .abs()
                .total_cmp(&(steps[b] - value).abs())
        })
        .unwrap_or(0)
}

fn scheme_cards(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    error: State<Option<String>>,
) -> ChoiceCards {
    let selected = match config.color_scheme {
        ColorScheme::DarkTint => 2,
        scheme => position(&SCHEMES, scheme),
    };
    [tr!("scheme-auto"), tr!("scheme-light"), tr!("scheme-dark")]
        .into_iter()
        .zip(SCHEMES)
        .fold(
            ChoiceCards::new(tr!("appearance")),
            |cards, (caption, scheme)| {
                cards.option(caption, theme_preview(scheme, config, dominant))
            },
        )
        .selected(selected)
        .on_change(edit(error, |config, index: usize| {
            config.color_scheme = SCHEMES[index];
        }))
}

fn window_color_cards(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    error: State<Option<String>>,
) -> ChoiceCards {
    [tr!("window-color-neutral"), tr!("window-color-accent")]
        .into_iter()
        .zip(WINDOW_COLORS)
        .fold(
            ChoiceCards::new(tr!("window-color")),
            |cards, (caption, mode)| {
                cards.option(caption, window_color_preview(config, mode, dominant))
            },
        )
        .selected(position(&WINDOW_COLORS, config.window_color))
        .on_change(edit(error, |config, index: usize| {
            config.window_color = WINDOW_COLORS[index];
        }))
}

fn tint_parts(parts: TintParts, disabled: bool, error: State<Option<String>>) -> VStack {
    VStack::new()
        .main_align(Alignment::Start)
        .cross_align(Alignment::Start)
        .spacing(SETTINGS.check.row_gap as f32)
        .children(PARTS.iter().map(|&(label, part)| {
            let mut current = parts;
            Checkbox::new(label())
                .checked(*part(&mut current))
                .disabled(disabled)
                .on_toggle(edit(error, move |config, checked: bool| {
                    *part(&mut config.tint_parts) = checked;
                }))
                .into_element()
        }))
}

fn option_column(control: impl IntoElement) -> Rect {
    rect()
        .width(Size::px(SETTINGS.option_column as f32))
        .child(control)
}

fn traffic_shape_cards(
    config: &AppearanceConfig,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> ChoiceCards {
    SHAPES
        .iter()
        .fold(
            ChoiceCards::new(tr!("traffic-shape")),
            |cards, &(shape, caption)| {
                cards.option(caption(), traffic_shape_preview(resolved, shape))
            },
        )
        .selected(position(
            &SHAPES.map(|(shape, _)| shape),
            config.traffic_light_shape,
        ))
        .on_change(edit(error, |config, index: usize| {
            config.traffic_light_shape = SHAPES[index].0;
        }))
}

fn traffic_popup(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> PopUpButton {
    TRAFFIC
        .into_iter()
        .fold(
            PopUpButton::new(tr!("traffic-lights")),
            |popup, (style, label)| {
                let lights = traffic_lights(style, config, resolved, dominant);
                let svg = traffic_row_svg(lights, config.traffic_light_shape);
                popup.icon_item(label(), MenuIcon::Svg(svg.into()))
            },
        )
        .selected(position(
            &TRAFFIC.map(|(style, _)| style),
            config.traffic_lights,
        ))
        .on_change(edit(error, |config, index: usize| {
            config.traffic_lights = TRAFFIC[index].0;
        }))
}

fn traffic_lights(
    style: TrafficLightStyle,
    config: &AppearanceConfig,
    resolved: &Resolved,
    dominant: Option<Rgba>,
) -> [Rgba; 3] {
    let normal_accent = config
        .accent_color
        .map(|[r, g, b]| Rgba { r, g, b, a: 1.0 })
        .unwrap_or(palette(resolved.scheme).accent);
    match style {
        TrafficLightStyle::Auto => resolved.lights,
        style => lights_for(style, normal_accent, dominant),
    }
}

fn traffic_row_svg(lights: [Rgba; 3], shape: TrafficLightShape) -> String {
    let size = MENU_POPOVER.swatch_grid_size;
    let dot = MENU_POPOVER.swatch_grid_dot;
    let gap = MENU_POPOVER.swatch_grid_gap;
    let corner = shape.corner_radius(dot);
    let y = (size - dot) / 2.0;
    let circles: String = lights
        .iter()
        .enumerate()
        .map(|(index, light)| {
            let x = index as f32 * (dot + gap);
            let [r, g, b, _] = light.to_rgba8();
            format!(
                r#"<rect x="{x}" y="{y}" width="{dot}" height="{dot}" rx="{corner}" fill="rgb({r},{g},{b})"/>"#
            )
        })
        .collect();
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">{circles}</svg>"#
    )
}

fn window_color_help(mode: WindowColorMode) -> &'static str {
    match mode {
        WindowColorMode::Neutral => tr!("window-color-neutral-help"),
        WindowColorMode::Accent => tr!("window-color-accent-help"),
    }
}

fn accent_index(config: &AppearanceConfig) -> Option<usize> {
    match config.accent_color {
        None => Some(0),
        Some(rgb) => ACCENT_SWATCHES
            .iter()
            .position(|(_, color)| [color.r, color.g, color.b] == rgb),
    }
}

fn strength_index(config: &AppearanceConfig) -> usize {
    let strength = match config.window_color {
        WindowColorMode::Accent => config.tint_strength_accent,
        WindowColorMode::Neutral => TintStrength::default(),
    };
    position(&STRENGTHS, strength)
}

fn position<T: PartialEq>(options: &[T], value: T) -> usize {
    options
        .iter()
        .position(|option| *option == value)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_traffic_row_icon_fits_the_menu_leading_column() {
        let row = 3.0 * MENU_POPOVER.swatch_grid_dot + 2.0 * MENU_POPOVER.swatch_grid_gap;
        assert!(row <= MENU_POPOVER.swatch_grid_size);
        const { assert!(MENU_POPOVER.swatch_grid_size <= MENU_POPOVER.leading_column) };
        let svg = traffic_row_svg([Rgba::TRANSPARENT; 3], TrafficLightShape::Square);
        assert_eq!(svg.matches("<rect").count(), 3);
    }
}
