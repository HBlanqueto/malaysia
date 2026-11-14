use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use bk::prelude::*;
use zbus::MatchRule;
use zbus::blocking::fdo::PropertiesProxy;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::names::InterfaceName;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const SETTLE: Duration = Duration::from_millis(150);
const TICK: Duration = Duration::from_secs(15);
const QUEUED_SIGNALS: usize = 64;

pub type Properties = HashMap<String, OwnedValue>;

pub trait Service: Default + Send + 'static {
    const NAME: &'static str;
    type Snapshot: Clone + PartialEq + Send + 'static;
    type Command: Send + 'static;

    fn read(&mut self, bus: &Connection) -> zbus::Result<Self::Snapshot>;
    fn run(&mut self, bus: &Connection, command: Self::Command) -> zbus::Result<()>;
    // Signals report every change; a service that also watches what none reports returns true to read again.
    fn tick(&mut self, _bus: &Connection) -> bool {
        false
    }
    fn signals() -> zbus::Result<MatchRule<'static>> {
        signals_from(Self::NAME)
    }
}

enum Event<C> {
    Changed,
    Command(C),
}

enum Update<S> {
    Snapshot(Option<S>),
    Failure(String),
}

pub struct Remote<C>(Sender<Event<C>>);

impl<C> Clone for Remote<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<C> Remote<C> {
    pub fn send(&self, command: C) {
        let _ = self.0.send(Event::Command(command));
    }
}

pub struct Live<S: Service> {
    pub snapshot: State<Option<S::Snapshot>>,
    pub failure: State<Option<String>>,
    pub remote: Remote<S::Command>,
}

pub fn use_service<S: Service>() -> Live<S> {
    let mut snapshot = use_state(|| None);
    let mut failure = use_state(|| None);
    let remote = use_hook(|| {
        let (events, inbox) = mpsc::channel();
        let (updates, outbox) = async_channel::unbounded();
        let worker_events = events.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("settings {}", S::NAME))
            .spawn(move || work(S::default(), worker_events, inbox, updates));
        if let Err(error) = spawned {
            tracing::warn!(%error, service = S::NAME, "no worker thread");
        }
        spawn(async move {
            while let Ok(update) = outbox.recv().await {
                match update {
                    Update::Snapshot(next) => snapshot.set(next),
                    Update::Failure(message) => failure.set(Some(message)),
                }
            }
        });
        Remote(events)
    });
    Live {
        snapshot,
        failure,
        remote,
    }
}

fn work<S: Service>(
    mut service: S,
    events: Sender<Event<S::Command>>,
    inbox: Receiver<Event<S::Command>>,
    updates: async_channel::Sender<Update<S::Snapshot>>,
) {
    let bus = match Connection::system() {
        Ok(bus) => bus,
        Err(error) => {
            tracing::warn!(%error, service = S::NAME, "no system bus");
            let _ = updates.send_blocking(Update::Snapshot(None));
            return;
        }
    };
    watch(&bus, S::signals(), events.clone());
    watch(&bus, owner_changes_of(S::NAME), events);
    let mut last: Option<Option<S::Snapshot>> = None;
    'serve: loop {
        let snapshot = service
            .read(&bus)
            .inspect_err(|error| tracing::debug!(%error, service = S::NAME, "unavailable"))
            .ok();
        if last.as_ref() != Some(&snapshot) {
            if updates
                .send_blocking(Update::Snapshot(snapshot.clone()))
                .is_err()
            {
                return;
            }
            last = Some(snapshot);
        }
        let first = loop {
            match inbox.recv_timeout(TICK) {
                Ok(event) => break event,
                Err(RecvTimeoutError::Timeout) => {
                    if service.tick(&bus) {
                        continue 'serve;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        };
        let mut event = Some(first);
        let settled = Instant::now() + SETTLE;
        while let Some(next) = event {
            if let Event::Command(command) = next
                && let Err(error) = service.run(&bus, command)
                && updates
                    .send_blocking(Update::Failure(failure_text(&error)))
                    .is_err()
            {
                return;
            }
            event = inbox
                .recv_timeout(settled.saturating_duration_since(Instant::now()))
                .ok();
        }
    }
}

fn failure_text(error: &zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(_, Some(message), _) => message.clone(),
        other => other.to_string(),
    }
}

fn watch<C: Send + 'static>(
    bus: &Connection,
    rule: zbus::Result<MatchRule<'static>>,
    events: Sender<Event<C>>,
) {
    let signals = match rule
        .and_then(|rule| MessageIterator::for_match_rule(rule, bus, Some(QUEUED_SIGNALS)))
    {
        Ok(signals) => signals,
        Err(error) => {
            tracing::warn!(%error, "no signal subscription");
            return;
        }
    };
    let spawned = std::thread::Builder::new()
        .name("settings signals".to_string())
        .spawn(move || {
            for message in signals {
                if message.is_ok() && events.send(Event::Changed).is_err() {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "no signal thread");
    }
}

fn signals_from(name: &'static str) -> zbus::Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(name)?
        .build())
}

fn owner_changes_of(name: &'static str) -> zbus::Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .add_arg(name)?
        .build())
}

fn properties_proxy<'a>(
    bus: &Connection,
    service: &'static str,
    path: impl TryInto<ObjectPath<'a>, Error: Into<zbus::Error>>,
) -> zbus::Result<PropertiesProxy<'a>> {
    PropertiesProxy::builder(bus)
        .destination(service)?
        .path(path)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
}

pub fn get_all<'a>(
    bus: &Connection,
    service: &'static str,
    path: impl TryInto<ObjectPath<'a>, Error: Into<zbus::Error>>,
    interface: &'static str,
) -> zbus::Result<Properties> {
    Ok(properties_proxy(bus, service, path)?
        .get_all(InterfaceName::from_static_str_unchecked(interface))?)
}

pub fn set<'a>(
    bus: &Connection,
    service: &'static str,
    path: impl TryInto<ObjectPath<'a>, Error: Into<zbus::Error>>,
    interface: &'static str,
    name: &str,
    value: Value<'_>,
) -> zbus::Result<()> {
    Ok(properties_proxy(bus, service, path)?.set(
        InterfaceName::from_static_str_unchecked(interface),
        name,
        value,
    )?)
}

pub fn property<T: TryFrom<OwnedValue>>(properties: &Properties, name: &str) -> Option<T> {
    properties
        .get(name)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| T::try_from(value).ok())
}

pub struct WhileVisible {
    child: Element,
    on_change: Callback<bool, ()>,
}

impl WhileVisible {
    pub fn new(child: impl IntoElement, on_change: impl Into<Callback<bool, ()>>) -> Self {
        Self {
            child: child.into_element(),
            on_change: on_change.into(),
        }
    }
}

impl PartialEq for WhileVisible {
    fn eq(&self, other: &Self) -> bool {
        self.child == other.child
    }
}

impl Component for WhileVisible {
    fn render(&self) -> impl IntoElement {
        let shown = self.on_change.clone();
        use_hook(move || shown.call(true));
        let hidden = self.on_change.clone();
        use_drop(move || hidden.call(false));
        self.child.clone()
    }
}
