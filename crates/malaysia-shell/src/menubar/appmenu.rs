use std::env;
use std::path::PathBuf;

use barrier_kit::menu::{CheckState, MenuIcon, MenuItem};
use barrier_kit::shell::{ToplevelInfo, Toplevels};

use crate::desktop::DesktopEntry;
use crate::menubar::remote::Title;
use crate::menubar::{
    create_desktop_folder, focused, help_topics, icons, items, resolve_app_name, system,
    xdg_user_dir,
};
use crate::spawn::{spawn_component, xdg_open};

fn malaysia_menu_items(toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let other_unminimized = toplevels.iter().any(|window| !window.minimized);
    vec![
        items::action("malaysia-about", "Acerca de Malaysia", None, false, None),
        MenuItem::Separator,
        items::action(
            "malaysia-settings",
            "Ajustes…",
            None,
            true,
            Some(MenuIcon::svg(icons::GEAR)),
        ),
        MenuItem::Separator,
        items::action(
            "malaysia-empty-trash",
            "Vaciar papelera…",
            Some("Shift+Super+BackSpace"),
            false,
            Some(MenuIcon::svg(icons::TRASH)),
        ),
        MenuItem::Separator,
        focused::services_submenu(),
        MenuItem::Separator,
        items::action(
            "desktop-hide",
            "Ocultar Escritorio",
            Some("Super+H"),
            false,
            None,
        ),
        items::action(
            "app-hide-others",
            "Ocultar otros",
            None,
            other_unminimized,
            None,
        ),
        items::action("app-show-all", "Mostrar todo", None, any_minimized, None),
    ]
}

fn file_menu_items() -> Vec<MenuItem> {
    vec![
        items::action("file-new-window", "Nueva ventana", None, true, None),
        items::action(
            "file-new-folder-desktop",
            "Nueva carpeta en el escritorio",
            None,
            true,
            None,
        ),
        items::action(
            "file-new-folder-selection",
            "Nueva carpeta con la selección",
            Some("Ctrl+Super+N"),
            false,
            None,
        ),
        items::action(
            "file-new-tab",
            "Nueva pestaña",
            Some("Super+T"),
            false,
            None,
        ),
        items::action("file-open", "Abrir", Some("Super+O"), false, None),
        items::submenu("Abrir con", false, None, Vec::new()),
        items::action(
            "file-close-window",
            "Cerrar ventana",
            Some("Super+W"),
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "file-get-info",
            "Obtener información",
            Some("Super+I"),
            false,
            None,
        ),
        items::action("file-rename", "Renombrar", None, false, None),
        items::action("file-compress", "Comprimir", None, false, None),
        items::action("file-duplicate", "Duplicar", Some("Super+D"), false, None),
        items::action(
            "file-make-alias",
            "Crear enlace",
            Some("Ctrl+Super+A"),
            false,
            None,
        ),
        items::action(
            "file-quick-look",
            "Vista rápida",
            Some("Super+Y"),
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "file-share",
            "Compartir…",
            None,
            false,
            Some(MenuIcon::svg(icons::SHARE)),
        ),
        MenuItem::Separator,
        items::action(
            "file-move-to-trash",
            "Mover a la papelera",
            Some("Super+BackSpace"),
            false,
            Some(MenuIcon::svg(icons::TRASH)),
        ),
        items::action(
            "file-eject",
            "Expulsar",
            Some("Super+E"),
            false,
            Some(MenuIcon::svg(icons::EJECT)),
        ),
        MenuItem::Separator,
        items::action(
            "file-print",
            "Imprimir",
            Some("Super+P"),
            false,
            Some(MenuIcon::svg(icons::PRINTER)),
        ),
    ]
}

fn edit_menu_items() -> Vec<MenuItem> {
    vec![
        items::action("edit-undo", "Deshacer", Some("Super+Z"), false, None),
        items::action("edit-redo", "Rehacer", Some("Shift+Super+Z"), false, None),
        MenuItem::Separator,
        items::action("edit-cut", "Cortar", Some("Super+X"), false, None),
        items::action("edit-copy", "Copiar", Some("Super+C"), false, None),
        items::action("edit-paste", "Pegar", Some("Super+V"), false, None),
        items::action(
            "edit-select-all",
            "Seleccionar todo",
            Some("Super+A"),
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "edit-show-clipboard",
            "Mostrar portapapeles",
            None,
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "edit-search",
            "Buscar",
            Some("Super+Space"),
            true,
            Some(MenuIcon::svg(icons::SEARCH)),
        ),
        MenuItem::Separator,
        items::action(
            "edit-emoji",
            "Emojis y símbolos",
            None,
            false,
            Some(MenuIcon::svg(icons::EMOJI)),
        ),
    ]
}

