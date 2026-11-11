use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bk::app::{load_defaults, use_defaults};
use bk::menu::{ItemsSource, MenuItem};
use bk::platform::Timers;
use bk::prelude::*;
use bk::shell::{
    Anchors, GtkMenusAddress, KeyboardInteractivity, Layer as SurfaceLayer, LayerSpec,
    ToplevelInfo, Toplevels, run_layer, use_status_items,
};
use ctappearance::MenuBarSize;
use ctappearance::menubar::MenubarConfig;
use ctdesign::{
    MENUBAR_FONT_SIZE, MENUBAR_FONT_SIZE_LARGE, MENUBAR_HEIGHT, MENUBAR_MARGIN,
    MENUBAR_MARGIN_RIGHT, MENUBAR_TEXT_LIFT, SANS_FAMILIES,
};
use freya_core::integration::AppComponent;
use torin::prelude::Gaps;

use crate::desktop::DesktopEntry;
use crate::localize::tr;
use crate::menubar::dbusmenu::{self, MenuClient};
use crate::menubar::dbusmenu_map;
use crate::menubar::gtkmenus;
use crate::menubar::menu::{self, MenuState, MenuTitleSpec, SearchableItems};
use crate::menubar::remote;
use crate::menubar::{
    appmenu, clock, extras, focused, live, lock_row, resolve_app_name, shortcuts, status, system,
};
use bk::menu::{RevealRoots, SearchQuery};
use bk::platform::MainMenuAddress;
use bk::theme;
use ctappearance::notifications::Focus;

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
    fn remember(&mut self, app_id: &str, labels: Vec<String>) {
        self.titles.insert(app_id.to_string(), labels);
        self.grace.take_if(|grace| grace.app_id == app_id);
    }

    fn forget(&mut self, app_id: &str) {
        self.titles.remove(app_id);
        self.grace.take_if(|grace| grace.app_id == app_id);
    }

    fn decide(
        &mut self,
        app_id: &str,
        address: Option<&MainMenuAddress>,
        newly_mapped: bool,
        now: Instant,
    ) -> Decision {
        if let Some(grace) = self.grace.take_if(|grace| grace.app_id == app_id) {
            if now < grace.deadline {
                let choice = if grace.showing_titles {
                    MenuChoice::Remembered(self.titles.get(app_id).cloned().unwrap_or_default())
                } else {
                    MenuChoice::Waiting
                };
                self.grace = Some(grace);
                return Decision {
                    choice,
                    started: None,
                };
            }
            if grace.showing_titles && address.is_none() {
                self.titles.remove(app_id);
            }
        }
        let (choice, showing_titles) = match (self.titles.get(app_id), address) {
            (Some(titles), None) => (MenuChoice::Remembered(titles.clone()), Some(true)),
            (Some(titles), Some(_)) => (MenuChoice::Remembered(titles.clone()), None),
            (None, None) if newly_mapped => (MenuChoice::Waiting, Some(false)),
            (None, None) => (MenuChoice::Fallback, None),
            (None, Some(_)) => (MenuChoice::Waiting, None),
        };
        let started = showing_titles.map(|showing_titles| {
            let deadline = now + MENU_GRACE;
            self.grace = Some(Grace {
                app_id: app_id.to_string(),
                deadline,
                showing_titles,
            });
            deadline
        });
        Decision { choice, started }
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
    pub address: MainMenuAddress,
    pub items: Vec<MenuItem>,
}

#[derive(Clone, Debug, PartialEq)]
struct MenuKey {
    app_id: String,
    app_menu: Option<MainMenuAddress>,
    gtk_menus: Option<GtkMenusAddress>,
    enabled: bool,
}

impl MenuKey {
    fn address(&self) -> Option<MainMenuAddress> {
        self.app_menu
            .clone()
            .or_else(|| self.gtk_menus.as_ref().map(gtkmenus::address))
    }
}

pub struct BarApp {
    pub clock: State<String>,
    pub app_menus: State<bool>,
    pub entries: Rc<Vec<DesktopEntry>>,
    pub dbus: MenuClient,
    pub menu_snapshot: State<Option<Arc<RemoteMenu>>>,
}

