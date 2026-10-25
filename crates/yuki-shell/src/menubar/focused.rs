//! Menus for the focused application (14c).
//!
//! Apps without DBusMenu get the three menus macOS shows: the app menu,
//! Ventana and Ayuda. The constructors are pure so the tests can drive them
//! with fake [`ToplevelInfo`]s; [`on_action`] reads the live list again.

use celestial_kit::menu::{CheckState, MenuItem};
use celestial_kit::platform::{ToplevelInfo, Toplevels};

use crate::menubar::{icons, xdg_open};

const ISSUES_URL: &str = "https://github.com/HBlanqueto/yukiwm/issues";

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
            Some(icons::INFO),
        ),
        MenuItem::Separator,
        action(
            "app-settings",
            "Configuración…",
            Some("Super+,"),
            false,
            Some(icons::GEAR),
        ),
        MenuItem::Separator,
        MenuItem::Submenu {
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
    let active = toplevels
        .iter()
        .position(|window| window.app_id == app_id && window.active);
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
            "app-window-fill",
            "Rellenar",
            Some("Ctrl+Super+Return"),
            false,
            None,
        ),
        action(
            "app-window-center",
            "Centrar",
            Some("Ctrl+Super+C"),
            false,
            None,
        ),
        MenuItem::Separator,
        MenuItem::Submenu {
            label: "Trasladar y redimensionar".to_string(),
            enabled: true,
            icon: Some(icons::GRID),
            items: move_resize_items(),
        },
        MenuItem::Submenu {
            label: "Ventana a pantalla completa".to_string(),
            enabled: true,
            icon: Some(icons::SPLIT),
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
                    "app-window-fullscreen",
                    "Activar pantalla completa",
                    Some("Ctrl+Super+F"),
                    false,
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
            check: CheckState::Checkbox(window.active),
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
            "Atajos de teclado de Yuki",
            None,
            false,
            None,
        ),
        action("app-help-docs", "Documentación de Yuki", None, false, None),
        action(
            "app-help-report",
            "Informar de un problema…",
            None,
            true,
            None,
        ),
    ]
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
        _ => {}
    }
}

fn move_resize_items() -> Vec<MenuItem> {
    vec![
        header("Mitades"),
        action(
            "app-move-left",
            "Izquierda",
            Some("Ctrl+Super+Left"),
            false,
            Some(icons::HALF_LEFT),
        ),
        action(
            "app-move-right",
            "Derecha",
            Some("Ctrl+Super+Right"),
            false,
            Some(icons::HALF_RIGHT),
        ),
        action(
            "app-move-up",
            "Superior",
            Some("Ctrl+Super+Up"),
            false,
            Some(icons::HALF_TOP),
        ),
        action(
            "app-move-down",
            "Inferior",
            Some("Ctrl+Super+Down"),
            false,
            Some(icons::HALF_BOTTOM),
        ),
        MenuItem::Separator,
        header("Cuartos"),
        action(
            "app-quarter-top-left",
            "Parte superior izquierda",
            None,
            false,
            Some(icons::QUARTER_TOP_LEFT),
        ),
        action(
            "app-quarter-top-right",
            "Parte superior derecha",
            None,
            false,
            Some(icons::QUARTER_TOP_RIGHT),
        ),
        action(
            "app-quarter-bottom-left",
            "Parte inferior izquierda",
            None,
            false,
            Some(icons::QUARTER_BOTTOM_LEFT),
        ),
        action(
            "app-quarter-bottom-right",
            "Parte inferior derecha",
            None,
            false,
            Some(icons::QUARTER_BOTTOM_RIGHT),
        ),
        MenuItem::Separator,
        header("Disposición"),
        action(
            "app-layout-side-by-side",
            "Izquierda y derecha",
            Some("Ctrl+Shift+Super+Left"),
            false,
            Some(icons::HALVES_SIDE_BY_SIDE),
        ),
        action(
            "app-layout-reversed-side-by-side",
            "Derecha e izquierda",
            Some("Ctrl+Shift+Super+Right"),
            false,
            Some(icons::HALVES_SIDE_BY_SIDE),
        ),
        action(
            "app-layout-stacked",
            "Superior e inferior",
            Some("Ctrl+Shift+Super+Up"),
            false,
            Some(icons::HALVES_STACKED),
        ),
        action(
            "app-layout-reversed-stacked",
            "Inferior y superior",
            Some("Ctrl+Shift+Super+Down"),
            false,
            Some(icons::HALVES_STACKED),
        ),
        action(
            "app-layout-quarters",
            "Cuartos",
            None,
            false,
            Some(icons::GRID),
        ),
        MenuItem::Separator,
        action(
            "app-move-restore",
            "Volver al tamaño anterior",
            Some("Ctrl+Super+R"),
            false,
            None,
        ),
    ]
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
    }
}

fn action(
    id: impl Into<String>,
    label: impl Into<String>,
    shortcut: Option<&str>,
    enabled: bool,
    icon: Option<&'static str>,
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
