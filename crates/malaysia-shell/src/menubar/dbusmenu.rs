use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender};
use bk::main_menu::layout::{DbusMenuLayout, LayoutItem};
use bk::platform::{MainMenuAddress, WakeHandle};
use bk::shell::GtkMenusAddress;
use futures_lite::{StreamExt, future};
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::menubar::gtkmenus;

pub const REGISTRAR_NAME: &str = "com.canonical.AppMenu.Registrar";
pub const REGISTRAR_PATH: &str = "/com/canonical/AppMenu/Registrar";
const DBUSMENU_INTERFACE: &str = "com.canonical.dbusmenu";
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_millis(500);
const FULL_TREE_TIMEOUT: Duration = Duration::from_secs(2);
const FULL_TREE_DELAY: Duration = Duration::from_secs(1);
const DEBOUNCE: Duration = Duration::from_millis(50);

#[derive(Clone, Debug)]
pub enum Command {
    Focus {
        address: MainMenuAddress,
        generation: u64,
    },
    GtkFocus {
        menus: GtkMenusAddress,
        app_name: String,
        generation: u64,
    },
    Unfocus,
    Open {
        address: MainMenuAddress,
        id: i32,
        generation: u64,
    },
    FullTree {
        address: MainMenuAddress,
        generation: u64,
    },
    Activate {
        address: MainMenuAddress,
        id: i32,
        timestamp: u32,
    },
    SetAppMenus(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RawItem {
    pub id: i32,
    pub properties: HashMap<String, OwnedValue>,
    pub children: Vec<RawItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuSnapshot {
    pub generation: u64,
    pub address: MainMenuAddress,
    pub parent: Option<i32>,
    pub items: Option<Vec<RawItem>>,
    pub cached: bool,
}

#[derive(Clone)]
pub struct MenuClient {
    commands: Sender<Command>,
    snapshots: Receiver<MenuSnapshot>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
}

impl MenuClient {
    pub fn focus(&self, address: MainMenuAddress, generation: u64) {
        let _ = self.commands.send_blocking(Command::Focus {
            address,
            generation,
        });
    }

    pub fn gtk_focus(&self, menus: GtkMenusAddress, app_name: String, generation: u64) {
        let _ = self.commands.send_blocking(Command::GtkFocus {
            menus,
            app_name,
            generation,
        });
    }

    pub fn unfocus(&self) {
        let _ = self.commands.send_blocking(Command::Unfocus);
    }

    pub fn open(&self, address: MainMenuAddress, id: i32, generation: u64) {
        let _ = self.commands.send_blocking(Command::Open {
            address,
            id,
            generation,
        });
    }

    pub fn full_tree(&self, address: MainMenuAddress, generation: u64) {
        let _ = self.commands.send_blocking(Command::FullTree {
            address,
            generation,
        });
    }

    pub fn activate(&self, address: MainMenuAddress, id: i32, timestamp: u32) {
        let _ = self.commands.send_blocking(Command::Activate {
            address,
            id,
            timestamp,
        });
    }

    pub fn set_app_menus(&self, enabled: bool) {
        let _ = self.commands.send_blocking(Command::SetAppMenus(enabled));
    }

    pub fn set_wake(&self, wake: WakeHandle) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(wake);
        }
    }

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

// Patched Electron announces its Wayland appmenu only while this name has an owner (nix/patches/electron-wayland-appmenu.patch).
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

pub fn start(app_menus_enabled: bool) -> MenuClient {
    let (commands, command_rx) = async_channel::unbounded();
    let (snapshot_tx, snapshot_rx) = async_channel::unbounded();
    let wake = Arc::new(Mutex::new(None));
    let thread_wake = wake.clone();
    std::thread::Builder::new()
        .name("malaysia-dbusmenu".to_string())
        .spawn(move || {
            async_io::block_on(run(command_rx, snapshot_tx, thread_wake, app_menus_enabled));
        })
        .expect("malaysia-dbusmenu thread");
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
    full: Option<CachedSubtree>,
}

impl MenuCache {
    fn put(&mut self, id: i32, revision: u32, items: Vec<RawItem>) {
        self.subtrees.insert(id, CachedSubtree { revision, items });
    }

    fn put_full(&mut self, revision: u32, items: Vec<RawItem>) {
        self.full = Some(CachedSubtree { revision, items });
    }

    fn invalidate(&mut self, parent: i32, revision: u32) -> Vec<i32> {
        // dbusmenu: `parent` "may not be the direct parent of the changed nodes".
        let candidates = match self.subtrees.get(&parent) {
            Some(subtree) if revision <= subtree.revision => return Vec::new(),
            Some(_) => self.descendants(parent),
            None => self.subtrees.keys().copied().collect(),
        };
        let mut dropped = Vec::new();
        self.subtrees.retain(|id, subtree| {
            let stale = candidates.contains(id) && revision > subtree.revision;
            if stale {
                dropped.push(*id);
            }
            !stale
        });
        dropped
    }

    fn descendants(&self, parent: i32) -> Vec<i32> {
        let mut ids = vec![parent];
        let mut next = 0;
        while let Some(&id) = ids.get(next) {
            next += 1;
            if let Some(subtree) = self.subtrees.get(&id) {
                collect_ids(&subtree.items, &mut ids);
            }
        }
        ids
    }

    fn invalidate_full(&mut self, revision: u32) -> bool {
        self.full.take_if(|full| revision > full.revision).is_some()
    }

    fn apply_properties(
        &mut self,
        updated: &[(i32, HashMap<String, OwnedValue>)],
        removed: &[(i32, Vec<String>)],
    ) -> bool {
        let mut changed = false;
        for subtree in self.subtrees.values_mut().chain(self.full.as_mut()) {
            changed |= apply_properties(&mut subtree.items, updated, removed);
        }
        changed
    }
}

fn collect_ids(items: &[RawItem], ids: &mut Vec<i32>) {
    for item in items {
        if !ids.contains(&item.id) {
            ids.push(item.id);
        }
        collect_ids(&item.children, ids);
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

type MenuCaches = HashMap<MainMenuAddress, MenuCache>;

struct AddressProxy {
    proxy: zbus::Proxy<'static>,
    owner: Option<String>,
}

type ProxyCache = HashMap<MainMenuAddress, AddressProxy>;

#[derive(Default)]
struct Pending {
    focus: Option<(MainMenuAddress, u64)>,
    subtrees: Vec<i32>,
    snapshot: bool,
    deadline: Option<Instant>,
}

impl Pending {
    fn arm(&mut self) {
        self.deadline
            .get_or_insert_with(|| Instant::now() + DEBOUNCE);
    }
}

#[derive(Default)]
struct Focus {
    current: Option<(MainMenuAddress, u64)>,
    pending: Pending,
    background: BackgroundFullTree,
}

#[derive(Default)]
struct BackgroundFullTree {
    wait: Option<(MainMenuAddress, u64, Instant)>,
}

impl BackgroundFullTree {
    fn schedule(&mut self, address: &MainMenuAddress, generation: u64, cached: bool, now: Instant) {
        if cached {
            self.cancel(address);
            return;
        }
        match &mut self.wait {
            Some((pending, pending_generation, _)) if pending == address => {
                *pending_generation = generation;
            }
            _ => {
                self.wait = Some((address.clone(), generation, now + FULL_TREE_DELAY));
            }
        }
    }

    fn cancel(&mut self, address: &MainMenuAddress) {
        self.wait.take_if(|(pending, _, _)| pending == address);
    }

    fn next(&self) -> Option<Instant> {
        self.wait.as_ref().map(|(_, _, at)| *at)
    }

    fn due(&mut self, now: Instant) -> Option<(MainMenuAddress, u64)> {
        self.wait
            .take_if(|(_, _, at)| *at <= now)
            .map(|(address, generation, _)| (address, generation))
    }
}

enum Event {
    Command(Result<Command, async_channel::RecvError>),
    Owner(Option<zbus::fdo::NameOwnerChanged>),
    Menu(Option<Result<zbus::Message, zbus::Error>>),
    Timer,
}

fn logged<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) -> Option<T> {
    result
        .inspect_err(|error| tracing::error!(%error, "menubar: could not {what}"))
        .ok()
}

async fn run(
    commands: Receiver<Command>,
    snapshots: Sender<MenuSnapshot>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
    app_menus_enabled: bool,
) -> Option<()> {
    let builder = logged(
        zbus::connection::Builder::session(),
        "build the session bus connection",
    )?;
    let connection = logged(
        builder.method_timeout(FULL_TREE_TIMEOUT).build().await,
        "connect to the session bus",
    )?;
    let windows = RegistrarWindows::default();
    set_registrar(&connection, &windows, app_menus_enabled).await;
    let dbus = logged(
        zbus::fdo::DBusProxy::new(&connection).await,
        "create the bus proxy",
    )?;
    let mut owner_changes = logged(
        dbus.receive_name_owner_changed().await,
        "listen for name changes",
    )?;
    let mut menu_signals = logged(
        menu_stream(&connection).await,
        "listen for dbusmenu signals",
    )?;

    let mut worker = Worker {
        connection,
        dbus,
        proxies: HashMap::new(),
        caches: HashMap::new(),
        snapshots,
        wake,
        windows,
        app_menus_enabled,
        focus: Focus::default(),
        gtk: None,
    };
    loop {
        let timer = worker
            .focus
            .pending
            .deadline
            .into_iter()
            .chain(worker.focus.background.next())
            .min();
        let selected = future::or(
            future::or(async { Event::Command(commands.recv().await) }, async {
                Event::Owner(owner_changes.next().await)
            }),
            future::or(async { Event::Menu(menu_signals.next().await) }, async {
                timer
                    .map_or_else(async_io::Timer::never, async_io::Timer::at)
                    .await;
                Event::Timer
            }),
        )
        .await;
        match selected {
            Event::Command(Ok(command)) => worker.on_command(command).await,
            Event::Owner(Some(signal)) => {
                if let Ok(args) = signal.args()
                    && args.new_owner().is_none()
                {
                    worker.service_vanished(&args.name().to_string()).await;
                }
            }
            Event::Menu(Some(Ok(message))) => worker.on_signal(message).await,
            Event::Menu(Some(Err(error))) => {
                tracing::warn!(%error, "menubar: dbusmenu signal stream error");
            }
            Event::Command(Err(_)) | Event::Owner(None) | Event::Menu(None) => return None,
            Event::Timer => worker.on_timer().await,
        }
    }
}

fn gtk_for<'a>(
    gtk: &'a Option<gtkmenus::Session>,
    address: &MainMenuAddress,
    generation: u64,
) -> Option<&'a gtkmenus::Session> {
    gtk.as_ref()
        .filter(|session| session.address == *address && session.generation == generation)
}

fn signal_address(proxies: &ProxyCache, sender: &str, path: &str) -> Option<MainMenuAddress> {
    proxies.iter().find_map(|(address, entry)| {
        (address.path == path && entry.owner.as_deref() == Some(sender)).then(|| address.clone())
    })
}

struct Worker {
    connection: zbus::Connection,
    dbus: zbus::fdo::DBusProxy<'static>,
    proxies: ProxyCache,
    caches: MenuCaches,
    snapshots: Sender<MenuSnapshot>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
    windows: RegistrarWindows,
    app_menus_enabled: bool,
    focus: Focus,
    gtk: Option<gtkmenus::Session>,
}

impl Worker {
    fn dbusmenu_signal(&mut self, message: zbus::Message) {
        let Some(sender) = message.header().sender().map(ToString::to_string) else {
            return;
        };
        let Some(path) = message.header().path().map(ToString::to_string) else {
            return;
        };
        let Some(address) = signal_address(&self.proxies, &sender, &path) else {
            return;
        };
        let is_focused = self
            .focus
            .current
            .as_ref()
            .is_some_and(|(focused, _)| *focused == address);
        match message.header().member().map(|member| member.as_str()) {
            Some("LayoutUpdated") => {
                let Ok((revision, parent)) = message.body().deserialize::<(u32, i32)>() else {
                    return;
                };
                let (dropped, full_invalidated) = self
                    .caches
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
                    ?dropped,
                    full_invalidated,
                    "menubar: dbusmenu LayoutUpdated"
                );
                let mut refetch = dropped;
                if !self.has_root(&address) {
                    refetch.push(0);
                }
                if is_focused && !refetch.is_empty() {
                    self.focus.pending.subtrees.extend(refetch);
                    self.focus.pending.arm();
                }
            }
            Some("ItemsPropertiesUpdated") => {
                let Ok((updated, removed)) = message.body().deserialize::<(
                    Vec<(i32, HashMap<String, OwnedValue>)>,
                    Vec<(i32, Vec<String>)>,
                )>() else {
                    return;
                };
                let changed = self
                    .caches
                    .get_mut(&address)
                    .is_some_and(|cache| cache.apply_properties(&updated, &removed));
                tracing::debug!(
                    service = %address.service,
                    ?updated,
                    ?removed,
                    changed,
                    "menubar: dbusmenu ItemsPropertiesUpdated"
                );
                if changed && is_focused {
                    self.focus.pending.snapshot = true;
                    self.focus.pending.arm();
                }
            }
            _ => {}
        }
    }

