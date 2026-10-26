//! The BarrierKit client-side titlebar (ADR 0035).
//!
//! 44 px, traffic lights on the left, a centred title and a toolbar gap on the
//! right. It draws only when the compositor's configure answered `ClientSide`.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::traffic::TrafficLights;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Position, Size};
use vow_appearance::SidebarStyle;
use vow_appearance::palette;
use vow_appearance::traffic::TrafficInput;
use vow_design::NATIVE_APP_ID_PREFIX;
use vow_design::{ButtonKind, Rgba, TITLE_2, TITLEBAR_CLIENT_HEIGHT, TITLEBAR_DOUBLE_CLICK};

use crate::platform::{DecorationResponse, WindowControls, WindowEdge, WindowStatus};

/// A client-side titlebar for a BarrierKit window.
#[derive(PartialEq)]
pub struct Titlebar {
    preview: Option<Preview>,
    title: Option<String>,
    on_close: Option<Callback<(), ()>>,
    sidebar: bool,
    left: Option<Element>,
    right: Option<Element>,
}

#[derive(Clone, PartialEq)]
struct Preview {
    status: WindowStatus,
    title: String,
    fixed: bool,
}

impl Default for Titlebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Titlebar {
    /// A titlebar bound to the current window's runner context.
    pub fn new() -> Self {
        Self {
            preview: None,
            title: None,
            on_close: None,
            sidebar: false,
            left: None,
            right: None,
        }
    }

    /// A titlebar with fixed state, for previews and tests.
    pub fn preview(status: WindowStatus, title: impl Into<String>, fixed: bool) -> Self {
        Self {
            preview: Some(Preview {
                status,
                title: title.into(),
                fixed,
            }),
            title: None,
            on_close: None,
            sidebar: false,
            left: None,
            right: None,
        }
    }

    /// Override the title shown in the titlebar; when absent the window's
    /// configured title is used.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Called when the close light is released.
    pub fn on_close(mut self, callback: impl Into<Callback<(), ()>>) -> Self {
        self.on_close = Some(callback.into());
        self
    }

    /// Configure the titlebar to live above a sidebar; it will not draw its own
    /// traffic lights.
    pub fn with_sidebar(mut self, sidebar: bool) -> Self {
        self.sidebar = sidebar;
        self
    }

    /// Content for the left slot (next to the centred title).
    pub fn left(mut self, child: impl IntoElement) -> Self {
        self.left = Some(child.into_element());
        self
    }

    /// Content for the right slot (next to the centred title).
    pub fn right(mut self, child: impl IntoElement) -> Self {
        self.right = Some(child.into_element());
        self
    }
}

