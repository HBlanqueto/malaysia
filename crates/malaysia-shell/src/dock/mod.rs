mod geometry;
mod items;
mod motion;
mod rim;
mod thumbnail;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    f32::consts::PI,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use bk::adornments::Badge;
use bk::animation::use_switch_progress;
use bk::app::{
    load_defaults, load_restoration, save_defaults, save_restoration, use_defaults, use_watched,
};
use bk::menu::{CheckState, MenuItem};
use bk::platform::{PopupAnchor, Timers, modifier_flags};
use bk::popup_button::pop_up_menu;
use bk::prelude::*;
use bk::shell::{
    Activation, Anchors, BlurRegion, CardFrames, InputRegion, KeyboardInteractivity, Layer,
    LayerGeometry, LayerSpec, MinimizeTarget, ToplevelInfo, Toplevels, capture_window, run_layer,
    set_blur_region, set_card_frames, set_input_region, set_layer_geometry, set_minimize_target,
};
use bk::styles::Animation;
use bk::theme;
use ctappearance::dock::{DockConfig, DockOrientation, MinimizeEffect};
use ctappearance::notifications::NotificationsConfig;
use ctdesign::{
    DOCK_BOUNCE_HEIGHT, DOCK_BOUNCE_MS, DOCK_DOT, DOCK_DOT_MAX, DOCK_DOT_MIN, DOCK_EDGE_MARGIN,
    DOCK_HIDE_MS, DOCK_ICON_BODY, DOCK_LABEL_FONT_SIZE, DOCK_LABEL_GAP, DOCK_LABEL_PADDING_X,
    DOCK_LABEL_PADDING_Y, DOCK_LABEL_RADIUS, DOCK_LAUNCH_TIMEOUT_MS, DOCK_MAGNIFY_MS,
    DOCK_PROGRESS_HEIGHT, DOCK_RADIUS, DOCK_REVEAL_STRIP, DOCK_SEPARATOR_LENGTH, DOCK_TILESIZE_MAX,
    DOCK_TILESIZE_MIN, DOCK_WINDOW_BADGE, DOCK_WINDOW_FRAME, DOCK_WINDOW_ICON, DRAG,
};
use freya_animation::prelude::{AnimNum, OnFinish, use_animation};
use freya_components::image_viewer::{DecodeMode, ImageViewer};
use freya_core::integration::AppComponent;
use freya_core::prelude::Modifiers;
use serde::{Deserialize, Serialize};
use torin::prelude::{Gaps, Position};

use crate::desktop::{self, DesktopEntry};
use crate::launcher::{ICON_SIZES, IconData, app_icon, forget_icons, launch, load_icon_in};
use crate::launcher_entry::{self, Entries, Entry};
use crate::localize::tr;
use crate::menubar::{find_entry, resolve_app_name};
use crate::spawn::{open_settings, spawn_component, xdg_open};
use geometry::Slot;
use items::{AppTile, Item};

const DOMAIN: &str = "dock";
const TRASH: &str = "trash:///";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct DockState {
    persistent_apps: Vec<String>,
    recent_apps: Vec<String>,
}

// `from` and `at` are (along the Dock, away from its edge): the surface grows while an icon is dragged.
#[derive(Clone, Debug, PartialEq)]
struct Drag {
    id: String,
    kept: bool,
    downloads: bool,
    from: (f32, f32),
    at: (f32, f32),
    moved: bool,
    away: bool,
    order: Vec<String>,
}

impl Drag {
    fn start(id: String, kept: bool, downloads: bool, at: (f32, f32), order: Vec<String>) -> Self {
        Self {
            id,
            kept,
            downloads,
            from: at,
            at,
            moved: false,
            away: false,
            order,
        }
    }

