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
use yuki_design::{
    MENUBAR_APP_NAME_WEIGHT, MENUBAR_FONT_SIZE, MENUBAR_HEIGHT, MENUBAR_MARGIN,
    MENUBAR_TITLE_MAX_WIDTH, SANS_FAMILIES,
};

use yuki_appearance::data_dir;

use crate::desktop::{self, DesktopEntry};

struct MenubarApp {
    clock: State<String>,
    app_name: State<String>,
    window_store: PathBuf,
    entries: Vec<DesktopEntry>,
}

impl App for MenubarApp {
    fn render(&self) -> impl IntoElement {
        let timers = celestial_kit::platform::Timers::get();
        let clock = self.clock;
        use_hook(move || schedule_next_minute(timers, clock));

        let timers = celestial_kit::platform::Timers::get();
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
        root.child(
            label()
                .max_lines(1)
                .width(Size::px(MENUBAR_TITLE_MAX_WIDTH))
                .text(app)
                .font_weight(MENUBAR_APP_NAME_WEIGHT)
                .color(text),
        )
        .child(label().text(now).color(text))
    }
}

fn schedule_active_poll(
    timers: celestial_kit::platform::Timers,
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
    let entries = desktop::read_entries();
    let clock = State::create_global(chrono::Local::now().format("%H:%M").to_string());
    let app_name = State::create_global(load_active_app(&window_store, &entries));
    let app = AppComponent::new(MenubarApp {
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

/// The app name for the active window in the compositor's store.
///
/// The window title is deliberately ignored: macOS names the app, not the
/// document. A missing window or app id is the desktop, «Yuki».
fn load_active_app(store_path: &PathBuf, entries: &[DesktopEntry]) -> String {
    #[derive(serde::Deserialize)]
    struct WindowStore {
        #[serde(default)]
        active: Active,
    }
    #[derive(Default, serde::Deserialize)]
    struct Active {
        #[serde(default)]
        app_id: String,
    }
    let Ok(text) = std::fs::read_to_string(store_path) else {
        return "Yuki".to_string();
    };
    let Ok(store) = toml::from_str::<WindowStore>(&text) else {
        return "Yuki".to_string();
    };
    resolve_app_name(Some(&store.active.app_id), entries)
}

/// Resolves an app id to its menu-bar name.
fn resolve_app_name(app_id: Option<&str>, entries: &[DesktopEntry]) -> String {
    let Some(app_id) = app_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return "Yuki".to_string();
    };
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.id == app_id || entry.startup_wm_class.as_deref() == Some(app_id))
    {
        return entry.name.clone();
    }
    let segment = app_id
        .rsplit('.')
        .find(|segment| !segment.is_empty())
        .unwrap_or(app_id);
    capitalize(segment)
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, wm_class: Option<&str>) -> DesktopEntry {
        DesktopEntry {
            id: id.to_string(),
            name: name.to_string(),
            generic_name: None,
            keywords: Vec::new(),
            icon: None,
            exec: vec![id.to_string()],
            path: None,
            terminal: false,
            startup_wm_class: wm_class.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn foot_resolves_through_its_desktop_entry() {
        let entries = vec![entry("foot", "Foot", None)];
        assert_eq!(resolve_app_name(Some("foot"), &entries), "Foot");
    }

    #[test]
    fn wezterm_resolves_through_its_desktop_id() {
        let entries = vec![entry("org.wezfurlong.wezterm", "WezTerm", None)];
        assert_eq!(
            resolve_app_name(Some("org.wezfurlong.wezterm"), &entries),
            "WezTerm"
        );
    }

    #[test]
    fn startup_wm_class_matches_when_the_id_does_not() {
        let entries = vec![entry("wezterm", "WezTerm", Some("org.wezfurlong.wezterm"))];
        assert_eq!(
            resolve_app_name(Some("org.wezfurlong.wezterm"), &entries),
            "WezTerm"
        );
    }

    #[test]
    fn an_app_without_a_desktop_entry_uses_the_last_segment() {
        let entries = Vec::new();
        assert_eq!(
            resolve_app_name(Some("org.example.notes"), &entries),
            "Notes"
        );
        assert_eq!(
            resolve_app_name(Some("com.example.my-app"), &entries),
            "My-app"
        );
    }

    #[test]
    fn no_window_or_no_app_id_is_yuki() {
        let entries = Vec::new();
        assert_eq!(resolve_app_name(None, &entries), "Yuki");
        assert_eq!(resolve_app_name(Some("   "), &entries), "Yuki");
    }

    #[test]
    fn the_window_title_is_never_used() {
        let dir = std::env::temp_dir().join(format!("yuki-menubar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("the test directory");
        let path = dir.join("window.domain");
        std::fs::write(
            &path,
            "[active]\ntitle = \"humbe@ThinkPad: ~/src/yukiwm\"\napp_id = \"\"\n",
        )
        .expect("the store");
        assert_eq!(load_active_app(&path, &[]), "Yuki");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
