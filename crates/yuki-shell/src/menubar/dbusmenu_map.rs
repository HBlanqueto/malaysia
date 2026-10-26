//! Pure mapping from dbusmenu layouts to [`MenuItem`] (ADR 0040 §5).
//!
//! Property names and value types follow `com.canonical.dbusmenu`; anything
//! the bar does not understand is logged once and ignored so a broken item
//! never breaks the whole menu.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use celestial_kit::menu::{CheckState, MenuItem};
use zbus::zvariant::OwnedValue;

use crate::menubar::dbusmenu::RawItem;

const KNOWN_PROPERTIES: &[&str] = &[
    "label",
    "enabled",
    "visible",
    "type",
    "children-display",
    "toggle-type",
    "toggle-state",
    "shortcut",
    "icon-name",
    "icon-data",
];

/// Maps a dbusmenu child list to menu items, dropping invisible entries.
pub fn menu_items(items: &[RawItem]) -> Vec<MenuItem> {
    items.iter().filter_map(menu_item).collect()
}

fn menu_item(item: &RawItem) -> Option<MenuItem> {
    let props = &item.properties;
    for key in props.keys() {
        if !KNOWN_PROPERTIES.contains(&key.as_str()) {
            debug_once(key, props.get(key));
        }
    }
    if !bool_prop(props, "visible", true) {
        return None;
    }
    if string(props, "type").as_deref() == Some("separator") {
        return Some(MenuItem::Separator);
    }

    let label = mnemonic(&string(props, "label").unwrap_or_default());
    let enabled = bool_prop(props, "enabled", true);
    // App menus never draw icons (14d-4): only three of Kate's 89 entries
    // carry one, and a mostly bare column looks broken. The kit keeps
    // `MenuIcon::Image` for the apps of 14e and future use.
    for key in ["icon-name", "icon-data"] {
        if props.contains_key(key) {
            debug_once(key, props.get(key));
        }
    }
    let children = menu_items(&item.children);

    if string(props, "children-display").as_deref() == Some("submenu") {
        return Some(MenuItem::Submenu {
            id: Some(item.id.to_string()),
            label,
            enabled,
            items: children,
            icon: None,
        });
    }

    Some(MenuItem::Action {
        id: format!("dbusmenu:{}", item.id),
        label,
        shortcut: shortcut(props),
        enabled,
        check: toggle(props),
        icon: None,
    })
}

fn string(props: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
}

fn bool_prop(props: &HashMap<String, OwnedValue>, key: &str, default: bool) -> bool {
    props
        .get(key)
        .and_then(|value| bool::try_from(value.clone()).ok())
        .unwrap_or(default)
}

