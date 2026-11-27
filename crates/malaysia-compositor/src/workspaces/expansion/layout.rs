use smithay::utils::{Logical, Point, Rectangle};

const OVERLAP_MARGIN: f64 = ctdesign::EXPANSION_WINDOW_GAP_MIN as f64;
const TARGET_GAP: f64 = ctdesign::EXPANSION_WINDOW_GAP as f64;
const STEP: f64 = 0.75;
const TIE_PER_STACKING_PLACE: f64 = 0.25;
const WALL_SIDEWAYS: f64 = 2.0;
const WALL_UP_AND_DOWN: f64 = 3.0;
const SHRINK_PER_CORNERED_SWEEP: f64 = 0.995;
const SWEEPS: usize = 400;
const BISECTIONS: usize = 18;
const LOWEST_SCALE: f64 = 0.05;
const LONE_INSET: f64 = ctdesign::EXPANSION_LONE_INSET as f64;
const LONE_HEIGHT_CAP_INSET: f64 = ctdesign::EXPANSION_LONE_HEIGHT_INSET as f64;
const SAME_CENTRE: f64 = 1e-9;
const GRID_GAP: f64 = ctdesign::EXPANSION_GRID_GAP as f64;
const CAPTION_HEIGHT: f64 =
    ctdesign::EXPANSION_TITLE_FONT.line_height as f64 + ctdesign::SPACES_LABEL_GAP as f64;

pub fn arrange(
    area: Rectangle<f64, Logical>,
    frames: &[Rectangle<f64, Logical>],
    slots: &mut Vec<Rectangle<f64, Logical>>,
    pushes: &mut Vec<(Point<f64, Logical>, u32)>,
) -> f64 {
    if let [frame] = frames {
        return arrange_lone(area, *frame, slots);
    }
    let widest = frames.iter().map(|frame| frame.size.w).fold(0.0, f64::max);
    let tallest = frames.iter().map(|frame| frame.size.h).fold(0.0, f64::max);
    let start = 1f64.min(area.size.w / widest).min(area.size.h / tallest);
    let (settled, scale) = settle(area, frames, start, slots, pushes);
    if settled {
        return scale;
    }
    let (mut low, mut high, mut best) = (LOWEST_SCALE, start, start);
    for _ in 0..BISECTIONS {
        let middle = (low + high) / 2.0;
        if settle(area, frames, middle, slots, pushes).0 {
            low = middle;
            best = middle;
        } else {
            high = middle;
        }
    }
    settle(area, frames, best, slots, pushes).1
}

fn arrange_lone(
    area: Rectangle<f64, Logical>,
    frame: Rectangle<f64, Logical>,
    slots: &mut Vec<Rectangle<f64, Logical>>,
) -> f64 {
    let scale = 1f64
        .min(area.size.w / frame.size.w)
        .min((area.size.h - LONE_HEIGHT_CAP_INSET) / frame.size.h);
    let size = frame.size.upscale(scale);
    let centre = centre(frame);
    let x = clamp_centre(centre.x, size.w, area.loc.x, area.loc.x + area.size.w);
    let y = clamp_centre(
        centre.y,
        size.h,
        area.loc.y + LONE_INSET,
        area.loc.y + area.size.h - LONE_INSET,
    );
    slots.clear();
    slots.push(Rectangle::new(
        Point::from((x, y)) - (size / 2.0).to_point(),
        size,
    ));
    scale
}

