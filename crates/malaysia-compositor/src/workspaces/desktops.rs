use std::time::Duration;

use smithay::backend::input::InputTime;
use smithay::desktop::{PopupManager, Space, Window};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::utils::{IsAlive, SERIAL_COUNTER, Serial};

use crate::spring::{Motion, rests, sample_motion};
use crate::state::State;
use crate::window_grab::{Drag, WindowGrab};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    pub fn left_right<T>(self, this: T, other: T) -> (T, T) {
        match self {
            Direction::Left => (this, other),
            Direction::Right => (other, this),
        }
    }
}

pub struct Slide {
    motion: Option<Motion>,
    pub position: f64,
    pub carried: Option<Window>,
    pub vacated: Option<usize>,
}

impl Slide {
    pub fn new(
        (from, velocity): (f64, f64),
        now: Duration,
        carried: Option<Window>,
        vacated: Option<usize>,
    ) -> Slide {
        Slide {
            motion: Some(Motion::new(from, velocity, now)),
            position: from,
            carried,
            vacated,
        }
    }
}

const OMEGA: f64 = ctdesign::DESKTOP_SWITCH_OMEGA as f64;
const DAMPING: f64 = ctdesign::DESKTOP_SWITCH_DAMPING as f64;
pub const MAX_DESKTOPS: usize = 16;

pub fn gap(scale: f64) -> i32 {
    (f64::from(ctdesign::DESKTOP_GAP) * scale).round() as i32
}

pub fn stride(width: i32, scale: f64) -> i32 {
    width + gap(scale)
}

pub fn gap_x(position: f64, width: i32, scale: f64) -> Option<i32> {
    let gap = gap(scale);
    let next = position.floor() as usize + 1;
    let x = desktop_x(next, position, stride(width, scale)) - gap;
    (x + gap > 0 && x < width).then_some(x)
}

pub fn desktop_x(index: usize, position: f64, stride: i32) -> i32 {
    ((index as f64 - position) * f64::from(stride)).round() as i32
}

fn suspend(window: &Window, output: &Output, time: Duration) {
    let toplevel = window.toplevel().unwrap();
    toplevel.with_pending_state(|state| {
        state.states.set(ToplevelState::Suspended);
    });
    send_changed_state(window);
    window.send_frame(output, time, Some(Duration::ZERO), |_, _| None);
}

pub fn send_changed_state(window: &Window) {
    let toplevel = window.toplevel().unwrap();
    if toplevel.is_initial_configure_sent() {
        toplevel.send_pending_configure();
    }
}

pub fn move_window(window: &Window, from: &mut Space<Window>, to: &mut Space<Window>) {
    let location = from.element_location(window).unwrap();
    from.unmap_elem(window);
    to.map_element(window.clone(), location, false);
}

fn switch_target(direction: Direction, current: usize, last: usize) -> Option<usize> {
    match direction {
        Direction::Left => current.checked_sub(1),
        Direction::Right => (current < last).then_some(current + 1),
    }
}

fn after_removal(removed: usize, index: usize) -> usize {
    index - usize::from(index > removed)
}

pub fn after_insertion(at: usize, index: usize) -> usize {
    index + usize::from(index >= at)
}

pub fn moved_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else {
        after_insertion(to, after_removal(from, index))
    }
}

fn wake(desktop: &mut Space<Window>, output: &Output) {
    desktop.map_output(output, (0, 0));
    for window in desktop.elements() {
        window.toplevel().unwrap().with_pending_state(|state| {
            state.states.unset(ToplevelState::Suspended);
        });
        send_changed_state(window);
    }
}

impl State {
    pub fn strip(&self, now: Duration) -> (f64, f64) {
        let target = self.current_desktop as f64;
        self.slide.as_ref().map_or((target, 0.0), |slide| {
            sample_motion(&slide.motion, OMEGA, DAMPING, slide.position, target, now)
        })
    }

    pub fn slide_held(&self) -> bool {
        self.slide
            .as_ref()
            .is_some_and(|slide| slide.motion.is_none())
    }

    pub fn hold_slide(&mut self, position: f64) {
        let slide = self.slide.get_or_insert(Slide {
            motion: None,
            position,
            carried: None,
            vacated: None,
        });
        slide.motion = None;
        slide.position = position;
        self.needs_redraw = true;
    }

    pub fn release_slide(&mut self, target: usize, velocity: f64, time: InputTime) {
        let now = self.start_time.elapsed();
        let from = self.strip(now).0;
        self.go_to_desktop(target, time);
        if let Some(slide) = &mut self.slide {
            slide.motion = Some(Motion::new(from, velocity, now));
        }
    }

