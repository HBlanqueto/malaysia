//! The `ext-session-lock-v1` integration: locking the session.
//!
//! A lock client (`yuki-shell lock`) binds the manager and asks to lock the
//! session. While locked, Yuki renders only the lock surfaces and feeds input
//! only to them: normal clients stop receiving frames and keyboard focus. The
//! shell panels and the windows are hidden (ADR 0023, the "lock stays locked"
//! failure case).
//!
//! The **never-unlock guarantee**: the session is only unlocked by an explicit
//! `unlock_and_destroy` request from the lock client (when the user has been
//! authenticated). If the lock client dies while the session is locked, the
//! session stays locked — Smithay never calls [`SessionLockHandler::unlock`]
//! on its own, and the draw path falls back to a solid color once the dead
//! client's lock surfaces are gone (`session::draw`). That matches the
//! protocol: "If the client dies while the session is locked, the compositor
//! must not unlock the session in response" (wayland-protocols 1.49,
//! `ext-session-lock-v1`).

use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::utils::{SERIAL_COUNTER, Size};
use smithay::wayland::session_lock::{
    LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
};

use crate::state::State;

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        // Lock the session: hide every window and shell panel (the draw path
        // only renders lock surfaces while `locked` is true) and drop input
        // focus so keystrokes cannot reach normal clients.
        tracing::info!("session locked");
        self.locked = true;
        self.lock_surfaces.clear();
        self.clear_keyboard_focus();
        self.request_redraw();
        // Confirm immediately: the next frame shows the locked session (a
        // solid color until the client's lock surfaces arrive), so no normal
        // content is visible after the `locked` event. Dropping the
        // `SessionLocker` here instead would tell the client locking failed.
        confirmation.lock();
    }

    fn unlock(&mut self) {
        // Only reached through an explicit `unlock_and_destroy` request from
        // the lock client. Client death never calls this (see the module
        // comment); the session would stay locked forever rather than leak a
        // frame of the desktop.
        tracing::info!("session unlocked");
        self.locked = false;
        self.lock_surfaces.clear();
        self.request_redraw();
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        tracing::info!(output = ?output.id(), "new session lock surface");
        // The lock surface covers the whole output; the first configure tells
        // the client the exact size to render.
        let size = self.logical_output_size();
        surface.with_pending_state(|state| {
            state.size = Some(Size::from((size.w as u32, size.h as u32)));
        });
        surface.send_configure();
        // Give the first lock surface keyboard focus (the protocol's suggested
        // behaviour): keystrokes go to the lock client, which authenticates.
        if self.lock_surfaces.is_empty() {
            self.focus_lock_surface(&surface);
        }
        self.lock_surfaces.push(surface);
        self.request_redraw();
    }
}

impl State {
    /// Gives keyboard focus to a lock surface, without touching the clipboard
    /// or primary-selection focus (no normal client may see the selection
    /// while the session is locked).
    fn focus_lock_surface(&mut self, surface: &LockSurface) {
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, Some(surface.wl_surface().clone()), serial);
        }
    }
}
