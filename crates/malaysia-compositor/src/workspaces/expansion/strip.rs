use std::time::Duration;

use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::spring::{Motion, advance_motion, sample_motion};
use crate::workspaces::desktops::moved_index;

const OMEGA: f64 = ctdesign::SPACES_STRIP_EXPAND_OMEGA as f64;
const DAMPING: f64 = ctdesign::SPACES_STRIP_EXPAND_DAMPING as f64;
const HEIGHT: f64 = ctdesign::SPACES_STRIP_HEIGHT as f64;
pub(super) const EXPANDED_HEIGHT: f64 = ctdesign::SPACES_STRIP_EXPANDED_HEIGHT as f64;
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
const ADD_PREVIEW_REACH: f64 = ctdesign::SPACES_ADD_PREVIEW_REACH as f64;

#[derive(Default)]
pub struct Strip {
    pub grown: bool,
    pub growth: f64,
    pub motion: Option<Motion>,
    pub hovered: Option<usize>,
    pub hovered_since: Duration,
    pub closable: bool,
    pub opening: Option<Opening>,
    pub moving: Option<(usize, f64)>,
}

pub struct Opening {
    stage: Stage,
    value: f64,
    motion: Option<Motion>,
    wanted: bool,
}

#[derive(Clone, Copy)]
enum Stage {
    Preview,
    Docking { at: usize, peek: f64 },
    Removal { at: usize },
}

impl Opening {
    fn new(stage: Stage, from: f64, wanted: bool, now: Duration) -> Opening {
        Opening {
            stage,
            value: from,
            motion: Some(Motion::new(from, 0.0, now)),
            wanted,
        }
    }

