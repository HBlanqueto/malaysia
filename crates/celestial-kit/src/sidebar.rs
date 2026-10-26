//! Neptunia sidebar, sidebar items, collapsible sections and sidebar layout.
//!
//! All colours and measurements come from `yuki-design` and the resolved
//! `appearance.domain`; there are no hardcoded design values.

use freya_core::prelude::*;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Alignment, Direction, Gaps, Position, Size};
use yuki_appearance::{SidebarStyle, palette};
use yuki_design::{Rgba, SIDEBAR, TITLEBAR};

use crate::platform::WindowControls;
use crate::titlebar::{window_drag_handler, window_traffic_lights};

fn rgba(color: Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

/// A vertical Neptunia sidebar.
#[derive(PartialEq)]
pub struct Sidebar {
    children: Vec<Element>,
    traffic_lights: bool,
    key: DiffKey,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            traffic_lights: false,
            key: DiffKey::None,
        }
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_element());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(IntoElement::into_element));
        self
    }

    /// Draw the window traffic lights at `(SIDEBAR.traffic_x, SIDEBAR.traffic_y)`.
    pub fn traffic_lights(mut self, show: bool) -> Self {
        self.traffic_lights = show;
        self
    }

    pub fn key(mut self, key: impl Into<DiffKey>) -> Self {
        self.key = key.into();
        self
    }
}

impl KeyExt for Sidebar {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for Sidebar {
    fn render(&self) -> impl IntoElement {
        let resolved = crate::theme::resolved();
        let palette = palette(resolved.scheme);
        let surface = resolved.sidebar;
        let island = surface.style == SidebarStyle::Island;

        let top_height = SIDEBAR.traffic_y + TITLEBAR.buttons.diameter;

        let mut root = rect()
            .width(Size::px(SIDEBAR.width as f32))
            .height(Size::fill())
            .direction(Direction::Vertical)
            .background(rgba(surface.bg))
            .a11y_role(AccessibilityRole::Navigation)
            .children(self.children.iter().cloned());

        if island {
            let rim = Border::new()
                .width(SIDEBAR.rim_width)
                .alignment(BorderAlignment::Inner)
                .fill(rgba(surface.rim));
            root = root
                .corner_radius(SIDEBAR.radius as f32)
                .border(rim)
                .padding(Gaps::new(
                    top_height as f32,
                    SIDEBAR.inset as f32,
                    SIDEBAR.inset as f32,
                    SIDEBAR.inset as f32,
                ));

            // shortcut: Freya's rect supports a single shadow, so only the
            // ambient shadow is drawn. Add a second shadow layer when Freya
            // supports shadow stacks.
            let shadow_layer = &surface.shadow[0];
            root = root.shadow(
                Shadow::new()
                    .x(0.0)
                    .y(shadow_layer.y_offset as f32)
                    .blur(shadow_layer.blur as f32)
                    .color(rgba(shadow_layer.color)),
            );

            if surface.inner.a > 0.0 {
                root = root.border(
                    Border::new()
                        .width(SIDEBAR.inner_width)
                        .alignment(BorderAlignment::Inner)
                        .fill(rgba(surface.inner)),
                );
            }
        } else {
            root = root.padding(Gaps::new(top_height as f32, 0.0, 0.0, 0.0));
            // Right-hand 1 px separator using the palette hairline.
            root = root.child(
                rect()
                    .position(Position::new_absolute().top(0.0).right(0.0))
                    .width(Size::px(SIDEBAR.inner_width))
                    .height(Size::fill())
                    .background(rgba(palette.hairline)),
            );
        }

        if self.traffic_lights {
            let fixed = WindowControls::get().fixed;
            root = root.child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::fill())
                    .height(Size::px(top_height as f32))
                    .padding(Gaps::new(
                        SIDEBAR.traffic_y as f32,
                        SIDEBAR.traffic_x as f32,
                        0.0,
                        0.0,
                    ))
                    .on_mouse_down(window_drag_handler(fixed))
                    .child(window_traffic_lights(None, true, fixed)),
            );
        }

        root
    }
}

/// One row inside a [`Sidebar`].
#[derive(PartialEq)]
pub struct SidebarItem {
    label: String,
    badge: Option<u32>,
    selected: bool,
    app_icon: bool,
    on_select: Option<Callback<(), ()>>,
    key: DiffKey,
}

impl SidebarItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            badge: None,
            selected: false,
            app_icon: false,
            on_select: None,
            key: DiffKey::None,
        }
    }

    pub fn badge(mut self, badge: u32) -> Self {
        self.badge = Some(badge);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Use `SIDEBAR.app_icon` instead of `SIDEBAR.item_icon`.
    pub fn app_icon(mut self) -> Self {
        self.app_icon = true;
        self
    }

    pub fn on_select(mut self, callback: impl Into<Callback<(), ()>>) -> Self {
        self.on_select = Some(callback.into());
        self
    }

    pub fn key(mut self, key: impl Into<DiffKey>) -> Self {
        self.key = key.into();
        self
    }
}

