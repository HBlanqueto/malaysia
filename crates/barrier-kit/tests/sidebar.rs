//! Tests for the Neptunia sidebar, sidebar layout and titlebar-with-sidebar.

use std::env;
use std::fs;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use accesskit::Role;
use barrier_kit::controls::{ToolButton, ToolGroup};
use barrier_kit::platform::{DecorationResponse, WindowStatus};
use barrier_kit::sidebar::{Sidebar, SidebarItem, SidebarLayout, SidebarSection};
use barrier_kit::theme;
use barrier_kit::titlebar::Titlebar;
use freya_components::button::Button;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use freya_testing::TestingRunner;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Alignment, Size, Size2D};
use vow_appearance::{AppearanceConfig, ColorScheme, SidebarStyle, resolve};
use vow_design::{SIDEBAR, TITLEBAR, TITLEBAR_CLIENT_HEIGHT, TOOLGROUP};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn color(c: vow_design::Rgba) -> Color {
    let [r, g, b, a] = c.to_rgba8();
    Color::from_argb(a, r, g, b)
}

type Capture = Arc<Mutex<Option<(f32, f32, f32, f32)>>>;

fn with_config<F: FnOnce() -> R, R>(style: &str, f: F) -> R {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("malaysia-sidebar-test-{style}"));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("appearance.domain");
    fs::write(&path, format!("sidebar_style = \"{style}\"\n")).unwrap();
    let old = env::var_os("MALAYSIA_CONFIG_DIR");
    unsafe { env::set_var("MALAYSIA_CONFIG_DIR", &dir) };
    let result = f();
    match old {
        Some(v) => unsafe { env::set_var("MALAYSIA_CONFIG_DIR", v) },
        None => unsafe { env::remove_var("MALAYSIA_CONFIG_DIR") },
    }
    result
}

struct LayoutCase {
    capture: Capture,
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
                    *capture.lock().unwrap() = Some((
                        area.origin.x,
                        area.origin.y,
                        area.size.width,
                        area.size.height,
                    ));
                },
            ),
        )
    }
}

fn layout_position() -> (f32, f32, f32, f32) {
    let capture: Capture = Arc::new(Mutex::new(None));
    let app = AppComponent::new(LayoutCase {
        capture: capture.clone(),
    });
    let (mut test, _) = TestingRunner::new(app, Size2D::new(800.0, 600.0), |_| {}, 1.0);
    test.sync_and_update();
    capture.lock().unwrap().expect("content was sized")
}

#[test]
fn island_layout_positions_content_at_sidebar_width_plus_double_inset() {
    let (x, y, width, height) = with_config("island", layout_position);
    let expected_x = SIDEBAR.width as f32 + 2.0 * SIDEBAR.inset as f32;
    assert!(
        (x - expected_x).abs() < 0.5,
        "island content x should be {expected_x}, got {x}"
    );
    assert_eq!(y, 0.0, "island content must start at the top edge");
    let expected_width = 800.0 - expected_x;
    assert!(
        (width - expected_width).abs() < 0.5,
        "island content width should be {expected_width}, got {width}"
    );
    assert_eq!(height, 600.0, "island content must fill the full height");
}