fn view_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::Action {
            id: "view-as-icons".to_string(),
            label: "Como iconos".to_string(),
            shortcut: Some("Super+1".to_string()),
            enabled: false,
            check: CheckState::Checkbox(true),
            icon: None,
        },
        items::action(
            "view-as-list",
            "Como lista",
            Some("Super+2"),
            false,
            Some(MenuIcon::svg(icons::VIEW_LIST)),
        ),
        items::action(
            "view-as-columns",
            "Como columnas",
            Some("Super+3"),
            false,
            Some(MenuIcon::svg(icons::VIEW_COLUMNS)),
        ),
        items::action(
            "view-as-gallery",
            "Como galería",
            Some("Super+4"),
            false,
            Some(MenuIcon::svg(icons::VIEW_GALLERY)),
        ),
        MenuItem::Separator,
        items::action(
            "view-toggle-dock",
            "Mostrar/ocultar Dock",
            None,
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "view-change-wallpaper",
            "Cambiar fondo de pantalla…",
            None,
            false,
            None,
        ),
        items::action(
            "view-show-view-options",
            "Mostrar opciones de visualización",
            Some("Super+J"),
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "view-fullscreen",
            "Ver en pantalla completa",
            Some("Ctrl+Super+F"),
            false,
            None,
        ),
    ]
}

