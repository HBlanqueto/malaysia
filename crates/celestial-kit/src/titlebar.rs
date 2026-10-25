//! The CelestialKit client-side titlebar (ADR 0035).
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
use yuki_appearance::palette;
use yuki_appearance::traffic::TrafficInput;
use yuki_design::NATIVE_APP_ID_PREFIX;
use yuki_design::{ButtonKind, Rgba, TITLEBAR_CLIENT_HEIGHT, TITLEBAR_DOUBLE_CLICK};

use crate::platform::{DecorationResponse, WindowControls, WindowEdge, WindowStatus};

/// A client-side titlebar for a CelestialKit window.
#[derive(PartialEq)]
pub struct Titlebar {
    preview: Option<Preview>,
    on_close: Option<Callback<(), ()>>,
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
            on_close: None,
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
            on_close: None,
        }
    }

    /// Called when the close light is released.
    pub fn on_close(mut self, callback: impl Into<Callback<(), ()>>) -> Self {
        self.on_close = Some(callback.into());
        self
    }
}

impl Component for Titlebar {
    fn render(&self) -> impl IntoElement {
        let (status, title, fixed) = if let Some(preview) = &self.preview {
            (preview.status, preview.title.clone(), preview.fixed)
        } else {
            let controls = WindowControls::get();
            let status = *controls.status.read();
            (status, controls.title.clone(), controls.fixed)
        };
        let last_click = use_hook(|| Rc::new(Cell::new(None::<(Instant, (f64, f64))>)));
        let double_done = use_hook(|| Rc::new(Cell::new(false)));
        if status.decoration == DecorationResponse::Server {
            return rect()
                .width(Size::fill())
                .height(Size::px(0.0))
                .into_element();
        }

        let controls = if self.preview.is_none() {
            Some(WindowControls::get())
        } else {
            None
        };
        let resolved = crate::theme::resolved();
        let theme = palette(resolved.scheme);
        let background = if status.activated {
            theme.titlebar_bg.focused
        } else {
            theme.titlebar_bg.unfocused
        };
        let title_color = if status.activated {
            theme.title.focused
        } else {
            theme.title.unfocused
        };
        let behavior = *window_behavior();
        let close = self.on_close.clone();
        let minimize = controls.clone();
        let maximize = controls.clone();
        let drag = controls.clone();
        let click = controls.clone();
        let green = green_action(behavior.green_button, fixed);

        let traffic = TrafficLights::new()
            .focused(status.activated)
            .zoom_supported(green_admitted(behavior.green_button, fixed))
            .on_action(move |kind| match kind {
                ButtonKind::Close => {
                    if let Some(close) = &close {
                        close.call(());
                    }
                }
                ButtonKind::Minimize => {
                    if let Some(controls) = &minimize {
                        controls.minimize();
                    }
                }
                ButtonKind::Maximize => {
                    if let Some(controls) = &maximize {
                        run_action(controls, green);
                    }
                }
            });

        let press = {
            let last_click = last_click.clone();
            let double_done = double_done.clone();
            move |event: Event<MouseEventData>| {
                if event.button != Some(MouseButton::Left) {
                    return;
                }
                let Some(controls) = &drag else {
                    return;
                };
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
                            controls,
                            double_click_action(behavior.double_click_titlebar, fixed),
                        );
                    }
                } else {
                    last_click.set(Some((now, point)));
                    double_done.set(false);
                    controls.move_window();
                }
            }
        };
        let press_second = {
            let last_click = last_click.clone();
            let double_done = double_done.clone();
            move |event: Event<MouseEventData>| {
                if event.button != Some(MouseButton::Left) {
                    return;
                }
                let Some(controls) = &click else {
                    return;
                };
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
                            controls,
                            double_click_action(behavior.double_click_titlebar, fixed),
                        );
                    }
                } else {
                    last_click.set(Some((now, point)));
                    double_done.set(false);
                    controls.move_window();
                }
            }
        };

        rect()
            .width(Size::fill())
            .height(Size::px(TITLEBAR_CLIENT_HEIGHT as f32))
            .background(rgba(background))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .child(
                rect()
                    .width(Size::px(80.0))
                    .height(Size::fill())
                    .center()
                    .child(traffic),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .center()
                    .on_mouse_down(press)
                    .child(
                        label()
                            .text(truncate(&title))
                            .font_size(13.0)
                            .color(rgba(title_color))
                            .max_lines(1),
                    ),
            )
            .child(
                rect()
                    .width(Size::px(80.0))
                    .height(Size::fill())
                    .on_mouse_down(press_second),
            )
            .into_element()
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
        let path = yuki_appearance::config_dir().join("window.domain");
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

/// The app-id for a CelestialKit window with the given suffix.
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

/// Four 6 px CSD resize edges. Put it as the last child of the window root.
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
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::fill())
        .height(Size::fill())
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
            edge,
            Size::fill(),
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