    pub fn advance_slide(&mut self, now: Duration) {
        if self.slide.is_none() || self.slide_held() {
            return;
        }
        let (position, velocity) = self.strip(now);
        let width = self.output.current_mode().map_or(0, |mode| mode.size.w);
        let scale = self.output.current_scale().fractional_scale();
        let target = self.current_desktop as f64;
        if !rests(
            OMEGA,
            position - target,
            velocity,
            f64::from(stride(width, scale)),
        ) {
            if let Some(slide) = &mut self.slide {
                slide.position = position;
            }
            self.needs_redraw = true;
            return;
        }
        self.slide = None;
        if !self.expansion.strip.grown {
            self.suspend_hidden_desktops(now);
        }
    }

    pub fn suspend_hidden_desktops(&mut self, now: Duration) {
        for (index, desktop) in self.desktops.iter_mut().enumerate() {
            if index != self.current_desktop && desktop.outputs().next().is_some() {
                for window in desktop.elements() {
                    suspend(window, &self.output, now);
                }
                desktop.unmap_output(&self.output);
            }
        }
    }

    pub fn wake_desktops(&mut self) {
        for desktop in &mut self.desktops {
            wake(desktop, &self.output);
        }
    }

    pub fn dismiss_popups(&self) {
        for window in self.desktops[self.current_desktop].elements() {
            let toplevel = window.toplevel().unwrap();
            for (popup, _) in PopupManager::popups_for_surface(toplevel.wl_surface()) {
                let _ = PopupManager::dismiss_popup(toplevel.wl_surface(), &popup);
            }
        }
    }

    pub fn top_window(&self) -> Option<Window> {
        self.desktops[self.current_desktop]
            .elements()
            .rev()
            .find(|window| window.alive())
            .cloned()
    }

    pub fn switch_desktop(&mut self, direction: Direction, time: InputTime) {
        let last = self.desktops.len() - 1;
        if let Some(target) = switch_target(direction, self.current_desktop, last) {
            self.go_to_desktop(target, time);
        }
    }

    pub fn go_to_desktop(&mut self, target: usize, time: InputTime) {
        if target != self.current_desktop && self.expansion.dragged.is_none() {
            self.show_desktop(target, time);
            self.rearrange_expansion();
        }
    }

    pub fn add_desktop(&mut self) -> Option<usize> {
        let index = self.new_desktop_index()?;
        let mut desktop = Space::default();
        if self.expansion.strip.grown {
            desktop.map_output(&self.output, (0, 0));
        }
        self.desktops.insert(index, desktop);
        self.reindex(|other| after_insertion(index, other));
        self.expansion.strip.dock(index, self.start_time.elapsed());
        Some(index)
    }

    pub fn reindex(&mut self, index_after: impl Fn(usize) -> usize) {
        self.current_desktop = index_after(self.current_desktop);
        self.expansion.desktop = index_after(self.expansion.desktop);
        self.slide = None;
        self.expansion.strip.hovered = None;
        self.expansion.strip.moving = None;
    }

    pub fn move_entry(&mut self, from: usize, to: usize) {
        if from < to {
            self.desktops[from..=to].rotate_left(1);
        } else {
            self.desktops[to..=from].rotate_right(1);
        }
        self.reindex(|index| moved_index(index, from, to));
        self.rearrange_expansion();
    }

    pub fn remove_entry(&mut self, index: usize) {
        let Some(heir) = self.heir(index) else {
            return;
        };
        let mut removed = self.desktops.remove(index);
        let heir = after_removal(index, heir);
        loop {
            let Some(window) = removed.elements().next().cloned() else {
                break;
            };
            move_window(&window, &mut removed, &mut self.desktops[heir]);
        }
        self.reindex(|other| {
            if other == index {
                heir
            } else {
                after_removal(index, other)
            }
        });
        self.expansion
            .strip
            .remove(index, self.start_time.elapsed());
        wake(&mut self.desktops[self.current_desktop], &self.output);
        if heir == self.current_desktop && !self.expansion.open {
            self.focus_top_window(SERIAL_COUNTER.next_serial());
        }
        if !self.expansion.strip.grown {
            self.suspend_hidden_desktops(self.start_time.elapsed());
        }
        self.rearrange_expansion();
        self.needs_redraw = true;
    }

    pub fn focus_top_window(&mut self, serial: Serial) {
        match self.top_window() {
            Some(window) => self.focus_window(&window, serial),
            None => self
                .seat
                .get_keyboard()
                .unwrap()
                .set_focus(self, None, serial),
        }
    }

