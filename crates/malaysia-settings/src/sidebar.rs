//! Isla lateral de Ajustes: semáforos, buscador, cuenta y sección.

use barrier_kit::controls::TextField;
use barrier_kit::traffic::traffic_lights_view_with_fixed;
use freya_components::svg_viewer::SvgViewer;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Position, Size};
use vow_appearance::traffic::TrafficInput;
use vow_design::{CALLOUT, HEADLINE, SETTINGS, SIDEBAR, TITLEBAR};

use crate::state::{
    Resolved, Rgba, account_name, bg_control, initials, load_appearance, palette, rgba,
};

const SEARCH_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"6.8\" cy=\"6.8\" r=\"4.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\"/><path d=\"M10 10l3.2 3.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\"/></svg>";
const CLEAR_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M4 4l8 8M12 4l-8 8\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\"/></svg>";
const APPEARANCE_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"22\" height=\"22\" viewBox=\"0 0 22 22\"><circle cx=\"11\" cy=\"11\" r=\"9\" fill=\"currentColor\"/><path d=\"M11 2v18\" stroke=\"white\" stroke-width=\"2\"/><path d=\"M2 11h18\" stroke=\"white\" stroke-width=\"2\"/></svg>";

/// Isla lateral completa.
pub fn sidebar(search: State<String>, resolved: &Resolved) -> freya_core::prelude::Rect {
    let sidebar_palette = resolved.sidebar.palette;
    let island_bg = resolved.sidebar.bg;
    let rim = resolved.sidebar.rim;
    let shadow = resolved.sidebar.shadow;

    let mut island = rect()
        .width(Size::fill())
        .height(Size::fill())
        .corner_radius(SIDEBAR.radius as f32)
        .background(rgba(island_bg))
        .direction(Direction::Vertical)
        .padding(Gaps::new(
            (SIDEBAR.traffic_y + TITLEBAR.buttons.diameter + SETTINGS.sidebar_lights_gap) as f32,
            SIDEBAR.padding_x as f32,
            SIDEBAR.padding_x as f32,
            SIDEBAR.padding_x as f32,
        ))
        .child(search_field(
            search,
            sidebar_palette,
            bg_control(resolved.scheme),
        ))
        .child(account_row(sidebar_palette))
        .child(sidebar_item("Apariencia", true, sidebar_palette));

    if rim.a > 0.0 {
        island = island.border(
            Border::new()
                .width(SIDEBAR.rim_width)
                .alignment(BorderAlignment::Inner)
                .fill(rgba(rim)),
        );
    }

    if shadow[0].opacity > 0.0 {
        island = island.shadow(
            Shadow::new()
                .x(0.0)
                .y(shadow[0].y_offset as f32)
                .blur(shadow[0].blur as f32)
                .color(rgba(shadow[0].color)),
        );
    }

    rect()
        .width(Size::px((SIDEBAR.width + 2 * SIDEBAR.inset) as f32))
        .height(Size::fill())
        .padding(Gaps::new(
            SIDEBAR.inset as f32,
            SIDEBAR.inset as f32,
            SIDEBAR.inset as f32,
            SIDEBAR.inset as f32,
        ))
        .child(island)
        .child(
            rect()
                .position(
                    Position::new_global()
                        .top((SIDEBAR.inset + SIDEBAR.traffic_y) as f32)
                        .left((SIDEBAR.inset + SIDEBAR.traffic_x) as f32),
                )
                .width(Size::px(
                    (TITLEBAR.buttons.diameter * 3 + TITLEBAR.buttons.spacing * 2) as f32,
                ))
                .height(Size::px(TITLEBAR.buttons.diameter as f32))
                .a11y_alt("sidebar-traffic-lights")
                .child(traffic_lights_view_with_fixed(
                    TrafficInput {
                        focused: true,
                        group_hovered: false,
                        pressed: None,
                        zoom_supported: true,
                    },
                    resolved.lights,
                    resolved.traffic_style,
                    palette(resolved.scheme).traffic_lights_inactive,
                    true,
                )),
        )
}

