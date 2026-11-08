use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_channel::{Receiver, Sender};
use bk::platform::WakeHandle;
use ctappearance::notifications::{AlertStyle, NotificationsConfig};
use zbus::fdo::{DBusProxy, RequestNameFlags};
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

use super::store::{Action, Center, Level, Moment, Notification, Origin, present};
use crate::app_scope::app_id_from_cgroup;
use crate::desktop::{DesktopEntry, read_entries};

const FDO_NAME: &str = "org.freedesktop.Notifications";
const FDO_PATH: &str = "/org/freedesktop/Notifications";
const GTK_NAME: &str = "org.gtk.Notifications";
const GTK_PATH: &str = "/org/gtk/Notifications";
pub(crate) const CONTROL_NAME: &str = "org.malaysia.Notifications1";
pub(crate) const CONTROL_PATH: &str = "/org/malaysia/Notifications1";
pub(crate) const DEFAULT_ACTION: &str = "default";
pub(crate) const CLOSED_DISMISSED: u32 = 2;
const CLOSED_BY_CALL: u32 = 3;
const SOUND: &str = "message-new-instant";
const INLINE_REPLY: &str = "inline-reply";
const REPLY_PURPOSE: &str = "im.reply-with-text";
const PORTAL_KEY: &str = "x-malaysia-portal";
const CAPABILITIES: [&str; 7] = [
    "actions",
    "body",
    "body-markup",
    "icon-static",
    "inline-reply",
    "persistence",
    "sound",
];

#[derive(Default)]
pub(crate) struct Hub {
    pub(crate) center: Center,
    pub(crate) banners: Vec<(u32, AlertStyle)>,
    pub(crate) panel_open: bool,
    pub(crate) front_app: Option<String>,
    wake: Option<WakeHandle>,
}

impl Hub {
    fn changed(&self) {
        if let Some(wake) = &self.wake {
            wake.wake();
        }
    }
}

pub(crate) enum Reply {
    Closed(Notification, u32),
    Invoked(Notification, String, Option<String>),
    Answered(Notification, String),
    DoNotDisturb,
}

#[derive(Clone)]
pub(crate) struct Link {
    pub(crate) hub: Arc<Mutex<Hub>>,
    replies: Sender<Reply>,
}

impl Link {
    pub(crate) fn hub(&self) -> MutexGuard<'_, Hub> {
        self.hub
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn set_wake(&self, wake: WakeHandle) {
        self.hub().wake = Some(wake);
    }

    pub(crate) fn reply(&self, reply: Reply) {
        let _ = self.replies.send_blocking(reply);
    }
}

pub(crate) fn start() -> Link {
    let (replies, received) = async_channel::unbounded();
    let link = Link {
        hub: Arc::new(Mutex::new(Hub {
            center: Center::load(),
            ..Hub::default()
        })),
        replies,
    };
    let thread = link.clone();
    std::thread::Builder::new()
        .name("malaysia-notifications".to_string())
        .spawn(move || {
            if let Err(error) = async_io::block_on(serve(thread, received)) {
                tracing::error!(%error, "notifications: the D-Bus server stopped");
            }
        })
        .expect("malaysia-notifications thread");
    link
}

async fn serve(link: Link, replies: Receiver<Reply>) -> zbus::Result<()> {
    let apps = Arc::new(Mutex::new(Apps::default()));
    let connection = zbus::connection::Builder::session()?
        .serve_at(
            FDO_PATH,
            Freedesktop {
                link: link.clone(),
                apps: apps.clone(),
            },
        )?
        .serve_at(
            GTK_PATH,
            Gtk {
                link: link.clone(),
                apps,
            },
        )?
        .serve_at(CONTROL_PATH, Control { link: link.clone() })?
        .build()
        .await?;
    for name in [FDO_NAME, GTK_NAME, CONTROL_NAME] {
        match connection
            .request_name_with_flags(
                name,
                RequestNameFlags::ReplaceExisting | RequestNameFlags::DoNotQueue,
            )
            .await
        {
            Ok(_) => tracing::info!(name, "notifications: name acquired"),
            Err(error) => {
                tracing::warn!(name, %error, "notifications: another server owns this name")
            }
        }
    }
    while let Ok(reply) = replies.recv().await {
        if let Err(error) = answer(&connection, reply).await {
            tracing::warn!(%error, "notifications: could not tell the app");
        }
    }
    Ok(())
}

