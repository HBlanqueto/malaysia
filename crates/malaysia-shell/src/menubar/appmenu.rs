use std::env;
use std::path::PathBuf;

use barrier_kit::menu::{CheckState, MenuIcon, MenuItem};
use barrier_kit::shell::{ToplevelInfo, Toplevels};

use crate::desktop::DesktopEntry;
use crate::menubar::remote::Title;
use crate::menubar::{
    create_desktop_folder, focused, help_topics, icons, resolve_app_name, system, xdg_user_dir,
};
use crate::spawn::{spawn_component, xdg_open};

fn malaysia_menu_items(toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let other_unminimized = toplevels.iter().any(|window| !window.minimized);
    vec![
        MenuItem::action("malaysia-about", "Acerca de Malaysia").with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("malaysia-settings", "Ajustes…").with_icon(MenuIcon::svg(icons::GEAR)),
        MenuItem::Separator,
        MenuItem::action("malaysia-empty-trash", "Vaciar papelera…")
            .with_shortcut("Shift+Super+BackSpace")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::TRASH)),
        MenuItem::Separator,
        focused::services_submenu(),
        MenuItem::Separator,
        MenuItem::action("desktop-hide", "Ocultar Escritorio")
            .with_shortcut("Super+H")
            .with_enabled(false),
        MenuItem::action("app-hide-others", "Ocultar otros").with_enabled(other_unminimized),
        MenuItem::action("app-show-all", "Mostrar todo").with_enabled(any_minimized),
    ]
}

fn file_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("file-new-window", "Nueva ventana"),
        MenuItem::action("file-new-folder-desktop", "Nueva carpeta en el escritorio"),
        MenuItem::action(
            "file-new-folder-selection",
            "Nueva carpeta con la selección",
        )
        .with_shortcut("Ctrl+Super+N")
        .with_enabled(false),
        MenuItem::action("file-new-tab", "Nueva pestaña")
            .with_shortcut("Super+T")
            .with_enabled(false),
        MenuItem::action("file-open", "Abrir")
            .with_shortcut("Super+O")
            .with_enabled(false),
        MenuItem::submenu("Abrir con", Vec::new()).with_enabled(false),
        MenuItem::action("file-close-window", "Cerrar ventana")
            .with_shortcut("Super+W")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("file-get-info", "Obtener información")
            .with_shortcut("Super+I")
            .with_enabled(false),
        MenuItem::action("file-rename", "Renombrar").with_enabled(false),
        MenuItem::action("file-compress", "Comprimir").with_enabled(false),
        MenuItem::action("file-duplicate", "Duplicar")
            .with_shortcut("Super+D")
            .with_enabled(false),
        MenuItem::action("file-make-alias", "Crear enlace")
            .with_shortcut("Ctrl+Super+A")
            .with_enabled(false),
        MenuItem::action("file-quick-look", "Vista rápida")
            .with_shortcut("Super+Y")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("file-share", "Compartir…")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::SHARE)),
        MenuItem::Separator,
        MenuItem::action("file-move-to-trash", "Mover a la papelera")
            .with_shortcut("Super+BackSpace")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::TRASH)),
        MenuItem::action("file-eject", "Expulsar")
            .with_shortcut("Super+E")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::EJECT)),
        MenuItem::Separator,
        MenuItem::action("file-print", "Imprimir")
            .with_shortcut("Super+P")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::PRINTER)),
    ]
}

fn edit_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("edit-undo", "Deshacer")
            .with_shortcut("Super+Z")
            .with_enabled(false),
        MenuItem::action("edit-redo", "Rehacer")
            .with_shortcut("Shift+Super+Z")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("edit-cut", "Cortar")
            .with_shortcut("Super+X")
            .with_enabled(false),
        MenuItem::action("edit-copy", "Copiar")
            .with_shortcut("Super+C")
            .with_enabled(false),
        MenuItem::action("edit-paste", "Pegar")
            .with_shortcut("Super+V")
            .with_enabled(false),
        MenuItem::action("edit-select-all", "Seleccionar todo")
            .with_shortcut("Super+A")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("edit-show-clipboard", "Mostrar portapapeles").with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("edit-search", "Buscar")
            .with_shortcut("Super+Space")
            .with_icon(MenuIcon::svg(icons::SEARCH)),
        MenuItem::Separator,
        MenuItem::action("edit-emoji", "Emojis y símbolos")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::EMOJI)),
    ]
}

