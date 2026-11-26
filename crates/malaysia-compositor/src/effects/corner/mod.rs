use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

#[derive(Clone, Copy)]
pub struct Curve {
    pub bezier: [f32; 4],
    pub rise: f32,
}

impl Curve {
    pub fn new(smoothing: f32) -> Curve {
        let reach = 1.0 + smoothing;
        let arc_measure = FRAC_PI_2 * (1.0 - smoothing);
        let beta = FRAC_PI_4 * smoothing;
        let arc_section = beta.cos() - beta.sin();
        let theta = (FRAC_PI_2 - arc_measure) / 2.0;
        let c = (theta / 2.0).tan() * beta.cos();
        let rise = c * beta.tan();
        let b = (reach - arc_section - c - rise) / 3.0;
        let a = 2.0 * b;
        Curve {
            bezier: [reach, reach - a, reach - a - b, reach - a - b - c],
            rise,
        }
    }

    pub fn fit(self, radius: f32, half_side: f32) -> (f32, Curve) {
        let radius = radius.min(half_side);
        if self.reach() * radius <= half_side {
            return (radius, self);
        }
        (radius, Curve::new(half_side / radius - 1.0))
    }

    pub fn reach(self) -> f32 {
        self.bezier[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::SQRT_2;

    const CAPTURE: &str = include_str!("window-16.csv");
    const CAPTURE_SIZE: usize = 32;

    fn slope(bezier: [f32; 4], t: f32) -> f32 {
        let w = 1.0 - t;
        3.0 * w * w * (bezier[1] - bezier[0])
            + 6.0 * w * t * (bezier[2] - bezier[1])
            + 3.0 * t * t * (bezier[3] - bezier[2])
    }

    fn signed_distance(curve: Curve, edge: [f32; 2], radius: f32) -> f32 {
        let u = edge[0].max(edge[1]);
        let v = edge[0].min(edge[1]);
        let bezier = curve.bezier.map(|control| control * radius);
        if u >= bezier[0] {
            return v;
        }
        if u >= bezier[3] {
            let rise = curve.rise * radius;
            let mut t = (bezier[0] - u) / (bezier[0] - bezier[3]);
            for _ in 0..2 {
                let w = 1.0 - t;
                let position = w * w * w * bezier[0]
                    + 3.0 * w * w * t * bezier[1]
                    + 3.0 * w * t * t * bezier[2]
                    + t * t * t * bezier[3];
                t = (t - (position - u) / slope(bezier, t)).clamp(0.0, 1.0);
            }
            let tangent = slope(bezier, t);
            return (v - rise * t * t * t) * tangent.abs() / tangent.hypot(3.0 * rise * t * t);
        }
        radius - (radius - u).hypot(radius - v)
    }

    fn coverage(curve: Curve, edge: [f32; 2], radius: f32) -> f32 {
        (signed_distance(curve, edge, radius) + 0.5).clamp(0.0, 1.0)
    }

    fn capture() -> Vec<f32> {
        CAPTURE
            .trim()
            .split(',')
            .skip(4)
            .map(|value| value.parse::<f32>().unwrap() / 255.0)
            .collect()
    }

    fn error_against_capture(radius: f32) -> (f32, f32) {
        let curve = Curve::new(ctdesign::CORNER_WINDOW.smoothing);
        let capture = capture();
        assert_eq!(capture.len(), CAPTURE_SIZE * CAPTURE_SIZE);
        let (mut squares, mut worst) = (0.0_f32, 0.0_f32);
        for (index, expected) in capture.iter().enumerate() {
            let x = (index % CAPTURE_SIZE) as f32 + 0.5;
            let y = (index / CAPTURE_SIZE) as f32 + 0.5;
            let error = coverage(curve, [x, y], radius) - expected;
            squares += error * error;
            worst = worst.max(error.abs());
        }
        ((squares / capture.len() as f32).sqrt(), worst)
    }

    #[test]
    fn a_window_corner_is_the_macos_window_capture() {
        let (rms, worst) = error_against_capture(ctdesign::CORNER_WINDOW.fitted);
        let (plain_rms, _) = error_against_capture(ctdesign::CORNER_WINDOW.radius);
        assert!(rms < 0.016, "rms {rms}");
        assert!(worst < 0.12, "max {worst}");
        assert!(plain_rms > 2.0 * rms, "radius 16 rms {plain_rms}");
    }

    #[test]
    fn the_continuous_corner_reaches_one_plus_smoothing_radii_along_the_edge_and_meets_the_circle_on_the_diagonal()
     {
        let smoothing = ctdesign::CORNER_WINDOW.smoothing;
        let curve = Curve::new(smoothing);
        let radius = 100.0;
        let reach = (1.0 + smoothing) * radius;
        assert!((curve.reach() - (1.0 + smoothing)).abs() < 1e-6);
        assert_eq!(signed_distance(curve, [reach, 0.0], radius), 0.0);
        assert!(signed_distance(curve, [reach - 1.0, 0.0], radius) < 0.0);
        let diagonal = radius * (1.0 - SQRT_2 / 2.0);
        assert!(signed_distance(curve, [diagonal, diagonal], radius).abs() < 1e-3);
    }

    #[test]
    fn a_corner_too_large_for_its_side_ends_its_tails_at_the_middle_and_is_the_exact_circle_at_half_the_side()
     {
        let curve = Curve::new(ctdesign::CORNER_WINDOW.smoothing);
        let (radius, same) = curve.fit(16.0, 30.0);
        assert_eq!(radius, 16.0);
        assert_eq!(same.bezier, curve.bezier);
        let (radius, shorter) = curve.fit(16.0, 20.0);
        assert!((shorter.reach() * radius - 20.0).abs() < 1e-4);
        let (radius, circle) = curve.fit(16.0, 10.0);
        assert_eq!(radius, 10.0);
        assert_eq!(circle.bezier, [1.0; 4]);
        assert_eq!(circle.rise, 0.0);
    }
}
