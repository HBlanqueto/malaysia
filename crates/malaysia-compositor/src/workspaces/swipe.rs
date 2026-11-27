use smithay::utils::{Logical, Point};

const LOCK_DISTANCE: f64 = ctdesign::SWIPE_LOCK_DISTANCE;
const HISTORY_MICROS: u64 = ctdesign::SWIPE_HISTORY_MS * 1000;
const FLING_VELOCITY: f64 = ctdesign::SWIPE_FLING_VELOCITY;
const DECELERATION: f64 = ctdesign::SWIPE_DECELERATION;
const SAMPLES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    fn distance(self) -> f64 {
        match self {
            Axis::Horizontal => ctdesign::SWIPE_DISTANCE_HORIZONTAL,
            Axis::Vertical => ctdesign::SWIPE_DISTANCE_VERTICAL,
        }
    }

    fn along(self, delta: Point<f64, Logical>) -> f64 {
        match self {
            Axis::Horizontal => -delta.x,
            Axis::Vertical => -delta.y,
        }
    }
}

#[derive(Default)]
pub struct Swipe {
    pending: Point<f64, Logical>,
    pub axis: Option<Axis>,
    progress: f64,
    initial: f64,
    cancel: f64,
    last: f64,
    samples: [(u64, f64); SAMPLES],
    newest: usize,
    count: usize,
}

impl Swipe {
    pub fn lock(&mut self, delta: Point<f64, Logical>) -> Option<(Axis, f64)> {
        self.pending += delta;
        if self.pending.x.hypot(self.pending.y) < LOCK_DISTANCE {
            return None;
        }
        let axis = if self.pending.x.abs() > self.pending.y.abs() {
            Axis::Horizontal
        } else {
            Axis::Vertical
        };
        Some((axis, axis.along(self.pending)))
    }

    pub fn track(&mut self, axis: Axis, progress: f64, cancel: f64, last: f64, time: u64) {
        self.axis = Some(axis);
        self.progress = progress;
        self.initial = progress;
        self.cancel = cancel;
        self.last = last;
        self.record(time, 0.0);
    }

    pub fn follow(&mut self, delta: Point<f64, Logical>, time: u64) -> f64 {
        let Some(axis) = self.axis else {
            return self.progress;
        };
        let along = axis.along(delta);
        self.record(time, along);
        let (lower, upper) = self.bounds();
        self.progress = (self.progress + along / axis.distance()).clamp(lower, upper);
        self.progress
    }

    pub fn release(&self, cancelled: bool, time: u64) -> (f64, f64) {
        let Some(axis) = self.axis else {
            return (self.cancel, 0.0);
        };
        let velocity = self.velocity(time);
        let (lower, upper) = self.bounds();
        let pinned = (velocity > 0.0 && self.progress >= upper)
            || (velocity < 0.0 && self.progress <= lower);
        let per_second = if pinned {
            0.0
        } else {
            velocity / axis.distance() * 1000.0
        };
        let target = if cancelled {
            self.cancel
        } else {
            self.end(velocity)
        };
        (target, per_second)
    }

    fn bounds(&self) -> (f64, f64) {
        (
            (self.initial.floor() - 1.0).max(0.0),
            (self.initial.ceil() + 1.0).min(self.last),
        )
    }

    fn end(&self, velocity: f64) -> f64 {
        if velocity.abs() < FLING_VELOCITY {
            return self.progress.round();
        }
        let slope = DECELERATION / (1.0 - DECELERATION) / 1000.0;
        let (lower, upper) = self.bounds();
        let projected = (self.progress + velocity * slope).clamp(lower, upper);
        let initial = self.initial.round();
        if velocity > 0.0 && projected.floor() == initial {
            projected.ceil()
        } else if velocity < 0.0 && projected.ceil() == initial {
            projected.floor()
        } else {
            projected.round()
        }
    }

    fn record(&mut self, time: u64, delta: f64) {
        self.newest = (self.newest + 1) % SAMPLES;
        self.samples[self.newest] = (time, delta);
        self.count = (self.count + 1).min(SAMPLES);
    }

