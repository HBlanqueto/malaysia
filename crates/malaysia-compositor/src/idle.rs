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

const AUTO_ID: u64 = 0;
pub(crate) const LOCK_RECOVERY_ID: u64 = u64::MAX;
pub(crate) const DISPLAY_SLEEP_ID: u64 = u64::MAX - 2;

pub(crate) trait TimerSink {
    fn insert(&self, id: u64, duration: Duration);
    fn remove(&self, id: u64);
}

impl TimerSink for () {
    fn insert(&self, _id: u64, _duration: Duration) {}
    fn remove(&self, _id: u64) {}
}

pub(crate) trait IdleHost {
    fn idle_state(&mut self) -> &mut State;

    fn request_redraw(&mut self);
}

pub(crate) struct IdleTimers<D> {
    handle: LoopHandle<'static, D>,
    tokens: Arc<Mutex<HashMap<u64, RegistrationToken>>>,
}

impl<D> IdleTimers<D> {
    pub(crate) fn new(handle: LoopHandle<'static, D>) -> Self {
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
                    if data.idle_state().take_redraw_request() {
                        data.request_redraw();
                    }
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
    pub id: u64,
    resource: ExtIdleNotificationV1,
    timeout: Duration,
    idle: bool,
    ignore_inhibitor: bool,
}

pub(crate) struct IdleState {
    notifications: Vec<Notification>,
    next_id: u64,
    inhibited: bool,
    locked: bool,
    auto_timeout: Option<Duration>,
    display_timeout: Option<Duration>,
    display_asleep: bool,
    sink: Box<dyn TimerSink>,
    auto_lock_child: Option<Child>,
    wayland_display: Option<String>,
    sleep_lock: Option<std::sync::mpsc::Sender<()>>,
}

impl IdleState {
    pub(crate) fn new(
        display: &DisplayHandle,
        sink: Box<dyn TimerSink>,
        auto_timeout: Option<Duration>,
        display_timeout: Option<Duration>,
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
            display_timeout,
            display_asleep: false,
            sink,
            auto_lock_child: None,
            wayland_display: None,
            sleep_lock: None,
        };
        idle.arm_auto();
        idle.arm_display();
        idle
    }

    pub(crate) fn arm_cursor_boost(&self) {
        self.sink.insert(
            crate::state::CURSOR_BOOST_ID,
            Duration::from_millis(ctdesign::CURSOR_SHAKE_TICK_MS),
        );
    }

    pub(crate) fn set_wayland_display(&mut self, display: String) {
        self.wayland_display = Some(display);
    }

    pub(crate) fn schedule_lock_recovery(&mut self, delay: Duration) {
        self.sink.insert(LOCK_RECOVERY_ID, delay);
    }

    pub(crate) fn cancel_lock_recovery(&mut self) {
        self.sink.remove(LOCK_RECOVERY_ID);
    }

    pub(crate) fn locks_automatically(&self) -> bool {
        self.auto_timeout.is_some()
    }

    pub(crate) fn lock_before_sleep(&mut self, locked: std::sync::mpsc::Sender<()>) {
        if self.locked || !self.spawn_lock_screen() {
            let _ = locked.send(());
        } else {
            self.sleep_lock = Some(locked);
        }
    }

