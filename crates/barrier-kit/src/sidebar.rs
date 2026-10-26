//! Neptunia sidebar, sidebar items, collapsible sections and sidebar layout.
//!
//! All colours and measurements come from `vow-design` and the resolved
//! `appearance.domain`; there are no hardcoded design values.

use std::time::Duration;

use freya_animation::prelude::{AnimNum, use_animation_transition};
use freya_components::svg_viewer::SvgViewer;
use freya_core::prelude::*;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Alignment, Direction, Gaps, Position, Size};
use vow_appearance::SidebarStyle;
use vow_design::{DURATION_BASE, Rgba, SIDEBAR, TITLEBAR};

use crate::platform::{DecorationResponse, WindowControls, WindowStatus};
use crate::titlebar::{window_drag_handler, window_traffic_lights};
use freya_core::prelude::try_consume_root_context;

fn rgba(color: Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn svg_with_color(template: &str, color: Rgba) -> String {
    let [r, g, b, _] = color.to_rgba8();
    template.replace("currentColor", &format!("rgb({r},{g},{b})"))
}

fn svg_viewer(template: &str, color: Rgba, size: u32) -> SvgViewer {
    SvgViewer::new((
        format!("sidebar-icon-{}", template.len()),
        Bytes::from(svg_with_color(template, color)),
    ))
    .width(Size::px(size as f32))
    .height(Size::px(size as f32))
    .show_loader(false)
}

fn chevron_svg(down: bool, stroke: f32, color: Rgba) -> String {
    let [r, g, b, _] = color.to_rgba8();
    let path = if down {
        "M2 4 L6 8 L10 4"
    } else {
        "M4 2 L8 6 L4 10"
    };
    let size = SIDEBAR.chevron;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" viewBox=\"0 0 12 12\"><path d=\"{path}\" fill=\"none\" stroke=\"rgb({r},{g},{b})\" stroke-width=\"{stroke}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>"
    )
}

/// A vertical Neptunia sidebar.
#[derive(PartialEq)]
pub struct Sidebar {
    children: Vec<Element>,
    traffic_lights: bool,
    on_close: Option<Callback<(), ()>>,
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
            on_close: None,
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

    /// Called when the close light of the sidebar traffic lights is released.
    pub fn on_close(mut self, callback: impl Into<Callback<(), ()>>) -> Self {
        self.on_close = Some(callback.into());
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
        let surface = resolved.sidebar;
        let island = surface.style == SidebarStyle::Island;

        let top_height = SIDEBAR.traffic_y + TITLEBAR.buttons.diameter;
        let padding_x = SIDEBAR.padding_x as f32;
        let radius = SIDEBAR.radius as f32;

        // The scrollable/content area starts below the traffic-light strip.
        let mut sidebar_body = rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Vertical)
            .background(rgba(surface.bg))
            .a11y_role(AccessibilityRole::Navigation)
            .padding(Gaps::new(
                top_height as f32,
                padding_x,
                if island { padding_x } else { 0.0 },
                padding_x,
            ))
            .children(self.children.iter().cloned());

        if island {
            sidebar_body = sidebar_body.corner_radius(radius);

            if surface.rim.a > 0.0 {
                sidebar_body = sidebar_body.border(
                    Border::new()
                        .width(SIDEBAR.rim_width)
                        .alignment(BorderAlignment::Inner)
                        .fill(rgba(surface.rim)),
                );
            }

            if surface.inner.a > 0.0 {
                sidebar_body = sidebar_body.child(
                    rect()
                        .position(
                            Position::new_absolute()
                                .top(0.0)
                                .left(0.0)
                                .right(0.0)
                                .bottom(0.0),
                        )
                        .margin(Gaps::new(1.0, 1.0, 1.0, 1.0))
                        .corner_radius((radius - 1.0).max(0.0))
                        .border(
                            Border::new()
                                .width(SIDEBAR.inner_width)
                                .alignment(BorderAlignment::Inner)
                                .fill(rgba(surface.inner)),
                        ),
                );
            }

            let ambient = &surface.shadow[0];
            if ambient.opacity > 0.0 {
                sidebar_body = sidebar_body.shadow(
                    Shadow::new()
                        .x(0.0)
                        .y(ambient.y_offset as f32)
                        .blur(ambient.blur as f32)
                        .color(rgba(ambient.color)),
                );
            }
        } else {
            sidebar_body = sidebar_body.child(
                rect()
                    .position(Position::new_absolute().top(0.0).right(0.0))
                    .width(Size::px(SIDEBAR.inner_width))
                    .height(Size::fill())
                    .background(rgba(surface.palette.line)),
            );
        }

