use std::collections::{BTreeSet, HashMap};

use std::time::Instant;

use barrier_kit::platform::AppMenuAddress;
use barrier_kit::shell::GtkMenusAddress;
use zbus::zvariant::{OwnedValue, Signature, Value};

use crate::localize::tr;
use crate::menubar::dbusmenu::{CALL_TIMEOUT, RawItem, timed};
use crate::menubar::dbusmenu_map::prop;

pub const MENUS_INTERFACE: &str = "org.gtk.Menus";
pub const ACTIONS_INTERFACE: &str = "org.gtk.Actions";
const ADW_APP_MENU: &str = "x-adw-app-menu";

type Attributes = HashMap<String, OwnedValue>;
type StartEntry = (u32, u32, Vec<Attributes>);
type MenuChange = (u32, u32, u32, u32, Vec<Attributes>);
type ActionDescription = (bool, Signature, Vec<OwnedValue>);

pub fn address(menus: &GtkMenusAddress) -> AppMenuAddress {
    let path = if menus.menubar_path.is_empty() {
        &menus.app_menu_path
    } else {
        &menus.menubar_path
    };
    AppMenuAddress {
        service: menus.unique_bus_name.clone(),
        path: path.clone(),
    }
}

#[derive(Debug, Default)]
pub struct Export {
    pub path: String,
    menus: HashMap<(u32, u32), Vec<Attributes>>,
    subscribed: BTreeSet<u32>,
}

impl Export {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            ..Self::default()
        }
    }

    pub fn subscribed(&self) -> Vec<u32> {
        self.subscribed.iter().copied().collect()
    }

    pub fn started(&mut self, groups: &[u32], entries: Vec<StartEntry>) {
        self.subscribed.extend(groups);
        for (group, menu, items) in entries {
            self.menus.insert((group, menu), items);
        }
    }

    pub fn changed(&mut self, changes: Vec<MenuChange>) -> bool {
        let mut changed = false;
        for (group, menu, position, removed, added) in changes {
            // GLib announces a menu added to a subscribed group as a change at 0 (gmenuexporter.c).
            if !self.subscribed.contains(&group) {
                continue;
            }
            let items = self.menus.entry((group, menu)).or_default();
            let start = (position as usize).min(items.len());
            let end = (start + removed as usize).min(items.len());
            items.splice(start..end, added);
            changed = true;
        }
        changed
    }

    pub fn missing_groups(&self) -> Vec<u32> {
        let mut missing = BTreeSet::new();
        for items in self.menus.values() {
            for item in items {
                for key in [":submenu", ":section"] {
                    if let Some((group, _)) = item.get(key).and_then(link)
                        && !self.subscribed.contains(&group)
                    {
                        missing.insert(group);
                    }
                }
            }
        }
        missing.into_iter().collect()
    }

    fn menu(&self, group: u32, menu: u32) -> &[Attributes] {
        self.menus
            .get(&(group, menu))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub enabled: bool,
    pub state: Option<OwnedValue>,
}

#[derive(Debug, Default)]
pub struct Actions(HashMap<String, Action>);

impl Actions {
    pub fn describe_all(&mut self, prefix: &str, all: HashMap<String, ActionDescription>) {
        self.0.retain(|name, _| !name.starts_with(prefix));
        self.changed(prefix, Vec::new(), HashMap::new(), HashMap::new(), all);
    }

    pub fn changed(
        &mut self,
        prefix: &str,
        removals: Vec<String>,
        enabled: HashMap<String, bool>,
        states: HashMap<String, OwnedValue>,
        additions: HashMap<String, ActionDescription>,
    ) {
        for name in removals {
            self.0.remove(&format!("{prefix}{name}"));
        }
        for (name, value) in enabled {
            if let Some(action) = self.0.get_mut(&format!("{prefix}{name}")) {
                action.enabled = value;
            }
        }
        for (name, value) in states {
            if let Some(action) = self.0.get_mut(&format!("{prefix}{name}")) {
                action.state = Some(value);
            }
        }
        for (name, (enabled, _, state)) in additions {
            self.0.insert(
                format!("{prefix}{name}"),
                Action {
                    enabled,
                    state: state.into_iter().next(),
                },
            );
        }
    }

