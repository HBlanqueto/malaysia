use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use barrier_kit::app::use_watched;
use barrier_kit::menu::MenuItem;
use barrier_kit::platform::Timers;
use barrier_kit::shell::{
    Anchors, GtkMenusAddress, KeyboardInteractivity, Layer as SurfaceLayer, LayerSpec, Toplevels,
    run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use vow_appearance::{config_dir, data_dir};
use vow_design::{
    MENUBAR_FONT_SIZE, MENUBAR_HEIGHT, MENUBAR_MARGIN, MENUBAR_MARGIN_RIGHT, MENUBAR_TEXT_LIFT,
    SANS_FAMILIES,
};

use crate::desktop::DesktopEntry;
use crate::menubar::dbusmenu::{self, MenuClient};
use crate::menubar::dbusmenu_map;
use crate::menubar::gtkmenus;
use crate::menubar::menu::{self, MenuItems, MenuState, MenuTitleSpec, SearchableItems};
use crate::menubar::remote;
use crate::menubar::{appmenu, clock, focused, load_active_window, resolve_app_name, system};
use barrier_kit::menu::{RevealRoots, SearchQuery};
use barrier_kit::platform::AppMenuAddress;
use barrier_kit::theme;

type TitleAnchor = Rc<Cell<(f32, f32)>>;

const MENU_GRACE: Duration = Duration::from_millis(399);

#[derive(Default)]
struct MenuMemory {
    titles: HashMap<String, Vec<String>>,
    grace: Option<Grace>,
}

#[derive(Clone, Debug)]
struct Grace {
    app_id: String,
    deadline: Instant,
    showing_titles: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum MenuChoice {
    Remembered(Vec<String>),
    Waiting,
    Fallback,
}

struct Decision {
    choice: MenuChoice,
    started: Option<Instant>,
}

impl MenuMemory {
    fn remember(&mut self, app_id: &str, labels: Vec<String>) -> bool {
        let changed = self.titles.get(app_id) != Some(&labels);
        self.titles.insert(app_id.to_string(), labels);
        if self
            .grace
            .as_ref()
            .is_some_and(|grace| grace.app_id == app_id)
        {
            self.grace = None;
        }
        changed
    }

    fn forget(&mut self, app_id: &str) {
        self.titles.remove(app_id);
        if self
            .grace
            .as_ref()
            .is_some_and(|grace| grace.app_id == app_id)
        {
            self.grace = None;
        }
    }

    fn decide(
        &mut self,
        app_id: &str,
        address: Option<&AppMenuAddress>,
        newly_mapped: bool,
        now: Instant,
    ) -> Decision {
        if let Some(grace) = &self.grace
            && grace.app_id == app_id
        {
            if now < grace.deadline {
                let choice = if grace.showing_titles {
                    MenuChoice::Remembered(self.titles.get(app_id).cloned().unwrap_or_default())
                } else {
                    MenuChoice::Waiting
                };
                return Decision {
                    choice,
                    started: None,
                };
            }
            let showing_titles = grace.showing_titles;
            self.grace = None;
            if showing_titles && address.is_none() {
                self.titles.remove(app_id);
            }
        }
        if let Some(titles) = self.titles.get(app_id) {
            if address.is_none() {
                let deadline = now + MENU_GRACE;
                self.grace = Some(Grace {
                    app_id: app_id.to_string(),
                    deadline,
                    showing_titles: true,
                });
                return Decision {
                    choice: MenuChoice::Remembered(titles.clone()),
                    started: Some(deadline),
                };
            }
            return Decision {
                choice: MenuChoice::Remembered(titles.clone()),
                started: None,
            };
        }
        if address.is_none() {
            if newly_mapped {
                let deadline = now + MENU_GRACE;
                self.grace = Some(Grace {
                    app_id: app_id.to_string(),
                    deadline,
                    showing_titles: false,
                });
                return Decision {
                    choice: MenuChoice::Waiting,
                    started: Some(deadline),
                };
            }
            return Decision {
                choice: MenuChoice::Fallback,
                started: None,
            };
        }
        Decision {
            choice: MenuChoice::Waiting,
            started: None,
        }
    }
}

fn title_labels(items: &[MenuItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Submenu { label, .. } | MenuItem::Action { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteMenu {
    pub generation: u64,
    pub address: AppMenuAddress,
    pub items: Vec<MenuItem>,
}

#[derive(Clone, Debug, PartialEq)]
struct MenuKey {
    app_id: String,
    app_menu: Option<AppMenuAddress>,
    gtk_menus: Option<GtkMenusAddress>,
    enabled: bool,
}

impl MenuKey {
    fn address(&self) -> Option<AppMenuAddress> {
        self.app_menu
            .clone()
            .or_else(|| self.gtk_menus.as_ref().map(gtkmenus::address))
    }
}

pub struct BarApp {
    pub clock: State<String>,
    pub active_app: State<Option<String>>,
    pub app_menus: State<bool>,
    pub window_store: PathBuf,
    pub menubar_store: PathBuf,
    pub entries: Vec<DesktopEntry>,
    pub dbus: MenuClient,
    pub menu_snapshot: State<Option<Arc<RemoteMenu>>>,
}

impl App for BarApp {
    fn render(&self) -> impl IntoElement {
        let timers = Timers::get();
        let clock_state = self.clock;
        use_hook(move || schedule_next_minute(timers, clock_state));

        let mut active_app = self.active_app;
        let window_store = self.window_store.clone();
        let poll_store = window_store.clone();
        let published_app = use_watched(move || load_active_app(&poll_store));
        use_side_effect(move || {
            let id = published_app.read().clone();
            if *active_app.peek() != id {
                if let Some(id) = &id {
                    super::recents::record_app(id);
                }
                active_app.set(id);
            }
        });

        let mut app_menus = self.app_menus;
        let dbus = self.dbus.clone();
        let menubar_store = self.menubar_store.clone();
        let menus_enabled = use_watched(move || load_app_menus(&menubar_store));
        use_side_effect(move || {
            let enabled = *menus_enabled.read();
            if *app_menus.peek() != enabled {
                app_menus.set(enabled);
                dbus.set_app_menus(enabled);
            }
        });

        let generation = use_hook(|| Rc::new(Cell::new(0u64)));
        let open_started = use_hook(|| Rc::new(Cell::new(None::<std::time::Instant>)));
        let timers = Timers::get();
        let dbus = self.dbus.clone();
        let menu_snapshot = self.menu_snapshot;
        let generation_for_drain = generation.clone();
        let open_started_for_drain = open_started.clone();
        use_hook(move || {
            schedule_snapshot_drain(
                timers,
                dbus,
                menu_snapshot,
                generation_for_drain,
                open_started_for_drain,
            )
        });
        let resolved = barrier_kit::theme::resolved();

        let now = self.clock.read().clone();
        let active_app = self.active_app.read().clone();
        let app_menus_enabled = *self.app_menus.read();
        let last_app_menu = use_hook(|| Rc::new(RefCell::new(None::<MenuKey>)));
        let last_gtk_menus =
            use_hook(|| Rc::new(RefCell::new(None::<(String, Option<GtkMenusAddress>)>)));
        let memory = use_hook(|| Rc::new(RefCell::new(MenuMemory::default())));
        let previous_len = use_hook(|| Rc::new(Cell::new(0usize)));
        let pending_new = use_hook(|| Rc::new(Cell::new(None::<usize>)));
        let menu_tick = use_state(|| 0u64);
        let _ = *menu_tick.read();
        let toplevels = Toplevels::get().list.read().clone();
        let last_app = use_hook(|| Rc::new(RefCell::new(None::<String>)));
        let dialog_active = toplevels
            .iter()
            .any(|window| window.active && is_shell_dialog(&window.app_id));
        let active_app = if dialog_active {
            last_app.borrow().clone()
        } else {
            active_app.filter(|id| !is_shell_dialog(id))
        };
        let active_info = toplevels
            .iter()
            .find(|window| window.active && !is_shell_dialog(&window.app_id))
            .or_else(|| {
                active_app
                    .as_deref()
                    .and_then(|id| toplevels.iter().find(|window| window.app_id == id))
            });
        let app_id = active_info
            .map(|info| info.app_id.clone())
            .or_else(|| active_app.clone());
        last_app.borrow_mut().clone_from(&app_id);
        let focused = app_id.is_some();
        let app = resolve_app_name(app_id.as_deref(), &self.entries);
        let key = active_info.map(|info| MenuKey {
            app_id: info.app_id.clone(),
            app_menu: info.app_menu.clone(),
            gtk_menus: info.gtk_menus.clone(),
            enabled: app_menus_enabled,
        });
        {
            let mut snapshot_state = self.menu_snapshot;
            let mut last = last_app_menu.borrow_mut();
            if *last != key {
                let next_generation = generation.get() + 1;
                generation.set(next_generation);
                snapshot_state.set(None);
                if let (Some(old), Some(new)) = (&*last, &key)
                    && old.app_id == new.app_id
                    && old.address().is_some()
                    && new.address().is_none()
                {
                    memory.borrow_mut().forget(&new.app_id);
                }
                match &key {
                    Some(key) if !key.enabled => {
                        tracing::info!(app_id = %key.app_id, "menubar: app_menus disabled");
                        self.dbus.unfocus();
                    }
                    Some(MenuKey {
                        app_id,
                        app_menu: Some(menu),
                        ..
                    }) => {
                        tracing::info!(
                            app_id = %app_id,
                            service = %menu.service,
                            path = %menu.path,
                            generation = next_generation,
                            "menubar: app_menu"
                        );
                        self.dbus.focus(menu.clone(), next_generation);
                    }
                    Some(MenuKey {
                        app_id,
                        gtk_menus: Some(menus),
                        ..
                    }) => {
                        tracing::info!(
                            app_id = %app_id,
                            generation = next_generation,
                            "menubar: org.gtk.Menus focus"
                        );
                        let menu_app = resolve_app_name(Some(app_id.as_str()), &self.entries);
                        self.dbus
                            .gtk_focus(menus.clone(), menu_app, next_generation);
                    }
                    Some(key) => {
                        tracing::info!(app_id = %key.app_id, "menubar: app_menu none");
                        self.dbus.unfocus();
                    }
                    None => {
                        tracing::info!(
                            active_app = ?active_app,
                            toplevels = toplevels.len(),
                            active = toplevels.iter().filter(|window| window.active).count(),
                            "menubar: no active window, Escritorio menu"
                        );
                        self.dbus.unfocus();
                    }
                }
                *last = key;
            }
        }
        {
            let mut last = last_gtk_menus.borrow_mut();
            let key = active_info.map(|info| (info.app_id.clone(), info.gtk_menus.clone()));
            if *last != key {
                match &key {
                    Some((app_id, Some(menu))) => {
                        tracing::info!(
                            app_id = %app_id,
                            bus = %menu.unique_bus_name,
                            menubar = %menu.menubar_path,
                            appmenu = %menu.app_menu_path,
                            "menubar: gtk_menus"
                        );
                    }
                    Some((app_id, None)) => {
                        tracing::info!(app_id = %app_id, "menubar: gtk_menus none");
                    }
                    None => {}
                }
                *last = key;
            }
        }
        let remote_menu = self.menu_snapshot.read().as_ref().cloned();
        let menu_ready = remote_menu
            .as_ref()
            .is_some_and(|menu| !menu.items.is_empty());
        let choice = if focused && app_menus_enabled {
            let active_index = toplevels
                .iter()
                .position(|window| window.active)
                .or_else(|| {
                    app_id
                        .as_deref()
                        .and_then(|id| toplevels.iter().position(|window| window.app_id == id))
                });
            let previous = previous_len.get();
            let mut pending = pending_new.get();
            if toplevels.len() > previous {
                pending = Some(pending.map_or(previous, |from| from.min(previous)));
            }
            if pending.is_some_and(|from| toplevels.len() < from) {
                pending = None;
            }
            let newly_mapped =
                active_index.is_some_and(|index| pending.is_some_and(|from| index >= from));
            if newly_mapped {
                pending = None;
            }
            pending_new.set(pending);
            previous_len.set(toplevels.len());

            let app_id = app_id.clone().unwrap_or_default();
            let mut stored = memory.borrow_mut();
            if menu_ready && let Some(menu) = &remote_menu {
                stored.remember(&app_id, title_labels(&menu.items));
            }
            let decision = if menu_ready {
                Decision {
                    choice: MenuChoice::Fallback,
                    started: None,
                }
            } else {
                let address = active_info.and_then(|info| {
                    info.app_menu
                        .clone()
                        .or_else(|| info.gtk_menus.as_ref().map(gtkmenus::address))
                });
                stored.decide(&app_id, address.as_ref(), newly_mapped, Instant::now())
            };
            drop(stored);
            if let Some(deadline) = decision.started {
                let tick = menu_tick;
                let delay =
                    deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1);
                Timers::get().after(delay, move || {
                    let mut tick = tick;
                    let next = *tick.peek() + 1;
                    tick.set(next);
                });
            }
            Some(decision.choice)
        } else {
            None
        };
        let palette = barrier_kit::theme::shell_palette(&resolved);
        let over_focus = Toplevels::get().space.read().focus_bar.over_focus();
        let text = if over_focus {
            theme::color(vow_design::FOCUS_BAR_TEXT)
        } else {
            theme::color(palette.text)
        };
        let bar = if over_focus {
            theme::color(vow_design::FOCUS_BAR_BACKGROUND)
        } else {
            theme::color(barrier_kit::theme::surface_background(
                &resolved,
                barrier_kit::theme::SurfaceKind::LayerShell,
            ))
        };

        let mut root = rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(bar)
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .padding(Gaps::new(
                0.0,
                MENUBAR_MARGIN_RIGHT as f32,
                2.0 * MENUBAR_TEXT_LIFT,
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
        let system_entries = Rc::new(self.entries.clone());
        let system_items = {
            let entries = system_entries.clone();
            move || system::system_menu_items(&entries)
        };
        let mut specs = vec![MenuTitleSpec {
            glyph: Some(system::SNOWFLAKE_SVG),
            ..MenuTitleSpec::new(
                "Sistema".to_string(),
                Rc::new(system_items),
                Rc::new(move |id| {
                    let toplevels = Toplevels::get().list.read().clone();
                    system::on_system_action(id, &toplevels, &system_entries);
                }),
                title_anchor(0),
            )
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
            if let Some(remote_menu) = remote_menu.filter(|menu| !menu.items.is_empty()) {
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
                    let open_started = open_started.clone();
                    Rc::new(move |id| {
                        if let Ok(id) = id.parse::<i32>() {
                            open_started.set(Some(std::time::Instant::now()));
                            dbus.open(address.clone(), id, generation_now);
                        }
                    })
                };
                let submenu_source: barrier_kit::menu::SubmenuSource = {
                    let snapshot = self.menu_snapshot;
                    Rc::new(move |id| {
                        let id = id.parse::<i32>().ok()?;
                        let menu = snapshot.read().as_ref().cloned()?;
                        remote::find_subtree(&menu.items, id)
                    })
                };
                let titles = composition();
                let reveal_roots: barrier_kit::menu::RevealRoots = {
                    let composition = composition.clone();
                    Rc::new(move || {
                        composition()
                            .iter()
                            .map(remote::Title::visible_items)
                            .collect()
                    })
                };
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
                    let is_help = index + 1 == titles.len();
                    let request_open: Option<Rc<dyn Fn()>> = if is_help || index == 0 {
                        let dbus = self.dbus.clone();
                        let address = address.clone();
                        let callback: Rc<dyn Fn()> = Rc::new(move || {
                            dbus.full_tree(address.clone(), generation_now);
                        });
                        Some(callback)
                    } else {
                        title.remote_id.map(|id| {
                            let dbus = self.dbus.clone();
                            let address = address.clone();
                            let callback: Rc<dyn Fn()> = Rc::new(move || {
                                dbus.open(address.clone(), id, generation_now);
                            });
                            callback
                        })
                    };
                    let on_open: Option<Rc<dyn Fn()>> = request_open.as_ref().map(|request| {
                        let request = request.clone();
                        let open_started = open_started.clone();
                        let callback: Rc<dyn Fn()> = Rc::new(move || {
                            open_started.set(Some(std::time::Instant::now()));
                            request();
                        });
                        callback
                    });
                    let live_items = Some(items.clone());
                    let on_hover_title = request_open;
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
                        bold: title.bold,
                        searchable,
                        live_items,
                        on_open,
                        on_hover_title,
                        on_open_submenu: Some(on_open_submenu.clone()),
                        submenu_source: Some(submenu_source.clone()),
                        enabled: title.enabled,
                        query_state: is_help.then(|| help_query.clone()),
                        reveal_roots: is_help.then(|| reveal_roots.clone()),
                        ..MenuTitleSpec::new(
                            title.label.clone(),
                            items,
                            on_activate.clone(),
                            title_anchor(index + 1),
                        )
                    });
                }
            } else if let Some(MenuChoice::Remembered(labels)) = choice.as_ref() {
                let sources = remote::MenuSources {
                    app_name: app_name.clone(),
                    app_items: app_items.clone(),
                    window_items: window_items.clone(),
                    help_items: help_items.clone(),
                };
                let remote_items: Vec<MenuItem> = labels
                    .iter()
                    .map(|label| MenuItem::Submenu {
                        id: Some("0".to_string()),
                        label: label.clone(),
                        enabled: true,
                        items: Vec::new(),
                        icon: None,
                    })
                    .collect();
                let titles = sources.compose(&remote_items);
                for (index, title) in titles.iter().enumerate() {
                    let items: MenuItems = {
                        let items = title.visible_items();
                        Rc::new(move || items.clone())
                    };
                    specs.push(MenuTitleSpec {
                        bold: title.bold,
                        enabled: title.enabled,
                        ..MenuTitleSpec::new(
                            title.label.clone(),
                            items,
                            Rc::new(focused::on_action),
                            title_anchor(index + 1),
                        )
                    });
                }
            } else if matches!(choice, Some(MenuChoice::Waiting)) {
                specs.push(MenuTitleSpec {
                    bold: true,
                    ..MenuTitleSpec::new(
                        app_name.clone(),
                        app_items.clone(),
                        Rc::new(focused::on_action),
                        title_anchor(1),
                    )
                });
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
                let reveal_roots: barrier_kit::menu::RevealRoots = {
                    let roots = Rc::new(vec![(app_items)(), (window_items)()]);
                    Rc::new(move || (*roots).clone())
                };
                let focused_titles = [
                    (
                        app_name.clone(),
                        app_items,
                        1usize,
                        None,
                        None::<Rc<RefCell<SearchQuery>>>,
                        None::<barrier_kit::menu::RevealRoots>,
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
                        bold: offset == 0,
                        searchable,
                        query_state,
                        reveal_roots,
                        ..MenuTitleSpec::new(
                            label,
                            items,
                            Rc::new(focused::on_action),
                            title_anchor(anchor),
                        )
                    });
                }
            }
        } else {
            let entries = Rc::new(self.entries.clone());
            let toplevels = Rc::new(Toplevels::get().list.read().clone());
            let desktop_menus = appmenu::desktop_menus(entries.clone(), toplevels.clone());
            let desktop_searchable: SearchableItems = {
                let entries = entries.clone();
                let toplevels = toplevels.clone();
                Rc::new(move || menu::SearchableLists {
                    menu: appmenu::searchable_menu_items(entries.clone(), toplevels.clone()),
                    help: focused::help_search_topics(),
                })
            };
            let desktop_reveal_roots: RevealRoots = {
                let menus = desktop_menus.clone();
                Rc::new(move || menus.iter().map(|(_, items)| items()).collect())
            };
            let labels: Vec<&str> = desktop_menus.iter().map(|(label, _)| *label).collect();
            let help_index = appmenu::desktop_help_title_index(&labels)
                .expect("Ayuda must be present in the desktop menus");
            for (offset, (label, items)) in desktop_menus.into_iter().enumerate() {
                let is_help = offset == help_index;
                specs.push(MenuTitleSpec {
                    bold: offset == 0,
                    searchable: is_help.then(|| desktop_searchable.clone()),
                    query_state: is_help.then(|| help_query.clone()),
                    reveal_roots: is_help.then(|| desktop_reveal_roots.clone()),
                    ..MenuTitleSpec::new(
                        label.to_string(),
                        items,
                        Rc::new(appmenu::on_action),
                        title_anchor(offset + 1),
                    )
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

fn schedule_snapshot_drain(
    timers: Timers,
    dbus: MenuClient,
    snapshot: State<Option<Arc<RemoteMenu>>>,
    generation: Rc<Cell<u64>>,
    open_started: Rc<Cell<Option<std::time::Instant>>>,
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
                            if let Some(started) = open_started.take() {
                                tracing::info!(
                                    id,
                                    cached = next.cached,
                                    available = next.items.is_some(),
                                    ms = started.elapsed().as_millis(),
                                    "menubar: title painted"
                                );
                            }
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

#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct MenubarConfig {
    #[serde(default = "app_menus_default")]
    app_menus: bool,
}

fn app_menus_default() -> bool {
    true
}

fn load_app_menus(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<MenubarConfig>(&text).ok())
        .map(|config| config.app_menus)
        .unwrap_or(true)
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

fn is_shell_dialog(app_id: &str) -> bool {
    app_id
        .strip_prefix(vow_design::NATIVE_APP_ID_PREFIX)
        .is_some_and(|name| {
            [
                crate::dialogs::ABOUT_APP_ID,
                crate::dialogs::ABOUT_APP_INFO_ID,
            ]
            .contains(&name)
        })
}

fn load_active_app(store_path: &PathBuf) -> Option<String> {
    let active = load_active_window(store_path).unwrap_or_default();
    let app_id = active.app_id.trim();
    (!app_id.is_empty()).then(|| app_id.to_string())
}

fn surface_spec() -> LayerSpec {
    LayerSpec {
        namespace: "malaysia-menubar".to_string(),
        layer: SurfaceLayer::Top,
        anchors: Anchors::TOP,
        exclusive_zone: MENUBAR_HEIGHT,
        keyboard: KeyboardInteractivity::None,
        size: Some((0, MENUBAR_HEIGHT as u32)),
    }
}

pub fn run() -> anyhow::Result<()> {
    let window_store = data_dir().join("window.domain");
    let menubar_store = config_dir().join("menubar.domain");
    let entries = crate::desktop::read_entries();
    let clock = State::create_global(chrono::Local::now().format("%H:%M").to_string());
    let active_app = State::create_global(load_active_app(&window_store));
    let app_menus = State::create_global(load_app_menus(&menubar_store));
    let menu_snapshot = State::create_global(None::<Arc<RemoteMenu>>);
    let dbus = dbusmenu::start(*app_menus.peek());
    let app = AppComponent::new(BarApp {
        clock,
        active_app,
        app_menus,
        window_store,
        menubar_store,
        entries,
        dbus,
        menu_snapshot,
    });

    let result = run_layer(app, surface_spec()).map_err(|error| anyhow::anyhow!("{error}"));
    let calls = dbusmenu::render_calls();
    debug_assert_eq!(calls, 0, "D-Bus calls on the render thread");
    tracing::info!(render_dbus_calls = calls, "menubar: closed");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_dialogs_are_recognised_by_their_prefixed_app_id() {
        let about = format!(
            "{}{}",
            vow_design::NATIVE_APP_ID_PREFIX,
            crate::dialogs::ABOUT_APP_ID
        );
        assert!(is_shell_dialog(&about));
        assert!(!is_shell_dialog(crate::dialogs::ABOUT_APP_ID));
        assert!(!is_shell_dialog("org.mozilla.firefox"));
    }

    #[test]
    fn menubar_domain_defaults_app_menus_to_true() {
        let dir = std::env::temp_dir().join(format!("malaysia-menubar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("menubar.domain");
        assert!(load_app_menus(&path));
        std::fs::write(&path, "app_menus = false\nautohide = true\n").unwrap();
        assert!(!load_app_menus(&path));
        std::fs::write(&path, "autohide = true\n").unwrap();
        assert!(load_app_menus(&path));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_bar_reserves_the_menubar_height() {
        let LayerSpec {
            layer,
            anchors,
            exclusive_zone,
            size,
            ..
        } = surface_spec();
        assert_eq!(layer, SurfaceLayer::Top);
        assert!(anchors.top && anchors.left && anchors.right);
        assert!(!anchors.bottom);
        assert_eq!(exclusive_zone, MENUBAR_HEIGHT);
        assert_eq!(size, Some((0, MENUBAR_HEIGHT as u32)));
    }

    #[test]
    fn remembered_titles_are_shown_then_replaced() {
        let mut memory = MenuMemory::default();
        let now = Instant::now();
        assert!(memory.remember("firefox", vec!["File".into(), "Edit".into()]));
        let shown = memory.decide("firefox", None, true, now);
        assert_eq!(
            shown.choice,
            MenuChoice::Remembered(vec!["File".into(), "Edit".into()])
        );
        assert!(shown.started.is_some());

        assert!(memory.remember("firefox", vec!["File".into(), "Edit".into(), "View".into()]));
        let replaced = memory.decide("firefox", None, false, now);
        assert_eq!(
            replaced.choice,
            MenuChoice::Remembered(vec!["File".into(), "Edit".into(), "View".into()])
        );
    }

    #[test]
    fn equal_titles_do_not_change_the_remembered_set() {
        let mut memory = MenuMemory::default();
        let labels = vec!["File".to_string(), "Edit".to_string()];
        assert!(memory.remember("firefox", labels.clone()));
        assert!(!memory.remember("firefox", labels.clone()));
        let decision = memory.decide("firefox", None, true, Instant::now());
        assert_eq!(decision.choice, MenuChoice::Remembered(labels));
    }

    #[test]
    fn grace_expiry_falls_back_and_drops_the_titles() {
        let mut memory = MenuMemory::default();
        let now = Instant::now();
        memory.remember("firefox", vec!["File".into()]);
        assert!(matches!(
            memory.decide("firefox", None, true, now).choice,
            MenuChoice::Remembered(_)
        ));
        let expired = memory.decide("firefox", None, false, now + MENU_GRACE);
        assert_eq!(expired.choice, MenuChoice::Fallback);
        assert!(!memory.titles.contains_key("firefox"));
    }

    #[test]
    fn an_empty_address_forgets_the_titles() {
        let mut memory = MenuMemory::default();
        memory.remember("firefox", vec!["File".into()]);
        memory.forget("firefox");
        assert!(!memory.titles.contains_key("firefox"));
        assert_eq!(
            memory.decide("firefox", None, false, Instant::now()).choice,
            MenuChoice::Fallback
        );
    }

    #[test]
    fn a_new_window_without_titles_waits_then_falls_back() {
        let mut memory = MenuMemory::default();
        let now = Instant::now();
        let waiting = memory.decide("foot", None, true, now);
        assert_eq!(waiting.choice, MenuChoice::Waiting);
        assert!(waiting.started.is_some());
        assert_eq!(
            memory.decide("foot", None, false, now + MENU_GRACE).choice,
            MenuChoice::Fallback
        );
    }

    #[test]
    fn an_already_mapped_window_falls_back_at_once() {
        let mut memory = MenuMemory::default();
        let decision = memory.decide("foot", None, false, Instant::now());
        assert_eq!(decision.choice, MenuChoice::Fallback);
        assert!(decision.started.is_none());
    }

    fn address() -> AppMenuAddress {
        AppMenuAddress {
            service: ":1.6".to_string(),
            path: "/com/canonical/menu/0".to_string(),
        }
    }

    #[test]
    fn an_address_keeps_remembered_titles_past_the_grace() {
        let mut memory = MenuMemory::default();
        let now = Instant::now();
        memory.remember("firefox", vec!["File".into()]);
        assert!(matches!(
            memory.decide("firefox", None, true, now).choice,
            MenuChoice::Remembered(_)
        ));
        let later = memory.decide("firefox", Some(&address()), false, now + MENU_GRACE);
        assert_eq!(later.choice, MenuChoice::Remembered(vec!["File".into()]));
        assert!(later.started.is_none());
        assert!(memory.titles.contains_key("firefox"));
    }

    #[test]
    fn an_address_without_a_snapshot_waits_instead_of_falling_back() {
        let mut memory = MenuMemory::default();
        let decision = memory.decide("firefox", Some(&address()), true, Instant::now());
        assert_eq!(decision.choice, MenuChoice::Waiting);
        assert!(decision.started.is_none());
    }
}
