//! Shared traffic-light geometry, appearance and pointer state.
//!
//! Pure geometry and colour logic used by the compositor's server-side chrome
//! and, later, by BarrierKit. Coordinates are relative to the top-left of
//! the titlebar: `x` grows right, `y` grows down, in logical pixels.

use vow_design::{
    ButtonKind, Rgba, TITLEBAR, TRAFFIC_GLYPH_ON_DARK, TRAFFIC_GLYPH_ON_LIGHT,
    TRAFFIC_GLYPH_STANDARD, TRAFFIC_PRESSED_DARKEN, TitlebarPosition,
};

use crate::TrafficLightStyle;

/// The top-left corner and side of one traffic light, relative to the bar.
pub fn button_rect(index: usize) -> (f64, f64, f64) {
    let diameter = f64::from(TITLEBAR.buttons.diameter);
    let gap = f64::from(TITLEBAR.buttons.spacing);
    let x = f64::from(TITLEBAR.buttons.margin_start) + index as f64 * (diameter + gap);
    let y = (f64::from(TITLEBAR.thickness) - diameter) / 2.0;
    (x, y, diameter)
}

/// The bounding box of the three traffic lights, relative to the bar.
pub fn group_rect() -> (f64, f64, f64, f64) {
    let (x, y, size) = button_rect(0);
    let count = TITLEBAR.buttons.order.len() as f64;
    let gap = f64::from(TITLEBAR.buttons.spacing);
    (x, y, count * size + (count - 1.0) * gap, size)
}

/// The button under a bar-relative point, if any.
pub fn hit(point: (f64, f64)) -> Option<ButtonKind> {
    if TITLEBAR.position != TitlebarPosition::Top {
        return None;
    }
    let (px, py) = point;
    for (index, button) in TITLEBAR.buttons.order.iter().enumerate() {
        let (x, y, size) = button_rect(index);
        if contains_rounded_square(px, py, x, y, size) {
            return Some(button.kind);
        }
    }
    None
}

fn contains_rounded_square(px: f64, py: f64, x: f64, y: f64, size: f64) -> bool {
    let half = size / 2.0;
    let radius = f64::from(TITLEBAR.buttons.corner_radius).min(half);
    let center_x = x + half;
    let center_y = y + half;
    let dx = (px - center_x).abs() - (half - radius);
    let dy = (py - center_y).abs() - (half - radius);
    (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - radius <= 0.0
}

/// Everything that affects how the lights look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrafficInput {
    /// The window has keyboard focus.
    pub focused: bool,
    /// The pointer is over the group.
    pub group_hovered: bool,
    /// The armed button whose pressed state should be visible.
    pub pressed: Option<ButtonKind>,
    /// The window supports zoom.
    pub zoom_supported: bool,
}

/// One light's fill and optional glyph colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonVisual {
    /// The light's fill.
    pub fill: Rgba,
    /// The glyph, only while the group is hovered and the light is admitted.
    pub glyph: Option<Rgba>,
}

/// Computes the three lights' appearance from one input snapshot.
pub fn visual(
    lights: [Rgba; 3],
    style: TrafficLightStyle,
    inactive: Rgba,
    input: TrafficInput,
) -> [ButtonVisual; 3] {
    let mut out = [ButtonVisual {
        fill: inactive,
        glyph: None,
    }; 3];
    for (index, slot) in out.iter_mut().enumerate() {
        let kind = TITLEBAR.buttons.order[index].kind;
        let admitted = kind != ButtonKind::Maximize || input.zoom_supported;
        if !admitted {
            *slot = ButtonVisual {
                fill: inactive,
                glyph: None,
            };
            continue;
        }
        let base = if input.focused || input.group_hovered {
            lights[index]
        } else {
            inactive
        };
        let fill = if input.pressed == Some(kind) {
            base.darken(TRAFFIC_PRESSED_DARKEN)
        } else {
            base
        };
        let glyph = input.group_hovered.then(|| match style {
            TrafficLightStyle::Accent | TrafficLightStyle::Wallpaper => {
                if kind == ButtonKind::Maximize {
                    TRAFFIC_GLYPH_ON_LIGHT
                } else {
                    TRAFFIC_GLYPH_ON_DARK
                }
            }
            TrafficLightStyle::Auto | TrafficLightStyle::Standard => TRAFFIC_GLYPH_STANDARD,
        });
        *slot = ButtonVisual { fill, glyph };
    }
    out
}

/// Hover, press and armed state for the traffic-light group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrafficPointer {
    /// Whether the pointer is over the group (including the gaps).
    hovered: bool,
    /// The button pressed on, if any.
    armed: Option<ButtonKind>,
    /// Whether the pointer is still over the armed button.
    over_armed: bool,
}

impl TrafficPointer {
    /// Updates hover and returns whether the drawn state changed.
    pub fn on_motion(&mut self, hit: Option<ButtonKind>, over_group: bool) -> bool {
        let before = (self.hovered, self.over_armed);
        self.hovered = over_group;
        self.over_armed = self.armed.is_some() && self.armed == hit;
        before != (self.hovered, self.over_armed)
    }

    /// Arms a button; returns whether the drawn state changed.
    pub fn on_press(&mut self, hit: Option<ButtonKind>, over_group: bool) -> bool {
        let before = (self.hovered, self.armed, self.over_armed);
        self.hovered = over_group;
        self.armed = hit;
        self.over_armed = hit.is_some();
        before != (self.hovered, self.armed, self.over_armed)
    }