/// Campo de búsqueda con lupa, texto y borrón.
fn search_field(
    search: State<String>,
    palette: vow_design::SidebarPalette,
    search_bg: Rgba,
) -> Rect {
    let mut search_mut = search;
    let placeholder_color = palette.count_ink;
    let text_color = palette.item_ink;

    rect()
        .width(Size::px((SIDEBAR.width - 2 * SIDEBAR.padding_x) as f32))
        .height(Size::px(SETTINGS.search_height as f32))
        .corner_radius(999.0)
        .background(rgba(search_bg))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .padding(Gaps::new(
            0.0,
            SETTINGS.search_padding_x as f32,
            0.0,
            SETTINGS.search_padding_x as f32,
        ))
        .spacing(6.0)
        .a11y_alt("Buscar")
        .child(svg_viewer(
            SEARCH_ICON,
            placeholder_color,
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
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .center()
                .maybe(!search.read().is_empty(), |el| {
                    el.child(svg_viewer(CLEAR_ICON, text_color, 14))
                        .on_press(move |_| search_mut.set(String::new()))
                }),
        )
}

/// Fila de cuenta con avatar e iniciales.
fn account_row(sidebar_palette: vow_design::SidebarPalette) -> Rect {
    let name = account_name();
    let scheme_palette = palette(load_appearance().color_scheme);

    rect()
        .width(Size::fill())
        .height(Size::px(48.0))
        .margin(Gaps::new(SETTINGS.sidebar_search_gap as f32, 0.0, 0.0, 0.0))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .spacing(SETTINGS.account_gap as f32)
        .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
        .child(
            rect()
                .width(Size::px(SETTINGS.account_avatar as f32))
                .height(Size::px(SETTINGS.account_avatar as f32))
                .corner_radius((SETTINGS.account_avatar as f32 / 2.0).max(0.0))
                // sys-gray de Neptunia; el token no se exporta como const todavía.
                .background(rgba(Rgba::rgb8(0x7d, 0x82, 0x8c)))
                .center()
                .child(
                    label()
                        .text(initials(&name))
                        .font_size(13.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(rgba(scheme_palette.on_accent))
                        .max_lines(1),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::auto())
                .direction(Direction::Vertical)
                .child(
                    label()
                        .text(name)
                        .font_size(HEADLINE.size as f32)
                        .font_weight(FontWeight::from(HEADLINE.weight as i32))
                        .color(rgba(sidebar_palette.item_ink))
                        .max_lines(1),
                )
                .child(
                    label()
                        .text("Cuenta local")
                        .font_size(CALLOUT.size as f32)
                        .font_weight(FontWeight::from(CALLOUT.weight as i32))
                        .color(rgba(sidebar_palette.count_ink))
                        .max_lines(1),
                ),
        )
}

/// Elemento de la barra lateral.
fn sidebar_item(label_text: &str, selected: bool, palette: vow_design::SidebarPalette) -> Rect {
    let (bg, fg) = if selected {
        (palette.selected_bg, palette.selected_ink)
    } else {
        (Rgba::rgba8(0, 0, 0, 0.0), palette.item_ink)
    };

    rect()
        .width(Size::fill())
        .height(Size::px(30.0))
        .margin(Gaps::new(SETTINGS.section_gap as f32, 0.0, 0.0, 0.0))
        .corner_radius(8.0)
        .background(rgba(bg))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .spacing(8.0)
        .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
        .child(
            rect()
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(6.0)
                .background(rgba(SETTINGS.section_icon_bg))
                .center()
                .child(svg_viewer(APPEARANCE_ICON, Rgba::rgb8(255, 255, 255), 22)),
        )
        .child(
            label()
                .text(label_text.to_string())
                .font_size(13.0)
                .font_weight(FontWeight::MEDIUM)
                .color(rgba(fg))
                .max_lines(1),
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
