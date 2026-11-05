use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender};
use barrier_kit::app_menu::layout::{DbusMenuLayout, LayoutItem};
use barrier_kit::platform::{AppMenuAddress, WakeHandle};
use barrier_kit::shell::GtkMenusAddress;
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
        address: AppMenuAddress,
        generation: u64,
    },
    GtkFocus {
        menus: GtkMenusAddress,
        app_name: String,
        generation: u64,
    },
    Unfocus,
    Open {
        address: AppMenuAddress,
        id: i32,
        generation: u64,
    },
    FullTree {
        address: AppMenuAddress,
        generation: u64,
    },
    Activate {
        address: AppMenuAddress,
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
    pub address: AppMenuAddress,
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
    pub fn focus(&self, address: AppMenuAddress, generation: u64) {
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

type MenuCaches = HashMap<AppMenuAddress, MenuCache>;

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

#[derive(Default)]
struct BackgroundFullTree {
    wait: Option<(AppMenuAddress, u64, Instant)>,
}

impl BackgroundFullTree {
    fn schedule(&mut self, address: &AppMenuAddress, generation: u64, cached: bool, now: Instant) {
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

    fn cancel(&mut self, address: &AppMenuAddress) {
        if self
            .wait
            .as_ref()
            .is_some_and(|(pending, _, _)| pending == address)
        {
            self.wait = None;
        }
    }

    fn next(&self) -> Option<Instant> {
        self.wait.as_ref().map(|(_, _, at)| *at)
    }

    fn due(&mut self, now: Instant) -> Option<(AppMenuAddress, u64)> {
        if self.wait.as_ref().is_some_and(|(_, _, at)| *at <= now) {
            self.wait
                .take()
                .map(|(address, generation, _)| (address, generation))
        } else {
            None
        }
    }
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
    mut app_menus_enabled: bool,
) {
    let builder = match zbus::connection::Builder::session() {
        Ok(builder) => builder.method_timeout(FULL_TREE_TIMEOUT),
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
    set_registrar(&connection, &windows, app_menus_enabled).await;

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

    let mut worker = Worker {
        connection,
        dbus,
        proxies: HashMap::new(),
        caches: HashMap::new(),
        snapshots,
        wake,
    };
    let mut focused: Option<(AppMenuAddress, u64)> = None;
    let mut pending = Pending::default();
    let mut deadline: Option<Instant> = None;
    let mut background = BackgroundFullTree::default();
    let mut gtk: Option<gtkmenus::Session> = None;
    loop {
        let timer = deadline.into_iter().chain(background.next()).min();
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
            Event::Command(Ok(Command::SetAppMenus(enabled))) => {
                if enabled == app_menus_enabled {
                    continue;
                }
                app_menus_enabled = enabled;
                pending = Pending::default();
                deadline = None;
                if !enabled {
                    focused = None;
                    background = BackgroundFullTree::default();
                    end_gtk(&worker.connection, &mut gtk).await;
                }
                tracing::info!(enabled, "menubar: app_menus changed");
                set_registrar(&worker.connection, &windows, enabled).await;
            }
            Event::Command(Ok(Command::Focus {
                address,
                generation,
            })) => {
                if !app_menus_enabled {
                    continue;
                }
                end_gtk(&worker.connection, &mut gtk).await;
                focused = Some((address.clone(), generation));
                pending = Pending::default();
                deadline = None;
                background.schedule(
                    &address,
                    generation,
                    worker.has_full_tree(&address),
                    Instant::now(),
                );
                if !worker.send_cached_root(&address, generation).await {
                    pending.focus = Some((address, generation));
                    arm(&mut deadline);
                }
            }
            Event::Command(Ok(Command::GtkFocus {
                menus,
                app_name,
                generation,
            })) => {
                if !app_menus_enabled {
                    continue;
                }
                focused = None;
                pending = Pending::default();
                deadline = None;
                background = BackgroundFullTree::default();
                end_gtk(&worker.connection, &mut gtk).await;
                let session =
                    gtkmenus::Session::open(&worker.connection, menus, app_name, generation).await;
                worker.send_gtk(&session, 0, false).await;
                gtk = Some(session);
            }
            Event::Command(Ok(Command::Unfocus)) => {
                focused = None;
                pending = Pending::default();
                deadline = None;
                background = BackgroundFullTree::default();
                end_gtk(&worker.connection, &mut gtk).await;
            }
            Event::Command(Ok(Command::Open {
                address,
                id,
                generation,
            })) => {
                if !app_menus_enabled {
                    continue;
                }
                if let Some(session) = gtk_for(&gtk, &address, generation) {
                    worker.send_gtk(session, id, true).await;
                    continue;
                }
                let started = Instant::now();
                let cached = worker
                    .caches
                    .get(&address)
                    .and_then(|cache| cache.subtrees.get(&id))
                    .map(|subtree| subtree.items.clone());
                let needs_update = worker.about_to_show(&address, id).await;
                let from_cache = !needs_update && cached.is_some();
                let items = if from_cache {
                    cached
                } else {
                    worker.fetch_subtree(&address, id).await
                };
                pending.subtrees.retain(|pending| *pending != id);
                tracing::info!(
                    generation,
                    id,
                    service = %address.service,
                    items = items.as_ref().map_or(0, Vec::len),
                    cached = from_cache,
                    ms = started.elapsed().as_millis(),
                    "menubar: dbusmenu submenu snapshot"
                );
                worker
                    .send(MenuSnapshot {
                        generation,
                        address,
                        parent: Some(id),
                        items,
                        cached: from_cache,
                    })
                    .await;
            }
            Event::Command(Ok(Command::FullTree {
                address,
                generation,
            })) => {
                if !app_menus_enabled {
                    continue;
                }
                if let Some(session) = gtk_for(&gtk, &address, generation) {
                    worker.send_gtk(session, 0, true).await;
                    continue;
                }
                let cached = worker.has_full_tree(&address);
                worker.send_full_tree(&address, generation).await;
                if !cached && worker.has_full_tree(&address) {
                    background.cancel(&address);
                }
            }
            Event::Command(Ok(Command::Activate {
                address,
                id,
                timestamp,
            })) => {
                if let Some(session) = gtk.as_ref().filter(|session| session.address == address) {
                    session.activate(&worker.connection, id).await;
                    continue;
                }
                if let Err(error) = worker.event(&address, id, timestamp).await {
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
                if let Some(session) =
                    gtk.take_if(|session| session.menus.unique_bus_name == service)
                {
                    tracing::info!(%service, "menubar: GTK menu service vanished");
                    worker
                        .send(MenuSnapshot {
                            generation: session.generation,
                            address: session.address,
                            parent: None,
                            items: None,
                            cached: false,
                        })
                        .await;
                }
                worker
                    .proxies
                    .retain(|address, _| address.service != service);
                let cached_menus = worker.caches.len();
                worker
                    .caches
                    .retain(|address, _| address.service != service);
                let evicted = cached_menus != worker.caches.len();
                if let Some((address, generation)) = &focused
                    && address.service == service
                {
                    tracing::info!(%service, generation, evicted, "menubar: menu service vanished");
                    pending = Pending::default();
                    deadline = None;
                    worker
                        .send(MenuSnapshot {
                            generation: *generation,
                            address: address.clone(),
                            parent: None,
                            items: None,
                            cached: false,
                        })
                        .await;
                }
            }
            Event::Owner(None) => break,
            Event::Menu(Some(Ok(message)))
                if message
                    .header()
                    .interface()
                    .is_some_and(|interface| interface.starts_with("org.gtk.")) =>
            {
                if let Some(session) = gtk.as_mut()
                    && session.signal(&worker.connection, &message).await
                {
                    worker.send_gtk(session, 0, false).await;
                }
            }
            Event::Menu(Some(Ok(message))) => {
                handle_signal(
                    message,
                    &worker.proxies,
                    &mut worker.caches,
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
                let now = Instant::now();
                if deadline.is_some_and(|at| at <= now) {
                    deadline = None;
                    worker.flush(&focused, &mut pending).await;
                }
                if let Some((address, generation)) = background.due(now)
                    && focused == Some((address.clone(), generation))
                    && !worker.has_full_tree(&address)
                {
                    worker.send_full_tree(&address, generation).await;
                }
            }
        }
    }
}

fn gtk_for<'a>(
    gtk: &'a Option<gtkmenus::Session>,
    address: &AppMenuAddress,
    generation: u64,
) -> Option<&'a gtkmenus::Session> {
    gtk.as_ref()
        .filter(|session| session.address == *address && session.generation == generation)
}

async fn end_gtk(connection: &zbus::Connection, gtk: &mut Option<gtkmenus::Session>) {
    if let Some(session) = gtk.take() {
        session.close(connection).await;
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
            let (dropped, full_invalidated) = caches
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
            let refetch: Vec<i32> = dropped
                .into_iter()
                .filter(|id| *id == parent || *id == 0)
                .collect();
            if is_focused && !refetch.is_empty() {
                pending.subtrees.extend(refetch);
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
            tracing::debug!(
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

struct Worker {
    connection: zbus::Connection,
    dbus: zbus::fdo::DBusProxy<'static>,
    proxies: ProxyCache,
    caches: MenuCaches,
    snapshots: Sender<MenuSnapshot>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
}

impl Worker {
    async fn flush(&mut self, focused: &Option<(AppMenuAddress, u64)>, pending: &mut Pending) {
        let Some((address, generation)) = focused.clone() else {
            *pending = Pending::default();
            return;
        };

        if pending.focus.take() == *focused && !self.send_cached_root(&address, generation).await {
            self.fetch_root(&address, generation).await;
        }

        let mut subtrees = std::mem::take(&mut pending.subtrees);
        subtrees.sort_unstable();
        subtrees.dedup();
        for id in subtrees {
            if id == 0 {
                if !self
                    .caches
                    .get(&address)
                    .is_some_and(|cache| cache.subtrees.contains_key(&0))
                {
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

    async fn send_cached_root(&self, address: &AppMenuAddress, generation: u64) -> bool {
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

    async fn fetch_root(&mut self, address: &AppMenuAddress, generation: u64) {
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

    async fn send_cached_subtrees(&self, address: &AppMenuAddress, generation: u64) {
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

    async fn fetch_subtree(&mut self, address: &AppMenuAddress, id: i32) -> Option<Vec<RawItem>> {
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

    fn has_full_tree(&self, address: &AppMenuAddress) -> bool {
        self.caches
            .get(address)
            .is_some_and(|cache| cache.full.is_some())
    }

    async fn send_full_tree(&mut self, address: &AppMenuAddress, generation: u64) {
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

    async fn proxy(&mut self, address: &AppMenuAddress) -> zbus::Result<zbus::Proxy<'static>> {
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
        address: &AppMenuAddress,
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

    async fn about_to_show(&mut self, address: &AppMenuAddress, id: i32) -> bool {
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
        address: &AppMenuAddress,
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

    fn address(service: &str) -> AppMenuAddress {
        AppMenuAddress {
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