    async fn on_command(&mut self, command: Command) {
        match command {
            Command::SetAppMenus(enabled) => {
                if enabled == self.app_menus_enabled {
                    return;
                }
                self.app_menus_enabled = enabled;
                self.focus.pending = Pending::default();
                if !enabled {
                    self.focus = Focus::default();
                    self.end_gtk().await;
                }
                tracing::info!(enabled, "menubar: app_menus changed");
                set_registrar(&self.connection, &self.windows, enabled).await;
            }
            Command::Focus {
                address,
                generation,
            } => {
                if !self.app_menus_enabled {
                    return;
                }
                self.end_gtk().await;
                self.focus.current = Some((address.clone(), generation));
                self.focus.pending = Pending::default();
                self.focus.background.schedule(
                    &address,
                    generation,
                    self.has_full_tree(&address),
                    Instant::now(),
                );
                if !self.send_cached_root(&address, generation).await {
                    self.focus.pending.focus = Some((address, generation));
                    self.focus.pending.arm();
                }
            }
            Command::GtkFocus {
                menus,
                app_name,
                generation,
            } => {
                if !self.app_menus_enabled {
                    return;
                }
                self.focus = Focus::default();
                self.end_gtk().await;
                let session =
                    gtkmenus::Session::open(&self.connection, menus, app_name, generation).await;
                self.send_gtk(&session, 0, false).await;
                self.gtk = Some(session);
            }
            Command::Unfocus => {
                self.focus = Focus::default();
                self.end_gtk().await;
            }
            Command::Open {
                address,
                id,
                generation,
            } => {
                if !self.app_menus_enabled {
                    return;
                }
                if let Some(session) = gtk_for(&self.gtk, &address, generation) {
                    self.send_gtk(session, id, true).await;
                    return;
                }
                self.focus.pending.subtrees.retain(|pending| *pending != id);
                self.send_subtree(address, id, generation).await;
            }
            Command::FullTree {
                address,
                generation,
            } => {
                if !self.app_menus_enabled {
                    return;
                }
                if let Some(session) = gtk_for(&self.gtk, &address, generation) {
                    self.send_gtk(session, 0, true).await;
                    return;
                }
                let cached = self.has_full_tree(&address);
                self.send_full_tree(&address, generation).await;
                if !cached && self.has_full_tree(&address) {
                    self.focus.background.cancel(&address);
                }
            }
            Command::Activate {
                address,
                id,
                timestamp,
            } => {
                if let Some(session) = self
                    .gtk
                    .as_ref()
                    .filter(|session| session.address == address)
                {
                    session.activate(&self.connection, id).await;
                    return;
                }
                if let Err(error) = self.event(&address, id, timestamp).await {
                    tracing::warn!(%error, id, service = %address.service, "menubar: dbusmenu Event failed");
                } else {
                    tracing::info!(id, service = %address.service, "menubar: dbusmenu Event sent");
                }
            }
        }
    }

