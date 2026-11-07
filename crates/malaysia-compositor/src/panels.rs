use malaysia_scene::scene::WindowId;
use smithay::reexports::wayland_server::Resource;

use crate::state::State;
use crate::window_style::{self, Panel};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Entry<T, C> {
    pub id: T,
    pub client: C,
    pub panel: bool,
    pub hides: bool,
    pub shown: bool,
}

// AppKit: a panel is never the main window, so its client's last main window stays activated.
pub(crate) fn shown_active<T: Copy + PartialEq, C: PartialEq>(
    stack: &[Entry<T, C>],
    focused: Option<T>,
) -> Option<T> {
    let focused = stack.iter().find(|entry| Some(entry.id) == focused)?;
    if !focused.panel {
        return Some(focused.id);
    }
    stack
        .iter()
        .rev()
        .find(|entry| entry.shown && !entry.panel && entry.client == focused.client)
        .map_or(Some(focused.id), |main| Some(main.id))
}

pub(crate) fn activated<T: Copy + PartialEq, C: PartialEq>(
    stack: &[Entry<T, C>],
    focused: Option<T>,
) -> Vec<T> {
    let shown = shown_active(stack, focused);
    stack
        .iter()
        .map(|entry| entry.id)
        .filter(|id| Some(*id) == focused || Some(*id) == shown)
        .collect()
}

pub(crate) fn away<T: Copy + PartialEq, C: PartialEq>(
    stack: &[Entry<T, C>],
    focused: Option<T>,
) -> Vec<T> {
    let active = stack
        .iter()
        .find(|entry| Some(entry.id) == focused)
        .map(|entry| &entry.client);
    stack
        .iter()
        .filter(|entry| entry.hides && active != Some(&entry.client))
        .map(|entry| entry.id)
        .collect()
}

impl State {
    pub(crate) fn panel_flags(&self, id: WindowId) -> Option<Panel> {
        window_style::panel(self.windows.surface_of(id)?)
    }

    fn panel_stack(
        &self,
    ) -> Vec<Entry<WindowId, Option<smithay::reexports::wayland_server::backend::ClientId>>> {
        let shown: Vec<WindowId> = self
            .windows
            .iter_visible(&self.minimized)
            .map(|window| window.id())
            .collect();
        self.windows
            .iter()
            .map(|window| {
                let id = window.id();
                let flags = self.panel_flags(id);
                Entry {
                    id,
                    client: self
                        .windows
                        .surface_of(id)
                        .and_then(|surface| surface.client())
                        .map(|client| client.id()),
                    panel: flags.is_some(),
                    hides: flags.is_some_and(|flags| flags.contains(Panel::HidesOnDeactivate)),
                    shown: shown.contains(&id),
                }
            })
            .collect()
    }

    pub(crate) fn shown_active(&self) -> Option<WindowId> {
        self.shown_active_for(self.focused_window())
    }

    pub(crate) fn shown_active_for(&self, focused: Option<WindowId>) -> Option<WindowId> {
        shown_active(&self.panel_stack(), focused)
    }

    pub(crate) fn takes_focus_on_press(&self, id: WindowId) -> bool {
        !self
            .panel_flags(id)
            .is_some_and(|flags| flags.contains(Panel::KeyOnlyIfNeeded))
    }

    pub(crate) fn focus_fallback(&self, leaving: Option<WindowId>) -> Option<WindowId> {
        let candidates: Vec<WindowId> = self
            .windows
            .iter_visible(&self.minimized)
            .rev()
            .map(|window| window.id())
            .filter(|id| Some(*id) != leaving)
            .collect();
        candidates
            .iter()
            .copied()
            .find(|id| self.panel_flags(*id).is_none())
            .or_else(|| candidates.first().copied())
    }

    pub(crate) fn sync_panels(&mut self) {
        let stack = self.panel_stack();
        let focused = self.focused_window();
        let hidden = away(&stack, focused);
        let mut changed = false;
        for entry in &stack {
            changed |= self
                .windows
                .set_deactivated(entry.id, hidden.contains(&entry.id));
        }
        let stack = self.panel_stack();
        let on = activated(&stack, focused);
        for entry in &stack {
            self.windows.set_activated(entry.id, on.contains(&entry.id));
        }
        if changed {
            self.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u32, client: u8, panel: bool) -> Entry<u32, u8> {
        Entry {
            id,
            client,
            panel,
            hides: panel,
            shown: true,
        }
    }

    #[test]
    fn a_focused_panel_keeps_its_clients_topmost_window_activated() {
        let stack = [
            entry(1, 1, false),
            entry(2, 1, false),
            entry(3, 2, false),
            entry(4, 1, true),
        ];
        assert_eq!(shown_active(&stack, Some(4)), Some(2));
        assert_eq!(activated(&stack, Some(4)), [2, 4]);
        assert_eq!(activated(&stack, Some(3)), [3]);
    }

    #[test]
    fn a_panel_without_a_main_window_is_the_active_one() {
        let stack = [entry(1, 2, false), entry(2, 1, true)];
        assert_eq!(shown_active(&stack, Some(2)), Some(2));
    }

    #[test]
    fn panels_hide_while_another_client_or_nobody_is_active() {
        let stack = [entry(1, 1, false), entry(2, 1, true), entry(3, 2, false)];
        assert_eq!(away(&stack, Some(1)), Vec::<u32>::new());
        assert_eq!(away(&stack, Some(2)), Vec::<u32>::new());
        assert_eq!(away(&stack, Some(3)), [2]);
        assert_eq!(away(&stack, None), [2]);
    }
}
