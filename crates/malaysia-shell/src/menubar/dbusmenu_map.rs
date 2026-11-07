use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

use bk::menu::{CheckState, MNEMONIC_MARK, MenuItem, normalize_separators};
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
];

pub fn menu_items(items: &[RawItem]) -> Vec<MenuItem> {
    let mut mapped: Vec<MenuItem> = items.iter().filter_map(menu_item).collect();
    normalize_separators(&mut mapped);
    mapped
}

fn menu_item(item: &RawItem) -> Option<MenuItem> {
    let props = &item.properties;
    for key in props.keys() {
        if !KNOWN_PROPERTIES.contains(&key.as_str()) {
            debug_once(key, props.get(key));
        }
    }
    if !prop(props, "visible").unwrap_or(true) {
        return None;
    }
    if prop::<String>(props, "type").as_deref() == Some("separator") {
        return Some(MenuItem::Separator);
    }

    let label = mnemonic(&prop::<String>(props, "label").unwrap_or_default());
    let enabled = prop(props, "enabled").unwrap_or(true);
    let children = menu_items(&item.children);

    if prop::<String>(props, "children-display").as_deref() == Some("submenu") {
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
        badge: None,
    })
}

pub(crate) fn prop<T: TryFrom<OwnedValue>>(
    props: &HashMap<String, OwnedValue>,
    key: &str,
) -> Option<T> {
    props
        .get(key)
        .and_then(|value| T::try_from(value.clone()).ok())
}

fn mnemonic(text: &str) -> String {
    let mut out = String::new();
    let mut marked = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '_' {
            out.push(ch);
        } else if chars.peek() == Some(&'_') {
            chars.next();
            out.push('_');
        } else if !marked && chars.peek().is_some() {
            marked = true;
            out.push(MNEMONIC_MARK);
        }
    }
    out
}

fn toggle(props: &HashMap<String, OwnedValue>) -> CheckState {
    let active = prop::<i32>(props, "toggle-state") == Some(1);
    match prop::<String>(props, "toggle-type").as_deref() {
        Some("checkmark") => CheckState::Checkbox(active),
        Some("radio") => CheckState::Radio(active),
        _ => CheckState::None,
    }
}

fn shortcut(props: &HashMap<String, OwnedValue>) -> Option<String> {
    let combinations: Vec<Vec<String>> = prop(props, "shortcut")?;
    let keys = combinations.first()?;
    if keys.is_empty() {
        return None;
    }
    Some(keys.join("+"))
}

fn debug_once(key: &str, value: Option<&OwnedValue>) {
    static SEEN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    if SEEN
        .lock()
        .is_ok_and(|mut seen| seen.insert(key.to_string()))
    {
        tracing::debug!(property = key, value = ?value, "menubar: unsupported dbusmenu property");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bk::menu::strip_mnemonic;
    use bk::menu::{CheckState, MenuItem};
    use std::collections::HashMap;

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

    #[test]
    fn separators_between_actions_are_kept() {
        let items = menu_items(&[
            item(1, &[("label", text("A"))], Vec::new()),
            item(2, &[("type", text("separator"))], Vec::new()),
            item(3, &[("label", text("B"))], Vec::new()),
        ]);
        assert_eq!(items.len(), 3);
        assert!(matches!(items[1], MenuItem::Separator));
    }

    #[test]
    fn a_run_left_after_dropping_invisible_items_collapses() {
        let hidden = item(
            3,
            &[
                ("label", text("hidden")),
                ("visible", OwnedValue::from(false)),
            ],
            Vec::new(),
        );
        let items = menu_items(&[
            item(1, &[("label", text("A"))], Vec::new()),
            item(2, &[("type", text("separator"))], Vec::new()),
            hidden,
            item(4, &[("type", text("separator"))], Vec::new()),
            item(5, &[("label", text("B"))], Vec::new()),
        ]);
        assert_eq!(items.len(), 3);
        assert!(matches!(items[1], MenuItem::Separator));
        assert_eq!(items[2].label(), Some("B"));
    }

    #[test]
    fn mnemonics_become_the_kit_mark() {
        let items = menu_items(&[
            item(1, &[("label", text("_Archivo"))], Vec::new()),
            item(2, &[("label", text("Sele_cción"))], Vec::new()),
            item(3, &[("label", text("a__b"))], Vec::new()),
            item(4, &[("label", text("_Uno _Dos"))], Vec::new()),
        ]);
        let mark = MNEMONIC_MARK;
        assert_eq!(
            items.iter().filter_map(MenuItem::label).collect::<Vec<_>>(),
            vec![
                format!("{mark}Archivo").as_str(),
                format!("Sele{mark}cción").as_str(),
                "a_b",
                format!("{mark}Uno Dos").as_str(),
            ]
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
            items.iter().filter_map(MenuItem::label).collect::<Vec<_>>(),
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
            } if id == "7" && strip_mnemonic(label) == "Archivo" && items.is_empty()
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
