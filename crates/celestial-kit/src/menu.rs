//! Neptunia menu control for CelestialKit popups.
//!
//! Distinct from [`freya_components::menu::Menu`]. This one is designed for the
//! shell's global menus and context menus: it opens submenus as child popups,
//! supports keyboard navigation and reports actions through `on_activate`.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use freya_components::svg_viewer::SvgViewer;
use freya_core::integration::{AppComponent, default_fonts};
use freya_core::prelude::*;
use freya_engine::prelude::{ParagraphBuilder, ParagraphStyle, TextStyle};
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Alignment, Direction, Gaps, Size};

// Freya re-exports `State` as the reactive handle returned by `use_state`.
type UseState<T> = State<T>;

use crate::platform::{
    PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupHandle, PopupId, PopupSpec, Popups,
};
use yuki_design::{MenuPalette, Rgba, SANS_FAMILIES};

/// Which pool a search result comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchSource {
    /// A real menu entry (App, Ventana or a nested submenu).
    Menu,
    /// A local help topic.
    Help,
}

/// Where a searchable menu entry lives in the menubar, so the Help popup can
/// reveal its real menu when the result is highlighted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchLocation {
    /// Index of the top-level menu title in the menubar.
    pub title_index: usize,
    /// Indices inside the menu tree down to the entry itself.
    pub indices: Vec<usize>,
}

/// One entry that the Help search field can match.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchableEntry {
    /// Human-readable path (e.g. "Ventana › Trasladar y redimensionar").
    pub path: String,
    /// The menu item to activate when selected.
    pub item: MenuItem,
    /// Where the entry lives in the menubar, if it belongs to a real menu.
    pub location: Option<SearchLocation>,
    /// Whether it is a menu action or a help topic.
    pub source: SearchSource,
}

/// Persistent state for a search field so it survives closing and reopening
/// the popup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchQuery {
    /// Current query text.
    pub text: String,
    /// Whether the text is selected and the next keystroke replaces it.
    pub selected: bool,
}

/// The process-wide text stack for measurement. Mirrors the renderer's
/// `FontCollection` (system manager plus the shell's fallback families).
fn font_families() -> Vec<Cow<'static, str>> {
    let mut families: Vec<Cow<'static, str>> = SANS_FAMILIES
        .iter()
        .map(|name| Cow::Borrowed(*name))
        .collect();
    families.extend(default_fonts());
    families
}

/// Measures `text` with the same paragraph stack Freya draws it with. The
/// previous implementation used its own `FontMgr`, which missed the fallback
/// faces that carry `⇧`, `⌃` or accented glyphs and returned a too-small width.
/// Formats a shortcut written as text (`Ctrl+Shift+G`, `Alt+Inicio`) with the
/// macOS key glyphs and order: `⌃` Ctrl, `⌥` Alt, `⇧` Shift, `❖` Super.
/// There is no `+` between them and the Command glyph is never emitted.
pub fn format_shortcut(text: &str) -> String {
    let text = text.trim();
    let (mut ctrl, mut alt, mut shift, mut sup) = (false, false, false, false);
    let mut key = String::new();
    for token in text.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" | "option" | "opt" => alt = true,
            "shift" => shift = true,
            "super" | "meta" | "cmd" | "command" => sup = true,
            "escape" | "esc" => key.push('⎋'),
            "left" => key.push('←'),
            "right" => key.push('→'),
            "up" => key.push('↑'),
            "down" => key.push('↓'),
            "return" | "enter" => key.push('↩'),
            "tab" => key.push('⇥'),
            _ => key.push_str(token),
        }
    }
    let mut out = String::new();
    if ctrl {
        out.push('⌃');
    }
    if alt {
        out.push('⌥');
    }
    if shift {
        out.push('⇧');
    }
    if sup {
        out.push('❖');
    }
    out.push_str(&key);
    out
}

/// Measures the width of `text` at `size` using the same font stack as menus.
pub fn measure_text(text: &str, size: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let mut paragraph_style = ParagraphStyle::default();
    let mut text_style = TextStyle::default();
    text_style.set_font_size(size);
    text_style.set_font_families(&font_families());
    paragraph_style.set_text_style(&text_style);

    let mut builder = ParagraphBuilder::new(&paragraph_style, crate::fonts::collection());
    builder.add_text(text);
    let mut paragraph = builder.build();
    paragraph.layout(f32::MAX);
    paragraph.max_intrinsic_width()
}

/// Truncates `text` in the middle with "…" so it fits `max_width`.
/// The ellipsis is always included when truncating; if even that does not fit,
/// the function falls back to the rightmost characters that do.
fn truncate_middle(text: &str, max_width: f32, font_size: f32) -> String {
    if max_width <= 0.0 || text.is_empty() || measure_text(text, font_size) <= max_width {
        return text.to_string();
    }
    let ellipsis = "…";
    let ellipsis_w = measure_text(ellipsis, font_size);
    let max_body = (max_width - ellipsis_w).max(0.0);
    let chars: Vec<char> = text.chars().collect();
    let mut left = chars.len() / 2;
    let mut right = chars.len() - left;
    while left > 0 || right > 0 {
        let body: String = chars
            .iter()
            .take(left)
            .chain(chars.iter().skip(chars.len() - right))
            .collect();
        if measure_text(&body, font_size) <= max_body {
            return format!("{body}{ellipsis}");
        }
        if left >= right && left > 0 {
            left -= 1;
        } else {
            right = right.saturating_sub(1);
        }
    }
    ellipsis.to_string()
}

/// The magnifier drawn at the start of a search field.
const SEARCH_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"6.8\" cy=\"6.8\" r=\"4.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\"/><path d=\"M10 10l3.2 3.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\"/></svg>";

/// Gap between the mark/icon column and the label, so an icon never touches
/// the text and plain entries line up with the ones that carry one.
const MARK_TEXT_GAP: f32 = 6.0;

/// State of a check/radio mark in a menu item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    /// No mark.
    #[default]
    None,
    /// Checked box with the given state.
    Checkbox(bool),
    /// Radio dot with the given state.
    Radio(bool),
}

/// Direction for root-menu keyboard navigation between menubar titles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuNavigation {
    /// Move to the previous title on the menubar.
    Prev,
    /// Move to the next title on the menubar.
    Next,
}

/// One entry in a Neptunia menu.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuItem {
    /// An action the user can trigger.
    Action {
        id: String,
        label: String,
        shortcut: Option<String>,
        enabled: bool,
        check: CheckState,
        /// Leading SVG icon, drawn in the mark column.
        icon: Option<&'static str>,
    },
    /// A nested submenu.
    Submenu {
        label: String,
        enabled: bool,
        items: Vec<MenuItem>,
        /// Leading SVG icon, drawn in the mark column.
        icon: Option<&'static str>,
    },
    /// A non-selectable group header.
    Header { label: String },
    /// A visual separator.
    Separator,
    /// A non-selectable search field. Visual only for now.
    SearchField { placeholder: String },
    /// A filtered search result carrying its menu path, source pool and, for
    /// menu entries, its location in the menubar so it can be revealed.
    SearchResult {
        path: String,
        item: Box<MenuItem>,
        source: SearchSource,
        location: Option<SearchLocation>,
    },
}

impl MenuItem {
    fn enabled(&self) -> bool {
        match self {
            MenuItem::Action { enabled, .. } => *enabled,
            MenuItem::Submenu { enabled, .. } => *enabled,
            MenuItem::SearchResult { item, .. } => item.enabled(),
            MenuItem::Header { .. } | MenuItem::Separator | MenuItem::SearchField { .. } => false,
        }
    }

