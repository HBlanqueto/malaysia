use smithay::utils::{Physical, Raw, Size};

const MOBILE_TARGET_DPI: f64 = 135.0;
const LARGE_TARGET_DPI: f64 = 110.0;
const LARGE_MIN_DIAGONAL_INCHES: f64 = 20.0;
const MM_PER_INCH: f64 = 25.4;
const MAX_SCALE: i32 = 4;
const MAX_DENOMINATOR: i32 = 4;
const MIN_LOGICAL_AREA: i32 = 600 * 600;
const ASPECT_RATIOS_AS_SIZE: [(i32, i32); 6] = [
    (1600, 900),
    (1600, 1000),
    (160, 90),
    (160, 100),
    (16, 9),
    (16, 10),
];

pub fn output_scale(size_mm: Size<i32, Raw>, mode_size: Size<i32, Physical>) -> f64 {
    if size_mm.w <= 0 || size_mm.h <= 0 || ASPECT_RATIOS_AS_SIZE.contains(&(size_mm.w, size_mm.h)) {
        return 1.0;
    }
    let diagonal_inches = f64::from(size_mm.w).hypot(f64::from(size_mm.h)) / MM_PER_INCH;
    let target_dpi = if diagonal_inches < LARGE_MIN_DIAGONAL_INCHES {
        MOBILE_TARGET_DPI
    } else {
        LARGE_TARGET_DPI
    };
    let dpi = f64::from(mode_size.w).hypot(f64::from(mode_size.h)) / diagonal_inches;
    let ideal = dpi / target_dpi;
    (1..=MAX_DENOMINATOR)
        .flat_map(|denominator| {
            (denominator..=MAX_SCALE * denominator).map(move |numerator| (numerator, denominator))
        })
        .filter(|&(numerator, denominator)| {
            mode_size.w * denominator % numerator == 0
                && mode_size.h * denominator % numerator == 0
                && (mode_size.w * denominator / numerator) * (mode_size.h * denominator / numerator)
                    >= MIN_LOGICAL_AREA
        })
        .map(|(numerator, denominator)| f64::from(numerator) / f64::from(denominator))
        .min_by(|a, b| {
            (a - ideal)
                .abs()
                .total_cmp(&(b - ideal).abs())
                .then(a.total_cmp(b))
        })
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(size_mm: (i32, i32), mode_size: (i32, i32)) -> f64 {
        output_scale(size_mm.into(), mode_size.into())
    }

    #[test]
    fn matches_the_fractional_scales_mutter_expects() {
        for (size_mm, mode_size, expected) in [
            ((65, 129), (720, 1440), 5.0 / 3.0),
            ((68, 144), (1080, 2280), 2.5),
            ((64, 142), (1080, 2400), 8.0 / 3.0),
            ((286, 179), (2560, 1600), 5.0 / 3.0),
            ((303, 202), (2400, 1600), 4.0 / 3.0),
            ((290, 180), (3840, 2400), 2.5),
            ((300, 190), (3840, 2400), 2.5),
            ((509, 286), (1920, 1080), 1.0),
            ((509, 286), (3840, 2160), 5.0 / 3.0),
            ((598, 336), (3840, 2160), 1.5),
            ((708, 398), (3840, 2160), 1.25),
            ((554, 312), (3840, 2160), 5.0 / 3.0),
            ((522, 294), (3840, 2160), 5.0 / 3.0),
            ((150, 80), (1600, 900), 2.0),
            ((180, 100), (1600, 900), 5.0 / 3.0),
        ] {
            assert_eq!(
                scale(size_mm, mode_size),
                expected,
                "{size_mm:?} {mode_size:?}"
            );
        }
    }

    #[test]
    fn unknown_or_aspect_ratio_sizes_give_scale_one() {
        for size_mm in [(0, 0), (0, 200), (300, 0)]
            .into_iter()
            .chain(ASPECT_RATIOS_AS_SIZE)
        {
            assert_eq!(scale(size_mm, (3840, 2160)), 1.0, "{size_mm:?}");
        }
    }
}
