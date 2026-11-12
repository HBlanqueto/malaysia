use std::time::{Duration, Instant};

pub(crate) const SHAKE_MIN_DX: f64 = 2.0;
pub(crate) const SHAKE_MIN_SWING: f64 = 24.0;
pub(crate) const SHAKE_MAX_SWING: Duration = Duration::from_millis(300);
pub(crate) const SHAKE_MAX_TOTAL: Duration = Duration::from_secs(1);
pub(crate) const SHAKE_REVERSALS: u8 = 3;

#[derive(Debug, Default)]
pub(crate) struct ShakeDetector {
    last_x: Option<f64>,
    direction: i8,
    swing: f64,
    reversals: u8,
    started: Option<Instant>,
    last_reversal: Option<Instant>,
    pub(crate) last_shake: Option<Instant>,
}

impl ShakeDetector {
    pub(crate) fn observe(&mut self, x: f64, now: Instant) -> bool {
        let Some(last) = self.last_x.replace(x) else {
            return false;
        };
        let dx = x - last;
        if dx.abs() < SHAKE_MIN_DX {
            return false;
        }
        let direction = if dx > 0.0 { 1 } else { -1 };

        if self.direction == 0 {
            self.start(direction, dx.abs(), now);
            return false;
        }
        if direction == self.direction {
            self.swing += dx.abs();
            if self
                .started
                .is_some_and(|started| now.duration_since(started) > SHAKE_MAX_TOTAL)
            {
                self.start(direction, dx.abs(), now);
            }
            return false;
        }

        let swing = self.swing;
        let reversal_dt = self
            .last_reversal
            .map(|last| now.duration_since(last))
            .unwrap_or_default();
        let total = self
            .started
            .map(|started| now.duration_since(started))
            .unwrap_or_default();
        if swing < SHAKE_MIN_SWING || reversal_dt > SHAKE_MAX_SWING || total > SHAKE_MAX_TOTAL {
            self.start(direction, dx.abs(), now);
            return false;
        }

        self.reversals += 1;
        self.direction = direction;
        self.last_reversal = Some(now);
        self.last_shake = Some(now);
        self.swing = dx.abs();
        if self.reversals >= SHAKE_REVERSALS {
            self.direction = 0;
            self.swing = 0.0;
            self.reversals = 0;
            self.started = None;
            self.last_reversal = None;
            return true;
        }
        false
    }

    fn start(&mut self, direction: i8, swing: f64, now: Instant) {
        self.direction = direction;
        self.swing = swing;
        self.reversals = 0;
        self.started = Some(now);
        self.last_reversal = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::ShakeDetector;

    #[test]
    fn a_rapid_side_to_side_shake_triggers_the_locate_growth() {
        let mut shake = ShakeDetector::default();
        let start = std::time::Instant::now();
        let mut fired = false;
        for (index, x) in [0.0, 100.0, 0.0, 100.0, 0.0].into_iter().enumerate() {
            fired |= shake.observe(
                x,
                start + std::time::Duration::from_millis(index as u64 * 20),
            );
        }
        assert!(fired, "left-right-left-right should trigger");
    }

    #[test]
    fn a_slow_reversal_does_not_trigger_the_locate_growth() {
        let mut shake = ShakeDetector::default();
        let start = std::time::Instant::now();
        let mut fired = false;
        for (index, x) in [0.0, 100.0, 0.0, 100.0, 0.0].into_iter().enumerate() {
            let elapsed = std::time::Duration::from_millis(index as u64 * 400);
            fired |= shake.observe(x, start + elapsed);
        }
        assert!(!fired, "one swing per few hundred ms is not a shake");
    }

    #[test]
    fn tiny_jitter_does_not_trigger_the_locate_growth() {
        let mut shake = ShakeDetector::default();
        let start = std::time::Instant::now();
        let mut fired = false;
        for (index, x) in [0.0, 1.0, 0.0, 1.0, 0.0].into_iter().enumerate() {
            fired |= shake.observe(
                x,
                start + std::time::Duration::from_millis(index as u64 * 10),
            );
        }
        assert!(!fired, "sub-pixel jitter is not a deliberate shake");
    }
}
