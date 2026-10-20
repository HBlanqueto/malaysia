//! `ext-idle-notify-v1`, automatic locking and idle inhibition.
//!
//! The compositor owns the idle clock: every input event calls
//! [`IdleState::note_activity`], which re-arms the client notifications and the
//! auto-lock timer. A timer firing is delivered back to [`State`] through
//! [`IdleHost`], so this module never needs a backend-specific loop handle.
//!
//! `zwp_idle_inhibit_manager_v1` suppresses both the client notifications and
//! auto-lock while at least one inhibitor surface is alive. mpv in fullscreen
//! creates one; a windowed video does not.

use std::{
    collections::HashMap,
    process::Child,
    sync::{Arc, Mutex},
    time::Duration,
};

use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay::reexports::wayland_protocols::ext::idle_notify::v1::server::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{self, ExtIdleNotifierV1},
};
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource};
use smithay::wayland::compositor::add_destruction_hook;
use smithay::wayland::idle_inhibit::IdleInhibitHandler;
use smithay::wayland::{Dispatch2, GlobalDispatch2};
use tracing::error;

use crate::state::State;

/// The auto-lock timer has no notification resource, so it gets this id;
/// client notifications start at 1.
const AUTO_ID: u64 = 0;
/// The lock-recovery timer shares the sink but is handled by `State`.
pub(crate) const LOCK_RECOVERY_ID: u64 = u64::MAX;

/// Where a backend's event loop inserts timers.
pub trait TimerSink {
    /// (Re)arms `id` for `duration`, replacing any timer already armed.
    fn insert(&self, id: u64, duration: Duration);
    /// Cancels `id` if armed.
    fn remove(&self, id: u64);
}

impl TimerSink for () {
    fn insert(&self, _id: u64, _duration: Duration) {}
    fn remove(&self, _id: u64) {}
}

/// A backend whose loop can host idle timers. Implemented by `Session` and
/// `Udev`; the callback always reaches the shared [`State`].
pub trait IdleHost {
    /// The shared compositor state.
    fn idle_state(&mut self) -> &mut State;
}

/// The concrete sink the two backends use.
pub struct IdleTimers<D> {
    handle: LoopHandle<'static, D>,
    tokens: Arc<Mutex<HashMap<u64, RegistrationToken>>>,
}

