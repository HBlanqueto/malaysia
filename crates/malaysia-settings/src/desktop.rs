use bk::app::use_defaults;
use bk::choices::ChoiceCards;
use bk::patterns::settings::Pane;
use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::views::form::{Form, Row, Section};
use ctappearance::dock::{DockConfig, DockOrientation, MinimizeEffect};
use ctappearance::menubar::{MenubarConfig, MenubarHeight, MenubarStyle};
use ctappearance::window::{DoubleClickAction, read_window_behavior, write_double_click_titlebar};
use ctappearance::{AppearanceConfig, Resolved};
use ctdesign::{DOCK_TILESIZE_MAX, DOCK_TILESIZE_MIN, SETTINGS};
use torin::prelude::Gaps;

use crate::app::edit as edit_appearance;
use crate::app::position;
use crate::dock::edit as edit_dock;
use crate::localize::{tr, tr_list};
use crate::menubar::edit;
use crate::previews::menubar_preview;

const STYLES: [MenubarStyle; 3] = [
    MenubarStyle::Solid,
    MenubarStyle::Translucent,
    MenubarStyle::Floating,
];
const HEIGHTS: [MenubarHeight; 2] = [MenubarHeight::Slim, MenubarHeight::Wide];
const ORIENTATIONS: [DockOrientation; 3] = [
    DockOrientation::Left,
    DockOrientation::Bottom,
    DockOrientation::Right,
];
const EFFECTS: [MinimizeEffect; 2] = [MinimizeEffect::Genie, MinimizeEffect::Scale];
const DOUBLE_CLICKS: [DoubleClickAction; 4] = [
    DoubleClickAction::Fill,
    DoubleClickAction::Zoom,
    DoubleClickAction::Minimize,
    DoubleClickAction::None,
];
const SIZE_STEP: i32 = 8;
const SIZE_STEPS: usize = ((DOCK_TILESIZE_MAX - DOCK_TILESIZE_MIN) / SIZE_STEP + 1) as usize;
const DELAY_MAX_SECONDS: f32 = 1.0;
const DELAY_STEPS: usize = 11;
const SPEED_MAX: f32 = 2.0;
const SPEED_STEPS: usize = 9;

fn size_index(size: i32) -> usize {
    let offset = size.clamp(DOCK_TILESIZE_MIN, DOCK_TILESIZE_MAX) - DOCK_TILESIZE_MIN;
    ((offset + SIZE_STEP / 2) / SIZE_STEP) as usize
}

fn size_at(index: usize) -> i32 {
    DOCK_TILESIZE_MIN + index as i32 * SIZE_STEP
}

fn switch_row(
    title: &'static str,
    on: bool,
    error: State<Option<String>>,
    set: fn(&mut DockConfig, bool),
) -> Row {
    Row::toggle(title, on, edit_dock(error, set))
}

fn dock_section(config: &DockConfig, error: State<Option<String>>) -> Section {
    let magnification = if config.magnification {
        size_index(config.largesize) + 1
    } else {
        0
    };
    let double_click = read_window_behavior().double_click_titlebar;
    Section::titled(tr!("dock"))
        .illustration(
            HStack::new()
                .width(Size::fill())
                .padding(Gaps::new(SPACE_3, SPACE_3, SPACE_2, SPACE_3))
                .main_align(Alignment::SpaceBetween)
                .child(
                    VStack::new()
                        .spacing(SPACE_2)
                        .child(Text::new(tr!("dock-size")).font(CALLOUT))
                        .child(
                            Slider::stepped(tr!("dock-size"), SIZE_STEPS)
                                .value(size_index(config.tilesize) as f32)
                                .labels(tr_list!("dock-small", "dock-large"))
                                .on_change(edit_dock(
                                    error,
                                    |config: &mut DockConfig, step: f32| {
                                        config.tilesize = size_at(step as usize);
                                    },
                                )),
                        ),
                )
                .child(
                    VStack::new()
                        .spacing(SPACE_2)
                        .child(Text::new(tr!("dock-magnification")).font(CALLOUT))
                        .child(
                            Slider::stepped(tr!("dock-magnification"), SIZE_STEPS + 1)
                                .value(magnification as f32)
                                .labels(tr_list!("dock-magnification-off", "dock-large"))
                                .on_change(edit_dock(
                                    error,
                                    |config: &mut DockConfig, value: f32| {
                                        let index = value.round() as usize;
                                        config.magnification = index > 0;
                                        if index > 0 {
                                            config.largesize = size_at(index - 1);
                                        }
                                    },
                                )),
                        ),
                ),
        )
        .row(
            Row::new(tr!("dock-position"))
                .keywords(tr_list!("dock-keywords"))
                .control(
                    PopUpButton::new(tr!("dock-position"))
                        .item(tr!("dock-left"))
                        .item(tr!("dock-bottom"))
                        .item(tr!("dock-right"))
                        .selected(position(&ORIENTATIONS, config.orientation))
                        .on_change(edit_dock(error, |config: &mut DockConfig, index: usize| {
                            config.orientation = ORIENTATIONS[index];
                        })),
                ),
        )
        .row(
            Row::new(tr!("dock-minimize-effect")).control(
                PopUpButton::new(tr!("dock-minimize-effect"))
                    .item(tr!("dock-genie"))
                    .item(tr!("dock-scale"))
                    .selected(position(&EFFECTS, config.mineffect))
                    .on_change(edit_dock(error, |config: &mut DockConfig, index: usize| {
                        config.mineffect = EFFECTS[index];
                    })),
            ),
        )
        .row(
            Row::new(tr!("dock-double-click")).control(
                PopUpButton::new(tr!("dock-double-click"))
                    .item(tr!("dock-double-click-fill"))
                    .item(tr!("dock-double-click-zoom"))
                    .item(tr!("dock-double-click-minimize"))
                    .item(tr!("dock-double-click-none"))
                    .selected(position(&DOUBLE_CLICKS, double_click))
                    .on_change(Callback::new(move |index: usize| {
                        let path = ctappearance::config_dir().join("window.domain");
                        if let Err(message) =
                            write_double_click_titlebar(&path, DOUBLE_CLICKS[index])
                        {
                            let mut error = error;
                            error.set(Some(message.to_string()));
                        }
                    })),
            ),
        )
        .row(switch_row(
            tr!("dock-minimize-into-app"),
            config.minimize_to_application,
            error,
            |config, on| config.minimize_to_application = on,
        ))
        .row(switch_row(
            tr!("dock-autohide"),
            config.autohide,
            error,
            |config, on| config.autohide = on,
        ))
        .row(switch_row(
            tr!("dock-launchanim"),
            config.launchanim,
            error,
            |config, on| config.launchanim = on,
        ))
        .row(switch_row(
            tr!("dock-indicators"),
            config.show_process_indicators,
            error,
            |config, on| config.show_process_indicators = on,
        ))
        .row(switch_row(
            tr!("dock-recents"),
            config.show_recents,
            error,
            |config, on| config.show_recents = on,
        ))
        .row(switch_row(
            tr!("dock-downloads"),
            config.show_downloads,
            error,
            |config, on| config.show_downloads = on,
        ))
}