    fn selectable(&self) -> bool {
        matches!(
            self,
            MenuItem::Action { .. } | MenuItem::Submenu { .. } | MenuItem::SearchResult { .. }
        ) && self.enabled()
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            MenuItem::Action { label, .. }
            | MenuItem::Submenu { label, .. }
            | MenuItem::Header { label } => Some(label),
            MenuItem::SearchResult { item, .. } => item.label(),
            MenuItem::Separator | MenuItem::SearchField { .. } => None,
        }
    }

    fn shortcut(&self) -> Option<String> {
        match self {
            MenuItem::Action { shortcut, .. } => shortcut.as_deref().map(format_shortcut),
            MenuItem::SearchResult { item, .. } => item.shortcut(),
            MenuItem::Submenu { .. }
            | MenuItem::Header { .. }
            | MenuItem::Separator
            | MenuItem::SearchField { .. } => None,
        }
    }

    fn height(&self, m: MenuPalette) -> f32 {
        match self {
            MenuItem::Separator => m.separator_height + m.separator_margin_y * 2.0,
            MenuItem::SearchField { .. } => m.search_height + m.search_margin_y * 2.0,
            MenuItem::Header { .. } => m.header_height,
            _ => m.item_height,
        }
    }
}

/// A Neptunia menu rendered inside a popup.
pub struct MenuView {
    items: Rc<Vec<MenuItem>>,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    on_navigation: Option<Rc<dyn Fn(MenuNavigation)>>,
    palette: MenuPalette,
    searchable: Option<Rc<Vec<SearchableEntry>>>,
    help_search: Option<Rc<Vec<SearchableEntry>>>,
    query_state: Option<Rc<RefCell<SearchQuery>>>,
    menu_search_icon: Option<&'static str>,
    help_search_icon: Option<&'static str>,
    popup_spec: Option<PopupSpec>,
    reveal_roots: Option<Rc<Vec<Vec<MenuItem>>>>,
}

impl MenuView {
    /// Creates a menu view. `on_dismiss_all` must close the root popup (and
    /// therefore the whole tree); every submenu reuses the same closure.
    pub fn new(
        items: Vec<MenuItem>,
        on_activate: impl Fn(String) + 'static,
        on_dismiss_all: impl Fn() + 'static,
        palette: MenuPalette,
    ) -> Self {
        Self {
            items: Rc::new(items),
            on_activate: Rc::new(on_activate),
            on_dismiss_all: Rc::new(on_dismiss_all),
            on_navigation: None,
            palette,
            searchable: None,
            help_search: None,
            query_state: None,
            menu_search_icon: None,
            help_search_icon: None,
            popup_spec: None,
            reveal_roots: None,
        }
    }

    /// Stores the original popup geometry so the view can resize itself while
    /// it is open.
    pub fn with_popup_spec(mut self, spec: PopupSpec) -> Self {
        self.popup_spec = Some(spec);
        self
    }

    /// Attaches a list of searchable menu entries. When the menu contains a
    /// [`MenuItem::SearchField`], typing filters these entries and shows them
    /// under an "Ítems del menú" header.
    pub fn with_searchable(mut self, items: Vec<SearchableEntry>) -> Self {
        self.searchable = Some(Rc::new(items));
        self
    }

    /// Attaches a list of searchable help topics shown under "Temas de ayuda".
    pub fn with_help_topics(mut self, items: Vec<SearchableEntry>) -> Self {
        self.help_search = Some(Rc::new(items));
        self
    }

    /// Sets the icons used for menu and help search results.
    ///
    /// `menu` may contain `{{text}}` and `{{accent}}` placeholders for a
    /// two-colour icon; `help` must use `currentColor`.
    pub fn with_search_icons(mut self, menu: &'static str, help: &'static str) -> Self {
        self.menu_search_icon = Some(menu);
        self.help_search_icon = Some(help);
        self
    }

    /// Sets the root menus that can be revealed when a menu search result is
    /// highlighted. The vector index matches [`SearchLocation::title_index`].
    pub fn with_reveal_roots(mut self, roots: Vec<Vec<MenuItem>>) -> Self {
        self.reveal_roots = Some(Rc::new(roots));
        self
    }

    /// Attaches an external query store so the search text survives closing and
    /// reopening the popup.
    pub fn with_query_state(mut self, state: Rc<RefCell<SearchQuery>>) -> Self {
        self.query_state = Some(state);
        self
    }

    /// Attaches a root-navigation callback.
    ///
    /// When the menu is the root of a menubar popup, `ArrowLeft` and
    /// `ArrowRight` move to the neighbouring title instead of closing the popup
    /// or doing nothing.
    pub fn with_navigation(mut self, on_navigation: impl Fn(MenuNavigation) + 'static) -> Self {
        self.on_navigation = Some(Rc::new(on_navigation));
        self
    }

    /// Returns the logical size needed to render `items` with `palette`.
    ///
    /// Labels are measured with Skia and truncated in the middle with "…" when
    /// they would exceed `panel.max_width`. Shortcuts and chevrons always stay
    /// visible and are never wrapped.
    pub fn preferred_size(items: &[MenuItem], m: MenuPalette) -> (f32, f32) {
        let inset = 2.0 * m.border_width + m.padding;
        let content_height: f32 = items.iter().map(|item| item.height(m)).sum();
        let min_width = m.min_width.max(1.0);
        let max_width = m.max_width.max(min_width);
        let max_content_width = (max_width - inset * 2.0).max(0.0);
        let min_content_width = (min_width - inset * 2.0).clamp(0.0, max_content_width);
        let mut content_width = min_content_width;

        for item in items {
            let Some(label) = item.label() else {
                continue;
            };
            let shortcut_w = item
                .shortcut()
                .map(|s| measure_text(&s, m.font_size))
                .unwrap_or(0.0);
            let trailing = if matches!(item, MenuItem::Submenu { .. }) {
                m.chevron_size
            } else if shortcut_w > 0.0 {
                m.shortcut_gap + shortcut_w
            } else {
                0.0
            };
            let fixed = m.item_padding_x * 2.0 + m.mark_width + MARK_TEXT_GAP + trailing;
            let max_label_w = (max_content_width - fixed).max(0.0);
            let label = truncate_middle(label, max_label_w, m.font_size);
            let label_w = measure_text(&label, m.font_size);
            let content_w = fixed + label_w;
            content_width = content_width.max(content_w);
        }

        let width =
            (content_width.min(max_content_width) + inset * 2.0).clamp(min_width, max_width);
        let height = content_height + inset * 2.0;
        (width.ceil(), height.ceil())
    }
}

impl Clone for MenuView {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
            on_activate: self.on_activate.clone(),
            on_dismiss_all: self.on_dismiss_all.clone(),
            on_navigation: self.on_navigation.clone(),
            palette: self.palette,
            searchable: self.searchable.clone(),
            help_search: self.help_search.clone(),
            query_state: self.query_state.clone(),
            menu_search_icon: self.menu_search_icon,
            help_search_icon: self.help_search_icon,
            popup_spec: self.popup_spec.clone(),
            reveal_roots: self.reveal_roots.clone(),
        }
    }
}