    fn lifts(&self, item: &Item) -> bool {
        match item {
            Item::App(app) => !self.downloads && app.id == self.id,
            Item::Downloads => self.downloads,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    orientation: DockOrientation,
    width: f32,
    height: f32,
}

type Area = (f32, f32, f32, f32);
type Regions = (Vec<(i32, i32, i32, i32)>, Vec<(i32, i32, i32, i32)>, u32);

impl Frame {
    fn vertical(self) -> bool {
        self.orientation != DockOrientation::Bottom
    }

    fn main_length(self) -> f32 {
        if self.vertical() {
            self.height
        } else {
            self.width
        }
    }

    fn canonical(self, (x, y): (f32, f32)) -> (f32, f32) {
        match self.orientation {
            DockOrientation::Bottom => (x, self.height - y),
            DockOrientation::Left => (y, x),
            DockOrientation::Right => (y, self.width - x),
        }
    }

    fn area(self, main: f32, cross: f32, main_length: f32, cross_length: f32) -> Area {
        match self.orientation {
            DockOrientation::Bottom => (
                main,
                self.height - cross - cross_length,
                main_length,
                cross_length,
            ),
            DockOrientation::Left => (cross, main, cross_length, main_length),
            DockOrientation::Right => (
                self.width - cross - cross_length,
                main,
                cross_length,
                main_length,
            ),
        }
    }
}

fn primary(event: &Event<PointerEventData>) -> bool {
    event.data().button() == Some(MouseButton::Left)
}

fn placed((x, y, width, height): Area) -> Rect {
    rect()
        .position(Position::new_absolute().left(x).top(y))
        .width(Size::px(width))
        .height(Size::px(height))
}

// macOS draws every icon on one 1024 grid with an 824 body. An app icon already sits on it: a Malaysia
// theme draws it so, and the kit puts any other on a plate. The places fill their canvas, so they shrink to the body.
fn on_grid(icon: Option<&IconData>, name: &str, size: f32) -> Element {
    let gridded = icon.is_some_and(|icon| icon.app);
    let drawn = if gridded { size } else { size * DOCK_ICON_BODY };
    rect()
        .width(Size::px(size))
        .height(Size::px(size))
        .center()
        .child(app_icon(icon, name, drawn))
        .into_element()
}

fn region((x, y, width, height): Area) -> (i32, i32, i32, i32) {
    (
        x.floor() as i32,
        y.floor() as i32,
        width.ceil() as i32,
        height.ceil() as i32,
    )
}

fn anchors(orientation: DockOrientation) -> Anchors {
    match orientation {
        DockOrientation::Bottom => Anchors::BOTTOM,
        DockOrientation::Left => Anchors {
            left: true,
            top: true,
            bottom: true,
            right: false,
        },
        DockOrientation::Right => Anchors {
            right: true,
            top: true,
            bottom: true,
            left: false,
        },
    }
}

// The compositor fits a layer to its output, so this asks for the whole screen.
const SCREEN: u32 = u16::MAX as u32;

// The surface leaves room above the bar for magnified and bouncing icons and the name label,
// and takes the screen while an icon is dragged, so the icon can leave the Dock.
fn layer_geometry(config: &DockConfig, thickness: f32, lifted: bool) -> LayerGeometry {
    let requested = config.tilesize();
    let headroom = (config.largesize() - requested).max(requested * DOCK_BOUNCE_HEIGHT);
    let label = 2.0 * (DOCK_LABEL_GAP + DOCK_LABEL_PADDING_Y) + DOCK_LABEL_FONT_SIZE;
    let extent = if lifted {
        SCREEN
    } else {
        (DOCK_EDGE_MARGIN + geometry::thickness(requested) + headroom + label).ceil() as u32
    };
    LayerGeometry {
        anchors: anchors(config.orientation),
        exclusive_zone: if config.autohide {
            0
        } else {
            (DOCK_EDGE_MARGIN + thickness).ceil() as i32
        },
        size: if config.orientation == DockOrientation::Bottom {
            (0, extent)
        } else {
            (extent, 0)
        },
        keyboard: KeyboardInteractivity::None,
    }
}

fn edit_config(change: impl FnOnce(&mut DockConfig)) {
    let mut config: DockConfig = load_defaults(DOMAIN);
    change(&mut config);
    if let Err(error) = save_defaults(DOMAIN, &config) {
        tracing::warn!(%error, "dock: could not save dock.domain");
    }
}

fn edit_state(mut state: State<DockState>, change: impl FnOnce(&mut DockState)) {
    let mut next = state.peek().clone();
    change(&mut next);
    if next == *state.peek() {
        return;
    }
    if let Err(error) = save_restoration(DOMAIN, &next) {
        tracing::warn!(%error, "dock: could not save dock.state");
    }
    state.set(next);
}

fn use_launcher_entries() -> Entries {
    let entries = use_hook(Entries::default);
    let tick = use_state(|| 0u64);
    use_hook({
        let entries = entries.clone();
        move || {
            let wake = Timers::get().wakeup(move || {
                let mut tick = tick;
                *tick.write() += 1;
            });
            launcher_entry::start(entries, wake);
        }
    });
    let _ = *tick.read();
    entries
}

#[derive(Clone, PartialEq)]
struct Thumbnail {
    size: (u32, u32),
    png: Bytes,
}

// macOS shows a minimized window as a picture of it; the picture is taken once, when it minimizes.
fn use_thumbnails(windows: &[ToplevelInfo]) -> State<HashMap<u64, Thumbnail>> {
    let mut thumbnails = use_state(HashMap::<u64, Thumbnail>::new);
    let minimized: Vec<u64> = windows
        .iter()
        .filter(|window| window.minimized)
        .map(|window| window.id)
        .collect();
    let asked = use_hook(|| Rc::new(RefCell::new(Vec::<u64>::new())));
    use_side_effect_with_deps(&minimized, move |minimized: &Vec<u64>| {
        asked.borrow_mut().retain(|id| minimized.contains(id));
        thumbnails.write().retain(|id, _| minimized.contains(id));
        for &id in minimized {
            if asked.borrow().contains(&id) {
                continue;
            }
            asked.borrow_mut().push(id);
            capture_window(id, move |image| {
                let Some(image) = image else {
                    return;
                };
                if let Some(png) = thumbnail::png(&image) {
                    let mut thumbnails = thumbnails;
                    thumbnails.write().insert(
                        id,
                        Thumbnail {
                            size: (image.width, image.height),
                            png: Bytes::from(png),
                        },
                    );
                }
            });
        }
    });
    thumbnails
}

// An app installed after the Dock started first shows up as a window whose app id no entry has.
fn use_desktop_entries(windows: &[ToplevelInfo]) -> Rc<Vec<DesktopEntry>> {
    let entries = use_hook(|| Rc::new(RefCell::new(Rc::new(desktop::read_entries()))));
    let unknown = use_hook(|| Rc::new(RefCell::new(Vec::<String>::new())));
    let now: Vec<String> = windows
        .iter()
        .map(|window| &window.app_id)
        .filter(|id| !id.is_empty() && entries.borrow().iter().all(|entry| entry.id != **id))
        .cloned()
        .collect();
    if now.iter().any(|id| !unknown.borrow().contains(id)) {
        let read = desktop::read_entries();
        if read != **entries.borrow() {
            *entries.borrow_mut() = Rc::new(read);
            forget_icons();
        }
    }
    *unknown.borrow_mut() = now;
    entries.borrow().clone()
}

fn launcher_entry(entries: &Entries, id: &str) -> Option<Entry> {
    launcher_entry::lock(entries)
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(id))
        .map(|(_, entry)| entry.clone())
}

fn entry_for<'a>(id: &str, entries: &'a [DesktopEntry]) -> Option<&'a DesktopEntry> {
    entries
        .iter()
        .find(|entry| entry.id == id)
        .or_else(|| find_entry(id, entries))
}

fn window_title(window: &ToplevelInfo) -> String {
    if window.title.is_empty() {
        window.app_id.clone()
    } else {
        window.title.clone()
    }
}

fn downloads_dir() -> PathBuf {
    ctappearance::xdg::user_dir("DOWNLOAD").unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Downloads")
    })
}

struct DockApp;

