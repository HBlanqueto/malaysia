use barrier_kit::menu::{CheckState, MenuIcon, MenuItem, SearchSource, SearchableEntry};
use barrier_kit::shell::{InfinityLayout, InfinityPairLayout, ToplevelInfo, Toplevels};

use crate::menubar::{help_topics, icons, items};
use crate::spawn::{spawn_component_with, xdg_open};

const ISSUES_URL: &str = "https://github.com/HBlanqueto/malaysia/issues";

pub fn app_menu_items(app_name: &str, app_id: &str, toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let has_windows = app_windows(toplevels, app_id).next().is_some();
    let other_unminimized = toplevels
        .iter()
        .any(|window| window.app_id != app_id && !window.minimized);
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    vec![
        items::action(
            format!("app-about:{app_id}"),
            format!("Acerca de {app_name}"),
            None,
            !app_id.is_empty(),
            Some(MenuIcon::svg(icons::INFO)),
        ),
        MenuItem::Separator,
        items::action(
            "app-settings",
            "Ajustes…",
            None,
            false,
            Some(MenuIcon::svg(icons::GEAR)),
        ),
        MenuItem::Separator,
        services_submenu(),
        MenuItem::Separator,
        items::action(
            format!("app-hide:{app_id}"),
            format!("Ocultar {app_name}"),
            Some("Super+H"),
            has_windows,
            None,
        ),
        items::action(
            format!("app-hide-others:{app_id}"),
            "Ocultar otros",
            Some("Alt+Super+H"),
            other_unminimized,
            None,
        ),
        items::action("app-show-all", "Mostrar todo", None, any_minimized, None),
        MenuItem::Separator,
        items::action(
            format!("app-quit:{app_id}"),
            format!("Salir de {app_name}"),
            Some("Super+Q"),
            has_windows,
            None,
        ),
    ]
}

pub fn window_menu_items(
    app_name: &str,
    app_id: &str,
    toplevels: &[ToplevelInfo],
) -> Vec<MenuItem> {
    let app: Vec<&ToplevelInfo> = app_windows(toplevels, app_id).collect();
    let active = active_window(&app);
    let mut items = window_controls(active);
    items.extend([
        MenuItem::Separator,
        items::action(
            active_id("app-window-close", active),
            "Cerrar ventana",
            Some("Super+W"),
            active.is_some(),
            None,
        ),
        MenuItem::Separator,
        items::action(
            format!("app-window-front:{app_id}"),
            "Traer todo al frente",
            None,
            !app.is_empty(),
            None,
        ),
        items::action(
            "app-window-remove",
            "Quitar ventana del conjunto",
            None,
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "app-tab-previous",
            "Mostrar pestaña anterior",
            Some("Ctrl+Shift+Tab"),
            false,
            None,
        ),
        items::action(
            "app-tab-next",
            "Mostrar pestaña siguiente",
            Some("Ctrl+Tab"),
            false,
            None,
        ),
        items::action(
            "app-tab-new-window",
            "Mover pestaña a una ventana nueva",
            None,
            false,
            None,
        ),
        items::action(
            "app-tab-merge",
            "Unir todas las ventanas",
            None,
            false,
            None,
        ),
        MenuItem::Separator,
    ]);
    items.extend(app.iter().map(|window| {
        let checked = active.is_some_and(|active| active.id == window.id);
        window_item(window, app_name, CheckState::Checkbox(checked))
    }));
    items
}

