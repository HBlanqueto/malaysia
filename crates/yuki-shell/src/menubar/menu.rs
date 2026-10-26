//! Shared menubar popup machinery.
//!
//! Each title on the bar can open a root `MenuView` popup anchored to its own
//! rectangle. Only one menu tree is open at a time; hovering over another title
//! while a menu is open switches to that title's menu. The root menu can also
//! navigate to the previous/next title with the Left/Right arrow keys.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use celestial_kit::menu::{
    MenuItem, MenuNavigation, MenuView, RevealRoots, SearchableEntry, SubmenuSource,
};
use celestial_kit::platform::{
    PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupId, PopupSpec, Popups,
};
use freya_components::svg_viewer::SvgViewer;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Gaps, Size};
use yuki_design::{MENUBAR_APP_NAME_WEIGHT, MENUBAR_HEIGHT, MENUBAR_TITLE_MAX_WIDTH};

use crate::menubar::{icons, rgba};

/// Searchable entries split by source: menu actions and help topics.
pub struct SearchableLists {
    /// Menu actions paired with their parent menu path and menubar location.
    pub menu: Vec<SearchableEntry>,
    /// Help topics paired with the "Temas de ayuda" path.
    pub help: Vec<SearchableEntry>,
}

/// Function that lazily returns a title's menu items.
pub type MenuItems = Rc<dyn Fn() -> Vec<MenuItem>>;

/// Function that lazily returns searchable menu entries.
pub type SearchableItems = Rc<dyn Fn() -> SearchableLists>;

/// One title that can open a root menu popup.
#[derive(Clone)]
pub struct MenuTitleSpec {
    pub label: String,
    /// Snowflake glyph drawn instead of `label`.
    pub glyph: Option<&'static [u8]>,
    /// Draw the label in the bold app-name weight.
    pub bold: bool,
    pub items: Rc<dyn Fn() -> Vec<MenuItem>>,
    /// Optional searchable entries for the help search field.
    pub searchable: Option<SearchableItems>,
    pub on_activate: Rc<dyn Fn(String)>,
    /// Re-reads the title's items on every render, so a remote menu updates
    /// while its popup stays open.
    pub live_items: Option<MenuItems>,
    /// Sends the lazy layout request for this title, if it has a remote id.
    pub on_open: Option<Rc<dyn Fn()>>,
    /// Requests a remote title's children when the pointer enters it.
    pub on_hover_title: Option<Rc<dyn Fn()>>,
    /// Requests the children of a nested remote submenu.
    pub on_open_submenu: Option<Rc<dyn Fn(String)>>,
    /// Resolves the loaded children of a remote submenu by id.
    pub submenu_source: Option<SubmenuSource>,
    /// Position and size of the title highlight, set by `on_sized`.
    pub anchor: Rc<Cell<(f32, f32)>>,
    pub enabled: bool,
    /// Optional external search-query store, used by the Help menu so the
    /// query survives closing and reopening.
    pub query_state: Option<Rc<RefCell<celestial_kit::menu::SearchQuery>>>,
    /// Root menus that can be revealed when a Help search result is
    /// highlighted. The index matches [`SearchLocation::title_index`].
    pub reveal_roots: Option<RevealRoots>,
}

/// State shared by all menu titles on the bar.
#[derive(Clone)]
pub struct MenuState {
    pub root_id: Arc<Mutex<Option<PopupId>>>,
    pub open_label: Rc<RefCell<State<Option<String>>>>,
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            root_id: use_hook(|| Arc::new(Mutex::new(None))),
            open_label: Rc::new(RefCell::new(use_state(|| None::<String>))),
        }
    }
}

/// Closes the currently open root popup, if any.
pub fn close_root_menu(state: &MenuState) {
    if let Some(id) = state.root_id.lock().unwrap().take() {
        Popups::get().close(id);
    }
    state.open_label.borrow_mut().set(None);
}

/// Opens a root menu popup for `title_index`.
pub fn open_root_menu(titles: &Rc<Vec<MenuTitleSpec>>, title_index: usize, state: &MenuState) {
    let Some(spec) = titles.get(title_index) else {
        return;
    };
    close_root_menu(state);
    if let Some(on_open) = &spec.on_open {
        on_open();
    }

    let items = (spec.items)();
    let palette = celestial_kit::theme::menu_palette(&celestial_kit::theme::resolved());
    let size = MenuView::preferred_size(&items, palette);
    let (anchor_x, anchor_width) = spec.anchor.get();
    let label = spec.label.clone();

    let cell = state.root_id.clone();
    let open_label = state.open_label.clone();
    let on_dismiss_all = move || {
        if let Some(id) = cell.lock().unwrap().take() {
            Popups::get().close(id);
        }
        open_label.borrow_mut().set(None);
    };

    let titles_for_nav = titles.clone();
    let state_for_nav = state.clone();
    let on_activate = spec.on_activate.clone();
    let mut view = MenuView::new(items, move |id| on_activate(id), on_dismiss_all, palette)
        .with_navigation(move |direction| {
            navigate_title(direction, title_index, &titles_for_nav, &state_for_nav);
        });
    if let Some(source) = &spec.live_items {
        let source = source.clone();
        view = view.with_items_source(move || source());
    }
    if let Some(callback) = &spec.on_open_submenu {
        let callback = callback.clone();
        view = view.with_open_submenu(move |id| callback(id));
    }
    if let Some(source) = &spec.submenu_source {
        let source = source.clone();
        view = view.with_submenu_source(move |id| source(id));
    }
    if let Some(searchable_fn) = &spec.searchable {
        let menu_source = searchable_fn.clone();
        let help_source = searchable_fn.clone();
        view = view
            .with_searchable(move || menu_source().menu)
            .with_help_topics(move || help_source().help)
            .with_search_icons(icons::MENU_ITEM, icons::HELP);
    }
    if let Some(query_state) = &spec.query_state {
        view = view.with_query_state(query_state.clone());
    }
    if let Some(roots) = &spec.reveal_roots {
        let roots = roots.clone();
        view = view.with_reveal_roots(move || roots());
    }

    let popup_spec = PopupSpec {
        anchor_rect: (
            anchor_x as i32,
            0,
            anchor_width.max(1.0) as u32,
            MENUBAR_HEIGHT as u32,
        ),
        anchor: PopupAnchor::BottomLeft,
        gravity: PopupGravity::BottomRight,
        offset: (0, 0),
        size: (size.0 as u32, size.1 as u32),
        constraint_adjustment: PopupConstraintAdjustment {
            flip_x: true,
            flip_y: true,
            slide_x: true,
            slide_y: true,
        },
        grab: true,
        on_dismissed: None,
    };
    view = view.with_popup_spec(popup_spec.clone());

    let open_label = state.open_label.clone();
    let dismissed_label = label.clone();
    let on_dismissed: Option<Rc<dyn Fn()>> = Some(Rc::new(move || {
        let still_open = open_label.borrow().read().as_deref() == Some(&dismissed_label);
        if still_open {
            open_label.borrow_mut().set(None);
        }
    }));
    let id = Popups::get().open(
        PopupSpec {
            on_dismissed,
            ..popup_spec
        },
        AppComponent::new(view),
    );
    state.root_id.lock().unwrap().replace(id);
    state.open_label.borrow_mut().set(Some(label));
}