    pub fn show_desktop(&mut self, target: usize, time: InputTime) {
        let pointer = self.seat.get_pointer().unwrap();
        let carried = pointer
            .with_grab(|_, grab| {
                grab.downcast_ref::<WindowGrab>()
                    .filter(|grab| matches!(grab.drag, Drag::Move { .. }))
                    .map(|grab| grab.window.clone())
            })
            .flatten()
            .and_then(|window| {
                let location = self.desktops[self.current_desktop].element_location(&window)?;
                Some((window, location))
            });
        let now = self.start_time.elapsed();
        self.slide = Some(Slide::new(
            self.strip(now),
            now,
            carried.as_ref().map(|(window, _)| window.clone()),
            None,
        ));
        self.growth = None;
        match &carried {
            Some((window, _)) => self.desktops[self.current_desktop].unmap_elem(window),
            None => pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), time),
        }
        let serial = SERIAL_COUNTER.next_serial();
        self.dismiss_popups();
        for window in self.desktops[self.current_desktop].elements() {
            window.set_activated(false);
            send_changed_state(window);
        }
        self.desktops[target].map_output(&self.output, (0, 0));
        if let Some((window, location)) = carried {
            self.desktops[target].map_element(window, location, true);
        }
        for window in self.desktops[target].elements() {
            window.toplevel().unwrap().with_pending_state(|state| {
                state.states.unset(ToplevelState::Suspended);
            });
        }
        self.current_desktop = target;
        if !self.expansion.open {
            self.focus_top_window(serial);
        }
        for window in self.desktops[target].elements() {
            send_changed_state(window);
        }
        self.move_pointer(pointer.current_location(), time);
        self.needs_redraw = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spring::spring;

    #[test]
    fn left_moves_one_back_and_stops_at_the_first_desktop() {
        assert_eq!(switch_target(Direction::Left, 0, 2), None);
        assert_eq!(switch_target(Direction::Left, 2, 2), Some(1));
    }

    #[test]
    fn right_moves_one_forward_and_stops_at_the_last_desktop() {
        assert_eq!(switch_target(Direction::Right, 0, 1), Some(1));
        assert_eq!(switch_target(Direction::Right, 1, 1), None);
        assert_eq!(switch_target(Direction::Right, 0, 0), None);
    }

    #[test]
    fn removing_an_entry_moves_only_the_indexes_after_it_down_one() {
        assert_eq!(after_removal(0, 2), 1);
        assert_eq!(after_removal(3, 2), 2);
        assert_eq!(after_removal(1, 1), 1);
    }

    #[test]
    fn inserting_an_entry_moves_the_index_at_it_and_after_it_up_one() {
        assert_eq!(after_insertion(2, 1), 1);
        assert_eq!(after_insertion(2, 2), 3);
        assert_eq!(after_insertion(0, 3), 4);
    }

    #[test]
    fn moving_an_entry_shifts_only_the_indexes_between_its_ends() {
        assert_eq!(moved_index(1, 1, 3), 3);
        assert_eq!(moved_index(2, 1, 3), 1);
        assert_eq!(moved_index(3, 1, 3), 2);
        assert_eq!(moved_index(0, 1, 3), 0);
        assert_eq!(moved_index(4, 1, 3), 4);
        assert_eq!(moved_index(3, 3, 1), 1);
        assert_eq!(moved_index(1, 3, 1), 2);
        assert_eq!(moved_index(2, 3, 1), 3);
        assert_eq!(moved_index(4, 3, 1), 4);
        assert_eq!(moved_index(2, 2, 2), 2);
    }

    fn first_millisecond_at_or_below(fraction: f64) -> u32 {
        (0..2000)
            .find(|&millisecond| {
                spring(OMEGA, DAMPING, 1.0, 0.0, f64::from(millisecond) / 1000.0).0 <= fraction
            })
            .unwrap()
    }

    #[test]
    fn a_step_from_rest_is_half_way_near_110_ms_and_at_ninety_percent_near_325_ms() {
        assert!((103..=119).contains(&first_millisecond_at_or_below(0.5)));
        assert!((319..=331).contains(&first_millisecond_at_or_below(0.1)));
    }

    #[test]
    fn a_step_from_rest_never_passes_its_target() {
        assert!((0..3000).all(|millisecond| {
            spring(OMEGA, DAMPING, 1.0, 0.0, f64::from(millisecond) / 1000.0).0 >= 0.0
        }));
    }

    #[test]
    fn the_next_desktop_starts_one_gap_past_the_output_edge() {
        assert_eq!(desktop_x(1, 0.0, stride(1280, 1.0)), 1280 + 64);
        assert_eq!(desktop_x(0, 1.0, stride(2560, 2.0)), -(2560 + 128));
        assert_eq!(desktop_x(1, 1.0, stride(2560, 2.0)), 0);
    }

    #[test]
    fn the_gap_sits_right_of_the_desktop_the_strip_is_on_and_is_absent_at_rest_and_left_of_the_first()
     {
        assert_eq!(gap_x(0.5, 1280, 1.0), Some(1280 + 64 - 672 - 64));
        assert_eq!(gap_x(0.0, 1280, 1.0), None);
        assert_eq!(gap_x(1.0, 1280, 1.0), None);
        assert_eq!(gap_x(-0.03, 1280, 1.0), None);
    }
}
