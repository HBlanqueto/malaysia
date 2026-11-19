mod geometry;
mod icon;
mod items;
mod motion;
mod rim;
mod surface;
mod thumbnail;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    f32::consts::PI,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use bk::animation::use_switch_progress;
use bk::app::{
    load_defaults, load_restoration, save_restoration, update_defaults, use_defaults, use_wake,
    use_watched,
};
use bk::menu::{CheckState, MenuItem, measure_text};
use bk::platform::{
    PopoverShape, PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupId, PopupSpec, Popups,
    Timers, modifier_flags,
};
use bk::popup_button::{arrow_band, pop_up_menu};
use bk::prelude::*;
use bk::shell::{
    Activation, Anchors, BlurRegion, CardFrames, InputRegion, KeyboardInteractivity, Layer,
    LayerGeometry, LayerSpec, MinimizeTarget, PlateFrames, ToplevelInfo, Toplevels, capture_window,
    run_layer, set_blur_region, set_card_frames, set_input_region, set_layer_geometry,
    set_minimize_target, set_plate_frames,
};
use bk::styles::Animation;
use bk::theme;
use ctappearance::dock::{DockConfig, DockOrientation, MinimizeEffect};
use ctappearance::notifications::NotificationsConfig;
use ctdesign::{
    DOCK_BOUNCE_HEIGHT, DOCK_BOUNCE_MS, DOCK_DOT, DOCK_DOT_MAX, DOCK_DOT_MIN, DOCK_EDGE_MARGIN,
    DOCK_HIDE_MS, DOCK_ICON_BODY, DOCK_LABEL_ARROW_HEIGHT, DOCK_LABEL_FONT_SIZE, DOCK_LABEL_GAP,
    DOCK_LABEL_PADDING_X, DOCK_LABEL_PADDING_Y, DOCK_LAUNCH_TIMEOUT_MS, DOCK_MAGNIFY_MS,
    DOCK_MENU_GAP, DOCK_MINIMIZE_MS, DOCK_SEPARATOR_LENGTH, DOCK_SLOT_MS, DOCK_TILESIZE_MAX,
    DOCK_TILESIZE_MIN, DRAG, ICON_PLATE, TEXT_LINE_HEIGHT,
};
use freya_animation::prelude::{AnimNum, OnFinish, use_animation};
use freya_core::integration::AppComponent;
use freya_core::prelude::Modifiers;
use serde::{Deserialize, Serialize};
use torin::prelude::{Gaps, Position};

use crate::desktop::{self, DesktopEntry};
use crate::launcher::{
    ICON_SIZES, IconData, app_icon, forget_icons, launch, load_icon_in, plated_icon,
    use_folder_theme,
};
use crate::launcher_entry::{self, Entries, Entry};
use crate::localize::tr;
use crate::menubar::{find_entry, resolve_app_name};
use crate::spawn::{open_settings, spawn_component, xdg_open};
use geometry::Slot;
use icon::{DockIcon, DockPlace, DockWindow, Place};
use items::{AppTile, Item};
use surface::{minimize_targets, surface_regions};

