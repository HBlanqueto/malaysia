pub fn spring(omega: f64, damping: f64, offset: f64, velocity: f64, seconds: f64) -> (f64, f64) {
    let spread = (damping * damping - 1.0).sqrt();
    let slow_rate = omega * (-damping + spread);
    let fast_rate = omega * (-damping - spread);
    let slow = (velocity - fast_rate * offset) / (slow_rate - fast_rate);
    let fast = offset - slow;
    let slow_decay = (slow_rate * seconds).exp();
    let fast_decay = (fast_rate * seconds).exp();
    (
        slow * slow_decay + fast * fast_decay,
        slow * slow_rate * slow_decay + fast * fast_rate * fast_decay,
    )
}

pub fn rests(omega: f64, offset: f64, velocity: f64, stride: f64) -> bool {
    offset.abs().max(velocity.abs() / omega) * stride < 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    const OMEGA: f64 = 25.0;
    const DAMPING: f64 = 1.4;

    #[test]
    fn a_spring_restarted_from_its_own_state_follows_the_same_curve() {
        let (offset, velocity) = spring(OMEGA, DAMPING, 1.0, 0.0, 0.1);
        let (restarted, restarted_velocity) = spring(OMEGA, DAMPING, offset, velocity, 0.2);
        let (continued, continued_velocity) = spring(OMEGA, DAMPING, 1.0, 0.0, 0.3);
        assert!((restarted - continued).abs() < 1e-12);
        assert!((restarted_velocity - continued_velocity).abs() < 1e-12);
    }

    #[test]
    fn a_spring_does_not_rest_while_crossing_its_target_at_speed() {
        let (_, velocity) = spring(OMEGA, DAMPING, 1.0, 0.0, 0.1);
        assert!(!rests(OMEGA, 0.0, velocity, 1344.0));
        assert!(rests(OMEGA, 0.0, 0.0, 1344.0));
    }
}
