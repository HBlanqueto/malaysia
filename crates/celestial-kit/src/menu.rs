//! Neptunia menu control for CelestialKit popups.
//!
//! Distinct from [`freya_components::menu::Menu`]. This one is designed for the
//! shell's global menus and context menus: it opens submenus as child popups,
//! supports keyboard navigation and reports actions through `on_activate`.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;

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
    if !text.contains('+') {
        return text.to_string();
    }
    let (mut ctrl, mut alt, mut shift, mut sup) = (false, false, false, false);
    let mut key = String::new();
    for token in text.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" | "option" | "opt" => alt = true,
            "shift" => shift = true,
            "super" | "meta" | "cmd" | "command" => sup = true,
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
    },
    /// A nested submenu.
    Submenu {
        label: String,
        enabled: bool,
        items: Vec<MenuItem>,
    },
    /// A visual separator.
    Separator,
}

impl MenuItem {
    fn enabled(&self) -> bool {
        match self {
            MenuItem::Action { enabled, .. } => *enabled,
            MenuItem::Submenu { enabled, .. } => *enabled,
            MenuItem::Separator => false,
        }
    }

    fn selectable(&self) -> bool {
        matches!(self, MenuItem::Action { .. } | MenuItem::Submenu { .. }) && self.enabled()
    }

    fn label(&self) -> Option<&str> {
        match self {
            MenuItem::Action { label, .. } | MenuItem::Submenu { label, .. } => Some(label),
            MenuItem::Separator => None,
        }
    }

    fn shortcut(&self) -> Option<String> {
        match self {
            MenuItem::Action { shortcut, .. } => shortcut.as_deref().map(format_shortcut),
            MenuItem::Submenu { .. } | MenuItem::Separator => None,
        }
    }
}

