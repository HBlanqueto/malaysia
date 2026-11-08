use bk::choices::{ChoiceCards, ColorWell, RadioGroup};
use bk::menu::MenuIcon;
use bk::patterns::language::system_language_row;
use bk::patterns::settings::{Pane, PaneGroup, SettingsWindow};
use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::views::feedback::Alert;
use bk::views::form::{Form, Row, Section};
use ctappearance::{
    AppearanceConfig, ColorScheme, Resolved, ScrollBars, TintParts, TintStrength,
    TrafficLightShape, TrafficLightStyle, WindowColorMode, WindowTintScope, lights_for,
    load_appearance, palette,
};
use ctdesign::{ACCENT_SWATCHES, MENU_POPOVER, Rgba, SETTINGS};

use crate::localize::{tr, tr_list};
use crate::previews::{theme_preview, traffic_shape_preview, window_color_preview};
use crate::state::{account_name, initials, save_config};

const SCHEMES: [ColorScheme; 3] = [ColorScheme::Auto, ColorScheme::Light, ColorScheme::Dark];
const WINDOW_COLORS: [WindowColorMode; 3] = [
    WindowColorMode::Neutral,
    WindowColorMode::Accent,
    WindowColorMode::Wallpaper,
];
const STRENGTHS: [TintStrength; 4] = [
    TintStrength::Subtle,
    TintStrength::Normal,
    TintStrength::Strong,
    TintStrength::Intense,
];
const SCOPES: [WindowTintScope; 2] = [WindowTintScope::Windows, WindowTintScope::Desktop];
const SCROLL_BARS: [ScrollBars; 2] = [ScrollBars::WhenScrolling, ScrollBars::Always];
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
    let name = account_name();
    let window = SettingsWindow::new()
        .account(initials(&name), name, tr!("local-account"))
        .sidebar_toggle(false)
        .sidebar_resizable(false);
    let window = window.group(PaneGroup::new(None).pane(Pane::new(
        "appearance",
        tr!("appearance"),
        Symbol::CircleHalf,
        SETTINGS.section_icon_bg,
        appearance(config, dominant, resolved, error),
    )));
    let message = error.read().clone();
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .child(window)
        .maybe_child(message.map(|message| {
            Alert::new(tr!("save-error-title"), message)
                .confirm(tr!("save-error-ok"), move |_| error.set(None))
                .cancel(tr!("save-error-close"), move |_| error.set(None))
        }))
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
                                .disabled(config.window_color == WindowColorMode::Wallpaper)
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
                        .detail(tr!("tint-strength-help"))
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
                                    let strength = STRENGTHS[index];
                                    match config.window_color {
                                        WindowColorMode::Accent => {
                                            config.tint_strength_accent = strength
                                        }
                                        WindowColorMode::Wallpaper => {
                                            config.tint_strength_wallpaper = strength
                                        }
                                        WindowColorMode::Neutral => {}
                                    }
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
                ),
        )
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
    [
        tr!("window-color-neutral"),
        tr!("window-color-accent"),
        tr!("window-color-wallpaper"),
    ]
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

fn tint_parts(parts: TintParts, disabled: bool, error: State<Option<String>>) -> Rect {
    rect()
        .direction(Direction::Vertical)
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
        WindowColorMode::Wallpaper => tr!("window-color-wallpaper-help"),
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
        WindowColorMode::Wallpaper => config.tint_strength_wallpaper,
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
