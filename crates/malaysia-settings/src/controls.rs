//! Controles de Ajustes: muestras de acento, radios y menú de semáforos.

use std::cell::RefCell;
use std::rc::Rc;

use barrier_kit::ProbeExt;
use barrier_kit::menu::{CheckState, MenuItem, MenuView};
use barrier_kit::platform::{
    PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupHandle, PopupSpec, Popups,
};
use freya_components::svg_viewer::SvgViewer;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Size};
use vow_design::{CAPTION, MENU_DARK, MENU_LIGHT, MENU_POPOVER, SETTINGS, TEXT, TITLEBAR};

use crate::state::{ColorScheme, Rgba, TrafficLightStyle, rgba};

/// Un círculo de acento con etiqueta solo cuando está elegido.
pub fn accent_swatch(
    name: &'static str,
    color: Rgba,
    selected: bool,
    disabled: bool,
    probe: &'static str,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> freya_core::prelude::Rect {
    let swatch_size = SETTINGS.swatch_size as f32;
    let ring = 2.0f32;
    let ring_color = vow_design::LIGHT.text.focused;

    let inner = rect()
        .width(Size::px(swatch_size))
        .height(Size::px(swatch_size))
        .corner_radius((swatch_size / 2.0).max(0.0))
        .background(rgba(color))
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(rgba(Rgba::rgba8(0, 0, 0, 0.15))),
        );

    let wrapper = rect()
        .width(Size::auto())
        .height(Size::auto())
        .corner_radius((swatch_size / 2.0).max(0.0))
        .border(
            Border::new()
                .width(if selected { ring } else { 0.0 })
                .alignment(BorderAlignment::Outer)
                .fill(rgba(ring_color)),
        )
        .child(inner);

    let mut column = rect()
        .width(Size::px(SETTINGS.swatch_column as f32))
        .height(Size::auto())
        .direction(Direction::Vertical)
        .spacing(SETTINGS.swatch_name_gap as f32)
        .cross_align(Alignment::Center)
        .probe(probe)
        .child(wrapper);

    column = column.child(
        label()
            .text(if selected { name } else { "" })
            .line_height(CAPTION.line_height as f32 / CAPTION.size as f32)
            .font_size(CAPTION.size as f32)
            .font_weight(FontWeight::from(CAPTION.weight as i32))
            .color(rgba(ring_color))
            .max_lines(1),
    );

    if disabled {
        column.opacity(0.4)
    } else {
        column.on_press(on_press)
    }
}

