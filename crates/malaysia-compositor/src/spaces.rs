//! Spaces in the compositor: which windows the shown space holds (ADR 0051).
//!
//! The row itself lives in [`malaysia_scene::spaces`]; this module keeps the
//! window tracker in step with it. A window on a space that is not shown is
//! taken out of the scene the way a minimized one is (`set_off_space`), so
//! drawing, hit-testing and focus all skip it with no extra checks.
//!
//! Step 1 of the plan: switching is instant. The slide, the black separator,
//! the per-desktop bar and focus mode come in later steps.

use malaysia_scene::scene::WindowId;
use malaysia_scene::spaces::{SpaceKind, SpacesError};

use crate::state::State;

impl State {
    /// Puts a newly mapped window on a space, switching to that space when it
    /// is not the shown one (a window opened from a focus space).
    pub(crate) fn space_window_mapped(&mut self, id: WindowId) {
        let parent = self.windows.parent_of(id);
        let assigned = self.spaces.assign(id, parent);
        if assigned.needs_switch
            && let Some(index) = self.spaces.index_of(assigned.space)
        {
            self.switch_space(index);
        }
    }

    /// Stops tracking a destroyed window. Its focus space, if any, goes away.
    pub(crate) fn space_window_removed(&mut self, id: WindowId) {
        let before = self.spaces.current().id;
        self.spaces.remove_window(id);
        if self.spaces.current().id != before {
            self.sync_space_visibility();
        }
    }

    /// Shows the space at `index`. Returns whether the shown space changed.
    pub(crate) fn switch_space(&mut self, index: usize) -> bool {
        if !self.spaces.switch_to(index) {
            return false;
        }
        self.sync_space_visibility();
        true
    }

    /// Shows the space to the right (Ctrl+Alt+→). The row does not wrap.
    pub(crate) fn next_space(&mut self) -> bool {
        self.switch_space(self.spaces.current_index() + 1)
    }

    /// Shows the space to the left (Ctrl+Alt+←). The row does not wrap.
    pub(crate) fn previous_space(&mut self) -> bool {
        match self.spaces.current_index() {
            0 => false,
            index => self.switch_space(index - 1),
        }
    }

    /// Appends a desktop at the right end of the row and shows it
    /// (Ctrl+Alt+N). Refused at 99 desktops.
    pub(crate) fn add_desktop(&mut self) -> bool {
        match self.spaces.add_desktop() {
            Ok(id) => {
                tracing::info!(
                    desktop = self.spaces.desktop_number(id),
                    desktops = self.spaces.desktop_count(),
                    "desktop added"
                );
                let index = self
                    .spaces
                    .index_of(id)
                    .expect("a new desktop is in the row");
                self.switch_space(index);
                true
            }
            Err(SpacesError::Full) => {
                tracing::info!("desktop not added: the row already has 99 desktops");
                false
            }
            Err(error) => {
                tracing::warn!(?error, "desktop not added");
                false
            }
        }
    }

    /// Shows the space a window lives on, if it is not the shown one. Used
    /// before focusing a window from the dock or by activation.
    pub(crate) fn show_space_of(&mut self, id: WindowId) {
        if self.spaces.is_shown(id) {
            return;
        }
        if let Some(index) = self
            .spaces
            .space_of(id)
            .and_then(|space| self.spaces.index_of(space))
        {
            self.switch_space(index);
        }
    }

    /// Hides every window off the shown space and shows the ones on it, then
    /// moves keyboard focus onto the shown space.
    fn sync_space_visibility(&mut self) {
        // Menus belong to a window or to the bar of the space being left.
        self.dismiss_all_popups();
        let ids: Vec<WindowId> = self.windows.iter().map(|window| window.id()).collect();
        for id in ids {
            self.windows.set_off_space(id, !self.spaces.is_shown(id));
        }
        let current = self.spaces.current();
        tracing::info!(
            index = self.spaces.current_index(),
            spaces = self.spaces.len(),
            desktop = self.spaces.desktop_number(current.id),
            focus = matches!(current.kind, SpaceKind::Focus { .. }),
            "space shown"
        );
        let focused = self.focused_window();
        if focused.is_none_or(|id| !self.spaces.is_shown(id)) {
            match self.windows.topmost_visible(&self.minimized) {
                Some(next) => self.focus_window(next),
                None => self.clear_keyboard_focus(),
            }
        }
        self.request_redraw();
    }
}
