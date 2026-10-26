//! Dev-only `com.canonical.AppMenu.Registrar` stub (ADR 0040, 14d-1).
//!
//! Qt and Firefox only export their menus when the registrar name has an
//! owner. This example owns it on the lab's private bus so the acceptance can
//! observe `org_kde_kwin_appmenu`; it is not part of the compositor or the
//! package.
//!
//! Run with `YUKI_LAB_REGISTRAR_STUB=1 ./dev.sh lab`.

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