impl App for BarApp {
    fn render(&self) -> impl IntoElement {
        let timers = Timers::get();
        let clock_state = self.clock;
        use_hook(move || schedule_next_minute(timers, clock_state));

        self.use_app_menus_setting();

        let (generation, open_started) = self.use_snapshot_drain();
        let (status_items, status_host) = use_status_items();
        let status_anchors = use_hook(status::StatusAnchors::default);
        let extras = extras::use_extras();
        let menubar_config = use_defaults::<MenubarConfig>("menubar");

        let now = self.clock.read().clone();
        let focus = use_focus();
        let activities = live::use_activities();
        let app_menus_enabled = *self.app_menus.read();
        let last_app_menu = use_hook(|| Rc::new(RefCell::new(None::<MenuKey>)));
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
        let active_app = dialog_active.then(|| last_app.borrow().clone()).flatten();
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
        if *last_app.borrow() != app_id
            && let Some(id) = &app_id
        {
            super::recents::record_app(id);
        }
        last_app.borrow_mut().clone_from(&app_id);
        let focused = app_id.is_some();
        let key = active_info.map(|info| MenuKey {
            app_id: info.app_id.clone(),
            app_menu: info.main_menu.clone(),
            gtk_menus: info.gtk_menus.clone(),
            enabled: app_menus_enabled,
        });
        let address = key.as_ref().and_then(MenuKey::address);
        {
            let mut last = last_app_menu.borrow_mut();
            if *last != key {
                let next_generation = generation.get() + 1;
                generation.set(next_generation);
                let mut snapshot_state = self.menu_snapshot;
                snapshot_state.set(None);
                if let (Some(old), Some(new)) = (&*last, &key)
                    && old.app_id == new.app_id
                    && old.address().is_some()
                    && new.address().is_none()
                {
                    memory.borrow_mut().forget(&new.app_id);
                }
                self.refocus(key.as_ref(), next_generation, &active_app, &toplevels);
                *last = key;
            }
        }
        let remote_menu = self
            .menu_snapshot
            .read()
            .as_ref()
            .filter(|menu| !menu.items.is_empty())
            .cloned();
        let choice = if focused && app_menus_enabled {
            let active_index = toplevels
                .iter()
                .position(|window| window.active)
                .or_else(|| {
                    app_id
                        .as_deref()
                        .and_then(|id| toplevels.iter().position(|window| window.app_id == id))
                });
            let newly_mapped =
                newly_mapped(&previous_len, &pending_new, toplevels.len(), active_index);
            let app_id = app_id.clone().unwrap_or_default();
            let mut stored = memory.borrow_mut();
            if let Some(menu) = &remote_menu {
                stored.remember(&app_id, title_labels(&menu.items));
                None
            } else {
                let decision =
                    stored.decide(&app_id, address.as_ref(), newly_mapped, Instant::now());
                if let Some(deadline) = decision.started {
                    tick_at(menu_tick, deadline);
                }
                Some(decision.choice)
            }
        } else {
            None
        };
        let progress = lock_row::use_lock_progress();
        let (cover, settle) = progress
            .as_ref()
            .map_or((0.0, 0.0), |progress| (progress.cover, progress.settle));
        let shown_desktop = 1.0 - cover;
        let (text, bar) = bar_colors(shown_desktop);

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
        let mut specs = vec![system_title(self.entries.clone(), title_anchor(0))];
        if focused {
            let app_id = app_id.unwrap_or_default();
            let sources = menu_sources(
                resolve_app_name(Some(&app_id), &self.entries),
                app_id.clone(),
            );
            if let Some(remote_menu) = remote_menu {
                specs.extend(self.remote_title_specs(
                    &remote_menu,
                    sources,
                    &open_started,
                    &help_query,
                    title_anchor,
                ));
            } else {
                specs.extend(local_title_specs(
                    sources,
                    shortcuts::menus(&app_id, &self.entries),
                    choice,
                    &help_query,
                    title_anchor,
                ));
            }
        } else {
            specs.extend(title_specs(
                appmenu::desktop_menus(&self.entries, &toplevels),
                Rc::new(appmenu::on_action),
                Some(help_query.clone()),
                title_anchor,
            ));
        }
        let titles = Rc::new(specs);
        let mut left = HStack::new()
            .cross_align(Alignment::Start)
            .spacing(0.0)
            .main_align(Alignment::Start)
            .opacity(shown_desktop);
        for index in 0..titles.len() {
            left = left.child(menu::menu_title(
                index,
                titles.clone(),
                menu_state.clone(),
                text,
            ));
        }

        let shown = status_items.read().clone();
        let status_specs = Rc::new(status::status_titles(&shown, &status_host, &status_anchors));
        let mut right = HStack::new()
            .main_align(Alignment::Start)
            .cross_align(Alignment::Start)
            .spacing(0.0)
            .height(Size::fill());
        let mut others = HStack::new()
            .cross_align(Alignment::Start)
            .spacing(0.0)
            .height(Size::fill())
            .opacity(shown_desktop);
        for (index, item) in shown.iter().enumerate() {
            others = others.child(if item.menu.is_empty() {
                status::activate_only(item, &status_host)
            } else {
                menu::menu_title(index, status_specs.clone(), menu_state.clone(), text)
            });
        }

        let extras_ink = bar_ink().mix(ctdesign::LOCK_INK, cover);
        let extra_specs = Rc::new(extras::extra_titles(
            &extras,
            &menubar_config.read(),
            extras_ink,
            &status_anchors,
        ));
        let mut tail_width = use_state(|| 0.0f32);
        let mut kept = HStack::new()
            .cross_align(Alignment::Start)
            .spacing(0.0)
            .height(Size::fill())
            .offset_x((*tail_width.read() - lock_row::POWER_SLOT) * settle);
        for index in 0..extra_specs.len() {
            kept = kept.child(menu::menu_title(
                index,
                extra_specs.clone(),
                menu_state.clone(),
                theme::color(extras_ink),
            ));
        }

        let tail = HStack::new()
            .height(Size::fill())
            .cross_align(Alignment::Start)
            .spacing(0.0)
            .opacity(shown_desktop)
            .on_sized(move |event: Event<SizedEventData>| {
                tail_width.set_if_modified(event.data().area.width());
            })
            .child(live::live_area(activities, text))
            .child(clock::clock_label(now, text, focus));
        let bar_view = bar_root(bar)
            .child(left)
            .child(right.child(others).child(kept).child(tail));
        let locking = progress.is_some();
        let closing_state = menu_state.clone();
        use_side_effect_with_deps(&locking, move |locking| {
            if *locking {
                menu::close_root_menu(&closing_state);
            }
        });
        bar_view.into_element()
    }
}