    async fn on_signal(&mut self, message: zbus::Message) {
        let gtk_signal = message
            .header()
            .interface()
            .is_some_and(|interface| interface.starts_with("org.gtk."));
        if !gtk_signal {
            self.dbusmenu_signal(message);
        } else if let Some(session) = self.gtk.as_mut()
            && session.signal(&self.connection, &message).await
            && let Some(session) = &self.gtk
        {
            self.send_gtk(session, 0, false).await;
        }
    }

    async fn on_timer(&mut self) {
        let now = Instant::now();
        if self.focus.pending.deadline.is_some_and(|at| at <= now) {
            let pending = std::mem::take(&mut self.focus.pending);
            self.flush(pending).await;
        }
        if let Some((address, generation)) = self.focus.background.due(now)
            && self.focus.current == Some((address.clone(), generation))
            && !self.has_full_tree(&address)
        {
            self.send_full_tree(&address, generation).await;
        }
    }

    async fn end_gtk(&mut self) {
        if let Some(session) = self.gtk.take() {
            session.close(&self.connection).await;
        }
    }

    async fn flush(&mut self, mut pending: Pending) {
        let focused = self.focus.current.clone();
        let Some((address, generation)) = focused.clone() else {
            return;
        };

        if pending.focus.take() == focused && !self.send_cached_root(&address, generation).await {
            self.fetch_root(&address, generation).await;
        }

        let mut subtrees = std::mem::take(&mut pending.subtrees);
        subtrees.sort_unstable();
        subtrees.dedup();
        for id in subtrees {
            if id == 0 {
                if !self.has_root(&address) {
                    self.fetch_root(&address, generation).await;
                }
                self.send_cached_subtrees(&address, generation).await;
            } else {
                let items = self.fetch_subtree(&address, id).await;
                self.send(MenuSnapshot {
                    generation,
                    address: address.clone(),
                    parent: Some(id),
                    items,
                    cached: false,
                })
                .await;
            }
        }

        if std::mem::take(&mut pending.snapshot)
            && self.send_cached_root(&address, generation).await
        {
            self.send_cached_subtrees(&address, generation).await;
        }
    }

