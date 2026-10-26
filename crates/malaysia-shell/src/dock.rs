//! The dock component: the macOS-style dock at the screen edge.
//!
//! A layer-shell surface on the `Top` layer. It lists the running toplevels
//! from `ext-foreign-toplevel-list-v1` and, on click, activates the window
//! through `zwlr_foreign_toplevel_management_v1`. The active app is
//! highlighted from the compositor's `window.domain` store. Drawing is
//! BarrierKit on Freya (ADR 0034).

use std::path::PathBuf;

use barrier_kit::platform::{
    Anchors, KeyboardInteractivity, Layer, SurfaceSpec, Toplevels, run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use vow_design::{DOCK_BAR_HEIGHT, DOCK_FONT_SIZE, DOCK_GAP, DOCK_PADDING, DOCK_TILESIZE};

use vow_appearance::data_dir;

struct DockApp {
    window_store: PathBuf,
}

impl App for DockApp {
    fn render(&self) -> impl IntoElement {
        let toplevels = Toplevels::get();
        let entries = toplevels.list.read().clone();
        let active = load_active_title(&self.window_store).unwrap_or_default();
        let palette = barrier_kit::theme::shell_palette(&barrier_kit::theme::resolved());
        let tile = rgba(palette.tile);
        let active_tile = rgba(palette.tile_active);
        let text = rgba(palette.text);
        let active_text = rgba(palette.tile_text_active);

        let tiles = entries.iter().enumerate().map(|(index, entry)| {
            let toplevels = toplevels.clone();
            let title = if entry.title.is_empty() {
                entry.app_id.clone()
            } else {
                entry.title.clone()
            };
            let initial = title.chars().next().unwrap_or('?').to_string();
            let is_active = entry.title == active && !active.is_empty();
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
            .background(rgba(palette.bar))
            .direction(Direction::Horizontal)
            .main_align(Alignment::Center)
            .cross_align(Alignment::Center)
            .spacing(DOCK_GAP as f32)
            .padding(Gaps::new(
                DOCK_PADDING as f32,
                DOCK_PADDING as f32,
                DOCK_PADDING as f32,
                DOCK_PADDING as f32,
            ))
            .children(tiles)
    }
}

/// Connects to the compositor and runs the dock.
pub fn run() -> anyhow::Result<()> {
    let window_store = data_dir().join("window.domain");
    let app = AppComponent::new(DockApp { window_store });

    run_layer(
        app,
        SurfaceSpec::Layer {
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

fn rgba(c: vow_design::Rgba) -> Color {
    let [r, g, b, a] = c.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn load_active_title(store_path: &PathBuf) -> anyhow::Result<String> {
    #[derive(serde::Deserialize)]
    struct WindowStore {
        #[serde(default)]
        active: Active,
    }
    #[derive(Default, serde::Deserialize)]
    struct Active {
        #[serde(default)]
        title: String,
    }
    let text = std::fs::read_to_string(store_path)?;
    let store: WindowStore = toml::from_str(&text)?;
    Ok(store.active.title)
}