        let mut root = rect().width(Size::fill()).height(Size::fill());
        if island {
            let contact = &surface.shadow[1];
            if contact.opacity > 0.0 {
                root = root.shadow(
                    Shadow::new()
                        .x(0.0)
                        .y(contact.y_offset as f32)
                        .blur(contact.blur as f32)
                        .color(rgba(contact.color)),
                );
            }
        }
        root = root.child(sidebar_body);

        if self.traffic_lights {
            let (status, fixed) = try_consume_root_context::<WindowControls>()
                .map(|controls| (*controls.status.read(), controls.fixed))
                .unwrap_or((
                    WindowStatus {
                        activated: true,
                        maximized: false,
                        decoration: DecorationResponse::Client,
                        fixed: false,
                    },
                    false,
                ));
            let (_, _, group_w, _) = vow_appearance::traffic::group_rect();
            let lights_right = SIDEBAR.traffic_x as f32 + group_w as f32;

            root = root
                .child(
                    rect()
                        .position(
                            Position::new_absolute()
                                .top(SIDEBAR.traffic_y as f32)
                                .left(SIDEBAR.traffic_x as f32),
                        )
                        .width(Size::px(group_w as f32))
                        .height(Size::px(TITLEBAR.buttons.diameter as f32))
                        .a11y_alt("sidebar-traffic-lights")
                        .child(window_traffic_lights(
                            self.on_close.clone(),
                            status.activated,
                            fixed,
                        )),
                )
                .child(
                    rect()
                        .position(Position::new_absolute().top(0.0).left(lights_right))
                        .width(Size::fill())
                        .height(Size::px(top_height as f32))
                        .a11y_alt("sidebar-drag-strip")
                        .on_mouse_down(window_drag_handler(fixed)),
                );
        }

        root.into_element()
    }
}

