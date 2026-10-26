//! Merges a real dbusmenu tree with the 14c fallback menus (ADR 0040 §6).
//!
//! The composition is pure: the bar rebuilds it from the current snapshot on
//! every render, and open popups use the same function to update in place.

use std::collections::HashSet;
use std::rc::Rc;

use barrier_kit::menu::{CheckState, MenuItem, SearchLocation, SearchSource, SearchableEntry};

/// The 14c menus that always frame the app's own titles.
#[derive(Clone)]
pub struct MenuSources {
    pub app_name: String,
    pub app_items: Rc<dyn Fn() -> Vec<MenuItem>>,
    pub window_items: Rc<dyn Fn() -> Vec<MenuItem>>,
    pub help_items: Rc<dyn Fn() -> Vec<MenuItem>>,
}

/// One first-level title on the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Title {
    pub label: String,
    pub items: Vec<MenuItem>,
    /// dbusmenu id to open lazily; `None` for static titles and root actions.
    pub remote_id: Option<i32>,
    pub bold: bool,
    pub enabled: bool,
}

impl Title {
    /// Items for the title's popup: the loading row while a remote submenu has
    /// not answered yet.
    pub fn visible_items(&self) -> Vec<MenuItem> {
        if self.items.is_empty() && self.remote_id.is_some() {
            vec![loading()]
        } else {
            self.items.clone()
        }
    }
}

pub fn loading() -> MenuItem {
    MenuItem::Action {
        id: String::new(),
        label: "Cargando…".to_string(),
        shortcut: None,
        enabled: false,
        check: CheckState::None,
        icon: None,
    }
}

pub fn unavailable() -> MenuItem {
    MenuItem::Action {
        id: String::new(),
        label: "No disponible".to_string(),
        shortcut: None,
        enabled: false,
        check: CheckState::None,
        icon: None,
    }
}

/// Lowercase and strip accents for title matching.
pub fn normalize(label: &str) -> String {
    label
        .trim()
        .to_lowercase()
        .replace('á', "a")
        .replace('é', "e")
        .replace('í', "i")
        .replace('ó', "o")
        .replace(['ú', 'ü'], "u")
        .replace('ñ', "n")
}

impl MenuSources {
    /// Builds the bar titles, folding Ventana/Window, Ayuda/Help and the app's
    /// own title into ours.
    pub fn compose(&self, remote: &[MenuItem]) -> Vec<Title> {
        let mut app_items = (self.app_items)();
        let mut window_items = (self.window_items)();
        let mut help_items = (self.help_items)();
        let mut app_remote = None;
        let mut window_remote = None;
        let mut help_remote = None;
        let app_name = normalize(&self.app_name);
        let mut seen = HashSet::from(["sistema".to_string()]);
        let mut middle = Vec::new();

        for item in remote {
            let (label, id, children, enabled) = match item {
                MenuItem::Submenu {
                    id,
                    label,
                    items,
                    enabled,
                    ..
                } => (
                    label.clone(),
                    id.as_ref().and_then(|id| id.parse::<i32>().ok()),
                    items.clone(),
                    *enabled,
                ),
                MenuItem::Action { label, .. } => (label.clone(), None, vec![item.clone()], true),
                _ => continue,
            };
            let normalized = normalize(&label);
            if normalized == app_name {
                if seen.insert(normalized) {
                    append(&mut app_items, children);
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
                    bold: false,
                    enabled,
                });
            }
        }

        let mut titles = Vec::with_capacity(middle.len() + 3);
        titles.push(Title {
            label: self.app_name.clone(),
            items: app_items,
            remote_id: app_remote,
            bold: true,
            enabled: true,
        });
        titles.extend(middle);
        titles.push(Title {
            label: "Ventana".to_string(),
            items: window_items,
            remote_id: window_remote,
            bold: false,
            enabled: true,
        });
        titles.push(Title {
            label: "Ayuda".to_string(),
            items: help_items,
            remote_id: help_remote,
            bold: false,
            enabled: true,
        });
        titles
    }
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

/// All actions reachable from the composed titles, used by the Help search.
pub fn searchable(titles: &[Title]) -> Vec<SearchableEntry> {
    let mut out = Vec::new();
    for (title_index, title) in titles.iter().enumerate() {
        collect(
            &title.items,
            &title.label,
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
                collect(items, &format!("{path} › {label}"), location, out);
            }
            _ => {}
        }
    }
}

/// Replaces the loaded children of the remote submenu `id`. Returns whether a
/// matching submenu existed.
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
        if item_id.as_deref().and_then(|id| id.parse::<i32>().ok()) == Some(id) {
            *current = children.to_vec();
            return true;
        }
        if merge_subtree(current, id, children) {
            return true;
        }
    }
    false
}

/// Finds the loaded children of a submenu by dbusmenu id.
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
        if item_id.as_deref().and_then(|id| id.parse::<i32>().ok()) == Some(id) {
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

    fn action(label: &str) -> MenuItem {
        MenuItem::Action {
            id: format!("static:{label}"),
            label: label.to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::None,
            icon: None,
        }
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
                    action("Documentación de MalaysiaDreams"),
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
                "Ventana",
                "Ayuda",
            ]
        );
        assert!(titles[0].bold);
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
        assert_eq!(labels(&titles), vec!["Kate", "Ventana", "Ayuda"]);
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
    fn titles_are_normalized_and_never_duplicated() {
        let remote = vec![
            submenu("AYUDA", "1", Vec::new()),
            submenu("ayúda", "2", Vec::new()),
            submenu("Archivo", "3", Vec::new()),
            submenu("archivo", "4", Vec::new()),
        ];
        let titles = sources().compose(&remote);
        assert_eq!(labels(&titles), vec!["Kate", "Archivo", "Ventana", "Ayuda"]);
    }

    #[test]
    fn an_empty_remote_title_shows_a_loading_row() {
        let titles = sources().compose(&kate_titles());
        let archivo = &titles[1];
        assert!(matches!(
            &archivo.visible_items().as_slice(),
            [MenuItem::Action { label, enabled: false, .. }] if label == "Cargando…"
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
            .map(|entry| format!("{} › {}", entry.path, entry.item.label().unwrap()))
            .collect();
        assert!(
            hits.contains(&"Archivo › Nuevo".to_string()),
            "hits: {hits:?}"
        );
        assert!(hits.contains(&"Archivo › Nueva ventana".to_string()));

        let nested = entries
            .iter()
            .find(|entry| entry.item.label() == Some("documento.txt"))
            .expect("a nested leaf is indexed");
        assert_eq!(nested.path, "Archivo › Abrir reciente");
        assert_eq!(nested.source, SearchSource::Menu);
        let location = nested.location.as_ref().unwrap();
        assert_eq!(location.title_index, 1);
        assert_eq!(location.indices.len(), 2);

        let disabled = entries
            .iter()
            .find(|entry| entry.item.label() == Some("Guardar"))
            .expect("a disabled action is still indexed");
        assert!(matches!(
            &disabled.item,
            MenuItem::Action { enabled: false, .. }
        ));
    }
}