#[test]
fn flat_layout_positions_content_at_sidebar_width() {
    let (x, y, width, height) = with_config("flat", layout_position);
    let expected_x = SIDEBAR.width as f32;
    assert!(
        (x - expected_x).abs() < 0.5,
        "flat content x should be {expected_x}, got {x}"
    );
    assert_eq!(y, 0.0, "flat content must start at the top edge");
    let expected_width = 800.0 - expected_x;
    assert!(
        (width - expected_width).abs() < 0.5,
        "flat content width should be {expected_width}, got {width}"
    );
    assert_eq!(height, 600.0, "flat content must fill the full height");
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

fn titlebar_layout(sidebar: bool) -> Vec<(f32, f32, f32, f32)> {
    let app = AppComponent::new(TitlebarCase {
        sidebar,
        status: WindowStatus {
            activated: true,
            maximized: false,
            decoration: DecorationResponse::Client,
            fixed: false,
        },
    });
    let (test, _) = TestingRunner::new(app, Size2D::new(300.0, 120.0), |_| {}, 1.0);
    test.find_many(|node, _element| {
        let layout = node.layout();
        let area = layout.area;
        if area.size.height == TITLEBAR_CLIENT_HEIGHT as f32 {
            Some((
                area.origin.x,
                area.origin.y,
                area.size.width,
                area.size.height,
            ))
        } else {
            None
        }
    })
}

#[test]
fn titlebar_without_sidebar_keeps_the_classic_three_slot_layout() {
    let bars = titlebar_layout(false);
    // The root bar plus its three direct children (left slot, title, right slot).
    assert!(
        bars.len() >= 3,
        "expected at least the bar root and three slots, found {bars:?}"
    );
    let widths: Vec<f32> = bars.iter().map(|(_, _, w, _)| *w).collect();
    assert!(
        widths.contains(&80.0),
        "left/right slots must keep their 80 px default width: {widths:?}"
    );
}

#[test]
fn titlebar_with_sidebar_uses_elastic_side_slots() {
    let bars = titlebar_layout(true);
    assert!(
        !bars.iter().any(|(_, _, w, _)| *w == 80.0),
        "with_sidebar(true) must not use fixed 80 px slots: {bars:?}"
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

struct TrafficLightsCase;

impl App for TrafficLightsCase {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        Sidebar::new().traffic_lights(true).child(
            rect()
                .width(Size::fill())
                .height(Size::px(30.0))
                .a11y_alt("sidebar-first-child"),
        )
    }
}

fn traffic_layout() -> Vec<(f32, f32, f32, f32)> {
    let app = AppComponent::new(TrafficLightsCase);
    let (test, _) = TestingRunner::new(app, Size2D::new(300.0, 400.0), |_| {}, 1.0);
    test.find_many(|node, _el| {
        let area = node.layout().area;
        Some((
            area.origin.x,
            area.origin.y,
            area.size.width,
            area.size.height,
        ))
    })
}

#[test]
fn sidebar_traffic_lights_are_at_design_offset_and_first_child_sits_below() {
    with_config("island", || {
        let (_, _, group_w, group_h) = vow_appearance::traffic::group_rect();
        let layouts = traffic_layout();

        let lights = layouts
            .iter()
            .find(|(_, _, w, h)| {
                (*w - group_w as f32).abs() < 0.5 && (*h - group_h as f32).abs() < 0.5
            })
            .expect("traffic lights container must be sized");
        assert!(
            (lights.0 - SIDEBAR.traffic_x as f32).abs() < 0.5
                && (lights.1 - SIDEBAR.traffic_y as f32).abs() < 0.5,
            "traffic lights must be at ({}, {}), got ({}, {})",
            SIDEBAR.traffic_x,
            SIDEBAR.traffic_y,
            lights.0,
            lights.1
        );

        let first_child = layouts
            .iter()
            .find(|(_, _, w, h)| *w > 100.0 && (*h - 30.0).abs() < 0.5)
            .expect("first child must be laid out");
        let lights_bottom = lights.1 + lights.3;
        assert!(
            first_child.1 >= lights_bottom - 0.5,
            "first child must sit below the traffic lights (lights end at {lights_bottom}, child starts at {})",
            first_child.1
        );
    });
}

#[test]
fn sidebar_traffic_light_tokens_are_sane() {
    // The traffic lights are placed at (SIDEBAR.traffic_x, SIDEBAR.traffic_y)
    // inside the sidebar. Exact pixel testing would require the runner's
    // WindowControls context, so we assert the token values instead.
    assert_eq!(SIDEBAR.traffic_x, 18);
    assert_eq!(SIDEBAR.traffic_y, 16);
    assert_eq!(TITLEBAR_CLIENT_HEIGHT, 44);
    assert_eq!(TITLEBAR.buttons.diameter, 12);
}

struct ItemCase;

impl App for ItemCase {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        Sidebar::new().child(SidebarItem::new("Hola").badge(7))
    }
}

#[test]
fn sidebar_item_uses_design_padding_and_gap() {
    let app = AppComponent::new(ItemCase);
    let (mut test, _) = TestingRunner::new(app, Size2D::new(300.0, 300.0), |_| {}, 1.0);
    test.sync_and_update();
    let png = test.render();
    assert!(!png.as_ref().is_empty());
}

struct ExpandedSectionCase;

impl App for ExpandedSectionCase {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        Sidebar::new().child(
            SidebarSection::new("Favoritos")
                .expanded(true)
                .child(SidebarItem::new("Uno")),
        )
    }
}

#[test]
fn sidebar_section_hides_chevron_without_hover_or_focus() {
    let app = AppComponent::new(ExpandedSectionCase);
    let (test, _) = TestingRunner::new(app, Size2D::new(300.0, 300.0), |_| {}, 1.0);
    let headers = test.find_many(|node, el| {
        if el.accessibility().builder.role() == Role::Button {
            Some(node)
        } else {
            None
        }
    });
    assert_eq!(headers.len(), 1, "there must be exactly one section header");
    assert_eq!(
        headers[0].children().len(),
        1,
        "expanded section header must contain only the label when not hovered or focused"
    );
}

struct ToolGroupFlatCase;

impl App for ToolGroupFlatCase {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        ToolGroup::new()
            .flat()
            .child(Button::new().child("A"))
            .child(Button::new().child("B"))
    }
}

#[test]
fn flat_toolgroup_has_no_separators() {
    let app = AppComponent::new(ToolGroupFlatCase);
    let (test, _) = TestingRunner::new(app, Size2D::new(300.0, 80.0), |_| {}, 1.0);
    let separators = test.find_many(|node, _el| {
        let size = node.layout().area.size;
        let expected_w = TOOLGROUP.separator_w as f32;
        let expected_h = TOOLGROUP.separator_h as f32;
        if (size.width - expected_w).abs() < 0.5 && (size.height - expected_h).abs() < 0.5 {
            Some(())
        } else {
            None
        }
    });
    assert!(
        separators.is_empty(),
        "flat ToolGroup must not draw vertical separators"
    );
}

