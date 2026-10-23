//! The dock component: the macOS-style dock at the screen edge.
//!
//! A layer-shell surface on the `Top` layer. It lists the running toplevels
//! from `ext-foreign-toplevel-list-v1` and, on click, activates the window
//! through `zwlr_foreign_toplevel_management_v1`. The active app is
//! highlighted from the compositor's `window.domain` store. Drawing is
//! CelestialKit on Freya (ADR 0034).

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use celestial_kit::platform::{
    Anchors, KeyboardInteractivity, Layer, SurfaceSpec, Toplevels, run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_design::{
    DOCK_BAR_HEIGHT, DOCK_FONT_SIZE, DOCK_GAP, DOCK_PADDING, DOCK_TILESIZE,
};

use yuki_appearance::data_dir;

#[derive(Clone, PartialEq)]
struct Model {
    active: String,
    theme_revision: u64,
}

struct DockApp {
    model: State<Model>,
    window_store: PathBuf,
}

impl App for DockApp {
    fn render(&self) -> impl IntoElement {
        let model = self.model;
        let store = self.window_store.clone();
        use_future(move || {
            let store = store.clone();
            let mut model = model;
            async move {
                let mut last: Option<(Option<SystemTime>, u64)> = None;
                loop {
                    async_io::Timer::after(Duration::from_millis(500)).await;
                    let modified = std::fs::metadata(&store).and_then(|m| m.modified()).ok();
                    let revision = theme_revision();
                    if last != Some((modified, revision)) {
                        last = Some((modified, revision));
                        let next = Model {
                            active: load_active_title(&store).unwrap_or_default(),
                            theme_revision: revision,
                        };
                        if next != *model.peek() {
                            model.set(next);
                        }
                    }
                }
            }
        });

        let toplevels = Toplevels::get();
        let entries = toplevels.list.read().clone();
        let model = self.model.read();
        let palette = celestial_kit::theme::shell_palette(&celestial_kit::theme::resolved());
        let tile = rgba(palette.tile);
        let active = rgba(palette.tile_active);
        let text = rgba(palette.tile_text_active);

        let tiles = entries.iter().enumerate().map(|(index, entry)| {
            let toplevels = toplevels.clone();
            let title = if entry.title.is_empty() {
                entry.app_id.clone()
            } else {
                entry.title.clone()
            };
            let initial = title.chars().next().unwrap_or('?').to_string();
            let is_active = entry.title == model.active && !model.active.is_empty();
            rect()
                .width(Size::px(DOCK_TILESIZE as f32))
                .height(Size::px(DOCK_TILESIZE as f32))
                .corner_radius((DOCK_TILESIZE / 4) as f32)
                .background(if is_active { active } else { tile })
                .center()
                .on_press(move |_| toplevels.activate(index))
                .child(
                    label()
                        .text(initial)
                        .font_size(DOCK_FONT_SIZE)
                        .color(text),
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
    let model = State::create_global(Model {
        active: load_active_title(&window_store).unwrap_or_default(),
        theme_revision: theme_revision(),
    });
    let app = AppComponent::new(DockApp {
        model,
        window_store,
    });
    run_layer(
        app,
        SurfaceSpec::Layer {
            namespace: "yuki-dock".to_string(),
            layer: Layer::Top,
            anchors: Anchors::BOTTOM,
            exclusive_zone: DOCK_BAR_HEIGHT,
            keyboard: KeyboardInteractivity::None,
            size: Some((0, DOCK_BAR_HEIGHT as u32)),
        },
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

fn rgba(c: yuki_design::Rgba) -> Color {
    let [r, g, b, a] = c.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn theme_revision() -> u64 {
    let mut hash = 0u64;
    for path in [
        yuki_appearance::appearance_path(),
        yuki_appearance::wallpaper_state_path(),
    ] {
        if let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified())
            && let Ok(value) = modified.duration_since(SystemTime::UNIX_EPOCH)
        {
            hash = hash.wrapping_mul(31).wrapping_add(value.as_nanos() as u64);
        }
    }
    hash
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
