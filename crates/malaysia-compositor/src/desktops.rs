use std::time::Duration;

use smithay::backend::input::InputTime;
use smithay::desktop::{PopupManager, Space, Window};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::utils::{IsAlive, SERIAL_COUNTER};

use crate::state::State;

#[derive(Debug, PartialEq)]
pub enum Direction {
    Left,
    Right,
}

pub struct Slide {
    from: f64,
    velocity: f64,
    started: Duration,
}

const OMEGA: f64 = ctdesign::DESKTOP_SWITCH_OMEGA as f64;
const DAMPING: f64 = ctdesign::DESKTOP_SWITCH_DAMPING as f64;

fn spring(offset: f64, velocity: f64, seconds: f64) -> (f64, f64) {
    let spread = (DAMPING * DAMPING - 1.0).sqrt();
    let slow_rate = OMEGA * (-DAMPING + spread);
    let fast_rate = OMEGA * (-DAMPING - spread);
    let slow = (velocity - fast_rate * offset) / (slow_rate - fast_rate);
    let fast = offset - slow;
    let slow_decay = (slow_rate * seconds).exp();
    let fast_decay = (fast_rate * seconds).exp();
    (
        slow * slow_decay + fast * fast_decay,
        slow * slow_rate * slow_decay + fast * fast_rate * fast_decay,
    )
}

fn rests(offset: f64, velocity: f64, stride: f64) -> bool {
    offset.abs().max(velocity.abs() / OMEGA) * stride < 0.5
}

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
    if toplevel.is_initial_configure_sent() {
        toplevel.send_pending_configure();
    }
    window.send_frame(output, time, Some(Duration::ZERO), |_, _| None);
}

fn switch_target(
    direction: Direction,
    current: usize,
    last: usize,
    last_has_windows: bool,
) -> Option<usize> {
    match direction {
        Direction::Left => current.checked_sub(1),
        Direction::Right if current < last => Some(current + 1),
        Direction::Right => last_has_windows.then_some(last + 1),
    }
}

impl State {
    fn strip(&self, now: Duration) -> (f64, f64) {
        let target = self.current_desktop as f64;
        match &self.slide {
            Some(slide) => {
                let (offset, velocity) = spring(
                    slide.from - target,
                    slide.velocity,
                    now.saturating_sub(slide.started).as_secs_f64(),
                );
                (target + offset, velocity)
            }
            None => (target, 0.0),
        }
    }

    pub fn advance_slide(&mut self, now: Duration) {
        if self.slide.is_none() {
            return;
        }
        let (position, velocity) = self.strip(now);
        let width = self.output.current_mode().map_or(0, |mode| mode.size.w);
        let scale = self.output.current_scale().fractional_scale();
        let target = self.current_desktop as f64;
        if !rests(position - target, velocity, f64::from(stride(width, scale))) {
            self.strip_position = position;
            self.needs_redraw = true;
            return;
        }
        self.slide = None;
        self.strip_position = target;
        for (index, desktop) in self.desktops.iter_mut().enumerate() {
            if index != self.current_desktop && desktop.outputs().next().is_some() {
                for window in desktop.elements() {
                    suspend(window, &self.output, now);
                }
                desktop.unmap_output(&self.output);
            }
        }
    }

    pub fn switch_desktop(&mut self, direction: Direction, time: InputTime) {
        let last = self.desktops.len() - 1;
        let last_has_windows = self.desktops[last].elements().next().is_some();
        let Some(target) = switch_target(direction, self.current_desktop, last, last_has_windows)
        else {
            return;
        };
        let now = self.start_time.elapsed();
        let (from, velocity) = self.strip(now);
        self.slide = Some(Slide {
            from,
            velocity,
            started: now,
        });
        let pointer = self.seat.get_pointer().unwrap();
        pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        let serial = SERIAL_COUNTER.next_serial();
        for window in self.desktops[self.current_desktop].elements() {
            let toplevel = window.toplevel().unwrap();
            for (popup, _) in PopupManager::popups_for_surface(toplevel.wl_surface()) {
                let _ = PopupManager::dismiss_popup(toplevel.wl_surface(), &popup);
            }
            window.set_activated(false);
            if toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure();
            }
        }
        if target == self.desktops.len() {
            self.desktops.push(Space::default());
        }
        self.desktops[target].map_output(&self.output, (0, 0));
        for window in self.desktops[target].elements() {
            window.toplevel().unwrap().with_pending_state(|state| {
                state.states.unset(ToplevelState::Suspended);
            });
        }
        self.current_desktop = target;
        let top_window = self.desktops[target]
            .elements()
            .rev()
            .find(|window| window.alive())
            .cloned();
        match top_window {
            Some(window) => self.focus_window(&window, serial),
            None => self
                .seat
                .get_keyboard()
                .unwrap()
                .set_focus(self, None, serial),
        }
        self.move_pointer(pointer.current_location(), time);
        while self.desktops.len() > self.current_desktop + 1
            && self.desktops[self.desktops.len() - 1]
                .elements()
                .next()
                .is_none()
        {
            self.desktops.pop();
        }
        self.needs_redraw = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_moves_one_back_and_stops_at_the_first_desktop() {
        assert_eq!(switch_target(Direction::Left, 0, 2, true), None);
        assert_eq!(switch_target(Direction::Left, 2, 2, true), Some(1));
    }

    #[test]
    fn right_moves_one_forward_and_adds_a_desktop_only_after_a_last_one_with_windows() {
        assert_eq!(switch_target(Direction::Right, 1, 1, true), Some(2));
        assert_eq!(switch_target(Direction::Right, 1, 1, false), None);
        assert_eq!(switch_target(Direction::Right, 0, 1, false), Some(1));
    }

    fn first_millisecond_at_or_below(fraction: f64) -> u32 {
        (0..2000)
            .find(|&millisecond| spring(1.0, 0.0, f64::from(millisecond) / 1000.0).0 <= fraction)
            .unwrap()
    }

    #[test]
    fn a_step_from_rest_is_half_way_near_110_ms_and_at_ninety_percent_near_325_ms() {
        assert!((103..=119).contains(&first_millisecond_at_or_below(0.5)));
        assert!((319..=331).contains(&first_millisecond_at_or_below(0.1)));
    }

    #[test]
    fn a_step_from_rest_never_passes_its_target() {
        assert!(
            (0..3000).all(|millisecond| spring(1.0, 0.0, f64::from(millisecond) / 1000.0).0 >= 0.0)
        );
    }

    #[test]
    fn a_spring_restarted_from_its_own_state_follows_the_same_curve() {
        let (offset, velocity) = spring(1.0, 0.0, 0.1);
        let (restarted, restarted_velocity) = spring(offset, velocity, 0.2);
        let (continued, continued_velocity) = spring(1.0, 0.0, 0.3);
        assert!((restarted - continued).abs() < 1e-12);
        assert!((restarted_velocity - continued_velocity).abs() < 1e-12);
    }

    #[test]
    fn the_strip_does_not_rest_while_crossing_its_target_at_speed() {
        let (_, velocity) = spring(1.0, 0.0, 0.1);
        assert!(!rests(0.0, velocity, 1344.0));
        assert!(rests(0.0, 0.0, 1344.0));
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