    fn aim(&mut self, wanted: bool, now: Duration) {
        if self.wanted == wanted {
            return;
        }
        let (from, velocity) = sample_motion(
            &self.motion,
            OMEGA,
            DAMPING,
            self.value,
            f64::from(self.wanted),
            now,
        );
        self.wanted = wanted;
        self.motion = Some(Motion::new(from, velocity, now));
    }
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
        self.motion = Some(Motion::new(self.growth, 0.0, now));
    }

    pub fn advance(&mut self, now: Duration, longer_side: f64) -> bool {
        if let Some(growth) =
            advance_motion(&mut self.motion, OMEGA, DAMPING, 1.0, now, longer_side)
        {
            self.growth = growth;
        }
        if let Some(opening) = &mut self.opening
            && let Some(value) = advance_motion(
                &mut opening.motion,
                OMEGA,
                DAMPING,
                f64::from(opening.wanted),
                now,
                longer_side,
            )
        {
            opening.value = value;
            let previewed = matches!(opening.stage, Stage::Preview) && opening.wanted;
            if opening.motion.is_none() && !previewed {
                self.opening = None;
            }
        }
        !self.at_rest()
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
            if let Some(index) = self.close() {
                let offset = point - close_centre(self.thumbnail(index, count, output));
                if offset.x * offset.x + offset.y * offset.y <= CLOSE_RADIUS * CLOSE_RADIUS {
                    return Some(Hit::Close(index));
                }
            }
            if let Some(index) =
                (0..count).find(|&index| self.thumbnail(index, count, output).contains(point))
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
        addable: bool,
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
        let peeking = self.grown
            && self.moving.is_none()
            && point.y < self.height()
            && point.x >= output.w - ADD_PREVIEW_REACH;
        match &mut self.opening {
            Some(opening) if !matches!(opening.stage, Stage::Preview) => {}
            _ if !addable => self.opening = None,
            Some(opening) => opening.aim(peeking, now),
            None if peeking => self.opening = Some(Opening::new(Stage::Preview, 0.0, true, now)),
            None => {}
        }
    }

    pub fn close(&self) -> Option<usize> {
        self.hovered.filter(|_| self.closable)
    }

    pub fn dock(&mut self, at: usize, now: Duration) {
        let peek = match &self.opening {
            Some(Opening {
                stage: Stage::Preview,
                value,
                ..
            }) => *value,
            _ => 1.0,
        };
        self.opening = self
            .grown
            .then(|| Opening::new(Stage::Docking { at, peek }, 0.0, true, now));
    }

    pub fn remove(&mut self, at: usize, now: Duration) {
        self.opening = self
            .grown
            .then(|| Opening::new(Stage::Removal { at }, 1.0, false, now));
    }

    pub fn thumbnail(
        &self,
        index: usize,
        count: usize,
        output: Size<f64, Logical>,
    ) -> Rectangle<f64, Logical> {
        let opening = self
            .opening
            .as_ref()
            .map(|opening| (opening.stage, opening.value));
        let (at, gap, peek) = match opening {
            Some((Stage::Docking { at, peek }, gap)) => (at, gap, Some(peek)),
            Some((Stage::Removal { at }, gap)) => (at, gap, None),
            _ => (count, 0.0, None),
        };
        let docking = peek.is_some();
        let others = count - usize::from(docking);
        let (first, slot, _) = row(others, gap, output);
        let mut thumbnail = entering(peek.unwrap_or_default(), output);
        thumbnail.loc.x = match peek {
            Some(_) if index == at => {
                let docked = row(others, 1.0, output).0 + at as f64 * slot;
                thumbnail.loc.x + (docked - thumbnail.loc.x) * gap
            }
            _ => {
                let position = index - usize::from(docking && index > at);
                let shift = if index >= at { gap * slot } else { 0.0 };
                first + position as f64 * slot + shift
            }
        };
        thumbnail
    }

    pub fn preview(&self, output: Size<f64, Logical>) -> Option<Rectangle<f64, Logical>> {
        self.opening
            .as_ref()
            .filter(|opening| matches!(opening.stage, Stage::Preview))
            .map(|opening| entering(opening.value, output))
    }

    pub fn moved(
        &self,
        x: f64,
        count: usize,
        output: Size<f64, Logical>,
    ) -> Option<(usize, usize, Rectangle<f64, Logical>)> {
        let (from, grip) = self.moving?;
        let mut thumbnail = self.thumbnail(from, count, output);
        thumbnail.loc.x = x - grip;
        let to = slot_at(thumbnail.loc.x + thumbnail.size.w / 2.0, count, output);
        Some((from, to, thumbnail))
    }

    pub fn placed(
        &self,
        index: usize,
        count: usize,
        output: Size<f64, Logical>,
        x: f64,
    ) -> Rectangle<f64, Logical> {
        match self.moved(x, count, output) {
            Some((from, _, thumbnail)) if index == from => thumbnail,
            Some((from, to, _)) => self.thumbnail(moved_index(index, from, to), count, output),
            None => self.thumbnail(index, count, output),
        }
    }

    pub fn at_rest(&self) -> bool {
        self.motion.is_none()
            && self
                .opening
                .as_ref()
                .is_none_or(|opening| opening.motion.is_none())
    }
}

fn row(others: usize, presence: f64, output: Size<f64, Logical>) -> (f64, f64, f64) {
    let width = output.w * thumbnail_factor(output);
    let slot = width + THUMBNAIL_GAP;
    let row = others as f64 * slot - THUMBNAIL_GAP + presence * slot;
    ((output.w - row) / 2.0, slot, width)
}

fn entering(peek: f64, output: Size<f64, Logical>) -> Rectangle<f64, Logical> {
    let width = output.w * thumbnail_factor(output);
    Rectangle::new(
        Point::from((output.w - peek * width / 2.0, THUMBNAIL_TOP)),
        Size::from((width, THUMBNAIL_HEIGHT)),
    )
}

fn slot_at(x: f64, count: usize, output: Size<f64, Logical>) -> usize {
    let (first, slot, width) = row(count, 0.0, output);
    ((x - (first + width / 2.0)) / slot)
        .round()
        .clamp(0.0, (count - 1) as f64) as usize
}

fn thumbnail_factor(output: Size<f64, Logical>) -> f64 {
    THUMBNAIL_HEIGHT / output.h
}