    pub(crate) fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
        if locked {
            self.sink.remove(AUTO_ID);
        } else {
            self.arm_auto();
        }
    }

    pub(crate) fn locking_for_sleep(&self) -> bool {
        self.sleep_lock.is_some()
    }

    pub(crate) fn confirm_locked(&mut self) {
        if let Some(sleep_lock) = self.sleep_lock.take() {
            let _ = sleep_lock.send(());
        }
    }

    pub(crate) fn note_activity(&mut self) {
        for notification in &mut self.notifications {
            if notification.idle {
                notification.resource.resumed();
                notification.idle = false;
            }
        }
        for notification in self
            .notifications
            .iter()
            .filter(|n| n.ignore_inhibitor || !self.inhibited)
        {
            self.sink.insert(notification.id, notification.timeout);
        }
        if !self.locked {
            self.arm_auto();
        }
        self.display_asleep = false;
        self.arm_display();
    }

    pub(crate) fn set_inhibited(&mut self, inhibited: bool) {
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
            } else if !idle && !ignore {
                self.sink.insert(id, timeout);
            }
        }
        if inhibited {
            self.sink.remove(AUTO_ID);
            self.sink.remove(DISPLAY_SLEEP_ID);
        } else {
            self.arm_auto();
            self.arm_display();
        }
    }

    pub(crate) fn display_asleep(&self) -> bool {
        self.display_asleep
    }

    pub(crate) fn idle_timer(&mut self, id: u64) {
        if id == AUTO_ID {
            self.auto_lock();
            return;
        }
        if id == DISPLAY_SLEEP_ID {
            self.display_asleep = !self.inhibited;
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

    fn add_notification(
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

    fn remove_notification(&mut self, id: u64) {
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

    fn arm_display(&mut self) {
        if self.inhibited {
            return;
        }
        if let Some(timeout) = self.display_timeout {
            self.sink.insert(DISPLAY_SLEEP_ID, timeout);
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

    pub(crate) fn wayland_display(&self) -> Option<&str> {
        self.wayland_display.as_deref()
    }

    pub(crate) fn spawn_lock_screen(&mut self) -> bool {
        if let Some(child) = &mut self.auto_lock_child {
            match child.try_wait() {
                Ok(None) => return true,
                Ok(Some(_)) | Err(_) => self.auto_lock_child = None,
            }
        }
        let Some(display) = self.wayland_display.as_deref() else {
            error!("starting the lock screen: the Wayland socket is not known yet");
            return false;
        };
        match crate::spawn::shell_component("lock", Some(display)) {
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

#[derive(Debug)]
struct IdleGlobalData;

#[derive(Debug)]
struct IdleNotificationData {
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

pub(crate) fn inhibit(
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
    state.sync_inhibition();
}

fn uninhibit(
    state: &mut State,
    surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
) {
    state.idle_inhibitors.retain(|s| s != surface);
    tracing::info!(
        inhibitors = state.idle_inhibitors.len(),
        "idle inhibition stopped"
    );
    state.sync_inhibition();
}

#[cfg(test)]
mod tests {
    use super::{AUTO_ID, DISPLAY_SLEEP_ID, IdleState, TimerSink};
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
        let idle = IdleState::new(&display.handle(), Box::new(()), None, None);
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
            None,
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
            None,
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

    #[test]
    fn the_display_sleeps_when_its_timeout_expires_and_wakes_with_activity() {
        let display = Display::<crate::state::State>::new().expect("display");
        let sink = FakeSink::default();
        let armed = sink.armed.clone();
        let timeout = Duration::from_secs(300);
        let mut idle = IdleState::new(&display.handle(), Box::new(sink), None, Some(timeout));
        assert!(armed.lock().unwrap().contains(&(DISPLAY_SLEEP_ID, timeout)));
        assert!(!idle.display_asleep());

        idle.idle_timer(DISPLAY_SLEEP_ID);
        assert!(idle.display_asleep());

        armed.lock().unwrap().clear();
        idle.note_activity();
        assert!(!idle.display_asleep());
        assert!(armed.lock().unwrap().contains(&(DISPLAY_SLEEP_ID, timeout)));
    }

    #[test]
    fn an_inhibitor_keeps_the_display_awake_until_it_goes_away() {
        let display = Display::<crate::state::State>::new().expect("display");
        let sink = FakeSink::default();
        let armed = sink.armed.clone();
        let removed = sink.removed.clone();
        let timeout = Duration::from_secs(300);
        let mut idle = IdleState::new(&display.handle(), Box::new(sink), None, Some(timeout));

        idle.set_inhibited(true);
        assert!(removed.lock().unwrap().contains(&DISPLAY_SLEEP_ID));
        idle.idle_timer(DISPLAY_SLEEP_ID);
        assert!(!idle.display_asleep());

        armed.lock().unwrap().clear();
        idle.set_inhibited(false);
        assert!(armed.lock().unwrap().contains(&(DISPLAY_SLEEP_ID, timeout)));
    }

    #[test]
    fn without_a_timeout_nothing_arms_the_display() {
        let display = Display::<crate::state::State>::new().expect("display");
        let sink = FakeSink::default();
        let armed = sink.armed.clone();
        let mut idle = IdleState::new(&display.handle(), Box::new(sink), None, None);
        idle.note_activity();
        assert!(armed.lock().unwrap().is_empty());
    }
}