    async fn send_subtree(&mut self, address: MainMenuAddress, id: i32, generation: u64) {
        let started = Instant::now();
        let cached = self
            .caches
            .get(&address)
            .and_then(|cache| cache.subtrees.get(&id))
            .map(|subtree| subtree.items.clone());
        let needs_update = self.about_to_show(&address, id).await;
        let from_cache = !needs_update && cached.is_some();
        let items = if from_cache {
            cached
        } else {
            self.fetch_subtree(&address, id).await
        };
        tracing::info!(
            generation,
            id,
            service = %address.service,
            items = items.as_ref().map_or(0, Vec::len),
            cached = from_cache,
            ms = started.elapsed().as_millis(),
            "menubar: dbusmenu submenu snapshot"
        );
        self.send(MenuSnapshot {
            generation,
            address,
            parent: Some(id),
            items,
            cached: from_cache,
        })
        .await;
    }

    async fn service_vanished(&mut self, service: &str) {
        if let Some(session) = self
            .gtk
            .take_if(|session| session.menus.unique_bus_name == service)
        {
            tracing::info!(%service, "menubar: GTK menu service vanished");
            self.send(MenuSnapshot {
                generation: session.generation,
                address: session.address,
                parent: None,
                items: None,
                cached: false,
            })
            .await;
        }
        self.proxies.retain(|address, _| address.service != service);
        let cached_menus = self.caches.len();
        self.caches.retain(|address, _| address.service != service);
        if let Ok(mut windows) = self.windows.lock() {
            windows.retain(|_, (owner, _)| owner != service);
        }
        let evicted = cached_menus != self.caches.len();
        if let Some((address, generation)) = self.focus.current.clone()
            && address.service == service
        {
            tracing::info!(%service, generation, evicted, "menubar: menu service vanished");
            self.focus.pending = Pending::default();
            self.send(MenuSnapshot {
                generation,
                address,
                parent: None,
                items: None,
                cached: false,
            })
            .await;
        }
    }