impl BarApp {
    fn use_snapshot_drain(&self) -> (Rc<Cell<u64>>, Rc<Cell<Option<Instant>>>) {
        let generation = use_hook(|| Rc::new(Cell::new(0u64)));
        let open_started = use_hook(|| Rc::new(Cell::new(None::<Instant>)));
        let timers = Timers::get();
        let dbus = self.dbus.clone();
        let menu_snapshot = self.menu_snapshot;
        let (drain_generation, drain_open_started) = (generation.clone(), open_started.clone());
        use_hook(move || {
            schedule_snapshot_drain(
                timers,
                dbus,
                menu_snapshot,
                drain_generation,
                drain_open_started,
            )
        });
        (generation, open_started)
    }

    fn use_app_menus_setting(&self) {
        let mut app_menus = self.app_menus;
        let dbus = self.dbus.clone();
        let config = use_defaults::<MenubarConfig>("menubar");
        use_side_effect(move || {
            let enabled = config.read().app_menus;
            if *app_menus.peek() != enabled {
                app_menus.set(enabled);
                dbus.set_app_menus(enabled);
            }
        });
    }

    fn refocus(
        &self,
        key: Option<&MenuKey>,
        generation: u64,
        active_app: &Option<String>,
        toplevels: &[ToplevelInfo],
    ) {
        match key {
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
                    generation,
                    "menubar: app_menu"
                );
                self.dbus.focus(menu.clone(), generation);
            }
            Some(MenuKey {
                app_id,
                gtk_menus: Some(menus),
                ..
            }) => {
                tracing::info!(app_id = %app_id, generation, "menubar: org.gtk.Menus focus");
                let menu_app = resolve_app_name(Some(app_id.as_str()), &self.entries);
                self.dbus.gtk_focus(menus.clone(), menu_app, generation);
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
    }

