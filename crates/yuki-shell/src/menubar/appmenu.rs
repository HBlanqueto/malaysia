//! The application menu title in the menubar.
//!
//! Shows the active application's name in bold. When no window is focused it
//! reads «Yuki» and exposes the Yuki desktop menu.

use std::rc::Rc;

use celestial_kit::menu::{CheckState, MenuItem};
use celestial_kit::platform::{ToplevelInfo, Toplevels};
use freya_core::prelude::*;

use crate::menubar::menu::{MenuState, MenuTitleSpec, menu_title};
use crate::menubar::{create_desktop_folder, spawn_component, xdg_open, xdg_user_dir};

/// Items for the Yuki desktop menu.
pub fn yuki_menu_items() -> Vec<MenuItem> {
    vec![
        go_menu(),
        file_menu(),
        edit_menu(),
        window_menu(),
        system_menu(),
    ]
}

fn go_menu() -> MenuItem {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let documents =
        xdg_user_dir("DOCUMENTS").or_else(|| home.as_ref().map(|h| h.join("Documents")));
    let downloads = xdg_user_dir("DOWNLOAD").or_else(|| home.as_ref().map(|h| h.join("Downloads")));
    let desktop = xdg_user_dir("DESKTOP").or_else(|| home.as_ref().map(|h| h.join("Desktop")));

    MenuItem::Submenu {
        label: "Ir".to_string(),
        enabled: true,
        items: vec![
            MenuItem::Action {
                id: "go-back".to_string(),
                label: "Atrás".to_string(),
                shortcut: Some("Cmd+[".to_string()),
                enabled: false,
                check: CheckState::None,
            },
            MenuItem::Action {
                id: "go-forward".to_string(),
                label: "Adelante".to_string(),
                shortcut: Some("Cmd+]".to_string()),
                enabled: false,
                check: CheckState::None,
            },
            MenuItem::Action {
                id: "go-to-folder".to_string(),
                label: "Ir a la carpeta…".to_string(),
                shortcut: Some("Cmd+Shift+G".to_string()),
                enabled: false,
                check: CheckState::None,
            },
            MenuItem::Separator,
            open_action("go-home", "Inicio", home),
            open_action("go-documents", "Documentos", documents),
            open_action("go-downloads", "Descargas", downloads),
            open_action("go-desktop", "Escritorio", desktop),
            MenuItem::Separator,
            MenuItem::Action {
                id: "go-trash".to_string(),
                label: "Papelera".to_string(),
                shortcut: Some("Cmd+Shift+Supr".to_string()),
                enabled: true,
                check: CheckState::None,
            },
        ],
    }
}

fn open_action(id: &str, label: &str, path: Option<std::path::PathBuf>) -> MenuItem {
    MenuItem::Action {
        id: id.to_string(),
        label: label.to_string(),
        shortcut: None,
        enabled: path.is_some(),
        check: CheckState::None,
    }
}

fn file_menu() -> MenuItem {
    MenuItem::Submenu {
        label: "Archivo".to_string(),
        enabled: true,
        items: vec![
            MenuItem::Action {
                id: "file-new-window".to_string(),
                label: "Nueva ventana".to_string(),
                shortcut: Some("Cmd+N".to_string()),
                enabled: true,
                check: CheckState::None,
            },
            MenuItem::Action {
                id: "file-open".to_string(),
                label: "Abrir…".to_string(),
                shortcut: Some("Cmd+O".to_string()),
                enabled: false,
                check: CheckState::None,
            },
            MenuItem::Separator,
            MenuItem::Action {
                id: "file-new-folder-desktop".to_string(),
                label: "Nueva carpeta en el Escritorio".to_string(),
                shortcut: Some("Cmd+Shift+N".to_string()),
                enabled: true,
                check: CheckState::None,
            },
            MenuItem::Separator,
            MenuItem::Action {
                id: "file-search".to_string(),
                label: "Buscar".to_string(),
                shortcut: Some("Cmd+Space".to_string()),
                enabled: true,
                check: CheckState::None,
            },
        ],
    }
}