pub fn split_shares(
    thumbnail: Rectangle<f64, Logical>,
    left: f64,
) -> (Rectangle<f64, Logical>, Rectangle<f64, Logical>) {
    let left_width = thumbnail.size.w * left;
    (
        Rectangle::new(thumbnail.loc, Size::from((left_width, thumbnail.size.h))),
        Rectangle::new(
            thumbnail.loc + Point::from((left_width, 0.0)),
            Size::from((thumbnail.size.w - left_width, thumbnail.size.h)),
        ),
    )
}

pub fn miniature(
    thumbnail: Rectangle<f64, Logical>,
    frame: Point<f64, Logical>,
    output: Size<f64, Logical>,
) -> (Point<f64, Logical>, f64) {
    let factor = thumbnail.size.h / output.h;
    (thumbnail.loc + frame.upscale(factor), factor)
}

pub fn fitted(
    share: Rectangle<f64, Logical>,
    size: Size<f64, Logical>,
) -> (Point<f64, Logical>, f64) {
    let scale = share.size.w / size.w.max(f64::EPSILON);
    let top = share.loc.y + (share.size.h - size.h * scale) / 2.0;
    (Point::from((share.loc.x, top)), scale)
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
    use crate::workspaces::desktops::after_insertion;

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
                    Strip::default().thumbnail(index, lefts.len(), display()),
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
    fn the_close_circle_of_the_hovered_thumbnail_wins_over_the_thumbnail_once_closable() {
        let mut strip = grown();
        let inside_corner = Point::from((1205.0, 34.0));
        let left_of_corner = Point::from((1190.0, 29.0));
        assert_eq!(
            strip.hit(inside_corner, 3, display()),
            Some(Hit::Thumbnail(1))
        );
        assert_eq!(strip.hit(left_of_corner, 3, display()), Some(Hit::Bar));
        strip.hovered = Some(1);
        assert_eq!(strip.hit(left_of_corner, 3, display()), Some(Hit::Bar));
        strip.closable = true;
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
        strip.hover(
            Point::from((1280.0, 60.0)),
            1,
            true,
            display(),
            Duration::ZERO,
        );
        assert!(!strip.grown);
        strip.hover(
            Point::from((1280.0, 39.0)),
            1,
            true,
            display(),
            Duration::ZERO,
        );
        assert!(strip.grown);
        assert_eq!(strip.hovered, None);
        while strip.advance(Duration::from_secs(1), LONGER_SIDE) {}
        assert_eq!(strip.height(), 146.0);
        strip.hover(
            Point::from((1280.0, 600.0)),
            1,
            true,
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

    fn staged(stage: Stage, value: f64) -> Strip {
        Strip {
            opening: Some(Opening {
                stage,
                value,
                motion: None,
                wanted: true,
            }),
            ..grown()
        }
    }

    fn edge_rect(peek: f64) -> Rectangle<f64, Logical> {
        Rectangle::new(
            Point::from((2560.0 - peek * 80.0, 29.0)),
            Size::from((160.0, 90.0)),
        )
    }

    #[test]
    fn the_preview_slides_in_until_its_centre_is_on_the_right_edge_and_moves_no_thumbnail() {
        let near_edge = Point::from((2500.0, 60.0));
        let mut strip = grown();
        strip.hover(near_edge, 16, false, display(), Duration::ZERO);
        assert!(strip.opening.is_none() && strip.preview(display()).is_none());
        strip.hover(near_edge, 3, true, display(), Duration::ZERO);
        assert_eq!(strip.preview(display()), Some(edge_rect(0.0)));
        while strip.advance(Duration::from_secs(1), LONGER_SIDE) {}
        assert_eq!(strip.preview(display()), Some(edge_rect(1.0)));
        for index in 0..3 {
            assert_eq!(
                strip.thumbnail(index, 3, display()),
                grown().thumbnail(index, 3, display())
            );
        }
        strip.hover(
            Point::from((1280.0, 60.0)),
            3,
            true,
            display(),
            Duration::from_secs(2),
        );
        while strip.advance(Duration::from_secs(3), LONGER_SIDE) {}
        assert!(strip.opening.is_none());
    }

    #[test]
    fn docking_starts_with_the_others_in_place_and_the_new_one_where_the_preview_was() {
        for (at, count) in [(1, 1), (2, 3), (1, 3)] {
            for (preview, peek) in [(None, 1.0), (Some(0.4), 0.4)] {
                let mut strip = preview.map_or_else(grown, |value| staged(Stage::Preview, value));
                strip.dock(at, Duration::ZERO);
                assert_eq!(strip.thumbnail(at, count + 1, display()), edge_rect(peek));
                for index in 0..count {
                    assert_eq!(
                        strip.thumbnail(after_insertion(at, index), count + 1, display()),
                        grown().thumbnail(index, count, display())
                    );
                }
            }
        }
    }

    #[test]
    fn docking_ends_at_the_row_of_n_plus_1() {
        for (at, count) in [(1, 1), (2, 3), (1, 3)] {
            let strip = staged(Stage::Docking { at, peek: 0.4 }, 1.0);
            for index in 0..=count {
                assert_eq!(
                    strip.thumbnail(index, count + 1, display()),
                    grown().thumbnail(index, count + 1, display())
                );
            }
        }
    }

    #[test]
    fn a_full_screen_space_after_the_new_desktop_moves_right_while_it_docks() {
        let strip = staged(Stage::Docking { at: 2, peek: 1.0 }, 0.5);
        let plain = grown();
        assert!(strip.thumbnail(3, 4, display()).loc.x > plain.thumbnail(2, 3, display()).loc.x);
        assert!(strip.thumbnail(1, 4, display()).loc.x < plain.thumbnail(1, 3, display()).loc.x);
    }

    #[test]
    fn removal_starts_at_the_old_row_and_ends_at_the_row_of_n() {
        for (at, count) in [(0, 1), (1, 1), (1, 3), (3, 3)] {
            let started = staged(Stage::Removal { at }, 1.0);
            let ended = staged(Stage::Removal { at }, 0.0);
            for index in 0..count {
                assert_eq!(
                    started.thumbnail(index, count, display()),
                    grown().thumbnail(after_insertion(at, index), count + 1, display())
                );
                assert_eq!(
                    ended.thumbnail(index, count, display()),
                    grown().thumbnail(index, count, display())
                );
            }
        }
    }

    #[test]
    fn a_window_lands_on_its_miniature_and_a_split_share_holds_it_whole_centred_down_its_height() {
        let thumbnail = Rectangle::new(Point::from((1104.0, 29.0)), Size::from((160.0, 90.0)));
        assert_eq!(
            miniature(thumbnail, Point::from((320.0, 160.0)), display()),
            (Point::from((1124.0, 39.0)), 1.0 / 16.0)
        );
        let (left, right) = split_shares(thumbnail, 0.25);
        assert_eq!(
            (left, right),
            (
                Rectangle::new(Point::from((1104.0, 29.0)), Size::from((40.0, 90.0))),
                Rectangle::new(Point::from((1144.0, 29.0)), Size::from((120.0, 90.0)))
            )
        );
        assert_eq!(
            fitted(right, Size::from((1200.0, 600.0))),
            (Point::from((1144.0, 44.0)), 0.1)
        );
    }

    #[test]
    fn slot_at_picks_the_nearest_thumbnail_centre_and_clamps_to_the_row() {
        assert_eq!(slot_at(1088.0 + 95.0, 3, display()), 0);
        assert_eq!(slot_at(1088.0 + 97.0, 3, display()), 1);
        assert_eq!(slot_at(1280.0 + 97.0, 3, display()), 2);
        assert_eq!(slot_at(0.0, 3, display()), 0);
        assert_eq!(slot_at(2560.0, 3, display()), 2);
    }

    #[test]
    fn on_a_display_too_narrow_for_its_row_the_thumbnails_still_do_not_overlap() {
        let narrow = Size::from((800.0, 600.0));
        let strip = Strip::default();
        let first = strip.thumbnail(0, 16, narrow);
        let last = strip.thumbnail(15, 16, narrow);
        assert!(first.loc.x < 0.0 && last.loc.x + last.size.w > narrow.w);
        for index in 1..16 {
            let left = strip.thumbnail(index - 1, 16, narrow);
            let right = strip.thumbnail(index, 16, narrow);
            assert_eq!(right.loc.x - (left.loc.x + left.size.w), 32.0);
        }
    }
}