/// A Neptunia menu rendered inside a popup.
pub struct MenuView {
    items: Rc<Vec<MenuItem>>,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    palette: MenuPalette,
    size: (f32, f32),
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
        let size = Self::preferred_size(&items, palette);
        Self {
            items: Rc::new(items),
            on_activate: Rc::new(on_activate),
            on_dismiss_all: Rc::new(on_dismiss_all),
            palette,
            size,
        }
    }

    /// Returns the logical size needed to render `items` with `palette`.
    ///
    /// Labels are measured with Skia and truncated in the middle with "…" when
    /// they would exceed `panel.max_width`. Shortcuts and chevrons always stay
    /// visible and are never wrapped.
    pub fn preferred_size(items: &[MenuItem], m: MenuPalette) -> (f32, f32) {
        let inset = 2.0 * m.border_width + m.padding;
        let content_height: f32 = items
            .iter()
            .map(|item| match item {
                MenuItem::Separator => m.separator_height + m.separator_margin_y * 2.0,
                _ => m.item_height,
            })
            .sum();
        let max_content_width = (m.max_width - inset * 2.0).max(m.min_width);
        let mut content_width = m.min_width;

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
            let fixed = m.item_padding_x * 2.0 + m.mark_width + trailing;
            let max_label_w = (max_content_width - fixed).max(0.0);
            let label = truncate_middle(label, max_label_w, m.font_size);
            let label_w = measure_text(&label, m.font_size);
            let content_w = fixed + label_w;
            content_width = content_width.max(content_w);
        }

        let width = content_width.min(max_content_width) + inset * 2.0;
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
            palette: self.palette,
            size: self.size,
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
        let m = self.palette;
        let size = self.size;
        let popup: Option<PopupHandle> = try_consume_root_context();
        let opens_left = popup.as_ref().is_some_and(PopupHandle::opens_left);
        let focused = use_state(|| first_selectable(&items));
        let open_child_id = use_hook(|| Rc::new(Cell::new(None)));

        let focused_index = *focused.read();

        let rows: Vec<Element> = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                render_item(
                    item,
                    index,
                    focused_index == Some(index),
                    m,
                    popup.clone(),
                    opens_left,
                    on_activate.clone(),
                    on_dismiss_all.clone(),
                    items.clone(),
                    focused,
                    open_child_id.clone(),
                    size,
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
            .width(Size::px(size.0))
            .height(Size::px(size.1))
            .background(rgba(m.outer_border))
            .corner_radius(m.radius)
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
        root.on_global_key_down(move |event: Event<KeyboardEventData>| {
            let mut focused = focused;
            let open_child_id = open_child_id.clone();
            let current = *focused.read();
            match event.key {
                Key::Named(NamedKey::ArrowDown) => {
                    focused.set(next_selectable(&items, current, 1));
                }
                Key::Named(NamedKey::ArrowUp) => {
                    focused.set(next_selectable(&items, current, -1));
                }
                Key::Named(NamedKey::Home) => {
                    focused.set(first_selectable(&items));
                }
                Key::Named(NamedKey::End) => {
                    focused.set(last_selectable(&items));
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(index) = *focused.read() {
                        activate_or_open(
                            &items,
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
                    if let Some(index) = *focused.read() {
                        activate_or_open(
                            &items,
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
                Key::Named(NamedKey::ArrowRight) => {
                    if let Some(index) = *focused.read()
                        && let Some(MenuItem::Submenu { items: sub, .. }) = items.get(index)
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
                            items.clone(),
                            opens_left,
                        );
                    }
                }
                Key::Named(NamedKey::ArrowLeft) => {
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
    if let Some(MenuItem::Action { id, enabled, .. }) = items.get(index)
        && *enabled
    {
        return Some(id.clone());
    }
    None
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

fn item_top(index: usize, items: &[MenuItem], m: MenuPalette) -> f32 {
    let mut y = 0.0;
    for (i, item) in items.iter().enumerate() {
        if i == index {
            return y;
        }
        match item {
            MenuItem::Separator => y += m.separator_height + m.separator_margin_y * 2.0,
            _ => y += m.item_height,
        }
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
    focused: UseState<Option<usize>>,
    open_child_id: Rc<Cell<Option<PopupId>>>,
    size: (f32, f32),
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
        } => {
            let id = id.clone();
            let label_text = label_text.clone();
            let shortcut_text = shortcut_text.as_ref().map(|s| format_shortcut(s));
            let enabled = *enabled;
            let check = *check;
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
            let shortcut_fg = shortcut_fg(enabled, is_focused, m);
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
                .on_pointer_enter(move |_| {
                    if enabled {
                        focused.set(Some(index));
                        close_child(open_child_id_enter.clone());
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
                .child(mark_element(check, fg, m))
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
                        .color(rgba(shortcut_fg))
                        .width(Size::px(shortcut_w))
                        .max_lines(1),
                )
                .into_element()
        }
        MenuItem::Submenu {
            label: label_text,
            enabled,
            items: sub_items,
        } => {
            let label_text = label_text.clone();
            let enabled = *enabled;
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
                .child(mark_element(CheckState::None, fg, m))
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
    }
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
            },
            MenuItem::Separator,
            MenuItem::Action {
                id: "open".to_string(),
                label: "Open".to_string(),
                shortcut: None,
                enabled: false,
                check: CheckState::Checkbox(true),
            },
            MenuItem::Submenu {
                label: "Recent".to_string(),
                enabled: true,
                items: vec![MenuItem::Action {
                    id: "r1".to_string(),
                    label: "a.txt".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::None,
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
            },
            MenuItem::Separator,
            MenuItem::Action {
                id: "two".to_string(),
                label: "Two".to_string(),
                shortcut: None,
                enabled: true,
                check: CheckState::None,
            },
            MenuItem::Separator,
            MenuItem::Submenu {
                label: "More".to_string(),
                enabled: true,
                items: vec![MenuItem::Action {
                    id: "deep".to_string(),
                    label: "Deep".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::None,
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
        assert!(!format_shortcut("Command+D").contains('\u{2318}'));
        assert_eq!(format_shortcut("F5"), "F5");
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
            },
            MenuItem::Action {
                id: "videos".to_string(),
                label: "Vídeos".to_string(),
                shortcut: Some("Ctrl+Shift+Z".to_string()),
                enabled: true,
                check: CheckState::None,
            },
        ];
        let m = MENU_LIGHT;
        let (w, h) = MenuView::preferred_size(&items, m);

        let label_w = measure_text("Imágenes", m.font_size);
        let shortcut_w = measure_text(&format_shortcut("Ctrl+Shift+G"), m.font_size);
        let expected_min = m.item_padding_x * 2.0
            + m.mark_width
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