async fn answer(connection: &zbus::Connection, reply: Reply) -> zbus::Result<()> {
    let fdo = SignalEmitter::new(connection, FDO_PATH)?;
    match reply {
        Reply::Closed(notification, reason) => {
            if notification.origin == Origin::Freedesktop {
                Freedesktop::notification_closed(&fdo, notification.id, reason).await?;
            }
        }
        Reply::Invoked(notification, key, token) => match &notification.origin {
            Origin::Freedesktop => {
                if let Some(token) = &token {
                    Freedesktop::activation_token(&fdo, notification.id, token).await?;
                }
                Freedesktop::action_invoked(&fdo, notification.id, &key).await?;
                Freedesktop::notification_closed(&fdo, notification.id, CLOSED_DISMISSED).await?;
            }
            Origin::Gtk { .. } => {
                activate_gtk(connection, &notification.app_id, &key, None, token).await?
            }
            Origin::Portal { id, .. } => {
                portal_action(connection, &notification.app_id, id, &key, Vec::new()).await?
            }
        },
        Reply::Answered(notification, text) => {
            let key = notification
                .reply
                .as_ref()
                .map(|reply| reply.key.clone())
                .unwrap_or_default();
            match &notification.origin {
                Origin::Freedesktop => {
                    Freedesktop::notification_replied(&fdo, notification.id, &text).await?;
                    Freedesktop::notification_closed(&fdo, notification.id, CLOSED_DISMISSED)
                        .await?;
                }
                Origin::Gtk { .. } => {
                    activate_gtk(connection, &notification.app_id, &key, Some(text), None).await?
                }
                Origin::Portal { id, .. } => {
                    let text =
                        OwnedValue::try_from(Value::from(text)).map_err(zbus::Error::from)?;
                    portal_action(connection, &notification.app_id, id, &key, vec![text]).await?
                }
            }
        }
        Reply::DoNotDisturb => {
            let control = connection
                .object_server()
                .interface::<_, Control>(CONTROL_PATH)
                .await?;
            control
                .get()
                .await
                .do_not_disturb_changed(control.signal_emitter())
                .await?;
        }
    }
    Ok(())
}

// GApplication actions are activated on the app itself (gio/gnotification.c, org.freedesktop.Application).
async fn activate_gtk(
    connection: &zbus::Connection,
    app_id: &str,
    key: &str,
    target: Option<String>,
    token: Option<String>,
) -> zbus::Result<()> {
    let path = format!("/{}", app_id.replace('.', "/").replace('-', "_"));
    let platform: HashMap<&str, Value<'_>> = token
        .iter()
        .flat_map(|token| {
            [
                ("activation-token", Value::from(token.as_str())),
                ("desktop-startup-id", Value::from(token.as_str())),
            ]
        })
        .collect();
    let proxy = zbus::Proxy::new(
        connection,
        app_id,
        path.as_str(),
        "org.freedesktop.Application",
    )
    .await?;
    match key.strip_prefix("app.") {
        Some(action) => {
            let target: Vec<Value<'_>> = target.map(Value::from).into_iter().collect();
            proxy
                .call_method("ActivateAction", &(action, target, platform))
                .await?;
        }
        None => {
            proxy.call_method("Activate", &(platform,)).await?;
        }
    }
    Ok(())
}

async fn portal_action(
    connection: &zbus::Connection,
    app_id: &str,
    id: &str,
    key: &str,
    parameter: Vec<OwnedValue>,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(connection, CONTROL_PATH)?;
    Control::portal_action(&emitter, app_id, id, key, parameter).await
}

#[derive(Default)]
struct Apps {
    entries: HashMap<String, DesktopEntry>,
}

impl Apps {
    fn entry(&mut self, id: &str) -> Option<DesktopEntry> {
        if !self.entries.contains_key(id) {
            self.entries = read_entries()
                .into_iter()
                .map(|entry| (entry.id.clone(), entry))
                .collect();
        }
        self.entries.get(id).cloned()
    }