impl Component for Titlebar {
    fn render(&self) -> impl IntoElement {
        let (status, title, fixed) = if let Some(preview) = &self.preview {
            let title = self.title.clone().unwrap_or_else(|| preview.title.clone());
            (preview.status, title, preview.fixed)
        } else {
            let controls = WindowControls::get();
            let status = *controls.status.read();
            let title = self.title.clone().unwrap_or_else(|| controls.title.clone());
            (status, title, controls.fixed)
        };
        if status.decoration == DecorationResponse::Server {
            return rect()
                .width(Size::fill())
                .height(Size::px(0.0))
                .into_element();
        }

        let resolved = crate::theme::resolved();
        let theme = palette(resolved.scheme);
        let background = if self.sidebar {
            resolved.surfaces.window
        } else if status.activated {
            theme.titlebar_bg.focused
        } else {
            theme.titlebar_bg.unfocused
        };
        let title_color = if status.activated {
            theme.title.focused
        } else {
            theme.title.unfocused
        };
        let traffic = if self.sidebar {
            None
        } else {
            Some(window_traffic_lights(
                self.on_close.clone(),
                status.activated,
                fixed,
            ))
        };

        let mut title_label = label()
            .text(truncate(&title))
            .font_size(if self.sidebar {
                TITLE_2.size as f32
            } else {
                13.0
            })
            .color(rgba(title_color))
            .max_lines(1);
        if self.sidebar {
            title_label = title_label.font_weight(FontWeight::from(TITLE_2.weight as i32));
        }

        if self.sidebar {
            // Sidebar titlebars keep the toolbar on the left and place the
            // title immediately after it; the right slot is intentionally empty.
            let left_slot = rect()
                .width(Size::auto())
                .height(Size::fill())
                .cross_align(Alignment::Center)
                .a11y_alt("titlebar-left-slot")
                .maybe(self.left.is_some(), |el| {
                    el.child(self.left.clone().unwrap())
                });

            let title = rect()
                .width(Size::auto())
                .height(Size::fill())
                .cross_align(Alignment::Center)
                .a11y_alt("titlebar-title")
                .on_mouse_down(window_drag_handler(fixed))
                .child(title_label);

            let mut root = rect()
                .width(Size::fill())
                .height(Size::px(TITLEBAR_CLIENT_HEIGHT as f32))
                .background(rgba(background))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                .child(left_slot)
                .child(title);

            if resolved.sidebar.style == SidebarStyle::Flat {
                root = root.child(
                    rect()
                        .position(Position::new_absolute().bottom(0.0).left(0.0))
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .background(rgba(resolved.sidebar.palette.line)),
                );
            }

            return root.into_element();
        }

        // Classic three-slot titlebar for windows without a sidebar.
        let left_slot = rect()
            .width(Size::px(80.0))
            .height(Size::fill())
            .cross_align(Alignment::Center)
            .a11y_alt("titlebar-left-slot")
            .maybe(self.left.is_some(), |el| {
                el.child(self.left.clone().unwrap())
            });
        let left_slot = if let Some(traffic) = traffic {
            left_slot.center().child(traffic)
        } else {
            left_slot
        };

        let right_slot = rect()
            .width(Size::px(80.0))
            .height(Size::fill())
            .cross_align(Alignment::Center)
            .a11y_alt("titlebar-right-slot")
            .maybe(self.right.is_some(), |el| {
                el.child(self.right.clone().unwrap())
            })
            .on_mouse_down(window_drag_handler(fixed));

        let title = rect()
            .width(Size::flex(1.0))
            .height(Size::fill())
            .cross_align(Alignment::Center)
            .main_align(Alignment::Center)
            .a11y_alt("titlebar-title")
            .on_mouse_down(window_drag_handler(fixed))
            .child(title_label);

        let mut root = rect()
            .width(Size::fill())
            .height(Size::px(TITLEBAR_CLIENT_HEIGHT as f32))
            .background(rgba(background))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .child(left_slot)
            .child(title)
            .child(right_slot);

        if self.sidebar && resolved.sidebar.style == SidebarStyle::Flat {
            root = root.child(
                rect()
                    .position(Position::new_absolute().bottom(0.0).left(0.0))
                    .width(Size::fill())
                    .height(Size::px(1.0))
                    .background(rgba(resolved.sidebar.palette.line)),
            );
        }

        root.into_element()
    }
}

/// Builds the window's traffic lights with the standard close/minimize/zoom
/// actions, reading `window.domain` like the compositor does.
pub fn window_traffic_lights(
    on_close: Option<Callback<(), ()>>,
    focused: bool,
    fixed: bool,
) -> TrafficLights {
    let behavior = *window_behavior();
    let green = green_action(behavior.green_button, fixed);
    TrafficLights::new()
        .focused(focused)
        .zoom_supported(green_admitted(behavior.green_button, fixed))
        .fixed_width(fixed)
        .on_action(move |kind| {
            let controls = WindowControls::get();
            match kind {
                ButtonKind::Close => {
                    if let Some(on_close) = &on_close {
                        on_close.call(());
                    }
                }
                ButtonKind::Minimize => controls.minimize(),
                ButtonKind::Maximize if fixed => controls.toggle_vertical_maximize(),
                ButtonKind::Maximize => run_action(&controls, green),
            }
        })
}

