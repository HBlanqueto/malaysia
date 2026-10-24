//! The menubar container.
//!
//! Owns the layer-shell surface, schedules the clock and active-app polling,
//! and lays out the modules left-to-right with the clock pushed to the right.

use std::path::PathBuf;

use celestial_kit::platform::{
    Anchors, KeyboardInteractivity, Layer as SurfaceLayer, SurfaceSpec, Timers, run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_appearance::data_dir;
use yuki_design::{MENUBAR_FONT_SIZE, MENUBAR_HEIGHT, MENUBAR_MARGIN, SANS_FAMILIES};

use crate::desktop::DesktopEntry;
use crate::menubar::{appmenu, clock, load_active_window, resolve_app_name, rgba, status, system};

pub struct BarApp {
    pub clock: State<String>,
    pub app_name: State<String>,
    pub window_store: PathBuf,
    pub entries: Vec<DesktopEntry>,
}

impl App for BarApp {
    fn render(&self) -> impl IntoElement {
        let timers = Timers::get();
        let clock_state = self.clock;
        use_hook(move || schedule_next_minute(timers, clock_state));

        let timers = Timers::get();
        let app_name = self.app_name;
        let window_store = self.window_store.clone();
        let entries = self.entries.clone();
        use_hook(move || schedule_active_poll(timers, window_store, entries, app_name));

        let now = self.clock.read().clone();
        let app = self.app_name.read().clone();
        let palette = celestial_kit::theme::shell_palette(&celestial_kit::theme::resolved());
        let text = rgba(palette.text);
        let bar = rgba(palette.bar);

        let mut root = rect()
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
            .font_size(MENUBAR_FONT_SIZE);
        for family in SANS_FAMILIES {
            root = root.font_family(family);
        }

        let left = rect()
            .direction(Direction::Horizontal)
            .main_align(Alignment::Start)
            .child(system::system_title())
            .child(appmenu::app_menu_title(app));

        root.child(left)
            .child(status::status_placeholder())
            .child(clock::clock_label(now, text))
    }
}

fn schedule_active_poll(
    timers: Timers,
    window_store: PathBuf,
    entries: Vec<DesktopEntry>,
    app_name: State<String>,
) {
    let retry = timers.clone();
    timers.after(std::time::Duration::from_millis(250), move || {
        let name = load_active_app(&window_store, &entries);
        let mut app_name = app_name;
        if *app_name.peek() != name {
            app_name.set(name);
        }
        schedule_active_poll(
            retry.clone(),
            window_store.clone(),
            entries.clone(),
            app_name,
        );
    });
}

fn schedule_next_minute(timers: Timers, clock: State<String>) {
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

fn load_active_app(store_path: &PathBuf, entries: &[DesktopEntry]) -> String {
    let active = load_active_window(store_path).unwrap_or_default();
    resolve_app_name(Some(&active.app_id), entries)
}

/// Connects to the compositor and runs the menu bar until closed.
pub fn run() -> anyhow::Result<()> {
    let window_store = data_dir().join("window.domain");
    let entries = crate::desktop::read_entries();
    let clock = State::create_global(chrono::Local::now().format("%H:%M").to_string());
    let app_name = State::create_global(load_active_app(&window_store, &entries));
    let app = AppComponent::new(BarApp {
        clock,
        app_name,
        window_store,
        entries,
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