/// Botón radio con punto blanco cuando está elegido.
pub fn radio(
    label_text: &str,
    selected: bool,
    disabled: bool,
    bg: Rgba,
    accent: Rgba,
    text_color: Rgba,
    probe: &'static str,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> freya_core::prelude::Rect {
    let radio_size = SETTINGS.radio_size as f32;
    let dot_size = SETTINGS.radio_dot as f32;

    let mut dot = rect()
        .width(Size::px(radio_size))
        .height(Size::px(radio_size))
        .corner_radius((radio_size / 2.0).max(0.0))
        .background(rgba(bg))
        .border(
            Border::new()
                .width(0.5)
                .alignment(BorderAlignment::Inner)
                .fill(rgba(Rgba::rgba8(0, 0, 0, 0.25))),
        )
        .center();

    if selected {
        dot = dot.background(rgba(accent)).child(
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
        .probe(probe)
        .child(dot)
        .child(
            label()
                .text(label_text.to_string())
                .line_height(TEXT.line_height as f32 / TEXT.size as f32)
                .font_size(TEXT.size as f32)
                .color(rgba(text_color))
                .max_lines(1),
        );

    if disabled {
        row = row.opacity(0.4);
    } else {
        row = row.on_press(on_press);
    }

    row
}

/// Botón desplegable con los tres semáforos y el estilo actual.
pub fn traffic_dropdown(
    current: TrafficLightStyle,
    lights: [Rgba; 3],
    text_color: Rgba,
    bg_control: Rgba,
    scheme: ColorScheme,
    probe: &'static str,
    on_change: impl FnMut(TrafficLightStyle) + 'static,
) -> freya_core::prelude::Rect {
    let light = SETTINGS.dropdown_preview_light as f32;
    let light_gap = SETTINGS.popup_light_gap as f32;
    let chevron = SETTINGS.popup_chevron as f32;
    let on_change = Rc::new(RefCell::new(on_change));

    rect()
        .width(Size::auto())
        .height(Size::px(SETTINGS.popup_height as f32))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .spacing(SETTINGS.popup_text_gap as f32)
        .probe(probe)
        .child(traffic_squares(lights, light, light_gap))
        .child(
            label()
                .text(traffic_label(current))
                .line_height(TEXT.line_height as f32 / TEXT.size as f32)
                .font_size(TEXT.size as f32)
                .color(rgba(text_color))
                .max_lines(1),
        )
        .child(
            rect()
                .width(Size::px(chevron))
                .height(Size::px(chevron))
                .corner_radius(chevron / 2.0)
                .background(rgba(bg_control))
                .center()
                .child(svg_viewer(
                    CHEVRON_UP_DOWN_ICON,
                    text_color,
                    SETTINGS.dropdown_chevron_glyph,
                )),
        )
        .on_press({
            let on_change = on_change.clone();
            move |event: Event<PressEventData>| {
                open_traffic_menu(current, scheme, event, on_change.clone());
            }
        })
}

fn traffic_squares(lights: [Rgba; 3], size: f32, gap: f32) -> freya_core::prelude::Rect {
    rect()
        .width(Size::auto())
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .spacing(gap)
        .children(lights.iter().map(|color| {
            rect()
                .width(Size::px(size))
                .height(Size::px(size))
                .corner_radius(TITLEBAR.buttons.corner_radius as f32)
                .background(rgba(*color))
                .into_element()
        }))
}

fn traffic_label(style: TrafficLightStyle) -> &'static str {
    match style {
        TrafficLightStyle::Auto => "Automático",
        TrafficLightStyle::Standard => "Clásico",
        TrafficLightStyle::Accent => "Acento",
        TrafficLightStyle::Wallpaper => "Fondo",
    }
}

pub(crate) fn traffic_menu_palette(scheme: ColorScheme) -> vow_design::MenuPalette {
    let base = match scheme {
        ColorScheme::Dark | ColorScheme::DarkTint => MENU_DARK,
        _ => MENU_LIGHT,
    };
    vow_design::MenuPalette {
        bg: MENU_POPOVER.bg,
        radius: MENU_POPOVER.radius,
        padding: MENU_POPOVER.padding,
        min_width: MENU_POPOVER.min_width,
        item_height: MENU_POPOVER.item_height,
        item_radius: MENU_POPOVER.item_radius,
        item_padding_x: MENU_POPOVER.item_padding_start,
        mark_width: MENU_POPOVER.check,
        mark_check: MENU_POPOVER.check,
        separator_margin_y: MENU_POPOVER.separator_margin,
        outer_border: MENU_POPOVER.outer_border,
        inner_line: MENU_POPOVER.inner_line,
        ..base
    }
}

/// Top-left of the popup relative to the dropdown: the checked row's traffic
/// lights land exactly on the button's.
pub(crate) fn traffic_menu_offset() -> (i32, i32) {
    let x = MENU_POPOVER.padding
        + MENU_POPOVER.item_padding_start
        + MENU_POPOVER.check
        + MENU_POPOVER.item_gap;
    (-(x.round() as i32), -(MENU_POPOVER.padding.round() as i32))
}

pub(crate) fn traffic_items(current: TrafficLightStyle) -> Vec<MenuItem> {
    vec![
        traffic_menu_item("auto", "Automático", current == TrafficLightStyle::Auto),
        MenuItem::Separator,
        traffic_menu_item(
            "standard",
            "Clásico",
            current == TrafficLightStyle::Standard,
        ),
        traffic_menu_item("accent", "Acento", current == TrafficLightStyle::Accent),
        traffic_menu_item(
            "wallpaper",
            "Fondo",
            current == TrafficLightStyle::Wallpaper,
        ),
    ]
}

fn open_traffic_menu(
    current: TrafficLightStyle,
    scheme: ColorScheme,
    event: Event<PressEventData>,
    on_change: Rc<RefCell<dyn FnMut(TrafficLightStyle)>>,
) {
    let palette = traffic_menu_palette(scheme);
    let items = traffic_items(current);
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
            on_change.borrow_mut()(style);
            PopupHandle::get().close();
        },
        || {
            PopupHandle::get().close();
        },
    )
    .with_fixed_palette(palette);

    let (global, element) = match event.data() {
        PressEventData::Mouse(mouse) => (mouse.global_location, mouse.element_location),
        PressEventData::Touch(touch) => (touch.global_location, touch.element_location),
        PressEventData::Keyboard(_) => return,
    };
    let origin = global - element;
    let (offset_x, offset_y) = traffic_menu_offset();
    let spec = PopupSpec {
        anchor_rect: (
            origin.x.round() as i32,
            origin.y.round() as i32,
            SETTINGS.popup_height,
            1,
        ),
        anchor: PopupAnchor::TopLeft,
        gravity: PopupGravity::TopLeft,
        offset: (offset_x, offset_y),
        size: (
            MENU_POPOVER.min_width.round() as u32,
            (2.0 * MENU_POPOVER.padding
                + 4.0 * MENU_POPOVER.item_height
                + 1.0
                + 2.0 * MENU_POPOVER.separator_margin)
                .round() as u32,
        ),
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

const CHEVRON_UP_DOWN_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"12\" viewBox=\"0 0 12 12\"><path d=\"M2 8l4-4 4 4M2 4l4 4 4-4\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