/// Returns a mouse-down handler that drags the window and honours the double-click
/// action configured in `window.domain`.
///
/// `fixed` is stored in a reactive cell so the handler keeps using the latest
/// value when the window's fixed state or decoration changes between renders.
pub fn window_drag_handler(fixed: bool) -> impl Fn(Event<MouseEventData>) {
    let last_click = use_hook(|| Rc::new(Cell::new(None::<(Instant, (f64, f64))>)));
    let double_done = use_hook(|| Rc::new(Cell::new(false)));
    let fixed_cell = use_hook(|| Rc::new(Cell::new(fixed)));
    fixed_cell.set(fixed);
    move |event: Event<MouseEventData>| {
        if event.button != Some(MouseButton::Left) {
            return;
        }
        let fixed = fixed_cell.get();
        let controls = WindowControls::get();
        let behavior = *window_behavior();
        let now = Instant::now();
        let point = (event.element_location.x, event.element_location.y);
        let double = last_click.get().is_some_and(|(then, before)| {
            now.duration_since(then)
                <= Duration::from_millis(u64::from(TITLEBAR_DOUBLE_CLICK.interval_ms))
                && (before.0 - point.0).abs() <= TITLEBAR_DOUBLE_CLICK.slop
                && (before.1 - point.1).abs() <= TITLEBAR_DOUBLE_CLICK.slop
        });
        if double {
            last_click.set(None);
            if !double_done.get() {
                double_done.set(true);
                run_action(
                    &controls,
                    double_click_action(behavior.double_click_titlebar, fixed),
                );
            }
        } else {
            last_click.set(Some((now, point)));
            double_done.set(false);
            controls.move_window();
        }
    }
}

/// What the green traffic light does (same values as the compositor).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum GreenButtonAction {
    /// Enter focus mode (pending).
    #[default]
    Focus,
    /// Zoom the window to the workspace.
    Zoom,
    /// Enter fullscreen (pending).
    Fullscreen,
    /// Do nothing.
    None,
}

/// What a double click on the title bar does (same values as the compositor).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DoubleClickAction {
    /// Expand to the workspace (treated as zoom today).
    Fill,
    /// Minimize the window.
    Minimize,
    /// Zoom the window.
    #[default]
    Zoom,
    /// Do nothing.
    None,
}

/// The `window.domain` keys this titlebar consumes.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default)]
struct WindowBehavior {
    double_click_titlebar: DoubleClickAction,
    green_button: GreenButtonAction,
}

/// The user's window behaviour, read once from `window.domain` like the
/// compositor does. A missing or unparsable file keeps the defaults.
fn window_behavior() -> &'static WindowBehavior {
    static BEHAVIOR: OnceLock<WindowBehavior> = OnceLock::new();
    BEHAVIOR.get_or_init(|| {
        let path = vow_appearance::config_dir().join("window.domain");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    })
}

/// The action a titlebar control requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitlebarAction {
    Focus,
    Maximize,
    Minimize,
    Fullscreen,
    None,
}

fn green_action(action: GreenButtonAction, fixed: bool) -> TitlebarAction {
    match action {
        GreenButtonAction::Focus => TitlebarAction::Focus,
        GreenButtonAction::Zoom if fixed => TitlebarAction::None,
        GreenButtonAction::Zoom => TitlebarAction::Maximize,
        GreenButtonAction::Fullscreen => TitlebarAction::Fullscreen,
        GreenButtonAction::None => TitlebarAction::None,
    }
}

fn green_admitted(action: GreenButtonAction, fixed: bool) -> bool {
    action != GreenButtonAction::Zoom || !fixed
}

fn double_click_action(action: DoubleClickAction, fixed: bool) -> TitlebarAction {
    match action {
        DoubleClickAction::Fill | DoubleClickAction::Zoom if fixed => TitlebarAction::None,
        DoubleClickAction::Fill | DoubleClickAction::Zoom => TitlebarAction::Maximize,
        DoubleClickAction::Minimize => TitlebarAction::Minimize,
        DoubleClickAction::None => TitlebarAction::None,
    }
}

fn run_action(controls: &WindowControls, action: TitlebarAction) {
    match action {
        TitlebarAction::Maximize => controls.toggle_maximize(),
        TitlebarAction::Minimize => controls.minimize(),
        TitlebarAction::Focus => {
            eprintln!("focus mode requested window={:?}", controls.surface_id());
        }
        TitlebarAction::Fullscreen => {
            eprintln!("fullscreen requested window={:?}", controls.surface_id());
        }
        TitlebarAction::None => {}
    }
}

fn truncate(title: &str) -> String {
    if title.chars().count() <= 42 {
        return title.to_string();
    }
    let mut out = title.chars().take(41).collect::<String>();
    out.push('…');
    out
}

