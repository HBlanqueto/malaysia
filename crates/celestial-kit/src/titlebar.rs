//! The CelestialKit client-side titlebar (ADR 0035).
//!
//! 44 px, traffic lights on the left, a centred title and a toolbar gap on the
//! right. It draws only when the compositor's configure answered `ClientSide`.

use std::time::{Duration, Instant};

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
        let (status, title, _fixed) = if let Some(preview) = &self.preview {
            (preview.status, preview.title.clone(), preview.fixed)
        } else {
            let controls = WindowControls::get();
            let status = *controls.status.read();
            (status, controls.title.clone(), controls.fixed)
        };
        let last_click = use_state(|| None::<(Instant, (f64, f64))>);
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
        let close = self.on_close.clone();
        let minimize = controls.clone();
        let maximize = controls.clone();
        let drag = controls.clone();
        let click = controls.clone();

        let traffic = TrafficLights::new()
            .focused(status.activated)
            .zoom_supported(!status.fixed)
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
                        controls.toggle_maximize();
                    }
                }
            });

        let press = move |event: Event<MouseEventData>| {
            if event.button != Some(MouseButton::Left) {
                return;
            }
            let Some(controls) = &drag else {
                return;
            };
            let now = Instant::now();
            let point = (event.element_location.x, event.element_location.y);
            let double = last_click.read().is_some_and(|(then, before)| {
                now.duration_since(then)
                    <= Duration::from_millis(u64::from(TITLEBAR_DOUBLE_CLICK.interval_ms))
                    && (before.0 - point.0).abs() <= TITLEBAR_DOUBLE_CLICK.slop
                    && (before.1 - point.1).abs() <= TITLEBAR_DOUBLE_CLICK.slop
            });
            if double {
                // TODO(ADR 0035): read window.double_click_titlebar from the
                // user's window.domain instead of the fixed zoom action.
                let mut last = last_click;
                last.set(None);
                controls.toggle_maximize();
            } else {
                let mut last = last_click;
                last.set(Some((now, point)));
                controls.move_window();
            }
        };
        let press_second = move |event: Event<MouseEventData>| {
            if event.button != Some(MouseButton::Left) {
                return;
            }
            let Some(controls) = &click else {
                return;
            };
            let now = Instant::now();
            let point = (event.element_location.x, event.element_location.y);
            let double = last_click.read().is_some_and(|(then, before)| {
                now.duration_since(then)
                    <= Duration::from_millis(u64::from(TITLEBAR_DOUBLE_CLICK.interval_ms))
                    && (before.0 - point.0).abs() <= TITLEBAR_DOUBLE_CLICK.slop
                    && (before.1 - point.1).abs() <= TITLEBAR_DOUBLE_CLICK.slop
            });
            if double {
                let mut last = last_click;
                last.set(None);
                controls.toggle_maximize();
            } else {
                let mut last = last_click;
                last.set(Some((now, point)));
                controls.move_window();
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
