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
    AppearanceConfig, BlurBehind, ColorFilter, ColorScheme, MenuBarSize, Resolved, ScrollBarClick,
    ScrollBars, TintParts, TintStrength, Toolkit, TrafficLightShape, TrafficLightStyle,
    WindowColorMode, lights_for, load_appearance, palette,
};
use ctdesign::{
    ACCENT_SWATCHES, COLOR_FILTER_INTENSITY, DISPLAY_CONTRAST, MENU_POPOVER, POINTER_SIZES, Rgba,
    SETTINGS, SPACE_3, TEXT_SIZES,
};
use freya_components::svg_viewer::SvgViewer;
use torin::prelude::Gaps;

use crate::bluetooth::{bluetooth_pane, pairing_sheet, use_busy_device, use_pairing_prompt};
use crate::collection::collection_sheet;
use crate::crop::crop_sheet;
use crate::desktop::desktop_pane;
use crate::energy::energy_pane;
use crate::freedesktop::{Themes, confirm, freedesktop, own_themes};
use crate::localize::{tr, tr_list};
use crate::network::{network_pane, vpn_pane};
use crate::previews::theme_preview;
use crate::state::{account_name, initials, save_config};
use crate::wallpaper::{color_sheet, use_wallpaper, wallpaper_pane};
use crate::wifi::{WifiDialog, wifi_dialog, wifi_pane};
use bk::app::use_menu_action;
use malaysia_services::SETTINGS_PANE_ACTION as PANE_ACTION;
use malaysia_services::bluez::{BlueZ, BluetoothCommand};
use malaysia_services::bus::use_service;
use malaysia_services::networkmanager::NetworkManager;
use malaysia_services::upower::{PowerProfiles, UPower};

const SCHEMES: [ColorScheme; 3] = [ColorScheme::Auto, ColorScheme::Light, ColorScheme::Dark];
const WINDOW_COLORS: [WindowColorMode; 2] = [WindowColorMode::Neutral, WindowColorMode::Accent];
const STRENGTHS: [TintStrength; 4] = [
    TintStrength::Subtle,
    TintStrength::Normal,
    TintStrength::Strong,
    TintStrength::Intense,
];
const SCROLL_BARS: [ScrollBars; 3] = [
    ScrollBars::Automatic,
    ScrollBars::WhenScrolling,
    ScrollBars::Always,
];
const SCROLL_CLICKS: [ScrollBarClick; 2] = [ScrollBarClick::NextPage, ScrollBarClick::ClickedSpot];
const MENU_BAR_SIZES: [MenuBarSize; 2] = [MenuBarSize::Default, MenuBarSize::Large];
const COLOR_FILTERS: [ColorFilter; 4] = [
    ColorFilter::Grayscale,
    ColorFilter::RedGreen,
    ColorFilter::GreenRed,
    ColorFilter::BlueYellow,
];
const BLUR_BEHIND: [BlurBehind; 3] = [
    BlurBehind::Mixed,
    BlurBehind::Wallpaper,
    BlurBehind::Windows,
];
pub(crate) const ACCESSIBILITY_TINT: Rgba = ACCENT_SWATCHES[0].1;
const COLOR_FILTER_PENCILS: &[u8] = include_bytes!("../assets/color-filters.svg");
const COLOR_FILTER_PENCILS_SIZE: (f32, f32) = (349.0, 130.0);
type Text = fn() -> &'static str;

const TRAFFIC: [(TrafficLightStyle, Text); 3] = [
    (TrafficLightStyle::Auto, || tr!("lights-auto")),
    (TrafficLightStyle::Standard, || tr!("lights-standard")),
    (TrafficLightStyle::Accent, || tr!("lights-accent")),
];
type Part = fn(&mut TintParts) -> &mut bool;