fn rgba(color: Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

/// The app-id for a BarrierKit window with the given suffix.
pub fn app_id(app_name: &str) -> String {
    format!("{NATIVE_APP_ID_PREFIX}{app_name}")
}

#[allow(dead_code)]
fn _traffic_input(status: WindowStatus, fixed: bool) -> TrafficInput {
    TrafficInput {
        focused: status.activated,
        group_hovered: false,
        pressed: None,
        zoom_supported: !fixed,
    }
}

/// CSD resize edges and corners. Put it as the last child of the window root
/// so it receives pointer events before the content.
pub fn resize_edges() -> Element {
    let controls = WindowControls::get();
    let make = |edge: WindowEdge, position: Position, width: Size, height: Size| {
        let controls = controls.clone();
        rect()
            .position(position)
            .width(width)
            .height(height)
            .background(Color::TRANSPARENT)
            .on_mouse_down(move |event: Event<MouseEventData>| {
                if event.button == Some(MouseButton::Left) {
                    controls.resize(edge);
                }
            })
            .into_element()
    };
    let edge = Size::px(6.0);
    let corner = Size::px(12.0);
    // Absolute positioned overlays do not resolve Size::fill() in this version
    // of Freya; root percentage keeps the hit-area matching the window.
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::window_percent(100.0))
        .height(Size::window_percent(100.0))
        .background(Color::TRANSPARENT)
        .child(make(
            WindowEdge::Top,
            Position::new_absolute().top(0.0).left(0.0),
            Size::fill(),
            edge.clone(),
        ))
        .child(make(
            WindowEdge::Bottom,
            Position::new_absolute().bottom(0.0).left(0.0),
            Size::fill(),
            edge.clone(),
        ))
        .child(make(
            WindowEdge::Left,
            Position::new_absolute().top(0.0).left(0.0),
            edge.clone(),
            Size::fill(),
        ))
        .child(make(
            WindowEdge::Right,
            Position::new_absolute().top(0.0).right(0.0),
            edge.clone(),
            Size::fill(),
        ))
        .child(make(
            WindowEdge::TopLeft,
            Position::new_absolute().top(0.0).left(0.0),
            corner.clone(),
            corner.clone(),
        ))
        .child(make(
            WindowEdge::TopRight,
            Position::new_absolute().top(0.0).right(0.0),
            corner.clone(),
            corner.clone(),
        ))
        .child(make(
            WindowEdge::BottomLeft,
            Position::new_absolute().bottom(0.0).left(0.0),
            corner.clone(),
            corner.clone(),
        ))
        .child(make(
            WindowEdge::BottomRight,
            Position::new_absolute().bottom(0.0).right(0.0),
            corner.clone(),
            corner,
        ))
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_green_light_uses_the_configured_action() {
        assert_eq!(
            green_action(GreenButtonAction::Focus, false),
            TitlebarAction::Focus
        );
        assert_eq!(
            green_action(GreenButtonAction::Zoom, false),
            TitlebarAction::Maximize
        );
        assert_eq!(
            green_action(GreenButtonAction::Zoom, true),
            TitlebarAction::None
        );
        assert_eq!(
            green_action(GreenButtonAction::Fullscreen, false),
            TitlebarAction::Fullscreen
        );
        assert_eq!(
            green_action(GreenButtonAction::None, false),
            TitlebarAction::None
        );
        assert!(green_admitted(GreenButtonAction::Focus, true));
        assert!(!green_admitted(GreenButtonAction::Zoom, true));
    }

    #[test]
    fn the_double_click_uses_the_configured_action() {
        assert_eq!(
            double_click_action(DoubleClickAction::Zoom, false),
            TitlebarAction::Maximize
        );
        assert_eq!(
            double_click_action(DoubleClickAction::Fill, false),
            TitlebarAction::Maximize
        );
        assert_eq!(
            double_click_action(DoubleClickAction::Minimize, false),
            TitlebarAction::Minimize
        );
        assert_eq!(
            double_click_action(DoubleClickAction::None, false),
            TitlebarAction::None
        );
        assert_eq!(
            double_click_action(DoubleClickAction::Zoom, true),
            TitlebarAction::None
        );
    }

    #[test]
    fn window_domain_defaults_match_the_compositor() {
        let behavior: WindowBehavior = toml::from_str("").expect("empty domain");
        assert_eq!(behavior.green_button, GreenButtonAction::Focus);
        assert_eq!(behavior.double_click_titlebar, DoubleClickAction::Zoom);

        let behavior: WindowBehavior =
            toml::from_str("green_button = \"zoom\"\ndouble_click_titlebar = \"minimize\"\n")
                .expect("window.domain");
        assert_eq!(behavior.green_button, GreenButtonAction::Zoom);
        assert_eq!(behavior.double_click_titlebar, DoubleClickAction::Minimize);
    }
}
