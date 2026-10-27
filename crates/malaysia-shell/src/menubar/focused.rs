//! Menus for the focused application (14c).
//!
//! Apps without DBusMenu get the three menus macOS shows: the app menu,
//! Ventana and Ayuda. The constructors are pure so the tests can drive them
//! with fake [`ToplevelInfo`]s; [`on_action`] reads the live list again.

use barrier_kit::menu::{
    CheckState, MenuIcon, MenuItem, SearchLocation, SearchSource, SearchableEntry,
};
use barrier_kit::platform::{InfinityLayout, InfinityPairLayout, ToplevelInfo, Toplevels};

use crate::menubar::{help_topics, icons, xdg_open};

const ISSUES_URL: &str = "https://github.com/HBlanqueto/malaysia/issues";

/// The app menu (`{App}`): about, settings, services, hide/show and quit.
pub fn app_menu_items(app_name: &str, app_id: &str, toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let app = app_indices(toplevels, app_id);
    let other_unminimized = toplevels
        .iter()
        .any(|window| window.app_id != app_id && !window.minimized);
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    vec![
        action(
            "app-about",
            format!("Acerca de {app_name}"),
            None,
            false,
            Some(MenuIcon::Svg(icons::INFO)),
        ),
        MenuItem::Separator,
        action(
            "app-settings",
            "Configuración…",
            Some("Super+,"),
            false,
            Some(MenuIcon::Svg(icons::GEAR)),
        ),
        MenuItem::Separator,
        MenuItem::Submenu {
            id: None,
            label: "Servicios".to_string(),
            enabled: true,
            icon: None,
            items: vec![
                action(
                    "app-services-none",
                    "Ningún servicio disponible",
                    None,
                    false,
                    None,
                ),
                MenuItem::Separator,
                action(
                    "app-services-settings",
                    "Ajustes de servicios…",
                    None,
                    false,
                    None,
                ),
            ],
        },
        MenuItem::Separator,
        action(
            format!("app-hide:{app_id}"),
            format!("Ocultar {app_name}"),
            Some("Super+H"),
            !app.is_empty(),
            None,
        ),
        action(
            format!("app-hide-others:{app_id}"),
            "Ocultar otros",
            Some("Alt+Super+H"),
            other_unminimized,
            None,
        ),
        action("app-show-all", "Mostrar todo", None, any_minimized, None),
        MenuItem::Separator,
        action(
            format!("app-quit:{app_id}"),
            format!("Salir de {app_name}"),
            Some("Super+Q"),
            !app.is_empty(),
            None,
        ),
    ]
}

