//! The menubar container.
//!
//! Owns the layer-shell surface, schedules the clock and active-app polling,
//! and lays out the modules left-to-right with the clock pushed to the right.
//! Every menu title shares a single popup state so only one menu tree is open
//! at a time.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use celestial_kit::menu::MenuItem;
use celestial_kit::platform::{
    Anchors, KeyboardInteractivity, Layer as SurfaceLayer, SurfaceSpec, Timers, Toplevels,
    run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_appearance::{Resolved, data_dir};
use yuki_design::{MENUBAR_FONT_SIZE, MENUBAR_HEIGHT, MENUBAR_MARGIN, SANS_FAMILIES};

use crate::desktop::DesktopEntry;
use crate::menubar::dbusmenu::{self, MenuClient};
use crate::menubar::dbusmenu_map;
use crate::menubar::menu::{self, MenuItems, MenuState, MenuTitleSpec, SearchableItems};
use crate::menubar::remote;
use crate::menubar::{appmenu, clock, focused, load_active_window, resolve_app_name, rgba, system};
use celestial_kit::menu::SearchQuery;
use celestial_kit::platform::AppMenuAddress;

type TitleAnchor = Rc<Cell<(f32, f32)>>;
/// The mapped menu of the focused window, kept only while its generation is
/// current.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteMenu {
    pub generation: u64,
    pub address: AppMenuAddress,
    pub items: Vec<MenuItem>,
}

pub struct BarApp {
    pub clock: State<String>,
    pub active_app: State<Option<String>>,
    pub appearance: State<Option<Resolved>>,
    pub window_store: PathBuf,
    pub entries: Vec<DesktopEntry>,
    pub dbus: MenuClient,
    pub menu_snapshot: State<Option<Arc<RemoteMenu>>>,
}

