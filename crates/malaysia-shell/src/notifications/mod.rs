mod batteries;
mod cups;
mod image;
mod live;
mod server;
mod store;
mod view;
mod widgets;

pub(crate) use live::{Activity, ActivityKind};
pub(crate) use store::{Center, Notification};
pub(crate) use view::lock_card;
pub(crate) use widgets::month_name;

use bk::prelude::Symbol;
use bk::shell::{Anchors, KeyboardInteractivity, Layer, LayerSpec, run_layer};
use ctappearance::notifications::Focus;
use freya_core::integration::AppComponent;

use crate::localize::tr;

use server::{CONTROL_NAME, CONTROL_PATH};

pub fn run() -> anyhow::Result<()> {
    let link = server::start();
    run_layer(
        AppComponent::new(view::NotificationsApp { link }),
        LayerSpec {
            namespace: "malaysia-notifications".to_string(),
            layer: Layer::Overlay,
            anchors: Anchors {
                top: true,
                right: true,
                bottom: false,
                left: false,
            },
            exclusive_zone: 0,
            keyboard: KeyboardInteractivity::None,
            size: Some((1, 1)),
        },
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

fn control() -> zbus::Result<zbus::blocking::Proxy<'static>> {
    let connection = zbus::blocking::Connection::session()?;
    zbus::blocking::Proxy::new(&connection, CONTROL_NAME, CONTROL_PATH, CONTROL_NAME)
}

fn in_background(what: &'static str, call: impl FnOnce() -> zbus::Result<()> + Send + 'static) {
    std::thread::spawn(move || {
        if let Err(error) = call() {
            tracing::warn!(%error, "notifications: could not {what}");
        }
    });
}

pub(crate) fn toggle_panel() {
    in_background("open the panel", || {
        control()?.call_method("TogglePanel", &()).map(drop)
    });
}

pub(crate) fn toggle_focus() {
    in_background("toggle Do Not Disturb", || {
        let proxy = control()?;
        let focus: String = proxy.get_property("Focus")?;
        let next = if focus.is_empty() {
            store::focus_name(Some(Focus::DoNotDisturb))
        } else {
            ""
        };
        proxy.set_property("Focus", next).map_err(zbus::Error::from)
    });
}

pub(crate) fn watch_focus(changed: impl Fn(Option<Focus>) + Send + 'static) {
    in_background("follow the Focus", move || {
        let proxy = control()?;
        let focus: String = proxy.get_property("Focus")?;
        changed(store::parse_focus(&focus));
        for change in proxy.receive_property_changed::<String>("Focus") {
            changed(store::parse_focus(&change.get()?));
        }
        Ok(())
    });
}

pub(crate) fn watch_activities(changed: impl Fn(Vec<Activity>) + Send + 'static) {
    in_background("follow the Live Activities", move || {
        let proxy = control()?;
        changed(proxy.get_property("Activities")?);
        for change in proxy.receive_property_changed::<Vec<Activity>>("Activities") {
            changed(change.get()?);
        }
        Ok(())
    });
}

pub(crate) fn cancel_activity(id: String) {
    in_background("cancel a Live Activity", move || {
        control()?.call_method("CancelActivity", &(id,)).map(drop)
    });
}

pub(crate) fn pause_activity(id: String, paused: bool) {
    in_background("pause a Live Activity", move || {
        control()?
            .call_method("PauseActivity", &(id, paused))
            .map(drop)
    });
}

pub(crate) fn focus_symbol(focus: Focus) -> Symbol {
    match focus {
        Focus::DoNotDisturb => Symbol::Moon,
        Focus::Sleep => Symbol::Bed,
        Focus::Personal => Symbol::Person,
        Focus::Work => Symbol::Briefcase,
    }
}

pub(crate) fn focus_title(focus: Focus) -> &'static str {
    match focus {
        Focus::DoNotDisturb => tr!("focus-do-not-disturb"),
        Focus::Sleep => tr!("focus-sleep"),
        Focus::Personal => tr!("focus-personal"),
        Focus::Work => tr!("focus-work"),
    }
}