impl PartialEq for MenuView {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl App for MenuView {
    fn render(&self) -> impl IntoElement {
        let items = self.items.clone();
        let on_activate = self.on_activate.clone();
        let on_dismiss_all = self.on_dismiss_all.clone();
        let on_navigation = self.on_navigation.clone();
        let m = self.palette;
        // The popup may have grown or shrunk after a reposition, so the tree
        // follows the configured surface instead of the size at creation.
        let platform = Platform::get();
        let scale = (*platform.scale_factor.peek()).max(f64::EPSILON);
        let root_size = *platform.root_size.peek();
        let size = (
            (root_size.width / scale as f32).max(1.0),
            (root_size.height / scale as f32).max(1.0),
        );
        let popup: Option<PopupHandle> = try_consume_root_context();
        let opens_left = popup.as_ref().is_some_and(PopupHandle::opens_left);
        let menu_search_icon = self.menu_search_icon;
        let help_search_icon = self.help_search_icon;
        let popup_spec = self.popup_spec.clone();
        let focused = use_state(|| first_selectable(&items));
        let open_child_id = use_hook(|| Rc::new(Cell::new(None)));
        let query_state = self.query_state.clone();
        let initial = query_state
            .as_ref()
            .map(|q| q.borrow().clone())
            .unwrap_or_default();
        let query = use_state(|| initial.text);
        let selected = use_state(|| initial.selected);
        let result_focused = use_state(|| None::<usize>);
        let prev_query = use_hook(|| Rc::new(RefCell::new(String::new())));
        let searchable = self.searchable.clone();
        let help_search = self.help_search.clone();

        let has_search = searchable.is_some() || help_search.is_some();
        let query_text = (*query.read()).clone();
        let is_selected = *selected.read();

        // Sync the external query store so reopening keeps the text.
        if let Some(state) = &query_state {
            let current = SearchQuery {
                text: query_text.clone(),
                selected: is_selected,
            };
            if state.borrow().text != current.text || state.borrow().selected != current.selected {
                *state.borrow_mut() = current;
            }
        }
        let visible = visible_items(
            &items,
            searchable.as_deref().map(|v| v.as_slice()),
            help_search.as_deref().map(|v| v.as_slice()),
            &query_text,
        );

        // Resize the popup when the query changes so the results stay visible.
        if prev_query.borrow().as_str() != query_text {
            *prev_query.borrow_mut() = query_text.clone();
            if let (Some(popup), Some(spec)) = (&popup, &popup_spec) {
                let new_size = MenuView::preferred_size(&visible, m);
                popup.reposition(PopupSpec {
                    size: (new_size.0 as u32, new_size.1 as u32),
                    ..spec.clone()
                });
            }
        }

        let results_start = visible
            .iter()
            .position(|item| matches!(item, MenuItem::SearchResult { .. }));
        let results_count = if let Some(start) = results_start {
            visible
                .iter()
                .skip(start)
                .take_while(|item| matches!(item, MenuItem::SearchResult { .. }))
                .count()
        } else {
            0
        };

        let focused_index = *focused.read();
        let result_focused_index = *result_focused.read();
        let reveal_child_id = use_hook(|| Rc::new(Cell::new(None::<PopupId>)));
        let prev_revealed = use_hook(|| Rc::new(Cell::new(None::<usize>)));
        let reveal_roots = self.reveal_roots.clone();

        let revealed_title = result_focused_index.and_then(|i| {
            visible.get(i).and_then(|item| match item {
                MenuItem::SearchResult {
                    source: SearchSource::Menu,
                    location: Some(loc),
                    ..
                } => Some(loc.title_index),
                _ => None,
            })
        });
        if revealed_title != prev_revealed.get() {
            prev_revealed.set(revealed_title);
            close_child(reveal_child_id.clone());
            if let (Some(title_index), Some(roots), Some(popup)) =
                (revealed_title, reveal_roots.as_ref(), popup.clone())
                && let Some(items) = roots.get(title_index)
            {
                open_reveal_popup(
                    items,
                    popup,
                    size,
                    m,
                    on_activate.clone(),
                    on_dismiss_all.clone(),
                    reveal_child_id.clone(),
                );
            }
        }

        let rows: Vec<Element> = visible
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let is_focused = if !query_text.is_empty() && results_start.is_some() {
                    result_focused_index == Some(index)
                } else {
                    focused_index == Some(index)
                };
                render_item(
                    item,
                    index,
                    is_focused,
                    m,
                    popup.clone(),
                    opens_left,
                    on_activate.clone(),
                    on_dismiss_all.clone(),
                    Rc::new(visible.clone()),
                    focused,
                    open_child_id.clone(),
                    size,
                    query,
                    selected,
                    menu_search_icon,
                    help_search_icon,
                )
            })
            .collect();

        let inner_padding = (m.padding - m.border_width).max(0.0);
        let content = rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new(
                inner_padding,
                inner_padding,
                inner_padding,
                inner_padding,
            ))
            .background(rgba(m.bg))
            .corner_radius((m.radius - m.border_width).max(0.0))
            .border(
                Border::new()
                    .width(m.border_width)
                    .alignment(BorderAlignment::Inner)
                    .fill(rgba(m.inner_line)),
            )
            .direction(Direction::Vertical)
            .children(rows);

        let mut root = rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(rgba(m.bg))
            .corner_radius(m.radius)
            .overflow(Overflow::Clip)
            .border(
                Border::new()
                    .width(m.border_width)
                    .alignment(BorderAlignment::Inner)
                    .fill(rgba(m.outer_border)),
            )
            .padding(Gaps::new(
                m.border_width,
                m.border_width,
                m.border_width,
                m.border_width,
            ))
            .child(content);
        for family in SANS_FAMILIES {
            root = root.font_family(family);
        }
        let visible_for_keys = Rc::new(visible.clone());
        root.on_global_key_down(move |event: Event<KeyboardEventData>| {
            let mut focused = focused;
            let mut query = query;
            let mut result_focused = result_focused;
            let mut selected = selected;
            let open_child_id = open_child_id.clone();
            let current = *focused.read();
            let query_text = (*query.read()).clone();
            let result_current = *result_focused.read();
            let searching = has_search && !query_text.is_empty();
            let is_selected = *selected.read();
            match event.key {
                Key::Named(NamedKey::ArrowDown) => {
                    selected.set(false);
                    if searching {
                        if results_count > 0 {
                            let start = results_start.unwrap();
                            let next = result_current
                                .map_or(start, |i| (i + 1).min(start + results_count - 1));
                            result_focused.set(Some(next));
                        }
                    } else {
                        focused.set(next_selectable(&items, current, 1));
                    }
                }
                Key::Named(NamedKey::ArrowUp) => {
                    selected.set(false);
                    if searching {
                        if results_count > 0 {
                            let start = results_start.unwrap();
                            let prev = result_current.map_or(start + results_count - 1, |i| {
                                i.saturating_sub(1).max(start)
                            });
                            result_focused.set(Some(prev));
                        }
                    } else {
                        focused.set(next_selectable(&items, current, -1));
                    }
                }
                Key::Named(NamedKey::Home) => {
                    if !searching {
                        focused.set(first_selectable(&items));
                    }
                }
                Key::Named(NamedKey::End) => {
                    if !searching {
                        focused.set(last_selectable(&items));
                    }
                }
                Key::Named(NamedKey::Enter) => {
                    selected.set(false);
                    let (index, source): (Option<usize>, &[MenuItem]) = if searching {
                        (result_current, visible_for_keys.as_slice())
                    } else {
                        (current, items.as_slice())
                    };
                    if let Some(index) = index {
                        activate_or_open(
                            source,
                            index,
                            popup.clone(),
                            size,
                            m,
                            on_activate.clone(),
                            on_dismiss_all.clone(),
                            open_child_id,
                            opens_left,
                        );
                    }
                }
                Key::Character(ref ch) if ch == " " => {
                    selected.set(false);
                    let (index, source): (Option<usize>, &[MenuItem]) = if searching {
                        (result_current, visible_for_keys.as_slice())
                    } else {
                        (current, items.as_slice())
                    };
                    if let Some(index) = index {
                        activate_or_open(
                            source,
                            index,
                            popup.clone(),
                            size,
                            m,
                            on_activate.clone(),
                            on_dismiss_all.clone(),
                            open_child_id,
                            opens_left,
                        );
                    }
                }
                Key::Character(ref ch) if has_search => {
                    let mut text = query_text;
                    if is_selected {
                        text.clear();
                        selected.set(false);
                    }
                    text.push_str(ch);
                    query.set(text);
                    result_focused.set(results_start);
                }
                Key::Named(NamedKey::Backspace) if has_search => {
                    if is_selected {
                        query.set(String::new());
                        selected.set(false);
                    } else {
                        let mut text = query_text;
                        text.pop();
                        query.set(text);
                    }
                    result_focused.set(results_start);
                }
                Key::Named(NamedKey::Escape) if has_search => {
                    if query_text.is_empty() {
                        if let Some(popup) = &popup {
                            popup.close();
                        }
                    } else {
                        query.set(String::new());
                        selected.set(false);
                        result_focused.set(None);
                        focused.set(first_selectable(&items));
                    }
                }
                Key::Named(NamedKey::ArrowRight) => {
                    if searching {
                        return;
                    }
                    if let Some(index) = current {
                        if let Some(MenuItem::Submenu { items: sub, .. }) = items.get(index)
                            && let Some(popup) = popup.clone()
                        {
                            open_child_popup(
                                index,
                                sub,
                                popup,
                                size,
                                m,
                                on_activate.clone(),
                                on_dismiss_all.clone(),
                                open_child_id,
                                Rc::new(items.to_vec()),
                                opens_left,
                            );
                        } else if popup.is_some()
                            && let Some(on_navigation) = &on_navigation
                        {
                            on_navigation(MenuNavigation::Next);
                        }
                    }
                }
                Key::Named(NamedKey::ArrowLeft) if popup.is_some() && on_navigation.is_some() => {
                    on_navigation.as_ref().unwrap()(MenuNavigation::Prev);
                }
                Key::Named(NamedKey::ArrowLeft) if !searching => {
                    if let Some(popup) = &popup {
                        popup.close();
                    }
                }
                // Esc is handled compositor-side: it dismisses the topmost
                // popup and sends `popup_done`. The client must not close an
                // extra level here. See `State::dismiss_topmost_popup`.
                _ => {}
            }
        })
    }
}

