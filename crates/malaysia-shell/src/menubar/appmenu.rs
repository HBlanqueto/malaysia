//! The desktop menus of the menubar.
//!
//! With no focused window the bar shows the Escritorio menu next to the shared
//! Archivo, Edición, Visualización, Ir, Ventana and Ayuda menus. Each title owns
//! one menu and every action reuses a shell helper that already exists.

/// The menubar title shown when no application is focused.
pub const TITLE: &str = "Escritorio";

use std::env;
use std::path::PathBuf;

use barrier_kit::menu::{MenuIcon, MenuItem};
use barrier_kit::platform::{ToplevelInfo, Toplevels};

use crate::desktop::DesktopEntry;
use crate::menubar::{
    create_desktop_folder, icons, items, resolve_app_name, spawn_component, xdg_open, xdg_user_dir,
};

/// The Escritorio menu (about, settings, empty trash, services, hide/show).
pub fn malaysia_menu_items() -> Vec<MenuItem> {
    vec![
        items::action(
            "malaysia-about",
            "Acerca de MalaysiaDreams",
            None,
            false,
            Some(MenuIcon::svg(icons::INFO)),
        ),
        items::separator(),
        items::action(
            "malaysia-settings",
            "Ajustes…",
            Some("Super+,"),
            false,
            Some(MenuIcon::svg(icons::GEAR)),
        ),
        items::separator(),
        items::action(
            "malaysia-empty-trash",
            "Vaciar papelera…",
            Some("Shift+Super+BackSpace"),
            false,
            Some(MenuIcon::svg(icons::TRASH)),
        ),
        items::separator(),
        items::submenu(
            None,
            "Servicios",
            true,
            None,
            vec![
                items::action(
                    "services-none",
                    "Ningún servicio disponible",
                    None,
                    false,
                    None,
                ),
                items::separator(),
                items::action(
                    "services-settings",
                    "Ajustes de servicios…",
                    None,
                    false,
                    None,
                ),
            ],
        ),
        items::separator(),
        // shortcut: Alt+Super+H is not wired in the compositor for the desktop,
        // so this enabled item shows no shortcut to stay honest.
        items::action("desktop-hide-others", "Ocultar otros", None, true, None),
        items::action("desktop-show-all", "Mostrar todo", None, true, None),
    ]
}

/// The Archivo menu.
pub fn file_menu_items() -> Vec<MenuItem> {
    vec![
        // shortcut: Super+N is not wired globally yet.
        items::action(
            "file-new-window",
            "Nueva ventana",
            None,
            true,
            Some(MenuIcon::svg(icons::WINDOW)),
        ),
        // shortcut: Shift+Super+N is not wired globally yet.
        items::action(
            "file-new-folder-desktop",
            "Nueva carpeta en el escritorio",
            None,
            true,
            Some(MenuIcon::svg(icons::FOLDER_PLUS)),
        ),
        items::action(
            "file-open",
            "Abrir…",
            Some("Super+O"),
            false,
            Some(MenuIcon::svg(icons::WINDOW)),
        ),
        // shortcut: Super+F is not wired globally yet; Super+Space opens the launcher.
        items::action(
            "file-search",
            "Buscar",
            None,
            true,
            Some(MenuIcon::svg(icons::SEARCH)),
        ),
    ]
}

/// The Edición menu. Needs a selection, so every item is disabled for now.
pub fn edit_menu_items() -> Vec<MenuItem> {
    vec![
        items::action(
            "edit-undo",
            "Deshacer",
            Some("Super+Z"),
            false,
            Some(MenuIcon::svg(icons::UNDO)),
        ),
        items::action(
            "edit-cut",
            "Cortar",
            Some("Super+X"),
            false,
            Some(MenuIcon::svg(icons::CUT)),
        ),
        items::action(
            "edit-copy",
            "Copiar",
            Some("Super+C"),
            false,
            Some(MenuIcon::svg(icons::COPY)),
        ),
        items::action(
            "edit-paste",
            "Pegar",
            Some("Super+V"),
            false,
            Some(MenuIcon::svg(icons::PASTE)),
        ),
        items::action(
            "edit-select-all",
            "Seleccionar todo",
            Some("Super+A"),
            false,
            Some(MenuIcon::svg(icons::SELECT_ALL)),
        ),
    ]
}

