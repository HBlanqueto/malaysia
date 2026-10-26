//! Dev-only `com.canonical.AppMenu.Registrar` diagnostic stub (ADR 0040).
//!
//! The bar owns the registrar name since 14d-2, so the lab no longer starts
//! this example. It stays as a manual diagnostic for checking whether an app
//! exports `org_kde_kwin_appmenu` without the bar running.

use std::time::Duration;

use zbus::zvariant::OwnedObjectPath;

struct AppMenuRegistrar;

#[zbus::interface(name = "com.canonical.AppMenu.Registrar")]
impl AppMenuRegistrar {
    fn register_window(&self, window_id: u32, menu_object_path: OwnedObjectPath) {
        eprintln!("appmenu registrar: RegisterWindow window={window_id} path={menu_object_path}");
    }

    fn unregister_window(&self, window_id: u32) {
        eprintln!("appmenu registrar: UnregisterWindow window={window_id}");
    }

    fn get_menu_for_window(&self, window_id: u32) -> zbus::fdo::Result<(String, OwnedObjectPath)> {
        Err(zbus::fdo::Error::Failed(format!(
            "no menu registered for window {window_id}"
        )))
    }
}

fn main() -> zbus::Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    connection
        .object_server()
        .at("/com/canonical/AppMenu/Registrar", AppMenuRegistrar)?;
    connection.request_name("com.canonical.AppMenu.Registrar")?;
    eprintln!("appmenu registrar: ready");
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