    /// Releases the arm and returns the button to activate, if the pointer is
    /// still over the same button.
    pub fn on_release(&mut self, hit: Option<ButtonKind>, over_group: bool) -> Option<ButtonKind> {
        let result = (self.armed == hit).then_some(self.armed).flatten();
        self.hovered = over_group;
        self.armed = None;
        self.over_armed = false;
        result
    }

    /// Clears hover and arm when the pointer leaves the window or the chrome.
    /// Returns whether the drawn state changed.
    pub fn on_leave(&mut self) -> bool {
        let changed = self.hovered || self.armed.is_some();
        self.hovered = false;
        self.armed = None;
        self.over_armed = false;
        changed
    }

    /// Whether the group is hovered.
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// The button whose pressed state is visible, if any.
    pub fn pressed_visual(&self) -> Option<ButtonKind> {
        if self.over_armed { self.armed } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lights() -> [Rgba; 3] {
        [
            Rgba::rgb8(255, 95, 87),
            Rgba::rgb8(254, 188, 46),
            Rgba::rgb8(40, 200, 64),
        ]
    }

    fn inactive() -> Rgba {
        Rgba::rgb8(0xd3, 0xd3, 0xd5)
    }

    #[test]
    fn hover_uses_the_lit_colour_even_when_unfocused() {
        let visual = visual(
            lights(),
            TrafficLightStyle::Standard,
            inactive(),
            TrafficInput {
                focused: false,
                group_hovered: true,
                zoom_supported: true,
                ..TrafficInput::default()
            },
        );
        assert_eq!(visual[0].fill, lights()[0]);
        assert_eq!(visual[1].fill, lights()[1]);
        assert_eq!(visual[0].glyph, Some(TRAFFIC_GLYPH_STANDARD));
    }

    #[test]
    fn without_focus_or_hover_the_lights_are_inactive() {
        let visual = visual(
            lights(),
            TrafficLightStyle::Standard,
            inactive(),
            TrafficInput {
                focused: false,
                group_hovered: false,
                zoom_supported: true,
                ..TrafficInput::default()
            },
        );
        assert!(visual.iter().all(|button| button.fill == inactive()));
        assert!(visual.iter().all(|button| button.glyph.is_none()));
    }

    #[test]
    fn pressed_darkens_only_the_pressed_button() {
        let visual = visual(
            lights(),
            TrafficLightStyle::Standard,
            inactive(),
            TrafficInput {
                focused: true,
                group_hovered: false,
                pressed: Some(ButtonKind::Close),
                zoom_supported: true,
            },
        );
        assert_eq!(visual[0].fill, lights()[0].darken(TRAFFIC_PRESSED_DARKEN));
        assert_eq!(visual[1].fill, lights()[1]);
    }

    #[test]
    fn glyphs_follow_the_style() {
        let standard = visual(
            lights(),
            TrafficLightStyle::Standard,
            inactive(),
            TrafficInput {
                focused: true,
                group_hovered: true,
                pressed: None,
                zoom_supported: true,
            },
        );
        assert!(
            standard
                .iter()
                .all(|button| button.glyph == Some(TRAFFIC_GLYPH_STANDARD))
        );
        let accent = visual(
            lights(),
            TrafficLightStyle::Accent,
            inactive(),
            TrafficInput {
                focused: true,
                group_hovered: true,
                pressed: None,
                zoom_supported: true,
            },
        );
        assert_eq!(accent[0].glyph, Some(TRAFFIC_GLYPH_ON_DARK));
        assert_eq!(accent[2].glyph, Some(TRAFFIC_GLYPH_ON_LIGHT));
    }

    #[test]
    fn unsupported_zoom_is_always_inactive_and_without_a_glyph() {
        let visual = visual(
            lights(),
            TrafficLightStyle::Accent,
            inactive(),
            TrafficInput {
                focused: true,
                group_hovered: true,
                pressed: Some(ButtonKind::Maximize),
                zoom_supported: false,
            },
        );
        assert_eq!(visual[2].fill, inactive());
        assert_eq!(visual[2].glyph, None);
    }

    #[test]
    fn pressing_one_button_and_releasing_on_another_does_not_fire() {
        let mut pointer = TrafficPointer::default();
        assert!(pointer.on_press(Some(ButtonKind::Close), true));
        assert!(pointer.on_motion(Some(ButtonKind::Minimize), true));
        assert_eq!(pointer.pressed_visual(), None);
        assert_eq!(pointer.on_release(Some(ButtonKind::Minimize), true), None);
    }

    #[test]
    fn pressing_and_releasing_the_same_button_fires() {
        let mut pointer = TrafficPointer::default();
        pointer.on_press(Some(ButtonKind::Close), true);
        assert_eq!(
            pointer.on_release(Some(ButtonKind::Close), true),
            Some(ButtonKind::Close)
        );
    }

    #[test]
    fn leaving_with_an_armed_button_does_not_fire() {
        let mut pointer = TrafficPointer::default();
        pointer.on_press(Some(ButtonKind::Close), true);
        assert!(pointer.on_leave());
        assert_eq!(pointer.on_release(None, false), None);
    }

    #[test]
    fn pressed_visual_is_only_shown_over_the_armed_button() {
        let mut pointer = TrafficPointer::default();
        pointer.on_press(Some(ButtonKind::Close), true);
        assert_eq!(pointer.pressed_visual(), Some(ButtonKind::Close));
        pointer.on_motion(Some(ButtonKind::Minimize), true);
        assert_eq!(pointer.pressed_visual(), None);
    }
}