pub fn grid(
    area: Rectangle<f64, Logical>,
    frames: &[Rectangle<f64, Logical>],
    slots: &mut Vec<Rectangle<f64, Logical>>,
) -> f64 {
    slots.clear();
    let count = frames.len();
    if count == 0 {
        return 1.0;
    }
    let columns = (count as f64).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let cell_w = (area.size.w - GRID_GAP * (columns - 1) as f64) / columns as f64;
    let cell_h = (area.size.h - GRID_GAP * (rows - 1) as f64) / rows as f64;
    let window_h = cell_h - CAPTION_HEIGHT;
    let scale = frames.iter().fold(1f64, |scale, frame| {
        scale
            .min(cell_w / frame.size.w)
            .min(window_h / frame.size.h)
    });
    slots.extend(frames.iter().enumerate().map(|(place, frame)| {
        let cell = count - 1 - place;
        let (row, column) = (cell / columns, cell % columns);
        let in_row = columns.min(count - row * columns);
        let row_w = in_row as f64 * cell_w + (in_row - 1) as f64 * GRID_GAP;
        let left = area.loc.x + (area.size.w - row_w) / 2.0 + column as f64 * (cell_w + GRID_GAP);
        let top = area.loc.y + row as f64 * (cell_h + GRID_GAP);
        let size = frame.size.upscale(scale);
        Rectangle::new(
            (
                left + (cell_w - size.w) / 2.0,
                top + (window_h - size.h) / 2.0,
            )
                .into(),
            size,
        )
    }));
    scale
}

pub fn preview(
    area: Rectangle<f64, Logical>,
    frame: Rectangle<f64, Logical>,
    slot: Rectangle<f64, Logical>,
) -> (Point<f64, Logical>, f64) {
    let scale = 1f64
        .min(area.size.w / frame.size.w)
        .min(area.size.h / frame.size.h);
    let size = frame.size.upscale(scale);
    let centre = centre(slot);
    let x = clamp_centre(centre.x, size.w, area.loc.x, area.loc.x + area.size.w);
    let y = clamp_centre(centre.y, size.h, area.loc.y, area.loc.y + area.size.h);
    (Point::from((x, y)) - (size / 2.0).to_point(), scale)
}

fn settle(
    area: Rectangle<f64, Logical>,
    frames: &[Rectangle<f64, Logical>],
    start: f64,
    centres: &mut Vec<Rectangle<f64, Logical>>,
    pushes: &mut Vec<(Point<f64, Logical>, u32)>,
) -> (bool, f64) {
    let right = area.loc.x + area.size.w;
    let bottom = area.loc.y + area.size.h;
    let mut scale = start;
    centres.clear();
    centres.extend(frames.iter().enumerate().map(|(place, frame)| {
        let tie = place as f64 * TIE_PER_STACKING_PLACE;
        let centre = centre(*frame);
        let x = clamp_centre(centre.x, frame.size.w * scale, area.loc.x, right);
        let y = clamp_centre(centre.y, frame.size.h * scale, area.loc.y, bottom);
        Rectangle::new((x + tie, y + tie).into(), frame.size)
    }));
    let mut settled = false;
    for _ in 0..SWEEPS {
        pushes.clear();
        pushes.resize(frames.len(), (Point::from((0.0, 0.0)), 0));
        settled = true;
        for behind in 0..frames.len() {
            for front in behind + 1..frames.len() {
                let mut dx = centres[front].loc.x - centres[behind].loc.x;
                let mut dy = centres[front].loc.y - centres[behind].loc.y;
                let reach_x = (frames[behind].size.w + frames[front].size.w) * scale / 2.0;
                let reach_y = (frames[behind].size.h + frames[front].size.h) * scale / 2.0;
                if dx.abs() >= reach_x + OVERLAP_MARGIN || dy.abs() >= reach_y + OVERLAP_MARGIN {
                    continue;
                }
                settled = false;
                if dx.abs() < SAME_CENTRE && dy.abs() < SAME_CENTRE {
                    dx = SAME_CENTRE;
                    dy = SAME_CENTRE;
                }
                let length = dx.hypot(dy);
                let (ux, uy) = (dx / length, dy / length);
                let need = needed(reach_x, dx, ux).min(needed(reach_y, dy, uy));
                let travel = STEP * need / 2.0;
                let push = Point::from((ux * travel, uy * travel));
                pushes[behind].0 -= push;
                pushes[behind].1 += 1;
                pushes[front].0 += push;
                pushes[front].1 += 1;
            }
        }
        if settled {
            break;
        }
        let mut cornered = false;
        for ((centre, frame), &(push, partners)) in centres.iter_mut().zip(frames).zip(&*pushes) {
            let moved = if partners > 0 {
                centre.loc + push.downscale(f64::from(partners))
            } else {
                centre.loc + push
            };
            let x = clamp_centre(
                moved.x,
                frame.size.w * scale,
                area.loc.x - WALL_SIDEWAYS,
                right + WALL_SIDEWAYS,
            );
            let y = clamp_centre(
                moved.y,
                frame.size.h * scale,
                area.loc.y - WALL_UP_AND_DOWN,
                bottom + WALL_UP_AND_DOWN,
            );
            cornered |= x != moved.x && y != moved.y;
            centre.loc = (x, y).into();
        }
        if cornered {
            scale *= SHRINK_PER_CORNERED_SWEEP;
        }
    }
    for (slot, frame) in centres.iter_mut().zip(frames) {
        slot.size = frame.size.upscale(scale);
        slot.loc -= (slot.size / 2.0).to_point();
    }
    (settled, scale)
}