    fn velocity(&self, now: u64) -> f64 {
        let threshold = now.saturating_sub(HISTORY_MICROS);
        let mut newest = None;
        let (mut oldest, mut oldest_delta, mut total) = (0, 0.0, 0.0);
        for (time, delta) in (0..self.count)
            .map(|age| self.samples[(self.newest + SAMPLES - age) % SAMPLES])
            .take_while(|(time, _)| *time >= threshold)
        {
            newest.get_or_insert(time);
            total += delta;
            (oldest, oldest_delta) = (time, delta);
        }
        match newest {
            Some(newest) if newest > oldest => {
                (total - oldest_delta) / ((newest - oldest) as f64 / 1000.0)
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MILLISECOND: u64 = 1000;

    fn tracking(axis: Axis, progress: f64, last: f64) -> Swipe {
        let mut swipe = Swipe::default();
        swipe.track(axis, progress, progress, last, 0);
        swipe
    }

    fn up(pixels: f64) -> Point<f64, Logical> {
        (0.0, -pixels).into()
    }

    fn left(pixels: f64) -> Point<f64, Logical> {
        (-pixels, 0.0).into()
    }

    #[test]
    fn the_axis_locks_after_16_of_travel_on_the_larger_component_with_up_and_left_positive() {
        let mut swipe = Swipe::default();
        assert_eq!(swipe.lock((3.0, -8.0).into()), None);
        assert_eq!(swipe.lock((3.0, -8.0).into()), Some((Axis::Vertical, 16.0)));
        let mut swipe = Swipe::default();
        assert_eq!(swipe.lock(left(16.0)), Some((Axis::Horizontal, 16.0)));
    }

    #[test]
    fn progress_is_travel_over_300_up_and_400_sideways_and_stays_within_one_entry_of_the_start() {
        let mut swipe = tracking(Axis::Vertical, 0.0, 1.0);
        assert_eq!(swipe.follow(up(150.0), MILLISECOND), 0.5);
        assert_eq!(swipe.follow(up(600.0), 2 * MILLISECOND), 1.0);
        let mut swipe = tracking(Axis::Horizontal, 1.0, 4.0);
        assert_eq!(swipe.follow(left(200.0), MILLISECOND), 1.5);
        assert_eq!(swipe.follow(left(2000.0), 2 * MILLISECOND), 2.0);
        assert_eq!(swipe.follow(left(-4000.0), 3 * MILLISECOND), 0.0);
    }

    #[test]
    fn a_slow_release_goes_to_the_nearest_end() {
        let mut swipe = tracking(Axis::Vertical, 0.0, 1.0);
        swipe.follow(up(140.0), 1000 * MILLISECOND);
        assert_eq!(swipe.release(false, 1000 * MILLISECOND).0, 0.0);
        swipe.follow(up(20.0), 2000 * MILLISECOND);
        assert_eq!(swipe.release(false, 2000 * MILLISECOND).0, 1.0);
    }

    #[test]
    fn a_fast_release_short_of_half_way_moves_one_entry_in_the_direction_of_the_fingers() {
        let mut swipe = tracking(Axis::Horizontal, 1.0, 2.0);
        for step in 1..=5 {
            swipe.follow(left(10.0), step * 10 * MILLISECOND);
        }
        let (target, velocity) = swipe.release(false, 50 * MILLISECOND);
        assert_eq!(target, 2.0);
        assert_eq!(velocity, 1.0 / 400.0 * 1000.0);
        let mut swipe = tracking(Axis::Horizontal, 1.0, 2.0);
        for step in 1..=5 {
            swipe.follow(left(-10.0), step * 10 * MILLISECOND);
        }
        assert_eq!(swipe.release(false, 50 * MILLISECOND).0, 0.0);
    }

    #[test]
    fn fingers_pushing_past_the_end_hand_no_velocity_to_the_spring() {
        let mut swipe = tracking(Axis::Vertical, 0.0, 1.0);
        for step in 1..=5 {
            swipe.follow(up(100.0), step * 10 * MILLISECOND);
        }
        assert_eq!(swipe.release(false, 50 * MILLISECOND), (1.0, 0.0));
        let mut swipe = tracking(Axis::Horizontal, 0.0, 2.0);
        for step in 1..=5 {
            swipe.follow(left(-10.0), step * 10 * MILLISECOND);
        }
        assert_eq!(swipe.release(false, 50 * MILLISECOND), (0.0, 0.0));
    }

    #[test]
    fn a_cancel_returns_to_where_the_gesture_started() {
        let mut swipe = Swipe::default();
        swipe.track(Axis::Vertical, 0.4, 0.0, 1.0, 0);
        swipe.follow(up(250.0), 10 * MILLISECOND);
        assert_eq!(swipe.release(true, 10 * MILLISECOND).0, 0.0);
    }

    #[test]
    fn velocity_leaves_out_the_first_sample_and_what_is_older_than_150_ms_and_survives_a_full_ring()
    {
        let mut swipe = tracking(Axis::Horizontal, 0.0, 100.0);
        swipe.follow(left(1000.0), 0);
        for step in 1..=100 {
            swipe.follow(left(4.0), 1000 * MILLISECOND + step * 5 * MILLISECOND);
        }
        assert_eq!(swipe.velocity(1500 * MILLISECOND), 0.8);
        assert_eq!(swipe.velocity(2000 * MILLISECOND), 0.0);
    }
}
