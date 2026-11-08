mod server;
mod store;
mod view;

use bk::shell::{Anchors, KeyboardInteractivity, Layer, LayerSpec, run_layer};
use freya_core::integration::AppComponent;

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

pub(crate) fn toggle_do_not_disturb() {
    in_background("toggle Do Not Disturb", || {
        let proxy = control()?;
        let on: bool = proxy.get_property("DoNotDisturb")?;
        proxy
            .set_property("DoNotDisturb", !on)
            .map_err(zbus::Error::from)
    });
}

pub(crate) fn watch_do_not_disturb(changed: impl Fn(bool) + Send + 'static) {
    in_background("follow Do Not Disturb", move || {
        let proxy = control()?;
        changed(proxy.get_property("DoNotDisturb")?);
        for change in proxy.receive_property_changed::<bool>("DoNotDisturb") {
            changed(change.get()?);
        }
        Ok(())
    });
}
