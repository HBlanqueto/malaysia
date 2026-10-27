//! App-side global menu export (ADR 0040 §10).
//!
//! BarrierKit apps declare a menu with [`AppMenu`] and receive activations
//! through [`WindowOptions::on_menu_action`][crate::platform::WindowOptions].
//! The kit exports the menu over D-Bus as `com.canonical.dbusmenu` and
//! announces the address through `org_kde_kwin_appmenu`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::zvariant::{OwnedValue, Value};

use crate::menu::{CheckState, MenuItem};
use crate::platform::WakeHandle;

pub mod layout;
pub mod server;

/// A declarative application menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppMenu {
    pub menus: Vec<Menu>,
}

impl AppMenu {
    pub fn new(menus: Vec<Menu>) -> Self {
        Self { menus }
    }
}

/// One top-level menu (e.g. File, Edit).
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

/// Handle to mutate a menu after the window has been created.
#[derive(Clone, Debug)]
pub struct MenuHandle {
    state: Arc<Mutex<MenuState>>,
    wake: Arc<Mutex<Option<WakeHandle>>>,
    path: Arc<Mutex<Option<String>>>,
}

#[derive(Clone, Debug)]
struct MenuState {
    menu: AppMenu,
    revision: u32,
}

impl MenuHandle {
    pub(crate) fn new(menu: AppMenu, wake: Option<WakeHandle>) -> Self {
        Self {
            state: Arc::new(Mutex::new(MenuState { menu, revision: 1 })),
            wake: Arc::new(Mutex::new(wake)),
            path: Arc::new(Mutex::new(None)),
        }
    }

    /// Builds a handle for tests that do not need a render wake.
    #[doc(hidden)]
    pub fn for_test(menu: AppMenu) -> Self {
        Self::new(menu, None)
    }

    pub(crate) fn set_path(&self, path: impl Into<String>) {
        if let Ok(mut slot) = self.path.lock() {
            *slot = Some(path.into());
        }
    }

    /// Updates one item in place, identified by its [`MenuItem::Action`] or
    /// [`MenuItem::Submenu`] id. The change is published to the global menu.
    pub fn update(&self, id: impl AsRef<str>, f: impl Fn(&mut MenuItem)) {
        let mut state = self.state.lock().expect("menu state poisoned");
        if update_item(&mut state.menu, id.as_ref(), &f) {
            state.revision = state.revision.wrapping_add(1);
            drop(state);
            if let Ok(slot) = self.path.lock()
                && let Some(path) = slot.as_ref()
            {
                server::update(path);
            }
            if let Ok(slot) = self.wake.lock()
                && let Some(wake) = slot.as_ref()
            {
                wake.wake();
            }
        }
    }

    /// The live menu and the current dbusmenu revision.
    /// Returns the current menu and its revision.
    #[doc(hidden)]
    pub fn snapshot(&self) -> (AppMenu, u32) {
        let state = self.state.lock().expect("menu state poisoned");
        (state.menu.clone(), state.revision)
    }
}

fn update_item(menu: &mut AppMenu, id: &str, f: &dyn Fn(&mut MenuItem)) -> bool {
    for m in &mut menu.menus {
        if update_menu_items(&mut m.items, id, f) {
            return true;
        }
    }
    false
}

fn update_menu_items(items: &mut [MenuItem], id: &str, f: &dyn Fn(&mut MenuItem)) -> bool {
    for item in items {
        let matched = match item {
            MenuItem::Action { id: item_id, .. } => item_id == id,
            MenuItem::Submenu { id: item_id, .. } => item_id.as_deref() == Some(id),
            _ => false,
        };
        if matched {
            f(item);
            return true;
        }
        if let MenuItem::Submenu {
            items: children, ..
        } = item
            && update_menu_items(children, id, f)
        {
            return true;
        }
    }
    false
}

/// Stable ids for dbusmenu nodes.
///
/// Actions keep their textual id; top-level menus and static submenus get
/// synthetic ids based on their position and label so they stay stable across
/// hot updates.
#[derive(Clone, Debug, Default)]
#[doc(hidden)]
pub struct IdMap {
    ids: HashMap<String, i32>,
    next: i32,
}

impl IdMap {
    #[doc(hidden)]
    pub fn get(&mut self, key: impl Into<String>) -> i32 {
        let key = key.into();
        if let Some(id) = self.ids.get(&key) {
            return *id;
        }
        let id = self.next;
        self.next += 1;
        self.ids.insert(key, id);
        id
    }
}

