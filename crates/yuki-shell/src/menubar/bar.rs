//! The menubar container.
//!
//! Owns the layer-shell surface, schedules the clock and active-app polling,
//! and lays out the modules left-to-right with the clock pushed to the right.
//! Every menu title shares a single popup state so only one menu tree is open
//! at a time.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use celestial_kit::menu::MenuItem;
use celestial_kit::platform::{
    Anchors, KeyboardInteractivity, Layer as SurfaceLayer, SurfaceSpec, Timers, run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_appearance::data_dir;
use yuki_design::{MENUBAR_FONT_SIZE, MENUBAR_HEIGHT, MENUBAR_MARGIN, SANS_FAMILIES};

use crate::desktop::DesktopEntry;
use crate::menubar::menu::{self, MenuState, MenuTitleSpec};
use crate::menubar::{appmenu, clock, load_active_window, resolve_app_name, rgba, system};

const MAX_TITLES: usize = 8;

type TitleAnchor = Rc<Cell<(f32, f32)>>;
type TitleAnchors = Rc<Vec<TitleAnchor>>;
type MenuItems = Rc<dyn Fn() -> Vec<MenuItem>>;

pub struct BarApp {
    pub clock: State<String>,
    pub active_app: State<Option<String>>,
    pub window_store: PathBuf,
    pub entries: Vec<DesktopEntry>,
}

impl App for BarApp {
    fn render(&self) -> impl IntoElement {
        let timers = Timers::get();
        let clock_state = self.clock;
        use_hook(move || schedule_next_minute(timers, clock_state));

        let timers = Timers::get();
        let active_app = self.active_app;
        let window_store = self.window_store.clone();
        use_hook(move || schedule_active_poll(timers, window_store, active_app));

        let now = self.clock.read().clone();
        let app_id = self.active_app.read().clone();
        let app = resolve_app_name(app_id.as_deref(), &self.entries);
        let focused = app_id.is_some();
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

        let menu_state = MenuState::new();
        let anchors: TitleAnchors = use_hook(|| {
            Rc::new(
                (0..MAX_TITLES)
                    .map(|_| Rc::new(Cell::new((0.0f32, 0.0f32))))
                    .collect(),
            )
        });
        let mut specs = vec![MenuTitleSpec {
            label: "Sistema".to_string(),
            glyph: Some(system::SNOWFLAKE_SVG),
            bold: false,
            items: Rc::new(system::system_menu_items),
            on_activate: system::on_system_action,
            anchor: anchors[0].clone(),
            enabled: true,
        }];
        if focused {
            specs.push(MenuTitleSpec {
                label: app,
                glyph: None,
                bold: true,
                items: Rc::new(no_items),
                on_activate: ignore_action,
                anchor: anchors[1].clone(),
                enabled: false,
            });
        } else {
            let entries_for_window = self.entries.clone();
            let menus: [(&str, MenuItems); 7] = [
                ("Yuki", Rc::new(appmenu::yuki_menu_items)),
                ("Archivo", Rc::new(appmenu::file_menu_items)),
                ("Edición", Rc::new(appmenu::edit_menu_items)),
                ("Visualización", Rc::new(appmenu::view_menu_items)),
                ("Ir", Rc::new(appmenu::go_menu_items)),
                (
                    "Ventana",
                    Rc::new(move || appmenu::window_menu_items(&entries_for_window)),
                ),
                ("Ayuda", Rc::new(appmenu::help_menu_items)),
            ];
            for (offset, (label, items)) in menus.into_iter().enumerate() {
                specs.push(MenuTitleSpec {
                    label: label.to_string(),
                    glyph: None,
                    bold: offset == 0,
                    items,
                    on_activate: appmenu::on_action,
                    anchor: anchors[offset + 1].clone(),
                    enabled: true,
                });
            }
        }
        let titles = Rc::new(specs);
        let mut left = rect()
            .direction(Direction::Horizontal)
            .main_align(Alignment::Start);
        for index in 0..titles.len() {
            left = left.child(menu::menu_title(index, titles.clone(), menu_state.clone()));
        }

        root.child(left).child(clock::clock_label(now, text))
    }
}

fn no_items() -> Vec<MenuItem> {
    Vec::new()
}

fn ignore_action(_id: String) {}

fn schedule_active_poll(timers: Timers, window_store: PathBuf, active_app: State<Option<String>>) {
    let retry = timers.clone();
    timers.after(std::time::Duration::from_millis(250), move || {
        let id = load_active_app(&window_store);
        let mut active_app = active_app;
        if *active_app.peek() != id {
            active_app.set(id);
        }
        schedule_active_poll(retry.clone(), window_store.clone(), active_app);
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

fn load_active_app(store_path: &PathBuf) -> Option<String> {
    let active = load_active_window(store_path).unwrap_or_default();
    let app_id = active.app_id.trim();
    (!app_id.is_empty()).then(|| app_id.to_string())
}

/// The layer-shell surface: top strip with its exclusive zone reserved.
fn surface_spec() -> SurfaceSpec {
    SurfaceSpec::Layer {
        namespace: "yuki-menubar".to_string(),
        layer: SurfaceLayer::Top,
        anchors: Anchors::TOP,
        exclusive_zone: MENUBAR_HEIGHT,
        keyboard: KeyboardInteractivity::None,
        size: Some((0, MENUBAR_HEIGHT as u32)),
    }
}

/// Connects to the compositor and runs the menu bar until closed.
pub fn run() -> anyhow::Result<()> {
    let window_store = data_dir().join("window.domain");
    let entries = crate::desktop::read_entries();
    let clock = State::create_global(chrono::Local::now().format("%H:%M").to_string());
    let active_app = State::create_global(load_active_app(&window_store));
    let app = AppComponent::new(BarApp {
        clock,
        active_app,
        window_store,
        entries,
    });

    run_layer(app, surface_spec()).map_err(|error| anyhow::anyhow!("{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_reserves_the_menubar_height() {
        match surface_spec() {
            SurfaceSpec::Layer {
                layer,
                anchors,
                exclusive_zone,
                size,
                ..
            } => {
                assert_eq!(layer, SurfaceLayer::Top);
                assert!(anchors.top && anchors.left && anchors.right);
                assert!(!anchors.bottom);
                assert_eq!(exclusive_zone, MENUBAR_HEIGHT);
                assert_eq!(size, Some((0, MENUBAR_HEIGHT as u32)));
            }
            _ => panic!("the menubar is a layer surface"),
        }
    }
}