    fn by_name(&mut self, name: &str) -> Option<DesktopEntry> {
        let wanted = name.to_lowercase();
        self.entry(name).or_else(|| {
            self.entries
                .values()
                .find(|entry| {
                    entry.name.to_lowercase() == wanted
                        || entry.id.to_lowercase().ends_with(&wanted)
                })
                .cloned()
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

struct Incoming {
    notification: Notification,
    transient: bool,
    sound: bool,
}

fn deliver(link: &Link, incoming: Incoming) -> u32 {
    let config: NotificationsConfig = bk::app::load_defaults("notifications");
    let mut hub = link.hub();
    let Incoming {
        notification,
        transient,
        sound,
    } = incoming;
    let moment = Moment {
        do_not_disturb: hub.center.do_not_disturb,
        panel_open: hub.panel_open,
        app_in_front: hub.front_app.as_deref() == Some(notification.app_id.as_str()),
        transient,
        now: notification.posted,
    };
    let shown = present(
        notification.level,
        &config.app(&notification.app_id),
        &moment,
    );
    let id = notification.id;
    hub.banners.retain(|(banner, _)| *banner != id);
    if let Some(style) = shown.banner {
        hub.banners.insert(0, (id, style));
        hub.banners.truncate(ctdesign::NOTIFICATION_MAX_BANNERS);
    }
    if shown.kept {
        hub.center.keep(notification);
    } else {
        hub.center.remove(id);
    }
    hub.center.save();
    if shown.sound && sound {
        bk::notifications::play_event_sound(SOUND);
    }
    hub.changed();
    id
}

struct Freedesktop {
    link: Link,
    apps: Arc<Mutex<Apps>>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Freedesktop {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let hint = |key: &str| {
            hints
                .get(key)
                .and_then(|value| String::try_from(value.clone()).ok())
        };
        let flag = |key: &str| {
            hints
                .get(key)
                .and_then(|value| bool::try_from(value).ok())
                .unwrap_or(false)
        };
        let sender_app = match header.sender() {
            Some(sender) => {
                sender_app_id(connection, zbus::names::BusName::Unique(sender.to_owned())).await
            }
            None => None,
        };
        let entry = {
            let mut apps = lock(&self.apps);
            sender_app
                .as_deref()
                .and_then(|id| apps.entry(id))
                .or_else(|| {
                    hint("desktop-entry").and_then(|id| apps.entry(id.trim_end_matches(".desktop")))
                })
                .or_else(|| apps.by_name(&app_name))
        };
        let mut pairs: Vec<Action> = actions
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[key, label]| Action {
                key: key.clone(),
                label: label.clone(),
            })
            .collect();
        let reply = pairs
            .iter()
            .position(|action| action.key == INLINE_REPLY)
            .map(|index| pairs.remove(index));
        let id = self.link.hub().center.allocate_id(replaces_id);
        let notification = Notification {
            id,
            reply,
            hide_on_lock: false,
            hide_content_on_lock: false,
            app_id: entry
                .as_ref()
                .map_or_else(|| app_name.clone(), |entry| entry.id.clone()),
            app_name: entry
                .as_ref()
                .map_or_else(|| app_name.clone(), |entry| entry.name.clone()),
            icon: entry
                .as_ref()
                .and_then(|entry| entry.icon.clone())
                .unwrap_or(app_icon),
            summary,
            body: strip_markup(&body),
            actions: pairs,
            level: Level::from_urgency(
                hints
                    .get("urgency")
                    .and_then(|value| u8::try_from(value).ok()),
            ),
            posted: now(),
            origin: Origin::Freedesktop,
        };
        deliver(
            &self.link,
            Incoming {
                notification,
                transient: flag("transient"),
                sound: !flag("suppress-sound"),
            },
        )
    }

    fn close_notification(&self, id: u32) {
        close(&self.link, id, true);
        self.link.hub().changed();
    }

    fn get_capabilities(&self) -> Vec<&'static str> {
        CAPABILITIES.to_vec()
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("Malaysia", "Malaysia", env!("CARGO_PKG_VERSION"), "1.3")
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_replied(
        emitter: &SignalEmitter<'_>,
        id: u32,
        text: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activation_token(
        emitter: &SignalEmitter<'_>,
        id: u32,
        activation_token: &str,
    ) -> zbus::Result<()>;
}

async fn sender_app_id(
    connection: &zbus::Connection,
    sender: zbus::names::BusName<'static>,
) -> Option<String> {
    let pid = DBusProxy::new(connection)
        .await
        .ok()?
        .get_connection_unix_process_id(sender)
        .await
        .ok()?;
    app_id_from_cgroup(&std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?)
}

fn strip_markup(body: &str) -> String {
    let mut text = String::with_capacity(body.len());
    let mut in_tag = false;
    for character in body.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

struct Gtk {
    link: Link,
    apps: Arc<Mutex<Apps>>,
}

#[zbus::interface(name = "org.gtk.Notifications")]
impl Gtk {
    fn add_notification(
        &self,
        app_id: String,
        id: String,
        notification: HashMap<String, OwnedValue>,
    ) {
        let text = |key: &str| {
            notification
                .get(key)
                .and_then(|value| String::try_from(value.clone()).ok())
                .unwrap_or_default()
        };
        let entry = lock(&self.apps).entry(&app_id);
        let hints: Vec<String> = notification
            .get("display-hint")
            .and_then(|value| Vec::<String>::try_from(value.clone()).ok())
            .unwrap_or_default();
        let hinted = |hint: &str| hints.iter().any(|given| given == hint);
        let mut reply = None;
        let mut actions = Vec::new();
        for button in notification
            .get("buttons")
            .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value.clone()).ok())
            .unwrap_or_default()
        {
            let field = |key: &str| {
                button
                    .get(key)
                    .and_then(|value| String::try_from(value.clone()).ok())
            };
            let (Some(key), Some(label)) = (field("action"), field("label")) else {
                continue;
            };
            let action = Action { key, label };
            if field("purpose").as_deref() == Some(REPLY_PURPOSE) {
                reply = Some(action);
            } else {
                actions.push(action);
            }
        }
        let default_action = Some(text("default-action"))
            .filter(|action| !action.is_empty())
            .map(|key| Action {
                key,
                label: String::new(),
            });
        let portal = notification
            .get(PORTAL_KEY)
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false);
        let replaces = self.link.hub().center.gtk_id(&app_id, &id).unwrap_or(0);
        let number = self.link.hub().center.allocate_id(replaces);
        let level = Level::from_priority(&text("priority"));
        let notification_body = text("body");
        let notification = Notification {
            id: number,
            app_name: entry
                .as_ref()
                .map_or_else(|| app_id.clone(), |entry| entry.name.clone()),
            icon: entry.and_then(|entry| entry.icon).unwrap_or_default(),
            app_id,
            summary: text("title"),
            body: notification_body,
            actions,
            reply,
            hide_on_lock: hinted("hide-on-lock-screen"),
            hide_content_on_lock: hinted("hide-content-on-lock-screen"),
            level,
            posted: now(),
            origin: if portal {
                Origin::Portal { id, default_action }
            } else {
                Origin::Gtk { id, default_action }
            },
        };
        let silent = text("sound") == "silent";
        deliver(
            &self.link,
            Incoming {
                notification,
                transient: hinted("transient"),
                sound: !silent,
            },
        );
    }

    fn remove_notification(&self, app_id: String, id: String) {
        let mut hub = self.link.hub();
        if let Some(number) = hub.center.gtk_id(&app_id, &id) {
            hub.center.remove(number);
            hub.banners.retain(|(banner, _)| *banner != number);
            hub.center.save();
            hub.changed();
        }
    }
}

struct Control {
    link: Link,
}

#[zbus::interface(name = "org.malaysia.Notifications1")]
impl Control {
    fn toggle_panel(&self) {
        let open = !self.link.hub().panel_open;
        show_panel(&self.link, open);
    }

    fn open_panel(&self) {
        show_panel(&self.link, true);
    }

    fn close_panel(&self) {
        show_panel(&self.link, false);
    }

    #[zbus(signal)]
    async fn portal_action(
        emitter: &SignalEmitter<'_>,
        app_id: &str,
        id: &str,
        action: &str,
        parameter: Vec<OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    fn do_not_disturb(&self) -> bool {
        self.link.hub().center.do_not_disturb
    }

    #[zbus(property)]
    fn set_do_not_disturb(&mut self, on: bool) {
        let mut hub = self.link.hub();
        hub.center.do_not_disturb = on;
        hub.center.save();
        hub.changed();
    }
}

fn show_panel(link: &Link, open: bool) {
    let mut hub = link.hub();
    if hub.panel_open == open {
        return;
    }
    hub.panel_open = open;
    hub.banners.clear();
    hub.changed();
}

pub(crate) fn close(link: &Link, id: u32, by_call: bool) {
    let removed = {
        let mut hub = link.hub();
        hub.banners.retain(|(banner, _)| *banner != id);
        let removed = hub.center.remove(id);
        hub.center.save();
        removed
    };
    if let Some(notification) = removed {
        link.reply(Reply::Closed(
            notification,
            if by_call {
                CLOSED_BY_CALL
            } else {
                CLOSED_DISMISSED
            },
        ));
    }
}

pub(crate) fn expire_banner(link: &Link, id: u32) {
    link.hub().banners.retain(|(banner, _)| *banner != id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_reduced_to_its_text() {
        assert_eq!(
            strip_markup("<b>Ana</b> &amp; <i>Luis</i> &lt;3"),
            "Ana & Luis <3"
        );
    }
}
