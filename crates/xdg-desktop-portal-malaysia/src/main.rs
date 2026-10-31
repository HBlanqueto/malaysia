//! `xdg-desktop-portal-malaysia`: Malaysia's portal backend.
//!
//! One process, D-Bus activated, implementing only the interfaces that need
//! the compositor. The first is `org.freedesktop.impl.portal.Settings`, served
//! from `appearance.domain` (ADR 0031). `Screenshot` and `ScreenCast` come in
//! the same block.
//!
//! The same process keeps the GTK/libadwaita bridge (ADR 0050): it writes the
//! exact accent and the window tint as GTK user CSS while it runs, and removes
//! them on `SIGTERM`/`SIGINT`, when `malaysia-session.target` stops.

#![deny(unsafe_code)]

mod gtk;
mod settings;

use std::sync::{Arc, RwLock};

use anyhow::Result;
use nix::sys::signal::{SigSet, Signal};
use vow_appearance::{read_appearance, read_dominant};

/// The backend name `xdg-desktop-portal` connects to.
const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.malaysia";
/// The standard backend object path.
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Block the stop signals before any thread exists, so every thread
    // inherits the mask and only the `wait` below receives them.
    let mut stop = SigSet::empty();
    stop.add(Signal::SIGTERM);
    stop.add(Signal::SIGINT);
    stop.thread_block()?;

    // The GTK user CSS is written before the bus name is taken, so an app
    // that activates the portal already finds it.
    let bridge = gtk::Bridge::from_env();
    match (&bridge, read_appearance()) {
        (Some(bridge), Ok(appearance)) => bridge.sync(appearance.as_ref(), read_dominant()),
        (Some(_), Err(error)) => {
            tracing::warn!(%error, "appearance.domain unreadable; GTK bridge not written")
        }
        (None, _) => tracing::warn!("no XDG_CONFIG_HOME or HOME; GTK bridge disabled"),
    }

    let values = Arc::new(RwLock::new(settings::Values::load()));
    let interface = settings::Settings::new(values.clone());
    let connection = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, interface)?
        .build()?;

    settings::watch(connection, values, gtk::Bridge::from_env());
    tracing::info!(name = BUS_NAME, path = OBJECT_PATH, "portal backend ready");

    let signal = stop.wait()?;
    tracing::info!(?signal, "stopping; removing the GTK bridge");
    if let Some(bridge) = bridge
        && let Err(error) = bridge.remove()
    {
        tracing::warn!(%error, "could not remove the GTK bridge");
    }
    Ok(())
}