/// Turns a kit shortcut string into a dbusmenu key list.
///
/// `Ctrl+Shift+S` becomes `["Control", "Shift", "S"]`; `Super` is kept as
/// `"Super"` and never mapped to "Command".
pub(crate) fn dbusmenu_shortcut(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let mut keys = Vec::new();
    for token in text.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        let normalized = match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "Control",
            "alt" | "option" | "opt" => "Alt",
            "shift" => "Shift",
            "super" | "meta" | "cmd" | "command" => "Super",
            "up" => "Up",
            "down" => "Down",
            "left" => "Left",
            "right" => "Right",
            "return" | "enter" => "Return",
            "tab" => "Tab",
            "escape" | "esc" => "Escape",
            _ => token,
        };
        keys.push(normalized.to_string());
    }
    keys
}

/// Escapes a label for dbusmenu: a literal `_` becomes `__`; mnemonic
/// underlines are not used.
pub(crate) fn dbusmenu_label(label: &str) -> String {
    label.replace('_', "__")
}

use keyboard_types::{Key, Modifiers, NamedKey};

/// Parsed shortcut for in-app matching.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedShortcut {
    pub modifiers: Modifiers,
    pub key: String,
}

/// Parses a shortcut written as text (`Ctrl+Shift+S`, `Alt+Up`) into modifiers
/// and a normalised key name.
pub(crate) fn parse_shortcut(text: &str) -> ParsedShortcut {
    let text = text.trim();
    let mut modifiers = Modifiers::empty();
    let mut key = String::new();
    for token in text.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" | "option" | "opt" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "super" | "meta" | "cmd" | "command" => modifiers |= Modifiers::META,
            "escape" | "esc" => key = "esc".to_string(),
            "left" => key = "left".to_string(),
            "right" => key = "right".to_string(),
            "up" => key = "up".to_string(),
            "down" => key = "down".to_string(),
            "return" | "enter" => key = "return".to_string(),
            "tab" => key = "tab".to_string(),
            _ => key = token.to_ascii_lowercase(),
        }
    }
    ParsedShortcut { modifiers, key }
}

fn normalised_key(key: &Key) -> String {
    match key {
        Key::Character(s) => s.to_ascii_lowercase(),
        Key::Named(n) => match n {
            NamedKey::ArrowLeft => "left".to_string(),
            NamedKey::ArrowRight => "right".to_string(),
            NamedKey::ArrowUp => "up".to_string(),
            NamedKey::ArrowDown => "down".to_string(),
            NamedKey::Enter => "return".to_string(),
            NamedKey::Tab => "tab".to_string(),
            NamedKey::Escape => "esc".to_string(),
            NamedKey::Backspace => "backspace".to_string(),
            NamedKey::Delete => "delete".to_string(),
            NamedKey::Home => "home".to_string(),
            NamedKey::End => "end".to_string(),
            NamedKey::PageUp => "pageup".to_string(),
            NamedKey::PageDown => "pagedown".to_string(),
            _ => String::new(),
        },
    }
}

/// True when `event` matches `shortcut` and the action is enabled.
pub fn shortcut_matches(text: &str, modifiers: Modifiers, key: &Key) -> bool {
    let parsed = parse_shortcut(text);
    parsed.modifiers == modifiers && parsed.key == normalised_key(key) && !parsed.key.is_empty()
}

/// Finds the first enabled action whose shortcut matches the key event.
pub fn find_menu_shortcut(menu: &AppMenu, modifiers: Modifiers, key: &Key) -> Option<String> {
    for m in &menu.menus {
        if let Some(id) = find_shortcut_in_items(&m.items, modifiers, key) {
            return Some(id);
        }
    }
    None
}

fn find_shortcut_in_items(items: &[MenuItem], modifiers: Modifiers, key: &Key) -> Option<String> {
    for item in items {
        match item {
            MenuItem::Action {
                id,
                shortcut: Some(shortcut),
                enabled: true,
                ..
            } => {
                if shortcut_matches(shortcut, modifiers, key) {
                    return Some(id.clone());
                }
            }
            MenuItem::Submenu {
                items: children, ..
            } => {
                if let Some(id) = find_shortcut_in_items(children, modifiers, key) {
                    return Some(id);
                }
            }
            _ => {}
        }
    }
    None
}

fn owned<T: Into<Value<'static>>>(value: T) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("value is ownable")
}

#[derive(Debug, Clone, PartialEq)]
/// A dbusmenu item for testing the round-trip between BarrierKit and the
/// menubar decoder. Not used at runtime.
pub struct RawItem {
    pub id: i32,
    pub properties: HashMap<String, OwnedValue>,
    pub children: Vec<RawItem>,
}

/// Exports a kit menu to dbusmenu `RawItem` form for tests in other crates.
pub fn to_dbusmenu(menu: &AppMenu) -> RawItem {
    let mut id_map = IdMap::default();
    let mut root = RawItem {
        id: 0,
        properties: HashMap::new(),
        children: Vec::new(),
    };
    for m in &menu.menus {
        let menu_id = id_map.get(format!("__menu__:{}", m.title));
        root.children.push(menu_to_raw(menu_id, m, &mut id_map));
    }
    root
}