/// Toggles a title's menu: open if closed, close if already open.
pub fn toggle_menu(titles: &Rc<Vec<MenuTitleSpec>>, title_index: usize, state: &MenuState) {
    let Some(spec) = titles.get(title_index) else {
        return;
    };
    if state.open_label.borrow().read().as_deref() == Some(&spec.label) {
        close_root_menu(state);
        return;
    }
    open_root_menu(titles, title_index, state);
}

fn navigate_title(
    direction: MenuNavigation,
    current_index: usize,
    titles: &Rc<Vec<MenuTitleSpec>>,
    state: &MenuState,
) {
    let len = titles.len();
    if len == 0 {
        return;
    }
    let delta = match direction {
        MenuNavigation::Prev => -1,
        MenuNavigation::Next => 1,
    };
    let mut index = (current_index as isize + delta).rem_euclid(len as isize) as usize;
    let start = index;
    loop {
        if titles[index].enabled {
            open_root_menu(titles, index, state);
            return;
        }
        index = (index as isize + delta).rem_euclid(len as isize) as usize;
        if index == start {
            break;
        }
    }
}

/// Renders one menubar title. When `enabled` is false it is a plain label.
pub fn menu_title(title_index: usize, titles: Rc<Vec<MenuTitleSpec>>, state: MenuState) -> Element {
    let spec = &titles[title_index];
    let label_text = spec.label.clone();
    let enabled = spec.enabled;
    let anchor = spec.anchor.clone();
    let is_open = state.open_label.borrow().read().as_deref() == Some(&label_text);

    let resolved = celestial_kit::theme::resolved();
    let palette = celestial_kit::theme::menu_palette(&resolved);
    let fg_rgba = celestial_kit::theme::shell_palette(&resolved).text;
    let fg = rgba(fg_rgba);

    let mut container = rect()
        .height(Size::px(MENUBAR_HEIGHT as f32))
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center);

    let mut title = rect()
        .height(Size::px(palette.bar_title_open_height))
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .corner_radius(palette.bar_title_open_radius)
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .on_sized(move |event: Event<SizedEventData>| {
            let area = event.data().area;
            anchor.set((area.origin.x, area.size.width));
        });

    if is_open {
        title = title.background(rgba(palette.bar_title_open_bg));
    }

    let content = if let Some(glyph) = spec.glyph {
        let [r, g, b, _] = fg_rgba.to_rgba8();
        let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
        let svg = String::from_utf8_lossy(glyph).replace("currentColor", &hex);
        SvgViewer::new(("menubar-snowflake", Bytes::from(svg)))
            .width(Size::px(16.0))
            .height(Size::px(16.0))
            .color(fg)
            .show_loader(false)
            .into_element()
    } else {
        let mut text = label()
            .text(label_text.clone())
            .font_size(13.0)
            .max_lines(1)
            .max_width(Size::px(MENUBAR_TITLE_MAX_WIDTH))
            .color(fg);
        if spec.bold {
            text = text.font_weight(MENUBAR_APP_NAME_WEIGHT);
        }
        text.into_element()
    };
    container = container.child(title.child(content));

    if !enabled {
        return container.into_element();
    }

    let titles_for_click = titles.clone();
    let state_for_click = state.clone();
    container = container.on_mouse_down(move |_: Event<MouseEventData>| {
        toggle_menu(&titles_for_click, title_index, &state_for_click);
    });

    let open_label_for_hover = state.open_label.clone();
    let titles_for_hover = titles.clone();
    let state_for_hover = state.clone();
    let label_for_hover = label_text.clone();
    let on_hover_title = spec.on_hover_title.clone();
    container = container.on_pointer_enter(move |_: Event<PointerEventData>| {
        if let Some(callback) = &on_hover_title {
            callback();
        }
        let current = open_label_for_hover.borrow().read().clone();
        if current.is_some_and(|current| current != label_for_hover) {
            open_root_menu(&titles_for_hover, title_index, &state_for_hover);
        }
    });

    container.into_element()
}
