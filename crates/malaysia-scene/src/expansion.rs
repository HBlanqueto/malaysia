use std::time::Duration;

use crate::geometry::{Logical, Point, Rect, Size};

pub fn rect_at(
    from: Rect<f64, Logical>,
    to: Rect<f64, Logical>,
    progress: f64,
) -> Rect<f64, Logical> {
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    let edge = |start: f64, end: f64| (start + (end - start) * progress).round();
    let left = edge(from.x(), to.x());
    let top = edge(from.y(), to.y());
    let right = edge(from.right(), to.right());
    let bottom = edge(from.bottom(), to.bottom());
    Rect::new(Point::new(left, top), Size::new(right - left, bottom - top))
}

pub fn duration(
    from: Rect<f64, Logical>,
    to: Rect<f64, Logical>,
    output: Size<f64, Logical>,
    shortest: Duration,
    longest: Duration,
) -> Duration {
    shortest + (longest - shortest).mul_f64(farthest_edge_share(from, to, output))
}

fn farthest_edge_share(
    from: Rect<f64, Logical>,
    to: Rect<f64, Logical>,
    output: Size<f64, Logical>,
) -> f64 {
    [
        (to.x() - from.x()).abs() / output.width,
        (to.right() - from.right()).abs() / output.width,
        (to.y() - from.y()).abs() / output.height,
        (to.bottom() - from.bottom()).abs() / output.height,
    ]
    .into_iter()
    .fold(0.0, f64::max)
    .min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORTEST: Duration = Duration::from_millis(220);
    const LONGEST: Duration = Duration::from_millis(420);

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect<f64, Logical> {
        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    fn output() -> Size<f64, Logical> {
        Size::new(1440.0, 900.0)
    }

    fn right(rect: Rect<f64, Logical>) -> f64 {
        rect.x() + rect.width()
    }

    fn bottom(rect: Rect<f64, Logical>) -> f64 {
        rect.y() + rect.height()
    }

    fn lasts(from: Rect<f64, Logical>, to: Rect<f64, Logical>) -> Duration {
        duration(from, to, output(), SHORTEST, LONGEST)
    }

    #[test]
    fn every_edge_is_half_way_to_its_own_target_at_half_progress() {
        let from = rect(80.0, 120.0, 720.0, 460.0);
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        let half = rect_at(from, to, 0.5);
        assert_eq!(half.x(), 40.0);
        assert_eq!(half.y(), 76.0);
        assert_eq!(right(half), 1120.0);
        assert_eq!(bottom(half), 696.0);
    }

    #[test]
    fn the_edge_near_its_target_moves_less_than_the_far_one() {
        let from = rect(80.0, 120.0, 720.0, 460.0);
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        let moved = rect_at(from, to, 0.4);
        let near = (moved.x() - from.x()).abs();
        let far = (right(moved) - right(from)).abs();
        assert!(near < far / 4.0);
    }

    #[test]
    fn every_edge_has_covered_the_same_share_of_its_distance() {
        let from = rect(80.0, 120.0, 720.0, 460.0);
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        let progress = 0.3;
        let moved = rect_at(from, to, progress);
        let shares = [
            (moved.x() - from.x()) / (to.x() - from.x()),
            (moved.y() - from.y()) / (to.y() - from.y()),
            (right(moved) - right(from)) / (right(to) - right(from)),
            (bottom(moved) - bottom(from)) / (bottom(to) - bottom(from)),
        ];
        for share in shares {
            assert!((share - progress).abs() < 0.01, "{share}");
        }
    }

    #[test]
    fn a_window_flush_with_a_side_keeps_that_edge_still() {
        let from = rect(0.0, 120.0, 720.0, 460.0);
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        assert_eq!(rect_at(from, to, 0.6).x(), 0.0);
    }

    #[test]
    fn progress_zero_is_the_start_and_one_is_exactly_the_target() {
        let from = rect(80.4, 120.7, 720.1, 460.3);
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        assert_eq!(rect_at(from, to, 0.0), from);
        assert_eq!(rect_at(from, to, 1.0), to);
        assert_eq!(rect_at(from, to, -0.2), from);
        assert_eq!(rect_at(from, to, 1.2), to);
    }

    #[test]
    fn the_edges_land_on_whole_pixels_in_between() {
        let from = rect(80.0, 120.0, 720.0, 460.0);
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        for step in 1..20 {
            let moved = rect_at(from, to, f64::from(step) / 20.0);
            for edge in [moved.x(), moved.y(), right(moved), bottom(moved)] {
                assert_eq!(edge, edge.round());
            }
        }
    }

    #[test]
    fn collapsing_runs_the_same_path_backwards() {
        let small = rect(80.0, 120.0, 720.0, 460.0);
        let large = rect(0.0, 32.0, 1440.0, 780.0);
        assert_eq!(rect_at(large, small, 0.25), rect_at(small, large, 0.75));
    }

    #[test]
    fn a_window_already_on_target_lasts_the_shortest() {
        let target = rect(0.0, 32.0, 1440.0, 780.0);
        assert_eq!(lasts(target, target), SHORTEST);
    }

    #[test]
    fn the_duration_grows_with_how_far_the_farthest_edge_goes() {
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        let near = lasts(rect(40.0, 60.0, 1360.0, 740.0), to);
        let middle = lasts(rect(360.0, 192.0, 720.0, 460.0), to);
        let far = lasts(rect(700.0, 400.0, 40.0, 40.0), to);
        assert!(SHORTEST < near && near < middle && middle < far && far <= LONGEST);
    }

    #[test]
    fn the_duration_follows_the_farthest_edge_and_ignores_the_nearest() {
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        let flush_left = lasts(rect(0.0, 32.0, 720.0, 780.0), to);
        let centred = lasts(rect(360.0, 32.0, 720.0, 780.0), to);
        assert_eq!(flush_left, SHORTEST + (LONGEST - SHORTEST).mul_f64(0.5));
        assert_eq!(centred, SHORTEST + (LONGEST - SHORTEST).mul_f64(0.25));
    }

    #[test]
    fn the_duration_never_passes_the_longest() {
        let to = rect(0.0, 32.0, 1440.0, 780.0);
        assert_eq!(lasts(rect(-5000.0, -5000.0, 10.0, 10.0), to), LONGEST);
    }
}
