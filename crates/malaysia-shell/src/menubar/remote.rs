use std::collections::HashSet;

use barrier_kit::menu::{
    ItemsSource, MenuItem, SearchLocation, SearchSource, SearchableEntry, normalize_query,
    normalize_separators, submenu_contents,
};

use crate::localize::tr;

#[derive(Clone)]
pub struct MenuSources {
    pub app_name: String,
    pub app_items: ItemsSource,
    pub window_items: ItemsSource,
    pub help_items: ItemsSource,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Title {
    pub label: String,
    pub items: Vec<MenuItem>,
    pub remote_id: Option<i32>,
    pub enabled: bool,
}

impl Title {
    pub fn visible_items(&self) -> Vec<MenuItem> {
        submenu_contents(self.remote_id.is_some(), &self.items)
    }
}

impl MenuSources {
    pub fn compose(&self, remote: &[MenuItem]) -> Vec<Title> {
        let mut app_items = (self.app_items)();
        let mut window_items = (self.window_items)();
        let mut help_items = (self.help_items)();
        let mut app_remote = None;
        let mut window_remote = None;
        let mut help_remote = None;
        let app_name = normalize_query(&self.app_name);
        let mut seen = HashSet::from([normalize_query(tr!("menubar-system"))]);
        let mut middle = Vec::new();
        let has_about_slot = app_items.iter().any(is_about_slot);
        let mut about = None;

        for item in remote {
            let (label, id, mut children, enabled) = match item {
                MenuItem::Submenu {
                    id,
                    label,
                    items,
                    enabled,
                    ..
                } => (label.clone(), submenu_id(id), items.clone(), *enabled),
                MenuItem::Action { label, .. } => (label.clone(), None, vec![item.clone()], true),
                _ => continue,
            };
            if has_about_slot && about.is_none() && matches!(item, MenuItem::Submenu { .. }) {
                about = take_about(&mut children);
            }
            let normalized = normalize_query(&label);
            if normalized == app_name {
                if seen.insert(normalized) {
                    let rest = fill_slots(&mut app_items, children);
                    append(&mut app_items, rest);
                    app_remote = id;
                }
            } else if normalized == "ventana" || normalized == "window" {
                if seen.insert(normalized) {
                    append(&mut window_items, children);
                    window_remote = id;
                }
            } else if normalized == "ayuda" || normalized == "help" {
                if seen.insert(normalized) {
                    insert_after_search(&mut help_items, children);
                    help_remote = id;
                }
            } else if seen.insert(normalized) {
                middle.push(Title {
                    label,
                    items: children,
                    remote_id: id,
                    enabled,
                });
            }
        }

        if let Some(about) = about {
            fill_slots(&mut app_items, vec![about]);
        }

        let mut titles = Vec::with_capacity(middle.len() + 3);
        titles.push(Title {
            label: self.app_name.clone(),
            items: app_items,
            remote_id: app_remote,
            enabled: true,
        });
        titles.extend(middle);
        titles.push(Title {
            label: tr!("menu-window").to_string(),
            items: window_items,
            remote_id: window_remote,
            enabled: true,
        });
        titles.push(Title {
            label: tr!("menu-help").to_string(),
            items: help_items,
            remote_id: help_remote,
            enabled: true,
        });
        titles
    }
}

fn fill_slots(items: &mut [MenuItem], extras: Vec<MenuItem>) -> Vec<MenuItem> {
    let mut rest = Vec::new();
    for extra in extras {
        let MenuItem::Action {
            id,
            label,
            shortcut,
            enabled,
            check,
            ..
        } = &extra
        else {
            rest.push(extra);
            continue;
        };
        let slot = items.iter_mut().find(
            |item| matches!(item, MenuItem::Action { label: slot, .. } if same_slot(slot, label)),
        );
        match slot {
            Some(MenuItem::Action {
                id: slot_id,
                shortcut: slot_shortcut,
                enabled: slot_enabled,
                check: slot_check,
                ..
            }) => {
                *slot_id = id.clone();
                *slot_enabled = *enabled;
                *slot_check = *check;
                if shortcut.is_some() {
                    *slot_shortcut = shortcut.clone();
                }
            }
            _ => rest.push(extra),
        }
    }
    rest
}

fn same_slot(slot: &str, label: &str) -> bool {
    normalize_query(slot) == normalize_query(label) || (is_about(slot) && is_about(label))
}

fn is_about_slot(item: &MenuItem) -> bool {
    matches!(item, MenuItem::Action { id, .. } if id.starts_with("app-about:"))
}

fn is_about(label: &str) -> bool {
    let label = normalize_query(label);
    let label = label.trim_end_matches(['…', '.']).trim_end();
    ["acerca de", "about", "sobre"].iter().any(|prefix| {
        label.strip_prefix(prefix).is_some_and(|rest| {
            rest.is_empty() || (rest.starts_with(' ') && !matches!(rest.trim(), "kde" | "qt"))
        })
    })
}

fn take_about(items: &mut Vec<MenuItem>) -> Option<MenuItem> {
    let index = items
        .iter()
        .position(|item| matches!(item, MenuItem::Action { label, .. } if is_about(label)))?;
    let about = items.remove(index);
    normalize_separators(items);
    Some(about)
}

fn append(items: &mut Vec<MenuItem>, extras: Vec<MenuItem>) {
    if extras.is_empty() {
        return;
    }
    if !items.is_empty() && !matches!(items.last(), Some(MenuItem::Separator)) {
        items.push(MenuItem::Separator);
    }
    items.extend(extras);
}

fn insert_after_search(items: &mut Vec<MenuItem>, extras: Vec<MenuItem>) {
    if extras.is_empty() {
        return;
    }
    let position = items
        .iter()
        .position(|item| matches!(item, MenuItem::SearchField { .. }))
        .map_or(0, |index| index + 1);
    items.splice(position..position, extras);
}

pub fn searchable(titles: &[Title]) -> Vec<SearchableEntry> {
    let mut out = Vec::new();
    for (title_index, title) in titles.iter().enumerate() {
        collect(
            &title.items,
            &barrier_kit::menu::strip_mnemonic(&title.label),
            SearchLocation {
                title_index,
                indices: Vec::new(),
            },
            &mut out,
        );
    }
    out
}

fn collect(
    items: &[MenuItem],
    path: &str,
    location: SearchLocation,
    out: &mut Vec<SearchableEntry>,
) {
    for (index, item) in items.iter().enumerate() {
        match item {
            MenuItem::Action { .. } => {
                let mut location = location.clone();
                location.indices.push(index);
                out.push(SearchableEntry {
                    path: path.to_string(),
                    item: item.clone(),
                    location: Some(location),
                    source: SearchSource::Menu,
                });
            }
            MenuItem::Submenu { label, items, .. } => {
                let mut location = location.clone();
                location.indices.push(index);
                let label = barrier_kit::menu::strip_mnemonic(label);
                collect(items, &format!("{path} › {label}"), location, out);
            }
            _ => {}
        }
    }
}

fn submenu_id(id: &Option<String>) -> Option<i32> {
    id.as_deref()?.parse().ok()
}

pub fn merge_subtree(items: &mut [MenuItem], id: i32, children: &[MenuItem]) -> bool {
    for item in items.iter_mut() {
        let MenuItem::Submenu {
            id: item_id,
            items: current,
            ..
        } = item
        else {
            continue;
        };
        if submenu_id(item_id) == Some(id) {
            *current = children.to_vec();
            return true;
        }
        if merge_subtree(current, id, children) {
            return true;
        }
    }
    false
}

pub fn find_subtree(items: &[MenuItem], id: i32) -> Option<Vec<MenuItem>> {
    for item in items {
        let MenuItem::Submenu {
            id: item_id,
            items: current,
            ..
        } = item
        else {
            continue;
        };
        if submenu_id(item_id) == Some(id) {
            return Some(current.clone());
        }
        if let Some(found) = find_subtree(current, id) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menubar::dbusmenu::RawItem;
    use std::collections::HashMap;
    use std::rc::Rc;

    fn action(label: &str) -> MenuItem {
        MenuItem::action(format!("static:{label}"), label)
    }

    fn submenu(label: &str, id: &str, items: Vec<MenuItem>) -> MenuItem {
        MenuItem::Submenu {
            id: Some(id.to_string()),
            label: label.to_string(),
            enabled: true,
            items,
            icon: None,
        }
    }

    fn sources() -> MenuSources {
        MenuSources {
            app_name: "Kate".to_string(),
            app_items: Rc::new(|| vec![action("Ocultar Kate"), action("Salir de Kate")]),
            window_items: Rc::new(|| vec![action("Minimizar"), action("Cerrar ventana")]),
            help_items: Rc::new(|| {
                vec![
                    MenuItem::SearchField {
                        placeholder: "Buscar".to_string(),
                    },
                    action("Documentación de Malaysia"),
                ]
            }),
        }
    }

    fn kate_titles() -> Vec<MenuItem> {
        [
            "Archivo",
            "Editar",
            "Selección",
            "Ver",
            "Ir",
            "Proyectos",
            "Cliente LSP",
            "Sesiones",
            "Herramientas",
            "Preferencias",
            "Ayuda",
        ]
        .iter()
        .enumerate()
        .map(|(index, label)| submenu(label, &(index + 1).to_string(), Vec::new()))
        .collect()
    }

    fn labels(titles: &[Title]) -> Vec<&str> {
        titles.iter().map(|title| title.label.as_str()).collect()
    }

    #[test]
    fn kate_titles_keep_their_order_between_the_app_and_ventana() {
        let titles = sources().compose(&kate_titles());
        assert_eq!(
            labels(&titles),
            vec![
                "Kate",
                "Archivo",
                "Editar",
                "Selección",
                "Ver",
                "Ir",
                "Proyectos",
                "Cliente LSP",
                "Sesiones",
                "Herramientas",
                "Preferencias",
                tr!("menu-window"),
                tr!("menu-help"),
            ]
        );
        assert_eq!(titles[1].remote_id, Some(1));
    }

    #[test]
    fn remote_ayuda_entries_sit_under_the_search_field() {
        let remote = vec![submenu(
            "Ayuda",
            "11",
            vec![action("Manual de Kate"), action("Acerca de Kate")],
        )];
        let titles = sources().compose(&remote);
        let help = titles.last().unwrap();
        assert_eq!(help.remote_id, Some(11));
        assert!(matches!(help.items[0], MenuItem::SearchField { .. }));
        assert_eq!(help.items[1].label(), Some("Manual de Kate"));
        assert_eq!(help.items[2].label(), Some("Acerca de Kate"));
        let indexed = searchable(&titles);
        assert!(
            indexed
                .iter()
                .any(|entry| entry.item.label() == Some("Manual de Kate")),
            "remote help entries must be searchable"
        );
    }

    #[test]
    fn remote_window_and_app_entries_are_appended_after_a_separator() {
        let remote = vec![
            submenu("Ventana", "20", vec![action("Pantalla completa")]),
            submenu("Kate", "21", vec![action("Preferencias de Kate")]),
        ];
        let titles = sources().compose(&remote);
        assert_eq!(
            labels(&titles),
            vec!["Kate", tr!("menu-window"), tr!("menu-help")]
        );
        let app = &titles[0];
        assert_eq!(
            app.items.last().unwrap().label(),
            Some("Preferencias de Kate")
        );
        assert!(matches!(
            app.items[app.items.len() - 2],
            MenuItem::Separator
        ));
        let window = &titles[1];
        assert_eq!(
            window.items.last().unwrap().label(),
            Some("Pantalla completa")
        );
        assert!(matches!(
            window.items[window.items.len() - 2],
            MenuItem::Separator
        ));
    }

    #[test]
    fn app_entries_take_the_places_of_the_greyed_out_fallbacks() {
        let disabled = |label: &str| action(label).with_enabled(false);
        let sources = MenuSources {
            app_name: "Calculadora".to_string(),
            app_items: Rc::new(move || {
                vec![
                    MenuItem::action("app-about:calc", "Acerca de Calculadora").with_enabled(false),
                    disabled("Ajustes…"),
                    action("Salir de Calculadora"),
                ]
            }),
            window_items: Rc::new(Vec::new),
            help_items: Rc::new(Vec::new),
        };
        let remote = vec![submenu(
            "Calculadora",
            "7",
            vec![
                MenuItem::action("dbusmenu:3", "Ajustes…").with_shortcut("Ctrl+comma"),
                MenuItem::action("dbusmenu:4", "Acerca de Calculadora"),
            ],
        )];
        let titles = sources.compose(&remote);
        let app = &titles[0].items;
        assert_eq!(app.len(), 3);
        assert!(matches!(
            &app[0],
            MenuItem::Action { id, enabled: true, .. } if id == "dbusmenu:4"
        ));
        assert!(matches!(
            &app[1],
            MenuItem::Action { id, enabled: true, shortcut: Some(shortcut), .. }
                if id == "dbusmenu:3" && shortcut == "Ctrl+comma"
        ));
        assert_eq!(app[2].label(), Some("Salir de Calculadora"));
    }

    #[test]
    fn the_apps_about_entry_moves_from_its_help_menu_into_the_app_menu() {
        let sources = MenuSources {
            app_name: "Firefox".to_string(),
            app_items: Rc::new(|| vec![MenuItem::action("app-about:firefox", "Acerca de Firefox")]),
            window_items: Rc::new(Vec::new),
            help_items: Rc::new(Vec::new),
        };
        let about = MenuItem::action("dbusmenu:9", "About Firefox");
        let remote = vec![submenu(
            "Help",
            "5",
            vec![
                action("Firefox Help"),
                MenuItem::Separator,
                action("About Qt"),
                about,
            ],
        )];
        let titles = sources.compose(&remote);
        assert!(matches!(
            &titles[0].items[0],
            MenuItem::Action { id, label, .. } if id == "dbusmenu:9" && label == "Acerca de Firefox"
        ));
        let help: Vec<_> = titles
            .last()
            .unwrap()
            .items
            .iter()
            .map(MenuItem::label)
            .collect();
        assert_eq!(help, vec![Some("Firefox Help"), None, Some("About Qt")]);
    }

    #[test]
    fn titles_are_normalized_and_never_duplicated() {
        let remote = vec![
            submenu("AYUDA", "1", Vec::new()),
            submenu("ayúda", "2", Vec::new()),
            submenu("Archivo", "3", Vec::new()),
            submenu("archivo", "4", Vec::new()),
        ];
        let titles = sources().compose(&remote);
        assert_eq!(
            labels(&titles),
            vec!["Kate", "Archivo", tr!("menu-window"), tr!("menu-help")]
        );
    }

    #[test]
    fn an_empty_remote_title_shows_a_loading_row() {
        let titles = sources().compose(&kate_titles());
        let archivo = &titles[1];
        assert!(matches!(
            &archivo.visible_items().as_slice(),
            [MenuItem::Action { id, enabled: false, .. }] if id.is_empty()
        ));
    }

    #[test]
    fn merge_subtree_replaces_children_and_keeps_the_label() {
        let mut remote = kate_titles();
        assert!(merge_subtree(&mut remote, 1, &[action("Nuevo")]));
        let MenuItem::Submenu { label, items, .. } = &remote[0] else {
            panic!("Archivo is a submenu");
        };
        assert_eq!(label, "Archivo");
        assert_eq!(items[0].label(), Some("Nuevo"));
        assert_eq!(find_subtree(&remote, 1).unwrap()[0].label(), Some("Nuevo"));
    }

    fn text(value: &str) -> zbus::zvariant::OwnedValue {
        zbus::zvariant::OwnedValue::try_from(zbus::zvariant::Value::from(value)).unwrap()
    }

    fn kate_item(id: i32, label: &str, enabled: bool, children: Vec<RawItem>) -> RawItem {
        let mut properties: HashMap<String, zbus::zvariant::OwnedValue> = [
            ("label".to_string(), text(&format!("_{label}"))),
            (
                "enabled".to_string(),
                zbus::zvariant::OwnedValue::from(enabled),
            ),
        ]
        .into_iter()
        .collect();
        if !children.is_empty() {
            properties.insert("children-display".to_string(), text("submenu"));
        }
        RawItem {
            id,
            properties,
            children,
        }
    }

    fn kate_menu() -> Vec<RawItem> {
        vec![
            kate_item(
                1,
                "Archivo",
                true,
                vec![
                    kate_item(10, "Nuevo", true, Vec::new()),
                    kate_item(11, "Nueva ventana", true, Vec::new()),
                    kate_item(
                        12,
                        "Abrir reciente",
                        true,
                        vec![
                            kate_item(13, "documento.txt", true, Vec::new()),
                            kate_item(14, "otro.txt", true, Vec::new()),
                        ],
                    ),
                    kate_item(15, "Guardar", false, Vec::new()),
                ],
            ),
            kate_item(
                2,
                "Ayuda",
                true,
                vec![kate_item(20, "Acerca de Kate", true, Vec::new())],
            ),
        ]
    }

    #[test]
    fn help_search_indexes_the_full_kate_tree() {
        let mapped = crate::menubar::dbusmenu_map::menu_items(&kate_menu());
        let titles = sources().compose(&mapped);
        let entries = searchable(&titles);

        let hits: Vec<String> = entries
            .iter()
            .filter(|entry| {
                entry
                    .item
                    .label()
                    .is_some_and(|label| label.to_lowercase().contains("nuev"))
            })
            .map(|entry| {
                let label = barrier_kit::menu::strip_mnemonic(entry.item.label().unwrap());
                format!("{} › {label}", entry.path)
            })
            .collect();
        assert!(
            hits.contains(&"Archivo › Nuevo".to_string()),
            "hits: {hits:?}"
        );
        assert!(hits.contains(&"Archivo › Nueva ventana".to_string()));

        let nested = entries
            .iter()
            .find(|entry| {
                entry.item.label().map(barrier_kit::menu::strip_mnemonic)
                    == Some("documento.txt".to_string())
            })
            .expect("a nested leaf is indexed");
        assert_eq!(nested.path, "Archivo › Abrir reciente");
        assert_eq!(nested.source, SearchSource::Menu);
        let location = nested.location.as_ref().unwrap();
        assert_eq!(location.title_index, 1);
        assert_eq!(location.indices.len(), 2);

        let disabled = entries
            .iter()
            .find(|entry| {
                entry.item.label().map(barrier_kit::menu::strip_mnemonic)
                    == Some("Guardar".to_string())
            })
            .expect("a disabled action is still indexed");
        assert!(matches!(
            &disabled.item,
            MenuItem::Action { enabled: false, .. }
        ));
    }
}
