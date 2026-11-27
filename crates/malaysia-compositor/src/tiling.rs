use std::cell::Cell;

use smithay::desktop::Window;
use smithay::utils::{FrameExtents, Logical, Point, Rectangle, Serial, Size};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::SurfaceCachedState;

use crate::placement::location;
use crate::state::State;
use crate::window_grab::acked;
use crate::workspaces::full_screen::is_full_screen;

const MARGIN: i32 = ctdesign::WINDOW_TILE_MARGIN;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tile {
    Fill,
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

type SavedFrame = Cell<Option<Rectangle<i32, Logical>>>;
type PendingMove = Cell<Option<(Serial, Point<i32, Logical>)>>;

fn tiled_frame(
    free: Rectangle<i32, Logical>,
    size: Size<i32, Logical>,
    tile: Tile,
) -> Rectangle<i32, Logical> {
    let margin = Size::from((MARGIN, MARGIN));
    let area = free - FrameExtents::new(MARGIN, MARGIN, MARGIN, MARGIN);
    let first = (area.size - margin) / 2;
    let second = area.size - first - margin;
    let second_loc = area.loc + first.to_point() + margin.to_point();
    match tile {
        Tile::Fill => area,
        Tile::Center => Rectangle::new(location(free, size, Some(free), None), size),
        Tile::Left => Rectangle::new(area.loc, (first.w, area.size.h).into()),
        Tile::Right => Rectangle::new(
            (second_loc.x, area.loc.y).into(),
            (second.w, area.size.h).into(),
        ),
        Tile::Top => Rectangle::new(area.loc, (area.size.w, first.h).into()),
        Tile::Bottom => Rectangle::new(
            (area.loc.x, second_loc.y).into(),
            (area.size.w, second.h).into(),
        ),
    }
}

fn within_limits(
    size: Size<i32, Logical>,
    min: Size<i32, Logical>,
    max: Size<i32, Logical>,
) -> bool {
    let within = |length: i32, min: i32, max: i32| length >= min && (max == 0 || length <= max);
    within(size.w, min.w, max.w) && within(size.h, min.h, max.h)
}

pub fn size_limits(window: &Window) -> (Size<i32, Logical>, Size<i32, Logical>) {
    with_states(window.toplevel().unwrap().wl_surface(), |states| {
        let mut guard = states.cached_state.get::<SurfaceCachedState>();
        let cached = guard.current();
        (cached.min_size, cached.max_size)
    })
}

pub fn fits(window: &Window, size: Size<i32, Logical>) -> bool {
    let (min, max) = size_limits(window);
    within_limits(size, min, max)
}

impl State {
    pub fn tile(&mut self, tile: Tile) {
        let Some((window, frame)) = self.tiling_target() else {
            return;
        };
        let Some(free) = self.desktops[self.current_desktop].output_geometry(&self.output) else {
            return;
        };
        let target = tiled_frame(free, frame.size, tile);
        if !fits(&window, target.size) {
            return;
        }
        if tile != Tile::Center {
            let saved = window.user_data().get_or_insert(SavedFrame::default);
            saved.set(saved.get().or(Some(frame)));
        }
        self.move_resize(&window, target);
    }

    pub fn restore_tiled(&mut self) {
        let Some((window, _)) = self.tiling_target() else {
            return;
        };
        let Some(saved) = window.user_data().get::<SavedFrame>().and_then(Cell::take) else {
            return;
        };
        self.move_resize(&window, saved);
    }

    fn tiling_target(&self) -> Option<(Window, Rectangle<i32, Logical>)> {
        if self.seat.get_pointer().unwrap().is_grabbed() {
            return None;
        }
        let window = self.top_window().filter(|window| !is_full_screen(window))?;
        let frame = self.desktops[self.current_desktop]
            .element_geometry(&window)
            .filter(|frame| !frame.size.is_empty())?;
        Some((window, frame))
    }

    fn move_resize(&mut self, window: &Window, target: Rectangle<i32, Logical>) {
        let pending = window.user_data().get_or_insert(PendingMove::default);
        let toplevel = window.toplevel().unwrap();
        let in_flight = pending.get().map(|(serial, _)| serial);
        if in_flight.is_some() || target.size != window.geometry().size {
            toplevel.with_pending_state(|state| state.size = Some(target.size));
            if let Some(serial) = toplevel.send_pending_configure().or(in_flight) {
                pending.set(Some((serial, target.loc)));
                return;
            }
            toplevel.with_pending_state(|state| state.size = None);
        }
        self.desktops[self.current_desktop].relocate_element(window, target.loc);
        self.needs_redraw = true;
    }

    pub fn finish_move_resize(&mut self, window: &Window) {
        let Some(pending) = window.user_data().get::<PendingMove>() else {
            return;
        };
        let Some((serial, location)) = pending.get() else {
            return;
        };
        let toplevel = window.toplevel().unwrap();
        if !acked(toplevel, serial) {
            return;
        }
        for desktop in &mut self.desktops {
            desktop.relocate_element(window, location);
        }
        pending.set(None);
        toplevel.with_pending_state(|state| state.size = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display() -> Rectangle<i32, Logical> {
        Rectangle::new((0, 0).into(), (1281, 801).into())
    }

    fn window() -> Size<i32, Logical> {
        (500, 300).into()
    }

    #[test]
    fn the_halves_share_an_odd_length_without_gap_or_overlap() {
        let free = display();
        let left = tiled_frame(free, window(), Tile::Left);
        let right = tiled_frame(free, window(), Tile::Right);
        let top = tiled_frame(free, window(), Tile::Top);
        let bottom = tiled_frame(free, window(), Tile::Bottom);
        assert_eq!(left.loc, (MARGIN, MARGIN).into());
        assert_eq!(left.loc.x + left.size.w + MARGIN, right.loc.x);
        assert_eq!(right.loc.x + right.size.w, free.size.w - MARGIN);
        assert_eq!(right.size.w - left.size.w, (free.size.w - 3 * MARGIN) % 2);
        assert_eq!(top.loc, (MARGIN, MARGIN).into());
        assert_eq!(top.loc.y + top.size.h + MARGIN, bottom.loc.y);
        assert_eq!(bottom.loc.y + bottom.size.h, free.size.h - MARGIN);
        assert_eq!(bottom.size.h - top.size.h, (free.size.h - 3 * MARGIN) % 2);
    }

    #[test]
    fn a_target_below_the_minimum_or_above_a_set_maximum_is_refused_and_zero_means_no_limit() {
        let none = Size::from((0, 0));
        assert!(within_limits(window(), none, none));
        assert!(!within_limits(window(), (501, 0).into(), none));
        assert!(!within_limits(window(), none, (0, 299).into()));
        assert!(within_limits(
            window(),
            (500, 300).into(),
            (500, 300).into()
        ));
    }
}
