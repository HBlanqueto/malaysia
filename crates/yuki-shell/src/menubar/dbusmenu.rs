//! The app-menu D-Bus client and the `AppMenu.Registrar` owner (ADR 0040).
//!
//! One dedicated thread owns a single session connection. The render thread
//! sends FIFO commands and drains immutable snapshots from another channel;
//! it never waits for D-Bus. Registrar calls only need the shared map.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_channel::{Receiver, Sender};
use celestial_kit::platform::{AppMenuAddress, WakeHandle};
use futures_lite::{StreamExt, future};
use zbus::proxy::CacheProperties;
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
    commands: Sender<Command>,
    snapshots: Receiver<MenuSnapshot>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
}

impl MenuClient {
    pub fn focus(&self, address: AppMenuAddress, generation: u64) {
        let _ = self.commands.send_blocking(Command::Focus {
            address,
            generation,
        });
    }

    pub fn open(&self, address: AppMenuAddress, id: i32, generation: u64) {
        let _ = self.commands.send_blocking(Command::Open {
            address,
            id,
            generation,
        });
    }

    pub fn activate(&self, address: AppMenuAddress, id: i32, timestamp: u32) {
        let _ = self.commands.send_blocking(Command::Activate {
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
        self.snapshots.try_recv().ok()
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
    let (commands, command_rx) = async_channel::unbounded();
    let (snapshot_tx, snapshot_rx) = async_channel::unbounded();
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
        snapshots: snapshot_rx,
        wake,
    }
}

type ProxyCache = HashMap<AppMenuAddress, zbus::Proxy<'static>>;

enum Event {
    Command(Result<Command, async_channel::RecvError>),
    Owner(Option<zbus::fdo::NameOwnerChanged>),
}

async fn run(
    commands: Receiver<Command>,
    snapshots: Sender<MenuSnapshot>,
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

    let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            tracing::error!(%error, "menubar: could not create the bus proxy");
            return;
        }
    };
    let mut owner_changes = match dbus.receive_name_owner_changed().await {
        Ok(stream) => stream,
        Err(error) => {
            tracing::error!(%error, "menubar: could not listen for name changes");
            return;
        }
    };

    let mut proxies: ProxyCache = HashMap::new();
    let mut focused: Option<(AppMenuAddress, u64)> = None;
    loop {
        let selected = future::or(
            async { Event::Command(commands.recv().await) },
            async { Event::Owner(owner_changes.next().await) },
        )
        .await;
        match selected {
            Event::Command(Ok(Command::Focus {
                address,
                generation,
            })) => {
                focused = Some((address.clone(), generation));
                let started = std::time::Instant::now();
                let items = children(&connection, &mut proxies, &address, 0, 1).await;
                let count = items.as_ref().map_or(0, Vec::len);
                tracing::info!(
                    generation,
                    service = %address.service,
                    titles = count,
                    ms = started.elapsed().as_millis(),
                    "menubar: dbusmenu root snapshot"
                );
                send(
                    &snapshots,
                    &wake,
                    MenuSnapshot {
                        generation,
                        address,
                        parent: None,
                        items,
                    },
                )
                .await;
            }
            Event::Command(Ok(Command::Open {
                address,
                id,
                generation,
            })) => {
                let started = std::time::Instant::now();
                let items = open_submenu(&connection, &mut proxies, &address, id).await;
                let count = items.as_ref().map_or(0, Vec::len);
                tracing::info!(
                    generation,
                    id,
                    service = %address.service,
                    items = count,
                    ms = started.elapsed().as_millis(),
                    "menubar: dbusmenu submenu snapshot"
                );
                send(
                    &snapshots,
                    &wake,
                    MenuSnapshot {
                        generation,
                        address,
                        parent: Some(id),
                        items,
                    },
                )
                .await;
            }
            Event::Command(Ok(Command::Activate {
                address,
                id,
                timestamp,
            })) => {
                if let Err(error) =
                    event(&connection, &mut proxies, &address, id, timestamp).await
                {
                    tracing::warn!(%error, id, service = %address.service, "menubar: dbusmenu Event failed");
                } else {
                    tracing::info!(id, service = %address.service, "menubar: dbusmenu Event sent");
                }
            }
            Event::Command(Err(_)) => break,
            Event::Owner(Some(signal)) => {
                let Ok(args) = signal.args() else {
                    continue;
                };
                if args.new_owner().is_some() {
                    continue;
                }
                let service = args.name().to_string();
                proxies.retain(|address, _| address.service != service);
                if let Some((address, generation)) = &focused
                    && address.service == service
                {
                    tracing::info!(%service, generation, "menubar: menu service vanished");
                    send(
                        &snapshots,
                        &wake,
                        MenuSnapshot {
                            generation: *generation,
                            address: address.clone(),
                            parent: None,
                            items: None,
                        },
                    )
                    .await;
                }
            }
            Event::Owner(None) => break,
        }
    }
}

async fn send(
    snapshots: &Sender<MenuSnapshot>,
    wake: &Arc<Mutex<Option<WakeHandle>>>,
    snapshot: MenuSnapshot,
) {
    let _ = snapshots.send(snapshot).await;
    notify(wake);
}

fn notify(wake: &Arc<Mutex<Option<WakeHandle>>>) {
    if let Some(wake) = wake.lock().ok().and_then(|slot| slot.clone()) {
        wake.wake();
    }
}

async fn proxy(
    connection: &zbus::Connection,
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
) -> zbus::Result<zbus::Proxy<'static>> {
    if let Some(proxy) = proxies.get(address) {
        return Ok(proxy.clone());
    }
    let proxy: zbus::Proxy<'static> = zbus::proxy::Builder::new(connection)
        .destination(address.service.clone())?
        .path(address.path.clone())?
        .interface(DBUSMENU_INTERFACE)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    proxies.insert(address.clone(), proxy.clone());
    Ok(proxy)
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
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
    parent: i32,
    depth: i32,
) -> zbus::Result<Vec<RawItem>> {
    let proxy = proxy(connection, proxies, address).await?;
    let (_, root) = proxy
        .call::<_, _, (u32, RawNode)>("GetLayout", &(parent, depth, &[] as &[&str]))
        .await?;
    Ok(root.2.into_iter().filter_map(raw_item).collect())
}

async fn children(
    connection: &zbus::Connection,
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
    parent: i32,
    depth: i32,
) -> Option<Vec<RawItem>> {
    match layout(connection, proxies, address, parent, depth).await {
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
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
    id: i32,
) -> Option<Vec<RawItem>> {
    if let Ok(proxy) = proxy(connection, proxies, address).await {
        match proxy.call::<_, _, bool>("AboutToShow", &(id,)).await {
            Ok(shown) => tracing::debug!(id, shown, "menubar: dbusmenu AboutToShow"),
            Err(error) => tracing::warn!(%error, id, "menubar: dbusmenu AboutToShow failed"),
        }
    }
    children(connection, proxies, address, id, 1).await
}

async fn event(
    connection: &zbus::Connection,
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
    id: i32,
    timestamp: u32,
) -> zbus::Result<()> {
    let proxy = proxy(connection, proxies, address).await?;
    proxy
        .call::<_, _, ()>(
            "Event",
            &(id, "clicked", zbus::zvariant::Value::I32(0), timestamp),
        )
        .await
}