fn menu_to_raw(id: i32, menu: &Menu, id_map: &mut IdMap) -> RawItem {
    let mut props = HashMap::new();
    props.insert("label".to_string(), owned(dbusmenu_label(&menu.title)));
    props.insert("children-display".to_string(), owned("submenu"));
    let children = menu
        .items
        .iter()
        .map(|item| item_to_raw(item, id_map))
        .collect();
    RawItem {
        id,
        properties: props,
        children,
    }
}

fn item_to_raw(item: &MenuItem, id_map: &mut IdMap) -> RawItem {
    match item {
        MenuItem::Separator => {
            let id = id_map.get("__separator__");
            let mut props = HashMap::new();
            props.insert("type".to_string(), owned("separator"));
            RawItem {
                id,
                properties: props,
                children: Vec::new(),
            }
        }
        MenuItem::Action {
            id: action_id,
            label,
            shortcut,
            enabled,
            check,
            ..
        } => {
            let id = id_map.get(action_id.as_str());
            let mut props = HashMap::new();
            props.insert("label".to_string(), owned(dbusmenu_label(label)));
            props.insert("enabled".to_string(), owned(*enabled));
            if let Some(shortcut) = shortcut {
                let keys = dbusmenu_shortcut(shortcut);
                if !keys.is_empty() {
                    let combo: Vec<Value<'static>> = keys.into_iter().map(Value::from).collect();
                    let array = zbus::zvariant::Array::from(vec![Value::Array(
                        zbus::zvariant::Array::from(combo),
                    )]);
                    props.insert("shortcut".to_string(), owned(array));
                }
            }
            let (toggle_type, toggle_state) = check_state(check);
            if let Some(toggle_type) = toggle_type {
                props.insert("toggle-type".to_string(), owned(toggle_type));
                props.insert("toggle-state".to_string(), owned(toggle_state));
            }
            RawItem {
                id,
                properties: props,
                children: Vec::new(),
            }
        }
        MenuItem::Submenu {
            id: submenu_id,
            label,
            enabled,
            items,
            ..
        } => {
            let key = submenu_id
                .as_deref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("__submenu__:{label}"));
            let id = id_map.get(key);
            let mut props = HashMap::new();
            props.insert("label".to_string(), owned(dbusmenu_label(label)));
            props.insert("enabled".to_string(), owned(*enabled));
            props.insert("children-display".to_string(), owned("submenu"));
            let children = items.iter().map(|item| item_to_raw(item, id_map)).collect();
            RawItem {
                id,
                properties: props,
                children,
            }
        }
        _ => {
            let id = id_map.get("__ignored__");
            RawItem {
                id,
                properties: HashMap::new(),
                children: Vec::new(),
            }
        }
    }
}

pub(crate) fn check_state(check: &CheckState) -> (Option<&'static str>, i32) {
    match check {
        CheckState::Checkbox(active) => (Some("checkmark"), *active as i32),
        CheckState::Radio(active) => (Some("radio"), *active as i32),
        CheckState::None => (None, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_parses_modifiers_and_keeps_super() {
        assert_eq!(
            dbusmenu_shortcut("Ctrl+Shift+S"),
            vec!["Control", "Shift", "S"]
        );
        assert_eq!(
            dbusmenu_shortcut("Super+Alt+Left"),
            vec!["Super", "Alt", "Left"]
        );
    }

    #[test]
    fn labels_escape_underscores() {
        assert_eq!(dbusmenu_label("a_b"), "a__b");
        assert_eq!(dbusmenu_label("Archivo"), "Archivo");
    }

    #[test]
    fn ctrl_shift_s_matches() {
        assert!(shortcut_matches(
            "Ctrl+Shift+S",
            Modifiers::CONTROL | Modifiers::SHIFT,
            &Key::Character("s".into())
        ));
    }

    #[test]
    fn ctrl_shift_z_uppercase_still_matches() {
        // With the kit fix Shift+Z arrives as Character("Z"), not lowercased.
        assert!(shortcut_matches(
            "Ctrl+Shift+Z",
            Modifiers::CONTROL | Modifiers::SHIFT,
            &Key::Character("Z".into())
        ));
    }

    #[test]
    fn disabled_action_does_not_fire() {
        let menu = AppMenu::new(vec![Menu::new(
            "Archivo",
            vec![MenuItem::Action {
                id: "save".into(),
                label: "Guardar".into(),
                shortcut: Some("Ctrl+S".into()),
                enabled: false,
                check: CheckState::None,
                icon: None,
            }],
        )]);
        assert!(
            find_menu_shortcut(
                &menu,
                Modifiers::CONTROL | Modifiers::SHIFT,
                &Key::Character("s".into())
            )
            .is_none()
        );
    }

    #[test]
    fn plain_typing_does_not_trigger_shortcut() {
        assert!(!shortcut_matches(
            "Ctrl+R",
            Modifiers::empty(),
            &Key::Character("r".into())
        ));
    }
}
