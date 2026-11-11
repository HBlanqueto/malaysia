use std::cell::{Cell, RefCell};
use std::rc::Rc;

use bk::menu::{
    ItemsSource, MenuNavigation, MenuView, RevealRoots, SearchableEntry, SubmenuSource,
};
use bk::platform::{
    PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupId, PopupSpec, Popups,
};
use bk::prelude::*;
use ctdesign::{
    MENUBAR_APP_NAME_WEIGHT, MENUBAR_HEIGHT, MENUBAR_TEXT_LIFT, MENUBAR_TITLE_BLEED,
    MENUBAR_TITLE_MAX_WIDTH, MENUBAR_TITLE_PADDING_X, MenuPalette, Rgba, SYMBOL,
};
use freya_components::svg_viewer::SvgViewer;
use freya_core::integration::AppComponent;
use torin::prelude::{Gaps, Position, SizeFnContext};

use crate::menubar::icons;
use bk::theme;

pub struct SearchableLists {
    pub menu: Vec<SearchableEntry>,
    pub help: Vec<SearchableEntry>,
}

pub type SearchableItems = Rc<dyn Fn() -> SearchableLists>;

#[derive(Clone)]
pub struct MenuTitleSpec {
    pub label: String,
    pub glyph: Option<&'static [u8]>,
    pub icon: Option<Element>,
    pub bold: bool,
    pub items: ItemsSource,
    pub searchable: Option<SearchableItems>,
    pub on_activate: Rc<dyn Fn(String)>,
    pub live_items: Option<ItemsSource>,
    pub on_open: Option<Rc<dyn Fn()>>,
    pub on_hover_title: Option<Rc<dyn Fn()>>,
    pub on_open_submenu: Option<Rc<dyn Fn(String)>>,
    pub submenu_source: Option<SubmenuSource>,
    pub anchor: Rc<Cell<(f32, f32)>>,
    pub enabled: bool,
    pub query_state: Option<Rc<RefCell<bk::menu::SearchQuery>>>,
    pub reveal_roots: Option<RevealRoots>,
    pub min_width: f32,
}

impl MenuTitleSpec {
    pub fn new(
        label: String,
        items: ItemsSource,
        on_activate: Rc<dyn Fn(String)>,
        anchor: Rc<Cell<(f32, f32)>>,
    ) -> Self {
        Self {
            label,
            glyph: None,
            icon: None,
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
            min_width: 0.0,
        }
    }
}

#[derive(Clone)]
pub struct MenuState {
    pub root_id: Rc<Cell<Option<PopupId>>>,
    pub open_label: State<Option<String>>,
    pub hovered_label: State<Option<String>>,
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            root_id: use_hook(|| Rc::new(Cell::new(None))),
            open_label: use_state(|| None::<String>),
            hovered_label: use_state(|| None::<String>),
        }
    }
}