fn edit_menu() -> MenuItem {
    MenuItem::Submenu {
        label: "Edición".to_string(),
        enabled: true,
        items: vec![
            disabled_action("edit-undo", "Deshacer", Some("Cmd+Z")),
            MenuItem::Separator,
            disabled_action("edit-cut", "Cortar", Some("Cmd+X")),
            disabled_action("edit-copy", "Copiar", Some("Cmd+C")),
            disabled_action("edit-paste", "Pegar", Some("Cmd+V")),
            MenuItem::Separator,
            disabled_action("edit-select-all", "Seleccionar todo", Some("Cmd+A")),
        ],
    }
}

fn window_menu() -> MenuItem {
    let mut items = vec![
        MenuItem::Action {
            id: "window-minimize-all".to_string(),
            label: "Minimizar todo".to_string(),
            shortcut: Some("Cmd+Alt+M".to_string()),
            enabled: true,
            check: CheckState::None,
        },
        MenuItem::Separator,
    ];

    let toplevels = Toplevels::get();
    let windows: Vec<ToplevelInfo> = toplevels.list.read().clone();
    if windows.is_empty() {
        items.push(MenuItem::Action {
            id: "window-none".to_string(),
            label: "No hay ventanas abiertas".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        });
    } else {
        for (index, window) in windows.iter().enumerate() {
            let label = if window.title.is_empty() {
                window.app_id.clone()
            } else {
                window.title.clone()
            };
            items.push(MenuItem::Action {
                id: format!("window-activate-{index}"),
                label,
                shortcut: None,
                enabled: true,
                check: CheckState::None,
            });
        }
    }

    MenuItem::Submenu {
        label: "Ventana".to_string(),
        enabled: true,
        items,
    }
}

fn system_menu() -> MenuItem {
    MenuItem::Submenu {
        label: "Sistema".to_string(),
        enabled: true,
        items: vec![
            disabled_action("system-about", "Acerca de este equipo", None),
            disabled_action("system-settings", "Ajustes del sistema…", None),
            MenuItem::Separator,
            MenuItem::Action {
                id: "system-lock".to_string(),
                label: "Bloquear pantalla".to_string(),
                shortcut: Some("Ctrl+Alt+L".to_string()),
                enabled: true,
                check: CheckState::None,
            },
            MenuItem::Separator,
            disabled_action("system-sleep", "Suspender", None),
            disabled_action("system-restart", "Reiniciar…", None),
            disabled_action("system-shutdown", "Apagar…", None),
            MenuItem::Separator,
            disabled_action("system-logout", "Cerrar sesión…", Some("Cmd+Shift+Q")),
        ],
    }
}

fn disabled_action(id: &str, label: &str, shortcut: Option<&str>) -> MenuItem {
    MenuItem::Action {
        id: id.to_string(),
        label: label.to_string(),
        shortcut: shortcut.map(|s| s.to_string()),
        enabled: false,
        check: CheckState::None,
    }
}

/// Handles actions from the Yuki desktop menu.
pub fn on_yuki_action(id: String) {
    match id.as_str() {
        "go-home" => {
            if let Some(home) = std::env::var_os("HOME") {
                xdg_open(std::path::PathBuf::from(home));
            }
        }
        "go-documents" => {
            if let Some(path) = xdg_user_dir("DOCUMENTS").or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Documents"))
            }) {
                xdg_open(path);
            }
        }
        "go-downloads" => {
            if let Some(path) = xdg_user_dir("DOWNLOAD").or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Downloads"))
            }) {
                xdg_open(path);
            }
        }
        "go-desktop" => {
            if let Some(path) = xdg_user_dir("DESKTOP").or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Desktop"))
            }) {
                xdg_open(path);
            }
        }
        "go-trash" => {
            xdg_open(std::path::PathBuf::from("trash:///"));
        }
        "file-new-window" => {
            if let Some(home) = std::env::var_os("HOME") {
                xdg_open(std::path::PathBuf::from(home));
            }
        }
        "file-new-folder-desktop" => create_desktop_folder(),
        "file-search" => spawn_component("launcher"),
        "window-minimize-all" => Toplevels::get().minimize_all(),
        id if id.starts_with("window-activate-") => {
            if let Ok(index) = id["window-activate-".len()..].parse::<usize>() {
                Toplevels::get().activate(index);
            }
        }
        "system-lock" => spawn_component("lock"),
        _ => {}
    }
}

/// Renders the application-name title (or the Yuki menu when no app is focused).
pub fn app_menu_title(_app: String, state: MenuState, titles: Rc<Vec<MenuTitleSpec>>) -> Element {
    menu_title(1, titles, state)
}
