use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Type, Value};

use super::server::{Link, Reply};

pub(crate) const TRANSFER: &str = "transfer";
pub(crate) const CALL: &str = "call";
pub(crate) const PRINT: &str = "print";
pub(crate) const BYTES: &str = "bytes";
pub(crate) const FILES: &str = "files";
pub(crate) const PAGES: &str = "pages";
const JOB_PATH: &str = "/org/kde/notificationmanager/jobs/JobView_";
const CAN_CANCEL: i32 = 0x1;
const CAN_SUSPEND: i32 = 0x2;
// ActivityKit keeps an activity alive for at most 8 hours (ADR 0068, Live Activities).
pub(crate) const LONGEST: i64 = 8 * 3600;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub(crate) struct Activity {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) app_name: String,
    pub(crate) icon: String,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) percent: i32,
    pub(crate) processed: u64,
    pub(crate) total: u64,
    pub(crate) unit: String,
    pub(crate) started: i64,
    pub(crate) ended: bool,
    pub(crate) cancellable: bool,
    pub(crate) pausable: bool,
    pub(crate) paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivityKind {
    Transfer,
    Call,
    Print,
}

impl Activity {
    pub(crate) fn kind(&self) -> ActivityKind {
        match self.kind.as_str() {
            CALL => ActivityKind::Call,
            PRINT => ActivityKind::Print,
            _ => ActivityKind::Transfer,
        }
    }

    pub(crate) fn new(id: String, kind: &str, started: i64) -> Self {
        Self {
            id,
            kind: kind.to_string(),
            percent: -1,
            started,
            ..Self::default()
        }
    }

    pub(crate) fn fraction(&self) -> Option<f64> {
        if self.percent >= 0 {
            return Some(f64::from(self.percent.min(100)) / 100.0);
        }
        (self.total > 0).then(|| (self.processed as f64 / self.total as f64).clamp(0.0, 1.0))
    }

    fn apply_update(&mut self, properties: &HashMap<String, OwnedValue>) {
        let text = |key: &str| {
            properties
                .get(key)
                .and_then(|value| String::try_from(value.clone()).ok())
        };
        let number = |key: &str| {
            properties
                .get(key)
                .and_then(|value| u64::try_from(value).ok())
        };
        if let Some(title) = text("title") {
            self.title = title;
        }
        if let Some(message) = text("infoMessage").or_else(|| text("descriptionValue1")) {
            self.detail = message;
        }
        if let Some(percent) = properties
            .get("percent")
            .and_then(|value| u32::try_from(value).ok())
        {
            self.percent = percent.min(100) as i32;
        }
        if let Some(paused) = properties
            .get("suspended")
            .and_then(|value| bool::try_from(value).ok())
        {
            self.paused = paused;
        }
        if let Some(total) = number("totalBytes").filter(|total| *total > 0) {
            self.unit = BYTES.into();
            self.total = total;
            self.processed = number("processedBytes").unwrap_or(self.processed);
        } else if let Some(total) = number("totalFiles").filter(|total| *total > 0) {
            if self.unit != BYTES {
                self.unit = FILES.into();
                self.total = total;
                self.processed = number("processedFiles").unwrap_or(self.processed);
            }
        } else if let Some(processed) = number(if self.unit == FILES {
            "processedFiles"
        } else {
            "processedBytes"
        }) {
            self.processed = processed;
        }
    }

    fn set_amount(&mut self, amount: u64, unit: &str, total: bool) {
        if unit == BYTES || (unit == FILES && self.unit != BYTES) {
            self.unit = unit.to_string();
            if total {
                self.total = amount;
            } else {
                self.processed = amount;
            }
        }
    }
}

pub(crate) fn upsert(link: &Link, activity: Activity) {
    {
        let mut hub = link.hub();
        match hub
            .activities
            .iter_mut()
            .find(|known| known.id == activity.id)
        {
            Some(known) if *known == activity => return,
            Some(known) => *known = activity,
            None => hub.activities.push(activity),
        }
    }
    link.reply(Reply::Activities);
}

pub(crate) fn change(link: &Link, id: &str, change: impl FnOnce(&mut Activity)) {
    {
        let mut hub = link.hub();
        let Some(activity) = hub.activities.iter_mut().find(|known| known.id == id) else {
            return;
        };
        change(activity);
    }
    link.reply(Reply::Activities);
}

pub(crate) fn end(link: &Link, id: &str) {
    change(link, id, |activity| {
        activity.ended = true;
        activity.cancellable = false;
        activity.pausable = false;
    });
    link.reply(Reply::Ended(id.to_string()));
}

