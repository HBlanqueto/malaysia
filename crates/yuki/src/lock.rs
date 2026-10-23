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

use std::time::Duration;

use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{SERIAL_COUNTER, Size};
use smithay::wayland::compositor::add_destruction_hook;
use smithay::wayland::session_lock::{
    LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
};

use crate::state::State;

/// How long a freshly confirmed lock may take to create its first surface.
const LOCK_START_GRACE: Duration = Duration::from_secs(3);
/// Give up after this many recovery launches; a manual `yuki-shell lock` is
/// still accepted.
const LOCK_RECOVERY_MAX_ATTEMPTS: u32 = 5;

/// Exponential backoff, capped at 16 seconds.
fn lock_recovery_delay(attempts: u32) -> Duration {
    Duration::from_secs(1 << attempts.min(4))
}

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        // Lock the session: hide every window and shell panel (the draw path
        // only renders lock surfaces while `locked` is true) and drop input
        // focus so keystrokes cannot reach normal clients.
        let already_locked = self.locked;
        tracing::info!("session locked");
        self.locked = true;
        self.idle.set_locked(true);
        self.lock_surfaces.clear();
        self.cancel_lock_recovery();
        if !already_locked {
            self.lock_recovery_attempts = 0;
        }
        self.arm_lock_recovery(LOCK_START_GRACE);
        self.clear_keyboard_focus();
        self.clear_chrome_pointer();
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
        self.idle.set_locked(false);
        self.lock_surfaces.clear();
        self.cancel_lock_recovery();
        self.lock_recovery_attempts = 0;
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
        // A live surface means the lock client is working; cancel the current
        // watchdog but keep the attempt count so a client that keeps dying
        // still backs off.
        self.cancel_lock_recovery();
        add_destruction_hook::<Self, _>(surface.wl_surface(), |state, surface| {
            state.lock_surface_destroyed(surface);
        });
        self.lock_surfaces.push(surface);
        self.request_redraw();
    }
}

impl State {
    /// Schedules the next recovery check.
    fn arm_lock_recovery(&mut self, delay: Duration) {
        if self.lock_recovery_pending {
            return;
        }
        self.lock_recovery_pending = true;
        self.idle.schedule_lock_recovery(delay);
    }

    /// Cancels the pending recovery check, if any.
    fn cancel_lock_recovery(&mut self) {
        self.lock_recovery_pending = false;
        self.idle.cancel_lock_recovery();
    }

    /// A lock surface was destroyed. If it was the last one while the session
    /// is still locked, the client is gone: start the backoff recovery.
    fn lock_surface_destroyed(&mut self, surface: &WlSurface) {
        let tracked = self
            .lock_surfaces
            .iter()
            .any(|locked| locked.wl_surface() == surface);
        if !tracked {
            // A surface from a lock instance that was just superseded.
            return;
        }
        self.lock_surfaces
            .retain(|locked| locked.wl_surface() != surface && locked.alive());
        if self.locked && self.lock_surfaces.is_empty() {
            tracing::warn!("lock client disappeared; scheduling recovery");
            self.arm_lock_recovery(lock_recovery_delay(self.lock_recovery_attempts));
        }
    }

    /// Starts a fresh lock client after a manual request.
    ///
    /// A manual `Ctrl+Alt+L` after recovery gave up must get a fresh budget;
    /// otherwise the new client's first death would hit the old limit
    /// immediately. The session stays locked, and this never unlocks it.
    pub(crate) fn relaunch_lock_screen(&mut self) -> bool {
        self.lock_recovery_attempts = 0;
        self.cancel_lock_recovery();
        self.idle.spawn_lock_screen()
    }

    /// The recovery timer fired: launch a fresh lock client, with backoff and
    /// a finite number of attempts. The session stays locked throughout.
    pub(crate) fn lock_recovery_timer(&mut self) {
        self.lock_recovery_pending = false;
        if !self.locked {
            self.lock_recovery_attempts = 0;
            return;
        }
        self.lock_surfaces.retain(|locked| locked.alive());
        if !self.lock_surfaces.is_empty() {
            return;
        }
        if self.lock_recovery_attempts >= LOCK_RECOVERY_MAX_ATTEMPTS {
            tracing::error!(
                attempts = self.lock_recovery_attempts,
                "lock recovery gave up; the session stays locked"
            );
            return;
        }
        self.lock_recovery_attempts += 1;
        let running = self.idle.spawn_lock_screen();
        tracing::warn!(
            attempts = self.lock_recovery_attempts,
            running,
            "lock recovery launched a new lock client"
        );
        self.arm_lock_recovery(lock_recovery_delay(self.lock_recovery_attempts));
    }

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