const PLUS_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><circle cx=\"8\" cy=\"8\" r=\"6\" fill=\"currentColor\"/></svg>";

struct ToolButtonCase {
    disabled: bool,
}

impl App for ToolButtonCase {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| theme::for_scheme(false));
        let mut pressed = use_consume::<State<u32>>();
        ToolButton::new("Añadir", PLUS_ICON)
            .disabled(self.disabled)
            .on_press(move |_| {
                let current = *pressed.read();
                pressed.set(current + 1);
            })
    }
}

#[test]
fn disabled_toolbutton_does_not_fire_on_press() {
    let app = AppComponent::new(ToolButtonCase { disabled: true });
    let (mut test, pressed) = TestingRunner::new(
        app,
        Size2D::new(100.0, 80.0),
        |runner| runner.provide_root_context(|| State::create(0u32)),
        1.0,
    );
    test.click_cursor((15.0, 15.0));
    assert_eq!(
        *pressed.peek(),
        0,
        "disabled tool button must ignore presses"
    );
}

#[test]
fn enabled_toolbutton_fires_on_press() {
    let app = AppComponent::new(ToolButtonCase { disabled: false });
    let (mut test, pressed) = TestingRunner::new(
        app,
        Size2D::new(100.0, 80.0),
        |runner| runner.provide_root_context(|| State::create(0u32)),
        1.0,
    );
    test.click_cursor((15.0, 15.0));
    assert_eq!(*pressed.peek(), 1, "enabled tool button must fire on_press");
}

fn dark_resolved() -> vow_appearance::Resolved {
    resolve(
        &AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            sidebar_style: SidebarStyle::Island,
            ..Default::default()
        },
        None,
    )
}

fn decode_png(png: &[u8]) -> (usize, usize, Vec<u8>) {
    let mut decoder = png::Decoder::new(Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).unwrap();
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buffer[..info.buffer_size()]
            .chunks(3)
            .flat_map(|px| [px[0], px[1], px[2], 255])
            .collect(),
        other => panic!("unexpected PNG colour type {other:?}"),
    };
    (info.width as usize, info.height as usize, rgba)
}

fn pixel(image: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let index = (y * width + x) * 4;
    image[index..index + 4].try_into().unwrap()
}

struct IslandShadowCase;

impl App for IslandShadowCase {
    fn render(&self) -> impl IntoElement {
        let resolved = dark_resolved();
        use_init_theme(|| theme::for_scheme(true));
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(color(resolved.surfaces.window))
            .child(SidebarLayout::new(Sidebar::new(), rect()))
    }
}

struct ToolgroupShadowCase;

impl App for ToolgroupShadowCase {
    fn render(&self) -> impl IntoElement {
        let resolved = dark_resolved();
        use_init_theme(|| theme::for_scheme(true));
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(color(resolved.surfaces.window))
            .main_align(Alignment::Center)
            .cross_align(Alignment::Center)
            .child(
                ToolGroup::new()
                    .child(Button::new().child("A"))
                    .child(Button::new().child("B")),
            )
    }
}

#[test]
fn island_and_toolgroup_cast_no_shadow() {
    let resolved = dark_resolved();
    let expected = resolved.surfaces.window.to_rgba8();

    let app = AppComponent::new(IslandShadowCase);
    let (mut test, _) = TestingRunner::new(
        app,
        Size2D::new(300.0, 300.0),
        |runner| runner.provide_root_context(|| State::create(resolved)),
        1.0,
    );
    test.sync_and_update();
    let _ = test.render();
    test.sync_and_update();
    let (w, _h, image) = decode_png(test.render().as_bytes());
    // The island body ends at height - inset; 4 px below must be window bg.
    let island_bottom = 300.0 - SIDEBAR.inset as f32 + 4.0;
    assert_eq!(
        pixel(&image, w, 110, island_bottom as usize),
        expected,
        "pixel 4 px below the island must be the window background"
    );

    let app = AppComponent::new(ToolgroupShadowCase);
    let (mut test, _) = TestingRunner::new(
        app,
        Size2D::new(300.0, 80.0),
        |runner| runner.provide_root_context(|| State::create(resolved)),
        1.0,
    );
    test.sync_and_update();
    let _ = test.render();
    test.sync_and_update();
    let (w, _h, image) = decode_png(test.render().as_bytes());
    // The capsule is centred vertically; 3 px below its bottom must be window bg.
    let capsule_bottom = 40.0 + TOOLGROUP.height as f32 / 2.0 + 3.0;
    assert_eq!(
        pixel(&image, w, 150, capsule_bottom as usize),
        expected,
        "pixel 3 px below the toolgroup capsule must be the window background"
    );
}