fn rgba(color: Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn rgba_to_hex(color: Rgba) -> String {
    let [r, g, b, a] = color.to_rgba8();
    format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
}

fn shortcut_fg(enabled: bool, focused: bool, m: MenuPalette) -> Rgba {
    if !enabled {
        m.disabled_fg
    } else if focused {
        m.hover_fg
    } else {
        m.shortcut_color
    }
}

fn chevron_fg(enabled: bool, focused: bool, m: MenuPalette) -> Rgba {
    if !enabled {
        m.disabled_fg
    } else if focused {
        m.hover_fg
    } else {
        m.chevron_color
    }
}

fn first_selectable(items: &[MenuItem]) -> Option<usize> {
    items.iter().position(|item| item.selectable())
}

fn last_selectable(items: &[MenuItem]) -> Option<usize> {
    items.iter().rposition(|item| item.selectable())
}

fn next_selectable(items: &[MenuItem], current: Option<usize>, delta: isize) -> Option<usize> {
    let len = items.len();
    if len == 0 {
        return None;
    }
    let start = current
        .map(|i| (i as isize + delta).rem_euclid(len as isize) as usize)
        .unwrap_or_else(|| if delta > 0 { 0 } else { len - 1 });
    let mut index = start;
    loop {
        if items[index].selectable() {
            return Some(index);
        }
        index = (index as isize + delta.signum()).rem_euclid(len as isize) as usize;
        if index == start {
            return None;
        }
    }
}

fn activate_at(items: &[MenuItem], index: usize) -> Option<String> {
    match items.get(index)? {
        MenuItem::Action { id, enabled, .. } if *enabled => Some(id.clone()),
        MenuItem::SearchResult { item, .. } => activate_at(std::slice::from_ref(item), 0),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn activate_or_open(
    items: &[MenuItem],
    index: usize,
    popup: Option<PopupHandle>,
    size: (f32, f32),
    m: MenuPalette,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    open_child_id: Rc<Cell<Option<PopupId>>>,
    opens_left: bool,
) {
    if let Some(id) = activate_at(items, index) {
        on_activate(id);
        on_dismiss_all();
    } else if let Some(MenuItem::Submenu { items: sub, .. }) = items.get(index)
        && let Some(popup) = popup
    {
        open_child_popup(
            index,
            sub,
            popup,
            size,
            m,
            on_activate,
            on_dismiss_all,
            open_child_id,
            Rc::new(items.to_vec()),
            opens_left,
        );
    }
}

/// Removes accents and lowercases a query so searches are case and accent
/// insensitive.
fn normalize_query(text: &str) -> String {
    text.to_lowercase()
        .replace('á', "a")
        .replace('é', "e")
        .replace('í', "i")
        .replace('ó', "o")
        .replace(['ú', 'ü'], "u")
        .replace('ñ', "n")
        .replace('Á', "a")
        .replace('É', "e")
        .replace('Í', "i")
        .replace('Ó', "o")
        .replace(['Ú', 'Ü'], "u")
        .replace('Ñ', "n")
}

fn filter_search_results(query: &str, searchable: &[SearchableEntry]) -> Vec<SearchableEntry> {
    let normalized = normalize_query(query);
    searchable
        .iter()
        .filter(|entry| {
            entry
                .item
                .label()
                .is_some_and(|label| normalize_query(label).contains(&normalized))
        })
        .cloned()
        .collect()
}

/// Builds the items actually rendered: the original list when there is no
/// active query, or the search field + results + the original help items when
/// there is.
fn visible_items(
    items: &[MenuItem],
    searchable: Option<&[SearchableEntry]>,
    help_search: Option<&[SearchableEntry]>,
    query: &str,
) -> Vec<MenuItem> {
    let Some(searchable) = searchable else {
        return items.to_vec();
    };
    if query.is_empty() {
        return items.to_vec();
    }
    let mut out = Vec::new();
    let mut saw_search = false;
    for item in items {
        if matches!(item, MenuItem::SearchField { .. }) {
            out.push(item.clone());
            saw_search = true;
        }
    }
    if !saw_search {
        // Defensive: if the caller forgot the search field, add one.
        out.push(MenuItem::SearchField {
            placeholder: "Buscar".to_string(),
        });
    }

    let menu_results = filter_search_results(query, searchable);
    let help_results = help_search
        .map(|list| filter_search_results(query, list))
        .unwrap_or_default();

    if !menu_results.is_empty() {
        out.push(MenuItem::Header {
            label: "Ítems del menú".to_string(),
        });
        for entry in menu_results {
            out.push(MenuItem::SearchResult {
                path: entry.path,
                item: Box::new(entry.item),
                source: SearchSource::Menu,
                location: entry.location,
            });
        }
    }

    if !help_results.is_empty() {
        out.push(MenuItem::Header {
            label: "Temas de ayuda".to_string(),
        });
        for entry in help_results {
            out.push(MenuItem::SearchResult {
                path: entry.path,
                item: Box::new(entry.item),
                source: SearchSource::Help,
                location: None,
            });
        }
    }

    // The "Show all help topics" footer is always present when searching.
    if help_search.is_some() {
        out.push(MenuItem::Separator);
        out.push(MenuItem::SearchResult {
            path: "Temas de ayuda".to_string(),
            item: Box::new(MenuItem::Action {
                id: "app-help-topics-index".to_string(),
                label: "Mostrar todos los temas de ayuda".to_string(),
                shortcut: None,
                enabled: true,
                check: CheckState::None,
                icon: None,
            }),
            source: SearchSource::Help,
            location: None,
        });
    }

    out.push(MenuItem::Separator);
    for item in items {
        if !matches!(item, MenuItem::SearchField { .. }) {
            out.push(item.clone());
        }
    }
    out
}

fn item_top(index: usize, items: &[MenuItem], m: MenuPalette) -> f32 {
    let mut y = 0.0;
    for (i, item) in items.iter().enumerate() {
        if i == index {
            return y;
        }
        y += item.height(m);
    }
    0.0
}

/// Which side a submenu opens to. A popup configured left of its parent
/// (`opens_left`) asks its children to open left too; `flip_x` lets the
/// compositor return them to the right only when the left side does not fit.
fn submenu_positioner(opens_left: bool) -> (PopupAnchor, PopupGravity, i32) {
    // The compositor does not invert the offset when it flips the popup, so a
    // non-zero horizontal offset would make one side overlap its ancestor.
    // The anchor is the panel edge (see `submenu_anchor_rect`); zero leaves
    // the child touching the parent on both sides.
    if opens_left {
        (PopupAnchor::TopLeft, PopupGravity::BottomLeft, 0)
    } else {
        (PopupAnchor::TopRight, PopupGravity::BottomRight, 0)
    }
}

/// The anchor rectangle of a submenu: the panel's full width at the row's
/// surface-local top. The horizontal anchor is the panel edge so a flipped
/// child touches the parent edge instead of covering it; the vertical anchor
/// is the top of the highlighted row, so the child's first row can align with
/// it after offsetting for the child's own panel inset.
fn submenu_anchor_rect(
    index: usize,
    items: &[MenuItem],
    m: MenuPalette,
    parent_width: f32,
) -> (i32, i32, u32, u32) {
    (
        0,
        (item_top(index, items, m) + m.padding) as i32,
        parent_width.max(1.0) as u32,
        m.item_height as u32,
    )
}

fn close_child(open_child_id: Rc<Cell<Option<PopupId>>>) {
    if let Some(id) = open_child_id.get() {
        if let Some(popups) = try_consume_root_context::<Popups>() {
            popups.close(id);
        }
        open_child_id.set(None);
    }
}

#[allow(clippy::too_many_arguments)]
fn open_child_popup(
    index: usize,
    sub_items: &[MenuItem],
    popup: PopupHandle,
    parent_size: (f32, f32),
    m: MenuPalette,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    open_child_id: Rc<Cell<Option<PopupId>>>,
    parent_items: Rc<Vec<MenuItem>>,
    opens_left: bool,
) -> Option<PopupId> {
    close_child(open_child_id.clone());
    let child_size = MenuView::preferred_size(sub_items, m);
    let on_dismiss_for_action = on_dismiss_all.clone();
    let child_view = MenuView::new(
        sub_items.to_vec(),
        move |id| {
            on_activate(id);
            on_dismiss_for_action();
        },
        move || on_dismiss_all(),
        m,
    );
    let open_child_id_set = open_child_id.clone();
    let on_dismissed = move || {
        open_child_id_set.set(None);
    };
    let inset = m.padding + m.border_width;
    let (anchor, gravity, dx) = submenu_positioner(opens_left);
    let spec = PopupSpec {
        anchor_rect: submenu_anchor_rect(index, &parent_items, m, parent_size.0),
        anchor,
        gravity,
        offset: (dx, -(inset as i32)),
        size: (child_size.0 as u32, child_size.1 as u32),
        constraint_adjustment: PopupConstraintAdjustment {
            flip_x: true,
            slide_y: true,
            ..PopupConstraintAdjustment::default()
        },
        grab: true,
        on_dismissed: Some(Rc::new(on_dismissed)),
    };
    let id = popup.open_child(spec, AppComponent::new(child_view));
    open_child_id.set(Some(id));
    Some(id)
}

fn open_reveal_popup(
    items: &[MenuItem],
    popup: PopupHandle,
    parent_size: (f32, f32),
    m: MenuPalette,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    reveal_child_id: Rc<Cell<Option<PopupId>>>,
) -> Option<PopupId> {
    close_child(reveal_child_id.clone());
    let child_size = MenuView::preferred_size(items, m);
    let on_dismiss_for_action = on_dismiss_all.clone();
    let child_view = MenuView::new(
        items.to_vec(),
        move |id| {
            on_activate(id);
            on_dismiss_for_action();
        },
        move || on_dismiss_all(),
        m,
    );
    let reveal_child_id_set = reveal_child_id.clone();
    let on_dismissed = move || {
        reveal_child_id_set.set(None);
    };
    let inset = m.padding + m.border_width;
    let spec = PopupSpec {
        anchor_rect: ((parent_size.0 - inset) as i32, inset as i32, 1, 1),
        anchor: PopupAnchor::TopRight,
        gravity: PopupGravity::TopLeft,
        offset: (0, 0),
        size: (child_size.0 as u32, child_size.1 as u32),
        constraint_adjustment: PopupConstraintAdjustment {
            flip_x: true,
            slide_y: true,
            ..PopupConstraintAdjustment::default()
        },
        grab: true,
        on_dismissed: Some(Rc::new(on_dismissed)),
    };
    let id = popup.open_child(spec, AppComponent::new(child_view));
    reveal_child_id.set(Some(id));
    Some(id)
}

#[allow(clippy::too_many_arguments)]
fn render_item(
    item: &MenuItem,
    index: usize,
    is_focused: bool,
    m: MenuPalette,
    popup: Option<PopupHandle>,
    opens_left: bool,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    items: Rc<Vec<MenuItem>>,
    mut focused: UseState<Option<usize>>,
    open_child_id: Rc<Cell<Option<PopupId>>>,
    size: (f32, f32),
    query: UseState<String>,
    selected: UseState<bool>,
    menu_search_icon: Option<&'static str>,
    help_search_icon: Option<&'static str>,
) -> Element {
    match item {
        MenuItem::Separator => rect()
            .width(Size::fill())
            .height(Size::px(m.separator_height))
            .margin(Gaps::new(
                m.separator_margin_y,
                0.0,
                m.separator_margin_y,
                0.0,
            ))
            .background(rgba(m.separator_color))
            .into_element(),
        MenuItem::Action {
            id,
            label: label_text,
            shortcut: shortcut_text,
            enabled,
            check,
            icon,
        } => {
            let id = id.clone();
            let label_text = label_text.clone();
            let shortcut_text = shortcut_text.as_ref().map(|s| format_shortcut(s));
            let enabled = *enabled;
            let check = *check;
            let icon = *icon;
            let mut focused = focused;
            let open_child_id_enter = open_child_id.clone();
            let shortcut_w = shortcut_text
                .as_ref()
                .map(|s| measure_text(s, m.font_size))
                .unwrap_or(0.0);
            let inset = 2.0 * m.border_width + m.padding;
            let max_label_w = (m.max_width
                - inset * 2.0
                - m.item_padding_x * 2.0
                - m.mark_width
                - MARK_TEXT_GAP
                - if shortcut_w > 0.0 {
                    m.shortcut_gap + shortcut_w
                } else {
                    0.0
                })
            .max(0.0);
            let label_text = truncate_middle(&label_text, max_label_w, m.font_size);
            let fg = if enabled {
                if is_focused { m.hover_fg } else { m.fg }
            } else {
                m.disabled_fg
            };
            let shortcut_color = shortcut_fg(enabled, is_focused, m);
            let bg = if is_focused && enabled {
                Some(m.hover_bg)
            } else {
                None
            };
            let mut row = rect()
                .width(Size::fill())
                .height(Size::px(m.item_height))
                .padding(Gaps::new(0.0, m.item_padding_x, 0.0, m.item_padding_x))
                .corner_radius(m.item_radius)
                .direction(Direction::Horizontal)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::Center)
                .on_pointer_enter({
                    let open_child_id_enter = open_child_id_enter.clone();
                    move |_| {
                        if enabled {
                            focused.set(Some(index));
                            close_child(open_child_id_enter.clone());
                        }
                    }
                })
                .on_pointer_move({
                    let open_child_id_enter = open_child_id_enter.clone();
                    move |_| {
                        if enabled {
                            focused.set(Some(index));
                            close_child(open_child_id_enter.clone());
                        }
                    }
                })
                .on_mouse_down(move |_: Event<MouseEventData>| {
                    if enabled {
                        on_activate(id.clone());
                        on_dismiss_all();
                    }
                });
            if let Some(bg) = bg {
                row = row.background(rgba(bg));
            }
            let left = rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .spacing(MARK_TEXT_GAP)
                .child(mark_slot(
                    icon,
                    check,
                    fg,
                    shortcut_fg(enabled, is_focused, m),
                    m,
                ))
                .child(
                    label()
                        .text(label_text)
                        .font_size(m.font_size)
                        .color(rgba(fg))
                        .max_lines(1),
                );
            row.child(left)
                .child(
                    label()
                        .text(shortcut_text.unwrap_or_default())
                        .font_size(m.font_size)
                        .color(rgba(shortcut_color))
                        .width(Size::px(shortcut_w))
                        .max_lines(1),
                )
                .into_element()
        }
        MenuItem::Submenu {
            label: label_text,
            enabled,
            items: sub_items,
            icon,
        } => {
            let label_text = label_text.clone();
            let enabled = *enabled;
            let icon = *icon;
            let mut focused = focused;
            let sub_items_enter = sub_items.clone();
            let sub_items_click = sub_items.clone();
            let fg = if enabled {
                if is_focused { m.hover_fg } else { m.fg }
            } else {
                m.disabled_fg
            };
            let bg = if is_focused && enabled {
                Some(m.hover_bg)
            } else {
                None
            };
            let popup_enter = popup.clone();
            let popup_click = popup.clone();
            let on_activate_enter = on_activate.clone();
            let on_activate_click = on_activate.clone();
            let on_dismiss_enter = on_dismiss_all.clone();
            let on_dismiss_click = on_dismiss_all.clone();
            let items_enter = items.clone();
            let items_click = items.clone();
            let open_child_id_enter = open_child_id.clone();
            let open_child_id_click = open_child_id.clone();
            let mut row = rect()
                .width(Size::fill())
                .height(Size::px(m.item_height))
                .padding(Gaps::new(0.0, m.item_padding_x, 0.0, m.item_padding_x))
                .corner_radius(m.item_radius)
                .direction(Direction::Horizontal)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::Center)
                .on_pointer_enter(move |_| {
                    if enabled {
                        focused.set(Some(index));
                    }
                    if enabled && let Some(popup) = popup_enter.clone() {
                        open_child_popup(
                            index,
                            &sub_items_enter,
                            popup,
                            size,
                            m,
                            on_activate_enter.clone(),
                            on_dismiss_enter.clone(),
                            open_child_id_enter.clone(),
                            items_enter.clone(),
                            opens_left,
                        );
                    }
                })
                .on_mouse_down(move |_: Event<MouseEventData>| {
                    if enabled && let Some(popup) = popup_click.clone() {
                        open_child_popup(
                            index,
                            &sub_items_click,
                            popup,
                            size,
                            m,
                            on_activate_click.clone(),
                            on_dismiss_click.clone(),
                            open_child_id_click.clone(),
                            items_click.clone(),
                            opens_left,
                        );
                    }
                });
            if let Some(bg) = bg {
                row = row.background(rgba(bg));
            }
            let left = rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .spacing(MARK_TEXT_GAP)
                .child(mark_slot(
                    icon,
                    CheckState::None,
                    fg,
                    shortcut_fg(enabled, is_focused, m),
                    m,
                ))
                .child(
                    label()
                        .text(label_text)
                        .font_size(m.font_size)
                        .color(rgba(fg))
                        .max_lines(1),
                );
            row.child(left)
                .child(chevron_element(chevron_fg(enabled, is_focused, m), m))
                .into_element()
        }
        MenuItem::Header { label: header } => rect()
            .width(Size::fill())
            .height(Size::px(m.header_height))
            .padding(Gaps::new(0.0, m.item_padding_x, 0.0, m.item_padding_x))
            .direction(Direction::Vertical)
            .main_align(Alignment::End)
            .cross_align(Alignment::Start)
            .child(
                label()
                    .text(header.clone())
                    .font_size(m.header_font_size)
                    .font_weight(m.header_weight)
                    .color(rgba(m.disabled_fg))
                    .max_lines(1),
            )
            .into_element(),
        MenuItem::SearchField { placeholder } => {
            let mut query = query;
            let mut selected = selected;
            let query_text = (*query.read()).clone();
            let is_selected = *selected.read();
            let has_text = !query_text.is_empty();
            let display_text = if has_text {
                query_text.clone()
            } else {
                placeholder.clone()
            };
            let text_color = if has_text { m.fg } else { m.shortcut_color };
            let text_w = measure_text(&display_text, m.font_size);
            let clear_size = 16.0f32;
            let clear_gap = if has_text { clear_size + 4.0 } else { 0.0 };
            let row_padding = m.item_padding_x;
            let available_w = m.max_width - row_padding * 2.0 - m.icon_size - 8.0 - clear_gap - 8.0;
            let show_text_w = text_w.min(available_w.max(0.0));

            let mut text_box = rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .width(Size::px(show_text_w))
                .child(
                    label()
                        .text(display_text)
                        .font_size(m.font_size)
                        .color(rgba(text_color))
                        .max_lines(1),
                );

            if has_text && !is_selected {
                text_box = text_box.child(
                    rect()
                        .width(Size::px(1.0))
                        .height(Size::px(m.font_size))
                        .margin(Gaps::new(0.0, 1.0, 0.0, 0.0))
                        .background(rgba(m.fg))
                        .into_element(),
                );
            }

            let mut content = rect()
                .width(Size::fill())
                .height(Size::px(m.search_height))
                .padding(Gaps::new(0.0, row_padding, 0.0, row_padding))
                .background(rgba(m.search_bg))
                .corner_radius(m.search_radius)
                .border(
                    Border::new()
                        .width(m.search_ring_width)
                        .alignment(BorderAlignment::Inner)
                        .fill(rgba(m.search_ring)),
                )
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .spacing(8.0)
                .child(
                    SvgViewer::new(SEARCH_ICON.as_bytes())
                        .width(Size::px(m.icon_size))
                        .height(Size::px(m.icon_size))
                        .color(rgba(m.shortcut_color))
                        .show_loader(false)
                        .into_element(),
                )
                .child(text_box);

            if has_text {
                let clear = rect()
                    .width(Size::px(clear_size))
                    .height(Size::px(clear_size))
                    .corner_radius(clear_size / 2.0)
                    .main_align(Alignment::Center)
                    .cross_align(Alignment::Center)
                    .background(rgba(m.shortcut_color.mix(m.bg, 0.7)))
                    .child(
                        label()
                            .text("×")
                            .font_size(12.0)
                            .color(rgba(m.fg))
                            .max_lines(1),
                    )
                    .on_mouse_down(move |_: Event<MouseEventData>| {
                        query.set(String::new());
                        selected.set(false);
                    });
                content = content.child(clear);
            }

            content
                .margin(Gaps::new(m.search_margin_y, 0.0, m.search_margin_y, 0.0))
                .into_element()
        }
        MenuItem::SearchResult {
            path: _,
            item,
            source,
            location: _,
        } => {
            let MenuItem::Action {
                id,
                label: label_text,
                shortcut: shortcut_text,
                enabled,
                ..
            } = item.as_ref()
            else {
                return rect().into_element();
            };
            let id = id.clone();
            let label_text = label_text.clone();
            let shortcut_text = shortcut_text.as_ref().map(|s| format_shortcut(s));
            let enabled = *enabled;
            let fg = if enabled {
                if is_focused { m.hover_fg } else { m.fg }
            } else {
                m.disabled_fg
            };
            let shortcut_color = shortcut_fg(enabled, is_focused, m);
            let bg = if is_focused && enabled {
                Some(m.hover_bg)
            } else {
                None
            };
            let shortcut_w = shortcut_text
                .as_ref()
                .map(|s| measure_text(s, m.font_size))
                .unwrap_or(0.0);
            let mut row = rect()
                .width(Size::fill())
                .height(Size::px(m.item_height))
                .padding(Gaps::new(0.0, m.item_padding_x, 0.0, m.item_padding_x))
                .corner_radius(m.item_radius)
                .direction(Direction::Horizontal)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::Center)
                .on_pointer_enter(move |_| {
                    if enabled {
                        focused.set(Some(index));
                    }
                })
                .on_pointer_move(move |_| {
                    if enabled {
                        focused.set(Some(index));
                    }
                })
                .on_mouse_down(move |_: Event<MouseEventData>| {
                    if enabled {
                        on_activate(id.clone());
                        on_dismiss_all();
                    }
                });
            if let Some(bg) = bg {
                row = row.background(rgba(bg));
            }
            let icon_element = search_icon_element(
                *source,
                is_focused,
                fg,
                m.hover_bg,
                menu_search_icon,
                help_search_icon,
                m,
            );
            let left = rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .spacing(MARK_TEXT_GAP)
                .child(icon_element)
                .child(
                    label()
                        .text(label_text)
                        .font_size(m.font_size)
                        .color(rgba(fg))
                        .max_lines(1),
                );
            row.child(left)
                .child(
                    label()
                        .text(shortcut_text.unwrap_or_default())
                        .font_size(m.font_size)
                        .color(rgba(shortcut_color))
                        .width(Size::px(shortcut_w))
                        .max_lines(1),
                )
                .into_element()
        }
    }
}

