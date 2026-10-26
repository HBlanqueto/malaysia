//! Traffic lights for Malaysia's own client-side windows (ADR 0035).
//!
//! Geometry, colours and pointer state come from [`vow_appearance::traffic`],
//! the same pure module the compositor uses. Glyphs are drawn with Skia via
//! Freya's SVG viewer.

use freya_components::svg_viewer::SvgViewer;
use freya_core::prelude::*;
use torin::prelude::{Direction, Size};
use vow_appearance::traffic::{self, TrafficInput, TrafficPointer};
use vow_appearance::{
    TrafficLightStyle, load_appearance, load_dominant, palette as base_palette, resolve,
};
use vow_design::{ButtonKind, Rgba, TITLEBAR, TRAFFIC_GLYPHS, Triangle};

/// The three-client-side-traffic-lights component.
#[derive(PartialEq)]
pub struct TrafficLights {
    input: Option<TrafficInput>,
    focused: bool,
    zoom_supported: bool,
    fixed_width: bool,
    on_action: Option<Callback<ButtonKind, ()>>,
}

impl Default for TrafficLights {
    fn default() -> Self {
        Self::new()
    }
}

impl TrafficLights {
    /// Creates an interactive set of lights for the current appearance.
    pub fn new() -> Self {
        Self {
            input: None,
            focused: true,
            zoom_supported: true,
            fixed_width: false,
            on_action: None,
        }
    }

    /// Overrides the state; used by previews and tests.
    pub fn input(mut self, input: TrafficInput) -> Self {
        self.input = Some(input);
        self
    }

    /// Whether the window currently has focus.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Whether zoom is admitted; fixed-size windows pass `false`.
    pub fn zoom_supported(mut self, supported: bool) -> Self {
        self.zoom_supported = supported;
        self
    }

    /// Whether the window has a fixed width; the zoom light becomes a vertical
    /// maximise indicator (a plus glyph) instead of the arrows.
    pub fn fixed_width(mut self, fixed: bool) -> Self {
        self.fixed_width = fixed;
        self
    }

    /// Receives the button when a press and release land on it.
    pub fn on_action(mut self, callback: impl Into<Callback<ButtonKind, ()>>) -> Self {
        self.on_action = Some(callback.into());
        self
    }
}

impl Component for TrafficLights {
    fn render(&self) -> impl IntoElement {
        let resolved = resolve(&load_appearance(), load_dominant());
        let inactive = base_palette(resolved.scheme).traffic_lights_inactive;
        let mut pointer = use_state(TrafficPointer::default);
        let input = self.input.unwrap_or_else(|| {
            let pointer = pointer.read();
            TrafficInput {
                focused: self.focused,
                group_hovered: pointer.hovered(),
                pressed: pointer.pressed_visual(),
                zoom_supported: self.zoom_supported,
            }
        });
        let visual_input = if self.fixed_width {
            TrafficInput {
                zoom_supported: true,
                ..input
            }
        } else {
            input
        };
        let visuals = traffic::visual(
            resolved.lights,
            resolved.traffic_style,
            inactive,
            visual_input,
        );
        let interactive = self.input.is_none();
        let action = self.on_action.clone();
        let zoom_supported = self.zoom_supported;
        let fixed_width = self.fixed_width;
        let (_, _, diameter) = traffic::button_rect(0);
        let gap = light_gap();

        let lights = visuals.iter().enumerate().map(|(index, visual)| {
            let kind = TITLEBAR.buttons.order[index].kind;
            let mut hover = pointer;
            let mut press = pointer;
            let mut release = pointer;
            let mut leave = pointer;
            let action = action.clone();
            let clickable = zoom_supported || fixed_width || kind != ButtonKind::Maximize;
            rect()
                .width(Size::px(diameter as f32))
                .height(Size::px(diameter as f32))
                .corner_radius(TITLEBAR.buttons.corner_radius as f32)
                .background(rgba(visual.fill))
                .center()
                .maybe_child(visual.glyph.map(|glyph| {
                    SvgViewer::new((
                        format!("traffic-{index}-{:?}-{fixed_width}", glyph.to_rgba8()),
                        Bytes::from(glyph_svg(index, glyph, fixed_width)),
                    ))
                    .width(Size::px(TRAFFIC_GLYPHS.box_size))
                    .height(Size::px(TRAFFIC_GLYPHS.box_size))
                    .color(rgba(glyph))
                    .show_loader(false)
                    .into_element()
                }))
                .maybe(interactive, |light| {
                    light
                        .on_pointer_enter(move |_| {
                            let mut p = *hover.read();
                            p.on_motion(Some(kind), true);
                            hover.set(p);
                        })
                        .on_pointer_leave(move |_| {
                            let mut p = *leave.read();
                            p.on_motion(None, true);
                            leave.set(p);
                        })
                        .on_mouse_down(move |event: Event<MouseEventData>| {
                            if event.button == Some(MouseButton::Left) && clickable {
                                let mut p = *press.read();
                                p.on_press(Some(kind), true);
                                press.set(p);
                            }
                        })
                        .on_mouse_up(move |event: Event<MouseEventData>| {
                            if event.button != Some(MouseButton::Left) {
                                return;
                            }
                            let mut p = *release.read();
                            let fired = p.on_release(Some(kind), true);
                            release.set(p);
                            if let (Some(kind), Some(action)) = (fired, &action) {
                                action.call(kind);
                            }
                        })
                })
                .into_element()
        });

        rect()
            .width(Size::px((traffic::group_rect().2) as f32))
            .height(Size::px(diameter as f32))
            .direction(Direction::Horizontal)
            .spacing(gap as f32)
            .maybe(interactive, |group| {
                group.on_pointer_leave(move |_| {
                    let mut p = *pointer.read();
                    p.on_leave();
                    pointer.set(p);
                })
            })
            .children(lights)
    }
}

