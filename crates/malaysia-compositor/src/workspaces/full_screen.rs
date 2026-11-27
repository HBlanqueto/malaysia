use std::cell::Cell;

use smithay::backend::input::InputTime;
use smithay::desktop::{Space, Window};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::utils::{Logical, Point, Rectangle};
use smithay::wayland::shell::xdg::ToplevelSurface;

use crate::placement::placed;
use crate::state::State;
use crate::workspaces::desktops::{MAX_DESKTOPS, move_window};

#[derive(Default)]
struct FullScreen {
    entered: Cell<bool>,
    frame: Cell<Option<Rectangle<i32, Logical>>>,
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

pub fn full_screen_window(entry: &Space<Window>) -> Option<&Window> {
    entry.elements().find(|window| is_full_screen(window))
}

pub fn nearest_desktop(
    index: usize,
    count: usize,
    is_desktop: impl Fn(usize) -> bool,
) -> Option<usize> {
    (0..index)
        .rev()
        .chain(index + 1..count)
        .find(|&other| is_desktop(other))
}

pub fn desktop_number(index: usize, is_desktop: impl Fn(usize) -> bool) -> usize {
    (0..index).filter(|&other| is_desktop(other)).count() + 1
}

fn committed_full_screen(toplevel: &ToplevelSurface) -> bool {
    toplevel.with_committed_state(|state| {
        state.is_some_and(|state| state.states.contains(ToplevelState::Fullscreen))
    })
}

impl State {
    pub fn is_desktop(&self, index: usize) -> bool {
        full_screen_window(&self.desktops[index]).is_none()
    }

    pub fn heir(&self, index: usize) -> Option<usize> {
        nearest_desktop(index, self.desktops.len(), |other| self.is_desktop(other))
    }

    pub fn request_full_screen(&mut self, toplevel: &ToplevelSurface) {
        if let Some(window) = self.window_for_surface(toplevel.wl_surface())
            && !is_full_screen(window)
            && self.desktops.len() < MAX_DESKTOPS
            && let Some(output) = self.desktops[self.current_desktop].output_geometry(&self.output)
        {
            let frame = self
                .desktops
                .iter()
                .find_map(|entry| entry.element_geometry(window))
                .filter(|_| placed(window));
            marker(window).frame.set(frame);
            toplevel.with_pending_state(|state| {
                state.states.set(ToplevelState::Fullscreen);
                state.size = Some(output.size);
            });
        }
        if toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
    }

    pub fn request_leave_full_screen(&mut self, toplevel: &ToplevelSurface) {
        let saved = self
            .window_for_surface(toplevel.wl_surface())
            .filter(|window| is_full_screen(window))
            .and_then(|window| marker(window).frame.get());
        toplevel.with_pending_state(|state| {
            state.states.unset(ToplevelState::Fullscreen);
            state.size = saved.map(|frame| frame.size);
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
    }

    pub fn reconcile_full_screen(&mut self, window: &Window) {
        let Some(own) = self
            .desktops
            .iter()
            .position(|entry| entry.element_location(window).is_some())
        else {
            return;
        };
        match (
            committed_full_screen(window.toplevel().unwrap()),
            is_full_screen(window),
        ) {
            (true, false) => self.enter_full_screen(window, own),
            (true, true) => self.centre_full_screen(window, own),
            (false, true) => self.leave_full_screen(window, own),
            (false, false) => {}
        }
    }

    fn enter_full_screen(&mut self, window: &Window, own: usize) {
        if self.desktops.len() == MAX_DESKTOPS {
            self.request_leave_full_screen(window.toplevel().unwrap());
            return;
        }
        marker(window).entered.set(true);
        let mut entry = Space::default();
        move_window(window, &mut self.desktops[own], &mut entry);
        let index = own + 1;
        self.desktops.insert(index, entry);
        if self.current_desktop >= index {
            self.current_desktop += 1;
        }
        self.slide = None;
        self.expansion.dragged = None;
        self.expansion.strip.hovered = None;
        self.centre_full_screen(window, index);
        self.show_desktop(index, InputTime::now());
        self.rearrange_expansion();
    }

    fn centre_full_screen(&mut self, window: &Window, index: usize) {
        let Some(output) = self.desktops[self.current_desktop].output_geometry(&self.output) else {
            return;
        };
        let free = output.size - window.geometry().size;
        self.desktops[index]
            .relocate_element(window, output.loc + Point::from((free.w / 2, free.h / 2)));
    }

    fn leave_full_screen(&mut self, window: &Window, own: usize) {
        marker(window).entered.set(false);
        let toplevel = window.toplevel().unwrap();
        toplevel.with_pending_state(|state| state.size = None);
        if let Some(frame) = marker(window).frame.get() {
            self.desktops[own].relocate_element(window, frame.loc);
        }
        self.remove_entry(own);
    }

    pub fn forget_full_screen(&mut self, toplevel: &ToplevelSurface) {
        let Some((index, window)) = self.desktops.iter().enumerate().find_map(|(index, entry)| {
            full_screen_window(entry)
                .filter(|window| window.toplevel() == Some(toplevel))
                .map(|window| (index, window.clone()))
        }) else {
            return;
        };
        self.desktops[index].unmap_elem(&window);
        self.remove_entry(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: [bool; 5] = [true, false, true, false, false];

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
}
