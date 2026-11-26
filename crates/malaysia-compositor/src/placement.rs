use smithay::desktop::Window;
use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::state::State;

const STEP: i32 = ctdesign::WINDOW_CASCADE_STEP;
const CENTER_TOP: f64 = ctdesign::WINDOW_CENTER_TOP as f64;

struct Placed;

fn on_display(wanted: i32, start: i32, free: i32, length: i32) -> i32 {
    wanted.min(start + free - length).max(start)
}

fn cascaded(previous: i32, start: i32, free: i32, length: i32) -> i32 {
    let next = previous + STEP;
    if next + length > start + free {
        start
    } else {
        next
    }
}

fn location(
    free: Rectangle<i32, Logical>,
    size: Size<i32, Logical>,
    parent: Option<Rectangle<i32, Logical>>,
    previous: Option<Point<i32, Logical>>,
) -> Point<i32, Logical> {
    let (x, y) = match (parent, previous) {
        (Some(parent), _) => (
            parent.loc.x + (parent.size.w - size.w) / 2,
            parent.loc.y + (parent.size.h - size.h) / 2,
        ),
        (None, Some(previous)) => (
            cascaded(previous.x, free.loc.x, free.size.w, size.w),
            cascaded(previous.y, free.loc.y, free.size.h, size.h),
        ),
        (None, None) => (
            free.loc.x + (free.size.w - size.w) / 2,
            free.loc.y + ((free.size.h - size.h) as f64 * CENTER_TOP) as i32,
        ),
    };
    (
        on_display(x, free.loc.x, free.size.w, size.w),
        on_display(y, free.loc.y, free.size.h, size.h),
    )
        .into()
}

impl State {
    pub fn place_new_window(&mut self, window: &Window) {
        let size = window.geometry().size;
        if size.is_empty() || !window.user_data().insert_if_missing(|| Placed) {
            return;
        }
        let Some(free) = self.desktops[self.current_desktop].output_geometry(&self.output) else {
            return;
        };
        let Some(desktop) = self
            .desktops
            .iter_mut()
            .find(|desktop| desktop.element_location(window).is_some())
        else {
            return;
        };
        let parent = window.toplevel().unwrap().parent().and_then(|parent| {
            desktop
                .elements()
                .find(|other| {
                    other.user_data().get::<Placed>().is_some()
                        && *other.toplevel().unwrap().wl_surface() == parent
                })
                .and_then(|other| desktop.element_geometry(other))
        });
        let previous = desktop
            .elements()
            .rev()
            .find(|other| *other != window && other.user_data().get::<Placed>().is_some())
            .and_then(|other| desktop.element_location(other));
        desktop.relocate_element(window, location(free, size, parent, previous));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display() -> Rectangle<i32, Logical> {
        Rectangle::new((0, 0).into(), (1280, 800).into())
    }

    fn window() -> Size<i32, Logical> {
        (500, 300).into()
    }

    #[test]
    fn the_first_window_is_centered_with_a_quarter_of_the_free_height_above_it() {
        assert_eq!(location(display(), window(), None, None), (390, 125).into());
    }

    #[test]
    fn the_next_window_opens_one_cascade_step_right_and_down_from_the_one_in_front() {
        assert_eq!(
            location(display(), window(), None, Some((390, 125).into())),
            (390 + STEP, 125 + STEP).into()
        );
    }

    #[test]
    fn a_window_that_would_cross_the_right_or_bottom_edge_starts_that_axis_again() {
        assert_eq!(
            location(
                display(),
                window(),
                None,
                Some((780 - STEP + 1, 100).into())
            ),
            (0, 100 + STEP).into()
        );
        assert_eq!(
            location(
                display(),
                window(),
                None,
                Some((100, 500 - STEP + 1).into())
            ),
            (100 + STEP, 0).into()
        );
    }

    #[test]
    fn a_window_that_ends_exactly_on_the_edge_still_cascades() {
        let touching = (780 - STEP, 500 - STEP).into();
        assert_eq!(
            location(display(), window(), None, Some(touching)),
            (780, 500).into()
        );
    }

    #[test]
    fn a_cascade_from_a_window_left_of_or_above_the_display_starts_on_the_display() {
        assert_eq!(
            location(display(), window(), None, Some((-900, -900).into())),
            (0, 0).into()
        );
    }

    #[test]
    fn a_window_larger_than_the_display_keeps_its_top_left_on_it() {
        let huge: Size<i32, Logical> = (1600, 1000).into();
        let parent = Some(Rectangle::new((100, 100).into(), (200, 200).into()));
        assert_eq!(location(display(), huge, None, None), (0, 0).into());
        assert_eq!(
            location(display(), huge, None, Some((50, 50).into())),
            (0, 0).into()
        );
        assert_eq!(location(display(), huge, parent, None), (0, 0).into());
    }

    #[test]
    fn a_window_with_a_parent_is_centered_on_it_and_kept_on_the_display() {
        let parent = Rectangle::new((100, 100).into(), (700, 500).into());
        assert_eq!(
            location(display(), window(), Some(parent), None),
            (200, 200).into()
        );
        let edge = Rectangle::new((1100, 700).into(), (100, 100).into());
        assert_eq!(
            location(display(), window(), Some(edge), None),
            (780, 500).into()
        );
    }
}