fn search_icon_element(
    source: SearchSource,
    focused: bool,
    fg: Rgba,
    accent: Rgba,
    menu_icon: Option<&'static str>,
    help_icon: Option<&'static str>,
    m: MenuPalette,
) -> Element {
    match source {
        SearchSource::Menu => {
            let svg = menu_icon.unwrap_or("").to_string();
            let text_color = if focused {
                rgba_to_hex(m.hover_fg)
            } else {
                rgba_to_hex(fg)
            };
            let accent_color = if focused {
                rgba_to_hex(m.hover_fg)
            } else {
                rgba_to_hex(accent)
            };
            let svg = svg
                .replace("{{text}}", &text_color)
                .replace("{{accent}}", &accent_color);
            rect()
                .width(Size::px(16.0))
                .direction(Direction::Horizontal)
                .main_align(Alignment::Center)
                .child(
                    SvgViewer::new(freya_core::prelude::Bytes::from(svg.into_bytes()))
                        .width(Size::px(16.0))
                        .height(Size::px(14.0))
                        .show_loader(false)
                        .into_element(),
                )
                .into_element()
        }
        SearchSource::Help => {
            let svg = help_icon.unwrap_or("").to_string();
            let color = if focused { m.hover_fg } else { fg };
            rect()
                .width(Size::px(16.0))
                .direction(Direction::Horizontal)
                .main_align(Alignment::Center)
                .child(
                    SvgViewer::new(freya_core::prelude::Bytes::from(svg.into_bytes()))
                        .width(Size::px(16.0))
                        .height(Size::px(16.0))
                        .color(rgba(color))
                        .show_loader(false)
                        .into_element(),
                )
                .into_element()
        }
    }
}

