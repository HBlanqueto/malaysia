//! CelestialKit controls with macOS-style names, built on `freya-components`
//! or on the core `rect`/`label` primitives (ADR 0034). Measurements and
//! colours come from the theme and `yuki-design`.

pub use freya_components::button::Button;
pub use freya_components::input::Input as TextField;
pub use freya_components::menu::Menu;
pub use freya_components::popup::Popup as Popover;
pub use freya_components::scrollviews::ScrollView;
pub use freya_components::segmented_button::SegmentedButton as SegmentedControl;
pub use freya_components::slider::Slider;
pub use freya_components::switch::Switch;

use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_appearance::palette;
use yuki_design::{TEXT, TEXT_LINE_HEIGHT, TOOLGROUP};

use crate::password::PasswordField;

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
                    Key::Character(text) if event.modifiers.is_empty() => {
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
        Self {
            children: Vec::new(),
            style: ToolGroupStyle::Island,
            key: DiffKey::None,
        }
    }

    /// Use the flat style (no background capsule).
    pub fn flat(mut self) -> Self {
        self.style = ToolGroupStyle::Flat;
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
        let palette = palette(resolved.scheme);

        let mut items: Vec<Element> = Vec::with_capacity(self.children.len().saturating_mul(2));
        let last = self.children.len().saturating_sub(1);
        for (i, child) in self.children.iter().cloned().enumerate() {
            items.push(child);
            if i != last {
                items.push(
                    rect()
                        .width(Size::px(TOOLGROUP.separator_w as f32))
                        .height(Size::px(TOOLGROUP.separator_h as f32))
                        .background(rgba(palette.hairline))
                        .into_element(),
                );
            }
        }

        let mut root = rect()
            .direction(Direction::Horizontal)
            .height(Size::px(TOOLGROUP.height as f32))
            .cross_align(Alignment::Center)
            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
            .children(items);

        if self.style == ToolGroupStyle::Island {
            root = root
                .background(rgba(resolved.toolgroup.bg))
                .corner_radius((TOOLGROUP.height as f32 / 2.0).max(1.0));
        }

        root
    }
}

fn rgba(color: yuki_design::Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}
