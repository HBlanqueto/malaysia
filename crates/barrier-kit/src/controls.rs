//! BarrierKit controls with macOS-style names, built on `freya-components`
//! or on the core `rect`/`label` primitives (ADR 0034). Measurements and
//! colours come from the theme and `vow-design`.

pub use freya_components::button::Button;
pub use freya_components::input::Input as TextField;
pub use freya_components::menu::Menu;
pub use freya_components::popup::Popup as Popover;
pub use freya_components::scrollviews::ScrollView;
pub use freya_components::segmented_button::SegmentedButton as SegmentedControl;
pub use freya_components::slider::Slider;
pub use freya_components::switch::Switch;

use freya_components::svg_viewer::SvgViewer;

use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use vow_design::{Rgba, TEXT, TEXT_LINE_HEIGHT, TOOLGROUP};

use crate::password::PasswordField;
use crate::platform::inserts_text;

/// True when a key event should be inserted into a [`SecureField`].
fn secure_field_accepts_text(key: &Key, modifiers: Modifiers) -> bool {
    matches!(key, Key::Character(_)) && inserts_text(modifiers)
}

/// A vertical list. The caller owns the children; `List` only fixes the
/// direction and the spacing from the text token.
#[derive(Clone, Default, PartialEq)]
pub struct List {
    children: Vec<Element>,
    key: DiffKey,
}