/// One row inside a [`Sidebar`].
#[derive(PartialEq)]
pub struct SidebarItem {
    label: String,
    icon: Option<&'static str>,
    app_icon: Option<(&'static str, Rgba)>,
    badge: Option<u32>,
    selected: bool,
    on_select: Option<Callback<(), ()>>,
    key: DiffKey,
}

impl SidebarItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            app_icon: None,
            badge: None,
            selected: false,
            on_select: None,
            key: DiffKey::None,
        }
    }

    pub fn icon(mut self, svg: &'static str) -> Self {
        self.icon = Some(svg);
        self.app_icon = None;
        self
    }

    pub fn app_icon(mut self, svg: &'static str, color: Rgba) -> Self {
        self.app_icon = Some((svg, color));
        self.icon = None;
        self
    }

    pub fn badge(mut self, badge: u32) -> Self {
        self.badge = Some(badge);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
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
        let palette = resolved.sidebar.palette;
        let mut hovered = use_state(|| false);

        let (bg, fg, weight) = if self.selected {
            (
                rgba(palette.selected_bg),
                rgba(palette.selected_ink),
                FontWeight::SEMI_BOLD,
            )
        } else if *hovered.read() {
            (
                rgba(palette.hover_bg),
                rgba(palette.item_ink),
                FontWeight::MEDIUM,
            )
        } else {
            (
                Color::TRANSPARENT,
                rgba(palette.item_ink),
                FontWeight::MEDIUM,
            )
        };

        let mut row = rect()
            .width(Size::fill())
            .height(Size::px(SIDEBAR.item_height as f32))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(SIDEBAR.item_gap as f32)
            .padding(Gaps::new(
                0.0,
                SIDEBAR.item_padding_x as f32,
                0.0,
                SIDEBAR.item_padding_x as f32,
            ))
            .corner_radius(SIDEBAR.item_radius as f32)
            .background(bg)
            .a11y_role(AccessibilityRole::ListItem)
            .a11y_focusable(true)
            .a11y_builder(move |node| {
                node.set_selected(self.selected);
            })
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .maybe(self.on_select.is_some(), |el| {
                let callback = self.on_select.clone().unwrap();
                el.on_press(move |_| callback.call(()))
            });

        if let Some((svg, bg_color)) = self.app_icon {
            row = row.child(
                rect()
                    .width(Size::px(SIDEBAR.app_icon as f32))
                    .height(Size::px(SIDEBAR.app_icon as f32))
                    .corner_radius(SIDEBAR.item_radius as f32)
                    .background(rgba(bg_color))
                    .center()
                    .child(svg_viewer(
                        svg,
                        Rgba {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        },
                        SIDEBAR.app_icon,
                    )),
            );
        } else if let Some(svg) = self.icon {
            row = row.child(svg_viewer(svg, palette.item_icon, SIDEBAR.item_icon));
        }

        row = row.child(
            label()
                .text(self.label.clone())
                .font_size(SIDEBAR.item_text_size as f32)
                .font_weight(weight)
                .color(fg)
                .max_lines(1),
        );

        if let Some(badge) = self.badge {
            row = row.child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .main_align(Alignment::End)
                    .cross_align(Alignment::Center)
                    .child(
                        label()
                            .text(badge.to_string())
                            .font_size(SIDEBAR.count_size as f32)
                            .color(rgba(palette.count_ink))
                            .max_lines(1),
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
        let palette = resolved.sidebar.palette;
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let keyboard_focused = matches!(*focus.read(), Focus::Keyboard);
        let show_chevron = *hovered.read() || keyboard_focused;

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

        let full_height = self.children.len() as f32 * SIDEBAR.item_height as f32;
        let reduced_motion = resolved.reduced_motion;
        let target = move |expanded: bool| if expanded { full_height } else { 0.0 };
        let animation = use_animation_transition(expanded, move |from, to| {
            let duration = if reduced_motion {
                0
            } else {
                Duration::from_millis(DURATION_BASE).as_millis() as u64
            };
            AnimNum::new(target(from), target(to)).time(duration)
        });
        let height = animation.get().value();

        rect()
            .width(Size::fill())
            .direction(Direction::Vertical)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(SIDEBAR.section_height as f32))
                    .margin(Gaps::new(SIDEBAR.section_margin_top as f32, 0.0, 0.0, 0.0))
                    .padding(Gaps::new(
                        0.0,
                        SIDEBAR.section_padding_x as f32,
                        0.0,
                        SIDEBAR.section_padding_x as f32,
                    ))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::Center)
                    .main_align(Alignment::SpaceBetween)
                    .a11y_id(a11y_id)
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
                            .color(rgba(palette.section_ink))
                            .max_lines(1),
                    )
                    .maybe(show_chevron, |el| {
                        let svg = chevron_svg(
                            *expanded.read(),
                            SIDEBAR.chevron_stroke,
                            palette.section_ink,
                        );
                        el.child(svg_viewer(&svg, palette.section_ink, SIDEBAR.chevron))
                    })
                    .on_pointer_enter(move |_| hovered.set(true))
                    .on_pointer_leave(move |_| hovered.set(false)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(height))
                    .overflow(Overflow::Clip)
                    .direction(Direction::Vertical)
                    .children(self.children.iter().cloned()),
            )
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
        let sidebar_width = SIDEBAR.width as f32;

        let sidebar_container_width = if island {
            sidebar_width + 2.0 * inset
        } else {
            sidebar_width
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .child(
                rect()
                    .width(Size::px(sidebar_container_width))
                    .height(Size::fill())
                    .padding(Gaps::new(inset, inset, inset, inset))
                    .child(self.sidebar.clone()),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(self.content.clone()),
            )
            .into_element()
    }
}