fn go_menu_items() -> Vec<MenuItem> {
    vec![
        items::action("go-back", "Atrás", Some("Super+["), false, None),
        items::action("go-forward", "Adelante", Some("Super+]"), false, None),
        items::action(
            "go-enclosing-folder",
            "Carpeta contenedora",
            Some("Super+Up"),
            false,
            None,
        ),
        MenuItem::Separator,
        items::action(
            "go-documents",
            "Documentos",
            None,
            true,
            Some(MenuIcon::svg(icons::DOCUMENTS)),
        ),
        items::action(
            "go-desktop",
            "Escritorio",
            None,
            true,
            Some(MenuIcon::svg(icons::DESKTOP)),
        ),
        items::action(
            "go-downloads",
            "Descargas",
            None,
            true,
            Some(MenuIcon::svg(icons::DOWNLOADS)),
        ),
        items::action(
            "go-home",
            "Inicio",
            None,
            true,
            Some(MenuIcon::svg(icons::HOUSE)),
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
        items::action(
            "go-applications",
            "Aplicaciones",
            None,
            true,
            Some(MenuIcon::svg(icons::APPS)),
        ),
        items::action(
            "go-trash",
            "Papelera",
            None,
            true,
            Some(MenuIcon::svg(icons::TRASH)),
        ),
        MenuItem::Separator,
        items::submenu("Carpetas recientes", false, None, Vec::new()),
        MenuItem::Separator,
        items::action(
            "go-to-folder",
            "Ir a la carpeta…",
            Some("Shift+Super+G"),
            false,
            None,
        ),
        items::action(
            "go-connect-to-server",
            "Conectarse al servidor…",
            Some("Super+K"),
            false,
            None,
        ),
    ]
}

fn window_menu_items(entries: &[DesktopEntry], toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let mut items = focused::window_controls(None);
    items.extend([
        MenuItem::Separator,
        items::action(
            "app-show-all",
            "Traer todo al frente",
            None,
            any_minimized,
            None,
        ),
        items::action(
            "window-show-desktop",
            "Mostrar escritorio",
            None,
            true,
            None,
        ),
        MenuItem::Separator,
    ]);
    if toplevels.is_empty() {
        items.push(items::action(
            "window-none",
            "No hay ventanas abiertas",
            None,
            false,
            None,
        ));
    }
    items.extend(toplevels.iter().map(|window| {
        let name = resolve_app_name(Some(&window.app_id), entries);
        focused::window_item(window, &name, CheckState::None)
    }));
    items
}

fn help_menu_items() -> Vec<MenuItem> {
    let topics = help_topics::topics();
    vec![
        MenuItem::SearchField {
            placeholder: "Buscar".to_string(),
        },
        topics[0].item(Some(MenuIcon::svg(icons::BOOK))),
        topics[1].item(Some(MenuIcon::svg(icons::KEYBOARD))),
        MenuItem::Separator,
        items::action(
            "app-help-report",
            "Informar de un problema…",
            None,
            true,
            None,
        ),
    ]
}

pub fn desktop_menus(entries: &[DesktopEntry], toplevels: &[ToplevelInfo]) -> Vec<Title> {
    [
        ("Escritorio", malaysia_menu_items(toplevels)),
        ("Archivo", file_menu_items()),
        ("Edición", edit_menu_items()),
        ("Visualización", view_menu_items()),
        ("Ir", go_menu_items()),
        ("Ventana", window_menu_items(entries, toplevels)),
        ("Ayuda", help_menu_items()),
    ]
    .into_iter()
    .map(|(label, items)| Title {
        label: label.to_string(),
        items,
        remote_id: None,
        enabled: true,
    })
    .collect()
}

pub fn on_action(id: String) {
    match id.as_str() {
        "malaysia-settings" => {
            let list = Toplevels::get().list.read().clone();
            system::on_system_action("system-settings".into(), &list, &[]);
        }
        "go-home" | "file-new-window" => open_home(),
        "go-desktop" => open_user_dir("DESKTOP", "Desktop"),
        "go-documents" => open_user_dir("DOCUMENTS", "Documents"),
        "go-downloads" => open_user_dir("DOWNLOAD", "Downloads"),
        "go-pictures" => open_user_dir("PICTURES", "Pictures"),
        "go-music" => open_user_dir("MUSIC", "Music"),
        "go-videos" => open_user_dir("VIDEOS", "Videos"),
        "go-applications" | "edit-search" => spawn_component("launcher"),
        "go-trash" => xdg_open("trash:///"),
        "file-new-folder-desktop" => create_desktop_folder(),
        "window-show-desktop" => {
            let toplevels = Toplevels::get();
            for window in toplevels.list.read().iter() {
                toplevels.minimize(window.id);
            }
        }
        _ => focused::on_action(id),
    }
}

fn open_home() {
    if let Some(home) = env::var_os("HOME") {
        xdg_open(home);
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

    fn enabled(items: &[MenuItem], wanted: &str) -> bool {
        items.iter().any(
            |item| matches!(item, MenuItem::Action { label, enabled: true, .. } if label == wanted),
        )
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

    fn toplevel(minimized: bool) -> ToplevelInfo {
        ToplevelInfo {
            app_id: "foot".to_string(),
            minimized,
            ..Default::default()
        }
    }

    #[test]
    fn escritorio_show_and_hide_enabled_states_follow_toplevels() {
        let not_minimized = vec![toplevel(false)];
        assert!(enabled(
            &malaysia_menu_items(&not_minimized),
            "Ocultar otros"
        ));
        assert!(!enabled(
            &malaysia_menu_items(&not_minimized),
            "Mostrar todo"
        ));

        let minimized = vec![toplevel(true)];
        assert!(!enabled(&malaysia_menu_items(&minimized), "Ocultar otros"));
        assert!(enabled(&malaysia_menu_items(&minimized), "Mostrar todo"));
    }

    #[test]
    fn enabled_desktop_items_have_no_unwired_shortcut() {
        const WIRED: &[&str] = &["Super+Space"];
        let toplevels = vec![toplevel(false), toplevel(true)];
        let mut actions = Vec::new();
        for title in desktop_menus(&[], &toplevels) {
            all_actions(&title.items, &mut actions);
        }

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
                let wired = shortcut
                    .as_ref()
                    .is_some_and(|s| WIRED.contains(&s.as_str()));
                assert!(
                    shortcut.is_none() || wired,
                    "enabled item «{label}» shows a shortcut that is not wired globally"
                );
            }
        }
    }

    #[test]
    fn window_front_all_is_enabled_only_when_a_window_is_minimized() {
        let not_minimized = vec![toplevel(false)];
        assert!(!enabled(
            &window_menu_items(&[], &not_minimized),
            "Traer todo al frente"
        ));

        let minimized = vec![toplevel(true)];
        assert!(enabled(
            &window_menu_items(&[], &minimized),
            "Traer todo al frente"
        ));
    }
}