/// The pure visual used by previews and tests.
pub fn traffic_lights_view(
    input: TrafficInput,
    lights: [Rgba; 3],
    style: TrafficLightStyle,
    inactive: Rgba,
) -> Element {
    traffic_lights_view_with_fixed(input, lights, style, inactive, false)
}

/// The pure visual used by previews and tests, optionally with a fixed-width
/// plus glyph on the green light.
pub fn traffic_lights_view_with_fixed(
    input: TrafficInput,
    lights: [Rgba; 3],
    style: TrafficLightStyle,
    inactive: Rgba,
    fixed_width: bool,
) -> Element {
    let visuals = traffic::visual(lights, style, inactive, input);
    let (_, _, diameter) = traffic::button_rect(0);
    let gap = light_gap();
    rect()
        .width(Size::px(traffic::group_rect().2 as f32))
        .height(Size::px(diameter as f32))
        .direction(Direction::Horizontal)
        .spacing(gap as f32)
        .children(visuals.iter().enumerate().map(|(index, visual)| {
            rect()
                .width(Size::px(diameter as f32))
                .height(Size::px(diameter as f32))
                .corner_radius(TITLEBAR.buttons.corner_radius as f32)
                .background(rgba(visual.fill))
                .center()
                .maybe_child(visual.glyph.map(|glyph| {
                    SvgViewer::new((
                        format!("traffic-view-{index}-{:?}-{fixed_width}", glyph.to_rgba8()),
                        Bytes::from(glyph_svg(index, glyph, fixed_width)),
                    ))
                    .width(Size::px(TRAFFIC_GLYPHS.box_size))
                    .height(Size::px(TRAFFIC_GLYPHS.box_size))
                    .color(rgba(glyph))
                    .show_loader(false)
                    .into_element()
                }))
                .into_element()
        }))
        .into_element()
}

/// The gap between two lights, from the Neptunia token
/// (`window-traffic-lights-gap`), shared with the compositor's chrome.
fn light_gap() -> f64 {
    f64::from(TITLEBAR.buttons.spacing)
}

fn glyph_svg(index: usize, color: Rgba, fixed_width: bool) -> String {
    let [r, g, b, a] = color.to_rgba8();
    let color = format!("#{r:02x}{g:02x}{b:02x}");
    let opacity = f32::from(a) / 255.0;
    let box_size = TRAFFIC_GLYPHS.box_size;
    match index {
        0 | 1 => {
            let segments = if index == 0 {
                TRAFFIC_GLYPHS.close
            } else {
                TRAFFIC_GLYPHS.minimize
            };
            let path = segments
                .iter()
                .map(|segment| {
                    format!(
                        "M{} {} L{} {}",
                        segment.a.0, segment.a.1, segment.b.0, segment.b.1
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{box_size}\" height=\"{box_size}\" viewBox=\"0 0 {box_size} {box_size}\"><path d=\"{path}\" fill=\"none\" stroke=\"{color}\" stroke-opacity=\"{opacity}\" stroke-width=\"{}\" stroke-linecap=\"round\"/></svg>",
                TRAFFIC_GLYPHS.stroke
            )
        }
        _ => {
            let segments = if fixed_width {
                TRAFFIC_GLYPHS.plus
            } else {
                &[]
            };
            if !segments.is_empty() {
                let path = segments
                    .iter()
                    .map(|segment| {
                        format!(
                            "M{} {} L{} {}",
                            segment.a.0, segment.a.1, segment.b.0, segment.b.1
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{box_size}\" height=\"{box_size}\" viewBox=\"0 0 {box_size} {box_size}\"><path d=\"{path}\" fill=\"none\" stroke=\"{color}\" stroke-opacity=\"{opacity}\" stroke-width=\"{}\" stroke-linecap=\"round\"/></svg>",
                    TRAFFIC_GLYPHS.stroke
                )
            } else {
                let polygons = TRAFFIC_GLYPHS
                    .zoom
                    .iter()
                    .map(polygon)
                    .collect::<Vec<_>>()
                    .join("");
                format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{box_size}\" height=\"{box_size}\" viewBox=\"0 0 {box_size} {box_size}\"><g fill=\"{color}\" fill-opacity=\"{opacity}\">{polygons}</g></svg>"
                )
            }
        }
    }
}

fn polygon(triangle: &Triangle) -> String {
    let points = triangle
        .0
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("<polygon points=\"{points}\"/>")
}

fn rgba(color: Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_traffic_lights_sit_at_their_token_positions() {
        let xs = [0, 1, 2].map(|index| traffic::button_rect(index).0);
        assert_eq!(xs, [16.0, 36.0, 56.0], "12 px lights, 8 px apart");
        assert_eq!(light_gap(), 8.0);
        assert_eq!(traffic::button_rect(0).2, 12.0);
    }
}
