use bk::app::use_defaults;
use bk::choices::ChoiceCards;
use bk::patterns::settings::Pane;
use bk::prelude::*;
use bk::views::form::{Form, Row, Section};
use ctappearance::Resolved;
use ctappearance::menubar::{MenubarConfig, MenubarHeight, MenubarStyle};
use ctdesign::SETTINGS;

use crate::app::position;
use crate::localize::{tr, tr_list};
use crate::menubar::{DOMAIN, edit};
use crate::previews::menubar_preview;

const STYLES: [MenubarStyle; 3] = [
    MenubarStyle::Solid,
    MenubarStyle::Translucent,
    MenubarStyle::Floating,
];
const HEIGHTS: [MenubarHeight; 2] = [MenubarHeight::Slim, MenubarHeight::Wide];

pub fn desktop_pane(resolved: &Resolved, error: State<Option<String>>) -> Pane {
    let config = use_defaults::<MenubarConfig>(DOMAIN).read().clone();
    let styles = [
        tr!("menu-bar-style-solid"),
        tr!("menu-bar-style-translucent"),
        tr!("menu-bar-style-floating"),
    ]
    .into_iter()
    .zip(STYLES)
    .fold(
        ChoiceCards::new(tr!("menu-bar-style")),
        |cards, (caption, style)| {
            cards.option(
                caption,
                menubar_preview(style, config.bar_height, false, resolved),
            )
        },
    )
    .selected(position(&STYLES, config.style))
    .on_change(edit(error, |config, index: usize| {
        config.style = STYLES[index];
    }));
    let heights = [tr!("menu-bar-height-slim"), tr!("menu-bar-height-wide")]
        .into_iter()
        .zip(HEIGHTS)
        .fold(
            ChoiceCards::new(tr!("menu-bar-height")),
            |cards, (caption, height)| {
                cards.option(
                    caption,
                    menubar_preview(config.style, height, true, resolved),
                )
            },
        )
        .selected(position(&HEIGHTS, config.bar_height))
        .on_change(edit(error, |config, index: usize| {
            config.bar_height = HEIGHTS[index];
        }));
    let style_help = match config.style {
        MenubarStyle::Solid => tr!("menu-bar-style-solid-help"),
        MenubarStyle::Translucent => tr!("menu-bar-style-translucent-help"),
        MenubarStyle::Floating => tr!("menu-bar-style-floating-help"),
    };
    Pane::new(
        "desktop-dock",
        tr!("desktop-dock"),
        Symbol::MenubarDockRectangle,
        SETTINGS.section_icon_bg,
        Form::new().section(
            Section::titled(tr!("menu-bar"))
                .row(
                    Row::new(tr!("menu-bar-style"))
                        .keywords(tr_list!("menu-bar-style-keywords"))
                        .detail(style_help)
                        .control(styles),
                )
                .row(
                    Row::new(tr!("menu-bar-height"))
                        .detail(tr!("menu-bar-height-help"))
                        .control(heights),
                ),
        ),
    )
}
