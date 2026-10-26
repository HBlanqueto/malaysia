//! The app-menu D-Bus client and the `AppMenu.Registrar` owner (ADR 0040).
//!
//! One dedicated thread owns a single session connection. The render thread
//! sends FIFO commands and drains immutable snapshots from another channel;
//! it never waits for D-Bus. Registrar calls only need the shared map.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use celestial_kit::platform::{AppMenuAddress, WakeHandle};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub const REGISTRAR_NAME: &str = "com.canonical.AppMenu.Registrar";
pub const REGISTRAR_PATH: &str = "/com/canonical/AppMenu/Registrar";
const DBUSMENU_INTERFACE: &str = "com.canonical.dbusmenu";
const CALL_TIMEOUT: Duration = Duration::from_millis(500);

/// One command for the D-Bus thread.
#[derive(Clone, Debug)]
pub enum Command {
    Focus {
        address: AppMenuAddress,
        generation: u64,
    },
    Open {
        address: AppMenuAddress,
        id: i32,
        generation: u64,
    },
    Activate {
        address: AppMenuAddress,
        id: i32,
        timestamp: u32,
    },
}

/// The properties and children of one dbusmenu node, before mapping.
#[derive(Clone, Debug, PartialEq)]
pub struct RawItem {
    pub id: i32,
    pub properties: HashMap<String, OwnedValue>,
    pub children: Vec<RawItem>,
}

/// One async answer for the bar: the children of the root (`parent = None`) or
/// of a submenu. `items = None` means the call failed or timed out.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuSnapshot {
    pub generation: u64,
    pub address: AppMenuAddress,
    pub parent: Option<i32>,
    pub items: Option<Vec<RawItem>>,
}

/// Render-side handle: commands go in, snapshots come out.
#[derive(Clone)]
pub struct MenuClient {
    commands: mpsc::Sender<Command>,
    snapshots: Arc<Mutex<mpsc::Receiver<MenuSnapshot>>>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
}

impl MenuClient {
    pub fn focus(&self, address: AppMenuAddress, generation: u64) {
        let _ = self.commands.send(Command::Focus {
            address,
            generation,
        });
    }

    pub fn open(&self, address: AppMenuAddress, id: i32, generation: u64) {
        let _ = self.commands.send(Command::Open {
            address,
            id,
            generation,
        });
    }

    pub fn activate(&self, address: AppMenuAddress, id: i32, timestamp: u32) {
        let _ = self.commands.send(Command::Activate {
            address,
            id,
            timestamp,
        });
    }

    /// Installs the render-loop wake used after every snapshot.
    pub fn set_wake(&self, wake: WakeHandle) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(wake);
        }
    }

    /// Returns the next ready snapshot without blocking.
    pub fn try_recv(&self) -> Option<MenuSnapshot> {
        self.snapshots.lock().ok()?.try_recv().ok()
    }
}

type RegistrarWindows = Arc<Mutex<HashMap<u32, (String, OwnedObjectPath)>>>;

struct Registrar {
    windows: RegistrarWindows,
}

#[zbus::interface(name = "com.canonical.AppMenu.Registrar")]
impl Registrar {
    fn register_window(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        window_id: u32,
        menu_object_path: OwnedObjectPath,
    ) {
        let service = header.sender().map(ToString::to_string).unwrap_or_default();
        tracing::info!(
            window = window_id,
            service = %service,
            path = %menu_object_path,
            "menubar: Registrar RegisterWindow"
        );
        if let Ok(mut windows) = self.windows.lock() {
            windows.insert(window_id, (service, menu_object_path));
        }
    }

    fn unregister_window(&self, window_id: u32) {
        tracing::info!(window = window_id, "menubar: Registrar UnregisterWindow");
        if let Ok(mut windows) = self.windows.lock() {
            windows.remove(&window_id);
        }
    }

    fn get_menu_for_window(&self, window_id: u32) -> zbus::fdo::Result<(String, OwnedObjectPath)> {
        self.windows
            .lock()
            .ok()
            .and_then(|windows| windows.get(&window_id).cloned())
            .ok_or_else(|| {
                zbus::fdo::Error::Failed(format!("no menu registered for window {window_id}"))
            })
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        4
    }
}

/// Starts the D-Bus thread. On connection failure the client keeps accepting
/// commands and simply never answers; the bar stays on the 14c fallback.
pub fn start() -> MenuClient {
    let (commands, command_rx) = mpsc::channel();
    let (snapshot_tx, snapshot_rx) = mpsc::channel();
    let snapshots = Arc::new(Mutex::new(snapshot_rx));
    let wake = Arc::new(Mutex::new(None));
    let thread_wake = wake.clone();
    std::thread::Builder::new()
        .name("yuki-dbusmenu".to_string())
        .spawn(move || {
            async_io::block_on(run(command_rx, snapshot_tx, thread_wake));
        })
        .expect("yuki-dbusmenu thread");
    MenuClient {
        commands,
        snapshots,
        wake,
    }
}

