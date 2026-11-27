use std::time::Duration;

use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::spring::{Motion, advance_motion};

const OMEGA: f64 = ctdesign::SPACES_STRIP_EXPAND_OMEGA as f64;
const DAMPING: f64 = ctdesign::SPACES_STRIP_EXPAND_DAMPING as f64;
const HEIGHT: f64 = ctdesign::SPACES_STRIP_HEIGHT as f64;
const EXPANDED_HEIGHT: f64 = ctdesign::SPACES_STRIP_EXPANDED_HEIGHT as f64;
const THUMBNAIL_HEIGHT: f64 = ctdesign::SPACES_THUMBNAIL_HEIGHT as f64;
const THUMBNAIL_TOP: f64 = ctdesign::SPACES_THUMBNAIL_TOP as f64;
const THUMBNAIL_GAP: f64 = ctdesign::SPACES_THUMBNAIL_GAP as f64;
const LABEL_CELL: f64 = ctdesign::SPACES_LABEL_CELL as f64;
const PILL_HEIGHT: f64 = ctdesign::SPACES_LABEL_PILL_HEIGHT as f64;
const PILL_TOP: f64 = ctdesign::SPACES_LABEL_PILL_TOP as f64;
const ADD_SIZE: f64 = ctdesign::SPACES_ADD_SIZE as f64;
const ADD_INSET: f64 = ctdesign::SPACES_ADD_INSET as f64;
const ADD_EXPANDED_CENTRE: f64 = ctdesign::SPACES_ADD_EXPANDED_CENTER as f64;
const CLOSE_RADIUS: f64 = ctdesign::SPACES_CLOSE_SIZE as f64 / 2.0;

#[derive(Default)]
pub struct Strip {
    pub grown: bool,
    pub growth: f64,
    pub motion: Option<Motion>,
    pub hovered: Option<usize>,
}

#[derive(Debug, PartialEq)]
pub enum Hit {
    Close(usize),
    Thumbnail(usize),
    Add,
    Bar,
}

impl Strip {
    pub fn grow(&mut self, now: Duration) {
        if self.grown {
            return;
        }
        self.grown = true;
        self.motion = Some(Motion {
            from: self.growth,
            velocity: 0.0,
            started: now,
        });
    }

    pub fn advance(&mut self, now: Duration, longer_side: f64) -> bool {
        let Some(growth) = advance_motion(&mut self.motion, OMEGA, DAMPING, 1.0, now, longer_side)
        else {
            return false;
        };
        self.growth = growth;
        self.motion.is_some()
    }

    pub fn height(&self) -> f64 {
        HEIGHT + (EXPANDED_HEIGHT - HEIGHT) * self.growth
    }

    pub fn target_height(&self) -> i32 {
        if self.grown {
            ctdesign::SPACES_STRIP_EXPANDED_HEIGHT
        } else {
            ctdesign::SPACES_STRIP_HEIGHT
        }
    }

    pub fn hit(
        &self,
        point: Point<f64, Logical>,
        count: usize,
        output: Size<f64, Logical>,
    ) -> Option<Hit> {
        let height = self.height();
        if point.y >= height {
            return None;
        }
        if self.grown {
            if let Some(index) = self.hovered {
                let offset = point - close_centre(thumbnail(index, count, output));
                if offset.x * offset.x + offset.y * offset.y <= CLOSE_RADIUS * CLOSE_RADIUS {
                    return Some(Hit::Close(index));
                }
            }
            if let Some(index) =
                (0..count).find(|&index| thumbnail(index, count, output).contains(point))
            {
                return Some(Hit::Thumbnail(index));
            }
        }
        let corner =
            add_centre(self.growth, output) - Point::from((ADD_SIZE / 2.0, ADD_SIZE / 2.0));
        if Rectangle::new(corner, Size::from((ADD_SIZE, ADD_SIZE))).contains(point) {
            return Some(Hit::Add);
        }
        Some(Hit::Bar)
    }

    pub fn hover(
        &mut self,
        point: Point<f64, Logical>,
        count: usize,
        output: Size<f64, Logical>,
        now: Duration,
    ) {
        let entered = !self.grown && point.y < self.height();
        if entered {
            self.grow(now);
        }
        let hovered = match self.hit(point, count, output) {
            Some(Hit::Close(index) | Hit::Thumbnail(index)) if count > 1 => Some(index),
            _ => None,
        };
        self.hovered = hovered;
    }
}

pub fn thumbnail_factor(output: Size<f64, Logical>) -> f64 {
    THUMBNAIL_HEIGHT / output.h
}

pub fn thumbnail(
    index: usize,
    count: usize,
    output: Size<f64, Logical>,
) -> Rectangle<f64, Logical> {
    let width = output.w * thumbnail_factor(output);
    let row = count as f64 * width + (count as f64 - 1.0) * THUMBNAIL_GAP;
    let left = (output.w - row) / 2.0 + index as f64 * (width + THUMBNAIL_GAP);
    Rectangle::new(
        Point::from((left, THUMBNAIL_TOP)),
        Size::from((width, THUMBNAIL_HEIGHT)),
    )
}

pub fn pill(current: usize, count: usize, output: Size<f64, Logical>) -> Rectangle<f64, Logical> {
    let left = (output.w - count as f64 * LABEL_CELL) / 2.0 + current as f64 * LABEL_CELL;
    Rectangle::new(
        Point::from((left, PILL_TOP)),
        Size::from((LABEL_CELL, PILL_HEIGHT)),
    )
}