    fn remote_title_specs(
        &self,
        remote_menu: &RemoteMenu,
        sources: remote::MenuSources,
        open_started: &Rc<Cell<Option<Instant>>>,
        help_query: &Rc<RefCell<SearchQuery>>,
        title_anchor: impl Fn(usize) -> TitleAnchor,
    ) -> Vec<MenuTitleSpec> {
        let snapshot = self.menu_snapshot;
        let address = remote_menu.address.clone();
        let generation = remote_menu.generation;
        let remote_items: Rc<dyn Fn() -> Vec<MenuItem>> = {
            let address = address.clone();
            let rendered = remote_menu.items.clone();
            Rc::new(move || {
                snapshot
                    .read()
                    .as_ref()
                    .filter(|menu| menu.generation == generation && menu.address == address)
                    .map_or_else(|| rendered.clone(), |menu| menu.items.clone())
            })
        };
        let composition: Rc<dyn Fn() -> Vec<remote::Title>> = {
            let remote_items = remote_items.clone();
            Rc::new(move || sources.compose(&remote_items()))
        };
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
                    open_started.set(Some(Instant::now()));
                    dbus.open(address.clone(), id, generation);
                }
            })
        };
        let submenu_source: bk::menu::SubmenuSource = Rc::new(move |id| {
            let id = id.parse::<i32>().ok()?;
            remote::find_subtree(&remote_items(), id)
        });
        let reveal_roots: RevealRoots = {
            let composition = composition.clone();
            Rc::new(move || {
                composition()
                    .iter()
                    .map(remote::Title::visible_items)
                    .collect()
            })
        };
        let titles = composition();
        titles
            .iter()
            .enumerate()
            .map(|(index, title)| {
                let items: ItemsSource = {
                    let composition = composition.clone();
                    let label = title.label.clone();
                    Rc::new(move || {
                        composition()
                            .iter()
                            .find(|title| title.label == label)
                            .map(remote::Title::visible_items)
                            .unwrap_or_default()
                    })
                };
                let is_help = index + 1 == titles.len();
                let opened_id = if is_help || index == 0 {
                    Some(None)
                } else {
                    title.remote_id.map(Some)
                };
                let request_open = opened_id.map(|id| {
                    let dbus = self.dbus.clone();
                    let address = address.clone();
                    Rc::new(move || match id {
                        Some(id) => dbus.open(address.clone(), id, generation),
                        None => dbus.full_tree(address.clone(), generation),
                    }) as Rc<dyn Fn()>
                });
                let on_open = request_open.clone().map(|request| {
                    let open_started = open_started.clone();
                    Rc::new(move || {
                        open_started.set(Some(Instant::now()));
                        request();
                    }) as Rc<dyn Fn()>
                });
                let searchable: Option<SearchableItems> = is_help.then(|| {
                    let composition = composition.clone();
                    Rc::new(move || menu::SearchableLists {
                        menu: remote::searchable(&composition()),
                        help: focused::help_search_topics(),
                    }) as SearchableItems
                });
                MenuTitleSpec {
                    bold: index == 0,
                    searchable,
                    live_items: Some(items.clone()),
                    on_open,
                    on_hover_title: request_open,
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
                }
            })
            .collect()
    }
}

pub(crate) fn bar_root(background: Color) -> HStack {
    let mut root = HStack::new()
        .cross_align(Alignment::Start)
        .spacing(0.0)
        .width(Size::fill())
        .height(Size::fill())
        .background(background)
        .main_align(Alignment::SpaceBetween)
        .padding(Gaps::new(
            0.0,
            MENUBAR_MARGIN_RIGHT as f32,
            2.0 * MENUBAR_TEXT_LIFT,
            MENUBAR_MARGIN as f32,
        ))
        .font_size(match theme::resolved().menu_bar_size {
            MenuBarSize::Default => MENUBAR_FONT_SIZE,
            MenuBarSize::Large => MENUBAR_FONT_SIZE_LARGE,
        });
    for family in SANS_FAMILIES {
        root = root.font_family(family);
    }
    root
}

fn system_title(entries: Rc<Vec<DesktopEntry>>, anchor: TitleAnchor) -> MenuTitleSpec {
    let items = {
        let entries = entries.clone();
        move || system::system_menu_items(&entries)
    };
    MenuTitleSpec {
        glyph: Some(system::SNOWFLAKE_SVG),
        ..MenuTitleSpec::new(
            tr!("menubar-system").to_string(),
            Rc::new(items),
            Rc::new(move |id| {
                let toplevels = Toplevels::get().list.read().clone();
                system::on_system_action(id, &toplevels, &entries);
            }),
            anchor,
        )
    }
}

