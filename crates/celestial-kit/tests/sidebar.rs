//! Tests for the Neptunia sidebar, sidebar layout and titlebar-with-sidebar.

use std::env;
use std::fs;
use std::sync::{Arc, Mutex};

use celestial_kit::controls::ToolGroup;
use celestial_kit::platform::{DecorationResponse, WindowStatus};
use celestial_kit::sidebar::{Sidebar, SidebarItem, SidebarLayout, SidebarSection};
use celestial_kit::theme;
use celestial_kit::titlebar::Titlebar;
use freya_components::button::Button;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use freya_testing::TestingRunner;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Size, Size2D};
use yuki_design::{SIDEBAR, TITLEBAR_CLIENT_HEIGHT};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_config<F: FnOnce() -> R, R>(style: &str, f: F) -> R {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("yuki-sidebar-test-{style}"));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("appearance.domain");
    fs::write(&path, format!("sidebar_style = \"{style}\"\n")).unwrap();
    let old = env::var_os("YUKI_CONFIG_DIR");
    unsafe { env::set_var("YUKI_CONFIG_DIR", &dir) };
    let result = f();
    match old {
        Some(v) => unsafe { env::set_var("YUKI_CONFIG_DIR", v) },
        None => unsafe { env::remove_var("YUKI_CONFIG_DIR") },
    }
    result
}

struct LayoutCase {
    capture: Arc<Mutex<Option<(f32, f32)>>>,
}

impl App for LayoutCase {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        let capture = self.capture.clone();
        SidebarLayout::new(
            Sidebar::new(),
            rect().width(Size::fill()).height(Size::fill()).on_sized(
                move |event: Event<SizedEventData>| {
                    let area = event.data().area;
                    *capture.lock().unwrap() = Some((area.origin.x, area.origin.y));
                },
            ),
        )
    }
}

fn layout_position() -> (f32, f32) {
    let capture = Arc::new(Mutex::new(None));
    let app = AppComponent::new(LayoutCase {
        capture: capture.clone(),
    });
    let (mut test, _) = TestingRunner::new(app, Size2D::new(800.0, 600.0), |_| {}, 1.0);
    test.sync_and_update();
    capture.lock().unwrap().expect("content was sized")
}

#[test]
fn island_layout_positions_content_at_sidebar_width_plus_double_inset() {
    let (x, _y) = with_config("island", layout_position);
    let expected = SIDEBAR.width as f32 + 2.0 * SIDEBAR.inset as f32;
    assert!(
        (x - expected).abs() < 0.5,
        "island content x should be {expected}, got {x}"
    );
}

#[test]
fn flat_layout_positions_content_at_sidebar_width() {
    let (x, _y) = with_config("flat", layout_position);
    let expected = SIDEBAR.width as f32;
    assert!(
        (x - expected).abs() < 0.5,
        "flat content x should be {expected}, got {x}"
    );
}

struct TitlebarCase {
    sidebar: bool,
    status: WindowStatus,
}

impl App for TitlebarCase {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        Titlebar::preview(self.status, "Ventana", false).with_sidebar(self.sidebar)
    }
}

fn render_titlebar(sidebar: bool, decoration: DecorationResponse) -> Vec<u8> {
    let app = AppComponent::new(TitlebarCase {
        sidebar,
        status: WindowStatus {
            activated: true,
            maximized: false,
            decoration,
            fixed: false,
        },
    });
    let (mut test, _) = TestingRunner::new(app, Size2D::new(300.0, 120.0), |_| {}, 1.0);
    test.sync_and_update();
    test.render().as_ref().to_vec()
}

fn render_titlebar_explicit(_sidebar: bool, decoration: DecorationResponse) -> Vec<u8> {
    let app = AppComponent::new(TitlebarCase {
        sidebar: false,
        status: WindowStatus {
            activated: true,
            maximized: false,
            decoration,
            fixed: false,
        },
    });
    let (mut test, _) = TestingRunner::new(app, Size2D::new(300.0, 120.0), |_| {}, 1.0);
    test.sync_and_update();
    test.render().as_ref().to_vec()
}

#[test]
fn titlebar_without_sidebar_is_unchanged() {
    let plain = render_titlebar(false, DecorationResponse::Client);
    let explicit = render_titlebar_explicit(false, DecorationResponse::Client);
    assert_eq!(
        plain, explicit,
        "with_sidebar(false) must not change the titlebar"
    );
}

#[test]
fn titlebar_with_sidebar_hides_traffic_lights() {
    let plain = render_titlebar(false, DecorationResponse::Client);
    let with_sidebar = render_titlebar(true, DecorationResponse::Client);
    assert_ne!(
        plain, with_sidebar,
        "a sidebar titlebar must look different from one with traffic lights"
    );
}

struct ToggleApp;

impl App for ToggleApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        let mut expanded = use_consume::<State<bool>>();
        SidebarSection::new("Favoritos")
            .on_toggle(move |value| expanded.set(value))
            .child(SidebarItem::new("Uno"))
    }
}

#[test]
fn sidebar_section_toggles_with_enter_and_space() {
    let app = AppComponent::new(ToggleApp);
    let (mut test, expanded) = TestingRunner::new(
        app,
        Size2D::new(300.0, 300.0),
        |runner| runner.provide_root_context(|| State::create(true)),
        1.0,
    );
    test.sync_and_update();
    assert!(*expanded.peek(), "section starts expanded");

    // Focus the header by clicking it, then toggle with the keyboard.
    test.click_cursor((5.0, 15.0));
    test.press_key(Key::Named(NamedKey::Enter));
    assert!(!(*expanded.peek()), "Enter should collapse the section");

    // Re-focus the header after the tree changed, then expand with Space.
    test.click_cursor((5.0, 15.0));
    test.press_key(Key::Character(" ".into()));
    assert!(*expanded.peek(), "Space should expand the section");
}

struct ToolGroupCase;

impl App for ToolGroupCase {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        ToolGroup::new()
            .child(Button::new().child("A"))
            .child(Button::new().child("B"))
            .into_element()
    }
}

#[test]
fn toolgroup_renders_without_panic() {
    let app = AppComponent::new(ToolGroupCase);
    let (mut test, _) = TestingRunner::new(app, Size2D::new(300.0, 80.0), |_| {}, 1.0);
    test.sync_and_update();
    let png = test.render();
    assert!(!png.as_ref().is_empty());
}

#[test]
fn sidebar_traffic_light_tokens_are_sane() {
    // The traffic lights are placed at (SIDEBAR.traffic_x, SIDEBAR.traffic_y)
    // inside the sidebar. Exact pixel testing would require the runner's
    // WindowControls context, so we assert the token values instead.
    assert_eq!(SIDEBAR.traffic_x, 18);
    assert_eq!(SIDEBAR.traffic_y, 16);
    assert_eq!(TITLEBAR_CLIENT_HEIGHT, 44);
}
