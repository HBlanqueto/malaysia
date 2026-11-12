use std::time::Duration;

use smithay::backend::input::InputTime;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{SERIAL_COUNTER, Size};
use smithay::wayland::compositor::add_destruction_hook;
use smithay::wayland::image_copy_capture::CaptureFailureReason;
use smithay::wayland::session_lock::{
    LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
};

use crate::state::State;

// ext-session-lock-v1 asks for a time limit when waiting for the lock surfaces.
pub(crate) const LOCK_START_GRACE: Duration = Duration::from_secs(3);
const LOCK_RECOVERY_MAX_ATTEMPTS: u32 = 5;

fn lock_recovery_delay(attempts: u32) -> Duration {
    Duration::from_secs(1 << attempts.min(4))
}

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        // ext-session-lock-v1: a second lock must not take over a live one, or its unlock_and_destroy unlocks the session.
        if self.lock_owner.as_ref().is_some_and(Resource::is_alive) {
            tracing::warn!("lock refused: a live client already holds the session lock");
            return;
        }
        self.lock_owner = Some(confirmation.ext_session_lock().clone());
        let already_locked = self.locked;
        tracing::info!("session locking");
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
        self.end_grab();
        if let Some(location) = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
        {
            self.pointer_motion(location, InputTime::from_millis(0));
        }
        for pending in std::mem::take(&mut self.pending_captures) {
            pending.frame.fail(CaptureFailureReason::Unknown);
        }
        self.screenshot = None;
        self.cancel_screenshot();
        self.start_locking(confirmation, already_locked);
        self.request_redraw();
    }

    fn unlock(&mut self) {
        tracing::info!("session unlocked");
        self.lock_owner = None;
        self.locked = false;
        self.idle.set_locked(false);
        self.lock_surfaces.clear();
        self.cancel_lock_recovery();
        self.lock_recovery_attempts = 0;
        if let Some(id) = self.focus_fallback(None) {
            self.focus_window(id);
        }
        self.start_unlocking();
        self.request_redraw();
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        tracing::info!(output = ?output.id(), "new session lock surface");
        self.fit_lock_surface(&surface);
        if self.lock_surfaces.is_empty() {
            self.focus_lock_surface(&surface);
        }
        self.cancel_lock_recovery();
        add_destruction_hook::<Self, _>(surface.wl_surface(), |state, surface| {
            state.lock_surface_destroyed(surface);
        });
        self.lock_surfaces.push(surface);
        if let Some(location) = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
        {
            self.pointer_motion(location, InputTime::from_millis(0));
        }
        self.request_redraw();
    }
}

impl State {
    fn arm_lock_recovery(&mut self, delay: Duration) {
        if self.lock_recovery_pending {
            return;
        }
        self.lock_recovery_pending = true;
        self.idle.schedule_lock_recovery(delay);
    }

    fn cancel_lock_recovery(&mut self) {
        self.lock_recovery_pending = false;
        self.idle.cancel_lock_recovery();
    }

    fn lock_surface_destroyed(&mut self, surface: &WlSurface) {
        let tracked = self
            .lock_surfaces
            .iter()
            .any(|locked| locked.wl_surface() == surface);
        if !tracked {
            return;
        }
        self.lock_surfaces
            .retain(|locked| locked.wl_surface() != surface && locked.alive());
        if self.locked && self.lock_surfaces.is_empty() {
            tracing::warn!("lock client disappeared; scheduling recovery");
            self.arm_lock_recovery(lock_recovery_delay(self.lock_recovery_attempts));
        }
    }

    pub(crate) fn relaunch_lock_screen(&mut self) -> bool {
        self.lock_recovery_attempts = 0;
        self.cancel_lock_recovery();
        self.idle.spawn_lock_screen()
    }

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

    pub(crate) fn fit_lock_surface(&self, surface: &LockSurface) {
        let size = self.logical_output_size();
        surface.with_pending_state(|state| {
            state.size = Some(Size::from((size.w as u32, size.h as u32)));
        });
    }

    fn focus_lock_surface(&mut self, surface: &LockSurface) {
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, Some(surface.wl_surface().clone()), serial);
        }
    }
}