pub(crate) struct JobServer {
    pub(crate) link: Link,
    pub(crate) next: Arc<AtomicU32>,
}

impl JobServer {
    async fn open(
        &self,
        connection: &zbus::Connection,
        sender: String,
        app: Option<crate::desktop::DesktopEntry>,
        fallback_name: String,
        capabilities: i32,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let number = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let path = format!("{JOB_PATH}{number}");
        let id = format!("job-{number}");
        let mut activity = Activity::new(id.clone(), TRANSFER, chrono::Utc::now().timestamp());
        activity.app_name = app.as_ref().map_or(fallback_name, |app| app.name.clone());
        activity.icon = app.and_then(|app| app.icon).unwrap_or_default();
        activity.title = activity.app_name.clone();
        activity.cancellable = capabilities & CAN_CANCEL != 0;
        activity.pausable = capabilities & CAN_SUSPEND != 0;
        self.link.hub().owners.insert(id.clone(), sender);
        upsert(&self.link, activity);
        let server = connection.object_server();
        server
            .at(
                path.as_str(),
                JobViewV2 {
                    link: self.link.clone(),
                    id: id.clone(),
                },
            )
            .await?;
        server
            .at(
                path.as_str(),
                JobViewV3 {
                    link: self.link.clone(),
                    id,
                },
            )
            .await?;
        Ok(OwnedObjectPath::try_from(path).map_err(zbus::Error::from)?)
    }
}

#[zbus::interface(name = "org.kde.JobViewServer")]
impl JobServer {
    #[zbus(name = "requestView")]
    async fn request_view(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        app_name: String,
        _app_icon_name: String,
        capabilities: i32,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let app = super::server::desktop_entry(&app_name);
        self.open(connection, sender, app, app_name, capabilities)
            .await
    }
}

pub(crate) struct JobServerV2(pub(crate) JobServer);

#[zbus::interface(name = "org.kde.JobViewServerV2")]
impl JobServerV2 {
    #[zbus(name = "requestView")]
    async fn request_view(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        desktop_entry: String,
        capabilities: i32,
        _hints: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let app = super::server::desktop_entry(&desktop_entry);
        self.0
            .open(connection, sender, app, desktop_entry, capabilities)
            .await
    }
}

// KIO picks the V2 job tracker only while kuiserver says it needs none (KDynamicJobTracker).
pub(crate) struct UiServer;

#[zbus::interface(name = "org.kde.kuiserver")]
impl UiServer {
    #[zbus(name = "registerService")]
    fn register_service(&self, _service: String, _object_path: String) {}

    #[zbus(name = "emitJobUrlsChanged")]
    fn emit_job_urls_changed(&self) {}

    #[zbus(name = "requiresJobTracker")]
    fn requires_job_tracker(&self) -> bool {
        false
    }

    #[zbus(name = "registeredJobContacts")]
    fn registered_job_contacts(&self) -> Vec<String> {
        Vec::new()
    }
}

pub(crate) struct JobViewV2 {
    link: Link,
    id: String,
}

#[zbus::interface(name = "org.kde.JobViewV2")]
impl JobViewV2 {
    #[zbus(name = "terminate")]
    fn terminate(&self, _error_message: String) {
        end(&self.link, &self.id);
    }

    #[zbus(name = "setSuspended")]
    fn set_suspended(&self, suspended: bool) {
        change(&self.link, &self.id, |activity| activity.paused = suspended);
    }

    #[zbus(name = "setTotalAmount")]
    fn set_total_amount(&self, amount: u64, unit: String) {
        change(&self.link, &self.id, |activity| {
            activity.set_amount(amount, &unit, true)
        });
    }

    #[zbus(name = "setProcessedAmount")]
    fn set_processed_amount(&self, amount: u64, unit: String) {
        change(&self.link, &self.id, |activity| {
            activity.set_amount(amount, &unit, false)
        });
    }

    #[zbus(name = "setPercent")]
    fn set_percent(&self, percent: u32) {
        change(&self.link, &self.id, |activity| {
            activity.percent = percent.min(100) as i32
        });
    }

    #[zbus(name = "setSpeed")]
    fn set_speed(&self, _bytes_per_second: u64) {}

    #[zbus(name = "setInfoMessage")]
    fn set_info_message(&self, message: String) {
        change(&self.link, &self.id, |activity| activity.detail = message);
    }