/// The Visualización menu.
pub fn view_menu_items() -> Vec<MenuItem> {
    vec![
        // Disabled until the shell can signal the dock to show/hide.
        items::action(
            "view-toggle-dock",
            "Mostrar/ocultar dock",
            None,
            false,
            Some(MenuIcon::svg(icons::DOCK)),
        ),
        items::action(
            "view-change-wallpaper",
            "Cambiar fondo de pantalla…",
            None,
            false,
            Some(MenuIcon::svg(icons::WALLPAPER)),
        ),
    ]
}

/// The Ir menu: the XDG user folders and the trash.
pub fn go_menu_items() -> Vec<MenuItem> {
    vec![
        items::action(
            "go-back",
            "Atrás",
            Some("Super+["),
            false,
            Some(MenuIcon::svg(icons::UNDO)),
        ),
        items::action(
            "go-forward",
            "Adelante",
            Some("Super+]"),
            false,
            Some(MenuIcon::svg(icons::REDO)),
        ),
        items::separator(),
        // Shortcuts for the enabled folder items below are not wired globally yet,
        // so they are omitted to keep the menu honest.
        items::action(
            "go-home",
            "Inicio",
            None,
            true,
            Some(MenuIcon::svg(icons::HOUSE)),
        ),
        items::action(
            "go-desktop",
            "Escritorio",
            None,
            true,
            Some(MenuIcon::svg(icons::DESKTOP)),
        ),
        items::action(
            "go-documents",
            "Documentos",
            None,
            true,
            Some(MenuIcon::svg(icons::DOCUMENTS)),
        ),
        items::action(
            "go-downloads",
            "Descargas",
            None,
            true,
            Some(MenuIcon::svg(icons::DOWNLOADS)),
        ),
        items::action(
            "go-pictures",
            "Imágenes",
            None,
            true,
            Some(MenuIcon::svg(icons::PICTURES)),
        ),
        items::action(
            "go-music",
            "Música",
            None,
            true,
            Some(MenuIcon::svg(icons::MUSIC)),
        ),
        items::action(
            "go-videos",
            "Vídeos",
            None,
            true,
            Some(MenuIcon::svg(icons::VIDEOS)),
        ),
        items::separator(),
        items::action(
            "go-trash",
            "Papelera",
            None,
            true,
            Some(MenuIcon::svg(icons::TRASH)),
        ),
        items::separator(),
        items::action(
            "go-to-folder",
            "Ir a la carpeta…",
            Some("Shift+Super+G"),
            false,
            Some(MenuIcon::svg(icons::FOLDER_GOTO)),
        ),
    ]
}

/// The Ventana menu: minimize/show desktop and the open windows.
pub fn window_menu_items(entries: &[DesktopEntry]) -> Vec<MenuItem> {
    let mut items = vec![
        // shortcut: Alt+Super+M is not wired globally yet.
        items::action("window-minimize-all", "Minimizar todo", None, true, None),
        items::action(
            "window-show-desktop",
            "Mostrar escritorio",
            None,
            true,
            None,
        ),
        items::separator(),
    ];

    let windows: Vec<ToplevelInfo> = Toplevels::get().list.read().clone();
    if windows.is_empty() {
        items.push(items::action(
            "window-none",
            "No hay ventanas abiertas",
            None,
            false,
            None,
        ));
    } else {
        for (index, window) in windows.iter().enumerate() {
            let name = resolve_app_name(Some(&window.app_id), entries);
            let label = if window.title.is_empty() || window.title == name {
                name
            } else {
                format!("{name} — {title}", title = window.title)
            };
            items.push(items::action(
                format!("window-activate-{index}"),
                label,
                None,
                true,
                None,
            ));
        }
    }

    items
}

/// The Ayuda menu.
pub fn help_menu_items() -> Vec<MenuItem> {
    vec![
        items::action(
            "help-search",
            "Buscar en la ayuda",
            None,
            false,
            Some(MenuIcon::svg(icons::SEARCH)),
        ),
        items::separator(),
        items::action(
            "help-docs",
            "Documentación de MalaysiaDreams",
            None,
            false,
            Some(MenuIcon::svg(icons::HELP)),
        ),
        items::action(
            "help-report",
            "Informar de un problema…",
            None,
            true,
            Some(MenuIcon::svg(icons::INFO)),
        ),
    ]
}

