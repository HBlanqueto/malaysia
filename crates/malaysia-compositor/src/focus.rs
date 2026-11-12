use malaysia_scene::scene::WindowId;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{SERIAL_COUNTER, Serial};
use smithay::wayland::selection::data_device::set_data_device_focus;
use smithay::wayland::selection::primary_selection::set_primary_focus;
use smithay::wayland::shell::wlr_layer::LayerSurface;

use crate::state::{Grab, State};

impl State {
    fn log_focus_stack(&self, focused: Option<WindowId>) {
        if !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        let mut lines = Vec::new();
        for window in self.windows.iter() {
            let id = window.id();
            let minimized = self.minimized.contains(&id);
            lines.push(format!(
                "app_id={:?} mapped={} hidden={} minimized={} rect={:?} focused={}",
                window.app_id(),
                window.is_mapped(),
                window.is_hidden(),
                minimized,
                window.outer(),
                focused == Some(id),
            ));
        }
        tracing::debug!(focused = ?focused, "focus stack (back-to-front):\n{}", lines.join("\n"));
    }

    pub(crate) fn focus_window(&mut self, id: WindowId) {
        if self.locked {
            return;
        }
        // A window with a modal dialog hands its focus to the dialog, like an AppKit sheet.
        if let Some(dialog) = self.windows.modal_child(id) {
            return self.focus_window(dialog);
        }
        if self.layer_focus.blocks_window_focus() {
            tracing::info!(
                window = id.get(),
                "window focus deferred: an exclusive layer owns it"
            );
            self.layer_focus.focus_window_after(id);
            return;
        }
        self.show_space_of(id);
        if !self.windows.is_visible(id, &self.minimized) {
            tracing::info!(
                window = id.get(),
                "window focus ignored: the window is not visible"
            );
            return;
        }
        let Some(surface) = self.windows.surface_of(id).cloned() else {
            return;
        };
        self.windows.raise(id);
        let previous = self.focused_window();
        let changed = self.active_window.set(Some(id));
        let serial = SERIAL_COUNTER.next_serial();
        match self.seat.get_keyboard() {
            Some(keyboard) => {
                tracing::info!("giving keyboard focus to a window");
                let client = surface.client();
                keyboard.set_focus(self, Some(surface), serial);
                set_data_device_focus(&self.display_handle, &self.seat, client.clone());
                set_primary_focus(&self.display_handle, &self.seat, client);
            }
            None => tracing::warn!("a window was focused but the seat has no keyboard"),
        }
        if changed {
            let shown_before = self.shown_active_for(previous);
            self.sync_panels();
            let shown = self.shown_active();
            for window in [previous, shown_before, Some(id), shown]
                .into_iter()
                .flatten()
            {
                self.foreign_state_changed(window);
            }
        }
        self.log_focus_stack(Some(id));
    }

    pub(crate) fn focus_popup_surface(&mut self, surface: &WlSurface, serial: Serial) {
        if self.locked {
            return;
        }
        if let Some(keyboard) = self.seat.get_keyboard() {
            let previous_focus = keyboard.current_focus();
            let previous_window = previous_focus
                .as_ref()
                .and_then(|focus| self.windows.id_of(focus));
            let root_layer = previous_focus.as_ref().and_then(|focus| {
                if let Some(entry) = self.layers.entry_of(focus) {
                    Some((focus.clone(), entry.keyboard_interactivity))
                } else if self.popups.is_popup_surface(focus) {
                    self.popups.current_root_layer()
                } else {
                    None
                }
            });
            self.popups.push_focus_frame(
                surface.clone(),
                serial,
                root_layer,
                previous_focus.clone(),
                previous_window,
            );
            keyboard.set_focus(self, Some(surface.clone()), serial);
            if let Some(client) = surface.client() {
                set_data_device_focus(&self.display_handle, &self.seat, Some(client.clone()));
                set_primary_focus(&self.display_handle, &self.seat, Some(client));
            }
        }
    }

    pub(crate) fn apply_focus_restore(&mut self, restore: crate::popup::FocusRestore) {
        if self.locked {
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        match restore {
            crate::popup::FocusRestore::Popup(surface) => {
                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.set_focus(self, Some(surface), serial);
                }
            }
            crate::popup::FocusRestore::Layer(surface) => {
                self.focus_layer_surface(&surface);
            }
            crate::popup::FocusRestore::Previous(surface, window) => {
                let target = self
                    .focused_window()
                    .or(window)
                    .filter(|id| self.windows.surface_of(*id).is_some());
                if let Some(id) = target {
                    self.focus_window(id);
                } else if let Some(surface) =
                    surface.filter(|surface| self.surface_still_exists(surface))
                {
                    tracing::info!(
                        surface = ?surface.id(),
                        "giving keyboard focus back to a surface"
                    );
                    if let Some(keyboard) = self.seat.get_keyboard() {
                        keyboard.set_focus(self, Some(surface.clone()), serial);
                        if let Some(client) = surface.client() {
                            set_data_device_focus(
                                &self.display_handle,
                                &self.seat,
                                Some(client.clone()),
                            );
                            set_primary_focus(&self.display_handle, &self.seat, Some(client));
                        }
                    }
                } else {
                    self.clear_keyboard_focus();
                }
            }
        }
    }

