use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use async_channel::{Receiver, Sender};
use zbus::zvariant::{OwnedValue, Value};

use crate::app_menu::layout::{
    DbusMenuLayout, RegisteredMenu, Update, action_id_for_numeric, build_layout, diff_layout,
};
use crate::app_menu::{AppMenu, IdMap, MenuHandle};

const VERSION: u32 = 3;

/// The path prefix for per-window menus: `/MenuBar/<n>`.
const PATH_PREFIX: &str = "/MenuBar/";

/// Commands sent from the render thread to the D-Bus thread.
enum Command {
    Register {
        path: String,
        registered: Arc<RegisteredMenu>,
        reply: Sender<Option<crate::platform::AppMenuAddress>>,
    },
    Unregister {
        path: String,
    },
    Update {
        path: String,
    },
}

/// Per-process D-Bus state. Only one thread and one session connection is
/// created, shared by every window of the app.
struct Server {
    commands: Sender<Command>,
    menus: Mutex<HashMap<String, Weak<RegisteredMenu>>>,
}

static SERVER: OnceLock<Server> = OnceLock::new();

impl Server {
    fn get() -> &'static Server {
        SERVER.get_or_init(|| {
            let (commands, rx) = async_channel::unbounded();
            std::thread::Builder::new()
                .name("barrier-dbusmenu".to_string())
                .spawn(move || {
                    async_io::block_on(run(rx));
                })
                .expect("barrier-dbusmenu thread");
            Server {
                commands,
                menus: Mutex::new(HashMap::new()),
            }
        })
    }

    fn register(
        _menu: AppMenu,
        handle: MenuHandle,
        on_action: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> Option<crate::platform::AppMenuAddress> {
        let server = Self::get();
        let index = {
            let menus = server.menus.lock().expect("menu registry poisoned");
            menus.len() as u32
        };
        let path = format!("{PATH_PREFIX}{index}");
        handle.set_path(&path);
        let registered = RegisteredMenu::new(handle, on_action);
        server
            .menus
            .lock()
            .expect("menu registry poisoned")
            .insert(path.clone(), Arc::downgrade(&registered));
        let (reply_tx, reply_rx) = async_channel::bounded(1);
        let _ = server.commands.send_blocking(Command::Register {
            path: path.clone(),
            registered,
            reply: reply_tx,
        });
        async_io::block_on(reply_rx.recv()).ok().flatten()
    }

    fn unregister(path: &str) {
        let server = Self::get();
        server
            .menus
            .lock()
            .expect("menu registry poisoned")
            .remove(path);
        let _ = server.commands.send_blocking(Command::Unregister {
            path: path.to_string(),
        });
    }

    fn update(path: &str) {
        let server = Self::get();
        let _ = server.commands.send_blocking(Command::Update {
            path: path.to_string(),
        });
    }
}

/// Registers a menu and returns the D-Bus address to announce.
pub fn register(
    menu: AppMenu,
    handle: MenuHandle,
    on_action: Arc<dyn Fn(&str) + Send + Sync>,
) -> Option<crate::platform::AppMenuAddress> {
    Server::register(menu, handle, on_action)
}

pub fn unregister(path: &str) {
    Server::unregister(path);
}

pub fn update(path: &str) {
    Server::update(path);
}