    async fn send_cached_root(&self, address: &MainMenuAddress, generation: u64) -> bool {
        let Some(root) = self
            .caches
            .get(address)
            .and_then(|cache| cache.subtrees.get(&0))
        else {
            return false;
        };
        tracing::info!(
            generation,
            service = %address.service,
            titles = root.items.len(),
            cached = true,
            "menubar: dbusmenu root snapshot"
        );
        self.send(MenuSnapshot {
            generation,
            address: address.clone(),
            parent: None,
            items: Some(root.items.clone()),
            cached: true,
        })
        .await;
        true
    }

    async fn fetch_root(&mut self, address: &MainMenuAddress, generation: u64) {
        let started = Instant::now();
        let items = self.fetch_subtree(address, 0).await;
        tracing::info!(
            generation,
            service = %address.service,
            titles = items.as_ref().map_or(0, Vec::len),
            cached = false,
            ms = started.elapsed().as_millis(),
            "menubar: dbusmenu root snapshot"
        );
        self.send(MenuSnapshot {
            generation,
            address: address.clone(),
            parent: None,
            items,
            cached: false,
        })
        .await;
    }

    async fn send_cached_subtrees(&self, address: &MainMenuAddress, generation: u64) {
        let Some(cache) = self.caches.get(address) else {
            return;
        };
        let mut subtrees: Vec<_> = cache.subtrees.iter().filter(|(id, _)| **id != 0).collect();
        subtrees.sort_unstable_by_key(|(id, _)| **id);
        for (id, subtree) in subtrees {
            self.send(MenuSnapshot {
                generation,
                address: address.clone(),
                parent: Some(*id),
                items: Some(subtree.items.clone()),
                cached: true,
            })
            .await;
        }
    }

