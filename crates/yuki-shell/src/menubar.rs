//! The menu bar component: the bar at the top of the screen.
//!
//! A `Top` layer-shell surface with an exclusive zone, like macOS's menu bar.
//! It shows the **active app's name** (published by the compositor in the
//! `window.domain` state store) on the left and the **clock** on the right.
//! Drawing is CelestialKit on Freya (ADR 0034).

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use celestial_kit::platform::{Anchors, KeyboardInteractivity, Layer as SurfaceLayer, SurfaceSpec, run_layer};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_design::{
    MENUBAR_FONT_SIZE, MENUBAR_HEIGHT, MENUBAR_MARGIN, MENUBAR_TITLE_MAX_WIDTH,
};

use yuki_appearance::data_dir;

#[derive(Clone, PartialEq)]
struct Model {
    title: String,
    clock: String,
    /// Changes when the appearance files change, forcing a re-theme.
    theme_revision: u64,
}

struct MenubarApp {
    model: State<Model>,
    window_store: PathBuf,
}

impl App for MenubarApp {
    fn render(&self) -> impl IntoElement {
        let model = self.model;
        let store = self.window_store.clone();

        use_future(move || {
            let store = store.clone();
            let mut model = model;
            async move {
                let mut last_window: Option<Option<SystemTime>> = None;
                let mut last_theme = theme_revision();
                loop {
                    async_io::Timer::after(Duration::from_millis(500)).await;
                    let window = std::fs::metadata(&store).and_then(|m| m.modified()).ok();
                    let theme = theme_revision();
                    if last_window != Some(window) || last_theme != theme {
                        last_window = Some(window);
                        last_theme = theme;
                        let title = load_active_window(&store).unwrap_or_default();
                        let clock = chrono::Local::now().format("%H:%M").to_string();
                        let next = Model {
                            title,
                            clock,
                            theme_revision: theme,
                        };
                        if next != *model.peek() {
                            model.set(next);
                        }
                    }
                }
            }
        });

        let model = self.model.read();
        let palette = celestial_kit::theme::shell_palette(&celestial_kit::theme::resolved());
        let text = Color::from_argb(
            (palette.text.a * 255.0) as u8,
            (palette.text.r * 255.0) as u8,
            (palette.text.g * 255.0) as u8,
            (palette.text.b * 255.0) as u8,
        );
        let bar = Color::from_argb(
            (palette.bar.a * 255.0) as u8,
            (palette.bar.r * 255.0) as u8,
            (palette.bar.g * 255.0) as u8,
            (palette.bar.b * 255.0) as u8,
        );

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(bar)
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .padding(Gaps::new(0.0, MENUBAR_MARGIN as f32, 0.0, MENUBAR_MARGIN as f32))
            .font_size(MENUBAR_FONT_SIZE)
            .child(
                label()
                    .max_lines(1)
                    .width(Size::px(MENUBAR_TITLE_MAX_WIDTH))
                    .text(model.title.clone())
                    .color(text),
            )
            .child(label().text(model.clock.clone()).color(text))
    }
}

/// Connects to the compositor and runs the menu bar until closed.
pub fn run() -> anyhow::Result<()> {
    let window_store = data_dir().join("window.domain");
    let model = State::create_global(Model {
        title: load_active_window(&window_store).unwrap_or_default(),
        clock: chrono::Local::now().format("%H:%M").to_string(),
        theme_revision: theme_revision(),
    });
    let app = AppComponent::new(MenubarApp {
        model,
        window_store,
    });

    run_layer(
        app,
        SurfaceSpec::Layer {
            namespace: "yuki-menubar".to_string(),
            layer: SurfaceLayer::Top,
            anchors: Anchors::TOP,
            exclusive_zone: MENUBAR_HEIGHT,
            keyboard: KeyboardInteractivity::None,
            size: Some((0, MENUBAR_HEIGHT as u32)),
        },
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
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

/// Reads the active window title from the compositor's store.
fn load_active_window(store_path: &PathBuf) -> anyhow::Result<String> {
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