impl App for DockApp {
    fn render(&self) -> impl IntoElement {
        let toplevels = Toplevels::get();
        let windows = toplevels.list.read().clone();
        let entries = &use_desktop_entries(&windows);
        let config = use_defaults::<DockConfig>(DOMAIN).read().clone();
        let notifications = use_defaults::<NotificationsConfig>("notifications")
            .read()
            .clone();
        let launcher_entries = use_launcher_entries();
        let state = use_state(|| load_restoration::<DockState>(DOMAIN));
        let mut pointer = use_state(|| None::<(f32, f32)>);
        let mut hovering = use_state(|| false);
        let menu_open = use_state(|| false);
        let mut shown = use_state(|| !config.autohide);
        let mut launching = use_state(Vec::<String>::new);
        let mut resizing = use_state(|| None::<(f32, i32)>);
        let mut dragging = use_state(|| None::<Drag>);
        let after_drag = use_hook(|| Rc::new(Cell::new(false)));
        let revealing = use_hook(|| Rc::new(Cell::new(false)));
        let written_size = use_hook(|| Rc::new(Cell::new(0)));
        let activation = Activation::get();
        let timers = Timers::get();

        let thumbnails = use_thumbnails(&windows);
        let tiles = motion::use_tiles(&windows, config.minimize_to_application);
        let wallpaper = use_watched(ctappearance::load_wallpaper_state);
        let running = items::running(&windows, entries);
        let running_ids: Vec<String> = running.iter().map(|(id, _)| id.clone()).collect();
        let previous = use_hook(|| Rc::new(RefCell::new(Vec::<String>::new())));
        use_side_effect_with_deps(&running_ids, move |now: &Vec<String>| {
            let closed: Vec<String> = previous
                .borrow()
                .iter()
                .filter(|id| !now.contains(id))
                .cloned()
                .collect();
            *previous.borrow_mut() = now.clone();
            launching.write().retain(|id| !now.contains(id));
            if !closed.is_empty() {
                edit_state(state, |state| {
                    state.recent_apps =
                        items::remember_closed(&state.recent_apps, &closed, &state.persistent_apps);
                });
            }
        });
        use_side_effect_with_deps(&config.autohide, move |autohide: &bool| {
            shown.set(!*autohide)
        });

        let magnify = use_switch_progress(
            hovering,
            Animation::Ease {
                duration_ms: DOCK_MAGNIFY_MS,
            },
        );
        let hide_ms = (DOCK_HIDE_MS as f32 * config.autohide_time_modifier.max(0.0)) as u64;
        let visible = use_switch_progress(
            shown,
            if hide_ms == 0 {
                Animation::None
            } else {
                Animation::Ease {
                    duration_ms: hide_ms,
                }
            },
        );
        let mut bounce = use_animation(|conf| {
            conf.on_finish(OnFinish::Restart {
                delay: Duration::ZERO,
            });
            AnimNum::new(0.0, 1.0).time(DOCK_BOUNCE_MS)
        });
        // macOS: an app that asks for attention bounces until one of its windows is active.
        let attention: Vec<String> = running
            .iter()
            .filter(|(id, windows)| {
                !windows.iter().any(|window| window.active)
                    && launcher_entry(&launcher_entries, id).is_some_and(|entry| entry.urgent())
            })
            .map(|(id, _)| id.clone())
            .collect();
        let bouncing = !(launching.read().is_empty() && attention.is_empty());
        use_side_effect_with_deps(&bouncing, move |bouncing: &bool| {
            if *bouncing {
                bounce.start();
            } else {
                bounce.reset();
            }
        });
        let phase = bounce.get().value();

        // on_sized comes a frame after the surface grows for a drag; the platform has the size already.
        let root = *Platform::get().current_window().root_size.read();
        let (width, height) = (root.width, root.height);
        let frame = Frame {
            orientation: config.orientation,
            width,
            height,
        };
        let dock_state = state.read().clone();
        let drag_now = dragging.read().clone();
        let lifted = drag_now.as_ref().filter(|drag| drag.moved);
        let tile_windows: Vec<ToplevelInfo> =
            tiles.iter().map(|tile| tile.window.clone()).collect();
        let mut list = items::items(
            &config,
            lifted.map_or(&dock_state.persistent_apps, |drag| &drag.order),
            &dock_state.recent_apps,
            &running,
            &tile_windows,
        );
        // macOS: the dragged icon leaves its slot and the others slide apart where it would land.
        let lifted_from = lifted.and_then(|drag| list.iter().position(|item| drag.lifts(item)));
        if let Some(index) = lifted_from {
            list.remove(index);
        }
        let landing =
            lifted_from.filter(|_| lifted.is_none_or(|drag| !(drag.downloads && drag.away)));
        let mut gaps: Vec<(usize, f32)> = motion::use_slots(lifted.map(|_| {
            landing
                .map(|index| (index as u64, index))
                .into_iter()
                .collect()
        }))
        .iter()
        .map(|gap| (gap.window, gap.presence))
        .collect();
        gaps.sort_by_key(|&(index, _)| std::cmp::Reverse(index));
        for (index, presence) in gaps {
            list.insert(index.min(list.len()), Item::Gap(presence));
        }
        let slots: Vec<Slot> = list
            .iter()
            .map(|item| match item {
                Item::Separator => Slot::Separator,
                Item::Gap(presence) => Slot::Wide(*presence),
                Item::Window(window) => Slot::Wide(
                    thumbnails
                        .read()
                        .get(&window.id)
                        .map_or(1.0, |picture| thumbnail::units(picture.size))
                        * tiles
                            .iter()
                            .find(|tile| tile.window.id == window.id)
                            .map_or(1.0, |tile| tile.presence),
                ),
                _ => Slot::Tile,
            })
            .collect();
        let tile = geometry::fit(config.tilesize(), frame.main_length(), &slots);
        let largesize = config.largesize();
        let pointer_at = *pointer.read();
        let layout = geometry::layout(
            &slots,
            tile,
            largesize,
            frame.main_length() / 2.0,
            pointer_at.map(|at| frame.canonical(at).0),
            magnify,
        );
        let cross0 = DOCK_EDGE_MARGIN - (layout.thickness + DOCK_EDGE_MARGIN) * (1.0 - visible);
        let icon_cross = cross0 + layout.padding;
        let envelope = layout.thickness.max(layout.padding + largesize);

        let applied_layer = use_hook(|| Rc::new(Cell::new(None::<LayerGeometry>)));
        let layer = layer_geometry(&config, layout.thickness, lifted.is_some());
        if width > 0.0 && applied_layer.replace(Some(layer)) != Some(layer) {
            set_layer_geometry(layer);
        }
        let bar = frame.area(
            layout.bar_start,
            cross0,
            layout.bar_length,
            layout.thickness,
        );
        let strip = frame.area(0.0, 0.0, frame.main_length(), DOCK_REVEAL_STRIP as f32);
        let reach = if *hovering.read() {
            envelope
        } else {
            layout.thickness
        };
        // The bar takes the pointer from the screen edge, the margin under it included; while it
        // slides, the strip that revealed it keeps the pointer too, so it does not hide again.
        let column = frame.area(
            layout.bar_start,
            0.0,
            layout.bar_length,
            (cross0 + reach).max(0.0),
        );
        // smithay's click grab follows a surface that grows only where the surface takes the pointer.
        let input = if lifted.is_some() {
            vec![(0.0, 0.0, width, height)]
        } else if !config.autohide {
            vec![column]
        } else if visible < 0.01 {
            vec![strip]
        } else {
            vec![strip, column]
        };
        let radius = (layout.thickness * DOCK_RADIUS).round();
        let regions: Regions = (
            input.into_iter().map(region).collect(),
            if visible > 0.01 {
                vec![region(bar)]
            } else {
                Vec::new()
            },
            radius as u32,
        );
        let applied_regions = use_hook(|| Rc::new(RefCell::new(None::<Regions>)));
        if width > 0.0 && applied_regions.borrow().as_ref() != Some(&regions) {
            set_input_region(InputRegion {
                rects: regions.0.clone(),
            });
            set_blur_region(BlurRegion {
                rects: regions.1.clone(),
            });
            set_card_frames(CardFrames {
                radius: regions.2,
                cards: regions.1.clone(),
            });
            *applied_regions.borrow_mut() = Some(regions);
        }
        // A window with no tile yet minimizes into the slot its tile will take before the Trash;
        // the Dock is centred, so that slot's centre does not depend on how wide the tile ends up.
        let future = list
            .iter()
            .position(|item| *item == Item::Trash)
            .map(|trash| {
                let mut slots = slots.clone();
                slots.insert(trash, Slot::Wide(1.0));
                let tile = geometry::fit(config.tilesize(), frame.main_length(), &slots);
                let layout = geometry::layout(
                    &slots,
                    tile,
                    largesize,
                    frame.main_length() / 2.0,
                    pointer_at.map(|at| frame.canonical(at).0),
                    magnify,
                );
                (layout, trash)
            });
        let targets: Vec<MinimizeTarget> =
            items::minimize_slots(&list, config.minimize_to_application)
                .into_iter()
                .filter_map(|(window, index)| {
                    let has_tile =
                        matches!(list.get(index), Some(Item::Window(tile)) if tile.id == window);
                    let (layout, index) = if has_tile || config.minimize_to_application {
                        (&layout, index)
                    } else {
                        let (future, trash) = future.as_ref()?;
                        (future, *trash)
                    };
                    let slot = layout.placed[index];
                    let (x, y, width, height) = region(frame.area(
                        slot.start + slot.length / 2.0 - slot.size / 2.0,
                        DOCK_EDGE_MARGIN + layout.padding,
                        slot.size,
                        slot.size,
                    ));
                    Some(MinimizeTarget {
                        window,
                        rect: (x, y, width, height),
                    })
                })
                .collect();
        let sent_targets = use_hook(|| Rc::new(RefCell::new(Vec::<MinimizeTarget>::new())));
        if width > 0.0 {
            let mut sent = sent_targets.borrow_mut();
            targets
                .iter()
                .filter(|target| !sent.contains(target))
                .for_each(|target| set_minimize_target(*target));
            *sent = targets;
        }

        let resolved = bk::theme::resolved();
        let dock = resolved.dock;
        let label_fill = theme::color(resolved.shell.bar);
        let ink = theme::color(resolved.shell.text);
        let progress_track = theme::color(resolved.shell.tile);
        let progress_fill = theme::color(resolved.shell.tile_active);
        let dot = (tile * DOCK_DOT).clamp(DOCK_DOT_MIN, DOCK_DOT_MAX);
        let launching_now = launching.read().clone();

        let icon = |name: &str, contexts: &[&str]| load_icon_in(Some(name), ICON_SIZES, contexts);
        let app_name = |id: &str| match entry_for(id, entries) {
            Some(entry) => entry.name.clone(),
            None => resolve_app_name(Some(id), entries),
        };
        let app_icon_named = |id: &str, size: f32| {
            let name = entry_for(id, entries)
                .and_then(|entry| entry.icon.clone())
                .unwrap_or_else(|| id.to_string());
            on_grid(icon(&name, &["apps"]).as_ref(), &app_name(id), size)
        };
        let downloads_icon = |size: f32| {
            app_icon(
                icon("folder-download", &["places"]).as_ref(),
                tr!("dock-downloads"),
                size,
            )
        };

        let open_app = {
            let toplevels = toplevels.clone();
            let entries = entries.clone();
            let activation = activation.clone();
            let timers = timers.clone();
            let launchanim = config.launchanim;
            Rc::new(move |app: &AppTile| {
                let mut launching = launching;
                if let Some(window) = app.windows.iter().find(|window| !window.minimized) {
                    toplevels.activate(window.id);
                    return;
                }
                if let Some(window) = app.windows.first() {
                    toplevels.unset_minimized(window.id);
                    toplevels.activate(window.id);
                    return;
                }
                let Some(entry) = entry_for(&app.id, &entries).cloned() else {
                    return;
                };
                if launchanim && !launching.peek().contains(&app.id) {
                    launching.write().push(app.id.clone());
                    let id = app.id.clone();
                    timers.after(Duration::from_millis(DOCK_LAUNCH_TIMEOUT_MS), move || {
                        let mut launching = launching;
                        launching.write().retain(|launching| *launching != id)
                    });
                }
                launch(&activation, &timers, entry, || {});
            })
        };

        let mut children: Vec<Element> = Vec::new();
        let menu = Menus {
            frame,
            cross0,
            padding: layout.padding,
            menu_open,
            hovering,
            shown,
            autohide: config.autohide,
        };
        let mut drop_targets: Vec<(String, f32)> = Vec::new();
        let active_app: Vec<u64> = running
            .iter()
            .find(|(_, windows)| windows.iter().any(|window| window.active))
            .map(|(_, windows)| windows.iter().map(|window| window.id).collect())
            .unwrap_or_default();
        for (item, slot) in list.iter().zip(&layout.placed) {
            let center = slot.start + slot.length / 2.0;
            match item {
                Item::Gap(_) => {}
                Item::Separator => {
                    let length = tile * DOCK_SEPARATOR_LENGTH;
                    let line = if frame.vertical() {
                        rect().width(Size::px(length)).height(Size::px(1.0))
                    } else {
                        rect().width(Size::px(1.0)).height(Size::px(length))
                    };
                    let start_size = config.tilesize;
                    let area = frame.area(slot.start, cross0, slot.length, layout.thickness);
                    let separator_ink = theme::color(rim::divider(
                        &wallpaper.read(),
                        *toplevels.desktop.read(),
                        config.orientation,
                        (width, height),
                        area,
                        dock,
                    ));
                    children.push(
                        placed(area)
                            .center()
                            .child(line.background(separator_ink))
                            .on_pointer_down(move |event: Event<PointerEventData>| {
                                if !primary(&event) {
                                    return;
                                }
                                let at = event.global_location();
                                let (_, cross) = frame.canonical((at.x as f32, at.y as f32));
                                resizing.set(Some((cross, start_size)));
                            })
                            .on_secondary_down({
                                let items = separator_menu(&config);
                                let anchor = menu.anchor(slot.start, slot.length, tile);
                                move |_| menu.open(items.clone(), anchor, on_separator_command)
                            })
                            .into_element(),
                    );
                }
                Item::App(app) => {
                    if app.kept && drag_now.as_ref().is_none_or(|drag| drag.id != app.id) {
                        drop_targets.push((app.id.clone(), center));
                    }
                    let lift = if launching_now.contains(&app.id) || attention.contains(&app.id) {
                        (PI * phase).sin() * tile * DOCK_BOUNCE_HEIGHT
                    } else {
                        0.0
                    };
                    let launcher = launcher_entry(&launcher_entries, &app.id);
                    let badge = launcher
                        .as_ref()
                        .and_then(Entry::badge)
                        .filter(|_| notifications.app(&app.id).badge);
                    let progress = launcher.as_ref().and_then(Entry::progress);
                    let open = open_app.clone();
                    let pressed = app.clone();
                    let hide = toplevels.clone();
                    let active_app = active_app.clone();
                    let after_drag_down = after_drag.clone();
                    let after_drag = after_drag.clone();
                    let commands = Rc::new(AppCommands {
                        app: app.clone(),
                        toplevels: toplevels.clone(),
                        state,
                        open: open_app.clone(),
                    });
                    let items = app_menu(app);
                    let anchor = menu.anchor(slot.start, slot.length, slot.size + lift);
                    let (kept, id) = (app.kept, app.id.clone());
                    children.push(
                        placed(frame.area(slot.start, icon_cross + lift, slot.size, slot.size))
                            .child(app_icon_named(&app.id, slot.size))
                            .maybe_child(badge.map(|count| {
                                rect()
                                    .position(Position::new_absolute().top(0.0).right(0.0))
                                    .child(Badge::new(count.to_string()))
                            }))
                            .maybe_child(progress.map(|progress| {
                                progress_bar(slot.size, progress, progress_track, progress_fill)
                            }))
                            .interactive(false)
                            .into_element(),
                    );
                    children.push(
                        placed(frame.area(
                            slot.start,
                            cross0,
                            slot.size,
                            layout.padding + lift + slot.size,
                        ))
                        .on_pointer_down(move |event: Event<PointerEventData>| {
                            if !primary(&event) {
                                return;
                            }
                            let at = event.global_location();
                            let at = frame.canonical((at.x as f32, at.y as f32));
                            after_drag_down.set(false);
                            dragging.set(Some(Drag::start(
                                id.clone(),
                                kept,
                                false,
                                at,
                                state.peek().persistent_apps.clone(),
                            )));
                        })
                        .on_press(move |_| {
                            let dragged = dragging.peek().as_ref().is_some_and(|drag| drag.moved);
                            if dragged || after_drag.replace(false) {
                                return;
                            }
                            // macOS: Option-click hides the active app; on another app it also opens it.
                            if modifier_flags().contains(Modifiers::ALT) {
                                active_app.iter().for_each(|id| hide.minimize(*id));
                                if pressed.windows.iter().any(|window| window.active) {
                                    return;
                                }
                            }
                            open(&pressed);
                        })
                        .on_secondary_down(move |_| {
                            let commands = commands.clone();
                            menu.open(items.clone(), anchor, move |command: String| {
                                commands.run(&command)
                            })
                        })
                        .into_element(),
                    );
                    let bouncing = launching_now.contains(&app.id);
                    if config.show_process_indicators && !app.windows.is_empty() && !bouncing {
                        children.push(
                            placed(frame.area(
                                center - dot / 2.0,
                                cross0 + layout.padding - dot,
                                dot,
                                dot,
                            ))
                            .corner_radius(dot / 2.0)
                            .background(ink)
                            .interactive(false)
                            .into_element(),
                        );
                    }
                }
                Item::Window(window) => {
                    let toplevels = toplevels.clone();
                    let close = toplevels.clone();
                    let id = window.id;
                    let key = items::app_key(window, entries);
                    let arrived = tiles
                        .iter()
                        .find(|tile| tile.window.id == id)
                        .is_some_and(|tile| tile.arrived);
                    let area =
                        placed(frame.area(slot.start, icon_cross, slot.length, slot.size)).center();
                    if !arrived {
                        children.push(area.into_element());
                        continue;
                    }
                    let picture = thumbnails.read().get(&id).cloned();
                    children.push(
                        area.child(match &picture {
                            Some(picture) => {
                                let (width, height) = thumbnail::body(picture.size, slot.size);
                                // Freya caches by id; a window minimized again has a new picture.
                                ImageViewer::new((id, picture.png.clone()))
                                    .decode_mode(DecodeMode::Source)
                                    .asset_age(Duration::ZERO)
                                    .width(Size::px(width))
                                    .height(Size::px(height))
                                    .into_element()
                            }
                            None => rect()
                                .width(Size::px(slot.size * DOCK_WINDOW_FRAME))
                                .height(Size::px(slot.size * DOCK_WINDOW_FRAME))
                                .corner_radius(slot.size * DOCK_RADIUS / 2.0)
                                .background(progress_track)
                                .center()
                                .child(app_icon_named(&key, slot.size * DOCK_WINDOW_ICON))
                                .into_element(),
                        })
                        .maybe_child(picture.is_some().then(|| {
                            rect()
                                .position(Position::new_absolute().right(0.0).bottom(0.0))
                                .child(app_icon_named(&key, slot.size * DOCK_WINDOW_BADGE))
                        }))
                        .interactive(false)
                        .into_element(),
                    );
                    children.push(
                        placed(frame.area(
                            slot.start,
                            cross0,
                            slot.length,
                            layout.padding + slot.size,
                        ))
                        .on_press(move |_| {
                            toplevels.unset_minimized(id);
                            toplevels.activate(id);
                        })
                        .on_secondary_down({
                            let anchor = menu.anchor(slot.start, slot.length, slot.size);
                            move |_| {
                                let close = close.clone();
                                menu.open(
                                    vec![MenuItem::action("close", tr!("menu-close-window"))],
                                    anchor,
                                    move |_: String| close.close(id),
                                )
                            }
                        })
                        .into_element(),
                    );
                }
                Item::Downloads => {
                    let folder = downloads_dir();
                    let after_drag = after_drag.clone();
                    let after_drag_down = after_drag.clone();
                    children.push(
                        placed(frame.area(slot.start, icon_cross, slot.length, slot.size))
                            .center()
                            .child(downloads_icon(slot.size))
                            .on_pointer_down(move |event: Event<PointerEventData>| {
                                if !primary(&event) {
                                    return;
                                }
                                let at = event.global_location();
                                let at = frame.canonical((at.x as f32, at.y as f32));
                                after_drag_down.set(false);
                                dragging.set(Some(Drag::start(
                                    String::new(),
                                    false,
                                    true,
                                    at,
                                    state.peek().persistent_apps.clone(),
                                )));
                            })
                            .on_press({
                                let folder = folder.clone();
                                move |_| {
                                    let dragged =
                                        dragging.peek().as_ref().is_some_and(|drag| drag.moved);
                                    if !(dragged || after_drag.replace(false)) {
                                        xdg_open(&folder)
                                    }
                                }
                            })
                            .on_secondary_down({
                                let anchor = menu.anchor(slot.start, slot.length, slot.size);
                                move |_| {
                                    let folder = folder.clone();
                                    menu.open(open_menu(), anchor, move |_: String| {
                                        xdg_open(&folder)
                                    })
                                }
                            })
                            .into_element(),
                    );
                }
                Item::Trash => {
                    children.push(
                        placed(frame.area(slot.start, icon_cross, slot.size, slot.size))
                            .child(on_grid(
                                icon("user-trash", &["places"]).as_ref(),
                                tr!("dock-trash"),
                                slot.size,
                            ))
                            .interactive(false)
                            .into_element(),
                    );
                    children.push(
                        placed(frame.area(
                            slot.start,
                            cross0,
                            slot.size,
                            layout.padding + slot.size,
                        ))
                        .on_press(move |_| xdg_open(TRASH))
                        .on_secondary_down({
                            let anchor = menu.anchor(slot.start, slot.length, slot.size);
                            move |_| {
                                let mut items = open_menu();
                                items.push(
                                    MenuItem::action("empty", tr!("trash-empty"))
                                        .with_enabled(!crate::trash::is_empty()),
                                );
                                menu.open(items, anchor, |command: String| {
                                    if command == "empty" {
                                        spawn_component("empty-trash")
                                    } else {
                                        xdg_open(TRASH)
                                    }
                                })
                            }
                        })
                        .into_element(),
                    );
                }
            }
        }

        let hovered = pointer_at
            .filter(|_| *hovering.read() && drag_now.is_none())
            .and_then(|at| geometry::slot_at(&layout, frame.canonical(at).0))
            .and_then(|index| {
                let label = match &list[index] {
                    Item::App(app) => app_name(&app.id),
                    Item::Window(window) => window_title(window),
                    Item::Downloads => tr!("dock-downloads").to_string(),
                    Item::Trash => tr!("dock-trash").to_string(),
                    Item::Separator | Item::Gap(_) => return None,
                };
                Some((label, layout.placed[index]))
            });
        if let Some((text, slot)) = hovered {
            let bubble = rect()
                .padding(Gaps::new(
                    DOCK_LABEL_PADDING_Y,
                    DOCK_LABEL_PADDING_X,
                    DOCK_LABEL_PADDING_Y,
                    DOCK_LABEL_PADDING_X,
                ))
                .corner_radius(DOCK_LABEL_RADIUS)
                .background(label_fill)
                .child(Text::new(text).font_size(DOCK_LABEL_FONT_SIZE).color(ink));
            let beyond = icon_cross + slot.size + DOCK_LABEL_GAP;
            let center = slot.start + slot.length / 2.0;
            let lane = match config.orientation {
                DockOrientation::Bottom => rect()
                    .position(
                        Position::new_absolute()
                            .left(center - width / 2.0)
                            .bottom(beyond),
                    )
                    .width(Size::px(width))
                    .main_align(Alignment::Center)
                    .direction(Direction::Horizontal),
                DockOrientation::Left => rect()
                    .position(
                        Position::new_absolute()
                            .left(beyond)
                            .top(center - slot.size / 2.0),
                    )
                    .height(Size::px(slot.size))
                    .main_align(Alignment::Center),
                DockOrientation::Right => rect()
                    .position(
                        Position::new_absolute()
                            .right(beyond)
                            .top(center - slot.size / 2.0),
                    )
                    .height(Size::px(slot.size))
                    .main_align(Alignment::Center)
                    .cross_align(Alignment::End),
            };
            children.push(lane.child(bubble).into_element());
        }

        if let Some(drag) = lifted {
            let (main, cross) = drag.at;
            children.push(
                placed(frame.area(main - tile / 2.0, cross - tile / 2.0, tile, tile))
                    .child(if drag.downloads {
                        downloads_icon(tile)
                    } else {
                        app_icon_named(&drag.id, tile)
                    })
                    .maybe_child(drag.away.then(|| {
                        rect()
                            .position(
                                Position::new_absolute()
                                    .left(0.0)
                                    .top(-DOCK_LABEL_GAP * 2.0),
                            )
                            .width(Size::px(tile))
                            .center()
                            .child(
                                rect()
                                    .padding(Gaps::new(
                                        DOCK_LABEL_PADDING_Y,
                                        DOCK_LABEL_PADDING_X,
                                        DOCK_LABEL_PADDING_Y,
                                        DOCK_LABEL_PADDING_X,
                                    ))
                                    .corner_radius(DOCK_LABEL_RADIUS)
                                    .background(label_fill)
                                    .child(
                                        Text::new(tr!("dock-remove"))
                                            .font_size(DOCK_LABEL_FONT_SIZE)
                                            .color(ink),
                                    ),
                            )
                    }))
                    .into_element(),
            );
        }

        let bar_span = (layout.bar_start, layout.bar_start + layout.bar_length);
        let thickness = layout.thickness;
        let autohide = config.autohide;
        let delay = Duration::try_from_secs_f32(config.autohide_delay).unwrap_or_default();
        let reveal_timers = timers.clone();
        let reveal = revealing.clone();
        let leave = revealing.clone();
        let tilesize = config.tilesize;
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .on_pointer_move(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let at = (at.x as f32, at.y as f32);
                pointer.set(Some(at));
                let (main, cross) = frame.canonical(at);
                let reach = if *hovering.peek() {
                    envelope
                } else {
                    thickness
                };
                hovering.set_if_modified(
                    (bar_span.0..=bar_span.1).contains(&main) && cross <= cross0 + reach,
                );
                if autohide && !*shown.peek() && !reveal.get() {
                    reveal.set(true);
                    let reveal = reveal.clone();
                    reveal_timers.after(delay, move || {
                        if reveal.replace(false) {
                            let mut shown = shown;
                            shown.set(true);
                        }
                    });
                }
            })
            .on_pointer_leave(move |_| {
                if *menu_open.peek() {
                    return;
                }
                hovering.set(false);
                leave.set(false);
                if autohide && dragging.peek().is_none() && resizing.peek().is_none() {
                    shown.set(false);
                }
            })
            .on_global_pointer_move({
                let targets = drop_targets.clone();
                move |event: Event<PointerEventData>| {
                    let at = event.global_location();
                    let at = (at.x as f32, at.y as f32);
                    if let Some((start_cross, start_size)) = *resizing.peek() {
                        let (_, cross) = frame.canonical(at);
                        let next = (start_size as f32 + cross - start_cross)
                            .round()
                            .clamp(DOCK_TILESIZE_MIN as f32, DOCK_TILESIZE_MAX as f32)
                            as i32;
                        if next != tilesize && written_size.replace(next) != next {
                            edit_config(|config| config.tilesize = next);
                        }
                    }
                    let moving = dragging.peek().clone();
                    if let Some(mut drag) = moving {
                        let (main, cross) = frame.canonical(at);
                        let (dx, dy) = (main - drag.from.0, cross - drag.from.1);
                        drag.moved |= dx.hypot(dy) > DRAG.threshold;
                        drag.at = (main, cross);
                        drag.away = cross > cross0 + thickness + tile;
                        let before = targets
                            .iter()
                            .find(|(_, center)| main < *center)
                            .map(|(id, _)| id.clone());
                        let past_kept = before.is_none()
                            && targets
                                .last()
                                .is_some_and(|(_, center)| main > center + tile * 1.5);
                        let keep = !drag.away && !(past_kept && !drag.kept);
                        if !drag.downloads {
                            drag.order =
                                items::dropped(&drag.order, &drag.id, before.as_deref(), keep);
                        }
                        dragging.set(Some(drag));
                    }
                }
            })
            .on_global_pointer_up(move |_| {
                resizing.set(None);
                let Some(drag) = dragging.peek().clone() else {
                    return;
                };
                dragging.set(None);
                if !drag.moved {
                    return;
                }
                after_drag.set(true);
                // macOS keeps Downloads among the folders; dragged out, it leaves the Dock.
                if !drag.downloads {
                    edit_state(state, |state| state.persistent_apps = drag.order);
                } else if drag.away {
                    edit_config(|config| config.show_downloads = false);
                }
            })
            .children(children)
    }
}