pub fn close_root_menu(state: &MenuState) {
    if let Some(id) = state.root_id.take() {
        Popups::get().close(id);
    }
    let mut open_label = state.open_label;
    open_label.set(None);
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
    let mut palette =
        bk::theme::menu_palette(&bk::theme::resolved(), bk::theme::SurfaceKind::LayerShell);
    palette.min_width = palette.min_width.max(spec.min_width);
    let size = MenuView::preferred_size(&items, palette);
    let (anchor_x, anchor_width) = spec.anchor.get();
    let label = spec.label.clone();

    let state_for_dismiss = state.clone();
    let on_dismiss_all = move || close_root_menu(&state_for_dismiss);

    let titles_for_nav = titles.clone();
    let state_for_nav = state.clone();
    let on_activate = spec.on_activate.clone();
    let min_width = spec.min_width;
    let mut view = MenuView::new(items, move |id| on_activate(id), on_dismiss_all)
        .with_navigation(move |direction| {
            navigate_title(direction, title_index, &titles_for_nav, &state_for_nav);
        })
        .style(move |style| MenuPalette {
            min_width: style.min_width.max(min_width),
            ..style
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
        offset: (
            (MENUBAR_TITLE_PADDING_X - palette.padding - palette.item_padding_x).round() as i32,
            0,
        ),
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

    let open_label = state.open_label;
    let dismissed_label = label.clone();
    let on_dismissed: Option<Rc<dyn Fn()>> = Some(Rc::new(move || {
        let still_open = open_label.read().as_deref() == Some(&dismissed_label);
        if still_open {
            let mut open_label = open_label;
            open_label.set(None);
        }
    }));
    let id = Popups::get().open(
        PopupSpec {
            on_dismissed,
            ..popup_spec
        },
        AppComponent::new(view),
    );
    state.root_id.set(Some(id));
    let mut open_label = state.open_label;
    open_label.set(Some(label));
}

pub fn toggle_menu(titles: &Rc<Vec<MenuTitleSpec>>, title_index: usize, state: &MenuState) {
    let Some(spec) = titles.get(title_index) else {
        return;
    };
    if state.open_label.read().as_deref() == Some(&spec.label) {
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
    let len = titles.len() as isize;
    let delta = match direction {
        MenuNavigation::Prev => -1,
        MenuNavigation::Next => 1,
    };
    if let Some(index) = (1..=len)
        .map(|step| (current_index as isize + delta * step).rem_euclid(len) as usize)
        .find(|&index| titles[index].enabled)
    {
        open_root_menu(titles, index, state);
    }
}

pub fn menu_title(
    title_index: usize,
    titles: Rc<Vec<MenuTitleSpec>>,
    state: MenuState,
    fg: Color,
) -> Element {
    let spec = &titles[title_index];
    let label_text = spec.label.clone();
    let enabled = spec.enabled;
    let anchor = spec.anchor.clone();
    let is_open = state.open_label.read().as_deref() == Some(&label_text);
    let hovered = state.hovered_label.read().as_deref() == Some(&label_text);

    let resolved = bk::theme::resolved();
    let palette = bk::theme::menu_palette(&resolved, bk::theme::SurfaceKind::LayerShell);

    let mut container = rect().height(Size::fill()).center();

    let mut title = rect()
        .height(Size::px(palette.bar_title_open_height))
        .padding(Gaps::new(
            0.0,
            MENUBAR_TITLE_PADDING_X,
            0.0,
            MENUBAR_TITLE_PADDING_X,
        ))
        .center()
        .on_sized(move |event: Event<SizedEventData>| {
            let area = event.data().area;
            anchor.set((area.origin.x, area.size.width));
        });

    if is_open {
        title = title.child(title_pill(palette.bar_title_open_bg, palette));
    } else if hovered {
        title = title.child(title_pill(palette.bar_hover_bg, palette));
    }

    let content = if let Some(icon) = &spec.icon {
        icon.clone()
    } else if let Some(glyph) = spec.glyph {
        SvgViewer::new(("menubar-snowflake", Bytes::from_static(glyph)))
            .width(Size::px(SYMBOL.grid))
            .height(Size::px(SYMBOL.grid))
            .color(fg)
            .show_loader(false)
            .into_element()
    } else {
        let mut text = Text::new(label_text.clone())
            .line_limit(1)
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

    let mut hovered_label = state.hovered_label;
    let label_for_leave = label_text.clone();
    container = container.on_pointer_leave(move |_: Event<PointerEventData>| {
        if hovered_label.read().as_deref() == Some(&label_for_leave) {
            hovered_label.set(None);
        }
    });

    let mut hovered_label = state.hovered_label;
    let open_label_for_hover = state.open_label;
    let titles_for_hover = titles.clone();
    let state_for_hover = state.clone();
    let label_for_hover = label_text.clone();
    let on_hover_title = spec.on_hover_title.clone();
    container = container.on_pointer_enter(move |_: Event<PointerEventData>| {
        if let Some(callback) = &on_hover_title {
            callback();
        }
        hovered_label.set(Some(label_for_hover.clone()));
        let current = open_label_for_hover.read().clone();
        if current.is_some_and(|current| current != label_for_hover) {
            open_root_menu(&titles_for_hover, title_index, &state_for_hover);
        }
    });

    container.into_element()
}

fn title_pill(color: Rgba, palette: MenuPalette) -> Rect {
    let overhang = MENUBAR_TITLE_PADDING_X + MENUBAR_TITLE_BLEED;
    let bled = move |context: SizeFnContext| Some(context.parent + 2.0 * overhang);
    rect()
        .position(
            Position::new_absolute()
                .left(-overhang)
                .top(MENUBAR_TEXT_LIFT),
        )
        // Freya paints same-layer siblings in hash order; the pill must stay under the label.
        .layer(-1)
        .width(Size::func(bled))
        .height(Size::px(palette.bar_title_open_height))
        .corner_radius(palette.bar_title_open_radius)
        .background(theme::color(color))
}