fn dock_advanced_section(config: &DockConfig, error: State<Option<String>>) -> Section {
    Section::titled(tr!("dock-advanced"))
        .row(switch_row(
            tr!("dock-static-only"),
            config.static_only,
            error,
            |config, on| config.static_only = on,
        ))
        .row(
            Row::new(tr!("dock-autohide-delay")).control(
                Slider::stepped(tr!("dock-autohide-delay"), DELAY_STEPS)
                    .value(
                        (config.autohide_delay / DELAY_MAX_SECONDS * (DELAY_STEPS - 1) as f32)
                            .round(),
                    )
                    .labels(tr_list!(
                        "dock-autohide-delay-none",
                        "dock-autohide-delay-max"
                    ))
                    .disabled(!config.autohide)
                    .on_change(edit_dock(error, |config: &mut DockConfig, step: f32| {
                        config.autohide_delay = step / (DELAY_STEPS - 1) as f32 * DELAY_MAX_SECONDS;
                    })),
            ),
        )
        .row(
            Row::new(tr!("dock-autohide-speed")).control(
                Slider::stepped(tr!("dock-autohide-speed"), SPEED_STEPS)
                    .value(
                        (config.autohide_time_modifier / SPEED_MAX * (SPEED_STEPS - 1) as f32)
                            .round(),
                    )
                    .labels(tr_list!(
                        "dock-autohide-speed-none",
                        "dock-autohide-speed-slow"
                    ))
                    .disabled(!config.autohide)
                    .on_change(edit_dock(error, |config: &mut DockConfig, step: f32| {
                        config.autohide_time_modifier = step / (SPEED_STEPS - 1) as f32 * SPEED_MAX;
                    })),
            ),
        )
}

pub(crate) fn desktop_pane(
    resolved: &Resolved,
    appearance: &AppearanceConfig,
    config: &MenubarConfig,
    error: State<Option<String>>,
) -> Pane {
    let dock = use_defaults::<DockConfig>(crate::dock::DOMAIN)
        .read()
        .clone();
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
    Pane::new(
        "desktop-dock",
        tr!("desktop-dock"),
        Symbol::MenubarDockRectangle,
        SETTINGS.tile_black,
        Form::new()
            .section(dock_section(&dock, error))
            .section(dock_advanced_section(&dock, error))
            .section(
                Section::titled(tr!("menu-bar"))
                    .row(
                        Row::new(tr!("menu-bar-style"))
                            .keywords(tr_list!("menu-bar-style-keywords"))
                            .control(styles),
                    )
                    .row(Row::new(tr!("menu-bar-height")).control(heights)),
            )
            .section(Section::titled(tr!("menu-bar-dock")).row(Row::toggle(
                tr!("tint-menu-bar-dock"),
                appearance.tint_parts.bars,
                edit_appearance(error, |config, on: bool| {
                    config.tint_parts.bars = on;
                }),
            ))),
    )
}
