//! The desktop menus of the menubar.
//!
//! With no focused window the bar shows the Yuki menu next to the shared
//! Archivo, Edición, Visualización, Ir, Ventana and Ayuda menus. Each title owns
//! one menu and every action reuses a shell helper that already exists.

use std::env;
use std::path::PathBuf;

use celestial_kit::menu::{CheckState, MenuItem};
use celestial_kit::platform::{ToplevelInfo, Toplevels};

use crate::desktop::DesktopEntry;
use crate::menubar::{
    create_desktop_folder, resolve_app_name, spawn_component, xdg_open, xdg_user_dir,
};

/// The Yuki menu (about, settings, empty trash).
pub fn yuki_menu_items() -> Vec<MenuItem> {
    vec![
        action("yuki-about", "Acerca de Yuki", false),
        MenuItem::Separator,
        action("yuki-settings", "Ajustes…", false),
        MenuItem::Separator,
        action("yuki-empty-trash", "Vaciar papelera…", false),
    ]
}

/// The Archivo menu.
pub fn file_menu_items() -> Vec<MenuItem> {
    vec![
        action("file-new-window", "Nueva ventana", true),
        action(
            "file-new-folder-desktop",
            "Nueva carpeta en el escritorio",
            true,
        ),
        action("file-open", "Abrir…", false),
        action("file-search", "Buscar", true),
    ]
}

/// The Edición menu. Needs a selection, so every item is disabled for now.
pub fn edit_menu_items() -> Vec<MenuItem> {
    vec![
        action("edit-undo", "Deshacer", false),
        action("edit-cut", "Cortar", false),
        action("edit-copy", "Copiar", false),
        action("edit-paste", "Pegar", false),
        action("edit-select-all", "Seleccionar todo", false),
    ]
}

/// The Visualización menu.
pub fn view_menu_items() -> Vec<MenuItem> {
    vec![
        action("view-toggle-dock", "Mostrar/ocultar dock", false),
        action("view-change-wallpaper", "Cambiar fondo de pantalla…", false),
    ]
}

/// The Ir menu: the XDG user folders and the trash.
pub fn go_menu_items() -> Vec<MenuItem> {
    vec![
        action("go-back", "Atrás", false),
        action("go-forward", "Adelante", false),
        MenuItem::Separator,
        action("go-home", "Inicio", true),
        action("go-desktop", "Escritorio", true),
        action("go-documents", "Documentos", true),
        action("go-downloads", "Descargas", true),
        action("go-pictures", "Imágenes", true),
        action("go-music", "Música", true),
        action("go-videos", "Vídeos", true),
        MenuItem::Separator,
        action("go-trash", "Papelera", true),
        MenuItem::Separator,
        action("go-to-folder", "Ir a la carpeta…", false),
    ]
}

/// The Ventana menu: minimize/show desktop and the open windows.
pub fn window_menu_items(entries: &[DesktopEntry]) -> Vec<MenuItem> {
    let mut items = vec![
        action("window-minimize-all", "Minimizar todo", true),
        action("window-show-desktop", "Mostrar escritorio", true),
        MenuItem::Separator,
    ];

    let windows: Vec<ToplevelInfo> = Toplevels::get().list.read().clone();
    if windows.is_empty() {
        items.push(action("window-none", "No hay ventanas abiertas", false));
    } else {
        for (index, window) in windows.iter().enumerate() {
            let name = resolve_app_name(Some(&window.app_id), entries);
            let label = if window.title.is_empty() || window.title == name {
                name
            } else {
                format!("{name} — {title}", title = window.title)
            };
            items.push(action(format!("window-activate-{index}"), label, true));
        }
    }

    items
}

/// The Ayuda menu.
pub fn help_menu_items() -> Vec<MenuItem> {
    vec![
        action("help-shortcuts", "Atajos de teclado", false),
        action("help-docs", "Documentación de Yuki", false),
    ]
}

fn action(id: impl Into<String>, label: impl Into<String>, enabled: bool) -> MenuItem {
    MenuItem::Action {
        id: id.into(),
        label: label.into(),
        shortcut: None,
        enabled,
        check: CheckState::None,
    }
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
        "file-new-folder-desktop" => create_desktop_folder(),
        "file-search" => spawn_component("launcher"),
        // The compositor only exposes minimize for now; both entries hide the
        // windows, which is what show-desktop asks for.
        "window-minimize-all" | "window-show-desktop" => Toplevels::get().minimize_all(),
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