    async fn fetch_subtree(&mut self, address: &MainMenuAddress, id: i32) -> Option<Vec<RawItem>> {
        match self.layout(address, id, 1, CALL_TIMEOUT).await {
            Ok((revision, items)) => {
                self.caches
                    .entry(address.clone())
                    .or_default()
                    .put(id, revision, items.clone());
                Some(items)
            }
            Err(error) => {
                tracing::warn!(%error, id, service = %address.service, "menubar: dbusmenu GetLayout failed");
                None
            }
        }
    }

    fn has_root(&self, address: &MainMenuAddress) -> bool {
        self.caches
            .get(address)
            .is_some_and(|cache| cache.subtrees.contains_key(&0))
    }

    fn has_full_tree(&self, address: &MainMenuAddress) -> bool {
        self.caches
            .get(address)
            .is_some_and(|cache| cache.full.is_some())
    }

    async fn send_full_tree(&mut self, address: &MainMenuAddress, generation: u64) {
        let started = Instant::now();
        let cached = self
            .caches
            .get(address)
            .and_then(|cache| cache.full.as_ref())
            .map(|full| full.items.clone());
        let from_cache = cached.is_some();
        let items = match cached {
            Some(items) => items,
            None => match self.layout(address, 0, -1, FULL_TREE_TIMEOUT).await {
                Ok((revision, items)) => {
                    self.caches
                        .entry(address.clone())
                        .or_default()
                        .put_full(revision, items.clone());
                    items
                }
                Err(error) => {
                    tracing::warn!(
                        %error,
                        service = %address.service,
                        "menubar: dbusmenu full GetLayout failed"
                    );
                    return;
                }
            },
        };
        tracing::info!(
            generation,
            service = %address.service,
            items = items.len(),
            cached = from_cache,
            ms = started.elapsed().as_millis(),
            "menubar: dbusmenu full tree snapshot"
        );
        self.send(MenuSnapshot {
            generation,
            address: address.clone(),
            parent: None,
            items: Some(items),
            cached: from_cache,
        })
        .await;
    }

    async fn send_gtk(&self, session: &gtkmenus::Session, id: i32, cached: bool) {
        self.send(MenuSnapshot {
            generation: session.generation,
            address: session.address.clone(),
            parent: (id != 0).then_some(id),
            items: session.tree.children(id),
            cached,
        })
        .await;
    }

    async fn send(&self, snapshot: MenuSnapshot) {
        let _ = self.snapshots.send(snapshot).await;
        if let Some(wake) = self.wake.lock().ok().and_then(|slot| slot.clone()) {
            wake.wake();
        }
    }

