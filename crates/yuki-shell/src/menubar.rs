//! The menu bar component: the bar at the top of the screen.
//!
//! A `Top` layer-shell surface with an exclusive zone, like macOS's menu bar.
//! It shows the **active app's name** (published by the compositor in the
//! `window.domain` state store) on the left and the **clock** on the right.
//! Drawing is CelestialKit on Freya (ADR 0034).

use std::path::PathBuf;

use celestial_kit::platform::{
    Anchors, KeyboardInteractivity, Layer as SurfaceLayer, SurfaceSpec, run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_design::{MENUBAR_FONT_SIZE, MENUBAR_HEIGHT, MENUBAR_MARGIN, MENUBAR_TITLE_MAX_WIDTH};

use yuki_appearance::data_dir;

struct MenubarApp {
    clock: State<String>,
    window_store: PathBuf,
}

impl App for MenubarApp {
    fn render(&self) -> impl IntoElement {
        let timers = celestial_kit::platform::Timers::get();
        let clock = self.clock;
        use_hook(move || schedule_next_minute(timers, clock));

        let now = self.clock.read().clone();
        let title = load_active_window(&self.window_store).unwrap_or_default();
        let palette = celestial_kit::theme::shell_palette(&celestial_kit::theme::resolved());
        let text = rgba(palette.text);
        let bar = rgba(palette.bar);

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(bar)
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .padding(Gaps::new(
                0.0,
                MENUBAR_MARGIN as f32,
                0.0,
                MENUBAR_MARGIN as f32,
            ))
            .font_size(MENUBAR_FONT_SIZE)
            .child(
                label()
                    .max_lines(1)
                    .width(Size::px(MENUBAR_TITLE_MAX_WIDTH))
                    .text(title)
                    .color(text),
            )
            .child(label().text(now).color(text))
    }
}

fn schedule_next_minute(timers: celestial_kit::platform::Timers, clock: State<String>) {
    let retry = timers.clone();
    timers.after(seconds_to_next_minute(), move || {
        let mut clock = clock;
        clock.set(chrono::Local::now().format("%H:%M").to_string());
        schedule_next_minute(retry.clone(), clock);
    });
}

fn seconds_to_next_minute() -> std::time::Duration {
    use chrono::Timelike;
    let seconds = chrono::Local::now().second();
    std::time::Duration::from_secs(if seconds == 0 {
        60
    } else {
        (60 - seconds) as u64
    })
}

fn rgba(c: yuki_design::Rgba) -> Color {
    let [r, g, b, a] = c.to_rgba8();
    Color::from_argb(a, r, g, b)
}

/// Connects to the compositor and runs the menu bar until closed.
pub fn run() -> anyhow::Result<()> {
    let window_store = data_dir().join("window.domain");
    let clock = State::create_global(chrono::Local::now().format("%H:%M").to_string());
    let app = AppComponent::new(MenubarApp {
        clock,
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
