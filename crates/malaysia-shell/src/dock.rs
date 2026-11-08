use bk::adornments::Badge;
use bk::app::use_defaults;
use bk::menu::MenuItem;
use bk::platform::Timers;
use bk::prelude::*;
use bk::shell::{Anchors, KeyboardInteractivity, Layer, LayerSpec, Toplevels, run_layer};
use ctappearance::notifications::NotificationsConfig;
use ctdesign::{
    DOCK_BAR_HEIGHT, DOCK_FONT_SIZE, DOCK_GAP, DOCK_PADDING, DOCK_PROGRESS_HEIGHT, DOCK_TILESIZE,
};
use freya_core::integration::AppComponent;
use torin::prelude::Position;

use bk::theme;

use crate::launcher_entry::{self, Entries, Entry};
use crate::localize::tr;

struct DockApp;

impl App for DockApp {
    fn render(&self) -> impl IntoElement {
        let toplevels = Toplevels::get();
        let entries = toplevels.list.read().clone();
        let launcher_entries = use_hook(Entries::default);
        let tick = use_state(|| 0u64);
        use_hook({
            let launcher_entries = launcher_entries.clone();
            move || {
                let wake = Timers::get().wakeup(move || {
                    let mut tick = tick;
                    *tick.write() += 1;
                });
                launcher_entry::start(launcher_entries, wake);
            }
        });
        let _ = *tick.read();
        let config = use_defaults::<NotificationsConfig>("notifications");
        let config = config.read();
        let resolved = bk::theme::resolved();
        let palette = resolved.shell;
        let background = theme::color(bk::theme::surface_background(
            &resolved,
            bk::theme::SurfaceKind::LayerShell,
        ));
        let tile = theme::color(palette.tile);
        let active_tile = theme::color(palette.tile_active);
        let text = theme::color(palette.text);
        let active_text = theme::color(palette.tile_text_active);

        let tiles = entries.iter().map(|entry| {
            let toplevels = toplevels.clone();
            let title = if entry.title.is_empty() {
                entry.app_id.clone()
            } else {
                entry.title.clone()
            };
            let initial = title.chars().next().unwrap_or('?').to_string();
            let is_active = entry.active;
            let id = entry.id;
            let minimized = entry.minimized;
            let menu = vec![
                MenuItem::header(entry.app_id.clone()),
                if minimized {
                    MenuItem::action("show", tr!("dock-show"))
                } else {
                    MenuItem::action("minimize", tr!("window-minimize"))
                },
                MenuItem::Separator,
                MenuItem::action("close", tr!("menu-close-window")),
            ];
            let commands = toplevels.clone();
            let launcher = launcher_entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .find(|(id, _)| id.eq_ignore_ascii_case(&entry.app_id))
                .map(|(_, launcher)| launcher.clone());
            let badge = launcher
                .as_ref()
                .and_then(Entry::badge)
                .filter(|_| config.app(&entry.app_id).badge);
            let progress = launcher.as_ref().and_then(Entry::progress);
            let tile = rect()
                .width(Size::px(DOCK_TILESIZE as f32))
                .height(Size::px(DOCK_TILESIZE as f32))
                .corner_radius((DOCK_TILESIZE / 4) as f32)
                .background(if is_active { active_tile } else { tile })
                .center()
                .on_press(move |_| toplevels.activate(id))
                .child(
                    Text::new(initial)
                        .font_size(DOCK_FONT_SIZE)
                        .color(if is_active { active_text } else { text }),
                )
                .maybe_child(badge.map(|count| {
                    rect()
                        .position(
                            Position::new_absolute()
                                .top(-DOCK_GAP as f32)
                                .right(-DOCK_GAP as f32),
                        )
                        .child(Badge::new(count.to_string()))
                }))
                .maybe_child(progress.map(|progress| {
                    let inset = DOCK_TILESIZE as f32 / 8.0;
                    let width = DOCK_TILESIZE as f32 - 2.0 * inset;
                    rect()
                        .position(Position::new_absolute().bottom(inset).left(inset))
                        .width(Size::px(width))
                        .height(Size::px(DOCK_PROGRESS_HEIGHT))
                        .corner_radius(DOCK_PROGRESS_HEIGHT / 2.0)
                        .background(theme::color(palette.tile))
                        .child(
                            rect()
                                .width(Size::px(width * progress as f32))
                                .height(Size::fill())
                                .corner_radius(DOCK_PROGRESS_HEIGHT / 2.0)
                                .background(active_tile),
                        )
                }));
            tile.context_menu(menu, move |command: String| match command.as_str() {
                "show" => {
                    commands.unset_minimized(id);
                    commands.activate(id);
                }
                "minimize" => commands.minimize(id),
                _ => commands.close(id),
            })
            .into_element()
        });

        HStack::new()
            .width(Size::fill())
            .height(Size::fill())
            .background(background)
            .center()
            .spacing(DOCK_GAP as f32)
            .padding(DOCK_PADDING as f32)
            .children(tiles)
    }
}

pub fn run() -> anyhow::Result<()> {
    let app = AppComponent::new(DockApp);

    run_layer(
        app,
        LayerSpec {
            namespace: "malaysia-dock".to_string(),
            layer: Layer::Top,
            anchors: Anchors::BOTTOM,
            exclusive_zone: DOCK_BAR_HEIGHT,
            keyboard: KeyboardInteractivity::None,
            size: Some((0, DOCK_BAR_HEIGHT as u32)),
        },
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}
