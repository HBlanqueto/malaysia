use std::cell::Cell;
use std::time::Duration;

use smithay::backend::input::InputTime;
use smithay::desktop::{Space, Window};
use smithay::input::pointer::{Focus, GrabStartData};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::utils::{IsAlive, Logical, Point, Rectangle, SERIAL_COUNTER, Serial, Size};
use smithay::wayland::shell::xdg::ToplevelSurface;

use crate::placement::placed;
use crate::spring::{Motion, advance_motion};
use crate::state::State;
use crate::tiling::{fits, size_limits};
use crate::window_grab::{Drag, WindowGrab, send_if_all_acked};
use crate::workspaces::desktops::{Direction, MAX_DESKTOPS, Slide, after_insertion, move_window};

const OMEGA: f64 = ctdesign::FULL_SCREEN_OMEGA as f64;
const DAMPING: f64 = ctdesign::FULL_SCREEN_DAMPING as f64;
const TILED: [ToplevelState; 4] = [
    ToplevelState::TiledLeft,
    ToplevelState::TiledRight,
    ToplevelState::TiledTop,
    ToplevelState::TiledBottom,
];

pub struct Growth {
    pub window: Window,
    from: Rectangle<f64, Logical>,
    motion: Option<Motion>,
    progress: f64,
}

impl Growth {
    pub fn drawn(&self, to: Rectangle<f64, Logical>) -> Rectangle<f64, Logical> {
        mixed(self.from, to, self.progress)
    }
}

fn mixed(
    from: Rectangle<f64, Logical>,
    to: Rectangle<f64, Logical>,
    progress: f64,
) -> Rectangle<f64, Logical> {
    let mix = |from: f64, to: f64| from + (to - from) * progress;
    Rectangle::new(
        (mix(from.loc.x, to.loc.x), mix(from.loc.y, to.loc.y)).into(),
        (mix(from.size.w, to.size.w), mix(from.size.h, to.size.h)).into(),
    )
}

pub fn cover(
    drawn: Rectangle<f64, Logical>,
    buffer: Size<i32, Logical>,
) -> (Rectangle<i32, Logical>, f64) {
    let buffer = buffer.to_f64();
    let factor = (drawn.size.w / buffer.w).max(drawn.size.h / buffer.h);
    (
        Rectangle::new(
            drawn.loc.to_i32_round(),
            drawn.size.downscale(factor).to_i32_round(),
        ),
        factor,
    )
}

#[derive(Default)]
struct FullScreen {
    entered: Cell<bool>,
    frame: Cell<Option<Rectangle<i32, Logical>>>,
    side: Cell<Option<Direction>>,
}

fn marker(window: &Window) -> &FullScreen {
    window.user_data().get_or_insert(FullScreen::default)
}

pub fn is_full_screen(window: &Window) -> bool {
    window
        .user_data()
        .get::<FullScreen>()
        .is_some_and(|marker| marker.entered.get())
}

fn side(window: &Window) -> Option<Direction> {
    window
        .user_data()
        .get::<FullScreen>()
        .and_then(|marker| marker.side.get())
}

pub fn split_windows(entry: &Space<Window>) -> Option<(&Window, &Window)> {
    let tile = |wanted| {
        entry
            .elements()
            .find(|window| is_full_screen(window) && side(window) == Some(wanted))
    };
    tile(Direction::Left).zip(tile(Direction::Right))
}

pub fn waiting(window: &Window) -> bool {
    side(window).is_some() && !is_full_screen(window)
}

fn waiting_partner<'a>(entry: &'a Space<Window>, window: &Window) -> Option<&'a Window> {
    entry
        .elements()
        .find(|other| *other != window && waiting(other))
}

fn send_back(toplevel: &ToplevelSurface, saved: Option<Rectangle<i32, Logical>>) {
    toplevel.with_pending_state(|state| {
        state.states.unset(ToplevelState::Fullscreen);
        state.states.unset(ToplevelState::Resizing);
        for tiled in TILED {
            state.states.unset(tiled);
        }
        state.size = saved.map(|frame| frame.size);
    });
    if toplevel.is_initial_configure_sent() {
        toplevel.send_configure();
    }
}