/// The Ventana menu for the focused app: window controls, layout submenus and
/// the app's own windows.
pub fn window_menu_items(
    app_name: &str,
    app_id: &str,
    toplevels: &[ToplevelInfo],
) -> Vec<MenuItem> {
    let app = app_indices(toplevels, app_id);
    let active = active_index(toplevels, app_id);
    let active_window = active.map(|index| &toplevels[index]);
    let has_active = active_window.is_some();
    let mut items = vec![
        action(
            active_id("app-window-minimize", active),
            "Minimizar",
            Some("Super+M"),
            active.is_some(),
            None,
        ),
        action(
            active_id("app-window-maximize", active),
            "Maximizar",
            None,
            active.is_some(),
            None,
        ),
        action(
            active_id("app-window-fill", active),
            "Rellenar",
            Some("Ctrl+Super+Return"),
            has_active,
            None,
        ),
        action(
            active_id("app-window-center", active),
            "Centrar",
            Some("Ctrl+Super+C"),
            has_active,
            None,
        ),
        MenuItem::Separator,
        MenuItem::Submenu {
            id: None,
            label: "Trasladar y redimensionar".to_string(),
            enabled: true,
            icon: Some(MenuIcon::Svg(icons::GRID)),
            items: move_resize_items(active, active_window),
        },
        MenuItem::Submenu {
            id: None,
            label: "Ventana a pantalla completa".to_string(),
            enabled: true,
            icon: Some(MenuIcon::Svg(icons::SPLIT)),
            items: vec![
                action(
                    "app-window-screen-left",
                    "Izquierda de la pantalla",
                    None,
                    false,
                    None,
                ),
                action(
                    "app-window-screen-right",
                    "Derecha de la pantalla",
                    None,
                    false,
                    None,
                ),
                MenuItem::Separator,
                action(
                    active_id("app-window-fullscreen", active),
                    "Activar pantalla completa",
                    Some("Ctrl+Super+F"),
                    has_active,
                    None,
                ),
            ],
        },
        MenuItem::Separator,
        action(
            active_id("app-window-close", active),
            "Cerrar ventana",
            Some("Super+W"),
            active.is_some(),
            None,
        ),
        MenuItem::Separator,
        action(
            format!("app-window-front:{app_id}"),
            "Traer todo al frente",
            None,
            !app.is_empty(),
            None,
        ),
        action(
            "app-window-remove",
            "Quitar ventana del conjunto",
            None,
            false,
            None,
        ),
        MenuItem::Separator,
        action(
            "app-tab-previous",
            "Mostrar pestaña anterior",
            Some("Ctrl+Shift+Tab"),
            false,
            None,
        ),
        action(
            "app-tab-next",
            "Mostrar pestaña siguiente",
            Some("Ctrl+Tab"),
            false,
            None,
        ),
        action(
            "app-tab-new-window",
            "Mover pestaña a una ventana nueva",
            None,
            false,
            None,
        ),
        action(
            "app-tab-merge",
            "Unir todas las ventanas",
            None,
            false,
            None,
        ),
        MenuItem::Separator,
    ];
    for index in &app {
        let window = &toplevels[*index];
        let label = if window.title.is_empty() || window.title == app_name {
            app_name.to_string()
        } else {
            format!("{app_name} — {}", window.title)
        };
        items.push(MenuItem::Action {
            id: format!("app-window-activate:{index}"),
            label,
            shortcut: None,
            enabled: true,
            check: CheckState::Checkbox(Some(*index) == active),
            icon: None,
        });
    }
    items
}

/// The Ayuda menu for the focused app.
pub fn help_menu_items(app_name: &str) -> Vec<MenuItem> {
    vec![
        MenuItem::SearchField {
            placeholder: "Buscar".to_string(),
        },
        action(
            "app-help",
            format!("Ayuda de {app_name}"),
            None,
            false,
            None,
        ),
        MenuItem::Separator,
        action(
            "app-help-shortcuts",
            "Atajos de teclado de MalaysiaDreams",
            None,
            false,
            None,
        ),
        action(
            "app-help-docs",
            "Documentación de MalaysiaDreams",
            None,
            false,
            None,
        ),
        action(
            "app-help-report",
            "Informar de un problema…",
            None,
            true,
            None,
        ),
    ]
}

/// All menu entries that the Help search field can reach, paired with their
/// menu path and menubar location. Keeps labels in one place so tests and UI
/// never drift apart.
pub fn searchable_menu_items(
    app_name: &str,
    app_id: &str,
    toplevels: &[ToplevelInfo],
) -> Vec<SearchableEntry> {
    let mut out = Vec::new();
    collect_searchable(
        &app_menu_items(app_name, app_id, toplevels),
        app_name,
        SearchLocation {
            title_index: 0,
            indices: Vec::new(),
        },
        &mut out,
    );
    collect_searchable(
        &window_menu_items(app_name, app_id, toplevels),
        "Ventana",
        SearchLocation {
            title_index: 1,
            indices: Vec::new(),
        },
        &mut out,
    );
    out
}