fn local_title_specs(
    sources: remote::MenuSources,
    shortcut_menus: Vec<MenuItem>,
    choice: Option<MenuChoice>,
    help_query: &Rc<RefCell<SearchQuery>>,
    title_anchor: impl Fn(usize) -> TitleAnchor,
) -> Vec<MenuTitleSpec> {
    let labels = match choice {
        Some(MenuChoice::Waiting) => {
            return vec![MenuTitleSpec {
                bold: true,
                ..MenuTitleSpec::new(
                    sources.app_name,
                    sources.app_items,
                    Rc::new(focused::on_action),
                    title_anchor(1),
                )
            }];
        }
        Some(MenuChoice::Remembered(labels)) => labels,
        _ => Vec::new(),
    };
    let remembered: Vec<MenuItem> = labels
        .into_iter()
        .map(|label| MenuItem::Submenu {
            id: Some("0".to_string()),
            label,
            enabled: true,
            items: Vec::new(),
            icon: None,
        })
        .collect();
    let help_query = remembered.is_empty().then(|| help_query.clone());
    let app_menus = if remembered.is_empty() {
        shortcut_menus
    } else {
        remembered
    };
    title_specs(
        sources.compose(&app_menus),
        Rc::new(focused::on_action),
        help_query,
        title_anchor,
    )
}

fn menu_sources(app_name: String, app_id: String) -> remote::MenuSources {
    let app_items: ItemsSource = {
        let (app_name, app_id) = (app_name.clone(), app_id.clone());
        Rc::new(move || focused::app_menu_items(&app_name, &app_id, &Toplevels::get().list.read()))
    };
    let window_items: ItemsSource = {
        let app_name = app_name.clone();
        Rc::new(move || {
            focused::window_menu_items(
                &app_name,
                &app_id,
                &Toplevels::get().list.read(),
                Toplevels::get().focus_bar.peek().over_focus(),
            )
        })
    };
    let help_items: ItemsSource = {
        let app_name = app_name.clone();
        Rc::new(move || focused::help_menu_items(&app_name))
    };
    remote::MenuSources {
        app_name,
        app_items,
        window_items,
        help_items,
    }
}

fn newly_mapped(
    previous_len: &Cell<usize>,
    pending_new: &Cell<Option<usize>>,
    len: usize,
    active_index: Option<usize>,
) -> bool {
    let previous = previous_len.replace(len);
    let mut pending = pending_new.get();
    if len > previous {
        pending = Some(pending.map_or(previous, |from| from.min(previous)));
    }
    if pending.is_some_and(|from| len < from) {
        pending = None;
    }
    let newly_mapped = active_index.is_some_and(|index| pending.is_some_and(|from| index >= from));
    pending_new.set(pending.filter(|_| !newly_mapped));
    newly_mapped
}

fn tick_at(tick: State<u64>, deadline: Instant) {
    let delay = deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1);
    Timers::get().after(delay, move || {
        let mut tick = tick;
        let next = *tick.peek() + 1;
        tick.set(next);
    });
}

fn bar_colors(shown_desktop: f32) -> (Color, Color) {
    let resolved = theme::resolved();
    let background = if Toplevels::get().focus_bar.read().over_focus() {
        ctdesign::FOCUS_BAR_BACKGROUND
    } else {
        theme::surface_background(&resolved, theme::SurfaceKind::LayerShell)
    };
    let background = Rgba {
        a: background.a * shown_desktop,
        ..background
    };
    (theme::color(bar_ink()), theme::color(background))
}

fn bar_ink() -> Rgba {
    if Toplevels::get().focus_bar.read().over_focus() {
        ctdesign::FOCUS_BAR_TEXT
    } else {
        theme::resolved().shell.text
    }
}

fn title_specs(
    titles: Vec<remote::Title>,
    on_activate: Rc<dyn Fn(String)>,
    help_query: Option<Rc<RefCell<SearchQuery>>>,
    title_anchor: impl Fn(usize) -> TitleAnchor,
) -> Vec<MenuTitleSpec> {
    let titles = Rc::new(titles);
    let help_index = titles.len() - 1;
    let searchable: SearchableItems = {
        let titles = titles.clone();
        Rc::new(move || menu::SearchableLists {
            menu: remote::searchable(&titles),
            help: focused::help_search_topics(),
        })
    };
    let reveal_roots: RevealRoots = {
        let titles = titles.clone();
        Rc::new(move || titles.iter().map(remote::Title::visible_items).collect())
    };
    titles
        .iter()
        .enumerate()
        .map(|(index, title)| {
            let items = title.visible_items();
            let query_state = help_query.clone().filter(|_| index == help_index);
            MenuTitleSpec {
                bold: index == 0,
                enabled: title.enabled,
                searchable: query_state.is_some().then(|| searchable.clone()),
                reveal_roots: query_state.is_some().then(|| reveal_roots.clone()),
                query_state,
                ..MenuTitleSpec::new(
                    title.label.clone(),
                    Rc::new(move || items.clone()),
                    on_activate.clone(),
                    title_anchor(index + 1),
                )
            }
        })
        .collect()
}