/// Removes the dbusmenu mnemonic underline: `_x` becomes `x` and `__` becomes
/// a literal `_`. No underline is drawn.
fn mnemonic(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '_' {
            if chars.peek() == Some(&'_') {
                chars.next();
                out.push('_');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn toggle(props: &HashMap<String, OwnedValue>) -> CheckState {
    let active = props
        .get("toggle-state")
        .and_then(|value| i32::try_from(value.clone()).ok())
        == Some(1);
    match string(props, "toggle-type").as_deref() {
        Some("checkmark") => CheckState::Checkbox(active),
        Some("radio") => CheckState::Radio(active),
        _ => CheckState::None,
    }
}

/// Only the first key combination; the kit formats it with `⌃⌥⇧❖`.
fn shortcut(props: &HashMap<String, OwnedValue>) -> Option<String> {
    let combinations = props
        .get("shortcut")
        .and_then(|value| Vec::<Vec<String>>::try_from(value.clone()).ok())?;
    let keys = combinations.first()?;
    if keys.is_empty() {
        return None;
    }
    Some(keys.join("+"))
}

fn debug_once(key: &str, value: Option<&OwnedValue>) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut seen) = seen.lock() else {
        return;
    };
    if !seen.insert(key.to_string()) {
        return;
    }
    tracing::debug!(property = key, value = ?value, "menubar: unsupported dbusmenu property");
}

#[cfg(test)]
mod tests {
    use super::*;
    use celestial_kit::app_menu::{AppMenu, Menu, RawItem as KitRawItem};
    use celestial_kit::menu::{CheckState, MenuItem};

    fn text(value: &str) -> OwnedValue {
        OwnedValue::try_from(zbus::zvariant::Value::from(value)).unwrap()
    }

    fn props(pairs: &[(&str, OwnedValue)]) -> HashMap<String, OwnedValue> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    fn item(id: i32, pairs: &[(&str, OwnedValue)], children: Vec<RawItem>) -> RawItem {
        RawItem {
            id,
            properties: props(pairs),
            children,
        }
    }

    fn label(item: &MenuItem) -> Option<&str> {
        item.label()
    }

    #[test]
    fn separators_are_kept_and_do_not_need_a_label() {
        let items = menu_items(&[item(1, &[("type", text("separator"))], Vec::new())]);
        assert!(matches!(items.as_slice(), [MenuItem::Separator]));
    }

    #[test]
    fn mnemonics_are_removed() {
        let items = menu_items(&[
            item(1, &[("label", text("_Archivo"))], Vec::new()),
            item(2, &[("label", text("Sele_cción"))], Vec::new()),
            item(3, &[("label", text("a__b"))], Vec::new()),
        ]);
        assert_eq!(
            items.iter().filter_map(label).collect::<Vec<_>>(),
            vec!["Archivo", "Selección", "a_b"]
        );
    }

    #[test]
    fn toggle_types_map_to_checkboxes_and_radios() {
        let items = menu_items(&[
            item(
                1,
                &[
                    ("label", text("check")),
                    ("toggle-type", text("checkmark")),
                    ("toggle-state", OwnedValue::from(1i32)),
                ],
                Vec::new(),
            ),
            item(
                2,
                &[
                    ("label", text("radio")),
                    ("toggle-type", text("radio")),
                    ("toggle-state", OwnedValue::from(0i32)),
                ],
                Vec::new(),
            ),
        ]);
        assert!(matches!(
            items[0],
            MenuItem::Action {
                check: CheckState::Checkbox(true),
                ..
            }
        ));
        assert!(matches!(
            items[1],
            MenuItem::Action {
                check: CheckState::Radio(false),
                ..
            }
        ));
    }

    #[test]
    fn invisible_items_and_their_subtrees_are_omitted() {
        let hidden = item(
            2,
            &[
                ("label", text("hidden")),
                ("visible", OwnedValue::from(false)),
            ],
            vec![item(3, &[("label", text("child"))], Vec::new())],
        );
        let items = menu_items(&[item(1, &[("label", text("shown"))], Vec::new()), hidden]);
        assert_eq!(
            items.iter().filter_map(label).collect::<Vec<_>>(),
            vec!["shown"]
        );
    }

    #[test]
    fn the_first_shortcut_is_used() {
        let combinations: Vec<Vec<String>> = vec![
            vec!["Ctrl".to_string(), "Shift".to_string(), "S".to_string()],
            vec!["Alt".to_string(), "S".to_string()],
        ];
        let items = menu_items(&[item(
            1,
            &[
                ("label", text("Save")),
                (
                    "shortcut",
                    OwnedValue::try_from(zbus::zvariant::Value::from(combinations)).unwrap(),
                ),
            ],
            Vec::new(),
        )]);
        assert!(matches!(
            &items[0],
            MenuItem::Action {
                shortcut: Some(shortcut),
                id,
                ..
            } if shortcut == "Ctrl+Shift+S" && id == "dbusmenu:1"
        ));
    }

    fn kit_to_bar(item: &KitRawItem) -> RawItem {
        RawItem {
            id: item.id,
            properties: item.properties.clone(),
            children: item.children.iter().map(kit_to_bar).collect(),
        }
    }

    #[test]
    fn contador_menu_round_trips_through_dbusmenu_map() {
        let menu = AppMenu::new(vec![
            Menu::new(
                "Archivo",
                vec![
                    MenuItem::Action {
                        id: "new-window".to_string(),
                        label: "Nueva ventana".to_string(),
                        shortcut: Some("Ctrl+N".to_string()),
                        enabled: false,
                        check: CheckState::None,
                        icon: None,
                    },
                    MenuItem::Separator,
                    MenuItem::Action {
                        id: "close-window".to_string(),
                        label: "Cerrar ventana".to_string(),
                        shortcut: Some("Ctrl+W".to_string()),
                        enabled: true,
                        check: CheckState::None,
                        icon: None,
                    },
                ],
            ),
            Menu::new(
                "Contador",
                vec![
                    MenuItem::Action {
                        id: "reset".to_string(),
                        label: "Reiniciar".to_string(),
                        shortcut: Some("Ctrl+R".to_string()),
                        enabled: true,
                        check: CheckState::None,
                        icon: None,
                    },
                    MenuItem::Submenu {
                        id: Some("step-submenu".to_string()),
                        label: "Paso".to_string(),
                        enabled: true,
                        icon: None,
                        items: vec![MenuItem::Action {
                            id: "step-5".to_string(),
                            label: "5".to_string(),
                            shortcut: None,
                            enabled: true,
                            check: CheckState::Radio(true),
                            icon: None,
                        }],
                    },
                    MenuItem::Action {
                        id: "allow-negative".to_string(),
                        label: "Permitir negativos".to_string(),
                        shortcut: None,
                        enabled: true,
                        check: CheckState::Checkbox(false),
                        icon: None,
                    },
                ],
            ),
        ]);
        let root = celestial_kit::app_menu::to_dbusmenu(&menu);
        let items = menu_items(&root.children.iter().map(kit_to_bar).collect::<Vec<_>>());
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label(), Some("Archivo"));
        assert!(matches!(&items[0], MenuItem::Submenu { enabled: true, .. }));
        let archivo = match &items[0] {
            MenuItem::Submenu { items, .. } => items,
            _ => panic!(),
        };
        assert!(matches!(
            &archivo[0],
            MenuItem::Action {
                label,
                shortcut: Some(shortcut),
                enabled: false,
                ..
            } if label == "Nueva ventana" && shortcut == "Control+N"
        ));
        assert!(matches!(&archivo[1], MenuItem::Separator));
        assert!(matches!(
            &archivo[2],
            MenuItem::Action {
                label,
                shortcut: Some(shortcut),
                enabled: true,
                ..
            } if label == "Cerrar ventana" && shortcut == "Control+W"
        ));
        let contador = match &items[1] {
            MenuItem::Submenu { items, .. } => items,
            _ => panic!(),
        };
        assert!(matches!(
            &contador[0],
            MenuItem::Action {
                label,
                shortcut: Some(shortcut),
                check: CheckState::None,
                enabled: true,
                ..
            } if label == "Reiniciar" && shortcut == "Control+R"
        ));
        assert!(matches!(
            &contador[2],
            MenuItem::Action {
                label,
                check: CheckState::Checkbox(false),
                enabled: true,
                ..
            } if label == "Permitir negativos"
        ));
    }

    #[test]
    fn submenus_keep_their_id_and_empty_children() {
        let items = menu_items(&[item(
            7,
            &[
                ("label", text("_Archivo")),
                ("children-display", text("submenu")),
            ],
            Vec::new(),
        )]);
        assert!(matches!(
            &items[0],
            MenuItem::Submenu {
                id: Some(id),
                label,
                items,
                ..
            } if id == "7" && label == "Archivo" && items.is_empty()
        ));
    }

    #[test]
    fn enabled_defaults_to_true_and_app_icons_are_ignored() {
        let png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let data = OwnedValue::try_from(zbus::zvariant::Value::from(png)).unwrap();
        let items = menu_items(&[
            item(
                1,
                &[("label", text("about")), ("icon-name", text("info"))],
                Vec::new(),
            ),
            item(
                2,
                &[("label", text("bitmap")), ("icon-data", data)],
                Vec::new(),
            ),
            item(
                3,
                &[
                    ("label", text("submenu")),
                    ("children-display", text("submenu")),
                    ("icon-name", text("settings")),
                ],
                Vec::new(),
            ),
        ]);
        assert!(matches!(
            &items[0],
            MenuItem::Action {
                enabled: true,
                icon: None,
                ..
            }
        ));
        assert!(matches!(&items[1], MenuItem::Action { icon: None, .. }));
        assert!(matches!(&items[2], MenuItem::Submenu { icon: None, .. }));
    }
}