// macOS opens an icon's menu beside it, centred, and keeps the Dock up while it is open.
#[derive(Clone, Copy)]
struct Menus {
    frame: Frame,
    cross0: f32,
    padding: f32,
    menu_open: State<bool>,
    hovering: State<bool>,
    shown: State<bool>,
    autohide: bool,
}

impl Menus {
    fn anchor(self, start: f32, length: f32, reach: f32) -> (i32, i32, u32, u32) {
        let (x, y, width, height) = region(self.frame.area(
            start,
            self.cross0,
            length,
            self.padding + reach + DOCK_LABEL_GAP,
        ));
        (x, y, width.max(1) as u32, height.max(1) as u32)
    }

    fn open(
        self,
        items: Vec<MenuItem>,
        anchor: (i32, i32, u32, u32),
        run: impl Fn(String) + 'static,
    ) {
        let mut menu_open = self.menu_open;
        menu_open.set(true);
        let side = match self.frame.orientation {
            DockOrientation::Bottom => PopupAnchor::Top,
            DockOrientation::Left => PopupAnchor::Right,
            DockOrientation::Right => PopupAnchor::Left,
        };
        pop_up_menu(items, anchor, side, run, move || {
            let (mut menu_open, mut hovering, mut shown) =
                (self.menu_open, self.hovering, self.shown);
            menu_open.set(false);
            hovering.set(false);
            if self.autohide {
                shown.set(false);
            }
        });
    }
}