impl KeyExt for SidebarItem {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SidebarItem {
    fn render(&self) -> impl IntoElement {
        let resolved = crate::theme::resolved();
        let palette = palette(resolved.scheme);
        let mut hovered = use_state(|| false);
        let icon_size = if self.app_icon {
            SIDEBAR.app_icon
        } else {
            SIDEBAR.item_icon
        };

        let (bg, fg, weight) = if self.selected {
            (
                rgba(palette.accent),
                rgba(palette.titlebar_bg.focused),
                FontWeight::SEMI_BOLD,
            )
        } else if *hovered.read() {
            (
                rgba(palette.hairline),
                rgba(palette.text.focused),
                FontWeight::MEDIUM,
            )
        } else {
            (
                Color::TRANSPARENT,
                rgba(palette.text.focused),
                FontWeight::MEDIUM,
            )
        };

        let mut row = rect()
            .width(Size::fill())
            .height(Size::px(SIDEBAR.item_height as f32))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .corner_radius(SIDEBAR.item_radius as f32)
            .background(bg)
            .a11y_role(AccessibilityRole::ListItem)
            .a11y_focusable(true)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .maybe(self.on_select.is_some(), |el| {
                let callback = self.on_select.clone().unwrap();
                el.on_press(move |_| callback.call(()))
            });

        // Icon placeholder, sized from the tokens.
        row = row.child(
            rect()
                .width(Size::px(icon_size as f32))
                .height(Size::px(icon_size as f32))
                .corner_radius((icon_size as f32 / 4.0).max(1.0))
                .background(rgba(palette.text.unfocused))
                .a11y_role(AccessibilityRole::Image),
        );

        row = row.child(
            label()
                .text(self.label.clone())
                .font_size(13.0)
                .font_weight(weight)
                .color(fg)
                .max_lines(1),
        );

        if let Some(badge) = self.badge {
            row = row.child(
                rect()
                    .padding(Gaps::new(2.0, 6.0, 2.0, 6.0))
                    .corner_radius(8.0)
                    .background(rgba(resolved.accent))
                    .child(
                        label()
                            .text(badge.to_string())
                            .font_size(11.0)
                            .color(rgba(palette.titlebar_bg.focused)),
                    ),
            );
        }

        row
    }
}

/// A collapsible section inside a [`Sidebar`].
#[derive(PartialEq)]
pub struct SidebarSection {
    title: String,
    children: Vec<Element>,
    initial_expanded: bool,
    on_toggle: Option<Callback<bool, ()>>,
    key: DiffKey,
}

impl SidebarSection {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            children: Vec::new(),
            initial_expanded: true,
            on_toggle: None,
            key: DiffKey::None,
        }
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_element());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(IntoElement::into_element));
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.initial_expanded = expanded;
        self
    }

    pub fn on_toggle(mut self, callback: impl Into<Callback<bool, ()>>) -> Self {
        self.on_toggle = Some(callback.into());
        self
    }

    pub fn key(mut self, key: impl Into<DiffKey>) -> Self {
        self.key = key.into();
        self
    }
}

impl KeyExt for SidebarSection {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SidebarSection {
    fn render(&self) -> impl IntoElement {
        let expanded = use_state(|| self.initial_expanded);
        let resolved = crate::theme::resolved();
        let palette = palette(resolved.scheme);
        let mut chevron_visible = use_state(|| false);

        let on_toggle = self.on_toggle.clone();
        let toggle = {
            let mut expanded = expanded;
            move |_| {
                let new = !*expanded.read();
                expanded.set(new);
                if let Some(cb) = &on_toggle {
                    cb.call(new);
                }
            }
        };

        let on_toggle = self.on_toggle.clone();
        let on_key = {
            let mut expanded = expanded;
            move |event: Event<KeyboardEventData>| {
                let toggles = matches!(event.key, Key::Named(NamedKey::Enter))
                    || matches!(event.key, Key::Character(ref s) if s == " ");
                if toggles {
                    let new = !*expanded.read();
                    expanded.set(new);
                    if let Some(cb) = &on_toggle {
                        cb.call(new);
                    }
                }
            }
        };

        rect()
            .width(Size::fill())
            .direction(Direction::Vertical)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(SIDEBAR.section_height as f32))
                    .margin(Gaps::new(SIDEBAR.section_margin_top as f32, 0.0, 0.0, 0.0))
                    .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::Center)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_focusable(true)
                    .a11y_alt(self.title.clone())
                    .a11y_builder(move |node| {
                        node.set_expanded(*expanded.read());
                    })
                    .on_press(toggle)
                    .on_key_down(on_key)
                    .child(
                        label()
                            .text(self.title.clone())
                            .font_size(SIDEBAR.section_size as f32)
                            .font_weight(FontWeight::from(SIDEBAR.section_weight as i32))
                            .color(rgba(palette.text.unfocused))
                            .max_lines(1),
                    )
                    .maybe(*chevron_visible.read() || *expanded.read(), |el| {
                        el.child(
                            label()
                                .text(if *expanded.read() { "▼" } else { "▶" })
                                .font_size(SIDEBAR.chevron as f32)
                                .color(rgba(palette.text.unfocused)),
                        )
                    })
                    .on_pointer_enter(move |_| chevron_visible.set(true))
                    .on_pointer_leave(move |_| chevron_visible.set(false)),
            )
            .maybe(*expanded.read(), |el| {
                el.children(self.children.iter().cloned())
            })
            .into_element()
    }
}

/// Lays out a window with a sidebar on the left and a content column on the
/// right.
#[derive(PartialEq)]
pub struct SidebarLayout {
    sidebar: Element,
    content: Element,
    key: DiffKey,
}

impl SidebarLayout {
    pub fn new(sidebar: impl IntoElement, content: impl IntoElement) -> Self {
        Self {
            sidebar: sidebar.into_element(),
            content: content.into_element(),
            key: DiffKey::None,
        }
    }

    pub fn key(mut self, key: impl Into<DiffKey>) -> Self {
        self.key = key.into();
        self
    }
}

impl KeyExt for SidebarLayout {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SidebarLayout {
    fn render(&self) -> impl IntoElement {
        let resolved = crate::theme::resolved();
        let island = resolved.sidebar.style == SidebarStyle::Island;
        let inset = if island { SIDEBAR.inset as f32 } else { 0.0 };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .padding(Gaps::new(inset, inset, inset, inset))
            .spacing(inset)
            .child(self.sidebar.clone())
            .child(self.content.clone())
            .into_element()
    }
}