    async fn proxy(&mut self, address: &MainMenuAddress) -> zbus::Result<zbus::Proxy<'static>> {
        if let Some(entry) = self.proxies.get(address) {
            return Ok(entry.proxy.clone());
        }
        let proxy: zbus::Proxy<'static> = zbus::proxy::Builder::new(&self.connection)
            .destination(address.service.clone())?
            .path(address.path.clone())?
            .interface(DBUSMENU_INTERFACE)?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let owner = match zbus::names::BusName::try_from(address.service.as_str()) {
            Ok(name) => timed(CALL_TIMEOUT, self.dbus.get_name_owner(name))
                .await
                .and_then(Result::ok)
                .map(|owner| owner.to_string()),
            Err(_) => None,
        };
        self.proxies.insert(
            address.clone(),
            AddressProxy {
                proxy: proxy.clone(),
                owner,
            },
        );
        Ok(proxy)
    }

    async fn layout(
        &mut self,
        address: &MainMenuAddress,
        parent: i32,
        depth: i32,
        limit: Duration,
    ) -> zbus::Result<(u32, Vec<RawItem>)> {
        let proxy = self.proxy(address).await?;
        let args = (parent, depth, &[] as &[&str]);
        let call = proxy.call::<_, _, DbusMenuLayout>("GetLayout", &args);
        let Some(result) = timed(limit, call).await else {
            return Err(zbus::Error::Failure("GetLayout timed out".to_string()));
        };
        let (revision, root) = result?;
        Ok((revision, root.2.into_iter().filter_map(raw_item).collect()))
    }

    async fn about_to_show(&mut self, address: &MainMenuAddress, id: i32) -> bool {
        let Ok(proxy) = self.proxy(address).await else {
            return false;
        };
        let args = (id,);
        let call = proxy.call::<_, _, bool>("AboutToShow", &args);
        match timed(CALL_TIMEOUT, call).await {
            Some(Ok(needs_update)) => {
                tracing::debug!(id, needs_update, "menubar: dbusmenu AboutToShow");
                needs_update
            }
            Some(Err(error)) => {
                tracing::warn!(%error, id, "menubar: dbusmenu AboutToShow failed");
                false
            }
            None => {
                tracing::warn!(id, "menubar: dbusmenu AboutToShow timed out");
                false
            }
        }
    }

    async fn event(
        &mut self,
        address: &MainMenuAddress,
        id: i32,
        timestamp: u32,
    ) -> zbus::Result<()> {
        let proxy = self.proxy(address).await?;
        let args = (id, "clicked", zbus::zvariant::Value::I32(0), timestamp);
        let call = proxy.call::<_, _, ()>("Event", &args);
        timed(CALL_TIMEOUT, call)
            .await
            .unwrap_or_else(|| Err(zbus::Error::Failure("Event timed out".to_string())))
    }
}

async fn menu_stream(
    connection: &zbus::Connection,
) -> zbus::Result<impl futures_lite::Stream<Item = zbus::Result<zbus::Message>> + Unpin + use<>> {
    Ok(signal_stream(connection, DBUSMENU_INTERFACE)
        .await?
        .or(signal_stream(connection, gtkmenus::MENUS_INTERFACE).await?)
        .or(signal_stream(connection, gtkmenus::ACTIONS_INTERFACE).await?))
}

async fn signal_stream(
    connection: &zbus::Connection,
    interface: &'static str,
) -> zbus::Result<zbus::MessageStream> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(interface)?
        .build();
    zbus::MessageStream::for_match_rule(rule, connection, None).await
}

fn raw_item(value: OwnedValue) -> Option<RawItem> {
    let value: zbus::zvariant::Value<'_> = value.into();
    let (id, properties, children) = value.downcast::<LayoutItem>().ok()?;
    Some(RawItem {
        id,
        properties,
        children: children.into_iter().filter_map(raw_item).collect(),
    })
}