impl App for BarApp {
    fn render(&self) -> impl IntoElement {
        let timers = Timers::get();
        let clock_state = self.clock;
        use_hook(move || schedule_next_minute(timers, clock_state));

        let timers = Timers::get();
        let active_app = self.active_app;
        let window_store = self.window_store.clone();
        let poll_store = window_store.clone();
        use_hook(move || schedule_active_poll(timers, poll_store, active_app));

        let timers = Timers::get();
        let appearance = self.appearance;
        use_hook(move || schedule_appearance_poll(timers, appearance));

        let generation = use_hook(|| Rc::new(Cell::new(0u64)));
        let timers = Timers::get();
        let dbus = self.dbus.clone();
        let menu_snapshot = self.menu_snapshot;
        let generation_for_drain = generation.clone();
        use_hook(move || {
            schedule_snapshot_drain(timers, dbus, menu_snapshot, generation_for_drain)
        });
        let resolved = (*self.appearance.read()).unwrap_or_else(celestial_kit::theme::resolved);

        let now = self.clock.read().clone();
        let app_id = self.active_app.read().clone();
        let app = resolve_app_name(app_id.as_deref(), &self.entries);
        let focused = app_id.is_some();
        let last_app_menu = use_hook(|| {
            Rc::new(RefCell::new(
                None::<(String, Option<celestial_kit::platform::AppMenuAddress>)>,
            ))
        });
        let toplevels = Toplevels::get().list.read().clone();
        let active_info = toplevels.iter().find(|window| window.active).or_else(|| {
            app_id
                .as_deref()
                .and_then(|id| toplevels.iter().find(|window| window.app_id == id))
        });
        let key = active_info.map(|info| (info.app_id.clone(), info.app_menu.clone()));
        {
            let mut snapshot_state = self.menu_snapshot;
            let mut last = last_app_menu.borrow_mut();
            if *last != key {
                let next_generation = generation.get() + 1;
                generation.set(next_generation);
                snapshot_state.set(None);
                match &key {
                    Some((app_id, Some(menu))) => {
                        tracing::info!(
                            app_id = %app_id,
                            service = %menu.service,
                            path = %menu.path,
                            generation = next_generation,
                            "menubar: app_menu"
                        );
                        self.dbus.focus(menu.clone(), next_generation);
                    }
                    Some((app_id, None)) => {
                        tracing::info!(app_id = %app_id, "menubar: app_menu none")
                    }
                    None => {}
                }
                *last = key;
            }
        }
        let palette = celestial_kit::theme::shell_palette(&resolved);
        let text = rgba(palette.text);
        // Opaque by default; the blur for reduce_transparency = false will
        // arrive with the compositor's surface effects (ADR 0024).
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
        let help_query = use_hook(|| {
            Rc::new(RefCell::new(SearchQuery {
                text: String::new(),
                selected: false,
            }))
        });
        let anchors = use_hook(|| Rc::new(RefCell::new(Vec::<TitleAnchor>::new())));
        let title_anchor = |index: usize| {
            let mut anchors = anchors.borrow_mut();
            while anchors.len() <= index {
                anchors.push(Rc::new(Cell::new((0.0, 0.0))));
            }
            anchors[index].clone()
        };
        let mut specs = vec![MenuTitleSpec {
            label: "Sistema".to_string(),
            glyph: Some(system::SNOWFLAKE_SVG),
            bold: false,
            items: Rc::new(system::system_menu_items),
            searchable: None,
            on_activate: Rc::new(system::on_system_action),
            live_items: None,
            on_open: None,
            on_hover_title: None,
            on_open_submenu: None,
            submenu_source: None,
            anchor: title_anchor(0),
            enabled: true,
            query_state: None,
            reveal_roots: None,
        }];
        if focused {
            let app_id = app_id.clone().unwrap_or_default();
            let app_name = app.clone();
            let app_items: MenuItems = {
                let app_id = app_id.clone();
                let app_name = app_name.clone();
                Rc::new(move || {
                    focused::app_menu_items(
                        &app_name,
                        &app_id,
                        &Toplevels::get().list.read().clone(),
                    )
                })
            };
            let window_items: MenuItems = {
                let app_id = app_id.clone();
                let app_name = app_name.clone();
                Rc::new(move || {
                    focused::window_menu_items(
                        &app_name,
                        &app_id,
                        &Toplevels::get().list.read().clone(),
                    )
                })
            };
            let help_items: MenuItems = {
                let app_name = app_name.clone();
                Rc::new(move || focused::help_menu_items(&app_name))
            };
            let remote_menu = self.menu_snapshot.read().as_ref().cloned();
            if let Some(remote_menu) = remote_menu {
                let sources = remote::MenuSources {
                    app_name: app_name.clone(),
                    app_items: app_items.clone(),
                    window_items: window_items.clone(),
                    help_items: help_items.clone(),
                };
                let snapshot = self.menu_snapshot;
                let composition: Rc<dyn Fn() -> Vec<remote::Title>> = {
                    let sources = sources.clone();
                    Rc::new(move || {
                        let items = snapshot
                            .read()
                            .as_ref()
                            .map(|menu| menu.items.clone())
                            .unwrap_or_default();
                        sources.compose(&items)
                    })
                };
                let address = remote_menu.address.clone();
                let generation_now = remote_menu.generation;
                let on_activate: Rc<dyn Fn(String)> = {
                    let dbus = self.dbus.clone();
                    let address = address.clone();
                    Rc::new(move |id| {
                        if let Some(remote_id) = id.strip_prefix("dbusmenu:") {
                            if let Ok(remote_id) = remote_id.parse::<i32>() {
                                dbus.activate(address.clone(), remote_id, now_ms());
                            }
                        } else {
                            focused::on_action(id);
                        }
                    })
                };
                let on_open_submenu: Rc<dyn Fn(String)> = {
                    let dbus = self.dbus.clone();
                    let address = address.clone();
                    Rc::new(move |id| {
                        if let Ok(id) = id.parse::<i32>() {
                            dbus.open(address.clone(), id, generation_now);
                        }
                    })
                };
                let submenu_source: celestial_kit::menu::SubmenuSource = {
                    let snapshot = self.menu_snapshot;
                    Rc::new(move |id| {
                        let id = id.parse::<i32>().ok()?;
                        let menu = snapshot.read().as_ref().cloned()?;
                        remote::find_subtree(&menu.items, id)
                    })
                };
                let titles = composition();
                let reveal_roots: Rc<Vec<Vec<MenuItem>>> =
                    Rc::new(titles.iter().map(remote::Title::visible_items).collect());
                for (index, title) in titles.iter().enumerate() {
                    let items: MenuItems = {
                        let composition = composition.clone();
                        Rc::new(move || {
                            composition()
                                .get(index)
                                .map(remote::Title::visible_items)
                                .unwrap_or_default()
                        })
                    };
                    let on_open: Option<Rc<dyn Fn()>> = title.remote_id.map(|id| {
                        let dbus = self.dbus.clone();
                        let address = address.clone();
                        let callback: Rc<dyn Fn()> = Rc::new(move || {
                            dbus.open(address.clone(), id, generation_now);
                        });
                        callback
                    });
                    let live_items = Some(items.clone());
                    let on_hover_title = on_open.clone();
                    let is_help = index + 1 == titles.len();
                    let searchable: Option<SearchableItems> = if is_help {
                        let composition = composition.clone();
                        Some(Rc::new(move || menu::SearchableLists {
                            menu: remote::searchable(&composition()),
                            help: focused::help_search_topics(),
                        }))
                    } else {
                        None
                    };
                    specs.push(MenuTitleSpec {
                        label: title.label.clone(),
                        glyph: None,
                        bold: title.bold,
                        items,
                        searchable,
                        on_activate: on_activate.clone(),
                        live_items,
                        on_open,
                        on_hover_title,
                        on_open_submenu: Some(on_open_submenu.clone()),
                        submenu_source: Some(submenu_source.clone()),
                        anchor: title_anchor(index + 1),
                        enabled: title.enabled,
                        query_state: is_help.then(|| help_query.clone()),
                        reveal_roots: is_help.then(|| reveal_roots.clone()),
                    });
                }
            } else {
                let help_searchable: SearchableItems = {
                    let app_name = app_name.clone();
                    let app_id = app_id.clone();
                    Rc::new(move || menu::SearchableLists {
                        menu: focused::searchable_menu_items(
                            &app_name,
                            &app_id,
                            &Toplevels::get().list.read().clone(),
                        ),
                        help: focused::help_search_topics(),
                    })
                };
                let reveal_roots = Rc::new(vec![(app_items)(), (window_items)()]);
                let focused_titles = [
                    (
                        app_name.clone(),
                        app_items,
                        1usize,
                        None,
                        None::<Rc<RefCell<SearchQuery>>>,
                        None::<Rc<Vec<Vec<MenuItem>>>>,
                    ),
                    ("Ventana".to_string(), window_items, 2, None, None, None),
                    (
                        "Ayuda".to_string(),
                        help_items,
                        3,
                        Some(help_searchable),
                        Some(help_query.clone()),
                        Some(reveal_roots),
                    ),
                ];
                for (offset, (label, items, anchor, searchable, query_state, reveal_roots)) in
                    focused_titles.into_iter().enumerate()
                {
                    specs.push(MenuTitleSpec {
                        label,
                        glyph: None,
                        bold: offset == 0,
                        items,
                        searchable,
                        on_activate: Rc::new(focused::on_action),
                        live_items: None,
                        on_open: None,
                        on_hover_title: None,
                        on_open_submenu: None,
                        submenu_source: None,
                        anchor: title_anchor(anchor),
                        enabled: true,
                        query_state,
                        reveal_roots,
                    });
                }
            }
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
                    searchable: None,
                    on_activate: Rc::new(appmenu::on_action),
                    live_items: None,
                    on_open: None,
                    on_hover_title: None,
                    on_open_submenu: None,
                    submenu_source: None,
                    anchor: title_anchor(offset + 1),
                    enabled: true,
                    query_state: None,
                    reveal_roots: None,
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

fn schedule_snapshot_drain(
    timers: Timers,
    dbus: MenuClient,
    snapshot: State<Option<Arc<RemoteMenu>>>,
    generation: Rc<Cell<u64>>,
) {
    let drain = {
        let dbus = dbus.clone();
        let generation = generation.clone();
        move || {
            let mut snapshot = snapshot;
            while let Some(next) = dbus.try_recv() {
                if next.generation != generation.get() {
                    continue;
                }
                let mapped = next
                    .items
                    .as_ref()
                    .map(|items| dbusmenu_map::menu_items(items));
                match next.parent {
                    None => {
                        let menu = mapped.map(|items| {
                            Arc::new(RemoteMenu {
                                generation: next.generation,
                                address: next.address.clone(),
                                items,
                            })
                        });
                        snapshot.set(menu);
                    }
                    Some(id) => {
                        let Some(current) = snapshot.peek().as_ref().cloned() else {
                            continue;
                        };
                        if current.generation != next.generation || current.address != next.address
                        {
                            continue;
                        }
                        let children = mapped.unwrap_or_else(|| vec![remote::unavailable()]);
                        let mut items = current.items.clone();
                        if remote::merge_subtree(&mut items, id, &children) {
                            snapshot.set(Some(Arc::new(RemoteMenu {
                                generation: current.generation,
                                address: current.address.clone(),
                                items,
                            })));
                        }
                    }
                }
            }
        }
    };
    dbus.set_wake(timers.wakeup(drain));
}

fn now_ms() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32
}

fn schedule_appearance_poll(timers: Timers, appearance: State<Option<Resolved>>) {
    let retry = timers.clone();
    timers.after(std::time::Duration::from_millis(500), move || {
        let resolved = celestial_kit::theme::resolved();
        let mut appearance = appearance;
        if appearance.peek().as_ref() != Some(&resolved) {
            appearance.set(Some(resolved));
        }
        schedule_appearance_poll(retry.clone(), appearance);
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
    let appearance = State::create_global(Some(celestial_kit::theme::resolved()));
    let menu_snapshot = State::create_global(None::<Arc<RemoteMenu>>);
    let app = AppComponent::new(BarApp {
        clock,
        active_app,
        appearance,
        window_store,
        entries,
        dbus: dbusmenu::start(),
        menu_snapshot,
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
