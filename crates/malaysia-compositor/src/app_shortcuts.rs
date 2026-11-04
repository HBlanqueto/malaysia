//! `Super+H`, `Alt+Super+H` and `Super+Q`: the focused app's menu actions as compositor shortcuts.

use malaysia_scene::scene::WindowId;

use crate::state::State;

impl State {
    pub(crate) fn hide_focused_app(&mut self) {
        for id in self.focused_app_windows() {
            self.set_minimized(id, true);
        }
    }

    pub(crate) fn hide_other_apps(&mut self) {
        let app = self.focused_app_windows();
        let others: Vec<WindowId> = self
            .windows
            .iter()
            .map(|window| window.id())
            .filter(|id| !app.contains(id))
            .collect();
        for id in others {
            self.set_minimized(id, true);
        }
    }

    pub(crate) fn quit_focused_app(&mut self) {
        for id in self.focused_app_windows() {
            self.windows.send_close(id);
        }
    }

    fn focused_app_windows(&self) -> Vec<WindowId> {
        let Some(focused) = self.focused_window() else {
            return Vec::new();
        };
        let windows: Vec<(WindowId, String)> = self
            .windows
            .iter()
            .map(|window| (window.id(), window.app_id()))
            .collect();
        same_app(&windows, focused)
    }
}

fn same_app(windows: &[(WindowId, String)], focused: WindowId) -> Vec<WindowId> {
    let Some((_, app_id)) = windows.iter().find(|(id, _)| *id == focused) else {
        return Vec::new();
    };
    if app_id.is_empty() {
        return vec![focused];
    }
    windows
        .iter()
        .filter(|(_, other)| other == app_id)
        .map(|(id, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::same_app;
    use malaysia_scene::scene::WindowId;

    fn id(value: u64) -> WindowId {
        WindowId::new(value)
    }

    #[test]
    fn the_focused_app_is_every_window_with_its_app_id() {
        let windows = [
            (id(1), "foot".to_string()),
            (id(2), "kate".to_string()),
            (id(3), "foot".to_string()),
        ];
        assert_eq!(same_app(&windows, id(3)), vec![id(1), id(3)]);
    }

    #[test]
    fn a_window_without_app_id_is_an_app_of_its_own() {
        let windows = [(id(1), String::new()), (id(2), String::new())];
        assert_eq!(same_app(&windows, id(2)), vec![id(2)]);
    }
}
