//! The app-menu D-Bus client and the `AppMenu.Registrar` owner (ADR 0040).
//!
//! One dedicated thread owns a single session connection. The render thread
//! sends FIFO commands and drains immutable snapshots from another channel;
//! it never waits for D-Bus. Registrar calls only need the shared map.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender};
use celestial_kit::platform::{AppMenuAddress, WakeHandle};
use futures_lite::{StreamExt, future};
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub const REGISTRAR_NAME: &str = "com.canonical.AppMenu.Registrar";
pub const REGISTRAR_PATH: &str = "/com/canonical/AppMenu/Registrar";
const DBUSMENU_INTERFACE: &str = "com.canonical.dbusmenu";
const CALL_TIMEOUT: Duration = Duration::from_millis(500);
const DEBOUNCE: Duration = Duration::from_millis(50);

static RENDER_THREAD: OnceLock<ThreadId> = OnceLock::new();
static RENDER_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Number of D-Bus calls made from the render thread during this session.
pub fn render_calls() -> usize {
    RENDER_CALLS.load(Ordering::Relaxed)
}

fn note_call() {
    if RENDER_THREAD.get() == Some(&std::thread::current().id()) {
        RENDER_CALLS.fetch_add(1, Ordering::Relaxed);
        debug_assert!(false, "D-Bus call on the render thread");
    }
}

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
    /// Fetches the whole tree for the Help search.
    FullTree {
        address: AppMenuAddress,
        generation: u64,
    },
    Activate {
        address: AppMenuAddress,
        id: i32,
        timestamp: u32,
    },
    /// Enables or disables the global menu at runtime.
    SetAppMenus(bool),
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
    pub cached: bool,
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

    pub fn full_tree(&self, address: AppMenuAddress, generation: u64) {
        let _ = self.commands.send_blocking(Command::FullTree {
            address,
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

    /// Enables or disables the global menu on the D-Bus thread.
    pub fn set_app_menus(&self, enabled: bool) {
        let _ = self.commands.send_blocking(Command::SetAppMenus(enabled));
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

/// Owns or releases the `AppMenu.Registrar` name and its object.
async fn set_registrar(connection: &zbus::Connection, windows: &RegistrarWindows, enabled: bool) {
    if enabled {
        match connection.request_name(REGISTRAR_NAME).await {
            Ok(_) => {
                tracing::info!(name = REGISTRAR_NAME, "menubar: Registrar acquired");
                let registrar = Registrar {
                    windows: windows.clone(),
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
    } else {
        match connection.release_name(REGISTRAR_NAME).await {
            Ok(released) => {
                tracing::info!(
                    name = REGISTRAR_NAME,
                    released,
                    "menubar: Registrar released"
                );
            }
            Err(error) => tracing::warn!(%error, "menubar: could not release the Registrar"),
        }
        if let Err(error) = connection
            .object_server()
            .remove::<Registrar, _>(REGISTRAR_PATH)
            .await
        {
            tracing::warn!(%error, "menubar: could not remove the Registrar object");
        }
    }
}

/// Starts the D-Bus thread. On connection failure the client keeps accepting
/// commands and simply never answers; the bar stays on the 14c fallback.
pub fn start(app_menus: bool) -> MenuClient {
    RENDER_THREAD.get_or_init(|| std::thread::current().id());
    let (commands, command_rx) = async_channel::unbounded();
    let (snapshot_tx, snapshot_rx) = async_channel::unbounded();
    let wake = Arc::new(Mutex::new(None));
    let thread_wake = wake.clone();
    std::thread::Builder::new()
        .name("yuki-dbusmenu".to_string())
        .spawn(move || {
            async_io::block_on(run(command_rx, snapshot_tx, thread_wake, app_menus));
        })
        .expect("yuki-dbusmenu thread");
    MenuClient {
        commands,
        snapshots: snapshot_rx,
        wake,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CachedSubtree {
    revision: u32,
    items: Vec<RawItem>,
}

#[derive(Default)]
struct MenuCache {
    subtrees: HashMap<i32, CachedSubtree>,
    /// The whole tree fetched for the Help search, kept until any
    /// `LayoutUpdated` for this address reports a newer revision.
    full: Option<CachedSubtree>,
}

impl MenuCache {
    fn get(&self, id: i32) -> Option<&CachedSubtree> {
        self.subtrees.get(&id)
    }

    fn put(&mut self, id: i32, revision: u32, items: Vec<RawItem>) {
        self.subtrees.insert(id, CachedSubtree { revision, items });
    }

    fn full(&self) -> Option<&CachedSubtree> {
        self.full.as_ref()
    }

    fn put_full(&mut self, revision: u32, items: Vec<RawItem>) {
        self.full = Some(CachedSubtree { revision, items });
    }

    /// Drops only `id` when `revision` is newer than the cached one.
    fn invalidate(&mut self, id: i32, revision: u32) -> bool {
        if self
            .subtrees
            .get(&id)
            .is_some_and(|subtree| revision > subtree.revision)
        {
            self.subtrees.remove(&id);
            true
        } else {
            false
        }
    }

    /// Any `LayoutUpdated` with a newer revision invalidates the full tree.
    fn invalidate_full(&mut self, revision: u32) -> bool {
        if self
            .full
            .as_ref()
            .is_some_and(|full| revision > full.revision)
        {
            self.full = None;
            true
        } else {
            false
        }
    }

    fn apply_properties(
        &mut self,
        updated: &[(i32, HashMap<String, OwnedValue>)],
        removed: &[(i32, Vec<String>)],
    ) -> bool {
        let mut changed = false;
        for subtree in self.subtrees.values_mut() {
            changed |= apply_properties(&mut subtree.items, updated, removed);
        }
        changed
    }
}

fn apply_properties(
    items: &mut [RawItem],
    updated: &[(i32, HashMap<String, OwnedValue>)],
    removed: &[(i32, Vec<String>)],
) -> bool {
    let mut changed = false;
    for item in items {
        if let Some((_, properties)) = updated.iter().find(|(id, _)| *id == item.id) {
            item.properties.extend(properties.clone());
            changed = true;
        }
        if let Some((_, keys)) = removed.iter().find(|(id, _)| *id == item.id) {
            for key in keys {
                changed |= item.properties.remove(key).is_some();
            }
        }
        changed |= apply_properties(&mut item.children, updated, removed);
    }
    changed
}

#[derive(Default)]
struct MenuCaches {
    menus: HashMap<AppMenuAddress, MenuCache>,
}

impl MenuCaches {
    fn get(&self, address: &AppMenuAddress) -> Option<&MenuCache> {
        self.menus.get(address)
    }

    fn get_mut(&mut self, address: &AppMenuAddress) -> Option<&mut MenuCache> {
        self.menus.get_mut(address)
    }

    fn entry(&mut self, address: &AppMenuAddress) -> &mut MenuCache {
        self.menus.entry(address.clone()).or_default()
    }

    fn evict_service(&mut self, service: &str) -> bool {
        let before = self.menus.len();
        self.menus.retain(|address, _| address.service != service);
        before != self.menus.len()
    }
}

struct AddressProxy {
    proxy: zbus::Proxy<'static>,
    owner: Option<String>,
}

type ProxyCache = HashMap<AppMenuAddress, AddressProxy>;

#[derive(Default)]
struct Pending {
    focus: Option<(AppMenuAddress, u64)>,
    subtrees: Vec<i32>,
    snapshot: bool,
}

enum Event {
    Command(Result<Command, async_channel::RecvError>),
    Owner(Option<zbus::fdo::NameOwnerChanged>),
    Menu(Option<Result<zbus::Message, zbus::Error>>),
    Timer,
}

async fn run(
    commands: Receiver<Command>,
    snapshots: Sender<MenuSnapshot>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
    initial_app_menus: bool,
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

    let windows: RegistrarWindows = Arc::new(Mutex::new(HashMap::new()));
    let mut app_menus = initial_app_menus;
    set_registrar(&connection, &windows, app_menus).await;

    note_call();
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
    let mut menu_signals = match menu_stream(&connection).await {
        Ok(stream) => stream,
        Err(error) => {
            tracing::error!(%error, "menubar: could not listen for dbusmenu signals");
            return;
        }
    };

    let mut proxies: ProxyCache = HashMap::new();
    let mut caches = MenuCaches::default();
    let mut focused: Option<(AppMenuAddress, u64)> = None;
    let mut pending = Pending::default();
    let mut deadline: Option<Instant> = None;
    loop {
        let selected = if let Some(at) = deadline {
            future::or(
                future::or(async { Event::Command(commands.recv().await) }, async {
                    Event::Owner(owner_changes.next().await)
                }),
                future::or(async { Event::Menu(menu_signals.next().await) }, async {
                    async_io::Timer::at(at).await;
                    Event::Timer
                }),
            )
            .await
        } else {
            future::or(
                future::or(async { Event::Command(commands.recv().await) }, async {
                    Event::Owner(owner_changes.next().await)
                }),
                async { Event::Menu(menu_signals.next().await) },
            )
            .await
        };
        match selected {
            Event::Command(Ok(Command::SetAppMenus(enabled))) => {
                if enabled == app_menus {
                    continue;
                }
                app_menus = enabled;
                pending = Pending::default();
                deadline = None;
                if !enabled {
                    focused = None;
                }
                tracing::info!(enabled, "menubar: app_menus changed");
                set_registrar(&connection, &windows, enabled).await;
            }
            Event::Command(Ok(Command::Focus {
                address,
                generation,
            })) => {
                if !app_menus {
                    continue;
                }
                focused = Some((address.clone(), generation));
                pending = Pending::default();
                deadline = None;
                if let Some(root) = caches.get(&address).and_then(|cache| cache.get(0)) {
                    tracing::info!(
                        generation,
                        service = %address.service,
                        titles = root.items.len(),
                        cached = true,
                        "menubar: dbusmenu root snapshot"
                    );
                    send(
                        &snapshots,
                        &wake,
                        MenuSnapshot {
                            generation,
                            address,
                            parent: None,
                            items: Some(root.items.clone()),
                            cached: true,
                        },
                    )
                    .await;
                } else {
                    pending.focus = Some((address, generation));
                    arm(&mut deadline);
                }
            }
            Event::Command(Ok(Command::Open {
                address,
                id,
                generation,
            })) => {
                if !app_menus {
                    continue;
                }
                let started = Instant::now();
                let cached = caches
                    .get(&address)
                    .and_then(|cache| cache.get(id))
                    .cloned();
                let shown = about_to_show(&connection, &dbus, &mut proxies, &address, id).await;
                let had_cache = cached.is_some();
                let items = if shown || cached.is_none() {
                    fetch_subtree(&connection, &dbus, &mut proxies, &mut caches, &address, id).await
                } else {
                    cached.map(|subtree| subtree.items)
                };
                pending.subtrees.retain(|pending| *pending != id);
                let from_cache = !shown && had_cache;
                let count = items.as_ref().map_or(0, Vec::len);
                tracing::info!(
                    generation,
                    id,
                    service = %address.service,
                    items = count,
                    cached = from_cache,
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
                        cached: from_cache,
                    },
                )
                .await;
            }
            Event::Command(Ok(Command::FullTree {
                address,
                generation,
            })) => {
                if !app_menus {
                    continue;
                }
                let started = Instant::now();
                let cached = caches.get(&address).and_then(|cache| cache.full()).cloned();
                let (items, from_cache) = match cached {
                    Some(full) => (Some(full.items), true),
                    None => match layout(&connection, &dbus, &mut proxies, &address, 0, -1).await {
                        Ok((revision, items)) => {
                            caches.entry(&address).put_full(revision, items.clone());
                            (Some(items), false)
                        }
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                service = %address.service,
                                "menubar: dbusmenu full GetLayout failed"
                            );
                            (None, false)
                        }
                    },
                };
                if let Some(items) = items {
                    let count = items.len();
                    tracing::info!(
                        generation,
                        service = %address.service,
                        items = count,
                        cached = from_cache,
                        ms = started.elapsed().as_millis(),
                        "menubar: dbusmenu full tree snapshot"
                    );
                    send(
                        &snapshots,
                        &wake,
                        MenuSnapshot {
                            generation,
                            address,
                            parent: None,
                            items: Some(items),
                            cached: from_cache,
                        },
                    )
                    .await;
                }
            }
            Event::Command(Ok(Command::Activate {
                address,
                id,
                timestamp,
            })) => {
                if let Err(error) =
                    event(&connection, &dbus, &mut proxies, &address, id, timestamp).await
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
                let evicted = caches.evict_service(&service);
                if let Some((address, generation)) = &focused
                    && address.service == service
                {
                    tracing::info!(%service, generation, evicted, "menubar: menu service vanished");
                    pending = Pending::default();
                    deadline = None;
                    send(
                        &snapshots,
                        &wake,
                        MenuSnapshot {
                            generation: *generation,
                            address: address.clone(),
                            parent: None,
                            items: None,
                            cached: false,
                        },
                    )
                    .await;
                }
            }
            Event::Owner(None) => break,
            Event::Menu(Some(Ok(message))) => {
                handle_signal(
                    message,
                    &proxies,
                    &mut caches,
                    &focused,
                    &mut pending,
                    &mut deadline,
                );
            }
            Event::Menu(Some(Err(error))) => {
                tracing::warn!(%error, "menubar: dbusmenu signal stream error");
            }
            Event::Menu(None) => break,
            Event::Timer => {
                deadline = None;
                flush(
                    &connection,
                    &dbus,
                    &mut proxies,
                    &mut caches,
                    &snapshots,
                    &wake,
                    &focused,
                    &mut pending,
                )
                .await;
            }
        }
    }
}

fn arm(deadline: &mut Option<Instant>) {
    if deadline.is_none() {
        *deadline = Some(Instant::now() + DEBOUNCE);
    }
}

fn signal_address(proxies: &ProxyCache, sender: &str, path: &str) -> Option<AppMenuAddress> {
    proxies.iter().find_map(|(address, entry)| {
        (address.path == path && entry.owner.as_deref() == Some(sender)).then(|| address.clone())
    })
}

fn handle_signal(
    message: zbus::Message,
    proxies: &ProxyCache,
    caches: &mut MenuCaches,
    focused: &Option<(AppMenuAddress, u64)>,
    pending: &mut Pending,
    deadline: &mut Option<Instant>,
) {
    let Some(sender) = message.header().sender().map(ToString::to_string) else {
        return;
    };
    let Some(path) = message.header().path().map(ToString::to_string) else {
        return;
    };
    let Some(address) = signal_address(proxies, &sender, &path) else {
        return;
    };
    let is_focused = focused
        .as_ref()
        .is_some_and(|(focused, _)| *focused == address);
    match message.header().member().map(|member| member.as_str()) {
        Some("LayoutUpdated") => {
            let Ok((revision, parent)) = message.body().deserialize::<(u32, i32)>() else {
                return;
            };
            let (invalidated, full_invalidated) = caches
                .get_mut(&address)
                .map(|cache| {
                    (
                        cache.invalidate(parent, revision),
                        cache.invalidate_full(revision),
                    )
                })
                .unwrap_or_default();
            tracing::debug!(
                service = %address.service,
                revision,
                parent,
                invalidated,
                full_invalidated,
                "menubar: dbusmenu LayoutUpdated"
            );
            if invalidated && is_focused {
                pending.subtrees.push(parent);
                arm(deadline);
            }
        }
        Some("ItemsPropertiesUpdated") => {
            let Ok((updated, removed)) = message.body().deserialize::<(
                Vec<(i32, HashMap<String, OwnedValue>)>,
                Vec<(i32, Vec<String>)>,
            )>() else {
                return;
            };
            let changed = caches
                .get_mut(&address)
                .is_some_and(|cache| cache.apply_properties(&updated, &removed));
            tracing::info!(
                service = %address.service,
                ?updated,
                ?removed,
                changed,
                "menubar: dbusmenu ItemsPropertiesUpdated"
            );
            if changed && is_focused {
                pending.snapshot = true;
                arm(deadline);
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
async fn flush(
    connection: &zbus::Connection,
    dbus: &zbus::fdo::DBusProxy<'_>,
    proxies: &mut ProxyCache,
    caches: &mut MenuCaches,
    snapshots: &Sender<MenuSnapshot>,
    wake: &Arc<Mutex<Option<WakeHandle>>>,
    focused: &Option<(AppMenuAddress, u64)>,
    pending: &mut Pending,
) {
    let Some((address, generation)) = focused.clone() else {
        *pending = Pending::default();
        return;
    };

    let focus_fetch = pending
        .focus
        .take()
        .is_some_and(|(pending, pending_generation)| {
            pending == address && pending_generation == generation
        });
    if focus_fetch {
        if let Some(root) = caches.get(&address).and_then(|cache| cache.get(0)) {
            send(
                snapshots,
                wake,
                MenuSnapshot {
                    generation,
                    address: address.clone(),
                    parent: None,
                    items: Some(root.items.clone()),
                    cached: true,
                },
            )
            .await;
        } else {
            fetch_root(
                connection, dbus, proxies, caches, snapshots, wake, &address, generation,
            )
            .await;
        }
    }

    let mut subtrees = std::mem::take(&mut pending.subtrees);
    subtrees.sort_unstable();
    subtrees.dedup();
    for id in subtrees {
        if id == 0 {
            if caches
                .get(&address)
                .and_then(|cache| cache.get(0))
                .is_none()
            {
                fetch_root(
                    connection, dbus, proxies, caches, snapshots, wake, &address, generation,
                )
                .await;
            }
            send_cached_subtrees(caches, snapshots, wake, &address, generation).await;
        } else {
            let items = fetch_subtree(connection, dbus, proxies, caches, &address, id).await;
            send(
                snapshots,
                wake,
                MenuSnapshot {
                    generation,
                    address: address.clone(),
                    parent: Some(id),
                    items,
                    cached: false,
                },
            )
            .await;
        }
    }

    if pending.snapshot {
        pending.snapshot = false;
        if let Some(root) = caches.get(&address).and_then(|cache| cache.get(0)) {
            send(
                snapshots,
                wake,
                MenuSnapshot {
                    generation,
                    address: address.clone(),
                    parent: None,
                    items: Some(root.items.clone()),
                    cached: true,
                },
            )
            .await;
            send_cached_subtrees(caches, snapshots, wake, &address, generation).await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn fetch_root(
    connection: &zbus::Connection,
    dbus: &zbus::fdo::DBusProxy<'_>,
    proxies: &mut ProxyCache,
    caches: &mut MenuCaches,
    snapshots: &Sender<MenuSnapshot>,
    wake: &Arc<Mutex<Option<WakeHandle>>>,
    address: &AppMenuAddress,
    generation: u64,
) {
    let started = Instant::now();
    let items = match layout(connection, dbus, proxies, address, 0, 1).await {
        Ok((revision, items)) => {
            caches.entry(address).put(0, revision, items.to_vec());
            Some(items)
        }
        Err(error) => {
            tracing::warn!(%error, service = %address.service, "menubar: dbusmenu GetLayout failed");
            None
        }
    };
    let count = items.as_ref().map_or(0, Vec::len);
    tracing::info!(
        generation,
        service = %address.service,
        titles = count,
        cached = false,
        ms = started.elapsed().as_millis(),
        "menubar: dbusmenu root snapshot"
    );
    send(
        snapshots,
        wake,
        MenuSnapshot {
            generation,
            address: address.clone(),
            parent: None,
            items,
            cached: false,
        },
    )
    .await;
}

async fn send_cached_subtrees(
    caches: &MenuCaches,
    snapshots: &Sender<MenuSnapshot>,
    wake: &Arc<Mutex<Option<WakeHandle>>>,
    address: &AppMenuAddress,
    generation: u64,
) {
    let Some(cache) = caches.get(address) else {
        return;
    };
    let mut ids: Vec<i32> = cache
        .subtrees
        .keys()
        .copied()
        .filter(|id| *id != 0)
        .collect();
    ids.sort_unstable();
    for id in ids {
        let Some(subtree) = cache.get(id) else {
            continue;
        };
        send(
            snapshots,
            wake,
            MenuSnapshot {
                generation,
                address: address.clone(),
                parent: Some(id),
                items: Some(subtree.items.clone()),
                cached: true,
            },
        )
        .await;
    }
}

async fn fetch_subtree(
    connection: &zbus::Connection,
    dbus: &zbus::fdo::DBusProxy<'_>,
    proxies: &mut ProxyCache,
    caches: &mut MenuCaches,
    address: &AppMenuAddress,
    id: i32,
) -> Option<Vec<RawItem>> {
    match layout(connection, dbus, proxies, address, id, 1).await {
        Ok((revision, items)) => {
            caches.entry(address).put(id, revision, items.to_vec());
            Some(items)
        }
        Err(error) => {
            tracing::warn!(%error, id, service = %address.service, "menubar: dbusmenu GetLayout failed");
            None
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

async fn menu_stream(connection: &zbus::Connection) -> zbus::Result<zbus::MessageStream> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(DBUSMENU_INTERFACE)?
        .build();
    zbus::MessageStream::for_match_rule(rule, connection, None).await
}

async fn proxy(
    connection: &zbus::Connection,
    dbus: &zbus::fdo::DBusProxy<'_>,
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
) -> zbus::Result<zbus::Proxy<'static>> {
    if let Some(entry) = proxies.get(address) {
        return Ok(entry.proxy.clone());
    }
    let proxy: zbus::Proxy<'static> = zbus::proxy::Builder::new(connection)
        .destination(address.service.clone())?
        .path(address.path.clone())?
        .interface(DBUSMENU_INTERFACE)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    note_call();
    let owner = match zbus::names::BusName::try_from(address.service.as_str()) {
        Ok(name) => dbus
            .get_name_owner(name)
            .await
            .ok()
            .map(|owner| owner.to_string()),
        Err(_) => None,
    };
    proxies.insert(
        address.clone(),
        AddressProxy {
            proxy: proxy.clone(),
            owner,
        },
    );
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
    dbus: &zbus::fdo::DBusProxy<'_>,
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
    parent: i32,
    depth: i32,
) -> zbus::Result<(u32, Vec<RawItem>)> {
    let proxy = proxy(connection, dbus, proxies, address).await?;
    note_call();
    let (revision, root) = proxy
        .call::<_, _, (u32, RawNode)>("GetLayout", &(parent, depth, &[] as &[&str]))
        .await?;
    Ok((revision, root.2.into_iter().filter_map(raw_item).collect()))
}

async fn about_to_show(
    connection: &zbus::Connection,
    dbus: &zbus::fdo::DBusProxy<'_>,
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
    id: i32,
) -> bool {
    let Ok(proxy) = proxy(connection, dbus, proxies, address).await else {
        return false;
    };
    note_call();
    match proxy.call::<_, _, bool>("AboutToShow", &(id,)).await {
        Ok(shown) => {
            tracing::debug!(id, shown, "menubar: dbusmenu AboutToShow");
            shown
        }
        Err(error) => {
            tracing::warn!(%error, id, "menubar: dbusmenu AboutToShow failed");
            false
        }
    }
}

async fn event(
    connection: &zbus::Connection,
    dbus: &zbus::fdo::DBusProxy<'_>,
    proxies: &mut ProxyCache,
    address: &AppMenuAddress,
    id: i32,
    timestamp: u32,
) -> zbus::Result<()> {
    let proxy = proxy(connection, dbus, proxies, address).await?;
    note_call();
    proxy
        .call::<_, _, ()>(
            "Event",
            &(id, "clicked", zbus::zvariant::Value::I32(0), timestamp),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> OwnedValue {
        OwnedValue::try_from(zbus::zvariant::Value::from(value)).unwrap()
    }

    fn item(id: i32, properties: &[(&str, OwnedValue)], children: Vec<RawItem>) -> RawItem {
        RawItem {
            id,
            properties: properties
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
            children,
        }
    }

    fn address(service: &str) -> AppMenuAddress {
        AppMenuAddress {
            service: service.to_string(),
            path: "/MenuBar/1".to_string(),
        }
    }

    #[test]
    fn a_cached_subtree_is_a_hit() {
        let mut cache = MenuCache::default();
        cache.put(7, 3, vec![item(8, &[], Vec::new())]);
        assert_eq!(cache.get(7).unwrap().revision, 3);
        assert_eq!(cache.get(7).unwrap().items[0].id, 8);
    }

    #[test]
    fn invalidation_drops_only_the_signalled_subtree() {
        let mut cache = MenuCache::default();
        cache.put(0, 1, vec![item(7, &[], Vec::new())]);
        cache.put(7, 1, vec![item(8, &[], Vec::new())]);
        assert!(cache.invalidate(7, 2));
        assert!(cache.get(7).is_none());
        assert!(cache.get(0).is_some());
        assert!(cache.invalidate(0, 2));
        assert!(cache.get(0).is_none());
    }

    #[test]
    fn an_older_revision_is_ignored() {
        let mut cache = MenuCache::default();
        cache.put(0, 9, vec![item(1, &[], Vec::new())]);
        assert!(!cache.invalidate(0, 8));
        assert!(cache.get(0).is_some());
    }

    #[test]
    fn any_newer_layout_update_invalidates_the_full_tree() {
        let mut cache = MenuCache::default();
        cache.put_full(3, vec![item(1, &[], Vec::new())]);
        assert!(!cache.invalidate_full(3));
        assert!(cache.full().is_some());
        assert!(cache.invalidate_full(4));
        assert!(cache.full().is_none());
    }

    #[test]
    fn items_properties_updated_changes_the_item_in_place() {
        let mut cache = MenuCache::default();
        cache.put(
            0,
            1,
            vec![item(
                4,
                &[("toggle-state", OwnedValue::from(0i32))],
                Vec::new(),
            )],
        );
        let updated = vec![(
            4,
            HashMap::from([("toggle-state".to_string(), OwnedValue::from(1i32))]),
        )];
        assert!(cache.apply_properties(&updated, &[]));
        assert_eq!(
            i32::try_from(cache.get(0).unwrap().items[0].properties["toggle-state"].clone())
                .unwrap(),
            1
        );
    }

    #[test]
    fn eviction_drops_every_entry_of_the_service() {
        let mut caches = MenuCaches::default();
        caches.entry(&address("org.kde.kate")).put(0, 1, Vec::new());
        caches.entry(&address("org.kde.kate")).put(7, 1, Vec::new());
        caches
            .entry(&address("org.mozilla.firefox"))
            .put(0, 1, Vec::new());
        assert!(caches.evict_service("org.kde.kate"));
        assert!(caches.get(&address("org.kde.kate")).is_none());
        assert!(caches.get(&address("org.mozilla.firefox")).is_some());
    }

    #[test]
    fn properties_updated_in_a_nested_subtree_are_found() {
        let mut cache = MenuCache::default();
        cache.put(0, 1, vec![item(4, &[("label", text("Ver"))], Vec::new())]);
        cache.put(
            4,
            1,
            vec![item(
                9,
                &[("toggle-state", OwnedValue::from(0i32))],
                Vec::new(),
            )],
        );
        let updated = vec![(
            9,
            HashMap::from([("toggle-state".to_string(), OwnedValue::from(1i32))]),
        )];
        assert!(cache.apply_properties(&updated, &[]));
        assert_eq!(
            i32::try_from(cache.get(4).unwrap().items[0].properties["toggle-state"].clone())
                .unwrap(),
            1
        );
    }
}