pub(crate) async fn timed<F: std::future::Future>(limit: Duration, future: F) -> Option<F::Output> {
    future::or(async move { Some(future.await) }, async move {
        async_io::Timer::after(limit).await;
        None
    })
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

    fn address(service: &str) -> MainMenuAddress {
        MainMenuAddress {
            service: service.to_string(),
            path: "/MenuBar/1".to_string(),
        }
    }

    #[test]
    fn a_layout_update_drops_the_subtree_and_its_cached_descendants() {
        let mut cache = MenuCache::default();
        cache.put(
            0,
            1,
            vec![item(7, &[], Vec::new()), item(9, &[], Vec::new())],
        );
        cache.put(7, 1, vec![item(8, &[], Vec::new())]);
        cache.put(8, 1, Vec::new());
        cache.put(9, 1, Vec::new());
        let mut dropped = cache.invalidate(7, 2);
        dropped.sort_unstable();
        assert_eq!(dropped, [7, 8]);
        assert!(cache.subtrees.contains_key(&0));
        assert!(cache.subtrees.contains_key(&9));
    }

    #[test]
    fn an_update_for_an_uncached_parent_drops_every_older_subtree() {
        let mut cache = MenuCache::default();
        cache.put(0, 1, vec![item(7, &[], Vec::new())]);
        cache.put(8, 3, Vec::new());
        assert_eq!(cache.invalidate(7, 2), [0]);
        assert!(cache.subtrees.contains_key(&8));
    }

    #[test]
    fn an_older_revision_is_ignored() {
        let mut cache = MenuCache::default();
        cache.put(0, 9, vec![item(1, &[], Vec::new())]);
        assert!(cache.invalidate(0, 8).is_empty());
        assert!(cache.subtrees.contains_key(&0));
    }

    #[test]
    fn any_newer_layout_update_invalidates_the_full_tree() {
        let mut cache = MenuCache::default();
        cache.put_full(3, vec![item(1, &[], Vec::new())]);
        assert!(!cache.invalidate_full(3));
        assert!(cache.full.is_some());
        assert!(cache.invalidate_full(4));
        assert!(cache.full.is_none());
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
            i32::try_from(
                cache.subtrees.get(&0).unwrap().items[0].properties["toggle-state"].clone()
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn items_properties_updated_also_refresh_the_full_tree() {
        let mut cache = MenuCache::default();
        cache.put_full(
            1,
            vec![item(4, &[("enabled", OwnedValue::from(true))], Vec::new())],
        );
        let updated = vec![(
            4,
            HashMap::from([("enabled".to_string(), OwnedValue::from(false))]),
        )];
        assert!(cache.apply_properties(&updated, &[]));
        assert!(
            !bool::try_from(cache.full.as_ref().unwrap().items[0].properties["enabled"].clone())
                .unwrap()
        );
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
            i32::try_from(
                cache.subtrees.get(&4).unwrap().items[0].properties["toggle-state"].clone()
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn the_full_tree_prefetch_fires_after_one_second() {
        let mut background = BackgroundFullTree::default();
        let kate = address("org.kde.kate");
        let now = Instant::now();
        background.schedule(&kate, 1, false, now);
        assert_eq!(background.next(), Some(now + FULL_TREE_DELAY));
        assert_eq!(
            background.due(now + FULL_TREE_DELAY - Duration::from_millis(1)),
            None
        );
        assert_eq!(background.due(now + FULL_TREE_DELAY), Some((kate, 1)));
        assert_eq!(background.next(), None);
    }

    #[test]
    fn a_second_focus_on_the_same_address_does_not_enqueue_another_prefetch() {
        let mut background = BackgroundFullTree::default();
        let kate = address("org.kde.kate");
        let now = Instant::now();
        background.schedule(&kate, 1, false, now);
        background.schedule(&kate, 2, false, now + Duration::from_millis(400));
        assert_eq!(background.next(), Some(now + FULL_TREE_DELAY));
        assert_eq!(background.due(now + FULL_TREE_DELAY), Some((kate, 2)));
    }

    #[test]
    fn a_cached_full_tree_or_a_new_focus_cancels_the_prefetch() {
        let mut background = BackgroundFullTree::default();
        let kate = address("org.kde.kate");
        let firefox = address("org.mozilla.firefox");
        let now = Instant::now();
        background.schedule(&kate, 1, false, now);
        background.schedule(&kate, 1, true, now);
        assert_eq!(background.next(), None);
        background.schedule(&kate, 1, false, now);
        background.schedule(&firefox, 2, false, now);
        assert_eq!(background.next(), Some(now + FULL_TREE_DELAY));
        assert_eq!(background.due(now + FULL_TREE_DELAY), Some((firefox, 2)));
    }
}