pub(crate) fn window_controls(active: Option<&ToplevelInfo>) -> Vec<MenuItem> {
    let has_active = active.is_some();
    vec![
        items::action(
            active_id("app-window-minimize", active),
            "Minimizar",
            Some("Super+M"),
            has_active,
            None,
        ),
        items::action(
            active_id("app-window-maximize", active),
            "Maximizar",
            None,
            has_active,
            None,
        ),
        items::action(
            active_id("app-window-fill", active),
            "Rellenar",
            Some("Ctrl+Super+Return"),
            has_active,
            None,
        ),
        items::action(
            active_id("app-window-center", active),
            "Centrar",
            Some("Ctrl+Super+C"),
            has_active,
            None,
        ),
        MenuItem::Separator,
        items::submenu(
            "Trasladar y redimensionar",
            true,
            Some(MenuIcon::svg(icons::GRID)),
            move_resize_items(active),
        ),
        items::submenu(
            "Ventana a pantalla completa",
            has_active,
            Some(MenuIcon::svg(icons::SPLIT)),
            vec![
                items::action(
                    "app-window-screen-left",
                    "Izquierda de la pantalla",
                    None,
                    false,
                    None,
                ),
                items::action(
                    "app-window-screen-right",
                    "Derecha de la pantalla",
                    None,
                    false,
                    None,
                ),
                MenuItem::Separator,
                items::action(
                    active_id("app-window-fullscreen", active),
                    "Activar pantalla completa",
                    Some("Ctrl+Super+F"),
                    has_active,
                    None,
                ),
            ],
        ),
    ]
}

pub(crate) fn window_item(window: &ToplevelInfo, app_name: &str, check: CheckState) -> MenuItem {
    let label = if window.title.is_empty() || window.title == app_name {
        app_name.to_string()
    } else {
        format!("{app_name} — {}", window.title)
    };
    MenuItem::Action {
        id: format!("app-window-activate:{}", window.id),
        label,
        shortcut: None,
        enabled: true,
        check,
        icon: None,
    }
}

pub(crate) fn services_submenu() -> MenuItem {
    items::submenu(
        "Servicios",
        true,
        None,
        vec![
            items::action(
                "app-services-none",
                "Ningún servicio disponible",
                None,
                false,
                None,
            ),
            MenuItem::Separator,
            items::action(
                "app-services-settings",
                "Ajustes de servicios…",
                None,
                false,
                None,
            ),
        ],
    )
}

pub fn help_menu_items(app_name: &str) -> Vec<MenuItem> {
    vec![
        MenuItem::SearchField {
            placeholder: "Buscar".to_string(),
        },
        items::action(
            "app-help",
            format!("Ayuda de {app_name}"),
            None,
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "app-help-shortcuts",
            "Atajos de teclado de Malaysia",
            None,
            false,
            None,
        ),
        items::action(
            "app-help-docs",
            "Documentación de Malaysia",
            None,
            false,
            None,
        ),
        items::action(
            "app-help-report",
            "Informar de un problema…",
            None,
            true,
            None,
        ),
    ]
}

