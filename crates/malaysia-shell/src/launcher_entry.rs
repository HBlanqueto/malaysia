use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bk::platform::WakeHandle;
use futures_lite::StreamExt;
use zbus::fdo::{DBusProxy, RequestNameFlags};
use zbus::zvariant::OwnedValue;
use zbus::{MatchRule, MessageStream};

// Plasma's task manager and Dash to Dock own this name; apps look for it before they send.
const UNITY: &str = "com.canonical.Unity";
const INTERFACE: &str = "com.canonical.Unity.LauncherEntry";

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Entry {
    count: Option<i64>,
    count_visible: bool,
    progress: Option<f64>,
    progress_visible: bool,
    urgent: bool,
    sender: String,
}

impl Entry {
    pub(crate) fn badge(&self) -> Option<i64> {
        self.count.filter(|count| self.count_visible && *count > 0)
    }

    pub(crate) fn progress(&self) -> Option<f64> {
        self.progress
            .filter(|_| self.progress_visible)
            .map(|progress| progress.clamp(0.0, 1.0))
    }

    pub(crate) fn urgent(&self) -> bool {
        self.urgent
    }
}

pub(crate) type Entries = Arc<Mutex<HashMap<String, Entry>>>;

pub(crate) fn app_id(app_uri: &str) -> &str {
    let id = app_uri.strip_prefix("application://").unwrap_or(app_uri);
    id.strip_suffix(".desktop").unwrap_or(id)
}

fn update(
    entries: &mut HashMap<String, Entry>,
    app_uri: &str,
    sender: &str,
    properties: &HashMap<String, OwnedValue>,
) {
    let entry = entries.entry(app_id(app_uri).to_string()).or_default();
    entry.sender = sender.to_string();
    for (key, value) in properties {
        match key.as_str() {
            "count" => entry.count = i64::try_from(value).ok(),
            "count-visible" => entry.count_visible = bool::try_from(value).unwrap_or(false),
            "progress" => entry.progress = f64::try_from(value).ok(),
            "progress-visible" => entry.progress_visible = bool::try_from(value).unwrap_or(false),
            "urgent" => entry.urgent = bool::try_from(value).unwrap_or(false),
            _ => {}
        }
    }
}

pub(crate) fn start(entries: Entries, wake: WakeHandle) {
    std::thread::Builder::new()
        .name("malaysia-launcher-entry".to_string())
        .spawn(move || {
            if let Err(error) = async_io::block_on(follow(entries, wake)) {
                tracing::warn!(%error, "dock: badges and progress are off");
            }
        })
        .expect("malaysia-launcher-entry thread");
}

async fn follow(entries: Entries, wake: WakeHandle) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    if let Err(error) = connection
        .request_name_with_flags(UNITY, RequestNameFlags::DoNotQueue.into())
        .await
    {
        tracing::info!(%error, "dock: another dock owns {UNITY}");
    }
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(INTERFACE)?
        .member("Update")?
        .build();
    let mut updates = MessageStream::for_match_rule(rule, &connection, None).await?;
    let mut owners = DBusProxy::new(&connection)
        .await?
        .receive_name_owner_changed()
        .await?;
    loop {
        let open = futures_lite::future::or(
            async {
                if let Ok(message) = updates.next().await? {
                    let sender = message
                        .header()
                        .sender()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    if let Ok((app_uri, properties)) = message
                        .body()
                        .deserialize::<(String, HashMap<String, OwnedValue>)>()
                    {
                        update(&mut lock(&entries), &app_uri, &sender, &properties);
                        wake.wake();
                    }
                }
                Some(())
            },
            async {
                if let Ok(args) = owners.next().await?.args()
                    && args.new_owner().is_none()
                {
                    let gone = args.name().to_string();
                    lock(&entries).retain(|_, entry| entry.sender != gone);
                    wake.wake();
                }
                Some(())
            },
        )
        .await;
        if open.is_none() {
            return Ok(());
        }
    }
}

pub(crate) fn lock(entries: &Entries) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
    entries
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn value<'a>(value: impl Into<Value<'a>>) -> OwnedValue {
        OwnedValue::try_from(value.into()).unwrap()
    }

    #[test]
    fn partial_updates_merge_and_hidden_values_stay_hidden() {
        let mut entries = HashMap::new();
        let uri = "application://org.telegram.desktop.desktop";
        update(
            &mut entries,
            uri,
            ":1.4",
            &HashMap::from([
                ("count".into(), value(3i64)),
                ("count-visible".into(), value(true)),
            ]),
        );
        update(
            &mut entries,
            uri,
            ":1.4",
            &HashMap::from([("progress".into(), value(0.5f64))]),
        );
        let entry = &entries["org.telegram.desktop"];
        assert_eq!(entry.badge(), Some(3));
        assert_eq!(entry.progress(), None);
        update(
            &mut entries,
            uri,
            ":1.4",
            &HashMap::from([("progress-visible".into(), value(true))]),
        );
        assert_eq!(entries["org.telegram.desktop"].progress(), Some(0.5));
    }

    #[test]
    fn a_zero_count_shows_no_badge() {
        let mut entries = HashMap::new();
        update(
            &mut entries,
            "application://mail.desktop",
            ":1.9",
            &HashMap::from([
                ("count".into(), value(0i64)),
                ("count-visible".into(), value(true)),
            ]),
        );
        assert_eq!(entries["mail"].badge(), None);
    }
}