impl List {
    pub fn new() -> Self {
        Self::default()
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

    pub fn key(mut self, key: impl Into<DiffKey>) -> Self {
        self.key = key.into();
        self
    }
}

impl KeyExt for List {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for List {
    fn render(&self) -> impl IntoElement {
        rect()
            .direction(Direction::Vertical)
            .spacing(TEXT_LINE_HEIGHT)
            .children(self.children.iter().cloned())
    }
}

/// A masked password field that keeps the same zeroizing buffer as the lock
/// screen (`password::PasswordField`). The masked text is the only value the
/// accessibility tree and the canvas see; the secret is never logged.
#[derive(Clone)]
pub struct SecureField {
    field: State<PasswordField>,
    width: f32,
    key: DiffKey,
}

impl SecureField {
    pub fn new() -> Self {
        Self {
            field: use_state(PasswordField::default),
            width: 240.0,
            key: DiffKey::None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn key(mut self, key: impl Into<DiffKey>) -> Self {
        self.key = key.into();
        self
    }
}

impl Default for SecureField {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyExt for SecureField {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl PartialEq for SecureField {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Component for SecureField {
    fn render(&self) -> impl IntoElement {
        let mut field = self.field;
        let masked = field.read().masked();
        rect()
            .width(Size::px(self.width))
            .height(Size::px(TEXT.size as f32 * TEXT_LINE_HEIGHT))
            .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let mut field = field.write();
                match &event.key {
                    Key::Named(NamedKey::Backspace) => field.backspace(),
                    Key::Character(text)
                        if secure_field_accepts_text(&event.key, event.modifiers) =>
                    {
                        for ch in text.chars() {
                            field.insert(ch);
                        }
                    }
                    _ => {}
                }
            })
            .child(label().text(masked))
    }
}

/// Visual style for a [`ToolGroup`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolGroupStyle {
    /// Capsule with a tinted background and vertical separators.
    #[default]
    Island,
    /// Transparent background with vertical separators only.
    Flat,
}

/// A macOS-style toolbar capsule group.
#[derive(PartialEq)]
pub struct ToolGroup {
    children: Vec<Element>,
    style: ToolGroupStyle,
    key: DiffKey,
}

impl Default for ToolGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolGroup {
    pub fn new() -> Self {
        let style = match crate::theme::resolved().sidebar.style {
            vow_appearance::SidebarStyle::Island => ToolGroupStyle::Island,
            vow_appearance::SidebarStyle::Flat => ToolGroupStyle::Flat,
        };
        Self {
            children: Vec::new(),
            style,
            key: DiffKey::None,
        }
    }

    /// Use the flat style (no background capsule).
    pub fn flat(mut self) -> Self {
        self.style = ToolGroupStyle::Flat;
        self
    }

    /// Use the island style (background capsule without shadow).
    pub fn island(mut self) -> Self {
        self.style = ToolGroupStyle::Island;
        self
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

    pub fn key(mut self, key: impl Into<DiffKey>) -> Self {
        self.key = key.into();
        self
    }
}

impl KeyExt for ToolGroup {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ToolGroup {
    fn render(&self) -> impl IntoElement {
        let resolved = crate::theme::resolved();
        let palette = resolved.toolgroup.palette;

        let mut items: Vec<Element> = Vec::with_capacity(self.children.len());
        let last = self.children.len().saturating_sub(1);
        for (i, child) in self.children.iter().cloned().enumerate() {
            items.push(child);
            if self.style == ToolGroupStyle::Island && i != last {
                items.push(
                    rect()
                        .width(Size::px(TOOLGROUP.separator_w as f32))
                        .height(Size::px(TOOLGROUP.separator_h as f32))
                        .background(rgba(palette.separator))
                        .into_element(),
                );
            }
        }

        let root = rect()
            .direction(Direction::Horizontal)
            .height(Size::px(TOOLGROUP.height as f32))
            .cross_align(Alignment::Center)
            .padding(Gaps::new(
                0.0,
                TOOLGROUP.padding_x as f32,
                0.0,
                TOOLGROUP.padding_x as f32,
            ))
            .children(items);

        if self.style == ToolGroupStyle::Island {
            return root
                .background(rgba(resolved.toolgroup.bg))
                .corner_radius((TOOLGROUP.height as f32 / 2.0).max(1.0))
                .into_element();
        }

        root.into_element()
    }
}

/// A Neptunia toolbar icon button.
#[derive(PartialEq)]
pub struct ToolButton {
    label: String,
    icon: &'static str,
    disabled: bool,
    on_press: Option<Callback<(), ()>>,
    key: DiffKey,
}

impl ToolButton {
    /// Create a tool button with a mandatory accessibility label and an SVG icon.
    pub fn new(label: impl Into<String>, icon: &'static str) -> Self {
        Self {
            label: label.into(),
            icon,
            disabled: false,
            on_press: None,
            key: DiffKey::None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_press(mut self, callback: impl Into<Callback<(), ()>>) -> Self {
        self.on_press = Some(callback.into());
        self
    }

    pub fn key(mut self, key: impl Into<DiffKey>) -> Self {
        self.key = key.into();
        self
    }
}

impl KeyExt for ToolButton {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ToolButton {
    fn render(&self) -> impl IntoElement {
        let resolved = crate::theme::resolved();
        let palette = resolved.toolgroup.palette;
        let mut hovered = use_state(|| false);

        let island = resolved.sidebar.style == vow_appearance::SidebarStyle::Island;
        let (width, height, radius) = if island {
            (
                TOOLGROUP.button_w as f32,
                TOOLGROUP.button_h as f32,
                (TOOLGROUP.button_h as f32 / 2.0).max(1.0),
            )
        } else {
            (
                TOOLGROUP.flat_button_w as f32,
                TOOLGROUP.flat_button_h as f32,
                TOOLGROUP.flat_radius as f32,
            )
        };

        let fg = if self.disabled {
            palette.disabled_ink
        } else {
            palette.ink
        };
        let bg = if !self.disabled && *hovered.read() {
            rgba(resolved.sidebar.palette.hover_bg)
        } else {
            Color::TRANSPARENT
        };

        let mut root = rect()
            .width(Size::px(width))
            .height(Size::px(height))
            .corner_radius(radius)
            .background(bg)
            .cross_align(Alignment::Center)
            .center()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .a11y_focusable(true)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .child(svg_viewer(self.icon, fg, TOOLGROUP.separator_h));

        if !self.disabled
            && let Some(callback) = self.on_press.clone()
        {
            root = root.on_press(move |_| callback.call(()));
        }

        root
    }
}

fn svg_viewer(template: &str, color: Rgba, size: u32) -> SvgViewer {
    let [r, g, b, _] = color.to_rgba8();
    SvgViewer::new((
        format!("toolbutton-icon-{}", template.len()),
        Bytes::from(template.replace("currentColor", &format!("rgb({r},{g},{b})"))),
    ))
    .width(Size::px(size as f32))
    .height(Size::px(size as f32))
    .show_loader(false)
}

fn rgba(color: vow_design::Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyboard_types::{Key, Modifiers};

    #[test]
    fn shift_h_is_accepted_as_secure_field_text() {
        assert!(secure_field_accepts_text(
            &Key::Character("H".into()),
            Modifiers::SHIFT
        ));
    }

    #[test]
    fn ctrl_a_is_rejected_as_secure_field_text() {
        assert!(!secure_field_accepts_text(
            &Key::Character("a".into()),
            Modifiers::CONTROL
        ));
    }
}
