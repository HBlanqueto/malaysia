use std::sync::OnceLock;

use bk::choices::{ChoiceCards, RadioGroup, swatch_name};
use bk::color_picker::ColorPanel;
use bk::menu::MenuIcon;
use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::styles::{Axis, ChoiceCardsStyle, CornerStyle, RadioGroupStyle, Shape};
use bk::views::form::Row;
use ctappearance::icon_style::{IconStyle, IconTone};
use ctappearance::{AppearanceConfig, Resolved};
use ctdesign::{ACCENT_SWATCHES, ICON_PLATE, MENU_POPOVER, Rgba, SETTINGS, SPACE_3};
use torin::prelude::{Area, Position};

use crate::app::{edit, position};
use crate::localize::{tr, tr_list};
use crate::previews::wallpaper_background;
use crate::wallpaper::over_artwork;

const STYLES: [IconStyle; 4] = [
    IconStyle::Default,
    IconStyle::Dark,
    IconStyle::Clear,
    IconStyle::Tinted,
];
const DARK_TONES: [IconTone; 2] = [IconTone::Dark, IconTone::Automatic];
const TONES: [IconTone; 3] = [IconTone::Light, IconTone::Dark, IconTone::Automatic];
// macOS lists the folder colors in this order.
const FOLDER_COLORS: [&str; 8] = [
    "red", "orange", "yellow", "green", "blue", "purple", "pink", "graphite",
];
const SETTINGS_ICON: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../share/icons/hicolor/scalable/apps/io.github.hblanqueto.malaysia.settings.svg"
));

pub fn icon_style_row(
    config: &AppearanceConfig,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Row {
    let cards = STYLES
        .into_iter()
        .zip([
            tr!("icon-style-default"),
            tr!("icon-style-dark"),
            tr!("icon-style-clear"),
            tr!("icon-style-tinted"),
        ])
        .fold(
            ChoiceCards::new(tr!("icon-style")).style(|style| ChoiceCardsStyle {
                card_size: (
                    SETTINGS.icon_card_size as f32,
                    SETTINGS.icon_card_size as f32,
                ),
                card_shape: Shape::RoundedRectangle {
                    radius: SETTINGS.icon_card_radius as f32,
                    style: CornerStyle::Circular,
                },
                selection_gap: SETTINGS.icon_card_ring_gap as f32,
                card_border: None,
                ..style
            }),
            |cards, (style, caption)| cards.option(caption, icon_card(style, config, resolved)),
        )
        .selected(position(&STYLES, config.icon_style))
        .on_change(edit(error, |config, index: usize| {
            config.icon_style = STYLES[index];
        }));
    Row::new(tr!("icon-style"))
        .keywords(tr_list!("icon-style-keywords"))
        .control(
            VStack::new()
                .main_align(Alignment::Start)
                .cross_align(Alignment::End)
                .spacing(SPACE_3)
                .child(cards)
                .maybe_child(tone_radios(config, error)),
        )
}

fn tone_radios(config: &AppearanceConfig, error: State<Option<String>>) -> Option<RadioGroup> {
    let tones: &'static [IconTone] = match config.icon_style {
        IconStyle::Default => return None,
        IconStyle::Dark => &DARK_TONES,
        IconStyle::Clear | IconStyle::Tinted => &TONES,
    };
    Some(
        tones
            .iter()
            .fold(
                RadioGroup::new(tr!("icon-tone")).style(|style| RadioGroupStyle {
                    axis: Axis::Horizontal,
                    ..style
                }),
                |radios, tone| radios.option(tone_name(config.icon_style, *tone)),
            )
            .selected(position(tones, config.icon_tone))
            .on_change(edit(error, move |config, index: usize| {
                config.icon_tone = tones[index];
            })),
    )
}

fn tone_name(style: IconStyle, tone: IconTone) -> &'static str {
    match (style, tone) {
        (IconStyle::Dark, IconTone::Dark) => tr!("icon-tone-always"),
        (_, IconTone::Light) => tr!("icon-tone-light"),
        (_, IconTone::Dark) => tr!("icon-tone-dark"),
        (_, IconTone::Automatic) => tr!("icon-tone-automatic"),
    }
}