fn mark_slot(
    icon: Option<&'static str>,
    check: CheckState,
    fg: Rgba,
    icon_fg: Rgba,
    m: MenuPalette,
) -> Element {
    let Some(icon) = icon else {
        return mark_element(check, fg, m);
    };
    rect()
        .width(Size::px(m.mark_width))
        .direction(Direction::Horizontal)
        .main_align(Alignment::Center)
        .child(
            SvgViewer::new(icon.as_bytes())
                .width(Size::px(m.icon_size))
                .height(Size::px(m.icon_size))
                .color(rgba(icon_fg))
                .show_loader(false)
                .into_element(),
        )
        .into_element()
}

fn mark_element(check: CheckState, color: Rgba, m: MenuPalette) -> Element {
    match check {
        CheckState::None | CheckState::Checkbox(false) | CheckState::Radio(false) => {
            rect().width(Size::px(m.mark_width)).into_element()
        }
        CheckState::Checkbox(true) => label()
            .text("✓")
            .font_size(m.mark_check)
            .color(rgba(color))
            .width(Size::px(m.mark_width))
            .text_align(TextAlign::Center)
            .max_lines(1)
            .into_element(),
        CheckState::Radio(true) => rect()
            .width(Size::px(m.mark_width))
            .direction(Direction::Horizontal)
            .main_align(Alignment::Center)
            .child(
                rect()
                    .width(Size::px(m.mark_dot))
                    .height(Size::px(m.mark_dot))
                    .corner_radius(m.mark_dot / 2.0)
                    .background(rgba(color)),
            )
            .into_element(),
    }
}