fn view_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("view-as-icons", "Como iconos")
            .with_shortcut("Super+1")
            .with_enabled(false)
            .with_check(CheckState::Checkbox(true)),
        MenuItem::action("view-as-list", "Como lista")
            .with_shortcut("Super+2")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::VIEW_LIST)),
        MenuItem::action("view-as-columns", "Como columnas")
            .with_shortcut("Super+3")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::VIEW_COLUMNS)),
        MenuItem::action("view-as-gallery", "Como galería")
            .with_shortcut("Super+4")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::VIEW_GALLERY)),
        MenuItem::Separator,
        MenuItem::action("view-toggle-dock", "Mostrar/ocultar Dock").with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("view-change-wallpaper", "Cambiar fondo de pantalla…").with_enabled(false),
        MenuItem::action(
            "view-show-view-options",
            "Mostrar opciones de visualización",
        )
        .with_shortcut("Super+J")
        .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("view-fullscreen", "Ver en pantalla completa")
            .with_shortcut("Ctrl+Super+F")
            .with_enabled(false),
    ]
}

fn go_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("go-back", "Atrás")
            .with_shortcut("Super+[")
            .with_enabled(false),
        MenuItem::action("go-forward", "Adelante")
            .with_shortcut("Super+]")
            .with_enabled(false),
        MenuItem::action("go-enclosing-folder", "Carpeta contenedora")
            .with_shortcut("Super+Up")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("go-documents", "Documentos").with_icon(MenuIcon::svg(icons::DOCUMENTS)),
        MenuItem::action("go-desktop", "Escritorio").with_icon(MenuIcon::svg(icons::DESKTOP)),
        MenuItem::action("go-downloads", "Descargas").with_icon(MenuIcon::svg(icons::DOWNLOADS)),
        MenuItem::action("go-home", "Inicio").with_icon(MenuIcon::svg(icons::HOUSE)),
        MenuItem::action("go-pictures", "Imágenes").with_icon(MenuIcon::svg(icons::PICTURES)),
        MenuItem::action("go-music", "Música").with_icon(MenuIcon::svg(icons::MUSIC)),
        MenuItem::action("go-videos", "Vídeos").with_icon(MenuIcon::svg(icons::VIDEOS)),
        MenuItem::action("go-applications", "Aplicaciones").with_icon(MenuIcon::svg(icons::APPS)),
        MenuItem::action("go-trash", "Papelera").with_icon(MenuIcon::svg(icons::TRASH)),
        MenuItem::Separator,
        MenuItem::submenu("Carpetas recientes", Vec::new()).with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("go-to-folder", "Ir a la carpeta…")
            .with_shortcut("Shift+Super+G")
            .with_enabled(false),
        MenuItem::action("go-connect-to-server", "Conectarse al servidor…")
            .with_shortcut("Super+K")
            .with_enabled(false),
    ]
}

fn window_menu_items(entries: &[DesktopEntry], toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let mut items = focused::window_controls(None);
    items.extend([
        MenuItem::Separator,
        MenuItem::action("app-show-all", "Traer todo al frente").with_enabled(any_minimized),
        MenuItem::action("window-show-desktop", "Mostrar escritorio"),
        MenuItem::Separator,
    ]);
    if toplevels.is_empty() {
        items.push(MenuItem::action("window-none", "No hay ventanas abiertas").with_enabled(false));
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
        MenuItem::action("app-help-report", "Informar de un problema…"),
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