impl<D> IdleTimers<D> {
    /// Creates a sink over the backend's loop.
    pub fn new(handle: LoopHandle<'static, D>) -> Self {
        Self {
            handle,
            tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<D> TimerSink for IdleTimers<D>
where
    D: IdleHost + 'static,
{
    fn insert(&self, id: u64, duration: Duration) {
        self.remove(id);
        let tokens = self.tokens.clone();
        let token =
            self.handle
                .insert_source(Timer::from_duration(duration), move |_, _, data: &mut D| {
                    tokens.lock().unwrap().remove(&id);
                    data.idle_state().idle_timer(id);
                    TimeoutAction::Drop
                });
        match token {
            Ok(token) => {
                self.tokens.lock().unwrap().insert(id, token);
            }
            Err(error) => error!(%error, "arming an idle timer"),
        }
    }

    fn remove(&self, id: u64) {
        if let Some(token) = self.tokens.lock().unwrap().remove(&id) {
            self.handle.remove(token);
        }
    }
}

struct Notification {
    id: u64,
    resource: ExtIdleNotificationV1,
    timeout: Duration,
    idle: bool,
    ignore_inhibitor: bool,
}

/// The idle-notifier global, its clients and the auto-lock timer.
pub struct IdleState {
    notifications: Vec<Notification>,
    next_id: u64,
    inhibited: bool,
    locked: bool,
    auto_timeout: Option<Duration>,
    sink: Box<dyn TimerSink>,
    auto_lock_child: Option<Child>,
    /// The compositor's own Wayland socket, passed to the spawned lock.
    wayland_display: Option<String>,
}

impl IdleState {
    /// Creates the global and arms the auto-lock timer.
    pub fn new(
        display: &DisplayHandle,
        sink: Box<dyn TimerSink>,
        auto_timeout: Option<Duration>,
    ) -> Self {
        let _global: GlobalId =
            display.create_global::<State, ExtIdleNotifierV1, _>(2, IdleGlobalData);
        tracing::debug!(?auto_timeout, "idle auto-lock armed");
        let mut idle = Self {
            notifications: Vec::new(),
            next_id: 1,
            inhibited: false,
            locked: false,
            auto_timeout,
            sink,
            auto_lock_child: None,
            wayland_display: None,
        };
        idle.arm_auto();
        idle
    }

    /// Records the compositor's socket so a spawned lock connects to Yuki, not
    /// to the host session.
    pub fn set_wayland_display(&mut self, display: String) {
        self.wayland_display = Some(display);
    }

    /// Schedules the lock-recovery watchdog/backoff. The id is reserved for
    /// the lock module; `IdleState::idle_timer` never sees it.
    pub fn schedule_lock_recovery(&mut self, delay: Duration) {
        self.sink.insert(LOCK_RECOVERY_ID, delay);
    }

    /// Cancels a pending lock-recovery timer.
    pub fn cancel_lock_recovery(&mut self) {
        self.sink.remove(LOCK_RECOVERY_ID);
    }

    /// The session locked: auto-lock has nothing left to do.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
        if locked {
            self.sink.remove(AUTO_ID);
        } else {
            self.arm_auto();
        }
    }

    /// Re-arms the idle clock after input.
    pub fn note_activity(&mut self) {
        for notification in &mut self.notifications {
            if notification.idle {
                notification.resource.resumed();
                notification.idle = false;
            }
        }
        let armed: Vec<(u64, Duration, bool)> = self
            .notifications
            .iter()
            .filter(|n| !n.idle && (n.ignore_inhibitor || !self.inhibited))
            .map(|n| (n.id, n.timeout, n.ignore_inhibitor))
            .collect();
        for (id, timeout, _) in armed {
            self.sink.insert(id, timeout);
        }
        if !self.locked {
            self.arm_auto();
        }
    }

    /// An inhibitor appeared or disappeared.
    pub fn set_inhibited(&mut self, inhibited: bool) {
        if self.inhibited == inhibited {
            return;
        }
        self.inhibited = inhibited;
        let states: Vec<(u64, Duration, bool, bool)> = self
            .notifications
            .iter()
            .map(|n| (n.id, n.timeout, n.ignore_inhibitor, n.idle))
            .collect();
        for (id, timeout, ignore, idle) in states {
            if !ignore && inhibited {
                if idle
                    && let Some(notification) = self.notifications.iter_mut().find(|n| n.id == id)
                {
                    notification.resource.resumed();
                    notification.idle = false;
                }
                self.sink.remove(id);
            } else if !idle && (ignore || !inhibited) {
                self.sink.insert(id, timeout);
            }
        }
        if inhibited {
            self.sink.remove(AUTO_ID);
        } else {
            self.arm_auto();
        }
    }

    /// A timer fired. Notification timers report `idled`; the auto-lock timer
    /// starts `yuki-shell lock`.
    pub fn idle_timer(&mut self, id: u64) {
        if id == AUTO_ID {
            self.auto_lock();
            return;
        }
        let Some(notification) = self.notifications.iter_mut().find(|n| n.id == id) else {
            return;
        };
        if notification.idle || (self.inhibited && !notification.ignore_inhibitor) {
            return;
        }
        notification.resource.idled();
        notification.idle = true;
    }

    /// Registers a client notification and arms its timer.
    pub fn add_notification(
        &mut self,
        resource: ExtIdleNotificationV1,
        timeout: Duration,
        ignore_inhibitor: bool,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.notifications.push(Notification {
            id,
            resource,
            timeout,
            idle: false,
            ignore_inhibitor,
        });
        if ignore_inhibitor || !self.inhibited {
            self.sink.insert(id, timeout);
        }
        id
    }

    /// Drops a client notification, normally because its resource died.
    pub fn remove_notification(&mut self, id: u64) {
        self.sink.remove(id);
        self.notifications.retain(|n| n.id != id);
    }

    fn arm_auto(&mut self) {
        if self.inhibited || self.locked {
            return;
        }
        if let Some(timeout) = self.auto_timeout {
            self.sink.insert(AUTO_ID, timeout);
        }
    }

    fn auto_lock(&mut self) {
        if self.locked || self.inhibited {
            return;
        }
        if self.spawn_lock_screen() {
            tracing::info!("idle timeout: started the lock screen");
        }
    }

    /// Starts `yuki-shell lock` if there is no live lock child.
    ///
    /// Shared by the idle timeout and by the compositor's lock recovery, so
    /// both use the same shell command and the same `WAYLAND_DISPLAY`.
    /// Returns whether a child is running (or was just started).
    pub(crate) fn spawn_lock_screen(&mut self) -> bool {
        if let Some(child) = &mut self.auto_lock_child {
            match child.try_wait() {
                Ok(None) => return true,
                Ok(Some(_)) | Err(_) => self.auto_lock_child = None,
            }
        }
        let Some(display) = &self.wayland_display else {
            error!("starting the lock screen: the Wayland socket is not known yet");
            return false;
        };
        let shell = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("yuki-shell")))
            .unwrap_or_else(|| "yuki-shell".into());
        let mut command = crate::spawn::shell_command(shell);
        command.arg("lock").env("WAYLAND_DISPLAY", display);
        match crate::spawn::spawn_shell(command) {
            Ok(child) => {
                self.auto_lock_child = Some(child);
                true
            }
            Err(error) => {
                error!(%error, "starting the lock screen");
                false
            }
        }
    }
}

