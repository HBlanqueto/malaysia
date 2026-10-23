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
use torin::prelude::{Direction, Gaps, Size};
use yuki_design::{TEXT, TEXT_LINE_HEIGHT};

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
