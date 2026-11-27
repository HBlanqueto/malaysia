use std::time::Duration;

pub struct Motion {
    from: f64,
    pub velocity: f64,
    started: Duration,
}

impl Motion {
    pub fn new(from: f64, velocity: f64, started: Duration) -> Motion {
        Motion {
            from,
            velocity,
            started,
        }
    }

    pub fn at(&self, omega: f64, damping: f64, target: f64, now: Duration) -> (f64, f64) {
        let seconds = now.saturating_sub(self.started).as_secs_f64();
        let (offset, velocity) = spring(omega, damping, self.from - target, self.velocity, seconds);
        (target + offset, velocity)
    }
}

pub fn spring(omega: f64, damping: f64, offset: f64, velocity: f64, seconds: f64) -> (f64, f64) {
    let decay = damping * omega;
    let (even, odd) = if damping < 1.0 {
        let fade = (-decay * seconds).exp();
        let frequency = omega * (1.0 - damping * damping).sqrt();
        let angle = frequency * seconds;
        (fade * angle.cos(), fade * angle.sin() / frequency)
    } else {
        let spread = omega * (damping * damping - 1.0).sqrt();
        let slow = ((spread - decay) * seconds).exp();
        let fast = ((-spread - decay) * seconds).exp();
        let even = (slow + fast) / 2.0;
        let odd = if spread == 0.0 {
            seconds * slow
        } else {
            (slow - fast) / (2.0 * spread)
        };
        (even, odd)
    };
    (
        offset * even + (velocity + decay * offset) * odd,
        velocity * even - (decay * velocity + omega * omega * offset) * odd,
    )
}

pub fn sample_motion(
    motion: &Option<Motion>,
    omega: f64,
    damping: f64,
    resting: f64,
    target: f64,
    now: Duration,
) -> (f64, f64) {
    motion.as_ref().map_or((resting, 0.0), |motion| {
        motion.at(omega, damping, target, now)
    })
}

pub fn rests(omega: f64, offset: f64, velocity: f64, stride: f64) -> bool {
    offset.abs().max(velocity.abs() / omega) * stride < 0.5
}

pub fn advance_motion(
    motion: &mut Option<Motion>,
    omega: f64,
    damping: f64,
    target: f64,
    now: Duration,
    stride: f64,
) -> Option<f64> {
    let (value, velocity) = motion.as_ref()?.at(omega, damping, target, now);
    if rests(omega, value - target, velocity, stride) {
        *motion = None;
        return Some(target);
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OMEGA: f64 = 25.0;
    const DAMPING: f64 = 1.4;

    #[test]
    fn a_spring_restarted_from_its_own_state_follows_the_same_curve() {
        for damping in [DAMPING, 1.0, ctdesign::SPACES_STRIP_EXIT_DAMPING as f64] {
            let (offset, velocity) = spring(OMEGA, damping, 1.0, 0.0, 0.1);
            let (restarted, restarted_velocity) = spring(OMEGA, damping, offset, velocity, 0.2);
            let (continued, continued_velocity) = spring(OMEGA, damping, 1.0, 0.0, 0.3);
            assert!((restarted - continued).abs() < 1e-12);
            assert!((restarted_velocity - continued_velocity).abs() < 1e-12);
        }
    }

    #[test]
    fn a_critically_damped_step_from_rest_is_half_way_near_67_ms_and_at_ninety_percent_near_156_ms()
    {
        let first_millisecond_at_or_below = |remaining: f64| {
            (0..)
                .find(|&millisecond| {
                    spring(OMEGA, 1.0, 1.0, 0.0, millisecond as f64 / 1000.0).0 <= remaining
                })
                .unwrap()
        };
        assert!((66..=69).contains(&first_millisecond_at_or_below(0.5)));
        assert!((154..=158).contains(&first_millisecond_at_or_below(0.1)));
    }

    #[test]
    fn a_spring_does_not_rest_while_crossing_its_target_at_speed() {
        let (_, velocity) = spring(OMEGA, DAMPING, 1.0, 0.0, 0.1);
        assert!(!rests(OMEGA, 0.0, velocity, 1344.0));
        assert!(rests(OMEGA, 0.0, 0.0, 1344.0));
    }
}