/// Help topics surfaced by the Help search field.
pub fn help_search_topics() -> Vec<SearchableEntry> {
    help_topics::topics()
        .iter()
        .map(|topic| SearchableEntry {
            path: "Temas de ayuda".to_string(),
            item: MenuItem::Action {
                id: format!("app-help-topic:{}", topic.url),
                label: topic.title.to_string(),
                shortcut: None,
                enabled: true,
                check: CheckState::None,
                icon: None,
            },
            location: None,
            source: SearchSource::Help,
        })
        .chain(std::iter::once(SearchableEntry {
            path: "Temas de ayuda".to_string(),
            item: MenuItem::Action {
                id: "app-help-topics-index".to_string(),
                label: "Mostrar todos los temas de ayuda".to_string(),
                shortcut: None,
                enabled: true,
                check: CheckState::None,
                icon: None,
            },
            location: None,
            source: SearchSource::Help,
        }))
        .collect()
}

fn collect_searchable(
    items: &[MenuItem],
    path: &str,
    location: SearchLocation,
    out: &mut Vec<SearchableEntry>,
) {
    for (index, item) in items.iter().enumerate() {
        match item {
            MenuItem::Action { .. } => {
                let mut loc = location.clone();
                loc.indices.push(index);
                out.push(SearchableEntry {
                    path: path.to_string(),
                    item: item.clone(),
                    location: Some(loc),
                    source: SearchSource::Menu,
                });
            }
            MenuItem::Submenu {
                id: None,
                label,
                items: sub,
                ..
            } => {
                let sub_path = format!("{path} › {label}");
                let mut loc = location.clone();
                loc.indices.push(index);
                collect_searchable(sub, &sub_path, loc, out);
            }
            _ => {}
        }
    }
}

/// Handles an action from the focused-app menus.
pub fn on_action(id: String) {
    let toplevels = Toplevels::get();
    let list = toplevels.list.read().clone();
    let (verb, argument) = id.split_once(':').unwrap_or((id.as_str(), ""));
    match verb {
        "app-hide" => {
            for index in app_indices(&list, argument) {
                toplevels.minimize(index);
            }
        }
        "app-hide-others" => {
            for (index, window) in list.iter().enumerate() {
                if window.app_id != argument && !window.minimized {
                    toplevels.minimize(index);
                }
            }
        }
        "app-show-all" => {
            for (index, window) in list.iter().enumerate() {
                if window.minimized {
                    toplevels.unset_minimized(index);
                }
            }
        }
        "app-quit" => {
            for index in app_indices(&list, argument).into_iter().rev() {
                toplevels.close(index);
            }
        }
        "app-window-minimize" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.minimize(index);
            }
        }
        "app-window-maximize" => {
            if let Ok(index) = argument.parse::<usize>() {
                let maximized = list.get(index).is_some_and(|window| window.maximized);
                toplevels.set_maximized(index, !maximized);
            }
        }
        "app-window-close" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.close(index);
            }
        }
        "app-window-front" => {
            let mut windows = app_indices(&list, argument);
            if let Some(position) = windows
                .iter()
                .position(|index| list.get(*index).is_some_and(|window| window.active))
            {
                let active = windows.remove(position);
                windows.push(active);
            }
            for index in windows {
                toplevels.activate(index);
            }
        }
        "app-window-activate" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.activate(index);
            }
        }
        "app-help-report" => xdg_open(ISSUES_URL),
        "app-help-topics-index" => xdg_open(help_topics::DOCS_INDEX),
        "app-help-topic" => xdg_open(argument),
        "app-window-fill" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::Fill);
            }
        }
        "app-window-center" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::Center);
            }
        }
        "app-move-left" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::Left);
            }
        }
        "app-move-right" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::Right);
            }
        }
        "app-move-up" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::Top);
            }
        }
        "app-move-down" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::Bottom);
            }
        }
        "app-quarter-top-left" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::TopLeft);
            }
        }
        "app-quarter-top-right" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::TopRight);
            }
        }
        "app-quarter-bottom-left" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::BottomLeft);
            }
        }
        "app-quarter-bottom-right" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.arrange(index, InfinityLayout::BottomRight);
            }
        }
        "app-layout-side-by-side" => {
            toplevels.arrange_pair(InfinityPairLayout::SideBySide);
        }
        "app-layout-reversed-side-by-side" => {
            toplevels.arrange_pair(InfinityPairLayout::SideBySideReversed);
        }
        "app-layout-stacked" => {
            toplevels.arrange_pair(InfinityPairLayout::Stacked);
        }
        "app-layout-reversed-stacked" => {
            toplevels.arrange_pair(InfinityPairLayout::StackedReversed);
        }
        "app-layout-quarters" => {
            toplevels.arrange_quarters();
        }
        "app-move-restore" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.restore(index);
            }
        }
        "app-window-fullscreen" => {
            if let Ok(index) = argument.parse::<usize>() {
                toplevels.set_fullscreen(index, true);
            }
        }
        _ => {}
    }
}

