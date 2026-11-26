use smithay::backend::input::InputTime;
use smithay::desktop::{PopupManager, Space};
use smithay::utils::{IsAlive, SERIAL_COUNTER};

use crate::state::State;

#[derive(Debug, PartialEq)]
pub enum Direction {
    Left,
    Right,
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
    pub fn switch_desktop(&mut self, direction: Direction, time: InputTime) {
        let last = self.desktops.len() - 1;
        let last_has_windows = self.desktops[last].elements().next().is_some();
        let Some(target) = switch_target(direction, self.current_desktop, last, last_has_windows)
        else {
            return;
        };
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
        self.desktops[self.current_desktop].unmap_output(&self.output);
        self.desktops[target].map_output(&self.output, (0, 0));
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
}