/// Handles an action from any of the desktop menus.
pub fn on_action(id: String) {
    match id.as_str() {
        "go-home" => open_home(),
        "go-desktop" => open_user_dir("DESKTOP", "Desktop"),
        "go-documents" => open_user_dir("DOCUMENTS", "Documents"),
        "go-downloads" => open_user_dir("DOWNLOAD", "Downloads"),
        "go-pictures" => open_user_dir("PICTURES", "Pictures"),
        "go-music" => open_user_dir("MUSIC", "Music"),
        "go-videos" => open_user_dir("VIDEOS", "Videos"),
        "go-trash" => xdg_open("trash:///"),
        "file-new-window" => open_home(),
        "file-search" => spawn_component("launcher"),
        "file-new-folder-desktop" => create_desktop_folder(),
        // The compositor only exposes minimize for now; both entries hide the
        // windows, which is what show-desktop asks for.
        "window-minimize-all" | "window-show-desktop" | "desktop-show-all" => {
            Toplevels::get().minimize_all()
        }
        "desktop-hide-others" => Toplevels::get().minimize_all(),
        "help-report" => xdg_open("https://github.com/HBlanqueto/malaysia/issues"),
        id if id.starts_with("window-activate-") => {
            if let Ok(index) = id["window-activate-".len()..].parse::<usize>() {
                Toplevels::get().activate(index);
            }
        }
        _ => {}
    }
}

fn open_home() {
    if let Some(home) = env::var_os("HOME") {
        xdg_open(PathBuf::from(home));
    }
}

fn open_user_dir(kind: &str, fallback: &str) {
    if let Some(path) = xdg_user_dir(kind)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
    {
        xdg_open(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use barrier_kit::menu::MenuItem;

    fn labels(items: &[MenuItem]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { label, .. }
                | MenuItem::Submenu { label, .. }
                | MenuItem::Header { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect()
    }

    fn all_actions(items: &[MenuItem], out: &mut Vec<MenuItem>) {
        for item in items {
            match item {
                MenuItem::Action { .. } => out.push(item.clone()),
                MenuItem::Submenu { items: sub, .. } => all_actions(sub, out),
                _ => {}
            }
        }
    }

    #[test]
    fn desktop_title_is_escritorio() {
        assert_eq!(TITLE, "Escritorio");
    }

    #[test]
    fn enabled_desktop_items_have_no_unwired_shortcut() {
        let mut actions = Vec::new();
        all_actions(&malaysia_menu_items(), &mut actions);
        all_actions(&file_menu_items(), &mut actions);
        all_actions(&edit_menu_items(), &mut actions);
        all_actions(&view_menu_items(), &mut actions);
        all_actions(&go_menu_items(), &mut actions);
        all_actions(&help_menu_items(), &mut actions);

        for item in &actions {
            let MenuItem::Action {
                enabled,
                shortcut,
                label,
                ..
            } = item
            else {
                continue;
            };
            if *enabled {
                assert!(
                    shortcut.is_none(),
                    "enabled item «{label}» shows a shortcut that is not wired globally"
                );
            }
        }
    }

    #[test]
    fn help_menu_contains_search_and_report_issue() {
        let items = help_menu_items();
        let labels = labels(&items);
        assert!(labels.contains(&"Buscar en la ayuda"), "{labels:?}");
        assert!(labels.contains(&"Informar de un problema…"), "{labels:?}");
    }

    #[test]
    fn file_edit_view_go_items_all_have_icons() {
        let mut actions = Vec::new();
        all_actions(&file_menu_items(), &mut actions);
        all_actions(&edit_menu_items(), &mut actions);
        all_actions(&view_menu_items(), &mut actions);
        all_actions(&go_menu_items(), &mut actions);

        for item in actions {
            let MenuItem::Action { icon, label, .. } = item else {
                continue;
            };
            assert!(icon.is_some(), "«{label}» is missing its icon");
        }
    }
}
