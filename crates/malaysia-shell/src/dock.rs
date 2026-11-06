use barrier_kit::context_menu::context_menu;
use barrier_kit::menu::MenuItem;
use barrier_kit::shell::{Anchors, KeyboardInteractivity, Layer, LayerSpec, Toplevels, run_layer};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Direction, Size};
use vow_design::{DOCK_BAR_HEIGHT, DOCK_FONT_SIZE, DOCK_GAP, DOCK_PADDING, DOCK_TILESIZE};

use barrier_kit::theme;

use crate::localize::tr;

struct DockApp;

impl App for DockApp {
    fn render(&self) -> impl IntoElement {
        let toplevels = Toplevels::get();
        let entries = toplevels.list.read().clone();
        let resolved = barrier_kit::theme::resolved();
        let palette = resolved.shell;
        let background = theme::color(barrier_kit::theme::surface_background(
            &resolved,
            barrier_kit::theme::SurfaceKind::LayerShell,
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
            let tile = rect()
                .width(Size::px(DOCK_TILESIZE as f32))
                .height(Size::px(DOCK_TILESIZE as f32))
                .corner_radius((DOCK_TILESIZE / 4) as f32)
                .background(if is_active { active_tile } else { tile })
                .center()
                .on_press(move |_| toplevels.activate(id))
                .child(
                    label()
                        .text(initial)
                        .font_size(DOCK_FONT_SIZE)
                        .color(if is_active { active_text } else { text }),
                );
            context_menu(tile, menu, move |command: String| match command.as_str() {
                "show" => {
                    commands.unset_minimized(id);
                    commands.activate(id);
                }
                "minimize" => commands.minimize(id),
                _ => commands.close(id),
            })
            .into_element()
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(background)
            .direction(Direction::Horizontal)
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