async fn run(commands: Receiver<Command>) {
    let connection = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(error) => {
            tracing::warn!(%error, "barrier-dbusmenu: no session bus; menus are local only");
            while let Ok(command) = commands.recv().await {
                if let Command::Register { reply, .. } = command {
                    let _ = reply.send(None).await;
                }
            }
            return;
        }
    };

    let mut menus: HashMap<String, Arc<RegisteredMenu>> = HashMap::new();
    let mut id_maps: HashMap<String, IdMap> = HashMap::new();

    while let Ok(command) = commands.recv().await {
        match command {
            Command::Register {
                path,
                registered,
                reply,
            } => {
                let iface = DbusMenuObject {
                    registered: registered.clone(),
                };
                let address = if connection.unique_name().is_some() {
                    let unique = connection
                        .unique_name()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    if let Err(error) = connection.object_server().at(path.clone(), iface).await {
                        tracing::warn!(%error, path, "barrier-dbusmenu: could not register object");
                        None
                    } else {
                        menus.insert(path.clone(), registered.clone());
                        let mut id_map = IdMap::default();
                        let (snapshot, _layout) =
                            build_layout_from_handle(&registered, &mut id_map);
                        let revision = snapshot.revision;
                        let _ = registered
                            .last_layout
                            .lock()
                            .expect("layout lock poisoned")
                            .replace(snapshot);
                        id_maps.insert(path.clone(), id_map);
                        if let Ok(iface_ref) = connection
                            .object_server()
                            .interface::<_, DbusMenuObject>(path.as_str())
                            .await
                            && let Err(error) = DbusMenuObject::layout_updated(
                                iface_ref.signal_emitter(),
                                revision,
                                0,
                            )
                            .await
                        {
                            tracing::warn!(%error, "barrier-dbusmenu: could not emit initial LayoutUpdated");
                        }
                        Some(crate::platform::AppMenuAddress {
                            service: unique,
                            path,
                        })
                    }
                } else {
                    None
                };
                let _ = reply.send(address).await;
            }
            Command::Unregister { path } => {
                if menus.remove(&path).is_some() {
                    id_maps.remove(&path);
                    let _ = connection
                        .object_server()
                        .remove::<DbusMenuObject, _>(path.as_str())
                        .await;
                }
            }
            Command::Update { path } => {
                let Some(registered) = menus.get(&path).cloned() else {
                    continue;
                };
                let id_map = id_maps.entry(path.clone()).or_default();
                let (snapshot, _layout) = build_layout_from_handle(&registered, id_map);
                let revision = snapshot.revision;
                match diff_layout(&registered, snapshot) {
                    Update::None => {}
                    Update::Layout => {
                        if let Ok(iface_ref) = connection
                            .object_server()
                            .interface::<_, DbusMenuObject>(path.as_str())
                            .await
                            && let Err(error) = DbusMenuObject::layout_updated(
                                iface_ref.signal_emitter(),
                                revision,
                                0,
                            )
                            .await
                        {
                            tracing::warn!(%error, "barrier-dbusmenu: could not emit LayoutUpdated");
                        }
                    }
                    Update::Properties(updated) => {
                        if let Ok(iface_ref) = connection
                            .object_server()
                            .interface::<_, DbusMenuObject>(path.as_str())
                            .await
                        {
                            let payload: Vec<(i32, HashMap<String, OwnedValue>)> =
                                updated.into_iter().map(|u| (u.id, u.properties)).collect();
                            if let Err(error) = DbusMenuObject::items_properties_updated(
                                iface_ref.signal_emitter(),
                                payload,
                                Vec::new(),
                            )
                            .await
                            {
                                tracing::warn!(%error, "barrier-dbusmenu: could not emit ItemsPropertiesUpdated");
                            }
                        }
                    }
                }
            }
        }
    }
}

fn build_layout_from_handle(
    registered: &Arc<RegisteredMenu>,
    id_map: &mut IdMap,
) -> (
    crate::app_menu::layout::LayoutSnapshot,
    crate::app_menu::layout::LayoutItem,
) {
    let (menu, revision) = registered.handle.snapshot();
    build_layout(&menu, id_map, revision)
}

