use barrier_kit::choices::{ChoiceCards, ColorWell, RadioGroup};
use barrier_kit::controls::{Checkbox, Slider};
use barrier_kit::menu::MenuIcon;
use barrier_kit::patterns::language::system_language_row;
use barrier_kit::patterns::preferences::{Pane, PaneGroup, PreferencesWindow};
use barrier_kit::popup_button::PopUpButton;
use barrier_kit::symbol::Symbol;
use barrier_kit::views::feedback::Alert;
use barrier_kit::views::form::{Form, Row, Section};
use freya_core::prelude::*;
use torin::prelude::{Direction, Size};
use vow_appearance::{
    AppearanceConfig, ColorScheme, Resolved, ScrollBars, TintParts, TintStrength,
    TrafficLightShape, TrafficLightStyle, WindowColorMode, WindowTintScope, lights_for,
    load_appearance, palette,
};
use vow_design::{ACCENT_SWATCHES, MENU_POPOVER, Rgba, SETTINGS};

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
const SHAPES: [(TrafficLightShape, &str); 2] = [
    (TrafficLightShape::Square, "Cuadrados"),
    (TrafficLightShape::Round, "Redondos"),
];
const TRAFFIC: [(TrafficLightStyle, &str); 4] = [
    (TrafficLightStyle::Auto, "Automático"),
    (TrafficLightStyle::Standard, "Clásico"),
    (TrafficLightStyle::Accent, "Acento"),
    (TrafficLightStyle::Wallpaper, "Fondo"),
];
type Part = fn(&mut TintParts) -> &mut bool;

const PARTS: [(&str, Part); 5] = [
    ("Barras laterales", |p| &mut p.sidebars),
    ("Grupos y tarjetas", |p| &mut p.groups),
    ("Menús y diálogos", |p| &mut p.menus),
    ("Fondo de la ventana", |p| &mut p.window),
    ("Controles", |p| &mut p.controls),
];

pub fn settings_window(
    config: &AppearanceConfig,
    dominant: Option<Rgba>,
    resolved: &Resolved,
) -> Rect {
    let mut error = use_state(|| None::<String>);
    let name = account_name();
    let window = PreferencesWindow::new()
        .account(initials(&name), name, "Cuenta local")
        .sidebar_toggle(false)
        .sidebar_resizable(false);
    let window = window.group(PaneGroup::new(None).pane(Pane::new(
        "appearance",
        "Apariencia",
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
            Alert::new("No se pudo guardar la apariencia", message)
                .confirm("Aceptar", move |_| error.set(None))
                .cancel("Cerrar", move |_| error.set(None))
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
                    Row::new("Apariencia")
                        .keywords(&["tema", "claro", "oscuro", "automático"])
                        .control(scheme_cards(config, dominant, error)),
                )
                .row(system_language_row()),
        )
        .section(
            Section::titled("Color")
                .row(
                    Row::new("Color de acento")
                        .keywords(&[
                            "azul", "morado", "rosa", "rojo", "naranja", "amarillo", "verde",
                            "gris",
                        ])
                        .control(
                            ColorWell::new("Color de acento", &ACCENT_SWATCHES)
                                .selected(accent_index(config))
                                .disabled(config.window_color == WindowColorMode::Wallpaper)
                                .on_change(edit(error, |config, index: usize| {
                                    let color = ACCENT_SWATCHES[index].1;
                                    config.accent_color = Some([color.r, color.g, color.b]);
                                })),
                        ),
                )
                .row(
                    Row::new("Color de las ventanas")
                        .detail(window_color_help(config.window_color))
                        .keywords(&["tinte", "neutro", "fondo de pantalla"])
                        .control(window_color_cards(config, dominant, error)),
                )
                .row(
                    Row::new("Intensidad del tinte")
                        .detail("Acento y Fondo guardan cada uno la suya.")
                        .keywords(&["sutil", "normal", "fuerte", "intenso"])
                        .control(
                            Slider::stepped(STRENGTHS.len())
                                .value(strength_index(config))
                                .labels(&["Sutil", "Normal", "Fuerte", "Intenso"])
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
                    Row::new("Teñir en las ventanas")
                        .detail("Lo que no marques queda en gris neutro.")
                        .keywords(&["barras laterales", "tarjetas", "controles", "menús"])
                        .control(tint_parts(config.tint_parts, tint_disabled, error)),
                )
                .row(
                    Row::new("Aplicar el tinte a")
                        .keywords(&["escritorio", "barra de menús", "dock"])
                        .control(
                            RadioGroup::new("Aplicar el tinte a")
                                .option("Solo las ventanas")
                                .option("Todo el escritorio")
                                .selected(position(&SCOPES, config.window_tint_scope))
                                .disabled(tint_disabled)
                                .on_change(edit(error, |config, index: usize| {
                                    config.window_tint_scope = SCOPES[index];
                                })),
                        ),
                ),
        )
        .section(
            Section::titled("Ventanas")
                .row(
                    Row::new("Semáforos")
                        .keywords(&["botones", "cerrar", "minimizar", "ampliar"])
                        .control(traffic_popup(config, dominant, resolved, error)),
                )
                .row(
                    Row::new("Forma de los semáforos")
                        .keywords(&["cuadrados", "redondos", "botones", "forma"])
                        .control(traffic_shape_cards(config, resolved, error)),
                )
                .row(
                    Row::new("Mostrar barras de desplazamiento")
                        .keywords(&["scroll", "desplazamiento", "barras"])
                        .control(
                            RadioGroup::new("Mostrar barras de desplazamiento")
                                .option("Al desplazarse")
                                .option("Siempre")
                                .selected(position(&SCROLL_BARS, config.scroll_bars))
                                .on_change(edit(error, |config, index: usize| {
                                    config.scroll_bars = SCROLL_BARS[index];
                                })),
                        ),
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
    ["Automático", "Claro", "Oscuro"]
        .into_iter()
        .zip(SCHEMES)
        .fold(
            ChoiceCards::new("Apariencia"),
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
    ["Neutro", "Acento", "Fondo"]
        .into_iter()
        .zip(WINDOW_COLORS)
        .fold(
            ChoiceCards::new("Color de las ventanas"),
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
            Checkbox::new(label)
                .checked(*part(&mut current))
                .disabled(disabled)
                .on_toggle(edit(error, move |config, checked: bool| {
                    *part(&mut config.tint_parts) = checked;
                }))
                .into_element()
        }))
}

fn traffic_shape_cards(
    config: &AppearanceConfig,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> ChoiceCards {
    SHAPES
        .iter()
        .fold(
            ChoiceCards::new("Forma de los semáforos"),
            |cards, &(shape, caption)| {
                cards.option(caption, traffic_shape_preview(resolved, shape))
            },
        )
        .selected(
            SHAPES
                .iter()
                .position(|(shape, _)| *shape == config.traffic_light_shape)
                .unwrap_or(0),
        )
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
        .fold(PopUpButton::new("Semáforos"), |popup, (style, label)| {
            let lights = traffic_lights(style, config, resolved, dominant);
            let svg = traffic_row_svg(lights, config.traffic_light_shape);
            popup.icon_item(label, MenuIcon::Svg(svg.into()))
        })
        .selected(
            TRAFFIC
                .iter()
                .position(|(style, _)| *style == config.traffic_lights)
                .unwrap_or(0),
        )
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
        WindowColorMode::Neutral => "Grises sin tinte.",
        WindowColorMode::Accent => "Toman un poco del color de acento.",
        WindowColorMode::Wallpaper => "Toman el color del fondo de pantalla.",
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
