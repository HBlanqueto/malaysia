use std::collections::HashMap;

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

const SHELL: &str = "org.gtk.Notifications";
const SHELL_PATH: &str = "/org/gtk/Notifications";
const CONTROL: &str = "org.malaysia.Notifications1";
const CONTROL_PATH: &str = "/org/malaysia/Notifications1";
// Read by malaysia-shell notifications, so ActionInvoked comes back here (ADR 0068).
const PORTAL_KEY: &str = "x-malaysia-portal";
const CATEGORIES: [&str; 7] = [
    "im.received",
    "alarm.ringing",
    "call.incoming",
    "call.ongoing",
    "call.unanswered",
    "os.battery.low",
    "browser.web-notification",
];
const PURPOSES: [&str; 1] = ["im.reply-with-text"];
// The icon and the sound arrive as file descriptors; the shell uses the app's own icon.
const DROPPED: [&str; 3] = ["icon", "sound", "markup-body"];

pub struct Notification;

#[zbus::interface(name = "org.freedesktop.impl.portal.Notification")]
impl Notification {
    async fn add_notification(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        app_id: String,
        id: String,
        notification: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        let shell = forwarded(notification)?;
        connection
            .call_method(
                Some(SHELL),
                SHELL_PATH,
                Some(SHELL),
                "AddNotification",
                &(app_id, id, shell),
            )
            .await?;
        Ok(())
    }

    async fn remove_notification(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        app_id: String,
        id: String,
    ) -> zbus::fdo::Result<()> {
        connection
            .call_method(
                Some(SHELL),
                SHELL_PATH,
                Some(SHELL),
                "RemoveNotification",
                &(app_id, id),
            )
            .await?;
        Ok(())
    }

    #[zbus(property)]
    fn supported_options(&self) -> HashMap<&'static str, Value<'static>> {
        HashMap::from([
            ("category", Value::from(CATEGORIES.to_vec())),
            ("button-purpose", Value::from(PURPOSES.to_vec())),
        ])
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        2
    }

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        app_id: &str,
        id: &str,
        action: &str,
        parameter: Vec<OwnedValue>,
    ) -> zbus::Result<()>;
}

fn forwarded(
    mut notification: HashMap<String, OwnedValue>,
) -> zbus::fdo::Result<HashMap<String, OwnedValue>> {
    if let Some(markup) = notification
        .get("markup-body")
        .and_then(|value| String::try_from(value.clone()).ok())
    {
        notification.insert("body".into(), owned(strip_markup(&markup))?);
    }
    for key in DROPPED {
        notification.remove(key);
    }
    notification.insert(PORTAL_KEY.into(), owned(true)?);
    Ok(notification)
}

fn owned<'a>(value: impl Into<Value<'a>>) -> zbus::fdo::Result<OwnedValue> {
    OwnedValue::try_from(value.into()).map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
}

fn strip_markup(markup: &str) -> String {
    let mut text = String::with_capacity(markup.len());
    let mut in_tag = false;
    for character in markup.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

pub fn relay_actions(connection: zbus::blocking::Connection, object_path: &'static str) {
    std::thread::spawn(move || {
        let relay = || -> zbus::Result<()> {
            let shell = zbus::blocking::Proxy::new(&connection, CONTROL, CONTROL_PATH, CONTROL)?;
            for signal in shell.receive_signal("PortalAction")? {
                let (app_id, id, action, parameter): (String, String, String, Vec<OwnedValue>) =
                    signal.body().deserialize()?;
                connection.emit_signal(
                    None::<()>,
                    object_path,
                    "org.freedesktop.impl.portal.Notification",
                    "ActionInvoked",
                    &(app_id, id, action, parameter),
                )?;
            }
            Ok(())
        };
        if let Err(error) = relay() {
            tracing::warn!(%error, "notification actions are not relayed to apps");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_portal_notification_reaches_the_shell_as_plain_text_and_marked() {
        let notification = HashMap::from([
            ("title".to_string(), owned("Ana").unwrap()),
            (
                "markup-body".to_string(),
                owned("<b>Hola</b> &amp; adiós").unwrap(),
            ),
        ]);
        let shell = forwarded(notification).unwrap();
        assert_eq!(
            String::try_from(shell["body"].clone()).unwrap(),
            "Hola & adiós"
        );
        assert!(!shell.contains_key("markup-body"));
        assert!(bool::try_from(&shell[PORTAL_KEY]).unwrap());
    }
}