    fn surface_still_exists(&self, surface: &WlSurface) -> bool {
        self.windows.id_of(surface).is_some()
            || self.popups.id_of(surface).is_some()
            || self.layers.entry_of(surface).is_some()
    }

    pub(crate) fn focus_layer_surface(&mut self, surface: &WlSurface) {
        if self.locked {
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            tracing::info!(surface = ?surface.id(), "giving keyboard focus to a layer surface");
            let client = surface.client();
            keyboard.set_focus(self, Some(surface.clone()), serial);
            set_data_device_focus(&self.display_handle, &self.seat, client.clone());
            set_primary_focus(&self.display_handle, &self.seat, client);
        }
    }

    pub(crate) fn layer_surface_mapped(&mut self, layer: &LayerSurface) {
        let surface = layer.wl_surface().clone();
        let Some(entry) = self.layers.entry_of(&surface) else {
            return;
        };
        if !matches!(
            entry.layer,
            smithay::wayland::shell::wlr_layer::Layer::Top
                | smithay::wayland::shell::wlr_layer::Layer::Overlay
        ) {
            return;
        }
        let focused_window = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus())
            .and_then(|focused| self.windows.id_of(&focused));
        let mode = entry.keyboard_interactivity;
        if self
            .layer_focus
            .mapped(surface.clone(), mode, focused_window)
        {
            self.focus_layer_surface(&surface);
        }
    }

    pub(crate) fn layer_keyboard_mode_committed(
        &mut self,
        surface: &WlSurface,
        before: Option<smithay::wayland::shell::wlr_layer::KeyboardInteractivity>,
    ) {
        use smithay::wayland::shell::wlr_layer::KeyboardInteractivity;
        let Some(entry) = self.layers.entry_of(surface) else {
            return;
        };
        let mode = entry.keyboard_interactivity;
        if before.is_none_or(|before| before == mode) {
            return;
        }
        if mode == KeyboardInteractivity::Exclusive {
            let layer = entry.surface.clone();
            self.layer_surface_mapped(&layer);
        } else {
            self.layer_surface_destroyed(surface);
        }
    }

    pub(crate) fn layer_surface_destroyed(&mut self, surface: &WlSurface) {
        use crate::layers::LayerUnfocus;
        match self.layer_focus.unmapped(surface) {
            LayerUnfocus::None => {}
            LayerUnfocus::Layer(next) => self.focus_layer_surface(&next),
            LayerUnfocus::Window(Some(id)) => {
                if self.windows.surface_of(id).is_some() {
                    self.focus_window(id);
                } else {
                    self.clear_keyboard_focus();
                }
            }
            LayerUnfocus::Window(None) => self.clear_keyboard_focus(),
        }
    }

    #[track_caller]
    pub(crate) fn clear_keyboard_focus(&mut self) {
        let location = std::panic::Location::caller();
        let shown_before = self.shown_active();
        let previous = self.active_window.clear();
        if previous.is_some() {
            tracing::info!(
                previous = previous.map(|id| id.get()),
                origin = format!("{}:{}", location.file(), location.line()),
                space = self.spaces.current_index(),
                bar_revealed = self.focus_reveal.is_shown(),
                "active window cleared"
            );
        }
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, None, serial);
            set_data_device_focus(&self.display_handle, &self.seat, None);
            set_primary_focus(&self.display_handle, &self.seat, None);
        }
        if let Some(previous) = previous {
            self.sync_panels();
            for window in [Some(previous), shown_before].into_iter().flatten() {
                self.foreign_state_changed(window);
            }
        }
        self.log_focus_stack(None);
    }

    pub(crate) fn forget_window(&mut self, surface: &WlSurface) {
        let active = self.focused_window();
        let next = self.focus_fallback(self.windows.id_of(surface));
        let Some(id) = self.windows.remove(surface) else {
            return;
        };
        if self
            .chrome_pointer
            .as_ref()
            .is_some_and(|(held, _)| *held == id)
        {
            self.chrome_pointer = None;
        }
        if self
            .titlebar_click
            .as_ref()
            .is_some_and(|(held, ..)| *held == id)
        {
            self.titlebar_click = None;
        }
        self.foreign_window_destroyed(id);
        self.space_window_removed(id);
        self.maximized.remove(&id);
        self.arranged.remove(&id);
        self.minimized.remove(&id);
        self.chrome.remove(crate::chrome::FrameOwner::Window(id));
        if self.grab.and_then(Grab::window) == Some(id) {
            self.grab = None;
        }
        tracing::info!(
            windows = self.windows.len(),
            topmost = ?self.windows.topmost(),
            "window removed"
        );
        if active == Some(id) {
            match next.filter(|next| self.windows.surface_of(*next).is_some()) {
                Some(next) => self.focus_window(next),
                None => self.clear_keyboard_focus(),
            }
        }
        if self.windows.is_empty() {
            tracing::info!("scene empty");
        }
        self.request_redraw();
    }
}