fn progress_bar(size: f32, progress: f64, track: Color, fill: Color) -> Rect {
    let inset = size / 8.0;
    let width = size - 2.0 * inset;
    rect()
        .position(Position::new_absolute().bottom(inset).left(inset))
        .width(Size::px(width))
        .height(Size::px(DOCK_PROGRESS_HEIGHT))
        .corner_radius(DOCK_PROGRESS_HEIGHT / 2.0)
        .background(track)
        .child(
            rect()
                .width(Size::px(width * progress.clamp(0.0, 1.0) as f32))
                .height(Size::fill())
                .corner_radius(DOCK_PROGRESS_HEIGHT / 2.0)
                .background(fill),
        )
}

fn open_menu() -> Vec<MenuItem> {
    vec![MenuItem::action("open", tr!("dock-open"))]
}

fn app_menu(app: &AppTile) -> Vec<MenuItem> {
    let mut menu: Vec<MenuItem> = app
        .windows
        .iter()
        .map(|window| {
            MenuItem::action(format!("window:{}", window.id), window_title(window)).with_check(
                if window.minimized {
                    CheckState::Minimized
                } else {
                    CheckState::Checkbox(window.active)
                },
            )
        })
        .collect();
    if !menu.is_empty() {
        menu.push(MenuItem::Separator);
    }
    menu.push(MenuItem::submenu(
        tr!("dock-options"),
        vec![
            MenuItem::action("keep", tr!("dock-keep-in-dock"))
                .with_check(CheckState::Checkbox(app.kept)),
        ],
    ));
    menu.push(MenuItem::Separator);
    if app.windows.is_empty() {
        menu.push(MenuItem::action("open", tr!("dock-open")));
    } else {
        menu.push(MenuItem::action("hide", tr!("dock-hide")));
        menu.push(MenuItem::action("quit", tr!("dock-quit")));
    }
    menu
}