fn move_resize_items(active: Option<usize>, active_window: Option<&ToplevelInfo>) -> Vec<MenuItem> {
    let has_active = active_window.is_some();
    let can_restore = active_window.is_some_and(|window| window.restorable);
    vec![
        header("Mitades"),
        action(
            active_id("app-move-left", active),
            "Izquierda",
            Some("Ctrl+Super+Left"),
            has_active,
            Some(MenuIcon::Svg(icons::HALF_LEFT)),
        ),
        action(
            active_id("app-move-right", active),
            "Derecha",
            Some("Ctrl+Super+Right"),
            has_active,
            Some(MenuIcon::Svg(icons::HALF_RIGHT)),
        ),
        action(
            active_id("app-move-up", active),
            "Superior",
            Some("Ctrl+Super+Up"),
            has_active,
            Some(MenuIcon::Svg(icons::HALF_TOP)),
        ),
        action(
            active_id("app-move-down", active),
            "Inferior",
            Some("Ctrl+Super+Down"),
            has_active,
            Some(MenuIcon::Svg(icons::HALF_BOTTOM)),
        ),
        MenuItem::Separator,
        header("Cuartos"),
        action(
            active_id("app-quarter-top-left", active),
            "Superior izquierdo",
            None,
            has_active,
            Some(MenuIcon::Svg(icons::QUARTER_TOP_LEFT)),
        ),
        action(
            active_id("app-quarter-top-right", active),
            "Superior derecho",
            None,
            has_active,
            Some(MenuIcon::Svg(icons::QUARTER_TOP_RIGHT)),
        ),
        action(
            active_id("app-quarter-bottom-left", active),
            "Inferior izquierdo",
            None,
            has_active,
            Some(MenuIcon::Svg(icons::QUARTER_BOTTOM_LEFT)),
        ),
        action(
            active_id("app-quarter-bottom-right", active),
            "Inferior derecho",
            None,
            has_active,
            Some(MenuIcon::Svg(icons::QUARTER_BOTTOM_RIGHT)),
        ),
        MenuItem::Separator,
        header("Disposición"),
        action(
            active_id("app-layout-side-by-side", active),
            "Izquierda y derecha",
            Some("Ctrl+Shift+Super+Left"),
            has_active,
            Some(MenuIcon::Svg(icons::HALVES_LEFT_RIGHT)),
        ),
        action(
            active_id("app-layout-reversed-side-by-side", active),
            "Derecha e izquierda",
            Some("Ctrl+Shift+Super+Right"),
            has_active,
            Some(MenuIcon::Svg(icons::HALVES_RIGHT_LEFT)),
        ),
        action(
            active_id("app-layout-stacked", active),
            "Superior e inferior",
            Some("Ctrl+Shift+Super+Up"),
            has_active,
            Some(MenuIcon::Svg(icons::HALVES_TOP_BOTTOM)),
        ),
        action(
            active_id("app-layout-reversed-stacked", active),
            "Inferior y superior",
            Some("Ctrl+Shift+Super+Down"),
            has_active,
            Some(MenuIcon::Svg(icons::HALVES_BOTTOM_TOP)),
        ),
        action(
            active_id("app-layout-quarters", active),
            "En cuartos",
            None,
            has_active,
            Some(MenuIcon::Svg(icons::GRID)),
        ),
        MenuItem::Separator,
        action(
            active_id("app-move-restore", active),
            "Regresar al tamaño anterior",
            Some("Ctrl+Super+R"),
            can_restore,
            None,
        ),
    ]
}

