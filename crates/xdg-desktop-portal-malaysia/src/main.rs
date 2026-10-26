//! `xdg-desktop-portal-malaysia`: Malaysia's portal backend.
//!
//! One process, D-Bus activated, implementing only the interfaces that need
//! the compositor. The first is `org.freedesktop.impl.portal.Settings`, served
//! from `appearance.domain` (ADR 0031). `Screenshot` and `ScreenCast` come in
//! the same block.

#![deny(unsafe_code)]

mod settings;

use std::sync::{Arc, RwLock};

use anyhow::Result;

/// The backend name `xdg-desktop-portal` connects to.
const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.malaysia";
/// The standard backend object path.
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let values = Arc::new(RwLock::new(settings::Values::load()));
    let interface = settings::Settings::new(values.clone());
    let connection = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, interface)?
        .build()?;

    settings::watch(connection, values);
    tracing::info!(name = BUS_NAME, path = OBJECT_PATH, "portal backend ready");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