fn split_widths(
    width: i32,
    wanted: i32,
    left: (i32, i32),
    right: (i32, i32),
) -> Option<(i32, i32)> {
    let room = width - ctdesign::SPLIT_DIVIDER_WIDTH;
    let widest = |max: i32| if max == 0 { i32::MAX } else { max };
    let narrowest = |min: i32| min.max(ctdesign::SPLIT_MIN_TILE);
    let lowest = narrowest(left.0).max(room - widest(right.1));
    let highest = widest(left.1).min(room - narrowest(right.0));
    (lowest <= highest).then(|| {
        let left = wanted.clamp(lowest, highest);
        (left, room - left)
    })
}

fn width_limits(window: &Window) -> (i32, i32) {
    let (min, max) = size_limits(window);
    (min.w, max.w)
}

fn split_tiles<'a>(
    left_window: &'a Window,
    right_window: &'a Window,
    wanted: i32,
    output: Size<i32, Logical>,
) -> Option<[(&'a Window, Direction, i32); 2]> {
    let (left, right) = split_widths(
        output.w,
        wanted,
        width_limits(left_window),
        width_limits(right_window),
    )?;
    let tiles = [
        (left_window, Direction::Left, left),
        (right_window, Direction::Right, right),
    ];
    tiles
        .iter()
        .all(|(window, _, width)| fits(window, (*width, output.h).into()))
        .then_some(tiles)
}

pub fn divider(
    entry: &Space<Window>,
    output: Rectangle<i32, Logical>,
) -> Option<(&Window, Rectangle<i32, Logical>)> {
    let (left, right) = split_windows(entry)?;
    let left_tile = entry.element_geometry(left)?;
    let x = left_tile.loc.x + left_tile.size.w;
    let right_x = entry.element_geometry(right)?.loc.x;
    Some((
        left,
        Rectangle::new(
            (x, output.loc.y).into(),
            ((right_x - x).max(0), output.size.h).into(),
        ),
    ))
}

fn double_click(
    last: Option<(InputTime, Point<f64, Logical>)>,
    time: InputTime,
    location: Point<f64, Logical>,
) -> bool {
    let rule = ctdesign::TITLEBAR_DOUBLE_CLICK;
    last.is_some_and(|(last_time, last_location)| {
        let moved = location - last_location;
        time.millis().wrapping_sub(last_time.millis()) <= rule.interval_ms
            && moved.x.hypot(moved.y) <= rule.slop
    })
}

pub fn full_screen_window(entry: &Space<Window>) -> Option<&Window> {
    entry.elements().find(|window| is_full_screen(window))
}

fn nearest_desktop(
    index: usize,
    count: usize,
    is_desktop: impl Fn(usize) -> bool,
) -> Option<usize> {
    (0..index)
        .rev()
        .chain(index + 1..count)
        .find(|&other| is_desktop(other))
}

fn new_desktop_slot(count: usize, is_desktop: impl Fn(usize) -> bool) -> usize {
    (0..count)
        .rfind(|&index| is_desktop(index))
        .map_or(0, |last| last + 1)
}

pub fn desktop_number(index: usize, is_desktop: impl Fn(usize) -> bool) -> usize {
    (0..index).filter(|&other| is_desktop(other)).count() + 1
}

pub fn nth_desktop(
    number: usize,
    count: usize,
    is_desktop: impl Fn(usize) -> bool,
) -> Option<usize> {
    (0..count)
        .filter(|&index| is_desktop(index))
        .nth(number.checked_sub(1)?)
}

fn abandon_split(window: &Window) {
    marker(window).side.set(None);
    send_back(window.toplevel().unwrap(), marker(window).frame.get());
}

fn tile(window: &Window, side: Direction, width: i32, height: i32) {
    marker(window).side.set(Some(side));
    let toplevel = window.toplevel().unwrap();
    toplevel.with_pending_state(|state| {
        state.states.unset(ToplevelState::Fullscreen);
        for tiled in TILED {
            state.states.set(tiled);
        }
        state.size = Some((width, height).into());
    });
    if toplevel.is_initial_configure_sent() {
        toplevel.send_configure();
    }
}

fn drop_target(entry: &Space<Window>) -> Option<&Window> {
    full_screen_window(entry)
        .filter(|window| side(window).is_some() && split_windows(entry).is_none())
}

fn committed(toplevel: &ToplevelSurface, wanted: ToplevelState) -> bool {
    toplevel.with_committed_state(|state| state.is_some_and(|state| state.states.contains(wanted)))
}

impl State {
    pub fn is_desktop(&self, index: usize) -> bool {
        full_screen_window(&self.desktops[index]).is_none()
    }

    pub fn heir(&self, index: usize) -> Option<usize> {
        nearest_desktop(index, self.desktops.len(), |other| self.is_desktop(other))
    }

    pub fn new_desktop_index(&self) -> Option<usize> {
        let count = self.desktops.len();
        (count < MAX_DESKTOPS).then(|| new_desktop_slot(count, |index| self.is_desktop(index)))
    }

    pub fn request_full_screen(&mut self, toplevel: &ToplevelSurface) {
        if let Some(window) = self
            .window_for_surface(toplevel.wl_surface())
            .filter(|window| !is_full_screen(window))
            .cloned()
            && self.desktops.len() < MAX_DESKTOPS
            && let Some(output) = self.output_geometry()
        {
            if waiting(&window) {
                self.cancel_splits();
            } else {
                self.remember_frame(&window);
            }
            toplevel.with_pending_state(|state| {
                state.states.set(ToplevelState::Fullscreen);
                state.size = Some(output.size);
            });
        }
        if toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
    }

    fn remember_frame(&self, window: &Window) {
        let frame = self
            .desktops
            .iter()
            .find_map(|entry| entry.element_geometry(window))
            .filter(|_| placed(window));
        marker(window).frame.set(frame);
    }

    fn cancel_splits(&self) -> bool {
        let mut cancelled = false;
        for window in self
            .desktops
            .iter()
            .flat_map(Space::elements)
            .filter(|window| waiting(window))
        {
            abandon_split(window);
            cancelled = true;
        }
        for target in self.desktops.iter().filter_map(drop_target) {
            self.promote(target);
            cancelled = true;
        }
        cancelled
    }

    pub fn request_split(&mut self, side: Direction) {
        if self.cancel_splits() {
            return;
        }
        let current = &self.desktops[self.current_desktop];
        let Some(output) = current.output_geometry(&self.output) else {
            return;
        };
        let mut live = current
            .elements()
            .rev()
            .filter(|window| window.alive() && window.toplevel().unwrap().parent().is_none());
        let (Some(front), Some(other)) = (live.next().cloned(), live.next().cloned()) else {
            return;
        };
        let (left_window, right_window) = side.left_right(front, other);
        let halves = (output.size.w - ctdesign::SPLIT_DIVIDER_WIDTH) / 2;
        let Some(tiles) =
            split_tiles(&left_window, &right_window, halves, output.size).filter(|_| {
                self.is_desktop(self.current_desktop) && self.desktops.len() < MAX_DESKTOPS
            })
        else {
            return;
        };
        for (window, side, width) in tiles {
            self.remember_frame(window);
            tile(window, side, width, output.size.h);
        }
    }

    pub fn joined_tiles<'a>(
        &'a self,
        window: &'a Window,
        entry: usize,
        side: Direction,
    ) -> Option<[(&'a Window, Direction, i32); 2]> {
        let space = &self.desktops[entry];
        let output = self.output_geometry()?;
        let full = full_screen_window(space).filter(|_| {
            space.elements().count() == 1 && window.toplevel().unwrap().parent().is_none()
        })?;
        let width = window.geometry().size.w;
        let (left_window, right_window) = side.left_right(window, full);
        let (wanted, _) =
            side.left_right(width, output.size.w - ctdesign::SPLIT_DIVIDER_WIDTH - width);
        split_tiles(left_window, right_window, wanted, output.size)
    }

    pub fn split_with(&self, window: &Window, entry: usize, side: Direction) {
        let (Some(output), Some(tiles)) = (
            self.output_geometry(),
            self.joined_tiles(window, entry, side),
        ) else {
            return;
        };
        self.cancel_splits();
        self.remember_frame(window);
        for (window, side, width) in tiles {
            tile(window, side, width, output.size.h);
        }
    }

    pub fn press_divider(&mut self, button: u32, time: InputTime, serial: Serial) {
        let pointer = self.seat.get_pointer().unwrap();
        let location = pointer.current_location();
        let current = &self.desktops[self.current_desktop];
        let Some(output) = current.output_geometry(&self.output) else {
            return;
        };
        let Some((left, _)) =
            divider(current, output).filter(|(_, area)| area.to_f64().contains(location))
        else {
            return;
        };
        let left = left.clone();
        if double_click(self.divider_press.take(), time, location) {
            self.resize_split(
                &left,
                (output.size.w - ctdesign::SPLIT_DIVIDER_WIDTH) / 2,
                false,
            );
            return;
        }
        self.divider_press = Some((time, location));
        let initial_width = left
            .toplevel()
            .unwrap()
            .with_pending_state(|state| state.size)
            .map_or(left.geometry().size.w, |size| size.w);
        let grab = WindowGrab {
            start_data: GrabStartData {
                focus: None,
                button,
                location,
            },
            window: left,
            drag: Drag::Divider {
                initial_width,
                width: initial_width,
            },
        };
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    pub fn resize_split(&self, left: &Window, wanted: i32, resizing: bool) {
        let Some(output) = self.output_geometry() else {
            return;
        };
        let Some((left, right)) = self
            .desktops
            .iter()
            .filter_map(split_windows)
            .find(|(tile, _)| *tile == left)
        else {
            return;
        };
        let Some((left_width, right_width)) = split_widths(
            output.size.w,
            wanted,
            width_limits(left),
            width_limits(right),
        ) else {
            return;
        };
        for (window, width) in [(left, left_width), (right, right_width)] {
            let toplevel = window.toplevel().unwrap();
            toplevel.with_pending_state(|state| {
                if resizing {
                    state.states.set(ToplevelState::Resizing);
                } else {
                    state.states.unset(ToplevelState::Resizing);
                }
                state.size = Some((width, output.size.h).into());
            });
            send_if_all_acked(toplevel);
        }
    }

    pub fn toggle_full_screen(&mut self) {
        let Some(window) = self.top_window() else {
            return;
        };
        let toplevel = window.toplevel().unwrap();
        if is_full_screen(&window) {
            self.request_leave_full_screen(toplevel);
        } else {
            self.request_full_screen(toplevel);
        }
    }

    pub fn request_leave_full_screen(&mut self, toplevel: &ToplevelSurface) {
        let window = self.window_for_surface(toplevel.wl_surface());
        if window.is_some_and(waiting) {
            self.cancel_splits();
            return;
        }
        let saved = window
            .filter(|window| is_full_screen(window))
            .and_then(|window| marker(window).frame.get());
        send_back(toplevel, saved);
    }

    pub fn reconcile_full_screen(&mut self, window: &Window) {
        let Some(own) = self
            .desktops
            .iter()
            .position(|entry| entry.element_location(window).is_some())
        else {
            return;
        };
        let toplevel = window.toplevel().unwrap();
        let full_screen = committed(toplevel, ToplevelState::Fullscreen);
        let tiled = committed(toplevel, ToplevelState::TiledLeft);
        match (is_full_screen(window), side(window)) {
            (false, None) if full_screen => self.enter_full_screen(window, own),
            (false, Some(_)) if tiled => self.join_split(window, own),
            (true, _) if full_screen || tiled => {
                self.place_in_space(window, own);
                send_if_all_acked(toplevel);
                if tiled && drop_target(&self.desktops[own]).is_some() {
                    self.join_drop(own);
                }
            }
            (true, _) => self.leave_full_screen(window, own),
            _ => {}
        }
    }

    fn growth_from(&self, window: &Window) -> Option<Rectangle<f64, Logical>> {
        self.growth
            .as_ref()
            .filter(|growth| growth.window == *window)
            .zip(
                self.desktops
                    .iter()
                    .find_map(|entry| entry.element_geometry(window)),
            )
            .map(|(growth, geometry)| growth.drawn(geometry.to_f64()))
    }

    fn grow(&mut self, window: &Window, from: Option<Rectangle<f64, Logical>>) {
        if let Some(from) = from
            && self.expansion.progress <= 0.0
            && self.desktops[self.current_desktop]
                .element_location(window)
                .is_some()
        {
            self.growth = Some(Growth {
                window: window.clone(),
                from,
                motion: Some(Motion::new(0.0, 0.0, self.start_time.elapsed())),
                progress: 0.0,
            });
        }
    }

    pub fn advance_growth(&mut self, now: Duration) {
        let Some(growth) = &mut self.growth else {
            return;
        };
        let longest_side = f64::from(
            self.output
                .current_mode()
                .map_or(0, |mode| mode.size.w.max(mode.size.h)),
        );
        if let Some(progress) =
            advance_motion(&mut growth.motion, OMEGA, DAMPING, 1.0, now, longest_side)
        {
            growth.progress = progress;
            self.needs_redraw |= growth.motion.is_some();
        }
        if growth.motion.is_none() && self.slide.is_none() {
            self.growth = None;
        }
    }

    fn enter_full_screen(&mut self, window: &Window, own: usize) {
        if self.desktops.len() == MAX_DESKTOPS {
            self.request_leave_full_screen(window.toplevel().unwrap());
            return;
        }
        let from = self.growth_from(window).or_else(|| {
            marker(window)
                .frame
                .get()
                .filter(|_| own == self.current_desktop)
                .map(Rectangle::to_f64)
        });
        self.open_space(std::slice::from_ref(window), own);
        self.grow(window, from);
    }

    fn join_split(&mut self, window: &Window, own: usize) {
        let Some(partner) = waiting_partner(&self.desktops[own], window).cloned() else {
            match self
                .desktops
                .iter()
                .position(|entry| drop_target(entry).is_some())
            {
                Some(target) => self.join_drop(target),
                None => abandon_split(window),
            }
            return;
        };
        if !committed(partner.toplevel().unwrap(), ToplevelState::TiledLeft) {
            return;
        }
        if self.desktops.len() == MAX_DESKTOPS {
            abandon_split(window);
            abandon_split(&partner);
            return;
        }
        let entry = &self.desktops[own];
        let depth = |one: &Window| entry.elements().position(|other| other == one);
        let pair = if depth(window) < depth(&partner) {
            [window.clone(), partner]
        } else {
            [partner, window.clone()]
        };
        self.open_space(&pair, own);
    }

    fn join_drop(&mut self, target: usize) {
        let Some((own, window)) = self.desktops.iter().enumerate().find_map(|(own, entry)| {
            entry
                .elements()
                .find(|window| waiting(window))
                .map(|window| (own, window.clone()))
        }) else {
            return;
        };
        let tiled =
            |window: &Window| committed(window.toplevel().unwrap(), ToplevelState::TiledLeft);
        if !tiled(&window) || !drop_target(&self.desktops[target]).is_some_and(tiled) {
            return;
        }
        marker(&window).entered.set(true);
        let [from, to] = self.desktops.get_disjoint_mut([own, target]).unwrap();
        move_window(&window, from, to);
        self.place_in_space(&window, target);
        self.rearrange_expansion();
    }

    fn open_space(&mut self, windows: &[Window], own: usize) {
        let mut entry = Space::default();
        for window in windows {
            marker(window).entered.set(true);
            move_window(window, &mut self.desktops[own], &mut entry);
        }
        let index = own + 1;
        self.desktops.insert(index, entry);
        self.reindex(|other| after_insertion(index, other));
        self.expansion.dragged = None;
        self.expansion.strip.opening = None;
        for window in windows {
            self.place_in_space(window, index);
        }
        let time = InputTime::now();
        let pointer = self.seat.get_pointer().unwrap();
        pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        self.show_desktop(index, time);
        self.rearrange_expansion();
    }

    fn place_in_space(&mut self, window: &Window, index: usize) {
        let Some(output) = self.output_geometry() else {
            return;
        };
        let free = output.size - window.geometry().size;
        let x = match side(window) {
            None => free.w / 2,
            Some(Direction::Left) => 0,
            Some(Direction::Right) => free.w,
        };
        self.desktops[index].relocate_element(window, output.loc + Point::from((x, free.h / 2)));
    }

    fn promote(&self, partner: &Window) {
        marker(partner).side.set(None);
        let Some(output) = self.output_geometry() else {
            return;
        };
        let toplevel = partner.toplevel().unwrap();
        toplevel.with_pending_state(|state| {
            state.states.set(ToplevelState::Fullscreen);
            state.states.unset(ToplevelState::Resizing);
            for tiled in TILED {
                state.states.unset(tiled);
            }
            state.size = Some(output.size);
        });
        toplevel.send_configure();
    }

    fn leave_split(&mut self, window: &Window, partner: &Window, own: usize) {
        let Some(heir) = self.heir(own) else {
            return;
        };
        self.promote(partner);
        marker(window).entered.set(false);
        marker(window).side.set(None);
        window
            .toplevel()
            .unwrap()
            .with_pending_state(|state| state.size = None);
        let location = marker(window)
            .frame
            .get()
            .map(|frame| frame.loc)
            .or_else(|| self.desktops[own].element_location(window))
            .unwrap_or_default();
        self.desktops[own].unmap_elem(window);
        self.desktops[heir].map_element(window.clone(), location, false);
        if own == self.current_desktop {
            self.show_desktop(heir, InputTime::now());
        }
        self.rearrange_expansion();
    }

    fn leave_full_screen(&mut self, window: &Window, own: usize) {
        let partner = self.desktops[own]
            .elements()
            .find(|other| *other != window && is_full_screen(other))
            .cloned();
        if let Some(partner) = partner {
            self.leave_split(window, &partner, own);
            return;
        }
        let was_current = own == self.current_desktop;
        let from = self
            .growth_from(window)
            .or_else(|| {
                self.desktops[own]
                    .output_geometry(&self.output)
                    .map(Rectangle::to_f64)
            })
            .filter(|_| was_current);
        let now = self.start_time.elapsed();
        let row = self.strip(now);
        marker(window).entered.set(false);
        let toplevel = window.toplevel().unwrap();
        toplevel.with_pending_state(|state| state.size = None);
        if let Some(frame) = marker(window).frame.get() {
            self.desktops[own].relocate_element(window, frame.loc);
        }
        if marker(window).side.take().is_some() {
            self.cancel_splits();
        }
        self.remove_entry(own);
        if was_current && self.current_desktop + 1 == own {
            self.slide = Some(Slide::new(row, now, None, Some(own)));
        }
        self.grow(window, from);
    }

    pub fn forget_full_screen(&mut self, toplevel: &ToplevelSurface) {
        self.growth
            .take_if(|growth| growth.window.toplevel() == Some(toplevel));
        let Some((index, window)) = self.desktops.iter().enumerate().find_map(|(index, entry)| {
            entry
                .elements()
                .find(|window| window.toplevel() == Some(toplevel))
                .map(|window| (index, window.clone()))
        }) else {
            return;
        };
        if !is_full_screen(&window) {
            if marker(&window).side.take().is_some() {
                self.cancel_splits();
            }
            return;
        }
        self.desktops[index].unmap_elem(&window);
        match full_screen_window(&self.desktops[index]).cloned() {
            Some(partner) => self.promote(&partner),
            None => {
                self.remove_entry(index);
                if side(&window).is_some() {
                    self.cancel_splits();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: [bool; 5] = [true, false, true, false, false];

    #[test]
    fn a_growing_window_covers_the_rectangle_mixed_between_its_ends_keeping_its_aspect() {
        let frame = Rectangle::new((100.0, 50.0).into(), (800.0, 600.0).into());
        let display = Rectangle::from_size((1280.0, 800.0).into());
        assert_eq!(
            cover(mixed(frame, display, 0.0), (1280, 800).into()),
            (Rectangle::new((100, 50).into(), (1067, 800).into()), 0.75)
        );
        assert_eq!(
            cover(mixed(frame, display, 0.5), (1280, 800).into()),
            (Rectangle::new((50, 25).into(), (1189, 800).into()), 0.875)
        );
        assert_eq!(
            cover(mixed(frame, display, 1.0), (1280, 800).into()),
            (display.to_i32_round(), 1.0)
        );
    }

    #[test]
    fn the_tiles_share_what_the_divider_leaves_and_neither_is_narrower_than_the_minimum_or_its_window_allows_nor_wider_than_its_maximum()
     {
        let divider = ctdesign::SPLIT_DIVIDER_WIDTH;
        let minimum = ctdesign::SPLIT_MIN_TILE;
        let free = (0, 0);
        assert_eq!(split_widths(2560, 1474, free, free), Some((1474, 1074)));
        assert_eq!(
            split_widths(2560, -50, free, free),
            Some((minimum, 2560 - divider - minimum))
        );
        assert_eq!(
            split_widths(2560, 5000, free, free),
            Some((2560 - divider - minimum, minimum))
        );
        assert_eq!(split_widths(2560, 100, (300, 0), free), Some((300, 2248)));
        assert_eq!(
            split_widths(2560, 1274, free, (0, 1000)),
            Some((1548, 1000))
        );
        assert_eq!(split_widths(2560, 1274, (2000, 0), (600, 0)), None);
        assert_eq!(
            split_widths(2 * minimum + divider, 0, free, free),
            Some((minimum, minimum))
        );
        assert_eq!(split_widths(2 * minimum + divider - 1, 0, free, free), None);
    }

    #[test]
    fn the_nearest_desktop_is_looked_for_left_first_then_right_and_never_is_the_entry_itself() {
        let is_desktop = |index: usize| ROW[index];
        assert_eq!(nearest_desktop(1, 5, is_desktop), Some(0));
        assert_eq!(nearest_desktop(4, 5, is_desktop), Some(2));
        assert_eq!(nearest_desktop(0, 5, is_desktop), Some(2));
        assert_eq!(nearest_desktop(0, 1, |_| true), None);
    }

    #[test]
    fn a_desktop_is_numbered_counting_only_the_desktops_before_it() {
        let is_desktop = |index: usize| ROW[index];
        assert_eq!(desktop_number(0, is_desktop), 1);
        assert_eq!(desktop_number(2, is_desktop), 2);
    }

    #[test]
    fn a_new_desktop_goes_right_after_the_last_desktop_leaving_the_spaces_after_it_behind() {
        assert_eq!(new_desktop_slot(5, |index: usize| ROW[index]), 3);
        assert_eq!(new_desktop_slot(1, |_| true), 1);
        assert_eq!(new_desktop_slot(2, |index: usize| index == 0), 1);
    }

    #[test]
    fn the_desktop_with_a_number_is_the_entry_that_number_names_and_spaces_have_none() {
        let is_desktop = |index: usize| ROW[index];
        assert_eq!(nth_desktop(1, 5, is_desktop), Some(0));
        assert_eq!(nth_desktop(2, 5, is_desktop), Some(2));
        assert_eq!(nth_desktop(3, 5, is_desktop), None);
        assert_eq!(nth_desktop(0, 5, is_desktop), None);
        for index in [0, 2] {
            assert_eq!(
                nth_desktop(desktop_number(index, is_desktop), 5, is_desktop),
                Some(index)
            );
        }
    }
}