const DOMAIN: &str = "dock";
const TRASH: &str = "trash:///";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct DockState {
    persistent_apps: Vec<String>,
    recent_apps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct Drag {
    id: String,
    kept: bool,
    downloads: bool,
    from: (f32, f32),
    at: (f32, f32),
    moved: bool,
    away: bool,
    released: Option<Instant>,
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
            released: None,
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

fn on_down(
    mut primary: impl FnMut((f32, f32)) + 'static,
    secondary: impl Fn() + 'static,
) -> impl FnMut(Event<PointerEventData>) + 'static {
    move |event| match event.data().button() {
        Some(MouseButton::Left) => {
            let at = event.global_location();
            primary((at.x as f32, at.y as f32))
        }
        Some(MouseButton::Right) => secondary(),
        _ => {}
    }
}

fn placed((x, y, width, height): Area) -> Rect {
    rect()
        .position(Position::new_absolute().left(x).top(y))
        .width(Size::px(width))
        .height(Size::px(height))
}

fn on_grid(
    icon: Option<&IconData>,
    name: &str,
    size: f32,
    raster_size: f32,
    glass_plate: bool,
) -> Element {
    let gridded = icon.is_some_and(|icon| icon.app);
    let body = if gridded { 1.0 } else { DOCK_ICON_BODY };
    rect()
        .width(Size::px(size))
        .height(Size::px(size))
        .center()
        .child(plated_icon(
            icon,
            name,
            size * body,
            Some(raster_size * body),
            glass_plate,
        ))
        .into_element()
}

fn plate((x, y, width, height): Area) -> (i32, i32, i32, i32) {
    let body = width.min(height) * DOCK_ICON_BODY;
    (
        (x + (width - body) / 2.0).round() as i32,
        (y + (height - body) / 2.0).round() as i32,
        body.round() as i32,
        body.round() as i32,
    )
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

const SCREEN: u32 = u16::MAX as u32;

fn layer_geometry(config: &DockConfig, thickness: f32, lifted: bool) -> LayerGeometry {
    let requested = config.tilesize();
    let headroom = (config.largesize() - requested).max(0.0) + requested * DOCK_BOUNCE_HEIGHT;
    let extent = if lifted {
        SCREEN
    } else {
        (DOCK_EDGE_MARGIN + geometry::thickness(requested) + headroom).ceil() as u32
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
    if let Err(error) = update_defaults::<DockConfig>(DOMAIN, change) {
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
    use_wake({
        let entries = entries.clone();
        move |wake| launcher_entry::start(entries, wake)
    });
    entries
}

fn use_trash_full() -> bool {
    let full = use_hook(|| Arc::new(AtomicBool::new(!crate::trash::is_empty())));
    use_wake({
        let full = full.clone();
        move |wake| crate::trash::start(full, wake)
    });
    full.load(Ordering::Relaxed)
}

#[derive(Clone, PartialEq)]
struct Thumbnail {
    size: (u32, u32),
    png: Bytes,
}

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

fn name_of(id: &str, entries: &[DesktopEntry]) -> String {
    match entry_for(id, entries) {
        Some(entry) => entry.name.clone(),
        None => resolve_app_name(Some(id), entries),
    }
}

fn app_icon_named(
    id: &str,
    entries: &[DesktopEntry],
    size: f32,
    raster_size: f32,
    glass_plate: bool,
) -> Element {
    let name = entry_for(id, entries)
        .and_then(|entry| entry.icon.clone())
        .unwrap_or_else(|| id.to_string());
    on_grid(
        load_icon_in(Some(name.as_str()), ICON_SIZES, &["apps"]).as_ref(),
        &name_of(id, entries),
        size,
        raster_size,
        glass_plate,
    )
}

fn downloads_icon(size: f32) -> Element {
    app_icon(
        load_icon_in(Some("folder-download"), ICON_SIZES, &["places"]).as_ref(),
        tr!("dock-downloads"),
        size,
    )
}

fn trash_icon(full: bool, size: f32, raster_size: f32) -> Element {
    let name = if full {
        "user-trash-full"
    } else {
        "user-trash"
    };
    on_grid(
        load_icon_in(Some(name), ICON_SIZES, &["places"]).as_ref(),
        tr!("dock-trash"),
        size,
        raster_size,
        false,
    )
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
        let windows: Vec<ToplevelInfo> = toplevels
            .list
            .read()
            .iter()
            .filter(|window| items::is_app_window(window))
            .cloned()
            .collect();
        let entries = &use_desktop_entries(&windows);
        let config = use_defaults::<DockConfig>(DOMAIN).read().clone();
        let autohide = config.autohide || toplevels.focus_bar.read().over_focus();
        let notifications = use_defaults::<NotificationsConfig>(crate::notifications::DOMAIN)
            .read()
            .clone();
        let launcher_entries = use_launcher_entries();
        let state = use_state(|| load_restoration::<DockState>(DOMAIN));
        let mut pointer = use_state(|| None::<f32>);
        let mut hovering = use_state(|| false);
        let menu_open = use_state(|| false);
        let mut shown = use_state(|| !config.autohide);
        let mut launching = use_state(Vec::<String>::new);
        let mut resizing = use_state(|| None::<(f32, i32)>);
        let mut dragging = use_state(|| None::<Drag>);
        let after_drag = use_hook(|| Rc::new(Cell::new(false)));
        let revealing = use_hook(|| Rc::new(Cell::new(false)));
        let written_size = use_hook(|| Rc::new(Cell::new(0)));
        let shown_label = use_hook(|| Rc::new(RefCell::new(None::<ShownLabel>)));
        let label_text = use_state(String::new);
        let activation = Activation::get();
        let timers = Timers::get();

        let thumbnails = use_thumbnails(&windows);
        let tiles = motion::use_tiles(&windows, config.minimize_to_application);
        let wallpaper = use_watched(ctappearance::load_wallpaper_state);
        let theme_revision = use_folder_theme();
        let trash_full = use_trash_full();
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
        use_side_effect_with_deps(&autohide, move |autohide: &bool| shown.set(!*autohide));

        let magnify = use_switch_progress(
            hovering,
            Animation::Ease {
                duration_ms: DOCK_MAGNIFY_MS,
            },
        );
        let hide_ms = (DOCK_HIDE_MS as f32 * config.autohide_time_modifier.max(0.0)) as u64;
        let minimized: Vec<(u64, bool)> = windows
            .iter()
            .map(|window| (window.id, window.minimized))
            .collect();
        let minimized_before = use_hook(|| Rc::new(RefCell::new(minimized.clone())));
        let peek_timers = timers.clone();
        use_side_effect_with_deps(
            &(minimized, autohide, hide_ms),
            move |(now, autohide, hide_ms): &(Vec<(u64, bool)>, bool, u64)| {
                let flipped = now
                    .iter()
                    .any(|(id, minimized)| minimized_before.borrow().contains(&(*id, !*minimized)));
                *minimized_before.borrow_mut() = now.clone();
                if !(flipped && *autohide && !*shown.peek()) {
                    return;
                }
                shown.set(true);
                peek_timers.after(
                    Duration::from_millis(hide_ms + DOCK_MINIMIZE_MS),
                    move || {
                        if !*hovering.peek() && !*menu_open.peek() {
                            let mut shown = shown;
                            shown.set(false);
                        }
                    },
                );
            },
        );
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
            lifted
                .filter(|drag| drag.released.is_none())
                .map_or(&dock_state.persistent_apps, |drag| &drag.order),
            &dock_state.recent_apps,
            &running,
            &tile_windows,
        );
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
        let magnifying = largesize > tile;
        let pointer_at = *pointer.read();
        let layout = geometry::layout(
            &slots,
            tile,
            largesize,
            frame.main_length() / 2.0,
            pointer_at,
            magnify,
        );
        let rest = geometry::layout(
            &slots,
            tile,
            largesize,
            frame.main_length() / 2.0,
            None,
            0.0,
        );
        let cross0 = DOCK_EDGE_MARGIN - (layout.thickness + DOCK_EDGE_MARGIN) * (1.0 - visible);
        let icon_cross = cross0 + layout.padding;
        let envelope = layout.thickness.max(layout.padding + largesize);

        let applied_layer = use_hook(|| Rc::new(Cell::new(None::<LayerGeometry>)));
        let layer = layer_geometry(&config, layout.thickness, lifted.is_some());
        if width > 0.0 && applied_layer.replace(Some(layer)) != Some(layer) {
            set_layer_geometry(layer);
        }
        let reach = if *hovering.read() {
            envelope
        } else {
            layout.thickness
        };
        let regions = surface_regions(
            frame,
            &layout,
            (cross0, visible, reach),
            autohide,
            lifted.is_some(),
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
        let targets = minimize_targets(
            &list,
            &slots,
            &layout,
            (frame, &config),
            (pointer_at, magnify),
        );
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
        let glass_plates = resolved.icon_style == IconStyle::Clear && !resolved.reduce_transparency;
        let applied_plates = use_hook(|| Rc::new(RefCell::new(None::<PlateFrames>)));
        let mut plates = Vec::new();
        let ink = theme::color(resolved.shell.text);
        let dot = (tile * DOCK_DOT).clamp(DOCK_DOT_MIN, DOCK_DOT_MAX);
        let launching_now = launching.read().clone();
        let lift_of = |id: &String| {
            if launching_now.contains(id) || attention.contains(id) {
                (PI * phase).sin() * tile * DOCK_BOUNCE_HEIGHT
            } else {
                0.0
            }
        };

        let app_name = |id: &str| name_of(id, entries);

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
            autohide,
        };
        let mut drop_targets: Vec<(String, f32)> = Vec::new();
        let active_app: Vec<u64> = running
            .iter()
            .find(|(_, windows)| windows.iter().any(|window| window.active))
            .map(|(_, windows)| windows.iter().map(|window| window.id).collect())
            .unwrap_or_default();
        for ((item, slot), resting) in list.iter().zip(&layout.placed).zip(&rest.placed) {
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
                            .on_pointer_down({
                                let config = config.clone();
                                let anchor = menu.anchor(resting.start, resting.length, tile);
                                on_down(
                                    move |at| {
                                        let (_, cross) = frame.canonical(at);
                                        resizing.set(Some((cross, start_size)));
                                    },
                                    move || {
                                        menu.open(
                                            separator_menu(&config),
                                            anchor,
                                            on_separator_command,
                                        )
                                    },
                                )
                            })
                            .into_element(),
                    );
                }
                Item::App(app) => {
                    if app.kept && drag_now.as_ref().is_none_or(|drag| drag.id != app.id) {
                        drop_targets.push((app.id.clone(), center));
                    }
                    let lift = lift_of(&app.id);
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
                    let menu_app = app.clone();
                    let menu_toplevels = toplevels.clone();
                    let menu_launch = open_app.clone();
                    let anchor = menu.anchor(resting.start, resting.length, resting.size);
                    let (kept, id) = (app.kept, app.id.clone());
                    let icon_area = frame.area(slot.start, icon_cross + lift, slot.size, slot.size);
                    if glass_plates {
                        plates.push(plate(icon_area));
                    }
                    children.push(
                        placed(icon_area)
                            .child(DockIcon {
                                id: app.id.clone(),
                                entries: entries.clone(),
                                theme_revision,
                                size: slot.size,
                                raster_size: largesize,
                                glass_plate: glass_plates,
                                badge,
                                progress,
                            })
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
                        .on_pointer_down(on_down(
                            move |at| {
                                after_drag_down.set(false);
                                dragging.set(Some(Drag::start(
                                    id.clone(),
                                    kept,
                                    false,
                                    frame.canonical(at),
                                    state.peek().persistent_apps.clone(),
                                )));
                            },
                            move || {
                                let commands = Rc::new(AppCommands {
                                    app: menu_app.clone(),
                                    toplevels: menu_toplevels.clone(),
                                    state,
                                    open: menu_launch.clone(),
                                });
                                menu.open(app_menu(&menu_app), anchor, move |command: String| {
                                    commands.run(&command)
                                })
                            },
                        ))
                        .on_press(move |_| {
                            let dragged = dragging.peek().as_ref().is_some_and(|drag| drag.moved);
                            if dragged || after_drag.replace(false) {
                                return;
                            }
                            if modifier_flags().contains(Modifiers::ALT) {
                                active_app.iter().for_each(|id| hide.minimize(*id));
                                if pressed.windows.iter().any(|window| window.active) {
                                    return;
                                }
                            }
                            open(&pressed);
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
                        area.child(DockWindow {
                            id,
                            key,
                            entries: entries.clone(),
                            theme_revision,
                            picture,
                            size: slot.size,
                            length: slot.length,
                            raster_size: largesize,
                        })
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
                            let anchor = menu.anchor(resting.start, resting.length, resting.size);
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
                    let after_drag = after_drag.clone();
                    let after_drag_down = after_drag.clone();
                    children.push(
                        placed(frame.area(slot.start, icon_cross, slot.length, slot.size))
                            .center()
                            .child(DockPlace {
                                place: Place::Downloads,
                                theme_revision,
                                size: slot.size,
                                raster_size: largesize,
                            })
                            .on_pointer_down({
                                let anchor =
                                    menu.anchor(resting.start, resting.length, resting.size);
                                on_down(
                                    move |at| {
                                        after_drag_down.set(false);
                                        dragging.set(Some(Drag::start(
                                            String::new(),
                                            false,
                                            true,
                                            frame.canonical(at),
                                            state.peek().persistent_apps.clone(),
                                        )));
                                    },
                                    move || {
                                        menu.open(open_menu(), anchor, |_: String| {
                                            xdg_open(downloads_dir())
                                        })
                                    },
                                )
                            })
                            .on_press(move |_| {
                                let dragged =
                                    dragging.peek().as_ref().is_some_and(|drag| drag.moved);
                                if !(dragged || after_drag.replace(false)) {
                                    xdg_open(downloads_dir())
                                }
                            })
                            .into_element(),
                    );
                }
                Item::Trash => {
                    children.push(
                        placed(frame.area(slot.start, icon_cross, slot.size, slot.size))
                            .child(DockPlace {
                                place: Place::Trash { full: trash_full },
                                theme_revision,
                                size: slot.size,
                                raster_size: largesize,
                            })
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
                            let anchor = menu.anchor(resting.start, resting.length, resting.size);
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
            .filter(|_| {
                *hovering.read() && drag_now.is_none() && !*menu_open.read() && visible > 0.99
            })
            .and_then(|main| geometry::slot_at(&layout, main))
            .and_then(|index| {
                let (label, lift) = match &list[index] {
                    Item::App(app) => (app_name(&app.id), lift_of(&app.id)),
                    Item::Window(window) => (window_title(window), 0.0),
                    Item::Downloads => (tr!("dock-downloads").to_string(), 0.0),
                    Item::Trash => (tr!("dock-trash").to_string(), 0.0),
                    Item::Separator | Item::Gap(_) => return None,
                };
                Some((label, layout.placed[index], lift))
            });
        let label_colors = bk::theme::popover_palette(&resolved, bk::theme::SurfaceKind::Toplevel);
        show_label(
            &shown_label,
            label_text,
            hovered.map(|(text, slot, lift)| {
                let anchor = menu.beside(slot.start, slot.length, slot.size + lift, DOCK_LABEL_GAP);
                (text, anchor)
            }),
            menu.placement(),
            (
                theme::color(bk::theme::over_glass(label_colors).bg),
                theme::color(label_colors.fg),
            ),
        );

        if let Some(drag) = lifted {
            let (main, cross) = match (
                drag.released,
                landing.and_then(|index| layout.placed.get(index)),
            ) {
                (Some(since), Some(home)) => {
                    let share = motion::returned(since, Instant::now());
                    let (home_main, home_cross) =
                        (home.start + home.length / 2.0, icon_cross + tile / 2.0);
                    (
                        drag.at.0 + (home_main - drag.at.0) * share,
                        drag.at.1 + (home_cross - drag.at.1) * share,
                    )
                }
                _ => drag.at,
            };
            children.push(
                placed(frame.area(main - tile / 2.0, cross - tile / 2.0, tile, tile))
                    .child(if drag.downloads {
                        downloads_icon(tile)
                    } else {
                        app_icon_named(&drag.id, entries, tile, largesize, false)
                    })
                    .into_element(),
            );
        }

        let frames = PlateFrames {
            corner: (ICON_PLATE.corner * 1000.0).round() as u32,
            plates: if visible > 0.01 { plates } else { Vec::new() },
        };
        if width > 0.0 && applied_plates.borrow().as_ref() != Some(&frames) {
            set_plate_frames(frames.clone());
            *applied_plates.borrow_mut() = Some(frames);
        }

        let bar_span = (layout.bar_start, layout.bar_start + layout.bar_length);
        let thickness = layout.thickness;
        let delay = Duration::try_from_secs_f32(config.autohide_delay).unwrap_or_default();
        let reveal_timers = timers.clone();
        let reveal = revealing.clone();
        let leave = revealing.clone();
        let tilesize = config.tilesize;
        let return_timers = timers.clone();
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .on_pointer_move(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let (main, cross) = frame.canonical((at.x as f32, at.y as f32));
                pointer.set_if_modified(Some(if magnifying {
                    main
                } else {
                    geometry::centre_of_slot_at(&rest, main)
                }));
                let reach = if *hovering.peek() {
                    envelope
                } else {
                    thickness
                };
                hovering.set_if_modified(
                    !*menu_open.peek()
                        && (bar_span.0..=bar_span.1).contains(&main)
                        && cross <= cross0 + reach,
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
                    let moving = dragging
                        .peek()
                        .clone()
                        .filter(|drag| drag.released.is_none());
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
                let Some(mut drag) = dragging.peek().clone() else {
                    return;
                };
                if drag.released.is_some() {
                    return;
                }
                if !drag.moved {
                    dragging.set(None);
                    return;
                }
                after_drag.set(true);
                if drag.away {
                    let since = Instant::now();
                    drag.away = false;
                    drag.released = Some(since);
                    dragging.set(Some(drag));
                    return_timers.after(Duration::from_millis(DOCK_SLOT_MS), move || {
                        let mut dragging = dragging;
                        if dragging
                            .peek()
                            .as_ref()
                            .is_some_and(|drag| drag.released == Some(since))
                        {
                            dragging.set(None);
                        }
                    });
                    return;
                }
                dragging.set(None);
                if !drag.downloads {
                    edit_state(state, |state| state.persistent_apps = drag.order);
                }
            })
            .children(children)
    }
}

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
    fn placement(self) -> (PopupAnchor, PopupGravity) {
        match self.frame.orientation {
            DockOrientation::Bottom => (PopupAnchor::Top, PopupGravity::Top),
            DockOrientation::Left => (PopupAnchor::Right, PopupGravity::Right),
            DockOrientation::Right => (PopupAnchor::Left, PopupGravity::Left),
        }
    }

    fn anchor(self, start: f32, length: f32, reach: f32) -> (i32, i32, u32, u32) {
        self.beside(start, length, reach, DOCK_MENU_GAP)
    }

    fn beside(self, start: f32, length: f32, reach: f32, gap: f32) -> (i32, i32, u32, u32) {
        let (x, y, width, height) =
            region(
                self.frame
                    .area(start, self.cross0, length, self.padding + reach + gap),
            );
        (x, y, width.max(1) as u32, height.max(1) as u32)
    }

    fn open(
        self,
        items: Vec<MenuItem>,
        anchor: (i32, i32, u32, u32),
        run: impl Fn(String) + 'static,
    ) {
        let (mut menu_open, mut hovering) = (self.menu_open, self.hovering);
        menu_open.set(true);
        hovering.set(false);
        let (side, _) = self.placement();
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

struct ShownLabel {
    text: String,
    anchor: (i32, i32, u32, u32),
    id: PopupId,
}

fn show_label(
    shown: &RefCell<Option<ShownLabel>>,
    mut text: State<String>,
    wanted: Option<(String, (i32, i32, u32, u32))>,
    placement: (PopupAnchor, PopupGravity),
    (fill, ink): (Color, Color),
) {
    let popups = Popups::get();
    let mut shown = shown.borrow_mut();
    let Some((wanted_text, anchor)) = wanted else {
        if let Some(label) = shown.take() {
            popups.close(label.id);
        }
        return;
    };
    if let Some(label) = shown.as_mut() {
        if label.text != wanted_text || label.anchor != anchor {
            popups.reposition(label.id, label_spec(&wanted_text, anchor, placement));
            text.set_if_modified(wanted_text.clone());
            label.text = wanted_text;
            label.anchor = anchor;
        }
        return;
    }
    text.set_if_modified(wanted_text.clone());
    let content = AppComponent::new(DockLabel {
        text,
        band: arrow_band(placement.0, DOCK_LABEL_ARROW_HEIGHT),
        fill,
        ink,
    });
    let spec = label_spec(&wanted_text, anchor, placement);
    let id = popups.open_popover(spec, content, PopoverShape::Label);
    *shown = Some(ShownLabel {
        text: wanted_text,
        anchor,
        id,
    });
}

fn label_spec(
    text: &str,
    anchor: (i32, i32, u32, u32),
    (side, gravity): (PopupAnchor, PopupGravity),
) -> PopupSpec {
    let band = arrow_band(side, DOCK_LABEL_ARROW_HEIGHT);
    let width = measure_text(text, DOCK_LABEL_FONT_SIZE) + 2.0 * DOCK_LABEL_PADDING_X;
    let height = DOCK_LABEL_FONT_SIZE * TEXT_LINE_HEIGHT + 2.0 * DOCK_LABEL_PADDING_Y;
    PopupSpec {
        anchor_rect: anchor,
        anchor: side,
        gravity,
        size: (
            (width + band.horizontal()).ceil() as u32,
            (height + band.vertical()).ceil() as u32,
        ),
        constraint_adjustment: PopupConstraintAdjustment {
            slide_x: true,
            slide_y: true,
            ..PopupConstraintAdjustment::default()
        },
        ..PopupSpec::default()
    }
}

struct DockLabel {
    text: State<String>,
    band: Gaps,
    fill: Color,
    ink: Color,
}

impl App for DockLabel {
    fn render(&self) -> impl IntoElement {
        use_hook(|| set_input_region(InputRegion::default()));
        rect()
            .expanded()
            .background(self.fill)
            .padding(self.band)
            .center()
            .child(
                Text::new(self.text.read().to_string())
                    .font_size(DOCK_LABEL_FONT_SIZE)
                    .color(self.ink),
            )
    }
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
        MenuItem::action(
            "downloads",
            if config.show_downloads {
                tr!("dock-downloads-off")
            } else {
                tr!("dock-downloads-on")
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
        "downloads" => edit_config(|config| config.show_downloads = !config.show_downloads),
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

pub(crate) fn run() -> anyhow::Result<()> {
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
