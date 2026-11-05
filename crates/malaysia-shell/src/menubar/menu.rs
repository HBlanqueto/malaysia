use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use barrier_kit::menu::{
    MenuItem, MenuNavigation, MenuView, RevealRoots, SearchableEntry, SubmenuSource,
};
use barrier_kit::platform::{
    PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupId, PopupSpec, Popups,
};
use bytes::Bytes;
use freya_components::svg_viewer::SvgViewer;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Gaps, Size};
use vow_design::{
    MENUBAR_APP_NAME_WEIGHT, MENUBAR_HEIGHT, MENUBAR_TITLE_MAX_WIDTH, MENUBAR_TITLE_PADDING_X,
};

use crate::menubar::icons;
use barrier_kit::theme;

pub struct SearchableLists {
    pub menu: Vec<SearchableEntry>,
    pub help: Vec<SearchableEntry>,
}

pub type MenuItems = Rc<dyn Fn() -> Vec<MenuItem>>;

pub type SearchableItems = Rc<dyn Fn() -> SearchableLists>;

#[derive(Clone)]
pub struct MenuTitleSpec {
    pub label: String,
    pub glyph: Option<&'static [u8]>,
    pub bold: bool,
    pub items: Rc<dyn Fn() -> Vec<MenuItem>>,
    pub searchable: Option<SearchableItems>,
    pub on_activate: Rc<dyn Fn(String)>,
    pub live_items: Option<MenuItems>,
    pub on_open: Option<Rc<dyn Fn()>>,
    pub on_hover_title: Option<Rc<dyn Fn()>>,
    pub on_open_submenu: Option<Rc<dyn Fn(String)>>,
    pub submenu_source: Option<SubmenuSource>,
    pub anchor: Rc<Cell<(f32, f32)>>,
    pub enabled: bool,
    pub query_state: Option<Rc<RefCell<barrier_kit::menu::SearchQuery>>>,
    pub reveal_roots: Option<RevealRoots>,
}

impl MenuTitleSpec {
    pub fn new(
        label: String,
        items: MenuItems,
        on_activate: Rc<dyn Fn(String)>,
        anchor: Rc<Cell<(f32, f32)>>,
    ) -> Self {
        Self {
            label,
            glyph: None,
            bold: false,
            items,
            searchable: None,
            on_activate,
            live_items: None,
            on_open: None,
            on_hover_title: None,
            on_open_submenu: None,
            submenu_source: None,
            anchor,
            enabled: true,
            query_state: None,
            reveal_roots: None,
        }
    }
}

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

pub fn close_root_menu(state: &MenuState) {
    if let Some(id) = state.root_id.lock().unwrap().take() {
        Popups::get().close(id);
    }
    state.open_label.borrow_mut().set(None);
}

pub fn open_root_menu(titles: &Rc<Vec<MenuTitleSpec>>, title_index: usize, state: &MenuState) {
    let Some(spec) = titles.get(title_index) else {
        return;
    };
    close_root_menu(state);
    if let Some(on_open) = &spec.on_open {
        on_open();
    }

    let items = (spec.items)();
    let palette = barrier_kit::theme::menu_palette(
        &barrier_kit::theme::resolved(),
        barrier_kit::theme::SurfaceKind::LayerShell,
    );
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
    let mut view = MenuView::new(items, move |id| on_activate(id), on_dismiss_all).with_navigation(
        move |direction| {
            navigate_title(direction, title_index, &titles_for_nav, &state_for_nav);
        },
    );
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
            slide_y: false,
            resize_y: true,
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

pub fn menu_title(title_index: usize, titles: Rc<Vec<MenuTitleSpec>>, state: MenuState) -> Element {
    let spec = &titles[title_index];
    let label_text = spec.label.clone();
    let enabled = spec.enabled;
    let anchor = spec.anchor.clone();
    let is_open = state.open_label.borrow().read().as_deref() == Some(&label_text);

    let resolved = barrier_kit::theme::resolved();
    let palette =
        barrier_kit::theme::menu_palette(&resolved, barrier_kit::theme::SurfaceKind::LayerShell);
    let over_focus = barrier_kit::shell::Toplevels::get()
        .space
        .read()
        .focus_bar
        .over_focus();
    let fg_rgba = if over_focus {
        vow_design::FOCUS_BAR_TEXT
    } else {
        barrier_kit::theme::shell_palette(&resolved).text
    };
    let fg = theme::color(fg_rgba);

    let mut container = rect()
        .height(Size::fill())
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center);

    let mut title = rect()
        .height(Size::px(palette.bar_title_open_height))
        .padding(Gaps::new(
            0.0,
            MENUBAR_TITLE_PADDING_X,
            0.0,
            MENUBAR_TITLE_PADDING_X,
        ))
        .corner_radius(palette.bar_title_open_radius)
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .on_sized(move |event: Event<SizedEventData>| {
            let area = event.data().area;
            anchor.set((area.origin.x, area.size.width));
        });

    if is_open {
        title = title.background(theme::color(palette.bar_title_open_bg));
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
            .font_size(vow_design::MENUBAR_FONT_SIZE)
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
