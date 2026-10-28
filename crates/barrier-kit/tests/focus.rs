//! Focus navigation tests for BarrierKit apps (F-01).
//!
//! These run headlessly through `freya-testing`, so they verify that the
//! platform focus plumbing (auto-focus, key delivery, Tab navigation) works
//! end-to-end without a live Wayland compositor.

use freya_components::input::Input;
use freya_core::integration::{App, AppComponent};
use freya_core::prelude::*;
use freya_testing::TestingRunner;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Direction, Size, Size2D};

struct FocusApp {
    value_a: State<String>,
    value_b: State<String>,
}

impl App for FocusApp {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Vertical)
            .child(Input::new(self.value_a.clone()).auto_focus(true))
            .child(Input::new(self.value_b.clone()))
    }
}

#[test]
fn auto_focused_input_receives_keys_and_tab_moves_focus() {
    let value_a = State::create_global(String::new());
    let value_b = State::create_global(String::new());

    let app = AppComponent::new(FocusApp {
        value_a: value_a.clone(),
        value_b: value_b.clone(),
    });
    let (mut test, _) = TestingRunner::new(app, Size2D::new(300.0, 200.0), |_| {}, 1.0);

    // Initial render: the first Input requested focus via auto_focus.
    test.sync_and_update();

    // Type into the focused input.
    test.write_text("a");
    assert_eq!(value_a.peek().as_str(), "a");
    assert!(value_b.peek().is_empty());

    // Tab moves focus to the next focusable control.
    test.press_key(Key::Named(NamedKey::Tab));
    // freya-testing applies the requested focus strategy at the START of the
    // next sync_and_update, so run one more cycle before typing again.
    test.sync_and_update();

    // The second input now receives keys.
    test.write_text("b");
    assert_eq!(value_a.peek().as_str(), "a");
    assert_eq!(value_b.peek().as_str(), "b");
}