pub fn add_centre(growth: f64, output: Size<f64, Logical>) -> Point<f64, Logical> {
    let collapsed = HEIGHT / 2.0;
    Point::from((
        output.w - ADD_INSET,
        collapsed + (ADD_EXPANDED_CENTRE - collapsed) * growth,
    ))
}

pub fn close_centre(thumbnail: Rectangle<f64, Logical>) -> Point<f64, Logical> {
    thumbnail.loc
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONGER_SIDE: f64 = 2560.0;

    fn display() -> Size<f64, Logical> {
        Size::from((2560.0, 1440.0))
    }

    fn grown() -> Strip {
        Strip {
            grown: true,
            growth: 1.0,
            ..Strip::default()
        }
    }

    #[test]
    fn on_2560_by_1440_thumbnails_are_160_by_90_at_29_and_32_apart_in_a_centred_row() {
        assert_eq!(thumbnail_factor(display()), 1.0 / 16.0);
        for lefts in [&[1200.0][..], &[1104.0, 1296.0], &[1008.0, 1200.0, 1392.0]] {
            for (index, &left) in lefts.iter().enumerate() {
                assert_eq!(
                    thumbnail(index, lefts.len(), display()),
                    Rectangle::new(Point::from((left, 29.0)), Size::from((160.0, 90.0)))
                );
            }
        }
    }

    #[test]
    fn the_pill_is_the_current_cell_of_a_centred_row_of_cells_95_wide() {
        let cell = |left: f64| Rectangle::new(Point::from((left, 7.0)), Size::from((95.0, 24.0)));
        assert_eq!(pill(0, 1, display()), cell(1232.5));
        assert_eq!(pill(0, 2, display()), cell(1185.0));
        assert_eq!(pill(1, 2, display()), cell(1280.0));
        assert_eq!(pill(1, 3, display()), cell(1232.5));
    }

    #[test]
    fn the_close_circle_of_the_hovered_thumbnail_wins_over_the_thumbnail() {
        let mut strip = grown();
        let inside_corner = Point::from((1205.0, 34.0));
        let left_of_corner = Point::from((1190.0, 29.0));
        assert_eq!(
            strip.hit(inside_corner, 3, display()),
            Some(Hit::Thumbnail(1))
        );
        assert_eq!(strip.hit(left_of_corner, 3, display()), Some(Hit::Bar));
        strip.hovered = Some(1);
        assert_eq!(strip.hit(inside_corner, 3, display()), Some(Hit::Close(1)));
        assert_eq!(strip.hit(left_of_corner, 3, display()), Some(Hit::Close(1)));
        assert_eq!(
            strip.hit(Point::from((1260.0, 100.0)), 3, display()),
            Some(Hit::Thumbnail(1))
        );
    }

    #[test]
    fn a_collapsed_strip_has_no_thumbnail_to_hit() {
        let strip = Strip::default();
        assert_eq!(
            strip.hit(Point::from((1280.0, 35.0)), 1, display()),
            Some(Hit::Bar)
        );
        assert_eq!(
            strip.hit(Point::from((2533.5, 20.0)), 1, display()),
            Some(Hit::Add)
        );
        assert_eq!(strip.hit(Point::from((1280.0, 40.0)), 1, display()), None);
    }

    #[test]
    fn the_pointer_entering_the_strip_grows_it_and_it_stays_grown() {
        let mut strip = Strip::default();
        strip.hover(Point::from((1280.0, 60.0)), 1, display(), Duration::ZERO);
        assert!(!strip.grown);
        strip.hover(Point::from((1280.0, 39.0)), 1, display(), Duration::ZERO);
        assert!(strip.grown);
        assert_eq!(strip.hovered, None);
        while strip.advance(Duration::from_secs(1), LONGER_SIDE) {}
        assert_eq!(strip.height(), 146.0);
        strip.hover(
            Point::from((1280.0, 600.0)),
            1,
            display(),
            Duration::from_secs(2),
        );
        assert!(strip.grown && strip.hovered.is_none());
        assert!(!strip.advance(Duration::from_secs(3), LONGER_SIDE));
        assert_eq!(strip.target_height(), 146);
        assert_eq!(
            strip.hit(Point::from((1280.0, 140.0)), 1, display()),
            Some(Hit::Bar)
        );
        assert_eq!(
            strip.hit(Point::from((2533.5, 77.5)), 1, display()),
            Some(Hit::Add)
        );
    }

    #[test]
    fn the_growth_is_half_way_near_105_ms() {
        let first_millisecond_at_or_above = |reached: f64| {
            let mut strip = Strip::default();
            strip.grow(Duration::ZERO);
            (0..)
                .find(|&millisecond| {
                    strip.advance(Duration::from_millis(millisecond), LONGER_SIDE);
                    strip.growth >= reached
                })
                .unwrap()
        };
        assert!((102..=108).contains(&first_millisecond_at_or_above(0.5)));
        assert!((240..=246).contains(&first_millisecond_at_or_above(0.9)));
    }

    #[test]
    fn on_a_display_too_narrow_for_its_row_the_thumbnails_still_do_not_overlap() {
        let narrow = Size::from((800.0, 600.0));
        let first = thumbnail(0, 16, narrow);
        let last = thumbnail(15, 16, narrow);
        assert!(first.loc.x < 0.0 && last.loc.x + last.size.w > narrow.w);
        for index in 1..16 {
            let left = thumbnail(index - 1, 16, narrow);
            let right = thumbnail(index, 16, narrow);
            assert_eq!(right.loc.x - (left.loc.x + left.size.w), 32.0);
        }
    }
}