/// Per-global marker data for the notifier.
#[derive(Debug)]
pub struct IdleGlobalData;

/// Per-resource data for one notification.
#[derive(Debug)]
pub struct IdleNotificationData {
    id: std::sync::atomic::AtomicU64,
}

impl GlobalDispatch2<ExtIdleNotifierV1, State> for IdleGlobalData {
    fn bind(
        &self,
        state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ExtIdleNotifierV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        let _ = state;
        data_init.init(resource, IdleGlobalData);
    }
}

impl Dispatch2<ExtIdleNotifierV1, State> for IdleGlobalData {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _resource: &ExtIdleNotifierV1,
        request: ext_idle_notifier_v1::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            ext_idle_notifier_v1::Request::GetIdleNotification { id, timeout, .. } => {
                let timeout = Duration::from_millis(u64::from(timeout));
                let notification = data_init.init(
                    id,
                    IdleNotificationData {
                        id: std::sync::atomic::AtomicU64::new(0),
                    },
                );
                let notification_id =
                    state
                        .idle
                        .add_notification(notification.clone(), timeout, false);
                notification
                    .data::<IdleNotificationData>()
                    .unwrap()
                    .id
                    .store(notification_id, std::sync::atomic::Ordering::Relaxed);
            }
            ext_idle_notifier_v1::Request::GetInputIdleNotification { id, timeout, .. } => {
                let timeout = Duration::from_millis(u64::from(timeout));
                let notification = data_init.init(
                    id,
                    IdleNotificationData {
                        id: std::sync::atomic::AtomicU64::new(0),
                    },
                );
                let notification_id =
                    state
                        .idle
                        .add_notification(notification.clone(), timeout, true);
                notification
                    .data::<IdleNotificationData>()
                    .unwrap()
                    .id
                    .store(notification_id, std::sync::atomic::Ordering::Relaxed);
            }
            ext_idle_notifier_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch2<ExtIdleNotificationV1, State> for IdleNotificationData {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _resource: &ExtIdleNotificationV1,
        request: ext_idle_notification_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        if matches!(request, ext_idle_notification_v1::Request::Destroy) {
            state
                .idle
                .remove_notification(self.id.load(std::sync::atomic::Ordering::Relaxed));
        }
    }