pub fn help_search_topics() -> Vec<SearchableEntry> {
    help_topics::topics()
        .iter()
        .map(|topic| SearchableEntry {
            path: "Temas de ayuda".to_string(),
            item: topic.item(None),
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

pub fn on_action(id: String) {
    let toplevels = Toplevels::get();
    let list = toplevels.list.read().clone();
    let (verb, argument) = id.split_once(':').unwrap_or((id.as_str(), ""));
    match verb {
        "app-about" => spawn_component_with("about-app", &[argument]),
        "app-hide" => {
            for window in app_windows(&list, argument) {
                toplevels.minimize(window.id);
            }
        }
        "app-hide-others" => {
            for window in list
                .iter()
                .filter(|window| window.app_id != argument && !window.minimized)
            {
                toplevels.minimize(window.id);
            }
        }
        "app-show-all" => {
            for window in list.iter().filter(|window| window.minimized) {
                toplevels.unset_minimized(window.id);
            }
        }
        "app-quit" => {
            for window in app_windows(&list, argument) {
                toplevels.close(window.id);
            }
        }
        "app-window-front" => {
            let mut windows: Vec<&ToplevelInfo> = app_windows(&list, argument).collect();
            windows.sort_by_key(|window| window.active);
            for window in windows {
                toplevels.activate(window.id);
            }
        }
        "app-help-report" => xdg_open(ISSUES_URL),
        "app-help-topics-index" => xdg_open(help_topics::DOCS_INDEX),
        "app-help-topic" => xdg_open(argument),
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
        _ => {
            if let Ok(id) = argument.parse() {
                on_window_action(&toplevels, &list, verb, id);
            }
        }
    }
}

fn on_window_action(toplevels: &Toplevels, list: &[ToplevelInfo], verb: &str, id: u64) {
    match verb {
        "app-window-minimize" => toplevels.minimize(id),
        "app-window-maximize" => {
            let maximized = list
                .iter()
                .any(|window| window.id == id && window.maximized);
            toplevels.set_maximized(id, !maximized);
        }
        "app-window-close" => toplevels.close(id),
        "app-window-activate" => toplevels.activate(id),
        "app-move-restore" => toplevels.restore(id),
        "app-window-fullscreen" => toplevels.set_fullscreen(id, true),
        verb => {
            if let Some(layout) = window_layout(verb) {
                toplevels.arrange(id, layout);
            }
        }
    }
}

fn window_layout(verb: &str) -> Option<InfinityLayout> {
    Some(match verb {
        "app-window-fill" => InfinityLayout::Fill,
        "app-window-center" => InfinityLayout::Center,
        "app-move-left" => InfinityLayout::Left,
        "app-move-right" => InfinityLayout::Right,
        "app-move-up" => InfinityLayout::Top,
        "app-move-down" => InfinityLayout::Bottom,
        "app-quarter-top-left" => InfinityLayout::TopLeft,
        "app-quarter-top-right" => InfinityLayout::TopRight,
        "app-quarter-bottom-left" => InfinityLayout::BottomLeft,
        "app-quarter-bottom-right" => InfinityLayout::BottomRight,
        _ => return None,
    })
}

fn move_resize_items(active: Option<&ToplevelInfo>) -> Vec<MenuItem> {
    let has_active = active.is_some();
    let can_restore = active.is_some_and(|window| window.restorable);
    vec![
        items::header("Mitades"),
        items::action(
            active_id("app-move-left", active),
            "Izquierda",
            Some("Ctrl+Super+Left"),
            has_active,
            Some(MenuIcon::svg(icons::HALF_LEFT)),
        ),
        items::action(
            active_id("app-move-right", active),
            "Derecha",
            Some("Ctrl+Super+Right"),
            has_active,
            Some(MenuIcon::svg(icons::HALF_RIGHT)),
        ),
        items::action(
            active_id("app-move-up", active),
            "Superior",
            Some("Ctrl+Super+Up"),
            has_active,
            Some(MenuIcon::svg(icons::HALF_TOP)),
        ),
        items::action(
            active_id("app-move-down", active),
            "Inferior",
            Some("Ctrl+Super+Down"),
            has_active,
            Some(MenuIcon::svg(icons::HALF_BOTTOM)),
        ),
        MenuItem::Separator,
        items::header("Cuartos"),
        items::action(
            active_id("app-quarter-top-left", active),
            "Superior izquierdo",
            None,
            has_active,
            Some(MenuIcon::svg(icons::QUARTER_TOP_LEFT)),
        ),
        items::action(
            active_id("app-quarter-top-right", active),
            "Superior derecho",
            None,
            has_active,
            Some(MenuIcon::svg(icons::QUARTER_TOP_RIGHT)),
        ),
        items::action(
            active_id("app-quarter-bottom-left", active),
            "Inferior izquierdo",
            None,
            has_active,
            Some(MenuIcon::svg(icons::QUARTER_BOTTOM_LEFT)),
        ),
        items::action(
            active_id("app-quarter-bottom-right", active),
            "Inferior derecho",
            None,
            has_active,
            Some(MenuIcon::svg(icons::QUARTER_BOTTOM_RIGHT)),
        ),
        MenuItem::Separator,
        items::header("Disposición"),
        items::action(
            active_id("app-layout-side-by-side", active),
            "Izquierda y derecha",
            Some("Ctrl+Shift+Super+Left"),
            has_active,
            Some(MenuIcon::svg(icons::HALVES_LEFT_RIGHT)),
        ),
        items::action(
            active_id("app-layout-reversed-side-by-side", active),
            "Derecha e izquierda",
            Some("Ctrl+Shift+Super+Right"),
            has_active,
            Some(MenuIcon::svg(icons::HALVES_RIGHT_LEFT)),
        ),
        items::action(
            active_id("app-layout-stacked", active),
            "Superior e inferior",
            Some("Ctrl+Shift+Super+Up"),
            has_active,
            Some(MenuIcon::svg(icons::HALVES_TOP_BOTTOM)),
        ),
        items::action(
            active_id("app-layout-reversed-stacked", active),
            "Inferior y superior",
            Some("Ctrl+Shift+Super+Down"),
            has_active,
            Some(MenuIcon::svg(icons::HALVES_BOTTOM_TOP)),
        ),
        items::action(
            active_id("app-layout-quarters", active),
            "En cuartos",
            None,
            has_active,
            Some(MenuIcon::svg(icons::GRID)),
        ),
        MenuItem::Separator,
        items::action(
            active_id("app-move-restore", active),
            "Regresar al tamaño anterior",
            Some("Ctrl+Super+R"),
            can_restore,
            None,
        ),
    ]
}

fn active_window<'a>(app: &[&'a ToplevelInfo]) -> Option<&'a ToplevelInfo> {
    app.iter()
        .copied()
        .find(|window| window.active)
        .or_else(|| app.last().copied())
}

