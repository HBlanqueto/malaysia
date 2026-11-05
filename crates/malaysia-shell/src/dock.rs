use barrier_kit::shell::{Anchors, KeyboardInteractivity, Layer, LayerSpec, Toplevels, run_layer};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Direction, Size};
use vow_design::{DOCK_BAR_HEIGHT, DOCK_FONT_SIZE, DOCK_GAP, DOCK_PADDING, DOCK_TILESIZE};

use barrier_kit::theme;

struct DockApp;

impl App for DockApp {
    fn render(&self) -> impl IntoElement {
        let toplevels = Toplevels::get();
        let entries = toplevels.list.read().clone();
        let resolved = barrier_kit::theme::resolved();
        let palette = barrier_kit::theme::shell_palette(&resolved);
        let background = theme::color(barrier_kit::theme::surface_background(
            &resolved,
            barrier_kit::theme::SurfaceKind::LayerShell,
        ));
        let tile = theme::color(palette.tile);
        let active_tile = theme::color(palette.tile_active);
        let text = theme::color(palette.text);
        let active_text = theme::color(palette.tile_text_active);

        let tiles = entries.iter().enumerate().map(|(index, entry)| {
            let toplevels = toplevels.clone();
            let title = if entry.title.is_empty() {
                entry.app_id.clone()
            } else {
                entry.title.clone()
            };
            let initial = title.chars().next().unwrap_or('?').to_string();
            let is_active = entry.active;
            rect()
                .width(Size::px(DOCK_TILESIZE as f32))
                .height(Size::px(DOCK_TILESIZE as f32))
                .corner_radius((DOCK_TILESIZE / 4) as f32)
                .background(if is_active { active_tile } else { tile })
                .center()
                .on_press(move |_| toplevels.activate(index))
                .child(
                    label()
                        .text(initial)
                        .font_size(DOCK_FONT_SIZE)
                        .color(if is_active { active_text } else { text }),
                )
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