struct DbusMenuObject {
    registered: Arc<RegisteredMenu>,
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl DbusMenuObject {
    #[zbus(property, name = "Version")]
    fn version(&self) -> u32 {
        VERSION
    }

    #[zbus(property, name = "TextDirection")]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property, name = "Status")]
    fn status(&self) -> &str {
        "normal"
    }

    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> DbusMenuLayout {
        let guard = self
            .registered
            .last_layout
            .lock()
            .expect("layout lock poisoned");
        let Some(snapshot) = guard.as_ref() else {
            return (1, empty_root());
        };
        let revision = snapshot.revision;
        if parent_id == 0 {
            drop(guard);
            let (menu, _) = self.registered.handle.snapshot();
            let mut id_map = IdMap::default();
            let (_snapshot, layout) = build_layout(&menu, &mut id_map, revision);
            return (revision, layout);
        }
        if let Some(node) = snapshot.nodes.get(&parent_id) {
            #[allow(clippy::needless_borrow)]
            return (
                revision,
                layout_node(&snapshot, parent_id, node, recursion_depth),
            );
        }
        (revision, empty_subtree(parent_id))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let guard = self
            .registered
            .last_layout
            .lock()
            .expect("layout lock poisoned");
        ids.into_iter()
            .filter_map(|id| {
                guard
                    .as_ref()
                    .and_then(|s| s.nodes.get(&id))
                    .map(|node| (id, node.properties.clone()))
            })
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> OwnedValue {
        let guard = self
            .registered
            .last_layout
            .lock()
            .expect("layout lock poisoned");
        guard
            .as_ref()
            .and_then(|s| s.nodes.get(&id))
            .and_then(|node| node.properties.get(&name).cloned())
            .unwrap_or_else(|| crate::app_menu::owned(""))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
        if event_id != "clicked" {
            return;
        }
        if let Some(action_id) = action_id_for_numeric(&self.registered, id) {
            let on_action = self.registered.on_action.clone();
            (on_action)(&action_id);
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<(i32, Vec<String>)> {
        for (id, event_id, data, timestamp) in events {
            self.event(id, event_id, data, timestamp);
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        true
    }

    fn about_to_show_group(&self, ids: Vec<i32>) -> Vec<(i32, bool)> {
        ids.into_iter().map(|id| (id, true)).collect()
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn items_properties_updated(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        updated_props: Vec<(i32, HashMap<String, OwnedValue>)>,
        removed_props: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()>;
}

fn empty_root() -> crate::app_menu::layout::LayoutItem {
    (
        0i32,
        HashMap::<String, OwnedValue>::new(),
        Vec::<OwnedValue>::new(),
    )
}

fn empty_subtree(id: i32) -> crate::app_menu::layout::LayoutItem {
    (
        id,
        HashMap::<String, OwnedValue>::new(),
        Vec::<OwnedValue>::new(),
    )
}

fn layout_node(
    snapshot: &crate::app_menu::layout::LayoutSnapshot,
    id: i32,
    node: &crate::app_menu::layout::LayoutNode,
    recursion_depth: i32,
) -> crate::app_menu::layout::LayoutItem {
    let children: Vec<OwnedValue> = if recursion_depth == 0 {
        Vec::new()
    } else {
        node.children
            .iter()
            .filter_map(|child_id| {
                snapshot.nodes.get(child_id).map(|child_node| {
                    let child = layout_node(
                        snapshot,
                        *child_id,
                        child_node,
                        recursion_depth.saturating_sub(1),
                    );
                    OwnedValue::try_from(Value::Structure(zbus::zvariant::Structure::from(child)))
                        .unwrap()
                })
            })
            .collect()
    };
    (id, node.properties.clone(), children)
}

/// Test-only peer-to-peer server handle (Commit B, ADR 0040 §10).
///
/// Lets integration tests in other crates serve a kit menu over a private
/// `UnixStream::pair` connection without needing a session bus.
#[derive(Clone)]
#[doc(hidden)]
pub struct TestMenuServer {
    registered: Arc<RegisteredMenu>,
    path: String,
}

impl TestMenuServer {
    /// Creates a server for `menu` that calls `on_action` for "clicked" events.
    pub fn new(
        menu: AppMenu,
        path: impl Into<String>,
        on_action: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> Self {
        let handle = crate::app_menu::MenuHandle::for_test(menu);
        let registered = RegisteredMenu::new(handle, on_action);
        Self {
            registered,
            path: path.into(),
        }
    }

    /// Registers the dbusmenu object on `connection` and emits the initial
    /// `LayoutUpdated`.
    pub async fn serve(&self, connection: &zbus::Connection) -> zbus::Result<()> {
        connection
            .object_server()
            .at(
                self.path.clone(),
                DbusMenuObject {
                    registered: self.registered.clone(),
                },
            )
            .await?;
        let mut id_map = IdMap::default();
        let (menu, _) = self.registered.handle.snapshot();
        let (snapshot, _) = build_layout(&menu, &mut id_map, 1);
        let revision = snapshot.revision;
        *self
            .registered
            .last_layout
            .lock()
            .expect("layout lock poisoned") = Some(snapshot);
        if let Ok(iface_ref) = connection
            .object_server()
            .interface::<_, DbusMenuObject>(self.path.as_str())
            .await
        {
            DbusMenuObject::layout_updated(iface_ref.signal_emitter(), revision, 0).await?;
        }
        Ok(())
    }

    /// Updates the cached properties and emits `ItemsPropertiesUpdated`.
    pub async fn update_properties(
        &self,
        connection: &zbus::Connection,
        updated: Vec<(i32, HashMap<String, OwnedValue>)>,
    ) -> zbus::Result<()> {
        {
            let mut guard = self
                .registered
                .last_layout
                .lock()
                .expect("layout lock poisoned");
            if let Some(snapshot) = guard.as_mut() {
                for (id, props) in &updated {
                    if let Some(node) = snapshot.nodes.get_mut(id) {
                        node.properties.extend(props.clone());
                    }
                }
            }
        }
        let iface_ref = connection
            .object_server()
            .interface::<_, DbusMenuObject>(self.path.as_str())
            .await?;
        DbusMenuObject::items_properties_updated(iface_ref.signal_emitter(), updated, Vec::new())
            .await
    }
}