    fn get(&self, name: &str) -> Option<&Action> {
        self.0.get(name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub action: String,
    pub target: Option<OwnedValue>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Tree {
    pub items: Vec<RawItem>,
    pub targets: HashMap<i32, Target>,
}

impl Tree {
    pub fn build(app_menu: &Export, menubar: &Export, actions: &Actions, app_name: &str) -> Self {
        let mut builder = Builder {
            actions,
            app_name,
            next_id: 1,
            targets: HashMap::new(),
        };
        let mut items = Vec::new();
        let app_items = builder.menu(app_menu, 0, 0);
        if !app_items.is_empty() {
            let id = builder.id();
            items.push(RawItem {
                id,
                properties: submenu_properties(app_name),
                children: app_items,
            });
        }
        items.extend(builder.menu(menubar, 0, 0));
        Self {
            items,
            targets: builder.targets,
        }
    }

    pub fn children(&self, id: i32) -> Option<Vec<RawItem>> {
        if id == 0 {
            return Some(self.items.clone());
        }
        find(&self.items, id).map(|item| item.children.clone())
    }
}

fn find(items: &[RawItem], id: i32) -> Option<&RawItem> {
    items.iter().find_map(|item| {
        if item.id == id {
            Some(item)
        } else {
            find(&item.children, id)
        }
    })
}

struct Builder<'a> {
    actions: &'a Actions,
    app_name: &'a str,
    next_id: i32,
    targets: HashMap<i32, Target>,
}

impl Builder<'_> {
    fn id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn menu(&mut self, export: &Export, group: u32, menu: u32) -> Vec<RawItem> {
        let mut out = Vec::new();
        for attributes in export.menu(group, menu) {
            if let Some((group, menu)) = attributes.get(":section").and_then(link) {
                out.push(separator(self.id()));
                out.extend(self.menu(export, group, menu));
                out.push(separator(self.id()));
                continue;
            }
            let label = prop::<String>(attributes, "label").unwrap_or_default();
            if let Some((group, menu)) = attributes.get(":submenu").and_then(link) {
                if prop(attributes, ADW_APP_MENU).unwrap_or(false) {
                    let children = self.menu(export, group, menu);
                    out.extend(self.split_primary_menu(children));
                    continue;
                }
                let id = self.id();
                let children = self.menu(export, group, menu);
                out.push(RawItem {
                    id,
                    properties: submenu_properties(&label),
                    children,
                });
                continue;
            }
            if let Some(item) = self.action(attributes, label) {
                out.push(item);
            }
        }
        out
    }

    fn split_primary_menu(&mut self, children: Vec<RawItem>) -> Vec<RawItem> {
        let mut app = Vec::new();
        let mut help = Vec::new();
        let mut rest = Vec::new();
        for child in children {
            let action = self
                .targets
                .get(&child.id)
                .map(|target| target.action.clone());
            match action.as_deref().map(primary_slot) {
                Some(PrimarySlot::About) => {
                    app.push(relabel(child, &tr!("app-about", app = self.app_name)))
                }
                Some(PrimarySlot::Preferences) => app.push(relabel(child, tr!("menu-settings"))),
                Some(PrimarySlot::Quit) => {
                    app.push(relabel(child, &tr!("app-quit", app = self.app_name)))
                }
                Some(PrimarySlot::Help) => help.push(child),
                Some(PrimarySlot::Window) => {}
                Some(PrimarySlot::Other) | None => rest.push(child),
            }
        }
        let opens_files = rest.iter().any(|item| {
            self.targets
                .get(&item.id)
                .is_some_and(|target| opens_files(&target.action))
        });
        let mut out = Vec::new();
        for (label, items) in [
            (self.app_name.to_string(), app),
            (
                if opens_files {
                    tr!("menu-file")
                } else {
                    tr!("menu-options")
                }
                .to_string(),
                rest,
            ),
            (tr!("menu-help").to_string(), help),
        ] {
            let has_entries = items.iter().any(|item| {
                !matches!(
                    prop::<String>(&item.properties, "type").as_deref(),
                    Some("separator")
                )
            });
            if has_entries {
                let id = self.id();
                out.push(RawItem {
                    id,
                    properties: submenu_properties(&label),
                    children: items,
                });
            }
        }
        out
    }

    fn action(&mut self, attributes: &Attributes, label: String) -> Option<RawItem> {
        let name = prop::<String>(attributes, "action");
        let action = name.as_deref().and_then(|name| self.actions.get(name));
        let enabled = action.is_some_and(|action| action.enabled);
        match prop::<String>(attributes, "hidden-when").as_deref() {
            Some("action-missing") if action.is_none() => return None,
            Some("action-disabled") if !enabled => return None,
            _ => {}
        }
        let target = attributes.get("target").cloned();
        let id = self.id();
        let mut properties = HashMap::from([
            ("label".to_string(), text(&label)),
            ("enabled".to_string(), OwnedValue::from(enabled)),
        ]);
        if let Some(state) = action.and_then(|action| action.state.as_ref()) {
            let (kind, active) = match (&target, bool::try_from(state.clone())) {
                (None, Ok(active)) => ("checkmark", active),
                (Some(target), _) => ("radio", target == state),
                (None, Err(_)) => ("", false),
            };
            if !kind.is_empty() {
                properties.insert("toggle-type".to_string(), text(kind));
                properties.insert(
                    "toggle-state".to_string(),
                    OwnedValue::from(i32::from(active)),
                );
            }
        }
        if let Some(keys) = prop::<String>(attributes, "accel").and_then(|accel| accel_keys(&accel))
        {
            let value = Value::from(vec![keys]);
            if let Ok(value) = OwnedValue::try_from(value) {
                properties.insert("shortcut".to_string(), value);
            }
        }
        if let Some(action) = name {
            self.targets.insert(id, Target { action, target });
        }
        Some(RawItem {
            id,
            properties,
            children: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimarySlot {
    About,
    Preferences,
    Quit,
    Help,
    Window,
    Other,
}

fn primary_slot(action: &str) -> PrimarySlot {
    match action {
        "app.about" => PrimarySlot::About,
        "app.preferences" | "app.settings" => PrimarySlot::Preferences,
        "app.quit" => PrimarySlot::Quit,
        "app.help" | "app.shortcuts" | "win.show-help-overlay" | "win.shortcuts" => {
            PrimarySlot::Help
        }
        "win.fullscreen" | "win.toggle-fullscreen" | "app.fullscreen" => PrimarySlot::Window,
        _ => PrimarySlot::Other,
    }
}

fn opens_files(action: &str) -> bool {
    let name = action.split_once('.').map_or(action, |(_, name)| name);
    ["new-window", "new-tab", "new", "open", "save"]
        .iter()
        .any(|word| name == *word || name.starts_with(&format!("{word}-")))
}

fn relabel(mut item: RawItem, label: &str) -> RawItem {
    item.properties.insert("label".to_string(), text(label));
    item
}

fn submenu_properties(label: &str) -> HashMap<String, OwnedValue> {
    HashMap::from([
        ("label".to_string(), text(label)),
        ("children-display".to_string(), text("submenu")),
    ])
}

fn separator(id: i32) -> RawItem {
    RawItem {
        id,
        properties: HashMap::from([("type".to_string(), text("separator"))]),
        children: Vec::new(),
    }
}

fn text(value: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(value)).expect("a string value is always ownable")
}

fn link(value: &OwnedValue) -> Option<(u32, u32)> {
    let value: Value<'_> = value.try_clone().ok()?.into();
    value.downcast::<(u32, u32)>().ok()
}

pub fn accel_keys(accel: &str) -> Option<Vec<String>> {
    let mut rest = accel.trim();
    let mut keys = Vec::new();
    while let Some(after) = rest.strip_prefix('<') {
        let (modifier, tail) = after.split_once('>')?;
        let name = match modifier.to_ascii_lowercase().as_str() {
            "primary" | "control" | "ctrl" | "ctl" => "Control",
            "shift" => "Shift",
            "alt" | "mod1" => "Alt",
            "super" | "meta" | "hyper" => "Super",
            _ => return None,
        };
        if !keys.iter().any(|key| key == name) {
            keys.push(name.to_string());
        }
        rest = tail;
    }
    if rest.is_empty() {
        return None;
    }
    keys.push(rest.to_string());
    Some(keys)
}

pub fn action_object<'a>(
    menus: &'a GtkMenusAddress,
    action: &'a str,
) -> Option<(&'a str, &'a str)> {
    let (prefix, name) = action.split_once('.')?;
    let (_, path) = action_groups(menus)
        .into_iter()
        .find(|(group, _)| group.strip_suffix('.') == Some(prefix))?;
    (!path.is_empty()).then_some((path, name))
}

// appmenu-gtk-module exports its `unity.*` actions next to the menubar model (platform.c).
fn action_groups(menus: &GtkMenusAddress) -> [(&'static str, &str); 3] {
    [
        ("app.", &menus.application_object_path),
        ("win.", &menus.window_object_path),
        ("unity.", &menus.menubar_path),
    ]
}

pub struct Session {
    pub menus: GtkMenusAddress,
    pub address: AppMenuAddress,
    pub generation: u64,
    app_name: String,
    app_menu: Export,
    menubar: Export,
    actions: Actions,
    pub tree: Tree,
}

impl Session {
    pub async fn open(
        connection: &zbus::Connection,
        menus: GtkMenusAddress,
        app_name: String,
        generation: u64,
    ) -> Self {
        let started = Instant::now();
        let mut actions = Actions::default();
        for (prefix, path) in action_groups(&menus) {
            if path.is_empty() {
                continue;
            }
            let reply = call(
                connection,
                &menus.unique_bus_name,
                path,
                ACTIONS_INTERFACE,
                "DescribeAll",
                &(),
            )
            .await;
            match reply.and_then(|message| {
                message
                    .body()
                    .deserialize::<HashMap<String, ActionDescription>>()
            }) {
                Ok(all) => actions.describe_all(prefix, all),
                Err(error) if prefix == "unity." => {
                    tracing::debug!(%error, path = %path, "menubar: no appmenu-gtk-module actions")
                }
                Err(error) => {
                    tracing::warn!(%error, path = %path, "menubar: org.gtk.Actions DescribeAll failed")
                }
            }
        }
        let mut session = Self {
            address: address(&menus),
            app_menu: Export::new(&menus.app_menu_path),
            menubar: Export::new(&menus.menubar_path),
            menus,
            generation,
            app_name,
            actions,
            tree: Tree::default(),
        };
        let bus = session.menus.unique_bus_name.clone();
        subscribe(connection, &bus, &mut session.app_menu).await;
        subscribe(connection, &bus, &mut session.menubar).await;
        session.rebuild();
        tracing::info!(
            generation,
            bus = %bus,
            titles = session.tree.items.len(),
            ms = started.elapsed().as_millis(),
            "menubar: org.gtk.Menus root snapshot"
        );
        session
    }

    pub async fn close(&self, connection: &zbus::Connection) {
        for export in [&self.app_menu, &self.menubar] {
            let groups = export.subscribed();
            if groups.is_empty() {
                continue;
            }
            let result = call(
                connection,
                &self.menus.unique_bus_name,
                &export.path,
                MENUS_INTERFACE,
                "End",
                &(groups.clone(),),
            )
            .await;
            match result {
                Ok(_) => tracing::info!(
                    bus = %self.menus.unique_bus_name,
                    path = %export.path,
                    ?groups,
                    "menubar: org.gtk.Menus End"
                ),
                Err(error) => {
                    tracing::warn!(%error, path = %export.path, "menubar: org.gtk.Menus End failed")
                }
            }
        }
    }

    pub async fn signal(&mut self, connection: &zbus::Connection, message: &zbus::Message) -> bool {
        let header = message.header();
        if header.sender().map(|sender| sender.as_str())
            != Some(self.menus.unique_bus_name.as_str())
            || header.member().map(|member| member.as_str()) != Some("Changed")
        {
            return false;
        }
        let Some(path) = header.path().map(|path| path.as_str().to_string()) else {
            return false;
        };
        let changed = match header.interface().map(|interface| interface.as_str()) {
            Some(MENUS_INTERFACE) => {
                let Ok(changes) = message.body().deserialize::<Vec<MenuChange>>() else {
                    return false;
                };
                let bus = self.menus.unique_bus_name.clone();
                let mut changed = false;
                for export in [&mut self.app_menu, &mut self.menubar] {
                    if export.path == path && export.changed(changes.clone()) {
                        subscribe(connection, &bus, export).await;
                        changed = true;
                    }
                }
                changed
            }
            Some(ACTIONS_INTERFACE) => {
                let Some((prefix, _)) = action_groups(&self.menus)
                    .into_iter()
                    .find(|(_, group_path)| *group_path == path)
                else {
                    return false;
                };
                let Ok((removals, enabled, states, additions)) = message.body().deserialize::<(
                    Vec<String>,
                    HashMap<String, bool>,
                    HashMap<String, OwnedValue>,
                    HashMap<String, ActionDescription>,
                )>() else {
                    return false;
                };
                self.actions
                    .changed(prefix, removals, enabled, states, additions);
                true
            }
            _ => false,
        };
        if changed {
            let before = std::mem::take(&mut self.tree);
            self.rebuild();
            return self.tree != before;
        }
        false
    }

    pub async fn activate(&self, connection: &zbus::Connection, id: i32) {
        let Some(Target { action, target }) = self.tree.targets.get(&id) else {
            return;
        };
        let Some((path, name)) = action_object(&self.menus, action) else {
            tracing::warn!(action = %action, "menubar: GTK action without an exported object");
            return;
        };
        let parameter: Vec<Value<'_>> = target
            .iter()
            .filter_map(|target| target.try_clone().ok().map(Value::from))
            .collect();
        let platform_data: HashMap<&str, Value<'_>> = HashMap::new();
        let result = call(
            connection,
            &self.menus.unique_bus_name,
            path,
            ACTIONS_INTERFACE,
            "Activate",
            &(name, parameter, platform_data),
        )
        .await;
        match result {
            Ok(_) => tracing::info!(action = %action, "menubar: org.gtk.Actions Activate sent"),
            Err(error) => {
                tracing::warn!(%error, action = %action, "menubar: org.gtk.Actions Activate failed")
            }
        }
    }

    fn rebuild(&mut self) {
        self.tree = Tree::build(&self.app_menu, &self.menubar, &self.actions, &self.app_name);
    }
}

async fn subscribe(connection: &zbus::Connection, bus: &str, export: &mut Export) {
    if export.path.is_empty() {
        return;
    }
    loop {
        let groups = if export.subscribed.is_empty() {
            vec![0]
        } else {
            export.missing_groups()
        };
        if groups.is_empty() {
            return;
        }
        let reply = call(
            connection,
            bus,
            &export.path,
            MENUS_INTERFACE,
            "Start",
            &(groups.clone(),),
        )
        .await;
        match reply.and_then(|message| message.body().deserialize::<Vec<StartEntry>>()) {
            Ok(entries) => {
                tracing::info!(path = %export.path, ?groups, menus = entries.len(), "menubar: org.gtk.Menus Start");
                export.started(&groups, entries);
            }
            Err(error) => {
                tracing::warn!(%error, path = %export.path, ?groups, "menubar: org.gtk.Menus Start failed");
                export.subscribed.extend(groups);
                return;
            }
        }
    }
}

async fn call<B>(
    connection: &zbus::Connection,
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let call = connection.call_method(Some(destination), path, Some(interface), method, body);
    timed(CALL_TIMEOUT, call)
        .await
        .unwrap_or_else(|| Err(zbus::Error::Failure(format!("{method} timed out"))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menubar::dbusmenu_map::menu_items;
    use barrier_kit::menu::strip_mnemonic;
    use barrier_kit::menu::{CheckState, MenuItem};

    fn attrs(pairs: &[(&str, OwnedValue)]) -> Attributes {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    fn pair(group: u32, menu: u32) -> OwnedValue {
        OwnedValue::try_from(Value::from((group, menu))).unwrap()
    }

    fn actions(names: &[(&str, bool, Option<OwnedValue>)]) -> Actions {
        let mut actions = Actions::default();
        for (name, enabled, state) in names {
            let (prefix, name) = name.split_once('.').unwrap();
            actions.changed(
                &format!("{prefix}."),
                Vec::new(),
                HashMap::new(),
                HashMap::new(),
                HashMap::from([(
                    name.to_string(),
                    (
                        *enabled,
                        Signature::try_from("s").unwrap(),
                        state.iter().cloned().collect(),
                    ),
                )]),
            );
        }
        actions
    }

    fn demo() -> Export {
        let mut export = Export::new("/org/gtk/Demo2/menus/menubar");
        export.started(
            &[0],
            vec![(
                0,
                0,
                vec![attrs(&[
                    ("label", text("_Preferences")),
                    (":submenu", pair(1, 0)),
                ])],
            )],
        );
        export.started(
            &[1],
            vec![
                (1, 0, vec![attrs(&[(":section", pair(1, 1))])]),
                (
                    1,
                    1,
                    vec![
                        attrs(&[
                            ("label", text("_Prefer Dark")),
                            ("action", text("app.dark")),
                            ("accel", text("<Primary>d")),
                        ]),
                        attrs(&[
                            ("label", text("_Red")),
                            ("action", text("win.color")),
                            ("target", text("red")),
                        ]),
                        attrs(&[
                            ("label", text("_Gone")),
                            ("action", text("app.gone")),
                            ("hidden-when", text("action-missing")),
                        ]),
                    ],
                ),
            ],
        );
        export
    }

    #[test]
    fn the_demo_menubar_maps_through_the_dbusmenu_mapper() {
        let actions = actions(&[
            ("app.dark", true, Some(OwnedValue::from(true))),
            ("win.color", true, Some(text("red"))),
        ]);
        let tree = Tree::build(&Export::default(), &demo(), &actions, "Demo");
        let items = menu_items(&tree.items);
        let MenuItem::Submenu { label, items, .. } = &items[0] else {
            panic!("Preferences is a submenu");
        };
        assert_eq!(strip_mnemonic(label), "Preferences");
        assert_eq!(
            items.len(),
            2,
            "section separators collapse; Gone is hidden"
        );
        let MenuItem::Action {
            label,
            shortcut,
            check,
            ..
        } = &items[0]
        else {
            panic!("an action");
        };
        assert_eq!(strip_mnemonic(label), "Prefer Dark");
        assert_eq!(shortcut.as_deref(), Some("Control+D"));
        assert_eq!(*check, CheckState::Checkbox(true));
        assert!(matches!(
            items[1],
            MenuItem::Action {
                check: CheckState::Radio(true),
                ..
            }
        ));
    }

    #[test]
    fn ids_resolve_to_actions_and_targets() {
        let actions = actions(&[("win.color", true, Some(text("blue")))]);
        let tree = Tree::build(&Export::default(), &demo(), &actions, "Demo");
        let red = tree
            .targets
            .values()
            .find(|target| target.action == "win.color")
            .unwrap();
        assert_eq!(red.target, Some(text("red")));
        let preferences = tree.items[0].id;
        assert_eq!(tree.children(preferences).unwrap().len(), 4);
    }

    #[test]
    fn a_missing_or_disabled_action_greys_the_item() {
        let tree = Tree::build(&Export::default(), &demo(), &Actions::default(), "Demo");
        let items = menu_items(&tree.items);
        let MenuItem::Submenu { items, .. } = &items[0] else {
            panic!();
        };
        assert!(
            items
                .iter()
                .all(|item| matches!(item, MenuItem::Action { enabled: false, .. }))
        );
    }

    #[test]
    fn the_app_menu_becomes_a_title_named_after_the_app() {
        let mut app = Export::new("/org/gtk/Demo2/menus/appmenu");
        app.started(
            &[0],
            vec![(
                0,
                0,
                vec![attrs(&[
                    ("label", text("_Quit")),
                    ("action", text("app.quit")),
                ])],
            )],
        );
        let actions = actions(&[("app.quit", true, None)]);
        let tree = Tree::build(&app, &demo(), &actions, "Demo");
        let items = menu_items(&tree.items);
        assert_eq!(items[0].label(), Some("Demo"));
        assert_eq!(
            items[1].label().map(strip_mnemonic).as_deref(),
            Some("Preferences")
        );
    }

    #[test]
    fn the_libadwaita_primary_menu_takes_the_app_name() {
        let mut menubar = Export::new("/org/gnome/Demo/menus/menubar");
        menubar.started(
            &[0, 1],
            vec![
                (
                    0,
                    0,
                    vec![attrs(&[
                        ("label", text("demo-binary")),
                        (ADW_APP_MENU, OwnedValue::from(true)),
                        (":submenu", pair(1, 0)),
                    ])],
                ),
                (
                    1,
                    0,
                    vec![attrs(&[
                        ("label", text("_About")),
                        ("action", text("app.about")),
                    ])],
                ),
            ],
        );
        let actions = actions(&[("app.about", true, None)]);
        let tree = Tree::build(&Export::new(""), &menubar, &actions, "Demo");
        let items = menu_items(&tree.items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label(), Some("Demo"));
    }

    fn calculator_primary_menu() -> (Export, Actions) {
        let mut menubar = Export::new("/org/gnome/Calculator/menus/menubar");
        let item =
            |label: &str, action: &str| attrs(&[("label", text(label)), ("action", text(action))]);
        menubar.started(
            &[0, 1],
            vec![
                (
                    0,
                    0,
                    vec![attrs(&[
                        ("label", text("gnome-calculator")),
                        (ADW_APP_MENU, OwnedValue::from(true)),
                        (":submenu", pair(1, 0)),
                    ])],
                ),
                (
                    1,
                    0,
                    vec![
                        attrs(&[(":section", pair(1, 1))]),
                        attrs(&[(":section", pair(1, 2))]),
                        attrs(&[(":section", pair(1, 3))]),
                    ],
                ),
                (1, 1, vec![item("_Ventana nueva", "app.new-window")]),
                (1, 2, vec![item("_Limpiar histórico", "win.clear-history")]),
                (
                    1,
                    3,
                    vec![
                        item("_Preferencias", "app.preferences"),
                        item("_Atajos de teclado", "win.show-help-overlay"),
                        item("_Ayuda", "app.help"),
                        item("_Acerca de Calculadora", "app.about"),
                    ],
                ),
            ],
        );
        let actions = actions(&[
            ("app.new-window", true, None),
            ("win.clear-history", true, None),
            ("app.preferences", true, None),
            ("win.show-help-overlay", true, None),
            ("app.help", true, None),
            ("app.about", true, None),
        ]);
        (menubar, actions)
    }

    #[test]
    fn the_primary_menu_is_split_by_what_each_entry_does() {
        let (menubar, actions) = calculator_primary_menu();
        let tree = Tree::build(&Export::new(""), &menubar, &actions, "Calculadora");
        let items = menu_items(&tree.items);
        let titles: Vec<_> = items.iter().filter_map(|item| item.label()).collect();
        assert_eq!(
            titles,
            vec!["Calculadora", tr!("menu-file"), tr!("menu-help")]
        );
        let children = |index: usize| match &items[index] {
            MenuItem::Submenu { items, .. } => items
                .iter()
                .filter_map(|item| item.label().map(strip_mnemonic))
                .collect::<Vec<_>>(),
            _ => panic!("expected a submenu"),
        };
        assert_eq!(
            children(0),
            vec![
                tr!("menu-settings").to_string(),
                tr!("app-about", app = "Calculadora")
            ]
        );
        assert_eq!(children(1), vec!["Ventana nueva", "Limpiar histórico"]);
        assert_eq!(children(2), vec!["Atajos de teclado", "Ayuda"]);
    }

    #[test]
    fn without_new_open_or_save_the_extra_title_is_opciones() {
        assert!(opens_files("app.new-window"));
        assert!(opens_files("win.new-tab"));
        assert!(opens_files("win.save-as"));
        assert!(opens_files("app.open"));
        assert!(!opens_files("win.clear-history"));
        assert!(!opens_files("win.newline"));
        assert_eq!(primary_slot("win.fullscreen"), PrimarySlot::Window);
        assert_eq!(primary_slot("app.shortcuts"), PrimarySlot::Help);
        assert_eq!(primary_slot("win.number-format"), PrimarySlot::Other);
    }

    #[test]
    fn changes_splice_and_reveal_new_groups() {
        let mut export = demo();
        assert!(export.missing_groups().is_empty());
        let changed = export.changed(vec![(
            0,
            0,
            1,
            0,
            vec![attrs(&[("label", text("_Help")), (":submenu", pair(2, 0))])],
        )]);
        assert!(changed);
        assert_eq!(export.missing_groups(), vec![2]);
        assert!(!export.changed(vec![(9, 9, 0, 0, Vec::new())]));
    }

    #[test]
    fn a_new_menu_in_a_subscribed_group_is_added() {
        let mut export = demo();
        assert!(export.changed(vec![(0, 7, 0, 0, vec![attrs(&[("label", text("Nueva"))])])]));
        assert_eq!(export.menu(0, 7).len(), 1);
    }

    #[test]
    fn accelerators_become_kit_shortcuts() {
        assert_eq!(
            accel_keys("<Primary>s").unwrap(),
            vec!["Control".to_string(), "s".to_string()]
        );
        assert_eq!(
            accel_keys("<Shift><Control>z").unwrap(),
            vec!["Shift", "Control", "z"]
        );
        let comma = accel_keys("<Primary>comma").unwrap();
        assert_eq!(barrier_kit::menu::format_shortcut(&comma.join("+")), "⌃,");
        assert_eq!(accel_keys("F11").unwrap(), vec!["F11"]);
        assert_eq!(accel_keys("<Bogus>x"), None);
        assert_eq!(accel_keys("<Primary>"), None);
    }

    #[test]
    fn actions_route_to_their_object() {
        let menus = GtkMenusAddress {
            unique_bus_name: ":1.36".to_string(),
            application_id: "org.gtk.Demo2".to_string(),
            menubar_path: "/org/gtk/Demo2/menus/menubar".to_string(),
            app_menu_path: String::new(),
            application_object_path: "/org/gtk/Demo2".to_string(),
            window_object_path: "/org/gtk/Demo2/window/1".to_string(),
        };
        assert_eq!(
            action_object(&menus, "app.quit"),
            Some(("/org/gtk/Demo2", "quit"))
        );
        assert_eq!(
            action_object(&menus, "win.copy"),
            Some(("/org/gtk/Demo2/window/1", "copy"))
        );
        assert_eq!(
            action_object(&menus, "unity.file-quit"),
            Some(("/org/gtk/Demo2/menus/menubar", "file-quit"))
        );
        assert_eq!(action_object(&menus, "doc.save"), None);
        assert_eq!(address(&menus).path, "/org/gtk/Demo2/menus/menubar");
    }
}