fn centre(rectangle: Rectangle<f64, Logical>) -> Point<f64, Logical> {
    rectangle.loc + (rectangle.size / 2.0).to_point()
}

fn needed(reach: f64, distance: f64, unit: f64) -> f64 {
    (reach + TARGET_GAP - distance.abs()) / unit.abs()
}

fn clamp_centre(centre: f64, length: f64, low: f64, high: f64) -> f64 {
    let half = length / 2.0;
    if length > high - low {
        (low + high) / 2.0
    } else {
        centre.max(low + half).min(high - half)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::utils::Size;

    type Row = (&'static str, &'static [([f64; 4], [f64; 4])]);

    const MEASURED_AREA: [f64; 4] = [20.0, 80.0, 2520.0, 1261.0];
    const MACOS_TOLERANCE: f64 = 3.0;

    const MEASURED: &[Row] = &[
        (
            "tiny4",
            &[
                ([200.0, 200.0, 300.0, 200.0], [201.0, 201.0, 298.0, 198.0]),
                ([2000.0, 250.0, 300.0, 200.0], [2001.0, 251.0, 298.0, 198.0]),
                ([400.0, 1100.0, 300.0, 200.0], [401.0, 1101.0, 298.0, 198.0]),
                (
                    [1900.0, 1000.0, 300.0, 200.0],
                    [1901.0, 1001.0, 298.0, 198.0],
                ),
            ],
        ),
        (
            "n1full",
            &[([0.0, 30.0, 2560.0, 1410.0], [173.0, 102.0, 2214.0, 1218.0])],
        ),
        (
            "bh605",
            &[
                ([800.0, 500.0, 600.0, 400.0], [798.0, 501.0, 597.0, 398.0]),
                ([1405.0, 500.0, 600.0, 400.0], [1410.0, 501.0, 597.0, 398.0]),
            ],
        ),
        (
            "bh612",
            &[
                ([800.0, 500.0, 600.0, 400.0], [801.0, 501.0, 598.0, 398.0]),
                ([1412.0, 500.0, 600.0, 400.0], [1413.0, 501.0, 598.0, 398.0]),
            ],
        ),
        (
            "u_nest",
            &[
                ([900.0, 500.0, 600.0, 400.0], [744.0, 344.0, 597.0, 397.0]),
                ([1050.0, 600.0, 300.0, 200.0], [1209.0, 759.0, 297.0, 197.0]),
            ],
        ),
        (
            "z1",
            &[
                ([900.0, 500.0, 600.0, 400.0], [694.0, 294.0, 597.0, 397.0]),
                ([900.0, 500.0, 600.0, 400.0], [1108.0, 709.0, 598.0, 397.0]),
            ],
        ),
        (
            "p1",
            &[
                ([700.0, 300.0, 700.0, 500.0], [622.0, 144.0, 698.0, 497.0]),
                ([900.0, 500.0, 400.0, 300.0], [980.0, 659.0, 397.0, 297.0]),
            ],
        ),
        (
            "p2",
            &[
                ([600.0, 500.0, 500.0, 300.0], [288.0, 470.0, 497.0, 297.0]),
                ([800.0, 520.0, 500.0, 300.0], [801.0, 521.0, 498.0, 298.0]),
                ([1000.0, 540.0, 500.0, 300.0], [1315.0, 573.0, 497.0, 297.0]),
            ],
        ),
        (
            "p3",
            &[
                ([20.0, 300.0, 600.0, 400.0], [17.0, 119.0, 597.0, 397.0]),
                ([120.0, 350.0, 600.0, 400.0], [413.0, 534.0, 598.0, 397.0]),
            ],
        ),
    ];

    struct Arranged {
        frames: Vec<Rectangle<f64, Logical>>,
        windows: Vec<Rectangle<f64, Logical>>,
        slots: Vec<Rectangle<f64, Logical>>,
        scale: f64,
    }

    fn rectangle([x, y, w, h]: [f64; 4]) -> Rectangle<f64, Logical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    fn arranged(
        area: [f64; 4],
        frames: &[Rectangle<f64, Logical>],
    ) -> (Vec<Rectangle<f64, Logical>>, f64) {
        let mut slots = Vec::new();
        let scale = arrange(rectangle(area), frames, &mut slots, &mut Vec::new());
        (slots, scale)
    }

    fn measured(name: &str) -> Arranged {
        let (_, rows) = MEASURED.iter().find(|(row, _)| *row == name).unwrap();
        let frames: Vec<_> = rows.iter().map(|&(frame, _)| rectangle(frame)).collect();
        let windows = rows
            .iter()
            .map(|&(_, [x, y, w, h])| rectangle([x - 1.0, y - 1.0, w + 2.0, h + 2.0]))
            .collect();
        let (slots, scale) = arranged(MEASURED_AREA, &frames);
        Arranged {
            frames,
            windows,
            slots,
            scale,
        }
    }

    fn largest_edge_error(
        slots: &[Rectangle<f64, Logical>],
        windows: &[Rectangle<f64, Logical>],
    ) -> f64 {
        slots
            .iter()
            .zip(windows)
            .flat_map(|(slot, window)| {
                [
                    slot.loc.x - window.loc.x,
                    slot.loc.y - window.loc.y,
                    slot.loc.x + slot.size.w - window.loc.x - window.size.w,
                    slot.loc.y + slot.size.h - window.loc.y - window.size.h,
                ]
            })
            .map(f64::abs)
            .fold(0.0, f64::max)
    }

    fn moves(arranged: &Arranged) -> (Point<f64, Logical>, Point<f64, Logical>) {
        let moved = |place: usize| centre(arranged.slots[place]) - centre(arranged.frames[place]);
        (moved(0), moved(1))
    }

    fn assert_close_to_macos(name: &str) {
        let row = measured(name);
        let error = largest_edge_error(&row.slots, &row.windows);
        assert!(error <= MACOS_TOLERANCE, "{name}: {error}");
    }

    fn assert_tidy(name: &str, area: [f64; 4], frames: &[Rectangle<f64, Logical>]) {
        let (slots, scale) = arranged(area, frames);
        let area = rectangle(area);
        assert!(scale <= 1.0, "{name}: {scale}");
        for (slot, frame) in slots.iter().zip(frames) {
            assert_eq!(slot.size, frame.size.upscale(scale), "{name}");
            assert!(slot.loc.x >= area.loc.x - WALL_UP_AND_DOWN, "{name}");
            assert!(slot.loc.y >= area.loc.y - WALL_UP_AND_DOWN, "{name}");
            assert!(
                slot.loc.x + slot.size.w <= area.loc.x + area.size.w + WALL_UP_AND_DOWN,
                "{name}"
            );
            assert!(
                slot.loc.y + slot.size.h <= area.loc.y + area.size.h + WALL_UP_AND_DOWN,
                "{name}"
            );
        }
        for (place, first) in slots.iter().enumerate() {
            for second in &slots[place + 1..] {
                let gap_x = (second.loc.x - first.loc.x - first.size.w)
                    .max(first.loc.x - second.loc.x - second.size.w);
                let gap_y = (second.loc.y - first.loc.y - first.size.h)
                    .max(first.loc.y - second.loc.y - second.size.h);
                assert!(gap_x.max(gap_y) >= OVERLAP_MARGIN - 1e-9, "{name}");
            }
        }
    }

    #[test]
    fn a_window_that_fits_and_overlaps_nothing_stays_where_it_is_at_scale_one() {
        let tiny4 = measured("tiny4");
        assert_eq!(tiny4.scale, 1.0);
        assert!(largest_edge_error(&tiny4.slots, &tiny4.frames) < 1.0);
        assert!(largest_edge_error(&tiny4.slots, &tiny4.windows) < 1.0);
    }

    #[test]
    fn a_lone_window_too_tall_shrinks_about_its_centre_to_the_measured_scale() {
        let n1full = measured("n1full");
        let measured_scale = n1full.windows[0].size.w / n1full.frames[0].size.w;
        assert!((n1full.scale - measured_scale).abs() < 0.001);
        assert!((centre(n1full.slots[0]).x - centre(n1full.frames[0]).x).abs() < 1e-9);
        assert_close_to_macos("n1full");
    }

    #[test]
    fn an_overlapping_pair_is_pushed_apart_along_the_line_between_its_centres_and_both_move_the_same_distance()
     {
        let bh605 = measured("bh605");
        let (behind, front) = moves(&bh605);
        assert!(behind.x < 0.0 && front.x > 0.0);
        assert!(behind.y.abs() < 0.01);
        assert!((behind.x + front.x).abs() <= TIE_PER_STACKING_PLACE);
        let u_nest = measured("u_nest");
        let (behind, front) = moves(&u_nest);
        assert!((behind.x + front.x).abs() <= TIE_PER_STACKING_PLACE + 1e-9);
        assert!((behind.y + front.y).abs() <= TIE_PER_STACKING_PLACE + 1e-9);
        for travel in [-behind.x, -behind.y, front.x, front.y] {
            assert!((travel - 157.5).abs() <= 1.0, "{travel}");
        }
        for name in ["bh605", "bh612", "u_nest"] {
            assert_close_to_macos(name);
        }
    }

    #[test]
    fn two_windows_on_the_same_centre_split_at_forty_five_degrees_with_the_one_behind_up_and_left()
    {
        let z1 = measured("z1");
        let (behind, front) = moves(&z1);
        assert!(behind.x < 0.0 && behind.y < 0.0);
        assert!(front.x > 0.0 && front.y > 0.0);
        assert!((behind.x - behind.y).abs() < 1e-9);
        assert!((front.x - front.y).abs() < 1e-9);
        assert_close_to_macos("z1");
    }

    #[test]
    fn the_fresh_arrangements_are_within_three_points_of_macos() {
        for name in ["p1", "p2", "p3"] {
            assert_close_to_macos(name);
        }
    }

    #[test]
    fn cascades_end_with_no_overlap_inside_the_area_with_one_scale_no_larger_than_one() {
        for (count, x, y) in [(4, 400.0, 200.0), (6, 400.0, 100.0), (9, 300.0, 60.0)] {
            let frames: Vec<_> = (0..count)
                .map(|place| {
                    let step = 60.0 * f64::from(place);
                    rectangle([x + step, y + step, 1200.0, 800.0])
                })
                .collect();
            assert_tidy(&format!("macOS cascade of {count}"), MEASURED_AREA, &frames);
        }
        for (width, height) in [(1280.0, 800.0), (2560.0, 1440.0)] {
            let area = [20.0, 80.0, width - 40.0, height - 179.0];
            let first = [(width - 700.0) / 2.0, (height - 500.0) / 2.0];
            for count in [2, 4, 9, 20] {
                let frames: Vec<_> = (0..count)
                    .map(|place| {
                        let step = 29.0 * f64::from(place);
                        rectangle([first[0] + step, first[1] + step, 700.0, 500.0])
                    })
                    .collect();
                assert_tidy(
                    &format!("cascade of {count} on {width}x{height}"),
                    area,
                    &frames,
                );
            }
            let same = vec![rectangle([first[0], first[1], 700.0, 500.0]); 5];
            assert_tidy(
                &format!("five on one frame on {width}x{height}"),
                area,
                &same,
            );
        }
    }

    const APP_AREA: [f64; 4] = [20.0, 40.0, 2520.0, 1385.0];

    fn gridded(count: usize, size: [f64; 2]) -> (Vec<Rectangle<f64, Logical>>, f64) {
        let frames = vec![rectangle([300.0, 200.0, size[0], size[1]]); count];
        let mut slots = Vec::new();
        let scale = grid(rectangle(APP_AREA), &frames, &mut slots);
        (slots, scale)
    }

    #[test]
    fn four_windows_of_an_app_fill_a_two_by_two_grid_at_one_scale_never_above_one_with_the_front_one_top_left_and_their_captions_inside()
     {
        let (slots, scale) = gridded(4, [1200.0, 800.0]);
        assert!(scale < 1.0);
        assert!(
            slots
                .iter()
                .all(|slot| slot.size == Size::from((1200.0, 800.0)).upscale(scale))
        );
        let [bottom_right, bottom_left, top_right, top_left] = [0, 1, 2, 3].map(|n| slots[n].loc);
        assert!(top_left.x < top_right.x && top_left.y == top_right.y);
        assert!(top_left.x == bottom_left.x && top_left.y < bottom_left.y);
        assert_eq!(bottom_right, Point::from((top_right.x, bottom_left.y)));
        let area = rectangle(APP_AREA);
        let bottom = slots[0].loc.y + slots[0].size.h;
        assert!((bottom + CAPTION_HEIGHT - (area.loc.y + area.size.h)).abs() < 1e-9);
        assert_eq!(gridded(4, [300.0, 200.0]).1, 1.0);
    }

    #[test]
    fn five_windows_take_three_columns_and_the_last_row_is_centred() {
        let (slots, _) = gridded(5, [1200.0, 800.0]);
        let top = slots[4].loc.y;
        assert_eq!(slots.iter().filter(|slot| slot.loc.y == top).count(), 3);
        let area = rectangle(APP_AREA);
        let middle = area.loc.x + area.size.w / 2.0;
        let (left, right) = (slots[1], slots[0]);
        assert!(left.loc.y > top && left.loc.y == right.loc.y);
        let row_centre = (left.loc.x + right.loc.x + right.size.w) / 2.0;
        assert!((row_centre - middle).abs() < 1e-9);
    }

    #[test]
    fn a_preview_never_grows_past_scale_one_and_stays_inside_the_area() {
        let area = rectangle(APP_AREA);
        let inside = |(loc, scale): (Point<f64, Logical>, f64), frame: Rectangle<f64, Logical>| {
            let size = frame.size.upscale(scale);
            scale <= 1.0
                && loc.x >= area.loc.x
                && loc.y >= area.loc.y
                && loc.x + size.w <= area.loc.x + area.size.w + 1e-9
                && loc.y + size.h <= area.loc.y + area.size.h + 1e-9
        };
        let small = rectangle([0.0, 0.0, 600.0, 400.0]);
        let corner = rectangle([2400.0, 1300.0, 120.0, 80.0]);
        let preview_small = preview(area, small, corner);
        assert_eq!(preview_small.1, 1.0);
        assert!(inside(preview_small, small));
        let huge = rectangle([0.0, 0.0, 3000.0, 2000.0]);
        let preview_huge = preview(area, huge, rectangle([100.0, 100.0, 300.0, 200.0]));
        assert!(preview_huge.1 < 1.0);
        assert!(inside(preview_huge, huge));
    }
}