    #[zbus(name = "setDescriptionField")]
    fn set_description_field(&self, number: u32, _name: String, value: String) -> bool {
        if number == 0 {
            change(&self.link, &self.id, |activity| activity.title = value);
        }
        true
    }

    #[zbus(name = "clearDescriptionField")]
    fn clear_description_field(&self, _number: u32) {}

    #[zbus(name = "setDestUrl")]
    fn set_dest_url(&self, _dest_url: OwnedValue) {}

    #[zbus(name = "setError")]
    fn set_error(&self, _error_code: u32) {}

    #[zbus(name = "suspendRequested")]
    #[zbus(signal)]
    async fn suspend_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(name = "resumeRequested")]
    #[zbus(signal)]
    async fn resume_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(name = "cancelRequested")]
    #[zbus(signal)]
    async fn cancel_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

pub(crate) struct JobViewV3 {
    link: Link,
    id: String,
}

#[zbus::interface(name = "org.kde.JobViewV3")]
impl JobViewV3 {
    #[zbus(name = "terminate")]
    fn terminate(
        &self,
        _error_code: u32,
        _error_message: String,
        _hints: HashMap<String, OwnedValue>,
    ) {
        end(&self.link, &self.id);
    }

    #[zbus(name = "update")]
    fn update(&self, properties: HashMap<String, OwnedValue>) {
        change(&self.link, &self.id, |activity| {
            activity.apply_update(&properties)
        });
    }

    #[zbus(name = "updateRequested")]
    #[zbus(signal)]
    async fn update_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(name = "suspendRequested")]
    #[zbus(signal)]
    async fn suspend_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(name = "resumeRequested")]
    #[zbus(signal)]
    async fn resume_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(name = "cancelRequested")]
    #[zbus(signal)]
    async fn cancel_requested(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Request {
    Cancel,
    Pause,
    Resume,
}

pub(crate) async fn ask_job(
    connection: &zbus::Connection,
    id: &str,
    request: Request,
) -> zbus::Result<()> {
    let Some(number) = id.strip_prefix("job-") else {
        return Ok(());
    };
    let emitter = SignalEmitter::new(connection, format!("{JOB_PATH}{number}"))?;
    match request {
        Request::Cancel => {
            JobViewV2::cancel_requested(&emitter).await?;
            JobViewV3::cancel_requested(&emitter).await
        }
        Request::Pause => {
            JobViewV2::suspend_requested(&emitter).await?;
            JobViewV3::suspend_requested(&emitter).await
        }
        Request::Resume => {
            JobViewV2::resume_requested(&emitter).await?;
            JobViewV3::resume_requested(&emitter).await
        }
    }
}

pub(crate) async fn forget_job(connection: &zbus::Connection, id: &str) {
    if let Some(number) = id.strip_prefix("job-") {
        let path = format!("{JOB_PATH}{number}");
        let server = connection.object_server();
        let _ = server.remove::<JobViewV2, _>(path.as_str()).await;
        let _ = server.remove::<JobViewV3, _>(path.as_str()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn value<'a>(value: impl Into<Value<'a>>) -> OwnedValue {
        OwnedValue::try_from(value.into()).unwrap()
    }

    #[test]
    fn a_v3_update_prefers_bytes_and_keeps_earlier_fields() {
        let mut activity = Activity::new("job-1".into(), TRANSFER, 0);
        activity.apply_update(&HashMap::from([
            ("title".into(), value("Copiando")),
            ("totalFiles".into(), value(3u64)),
            ("processedFiles".into(), value(1u64)),
        ]));
        assert_eq!(
            (activity.unit.as_str(), activity.processed, activity.total),
            (FILES, 1, 3)
        );
        activity.apply_update(&HashMap::from([
            ("totalBytes".into(), value(2_000u64)),
            ("processedBytes".into(), value(500u64)),
        ]));
        assert_eq!(
            (activity.unit.as_str(), activity.fraction()),
            (BYTES, Some(0.25))
        );
        activity.apply_update(&HashMap::from([("percent".into(), value(60u32))]));
        assert_eq!(activity.fraction(), Some(0.6));
        assert_eq!(activity.title, "Copiando");
    }

    #[test]
    fn v2_amounts_in_files_do_not_replace_bytes() {
        let mut activity = Activity::new("job-2".into(), TRANSFER, 0);
        activity.set_amount(100, BYTES, true);
        activity.set_amount(4, FILES, true);
        assert_eq!((activity.unit.as_str(), activity.total), (BYTES, 100));
    }
}