// The shell draws the dark twin for a dark or clear icon; the Dark card always shows its dark look.
fn icon_card(style: IconStyle, config: &AppearanceConfig, resolved: &Resolved) -> Rect {
    let tone = if style == config.icon_style {
        config.icon_tone
    } else {
        IconTone::Automatic
    };
    let dark = matches!(style, IconStyle::Dark | IconStyle::Clear)
        || style.dark(tone, resolved.scheme.is_dark());
    let art = Image::data(if dark { dark_twin() } else { SETTINGS_ICON })
        .icon_style(style)
        .size(SETTINGS.icon_card_size as f32 / ICON_PLATE.body);
    let icon = over_artwork(Position::new_absolute())
        .width(Size::percent(100.0))
        .height(Size::percent(100.0))
        .center()
        .child(art);
    if style == IconStyle::Clear {
        wallpaper_background(resolved.scheme.is_dark()).child(icon)
    } else {
        icon
    }
}

fn dark_twin() -> &'static [u8] {
    static TWIN: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    TWIN.get_or_init(|| {
        ctappearance::xdg::data_dirs().into_iter().find_map(|dir| {
            std::fs::read(dir.join(
                "icons/Malaysia-Dark/scalable/apps/io.github.hblanqueto.malaysia.settings.svg",
            ))
            .ok()
        })
    })
    .as_deref()
    .unwrap_or(SETTINGS_ICON)
}

pub fn icon_color_row(
    config: &AppearanceConfig,
    resolved: &Resolved,
    error: State<Option<String>>,
) -> Row {
    let title = if config.icon_style == IconStyle::Tinted {
        tr!("icon-color-tinted")
    } else {
        tr!("icon-color-folders")
    };
    Row::new(title)
        .keywords(tr_list!("icon-color-keywords"))
        .control(IconColor {
            title,
            color: config.icon_color,
            accent: resolved.accent,
            error,
        })
}

#[derive(PartialEq)]
struct IconColor {
    title: &'static str,
    color: Option<[f32; 3]>,
    accent: Rgba,
    error: State<Option<String>>,
}

impl Component for IconColor {
    fn render(&self) -> impl IntoElement {
        let mut area = use_state(Area::default);
        let swatches = FOLDER_COLORS.map(|name| {
            ACCENT_SWATCHES
                .iter()
                .find(|(id, _)| *id == name)
                .map(|(_, color)| *color)
                .expect("ctdesign has every folder color")
        });
        let current = self.color.map(|[r, g, b]| Rgba { r, g, b, a: 1.0 });
        let index = current.map(|color| {
            swatches
                .iter()
                .position(|swatch| swatch.to_rgba8() == color.to_rgba8())
        });
        let custom = current.filter(|_| index == Some(None));
        let selected = match index {
            None => 0,
            Some(Some(index)) => index + 1,
            Some(None) => swatches.len() + 1,
        };
        let popup = FOLDER_COLORS.iter().zip(swatches).fold(
            PopUpButton::new(self.title)
                .icon_item(tr!("icon-color-automatic"), dot(self.accent))
                .separator(),
            |popup, (name, color)| popup.icon_item(swatch_name(name), dot(color)),
        );
        let popup = match custom {
            Some(color) => popup
                .separator()
                .icon_item(tr!("icon-color-custom"), dot(color)),
            None => popup.separator().item(tr!("icon-color-custom")),
        };
        let error = self.error;
        let shown = current.unwrap_or(self.accent);
        rect()
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .child(popup.selected(selected).on_change(move |index: usize| {
                let pick: Callback<Option<[f32; 3]>, ()> =
                    edit(error, |config, color| config.icon_color = color);
                match index {
                    0 => pick.call(None),
                    index if index <= swatches.len() => {
                        let Rgba { r, g, b, .. } = swatches[index - 1];
                        pick.call(Some([r, g, b]));
                    }
                    _ => ColorPanel::show(
                        shown,
                        *area.peek(),
                        edit(error, |config, Rgba { r, g, b, .. }| {
                            config.icon_color = Some([r, g, b]);
                        }),
                    ),
                }
            }))
    }
}

fn dot(color: Rgba) -> MenuIcon {
    let size = MENU_POPOVER.swatch_grid_size;
    let radius = MENU_POPOVER.check / 2.0;
    let [r, g, b, _] = color.to_rgba8();
    MenuIcon::Svg(
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}"><circle cx="{c}" cy="{c}" r="{radius}" fill="rgb({r},{g},{b})"/></svg>"#,
            c = size / 2.0
        )
        .into(),
    )
}