async fn run(
    commands: mpsc::Receiver<Command>,
    snapshots: mpsc::Sender<MenuSnapshot>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
) {
    let builder = match zbus::connection::Builder::session() {
        Ok(builder) => builder.method_timeout(CALL_TIMEOUT),
        Err(error) => {
            tracing::error!(%error, "menubar: could not build the session bus connection");
            return;
        }
    };
    let connection = match builder.build().await {
        Ok(connection) => connection,
        Err(error) => {
            tracing::error!(%error, "menubar: could not connect to the session bus");
            return;
        }
    };

    match connection.request_name(REGISTRAR_NAME).await {
        Ok(_) => {
            tracing::info!(name = REGISTRAR_NAME, "menubar: Registrar acquired");
            let registrar = Registrar {
                windows: Arc::new(Mutex::new(HashMap::new())),
            };
            if let Err(error) = connection
                .object_server()
                .at(REGISTRAR_PATH, registrar)
                .await
            {
                tracing::warn!(%error, "menubar: could not serve the Registrar");
            }
        }
        Err(error) => {
            tracing::warn!(
                %error,
                name = REGISTRAR_NAME,
                "menubar: Registrar name already owned; continuing without it"
            );
        }
    }

    while let Ok(command) = commands.recv() {
        async_io::block_on(handle(&connection, &snapshots, &wake, command));
    }
}

async fn handle(
    connection: &zbus::Connection,
    snapshots: &mpsc::Sender<MenuSnapshot>,
    wake: &Arc<Mutex<Option<WakeHandle>>>,
    command: Command,
) {
    match command {
        Command::Focus {
            address,
            generation,
        } => {
            let started = std::time::Instant::now();
            let items = children(connection, &address, 0, 1).await;
            let count = items.as_ref().map_or(0, Vec::len);
            tracing::info!(
                generation,
                service = %address.service,
                titles = count,
                ms = started.elapsed().as_millis(),
                "menubar: dbusmenu root snapshot"
            );
            let _ = snapshots.send(MenuSnapshot {
                generation,
                address,
                parent: None,
                items,
            });
            notify(wake);
        }
        Command::Open {
            address,
            id,
            generation,
        } => {
            let started = std::time::Instant::now();
            let items = open_submenu(connection, &address, id).await;
            let count = items.as_ref().map_or(0, Vec::len);
            tracing::info!(
                generation,
                id,
                service = %address.service,
                items = count,
                ms = started.elapsed().as_millis(),
                "menubar: dbusmenu submenu snapshot"
            );
            let _ = snapshots.send(MenuSnapshot {
                generation,
                address,
                parent: Some(id),
                items,
            });
            notify(wake);
        }
        Command::Activate {
            address,
            id,
            timestamp,
        } => {
            if let Err(error) = event(connection, &address, id, timestamp).await {
                tracing::warn!(%error, id, service = %address.service, "menubar: dbusmenu Event failed");
            } else {
                tracing::info!(id, service = %address.service, "menubar: dbusmenu Event sent");
            }
        }
    }
}

fn notify(wake: &Arc<Mutex<Option<WakeHandle>>>) {
    if let Some(wake) = wake.lock().ok().and_then(|slot| slot.clone()) {
        wake.wake();
    }
}

async fn proxy<'a>(
    connection: &'a zbus::Connection,
    address: &'a AppMenuAddress,
) -> zbus::Result<zbus::Proxy<'a>> {
    zbus::Proxy::new(
        connection,
        address.service.as_str(),
        address.path.as_str(),
        DBUSMENU_INTERFACE,
    )
    .await
}

type RawNode = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

fn raw_item(value: OwnedValue) -> Option<RawItem> {
    let value: zbus::zvariant::Value<'_> = value.into();
    let (id, properties, children) = value.downcast::<RawNode>().ok()?;
    Some(RawItem {
        id,
        properties,
        children: children.into_iter().filter_map(raw_item).collect(),
    })
}

async fn layout(
    connection: &zbus::Connection,
    address: &AppMenuAddress,
    parent: i32,
    depth: i32,
) -> zbus::Result<Vec<RawItem>> {
    let proxy = proxy(connection, address).await?;
    let (_, root) = proxy
        .call::<_, _, (u32, RawNode)>("GetLayout", &(parent, depth, &[] as &[&str]))
        .await?;
    Ok(root.2.into_iter().filter_map(raw_item).collect())
}

async fn children(
    connection: &zbus::Connection,
    address: &AppMenuAddress,
    parent: i32,
    depth: i32,
) -> Option<Vec<RawItem>> {
    match layout(connection, address, parent, depth).await {
        Ok(items) => Some(items),
        Err(error) => {
            tracing::warn!(
                %error,
                parent,
                service = %address.service,
                "menubar: dbusmenu GetLayout failed"
            );
            None
        }
    }
}

async fn open_submenu(
    connection: &zbus::Connection,
    address: &AppMenuAddress,
    id: i32,
) -> Option<Vec<RawItem>> {
    if let Ok(proxy) = proxy(connection, address).await {
        match proxy.call::<_, _, bool>("AboutToShow", &(id,)).await {
            Ok(shown) => tracing::debug!(id, shown, "menubar: dbusmenu AboutToShow"),
            Err(error) => tracing::warn!(%error, id, "menubar: dbusmenu AboutToShow failed"),
        }
    }
    children(connection, address, id, 1).await
}

async fn event(
    connection: &zbus::Connection,
    address: &AppMenuAddress,
    id: i32,
    timestamp: u32,
) -> zbus::Result<()> {
    let proxy = proxy(connection, address).await?;
    proxy
        .call::<_, _, ()>(
            "Event",
            &(id, "clicked", zbus::zvariant::Value::I32(0), timestamp),
        )
        .await
}