fn chevron_element(color: Rgba, m: MenuPalette) -> Element {
    label()
        .text("›")
        .font_size(m.font_size)
        .color(rgba(color))
        .width(Size::px(m.chevron_size))
        .max_lines(1)
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use yuki_design::{MENU_DARK, MENU_LIGHT};

    fn sample() -> Vec<MenuItem> {
        vec![
            MenuItem::Action {
                id: "new".to_string(),
                label: "New Window".to_string(),
                shortcut: Some("Ctrl+N".to_string()),
                enabled: true,
                check: CheckState::None,
                icon: None,
            },
            MenuItem::Separator,
            MenuItem::Action {
                id: "open".to_string(),
                label: "Open".to_string(),
                shortcut: None,
                enabled: false,
                check: CheckState::Checkbox(true),
                icon: None,
            },
            MenuItem::Submenu {
                label: "Recent".to_string(),
                enabled: true,
                icon: None,
                items: vec![MenuItem::Action {
                    id: "r1".to_string(),
                    label: "a.txt".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::None,
                    icon: None,
                }],
            },
        ]
    }

    #[test]
    fn preferred_size_is_positive() {
        let (w, h) = MenuView::preferred_size(&sample(), MENU_LIGHT);
        assert!(w > 0.0);
        assert!(h > 0.0);
    }

    #[test]
    fn root_box_follows_the_configured_popup_size() {
        use freya_testing::TestingRunner;
        use torin::prelude::Size2D;

        let items: Vec<MenuItem> = std::iter::once(MenuItem::SearchField {
            placeholder: "Buscar".to_string(),
        })
        .chain((0..3).map(|i| MenuItem::Action {
            id: format!("item-{i}"),
            label: format!("Item {i}"),
            shortcut: None,
            enabled: true,
            check: CheckState::None,
            icon: None,
        }))
        .collect();
        assert_eq!(MenuView::preferred_size(&items, MENU_LIGHT), (220.0, 120.0));

        // The popup was created at its natural 220 x 120 and the configure
        // after a reposition gave it 340 x 300.
        let view = MenuView::new(items, |_| {}, || {}, MENU_LIGHT);
        let app = AppComponent::new(view);
        let (mut test, _) = TestingRunner::new(app, Size2D::new(340.0, 300.0), |_| {}, 1.0);
        test.sync_and_update();

        let area = test
            .find(|node, element| {
                let style = element.style();
                (style.background == Fill::Color(rgba(MENU_LIGHT.bg))
                    && (style.corner_radius.top_left - MENU_LIGHT.radius).abs() < 0.01)
                    .then(|| node.layout().area)
            })
            .expect("menu root box");
        assert_eq!((area.size.width, area.size.height), (340.0, 300.0));
    }

    #[test]
    fn preferred_size_accounts_for_separators() {
        let with_sep = sample();
        let without_sep: Vec<MenuItem> = with_sep
            .iter()
            .filter(|item| !matches!(item, MenuItem::Separator))
            .cloned()
            .collect();
        let (_, h_with) = MenuView::preferred_size(&with_sep, MENU_LIGHT);
        let (_, h_without) = MenuView::preferred_size(&without_sep, MENU_LIGHT);
        assert!(h_with > h_without);
    }

    #[test]
    fn navigation_skips_separators_and_disabled_items() {
        let items = sample();
        assert_eq!(first_selectable(&items), Some(0));
        assert_eq!(next_selectable(&items, Some(0), 1), Some(3));
        assert_eq!(next_selectable(&items, Some(3), 1), Some(0));
        assert_eq!(last_selectable(&items), Some(3));
    }

    #[test]
    fn activate_at_ignores_disabled_and_submenus() {
        let items = sample();
        assert_eq!(activate_at(&items, 0), Some("new".to_string()));
        assert_eq!(activate_at(&items, 1), None);
        assert_eq!(activate_at(&items, 2), None);
        assert_eq!(activate_at(&items, 3), None);
    }

    #[test]
    fn navigation_skips_headers_and_search_fields() {
        let items = vec![
            MenuItem::SearchField {
                placeholder: "Buscar".to_string(),
            },
            MenuItem::Header {
                label: "Mitades".to_string(),
            },
            MenuItem::Action {
                id: "a".to_string(),
                label: "A".to_string(),
                shortcut: None,
                enabled: true,
                check: CheckState::None,
                icon: None,
            },
            MenuItem::Header {
                label: "Cuartos".to_string(),
            },
            MenuItem::SearchField {
                placeholder: "Filtrar".to_string(),
            },
            MenuItem::Submenu {
                label: "B".to_string(),
                enabled: true,
                items: Vec::new(),
                icon: None,
            },
        ];
        assert_eq!(first_selectable(&items), Some(2));
        assert_eq!(next_selectable(&items, Some(2), 1), Some(5));
        assert_eq!(next_selectable(&items, Some(5), 1), Some(2));
        assert_eq!(last_selectable(&items), Some(5));
    }

    #[test]
    fn icons_do_not_widen_the_menu() {
        let built = |icon| {
            vec![
                MenuItem::Action {
                    id: "a".to_string(),
                    label: "Configuración…".to_string(),
                    shortcut: Some("Super+,".to_string()),
                    enabled: true,
                    check: CheckState::None,
                    icon,
                },
                MenuItem::Submenu {
                    label: "Disposición".to_string(),
                    enabled: true,
                    items: Vec::new(),
                    icon,
                },
            ]
        };
        let plain = MenuView::preferred_size(&built(None), MENU_LIGHT);
        let with_icons = MenuView::preferred_size(&built(Some("x")), MENU_LIGHT);
        assert_eq!(plain, with_icons);
    }

    #[test]
    fn left_handed_submenus_keep_their_descendants_on_the_left() {
        let (anchor, gravity, dx) = submenu_positioner(true);
        assert_eq!(anchor, PopupAnchor::TopLeft);
        assert_eq!(gravity, PopupGravity::BottomLeft);
        assert_eq!(dx, 0, "the panel-edge anchor needs no horizontal offset");

        // Three levels: each child's right edge is at its parent's left edge,
        // so no popup overlaps an ancestor.
        let mut parent_left = 1000.0;
        for _ in 0..3 {
            let child_right = parent_left + dx as f32;
            assert!(
                child_right <= parent_left,
                "a left-handed child overlaps its ancestor"
            );
            parent_left = child_right;
        }

        let (anchor, gravity, dx) = submenu_positioner(false);
        assert_eq!(anchor, PopupAnchor::TopRight);
        assert_eq!(gravity, PopupGravity::BottomRight);
        assert_eq!(dx, 0);
    }

    #[test]
    fn submenu_anchor_rect_matches_the_row_top() {
        let items = vec![
            MenuItem::Action {
                id: "one".to_string(),
                label: "One".to_string(),
                shortcut: None,
                enabled: true,
                check: CheckState::None,
                icon: None,
            },
            MenuItem::Separator,
            MenuItem::Action {
                id: "two".to_string(),
                label: "Two".to_string(),
                shortcut: None,
                enabled: true,
                check: CheckState::None,
                icon: None,
            },
            MenuItem::Separator,
            MenuItem::Submenu {
                label: "More".to_string(),
                enabled: true,
                icon: None,
                items: vec![MenuItem::Action {
                    id: "deep".to_string(),
                    label: "Deep".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::None,
                    icon: None,
                }],
            },
        ];
        let m = MENU_LIGHT;
        let (width, _) = MenuView::preferred_size(&items, m);
        let (x, y, w, h) = submenu_anchor_rect(4, &items, m, width);
        let row_top = item_top(4, &items, m) + m.padding;
        assert_eq!(y as f32, row_top);
        assert_eq!(x, 0);
        assert_eq!(w as f32, width);
        assert_eq!(h as f32, m.item_height);

        // With a bottom gravity and a negative inset offset, the child's first
        // row ends up at the same y as the parent's row.
        let (_, _, dx) = submenu_positioner(false);
        let inset = m.padding + m.border_width;
        assert_eq!(dx, 0);
        let child_top = row_top - inset;
        let child_first_row_top = child_top + inset;
        assert_eq!(child_first_row_top, row_top);
    }

    fn relative_luminance(color: Rgba) -> f32 {
        fn linear(channel: f32) -> f32 {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    fn contrast(a: Rgba, b: Rgba) -> f32 {
        let a = relative_luminance(a);
        let b = relative_luminance(b);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn shortcuts_use_macos_glyphs_in_order() {
        assert_eq!(format_shortcut("Ctrl+Shift+G"), "⌃⇧G");
        assert_eq!(format_shortcut("Alt+Inicio"), "⌥Inicio");
        assert_eq!(format_shortcut("Super+L"), "❖L");
        assert_eq!(format_shortcut("Ctrl+Shift+Z"), "⌃⇧Z");
        assert_eq!(format_shortcut("Shift+Ctrl+G"), "⌃⇧G");
        assert_eq!(format_shortcut("Cmd+D"), "❖D");
        assert_eq!(format_shortcut("Alt+Super+Escape"), "⌥❖⎋");
        assert!(!format_shortcut("Command+D").contains('\u{2318}'));
        assert_eq!(format_shortcut("F5"), "F5");
        assert_eq!(format_shortcut("Ctrl+Super+Left"), "⌃❖←");
        assert_eq!(format_shortcut("Ctrl+Shift+Super+Right"), "⌃⇧❖→");
        assert_eq!(format_shortcut("Up"), "↑");
        assert_eq!(format_shortcut("Down"), "↓");
        assert_eq!(format_shortcut("Return"), "↩");
        assert_eq!(format_shortcut("Enter"), "↩");
        assert_eq!(format_shortcut("Ctrl+Tab"), "⌃⇥");
    }

    #[test]
    fn mark_sizes_fit_the_mark_column() {
        for (name, m) in [("light", MENU_LIGHT), ("dark", MENU_DARK)] {
            assert!(
                (6.0..=8.0).contains(&m.mark_dot),
                "{name}: radio dot {} is outside 6-8 px",
                m.mark_dot
            );
            assert!(
                m.mark_dot <= m.mark_width,
                "{name}: radio dot is wider than its column"
            );
            assert!(
                m.mark_check > 0.0 && m.mark_check <= m.mark_width,
                "{name}: check glyph does not fit its column"
            );
        }
    }

    #[test]
    fn focused_shortcut_and_chevron_keep_4_5_contrast() {
        for (name, m) in [("light", MENU_LIGHT), ("dark", MENU_DARK)] {
            for color in [
                shortcut_fg(true, true, m),
                chevron_fg(true, true, m),
                m.hover_fg,
            ] {
                let ratio = contrast(color, m.hover_bg);
                assert!(
                    ratio >= 4.5,
                    "{name}: focused row color has {ratio:.2}:1 contrast on accent"
                );
            }
        }
    }

    #[test]
    fn preferred_size_handles_accents_and_long_shortcuts() {
        let items = vec![
            MenuItem::Action {
                id: "images".to_string(),
                label: "Imágenes".to_string(),
                shortcut: Some("Ctrl+Shift+G".to_string()),
                enabled: true,
                check: CheckState::None,
                icon: None,
            },
            MenuItem::Action {
                id: "videos".to_string(),
                label: "Vídeos".to_string(),
                shortcut: Some("Ctrl+Shift+Z".to_string()),
                enabled: true,
                check: CheckState::None,
                icon: None,
            },
        ];
        let m = MENU_LIGHT;
        let (w, h) = MenuView::preferred_size(&items, m);

        let label_w = measure_text("Imágenes", m.font_size);
        let shortcut_w = measure_text(&format_shortcut("Ctrl+Shift+G"), m.font_size);
        let expected_min = m.item_padding_x * 2.0
            + m.mark_width
            + MARK_TEXT_GAP
            + label_w
            + m.shortcut_gap
            + shortcut_w
            + (2.0 * m.border_width + m.padding) * 2.0;
        assert!(
            w >= expected_min,
            "menu width {w} should fit the widest accented label + shortcut ({expected_min})"
        );

        // Height must fit two rows plus panel insets.
        let expected_height = m.item_height * 2.0 + (2.0 * m.border_width + m.padding) * 2.0;
        assert!(
            h >= expected_height,
            "menu height {h} should fit two rows ({expected_height})"
        );
    }
}