const PARTS: [(Text, Part); 6] = [
    (|| tr!("tint-part-sidebars"), |p| &mut p.sidebars),
    (|| tr!("tint-part-groups"), |p| &mut p.groups),
    (|| tr!("tint-part-menus"), |p| &mut p.menus),
    (|| tr!("tint-part-window"), |p| &mut p.window),
    (|| tr!("tint-part-controls"), |p| &mut p.controls),
    (|| tr!("tint-part-bars"), |p| &mut p.bars),
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
    let agent = bluetooth.remote.clone();
    use_hook(move || agent.send(BluetoothCommand::HostAgent));
    let pairing = use_pairing_prompt();
    let destination = use_state(|| std::env::args().nth(1));
    use_menu_action(move |action| {
        if let Some(pane) = action.strip_prefix(PANE_ACTION) {
            let mut destination = destination;
            destination.set(Some(pane.to_string()));
        }
    });
    let wifi_shown = use_state(|| None::<WifiDialog>);
    let wallpaper = use_wallpaper();
    let busy_device = use_busy_device(&bluetooth);
    let confirming_own_themes = use_state(|| false);
    let themes_changed = use_state(|| 0u32);
    let themes = use_memo(move || {
        themes_changed.read();
        Themes::read()
    });
    let name = account_name();
    let window = SettingsWindow::new()
        .account(initials(&name), name, tr!("local-account"))
        .sidebar_toggle(false)
        .sidebar_resizable(false)
        .restoration("settings")
        .navigation_destination(destination)
        .group(
            [
                wifi_pane(&network, wifi_shown, error),
                bluetooth_pane(&bluetooth, busy_device, error),
                network_pane(&network),
                vpn_pane(&network, error),
                Some(energy_pane(&power, &profiles, error)),
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
                    accessibility(config),
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
                )
                .page(Pane::new(
                    "freedesktop",
                    tr!("freedesktop"),
                    Symbol::SlidersVertical,
                    ACCESSIBILITY_TINT,
                    freedesktop(
                        config,
                        &themes.read(),
                        confirming_own_themes,
                        themes_changed,
                        error,
                    ),
                )),
            )
            .pane(Pane::new(
                "appearance",
                tr!("appearance"),
                Symbol::CircleHalf,
                SETTINGS.section_icon_bg,
                appearance(config, dominant, resolved, error),
            ))
            .pane(desktop_pane(resolved, error))
            .pane(wallpaper_pane(wallpaper, resolved.scheme.is_dark(), error))
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
        .maybe_child(wifi_dialog(wifi_shown, &network))
        .maybe_child(pairing_sheet(pairing))
        .maybe_child(crop_sheet(wallpaper, error))
        .maybe_child(collection_sheet(wallpaper, error))
        .child(crate::collection::FilePicker { wallpaper })
        .maybe_child(color_sheet(wallpaper, error))
        .maybe_child(confirm(confirming_own_themes, error))
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
    let level = tint_level(config);
    let accent_source = config.window_color == WindowColorMode::Accent;
    let automatic_accent = config.accent_color.is_none();
    let accent = Row::new(tr!("accent-color"))
        .detail(if automatic_accent {
            tr!("accent-automatic-help")
        } else {
            tr!("accent-fixed-help")
        })
        .keywords(tr_list!("accent-color-keywords"))
        .control(
            ColorWell::new(tr!("accent-color"), &ACCENT_SWATCHES)
                .multicolor(tr!("accent-automatic"))
                .selected(accent_index(config))
                .on_change(edit(error, |config, index: usize| {
                    config.accent_color = index
                        .checked_sub(1)
                        .map(|i| ACCENT_SWATCHES[i].1)
                        .map(|color| [color.r, color.g, color.b]);
                })),
        );
    let source_help = match (accent_source, automatic_accent) {
        (true, true) => tr!("tint-source-accent-automatic-help"),
        (true, false) => tr!("tint-source-accent-help"),
        (false, _) if resolved.scheme.is_dark() || config.wallpaper_tinting_in_light => {
            tr!("tint-source-wallpaper-help")
        }
        (false, _) => tr!("tint-source-wallpaper-light-help"),
    };
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
                .row(accent)
                .row(
                    Row::new(tr!("tint-windows"))
                        .detail(if level == 0 {
                            tr!("tint-windows-off-help")
                        } else {
                            tr!("tint-windows-help")
                        })
                        .keywords(tr_list!("tint-windows-keywords"))
                        .control(
                            Slider::stepped(tr!("tint-windows"), STRENGTHS.len() + 1)
                                .value(level)
                                .labels(tr_list!(
                                    "tint-windows-neutral",
                                    "tint-strength-subtle",
                                    "tint-strength-normal",
                                    "tint-strength-strong",
                                    "tint-strength-intense"
                                ))
                                .on_change(edit(error, set_tint_level)),
                        ),
                )
                .row(
                    Row::new(tr!("tint-source"))
                        .detail(source_help)
                        .keywords(tr_list!("tint-source-keywords"))
                        .stacked()
                        .control(
                            RadioGroup::new(tr!("tint-source"))
                                .option(tr!("tint-source-accent"))
                                .option(tr!("tint-source-wallpaper"))
                                .selected(usize::from(!accent_source))
                                .disabled(level == 0)
                                .on_change(edit(error, |config, index: usize| {
                                    config.window_color = WINDOW_COLORS[usize::from(index == 0)];
                                    config.wallpaper_tinting = index == 1;
                                })),
                        ),
                )
                .row(
                    Row::new(tr!("tint-parts"))
                        .detail(tr!("tint-parts-help"))
                        .keywords(tr_list!("tint-parts-keywords"))
                        .stacked()
                        .control(tint_parts(config.tint_parts, level == 0, error)),
                ),
        )
        .section(
            Section::titled(tr!("windows"))
                .row(
                    Row::new(tr!("traffic-lights"))
                        .keywords(tr_list!("traffic-lights-keywords"))
                        .control(traffic_popup(config, resolved, error)),
                )
                .row(traffic_shape_row(config, error))
                .row(
                    Row::new(tr!("scroll-bars"))
                        .keywords(tr_list!("scroll-bars-keywords"))
                        .stacked()
                        .control(
                            RadioGroup::new(tr!("scroll-bars"))
                                .option(tr!("scroll-bars-automatic"))
                                .option(tr!("scroll-bars-when-scrolling"))
                                .option(tr!("scroll-bars-always"))
                                .selected(position(&SCROLL_BARS, config.scroll_bars))
                                .on_change(edit(error, |config, index: usize| {
                                    config.scroll_bars = SCROLL_BARS[index];
                                })),
                        ),
                )
                .row(
                    Row::new(tr!("scroll-bar-click"))
                        .keywords(tr_list!("scroll-bar-click-keywords"))
                        .stacked()
                        .control(
                            RadioGroup::new(tr!("scroll-bar-click"))
                                .option(tr!("scroll-bar-click-next-page"))
                                .option(tr!("scroll-bar-click-spot"))
                                .selected(position(&SCROLL_CLICKS, config.scroll_bar_click))
                                .on_change(edit(error, |config, index: usize| {
                                    config.scroll_bar_click = SCROLL_CLICKS[index];
                                })),
                        ),
                ),
        )
}