    fn destroyed(&self, state: &mut State, _client: ClientId, _resource: &ExtIdleNotificationV1) {
        state
            .idle
            .remove_notification(self.id.load(std::sync::atomic::Ordering::Relaxed));
    }
}

impl IdleInhibitHandler for State {
    fn inhibit(
        &mut self,
        surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        inhibit(self, &surface);
    }

    fn uninhibit(
        &mut self,
        surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        uninhibit(self, &surface);
    }
}

/// A surface asked to keep the session awake.
pub fn inhibit(
    state: &mut State,
    surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
) {
    if !state.idle_inhibitors.iter().any(|s| s == surface) {
        add_destruction_hook::<State, _>(surface, |state, surface| {
            uninhibit(state, surface);
        });
        state.idle_inhibitors.push(surface.clone());
    }
    tracing::info!(
        inhibitors = state.idle_inhibitors.len(),
        "idle inhibition started"
    );
    state.idle.set_inhibited(!state.idle_inhibitors.is_empty());
}

/// A surface stopped inhibiting, or died.
pub fn uninhibit(
    state: &mut State,
    surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
) {
    state.idle_inhibitors.retain(|s| s != surface);
    tracing::info!(
        inhibitors = state.idle_inhibitors.len(),
        "idle inhibition stopped"
    );
    state.idle.set_inhibited(!state.idle_inhibitors.is_empty());
}

#[cfg(test)]
mod tests {
    use super::{AUTO_ID, IdleState, TimerSink};
    use smithay::reexports::wayland_server::Display;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Default)]
    struct FakeSink {
        armed: Arc<Mutex<Vec<(u64, Duration)>>>,
        removed: Arc<Mutex<Vec<u64>>>,
    }

    impl TimerSink for FakeSink {
        fn insert(&self, id: u64, duration: Duration) {
            self.armed.lock().unwrap().push((id, duration));
        }
        fn remove(&self, id: u64) {
            self.removed.lock().unwrap().push(id);
        }
    }

    #[test]
    fn a_zero_timeout_does_not_arm_auto_lock() {
        let display = Display::<crate::state::State>::new().expect("display");
        let idle = IdleState::new(&display.handle(), Box::new(()), None);
        let _ = idle;
    }

    #[test]
    fn activity_rearms_the_timeout() {
        let display = Display::<crate::state::State>::new().expect("display");
        let sink = FakeSink::default();
        let armed = sink.armed.clone();
        let mut idle = IdleState::new(
            &display.handle(),
            Box::new(sink),
            Some(Duration::from_secs(15)),
        );
        idle.note_activity();
        assert!(
            armed
                .lock()
                .unwrap()
                .iter()
                .any(|(id, duration)| *id == AUTO_ID && *duration == Duration::from_secs(15))
        );
    }

    #[test]
    fn an_inhibitor_pauses_the_auto_lock_until_it_goes_away() {
        let display = Display::<crate::state::State>::new().expect("display");
        let sink = FakeSink::default();
        let armed = sink.armed.clone();
        let removed = sink.removed.clone();
        let mut idle = IdleState::new(
            &display.handle(),
            Box::new(sink),
            Some(Duration::from_secs(15)),
        );
        armed.lock().unwrap().clear();
        removed.lock().unwrap().clear();

        idle.set_inhibited(true);
        assert!(removed.lock().unwrap().contains(&AUTO_ID));
        idle.note_activity();
        assert!(
            !armed.lock().unwrap().iter().any(|(id, _)| *id == AUTO_ID),
            "activity must not re-arm auto-lock while inhibited"
        );

        idle.set_inhibited(false);
        assert!(
            armed.lock().unwrap().iter().any(|(id, _)| *id == AUTO_ID),
            "removing the inhibitor must re-arm auto-lock"
        );
    }
}