fn app_windows<'a>(
    toplevels: &'a [ToplevelInfo],
    app_id: &'a str,
) -> impl Iterator<Item = &'a ToplevelInfo> {
    toplevels
        .iter()
        .filter(move |window| window.app_id == app_id)
}

fn active_id(prefix: &str, active: Option<&ToplevelInfo>) -> String {
    active.map_or_else(String::new, |window| format!("{prefix}:{}", window.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(
        id: u64,
        app_id: &str,
        title: &str,
        active: bool,
        minimized: bool,
        restorable: bool,
    ) -> ToplevelInfo {
        ToplevelInfo {
            id,
            title: title.to_string(),
            app_id: app_id.to_string(),
            active,
            minimized,
            restorable,
            ..Default::default()
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
            window(20, "foot", "Malaysia", true, false, false),
            window(21, "foot", "Segunda ventana", false, true, false),
            window(
                22,
                "org.mozilla.firefox",
                "Mozilla Firefox",
                false,
                false,
                false,
            ),
        ]
    }

    #[test]
    fn the_active_window_prefers_the_flag_then_the_last_window() {
        let mut list = [
            window(20, "foot", "Malaysia", false, false, false),
            window(21, "foot", "Segunda ventana", false, false, false),
        ];
        assert_eq!(active_window(&[&list[0], &list[1]]).map(|w| w.id), Some(21));
        list[0].active = true;
        assert_eq!(active_window(&[&list[0], &list[1]]).map(|w| w.id), Some(20));
    }

    #[test]
    fn show_all_is_off_without_minimized_windows() {
        let items = app_menu_items(
            "Foot",
            "foot",
            &[
                window(20, "foot", "Malaysia", true, false, false),
                window(21, "foot", "Otra", false, false, false),
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
        assert_eq!(names, vec!["Foot — Malaysia", "Foot — Segunda ventana"]);
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
        assert!(matches!(layout[0], MenuItem::Header { .. }));
        assert!(
            layout[1..18]
                .iter()
                .filter(|item| matches!(item, MenuItem::Action { .. } | MenuItem::Submenu { .. }))
                .all(is_enabled)
        );
        assert!(!is_enabled(&layout[19]));
        assert!(matches!(
            &layout[1],
            MenuItem::Action { id, .. } if id == "app-move-left:20"
        ));
        assert!(matches!(
            &layout[19],
            MenuItem::Action { id, .. } if id == "app-move-restore:20"
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
}