fn tint_level(config: &AppearanceConfig) -> usize {
    let tinted = config.window_color == WindowColorMode::Accent || config.wallpaper_tinting;
    if tinted {
        position(&STRENGTHS, config.tint_strength) + 1
    } else {
        0
    }
}

fn set_tint_level(config: &mut AppearanceConfig, level: usize) {
    match level.checked_sub(1) {
        None => {
            config.window_color = WindowColorMode::Neutral;
            config.wallpaper_tinting = false;
        }
        Some(index) => {
            config.tint_strength = STRENGTHS[index];
            if tint_level(config) == 0 {
                config.wallpaper_tinting = true;
            }
        }
    }
}

fn traffic_shape_row(config: &AppearanceConfig, error: State<Option<String>>) -> Row {
    let radius = config.traffic_light_shape.radius();
    let round = TrafficLightShape::ROUND.radius();
    let detail = match radius {
        0 => tr!("traffic-shape-square"),
        r if r >= round => tr!("traffic-shape-round"),
        _ => tr!("traffic-shape-rounded"),
    };
    Row::new(tr!("traffic-shape"))
        .detail(detail)
        .keywords(tr_list!("traffic-shape-keywords"))
        .control(
            HStack::new()
                .cross_align(Alignment::Start)
                .spacing(SPACE_3)
                .child(shape_end(Symbol::Square))
                .child(
                    Slider::stepped(tr!("traffic-shape"), round as usize + 1)
                        .value(radius as usize)
                        .on_change(edit(error, |config, radius: usize| {
                            config.traffic_light_shape = TrafficLightShape::new(radius as u32);
                        })),
                )
                .child(shape_end(Symbol::Circle)),
        )
}

fn shape_end(glyph: Symbol) -> Image {
    Image::system(glyph).color(palette(bk::theme::resolved().scheme).ink_muted)
}