fn schedule_snapshot_drain(
    timers: Timers,
    dbus: MenuClient,
    snapshot: State<Option<Arc<RemoteMenu>>>,
    generation: Rc<Cell<u64>>,
    open_started: Rc<Cell<Option<Instant>>>,
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
                        let children = mapped.unwrap_or_else(|| {
                            vec![MenuItem::placeholder(tr!("menubar-unavailable"))]
                        });
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

fn use_focus() -> Option<Focus> {
    let state = use_state(|| None);
    use_hook(move || {
        let latest = Arc::new(Mutex::new(None));
        let shared = latest.clone();
        let wake = Timers::get().wakeup(move || {
            let mut state = state;
            state.set_if_modified(
                *shared
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            );
        });
        crate::notifications::watch_focus(move |focus| {
            *latest
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = focus;
            wake.wake();
        });
    });
    *state.read()
}

pub(crate) fn schedule_next_minute(timers: Timers, clock: State<String>) {
    let retry = timers.clone();
    let seconds = u64::from(60 - chrono::Timelike::second(&chrono::Local::now()));
    timers.after(Duration::from_secs(seconds), move || {
        let mut clock = clock;
        clock.set(chrono::Local::now().format("%H:%M").to_string());
        schedule_next_minute(retry.clone(), clock);
    });
}

fn is_shell_dialog(app_id: &str) -> bool {
    app_id
        .strip_prefix(ctdesign::NATIVE_APP_ID_PREFIX)
        .is_some_and(|name| {
            [
                crate::dialogs::ABOUT_APP_ID,
                crate::dialogs::ABOUT_APP_INFO_ID,
            ]
            .contains(&name)
        })
}

fn surface_spec() -> LayerSpec {
    LayerSpec {
        namespace: ctdesign::MENUBAR_LAYER_NAMESPACE.to_string(),
        layer: SurfaceLayer::Top,
        anchors: Anchors::TOP,
        exclusive_zone: MENUBAR_HEIGHT,
        keyboard: KeyboardInteractivity::None,
        size: Some((0, MENUBAR_HEIGHT as u32)),
    }
}

pub fn run() -> anyhow::Result<()> {
    let entries = Rc::new(crate::desktop::read_entries());
    let clock = State::create_global(chrono::Local::now().format("%H:%M").to_string());
    let app_menus = State::create_global(load_defaults::<MenubarConfig>("menubar").app_menus);
    let menu_snapshot = State::create_global(None::<Arc<RemoteMenu>>);
    let dbus = dbusmenu::start(*app_menus.peek());
    let app = AppComponent::new(BarApp {
        clock,
        app_menus,
        entries,
        dbus,
        menu_snapshot,
    });

    let result = run_layer(app, surface_spec()).map_err(|error| anyhow::anyhow!("{error}"));
    tracing::info!("menubar: closed");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_dialogs_are_recognised_by_their_prefixed_app_id() {
        let about = format!(
            "{}{}",
            ctdesign::NATIVE_APP_ID_PREFIX,
            crate::dialogs::ABOUT_APP_ID
        );
        assert!(is_shell_dialog(&about));
        assert!(!is_shell_dialog(crate::dialogs::ABOUT_APP_ID));
        assert!(!is_shell_dialog("org.mozilla.firefox"));
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
        memory.remember("firefox", vec!["File".into(), "Edit".into()]);
        let shown = memory.decide("firefox", None, true, now);
        assert_eq!(
            shown.choice,
            MenuChoice::Remembered(vec!["File".into(), "Edit".into()])
        );
        assert!(shown.started.is_some());

        memory.remember("firefox", vec!["File".into(), "Edit".into(), "View".into()]);
        let replaced = memory.decide("firefox", None, false, now);
        assert_eq!(
            replaced.choice,
            MenuChoice::Remembered(vec!["File".into(), "Edit".into(), "View".into()])
        );
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

    fn address() -> MainMenuAddress {
        MainMenuAddress {
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