struct AppCommands {
    app: AppTile,
    toplevels: Toplevels,
    state: State<DockState>,
    open: Rc<dyn Fn(&AppTile)>,
}

impl AppCommands {
    fn run(&self, command: &str) {
        let windows = || self.app.windows.iter().map(|window| window.id);
        match command {
            "keep" => {
                let (id, kept) = (self.app.id.clone(), self.app.kept);
                edit_state(self.state, |state| {
                    state.persistent_apps =
                        items::dropped(&state.persistent_apps, &id, None, !kept);
                });
            }
            "open" => (self.open)(&self.app),
            "hide" => windows().for_each(|id| self.toplevels.minimize(id)),
            "quit" => windows().for_each(|id| self.toplevels.close(id)),
            window => {
                if let Some(id) = window
                    .strip_prefix("window:")
                    .and_then(|id| id.parse().ok())
                {
                    self.toplevels.unset_minimized(id);
                    self.toplevels.activate(id);
                }
            }
        }
    }
}

const ORIENTATIONS: [(DockOrientation, &str); 3] = [
    (DockOrientation::Left, "left"),
    (DockOrientation::Bottom, "bottom"),
    (DockOrientation::Right, "right"),
];

const EFFECTS: [(MinimizeEffect, &str); 2] = [
    (MinimizeEffect::Genie, "genie"),
    (MinimizeEffect::Scale, "scale"),
];