fn active_index(toplevels: &[ToplevelInfo], app_id: &str) -> Option<usize> {
    let app = app_indices(toplevels, app_id);
    app.iter()
        .copied()
        .find(|index| toplevels[*index].active)
        .or_else(|| app.last().copied())
}

fn app_indices(toplevels: &[ToplevelInfo], app_id: &str) -> Vec<usize> {
    toplevels
        .iter()
        .enumerate()
        .filter(|(_, window)| window.app_id == app_id)
        .map(|(index, _)| index)
        .collect()
}

fn active_id(prefix: &str, active: Option<usize>) -> String {
    active.map_or_else(String::new, |index| format!("{prefix}:{index}"))
}

fn header(label: &str) -> MenuItem {
    MenuItem::Header {
        label: label.to_string(),
        count: None,
    }
}

fn action(
    id: impl Into<String>,
    label: impl Into<String>,
    shortcut: Option<&str>,
    enabled: bool,
    icon: Option<MenuIcon>,
) -> MenuItem {
    MenuItem::Action {
        id: id.into(),
        label: label.into(),
        shortcut: shortcut.map(ToOwned::to_owned),
        enabled,
        check: CheckState::None,
        icon,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn window(
        app_id: &str,
        title: &str,
        active: bool,
        minimized: bool,
        restorable: bool,
    ) -> ToplevelInfo {
        ToplevelInfo {
            title: title.to_string(),
            app_id: app_id.to_string(),
            active,
            maximized: false,
            minimized,
            restorable,
            app_menu: None,
            gtk_menus: None,
        }
    }

    fn label(item: &MenuItem) -> Option<&str> {
        match item {
            MenuItem::Action { label, .. }
            | MenuItem::Submenu { label, .. }
            | MenuItem::Header { label, .. } => Some(label),
            _ => None,
        }
    }

    fn is_enabled(item: &MenuItem) -> bool {
        match item {
            MenuItem::Action { enabled, .. } | MenuItem::Submenu { enabled, .. } => *enabled,
            _ => false,
        }
    }

    fn labels(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| label(item).map(ToOwned::to_owned))
            .collect()
    }

    fn find<'a>(items: &'a [MenuItem], wanted: &str) -> &'a MenuItem {
        items
            .iter()
            .find(|item| label(item) == Some(wanted))
            .unwrap_or_else(|| panic!("missing «{wanted}» in {:?}", labels(items)))
    }

    fn enabled(items: &[MenuItem], wanted: &str) -> bool {
        is_enabled(find(items, wanted))
    }

    fn toplevels() -> Vec<ToplevelInfo> {
        vec![
            window("foot", "MalaysiaDreams", true, false, false),
            window("foot", "Segunda ventana", false, true, false),
            window(
                "org.mozilla.firefox",
                "Mozilla Firefox",
                false,
                false,
                false,
            ),
        ]
    }

    #[test]
    fn active_index_prefers_the_flag_then_the_last_window() {
        let mut list = vec![
            window("foot", "MalaysiaDreams", false, false, false),
            window("foot", "Segunda ventana", false, false, false),
        ];
        assert_eq!(active_index(&list, "foot"), Some(1));
        list[0].active = true;
        assert_eq!(active_index(&list, "foot"), Some(0));
    }

    #[test]
    fn app_menu_labels_and_enabled_states() {
        let items = app_menu_items("Foot", "foot", &toplevels());
        assert_eq!(
            labels(&items),
            vec![
                "Acerca de Foot",
                "Configuración…",
                "Servicios",
                "Ocultar Foot",
                "Ocultar otros",
                "Mostrar todo",
                "Salir de Foot",
            ]
        );
        assert!(!enabled(&items, "Acerca de Foot"));
        assert!(!enabled(&items, "Configuración…"));
        assert!(enabled(&items, "Servicios"));
        assert!(enabled(&items, "Ocultar Foot"));
        assert!(enabled(&items, "Ocultar otros"));
        assert!(enabled(&items, "Mostrar todo"));
        assert!(enabled(&items, "Salir de Foot"));
    }

    #[test]
    fn show_all_is_off_without_minimized_windows() {
        let items = app_menu_items(
            "Foot",
            "foot",
            &[
                window("foot", "MalaysiaDreams", true, false, false),
                window("foot", "Otra", false, false, false),
            ],
        );
        assert!(!enabled(&items, "Mostrar todo"));
        assert!(!enabled(&items, "Ocultar otros"));
    }

    #[test]
    fn window_menu_lists_only_the_app_windows_with_the_active_checked() {
        let items = window_menu_items("Foot", "foot", &toplevels());
        let listed: Vec<&MenuItem> = items
            .iter()
            .filter(|item| matches!(item, MenuItem::Action { id, .. } if id.starts_with("app-window-activate:")))
            .collect();
        assert_eq!(listed.len(), 2);
        let names: Vec<&str> = listed.iter().filter_map(|item| label(item)).collect();
        assert_eq!(
            names,
            vec!["Foot — MalaysiaDreams", "Foot — Segunda ventana"]
        );
        assert!(matches!(
            listed[0],
            MenuItem::Action {
                check: CheckState::Checkbox(true),
                ..
            }
        ));
        assert!(matches!(
            listed[1],
            MenuItem::Action {
                check: CheckState::Checkbox(false),
                ..
            }
        ));
    }

    #[test]
    fn window_menu_controls_are_enabled_and_the_rest_is_gray() {
        let items = window_menu_items("Foot", "foot", &toplevels());
        assert!(enabled(&items, "Minimizar"));
        assert!(enabled(&items, "Maximizar"));
        assert!(enabled(&items, "Rellenar"));
        assert!(enabled(&items, "Centrar"));
        assert!(enabled(&items, "Trasladar y redimensionar"));
        assert!(enabled(&items, "Ventana a pantalla completa"));
        assert!(enabled(&items, "Cerrar ventana"));
        assert!(enabled(&items, "Traer todo al frente"));
        assert!(!enabled(&items, "Quitar ventana del conjunto"));
        assert!(!enabled(&items, "Mostrar pestaña anterior"));
        assert!(!enabled(&items, "Mostrar pestaña siguiente"));
        assert!(!enabled(&items, "Mover pestaña a una ventana nueva"));
        assert!(!enabled(&items, "Unir todas las ventanas"));
    }

    #[test]
    fn move_and_resize_submenu_has_headers_and_enabled_geometry_items() {
        let items = window_menu_items("Foot", "foot", &toplevels());
        let Some(MenuItem::Submenu { items: layout, .. }) = items
            .iter()
            .find(|item| label(item) == Some("Trasladar y redimensionar"))
        else {
            panic!("missing the layout submenu");
        };
        assert_eq!(
            labels(layout),
            vec![
                "Mitades",
                "Izquierda",
                "Derecha",
                "Superior",
                "Inferior",
                "Cuartos",
                "Superior izquierdo",
                "Superior derecho",
                "Inferior izquierdo",
                "Inferior derecho",
                "Disposición",
                "Izquierda y derecha",
                "Derecha e izquierda",
                "Superior e inferior",
                "Inferior y superior",
                "En cuartos",
                "Regresar al tamaño anterior",
            ]
        );
        assert!(matches!(layout[0], MenuItem::Header { .. }));
        // All geometry actions are enabled because there is an active window.
        assert!(
            layout[1..18]
                .iter()
                .filter(|item| matches!(item, MenuItem::Action { .. } | MenuItem::Submenu { .. }))
                .all(is_enabled)
        );
        // Restore is disabled because the active window has no saved rectangle.
        assert!(!is_enabled(&layout[19]));
        // The submenu actions carry the active window index, or `on_action`
        // parses an empty argument and silently does nothing.
        assert!(matches!(
            &layout[1],
            MenuItem::Action { id, .. } if id == "app-move-left:0"
        ));
        assert!(matches!(
            &layout[19],
            MenuItem::Action { id, .. } if id == "app-move-restore:0"
        ));
    }

    #[test]
    fn restore_is_enabled_when_the_active_window_is_restorable() {
        let mut toplevels = toplevels();
        toplevels[0].restorable = true;
        let items = window_menu_items("Foot", "foot", &toplevels);
        let Some(MenuItem::Submenu { items: layout, .. }) = items
            .iter()
            .find(|item| label(item) == Some("Trasladar y redimensionar"))
        else {
            panic!("missing the layout submenu");
        };
        assert!(is_enabled(&layout[19]));
    }

    #[test]
    fn help_menu_has_the_search_field_and_only_the_report_enabled() {
        let items = help_menu_items("Foot");
        assert!(matches!(items[0], MenuItem::SearchField { .. }));
        assert!(enabled(&items, "Informar de un problema…"));
        assert!(!enabled(&items, "Ayuda de Foot"));
        assert!(!enabled(&items, "Atajos de teclado de MalaysiaDreams"));
        assert!(!enabled(&items, "Documentación de MalaysiaDreams"));
    }

    fn search_results(query: &str) -> Vec<(String, String)> {
        searchable_menu_items("Foot", "foot", &toplevels())
            .into_iter()
            .filter(|entry| {
                entry
                    .item
                    .label()
                    .is_some_and(|label| normalize_query(label).contains(&normalize_query(query)))
            })
            .map(|entry| (entry.path, entry.item.label().unwrap().to_string()))
            .collect()
    }

    #[test]
    fn search_finds_hide_actions_under_app_path() {
        let results = search_results("ocul");
        assert_eq!(
            results,
            vec![
                ("Foot".to_string(), "Ocultar Foot".to_string()),
                ("Foot".to_string(), "Ocultar otros".to_string()),
            ]
        );
    }

    #[test]
    fn search_finds_left_actions_under_window_layout_path() {
        let results = search_results("IZQUIER");
        let labels: Vec<&str> = results.iter().map(|(_, label)| label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Izquierda",
                "Superior izquierdo",
                "Inferior izquierdo",
                "Izquierda y derecha",
                "Derecha e izquierda",
                "Izquierda de la pantalla",
            ]
        );
        let window_layout = "Ventana › Trasladar y redimensionar";
        for (path, _) in &results[..5] {
            assert_eq!(path, window_layout);
        }
        assert_eq!(results[5].0, "Ventana › Ventana a pantalla completa");
    }

    #[test]
    fn search_locations_trace_title_and_submenu_indices() {
        let entries = searchable_menu_items("Foot", "foot", &toplevels());
        let left = entries
            .iter()
            .find(|e| e.item.label() == Some("Izquierda"))
            .expect("Izquierda should be searchable");
        let location = left.location.as_ref().expect("menu entry has a location");
        assert_eq!(
            location.title_index, 1,
            "Izquierda lives in the Ventana menu"
        );
        assert_eq!(
            location.indices,
            vec![5, 1],
            "Ventana[5] is the layout submenu, then Izquierda[1] inside it"
        );
    }

    fn normalize_query(text: &str) -> String {
        text.to_lowercase()
            .replace('á', "a")
            .replace('é', "e")
            .replace('í', "i")
            .replace('ó', "o")
            .replace(['ú', 'ü'], "u")
            .replace('ñ', "n")
    }
}