fn accessibility(config: &AppearanceConfig) -> Form {
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
        .section(
            Section::titled(tr!("other-environments")).row(
                Row::page(tr!("freedesktop"), "freedesktop")
                    .symbol(Symbol::SlidersVertical, ACCESSIBILITY_TINT)
                    .detail(if own_themes(config) {
                        tr!("freedesktop-custom")
                    } else {
                        tr!("freedesktop-synced")
                    }),
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
                                .disabled(
                                    !config.wallpaper_tinting
                                        || config.window_color == WindowColorMode::Accent,
                                )
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
        .section(blur(config, error))
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

pub(crate) fn toolkit_text_size(
    config: &AppearanceConfig,
    error: State<Option<String>>,
    toolkit: Toolkit,
    title: &'static str,
    help: &'static str,
) -> Row {
    let mut sizes = config.text_sizes;
    let selected = sizes
        .get_mut(toolkit)
        .map_or(0, |size| 1 + nearest(&TEXT_SIZES, size));
    Row::new(title).detail(help).control(
        text_size_names()
            .iter()
            .fold(PopUpButton::new(title), |popup, name| popup.item(name))
            .selected(selected)
            .on_change(edit(error, move |config, index: usize| {
                *config.text_sizes.get_mut(toolkit) =
                    index.checked_sub(1).map(|step| TEXT_SIZES[step]);
            })),
    )
}

fn text_size(config: &AppearanceConfig, error: State<Option<String>>) -> Form {
    let apps = Section::titled(tr!("text-size-apps")).row(toolkit_text_size(
        config,
        error,
        Toolkit::BarrierKit,
        tr!("text-size-barrierkit"),
        tr!("text-size-barrierkit-help"),
    ));
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

const BLUR_BEHIND_NAMES: [Text; 3] = [
    || tr!("blur-behind-mixed"),
    || tr!("blur-behind-wallpaper"),
    || tr!("blur-behind-windows"),
];

const BLUR_BEHIND_HELP: [Text; 3] = [
    || tr!("blur-behind-mixed-help"),
    || tr!("blur-behind-wallpaper-help"),
    || tr!("blur-behind-windows-help"),
];

fn blur(config: &AppearanceConfig, error: State<Option<String>>) -> Section {
    let reduced = config.transparency_reduced();
    let selected = position(&BLUR_BEHIND, config.blur_behind);
    let help = |text: &'static str| {
        if reduced {
            tr!("blur-reduced-help")
        } else {
            text
        }
    };
    Section::titled(tr!("blur"))
        .row(
            Row::new(tr!("blur-behind"))
                .detail(help(BLUR_BEHIND_HELP[selected]()))
                .control(
                    BLUR_BEHIND_NAMES
                        .iter()
                        .fold(PopUpButton::new(tr!("blur-behind")), |popup, name| {
                            popup.item(name())
                        })
                        .selected(selected)
                        .disabled(reduced)
                        .on_change(edit(error, |config, index: usize| {
                            config.blur_behind = BLUR_BEHIND[index];
                        })),
                ),
        )
        .row(
            Row::new(tr!("blur-all-windows"))
                .detail(help(tr!("blur-all-windows-help")))
                .control(
                    Switch::new(tr!("blur-all-windows"))
                        .on(config.blur_all_windows)
                        .disabled(reduced)
                        .on_toggle(edit(error, |config, on: bool| {
                            config.blur_all_windows = on;
                        })),
                ),
        )
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

fn traffic_popup(
    config: &AppearanceConfig,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> PopUpButton {
    TRAFFIC
        .into_iter()
        .fold(
            PopUpButton::new(tr!("traffic-lights")),
            |popup, (style, label)| {
                let lights = traffic_lights(style, config, resolved);
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
) -> [Rgba; 3] {
    let normal_accent = config
        .accent_color
        .map(|[r, g, b]| Rgba { r, g, b, a: 1.0 })
        .unwrap_or(palette(resolved.scheme).accent);
    match style {
        TrafficLightStyle::Auto => resolved.lights,
        style => lights_for(style, normal_accent),
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

fn accent_index(config: &AppearanceConfig) -> Option<usize> {
    match config.accent_color {
        None => Some(0),
        Some(rgb) => ACCENT_SWATCHES
            .iter()
            .position(|(_, color)| [color.r, color.g, color.b] == rgb)
            .map(|index| index + 1),
    }
}

pub(crate) fn position<T: PartialEq>(options: &[T], value: T) -> usize {
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
        let svg = traffic_row_svg([Rgba::TRANSPARENT; 3], TrafficLightShape::default());
        assert_eq!(svg.matches("<rect").count(), 3);
    }
}