fn orientation_label(orientation: DockOrientation) -> &'static str {
    match orientation {
        DockOrientation::Left => tr!("dock-left"),
        DockOrientation::Bottom => tr!("dock-bottom"),
        DockOrientation::Right => tr!("dock-right"),
    }
}

fn effect_label(effect: MinimizeEffect) -> &'static str {
    match effect {
        MinimizeEffect::Genie => tr!("dock-genie"),
        MinimizeEffect::Scale => tr!("dock-scale"),
        MinimizeEffect::Suck => tr!("dock-suck"),
    }
}

fn separator_menu(config: &DockConfig) -> Vec<MenuItem> {
    let radio = |id: &str, label: &str, on: bool| {
        MenuItem::action(id.to_string(), label.to_string()).with_check(CheckState::Radio(on))
    };
    vec![
        MenuItem::action(
            "hiding",
            if config.autohide {
                tr!("dock-hiding-off")
            } else {
                tr!("dock-hiding-on")
            },
        ),
        MenuItem::action(
            "magnification",
            if config.magnification {
                tr!("dock-magnification-off")
            } else {
                tr!("dock-magnification-on")
            },
        ),
        MenuItem::Separator,
        MenuItem::submenu(
            tr!("dock-position"),
            ORIENTATIONS
                .iter()
                .map(|(value, id)| {
                    radio(id, orientation_label(*value), config.orientation == *value)
                })
                .collect(),
        ),
        MenuItem::submenu(
            tr!("dock-minimize-using"),
            EFFECTS
                .iter()
                .map(|(value, id)| radio(id, effect_label(*value), config.mineffect == *value))
                .collect(),
        ),
        MenuItem::Separator,
        MenuItem::action("settings", tr!("dock-settings")),
    ]
}

fn on_separator_command(command: String) {
    match command.as_str() {
        "hiding" => edit_config(|config| config.autohide = !config.autohide),
        "magnification" => edit_config(|config| config.magnification = !config.magnification),
        "settings" => open_settings("desktop-dock"),
        id => {
            if let Some((value, _)) = ORIENTATIONS.iter().find(|(_, key)| *key == id) {
                edit_config(|config| config.orientation = *value);
            } else if let Some((value, _)) = EFFECTS.iter().find(|(_, key)| *key == id) {
                edit_config(|config| config.mineffect = *value);
            }
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let config: DockConfig = load_defaults(DOMAIN);
    let geometry = layer_geometry(&config, geometry::thickness(config.tilesize()), false);
    let app = AppComponent::new(DockApp);
    run_layer(
        app,
        LayerSpec {
            namespace: ctdesign::DOCK_LAYER_NAMESPACE.to_string(),
            layer: Layer::Top,
            anchors: geometry.anchors,
            exclusive_zone: geometry.exclusive_zone,
            keyboard: geometry.keyboard,
            size: Some(geometry.size),
        },
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}
