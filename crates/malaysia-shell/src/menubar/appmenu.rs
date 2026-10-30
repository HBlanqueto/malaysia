//! The desktop menus of the menubar.
//!
//! With no focused window the bar shows the Escritorio menu next to the shared
//! Archivo, Edición, Visualización, Ir, Ventana and Ayuda menus. Each title owns
//! one menu and every action reuses a shell helper that already exists.

/// The menubar title shown when no application is focused.
pub const TITLE: &str = "Escritorio";

use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use barrier_kit::menu::{CheckState, MenuIcon, MenuItem, SearchLocation, SearchableEntry};
use barrier_kit::platform::{ToplevelInfo, Toplevels};

use crate::desktop::DesktopEntry;
use crate::menubar::{
    create_desktop_folder, focused, help_topics, icons, items, menu::MenuItems, resolve_app_name,
    spawn_component, system, xdg_open, xdg_user_dir,
};

/// The Escritorio menu (about, settings, empty trash, services, hide/show).
pub fn malaysia_menu_items(toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let other_unminimized = toplevels.iter().any(|window| !window.minimized);
    vec![
        // shortcut: falta diálogo Acerca de — mejora: añadir ventana modal.
        items::action(
            "malaysia-about",
            "Acerca de MalaysiaDreams",
            None,
            false,
            None,
        ),
        items::separator(),
        items::action(
            "malaysia-settings",
            "Ajustes…",
            None,
            true,
            Some(MenuIcon::svg(icons::GEAR)),
        ),
        items::separator(),
        // shortcut: falta diálogo de confirmación en la shell.
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
        // shortcut: Super+H no está cableado para el escritorio.
        items::action(
            "desktop-hide",
            "Ocultar Escritorio",
            Some("Super+H"),
            false,
            None,
        ),
        // shortcut: no hay atajo global cableado todavía.
        items::action(
            "desktop-hide-others",
            "Ocultar otros",
            None,
            other_unminimized,
            None,
        ),
        // shortcut: no hay atajo global cableado todavía.
        items::action(
            "desktop-show-all",
            "Mostrar todo",
            None,
            any_minimized,
            None,
        ),
    ]
}

/// The Archivo menu.
pub fn file_menu_items() -> Vec<MenuItem> {
    vec![
        items::action("file-new-window", "Nueva ventana", None, true, None),
        items::action(
            "file-new-folder-desktop",
            "Nueva carpeta en el escritorio",
            None,
            true,
            None,
        ),
        // shortcut: Ctrl+Super+N no está cableado globalmente.
        items::action(
            "file-new-folder-selection",
            "Nueva carpeta con la selección",
            Some("Ctrl+Super+N"),
            false,
            None,
        ),
        // shortcut: Super+T no está cableado globalmente.
        items::action(
            "file-new-tab",
            "Nueva pestaña",
            Some("Super+T"),
            false,
            None,
        ),
        // shortcut: Super+O no está cableado globalmente.
        items::action("file-open", "Abrir", Some("Super+O"), false, None),
        // shortcut: falta selección y submenú de apps asociadas.
        items::submenu(None, "Abrir con", false, None, Vec::new()),
        // shortcut: Super+W no está cableado globalmente.
        items::action(
            "file-close-window",
            "Cerrar ventana",
            Some("Super+W"),
            false,
            None,
        ),
        items::separator(),
        // shortcut: Super+I no está cableado globalmente.
        items::action(
            "file-get-info",
            "Obtener información",
            Some("Super+I"),
            false,
            None,
        ),
        // shortcut: falta selección y diálogo de renombre.
        items::action("file-rename", "Renombrar", None, false, None),
        // shortcut: falta selección y backend de compresión.
        items::action("file-compress", "Comprimir", None, false, None),
        // shortcut: Super+D no está cableado globalmente.
        items::action("file-duplicate", "Duplicar", Some("Super+D"), false, None),
        // shortcut: Ctrl+Super+A no está cableado globalmente.
        items::action(
            "file-make-alias",
            "Crear enlace",
            Some("Ctrl+Super+A"),
            false,
            None,
        ),
        // shortcut: Super+Y no está cableado globalmente.
        items::action(
            "file-quick-look",
            "Vista rápida",
            Some("Super+Y"),
            false,
            None,
        ),
        items::separator(),
        // shortcut: falta backend de compartir.
        items::action(
            "file-share",
            "Compartir…",
            None,
            false,
            Some(MenuIcon::svg(icons::SHARE)),
        ),
        items::separator(),
        // shortcut: Super+BackSpace no está cableado globalmente.
        items::action(
            "file-move-to-trash",
            "Mover a la papelera",
            Some("Super+BackSpace"),
            false,
            Some(MenuIcon::svg(icons::TRASH)),
        ),
        // shortcut: Super+E no está cableado globalmente.
        items::action(
            "file-eject",
            "Expulsar",
            Some("Super+E"),
            false,
            Some(MenuIcon::svg(icons::EJECT)),
        ),
        items::separator(),
        // shortcut: Super+P no está cableado globalmente.
        items::action(
            "file-print",
            "Imprimir",
            Some("Super+P"),
            false,
            Some(MenuIcon::svg(icons::PRINTER)),
        ),
    ]
}

/// The Edición menu.
pub fn edit_menu_items() -> Vec<MenuItem> {
    vec![
        // shortcut: Super+Z no está cableado globalmente.
        items::action("edit-undo", "Deshacer", Some("Super+Z"), false, None),
        // shortcut: Shift+Super+Z no está cableado globalmente.
        items::action("edit-redo", "Rehacer", Some("Shift+Super+Z"), false, None),
        items::separator(),
        // shortcut: Super+X no está cableado globalmente.
        items::action("edit-cut", "Cortar", Some("Super+X"), false, None),
        // shortcut: Super+C no está cableado globalmente.
        items::action("edit-copy", "Copiar", Some("Super+C"), false, None),
        // shortcut: Super+V no está cableado globalmente.
        items::action("edit-paste", "Pegar", Some("Super+V"), false, None),
        // shortcut: Super+A no está cableado globalmente.
        items::action(
            "edit-select-all",
            "Seleccionar todo",
            Some("Super+A"),
            false,
            None,
        ),
        items::separator(),
        // shortcut: falta visor del portapapeles.
        items::action(
            "edit-show-clipboard",
            "Mostrar portapapeles",
            None,
            false,
            None,
        ),
        items::separator(),
        // Super+Space abre el launcher en el compositor.
        items::action(
            "edit-search",
            "Buscar",
            Some("Super+Space"),
            true,
            Some(MenuIcon::svg(icons::SEARCH)),
        ),
        items::separator(),
        // shortcut: falta panel de emojis y símbolos.
        items::action(
            "edit-emoji",
            "Emojis y símbolos",
            None,
            false,
            Some(MenuIcon::svg(icons::EMOJI)),
        ),
    ]
}

/// The Visualización menu.
pub fn view_menu_items() -> Vec<MenuItem> {
    vec![
        // shortcut: Super+1 no está cableado globalmente.
        // The check mark and the icon share the leading column, so this row
        // keeps the mark and omits the icon.
        MenuItem::Action {
            id: "view-as-icons".to_string(),
            label: "Como iconos".to_string(),
            shortcut: Some("Super+1".to_string()),
            enabled: false,
            check: CheckState::Checkbox(true),
            icon: None,
        },
        // shortcut: Super+2 no está cableado globalmente.
        items::action(
            "view-as-list",
            "Como lista",
            Some("Super+2"),
            false,
            Some(MenuIcon::svg(icons::VIEW_LIST)),
        ),
        // shortcut: Super+3 no está cableado globalmente.
        items::action(
            "view-as-columns",
            "Como columnas",
            Some("Super+3"),
            false,
            Some(MenuIcon::svg(icons::VIEW_COLUMNS)),
        ),
        // shortcut: Super+4 no está cableado globalmente.
        items::action(
            "view-as-gallery",
            "Como galería",
            Some("Super+4"),
            false,
            Some(MenuIcon::svg(icons::VIEW_GALLERY)),
        ),
        items::separator(),
        // shortcut: falta señal al dock para mostrar/ocultar.
        items::action(
            "view-toggle-dock",
            "Mostrar/ocultar Dock",
            None,
            false,
            None,
        ),
        items::separator(),
        // shortcut: falta diálogo de fondo de pantalla.
        items::action(
            "view-change-wallpaper",
            "Cambiar fondo de pantalla…",
            None,
            false,
            None,
        ),
        // shortcut: Super+J no está cableado globalmente.
        items::action(
            "view-show-view-options",
            "Mostrar opciones de visualización",
            Some("Super+J"),
            false,
            None,
        ),
        items::separator(),
        // shortcut: Ctrl+Super+F no está cableado globalmente.
        items::action(
            "view-fullscreen",
            "Ver en pantalla completa",
            Some("Ctrl+Super+F"),
            false,
            None,
        ),
    ]
}

/// The Ir menu: the XDG user folders and the trash.
pub fn go_menu_items() -> Vec<MenuItem> {
    vec![
        // shortcut: Super+[ no está cableado globalmente.
        items::action("go-back", "Atrás", Some("Super+["), false, None),
        // shortcut: Super+] no está cableado globalmente.
        items::action("go-forward", "Adelante", Some("Super+]"), false, None),
        // shortcut: Super+Up no está cableado globalmente.
        items::action(
            "go-enclosing-folder",
            "Carpeta contenedora",
            Some("Super+Up"),
            false,
            None,
        ),
        items::separator(),
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
        items::separator(),
        // shortcut: falta historial de carpetas recientes.
        items::submenu(None, "Carpetas recientes", false, None, Vec::new()),
        items::separator(),
        // shortcut: Shift+Super+G no está cableado globalmente.
        items::action(
            "go-to-folder",
            "Ir a la carpeta…",
            Some("Shift+Super+G"),
            false,
            None,
        ),
        // shortcut: Super+K no está cableado globalmente.
        items::action(
            "go-connect-to-server",
            "Conectarse al servidor…",
            Some("Super+K"),
            false,
            None,
        ),
    ]
}

/// The Ventana menu: window controls, layout submenus and the open windows.
pub fn window_menu_items(entries: &[DesktopEntry], toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let mut items = vec![
        // shortcut: Super+M no está cableado globalmente.
        items::action("window-minimize", "Minimizar", Some("Super+M"), false, None),
        // shortcut: falta maximizar la ventana activa.
        items::action("window-maximize", "Maximizar", None, false, None),
        // shortcut: Ctrl+Super+Return no está cableado globalmente.
        items::action(
            "window-fill",
            "Rellenar",
            Some("Ctrl+Super+Return"),
            false,
            None,
        ),
        // shortcut: Ctrl+Super+C no está cableado globalmente.
        items::action(
            "window-center",
            "Centrar",
            Some("Ctrl+Super+C"),
            false,
            None,
        ),
        items::separator(),
        items::submenu(
            None,
            "Trasladar y redimensionar",
            true,
            Some(MenuIcon::svg(icons::GRID)),
            focused::move_resize_items(None, None),
        ),
        // shortcut: falta gestión de pantalla completa.
        items::submenu(
            None,
            "Ventana a pantalla completa",
            false,
            Some(MenuIcon::svg(icons::SPLIT)),
            vec![
                items::action(
                    "window-fullscreen-left",
                    "Izquierda de la pantalla",
                    None,
                    false,
                    None,
                ),
                items::action(
                    "window-fullscreen-right",
                    "Derecha de la pantalla",
                    None,
                    false,
                    None,
                ),
                items::separator(),
                // shortcut: Ctrl+Super+F no está cableado globalmente.
                items::action(
                    "window-fullscreen-toggle",
                    "Activar pantalla completa",
                    Some("Ctrl+Super+F"),
                    false,
                    None,
                ),
            ],
        ),
        items::separator(),
        items::action(
            "window-front-all",
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
        items::separator(),
    ];

    if toplevels.is_empty() {
        items.push(items::action(
            "window-none",
            "No hay ventanas abiertas",
            None,
            false,
            None,
        ));
    } else {
        for (index, window) in toplevels.iter().enumerate() {
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
        MenuItem::SearchField {
            placeholder: "Buscar".to_string(),
        },
        help_topic_item("Documentación de MalaysiaDreams", icons::BOOK),
        help_topic_item("Atajos de teclado", icons::KEYBOARD),
        items::separator(),
        items::action("help-report", "Informar de un problema…", None, true, None),
    ]
}

fn help_topic_item(title: &str, icon: &'static str) -> MenuItem {
    help_topics::topics()
        .iter()
        .find(|topic| topic.title == title)
        .map(|topic| MenuItem::Action {
            id: format!("app-help-topic:{}", topic.url),
            label: topic.title.to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::None,
            icon: Some(MenuIcon::svg(icon)),
        })
        .unwrap_or_else(|| MenuItem::Action {
            id: String::new(),
            label: title.to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
            icon: Some(MenuIcon::svg(icon)),
        })
}

/// Returns true for the help ids that must be delegated to focused::on_action.
pub fn is_help_action(id: &str) -> bool {
    id.starts_with("app-help-topic:") || id == "app-help-topics-index"
}

/// The seven desktop menus in the order they appear in the menubar.
pub fn desktop_menus(
    entries: Rc<Vec<DesktopEntry>>,
    toplevels: Rc<Vec<ToplevelInfo>>,
) -> Vec<(&'static str, MenuItems)> {
    let toplevels_for_title = toplevels.clone();
    vec![
        (
            TITLE,
            Rc::new(move || malaysia_menu_items(&toplevels_for_title)) as MenuItems,
        ),
        ("Archivo", Rc::new(file_menu_items)),
        ("Edición", Rc::new(edit_menu_items)),
        ("Visualización", Rc::new(view_menu_items)),
        ("Ir", Rc::new(go_menu_items)),
        (
            "Ventana",
            Rc::new(move || window_menu_items(&entries, &toplevels)),
        ),
        ("Ayuda", Rc::new(help_menu_items)),
    ]
}

/// Returns the index of the title that owns the Help search field in the
/// desktop menu list. Used by bar.rs so the wiring is not hidden in a label
/// comparison that can drift if the menu order changes.
pub fn desktop_help_title_index(labels: &[&str]) -> Option<usize> {
    labels.iter().position(|&label| label == "Ayuda")
}

/// All menu entries that the Help search field can reach from the desktop
/// menus, paired with their menu path and menubar location. The Ayuda menu
/// itself is excluded because its topics come from `focused::help_search_topics`.
pub fn searchable_menu_items(
    entries: Rc<Vec<DesktopEntry>>,
    toplevels: Rc<Vec<ToplevelInfo>>,
) -> Vec<SearchableEntry> {
    let mut out = Vec::new();
    let menus = desktop_menus(entries, toplevels);
    let labels: Vec<&str> = menus.iter().map(|(label, _)| *label).collect();
    let help_index = desktop_help_title_index(&labels);
    for (title_index, (title, items)) in menus.into_iter().enumerate() {
        if Some(title_index) == help_index {
            continue;
        }
        focused::collect_searchable(
            &items(),
            title,
            SearchLocation {
                title_index,
                indices: Vec::new(),
            },
            &mut out,
        );
    }
    out
}

/// Handles an action from any of the desktop menus.
pub fn on_action(id: String) {
    let toplevels = Toplevels::get();
    let list = toplevels.list.read().clone();
    // Settings is handled by the existing system helper so it activates an
    // existing settings window or launches the real malaysia-settings binary.
    if id == "malaysia-settings" {
        system::on_system_action("system-settings".into(), &list);
        return;
    }
    on_action_with(
        id,
        &list,
        |index| toplevels.minimize(index),
        |index| toplevels.unset_minimized(index),
        |index| toplevels.activate(index),
        |binary, _args| spawn_component(binary),
        |path| xdg_open(path),
        focused::on_action,
    );
}

#[allow(clippy::too_many_arguments)]
/// Injected version of [`on_action`]: decisions are pure and rely only on the
/// supplied `toplevels` list and the caller-provided callbacks.
pub fn on_action_with(
    id: String,
    toplevels: &[ToplevelInfo],
    mut minimize: impl FnMut(usize),
    mut unminimize: impl FnMut(usize),
    mut activate: impl FnMut(usize),
    mut spawn: impl FnMut(&str, &[&str]),
    mut open: impl FnMut(&Path),
    mut delegate: impl FnMut(String),
) {
    match id.as_str() {
        "go-home" => open_home(&mut open),
        "go-desktop" => open_user_dir("DESKTOP", "Desktop", &mut open),
        "go-documents" => open_user_dir("DOCUMENTS", "Documents", &mut open),
        "go-downloads" => open_user_dir("DOWNLOAD", "Downloads", &mut open),
        "go-pictures" => open_user_dir("PICTURES", "Pictures", &mut open),
        "go-music" => open_user_dir("MUSIC", "Music", &mut open),
        "go-videos" => open_user_dir("VIDEOS", "Videos", &mut open),
        "go-applications" => spawn("launcher", &[]),
        "go-trash" => open(Path::new("trash:///")),
        "file-new-window" => open_home(&mut open),
        "file-search" | "edit-search" => spawn("launcher", &[]),
        "file-new-folder-desktop" => create_desktop_folder(),
        // Minimize every window that is not already minimized. Enabled only
        // when at least one window is in that state.
        "desktop-hide-others" => {
            for (index, window) in toplevels.iter().enumerate() {
                if !window.minimized {
                    minimize(index);
                }
            }
        }
        // Unminimize every minimized window.
        "desktop-show-all" | "window-front-all" => {
            for (index, window) in toplevels.iter().enumerate() {
                if window.minimized {
                    unminimize(index);
                }
            }
        }
        "window-minimize-all" | "window-show-desktop" => {
            for index in 0..toplevels.len() {
                minimize(index);
            }
        }
        "help-report" => open(Path::new("https://github.com/HBlanqueto/malaysia/issues")),
        id if id.starts_with("window-activate-") => {
            if let Ok(index) = id["window-activate-".len()..].parse::<usize>() {
                activate(index);
            }
        }
        id if is_help_action(id) => {
            delegate(id.to_string());
        }
        _ => {}
    }
}

fn open_home(open: &mut impl FnMut(&Path)) {
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        open(&home);
    }
}

fn open_user_dir(kind: &str, fallback: &str, open: &mut impl FnMut(&Path)) {
    if let Some(path) = xdg_user_dir(kind)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
    {
        open(&path);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use barrier_kit::menu::{MenuItem, SearchSource};

    fn label(item: &MenuItem) -> Option<&str> {
        match item {
            MenuItem::Action { label, .. }
            | MenuItem::Submenu { label, .. }
            | MenuItem::Header { label, .. } => Some(label),
            _ => None,
        }
    }

    fn labels_with_separators(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                MenuItem::Action { label, .. }
                | MenuItem::Submenu { label, .. }
                | MenuItem::Header { label, .. } => label.clone(),
                MenuItem::SearchField { placeholder } => placeholder.clone(),
                MenuItem::Separator => "—".to_string(),
                _ => String::new(),
            })
            .collect()
    }

    fn top_level_icon_labels(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { label, icon, .. } | MenuItem::Submenu { label, icon, .. } => {
                    icon.is_some().then(|| label.clone())
                }
                _ => None,
            })
            .collect()
    }

    fn is_enabled(item: &MenuItem) -> bool {
        match item {
            MenuItem::Action { enabled, .. } | MenuItem::Submenu { enabled, .. } => *enabled,
            _ => false,
        }
    }

    fn enabled(items: &[MenuItem], wanted: &str) -> bool {
        items
            .iter()
            .find(|item| label(item) == Some(wanted))
            .map(is_enabled)
            .unwrap_or(false)
    }

    fn action_id(item: &MenuItem) -> Option<&str> {
        match item {
            MenuItem::Action { id, .. } => Some(id),
            _ => None,
        }
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
            title: String::new(),
            app_id: "foot".to_string(),
            active: false,
            maximized: false,
            minimized,
            restorable: false,
            app_menu: None,
            gtk_menus: None,
        }
    }

    #[test]
    fn desktop_title_is_escritorio() {
        assert_eq!(TITLE, "Escritorio");
    }

    #[test]
    fn escritorio_menu_labels_and_order() {
        assert_eq!(
            labels_with_separators(&malaysia_menu_items(&[])),
            vec![
                "Acerca de MalaysiaDreams",
                "—",
                "Ajustes…",
                "—",
                "Vaciar papelera…",
                "—",
                "Servicios",
                "—",
                "Ocultar Escritorio",
                "Ocultar otros",
                "Mostrar todo",
            ]
        );
    }

    #[test]
    fn escritorio_settings_is_enabled_and_has_no_shortcut() {
        let items = malaysia_menu_items(&[]);
        let settings = items
            .iter()
            .find(|item| label(item) == Some("Ajustes…"))
            .expect("Ajustes… must exist");
        assert!(is_enabled(settings));
        assert!(
            matches!(settings, MenuItem::Action { shortcut: None, .. }),
            "Ajustes… must not show a shortcut"
        );
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
    fn archivo_menu_labels_and_order() {
        assert_eq!(
            labels_with_separators(&file_menu_items()),
            vec![
                "Nueva ventana",
                "Nueva carpeta en el escritorio",
                "Nueva carpeta con la selección",
                "Nueva pestaña",
                "Abrir",
                "Abrir con",
                "Cerrar ventana",
                "—",
                "Obtener información",
                "Renombrar",
                "Comprimir",
                "Duplicar",
                "Crear enlace",
                "Vista rápida",
                "—",
                "Compartir…",
                "—",
                "Mover a la papelera",
                "Expulsar",
                "—",
                "Imprimir",
            ]
        );
    }

    #[test]
    fn edicion_menu_labels_and_order() {
        assert_eq!(
            labels_with_separators(&edit_menu_items()),
            vec![
                "Deshacer",
                "Rehacer",
                "—",
                "Cortar",
                "Copiar",
                "Pegar",
                "Seleccionar todo",
                "—",
                "Mostrar portapapeles",
                "—",
                "Buscar",
                "—",
                "Emojis y símbolos",
            ]
        );
    }

    #[test]
    fn visualizacion_menu_labels_and_order() {
        assert_eq!(
            labels_with_separators(&view_menu_items()),
            vec![
                "Como iconos",
                "Como lista",
                "Como columnas",
                "Como galería",
                "—",
                "Mostrar/ocultar Dock",
                "—",
                "Cambiar fondo de pantalla…",
                "Mostrar opciones de visualización",
                "—",
                "Ver en pantalla completa",
            ]
        );
    }

    #[test]
    fn ir_menu_labels_and_order() {
        assert_eq!(
            labels_with_separators(&go_menu_items()),
            vec![
                "Atrás",
                "Adelante",
                "Carpeta contenedora",
                "—",
                "Documentos",
                "Escritorio",
                "Descargas",
                "Inicio",
                "Imágenes",
                "Música",
                "Vídeos",
                "Aplicaciones",
                "Papelera",
                "—",
                "Carpetas recientes",
                "—",
                "Ir a la carpeta…",
                "Conectarse al servidor…",
            ]
        );
    }

    #[test]
    fn ventana_menu_labels_and_order_when_no_windows() {
        assert_eq!(
            labels_with_separators(&window_menu_items(&[], &[])),
            vec![
                "Minimizar",
                "Maximizar",
                "Rellenar",
                "Centrar",
                "—",
                "Trasladar y redimensionar",
                "Ventana a pantalla completa",
                "—",
                "Traer todo al frente",
                "Mostrar escritorio",
                "—",
                "No hay ventanas abiertas",
            ]
        );
    }

    #[test]
    fn ayuda_menu_labels_and_order() {
        assert_eq!(
            labels_with_separators(&help_menu_items()),
            vec![
                "Buscar",
                "Documentación de MalaysiaDreams",
                "Atajos de teclado",
                "—",
                "Informar de un problema…",
            ]
        );
    }

    #[test]
    fn only_the_expected_rows_have_icons() {
        assert_eq!(
            top_level_icon_labels(&malaysia_menu_items(&[])),
            vec!["Ajustes…", "Vaciar papelera…"]
        );
        assert_eq!(
            top_level_icon_labels(&file_menu_items()),
            vec!["Compartir…", "Mover a la papelera", "Expulsar", "Imprimir",]
        );
        assert_eq!(
            top_level_icon_labels(&edit_menu_items()),
            vec!["Buscar", "Emojis y símbolos"]
        );
        assert_eq!(
            top_level_icon_labels(&view_menu_items()),
            vec!["Como lista", "Como columnas", "Como galería"]
        );
        assert_eq!(
            top_level_icon_labels(&go_menu_items()),
            vec![
                "Documentos",
                "Escritorio",
                "Descargas",
                "Inicio",
                "Imágenes",
                "Música",
                "Vídeos",
                "Aplicaciones",
                "Papelera",
            ]
        );
        assert_eq!(
            top_level_icon_labels(&window_menu_items(&[], &[])),
            vec!["Trasladar y redimensionar", "Ventana a pantalla completa"]
        );
        assert_eq!(
            top_level_icon_labels(&help_menu_items()),
            vec!["Documentación de MalaysiaDreams", "Atajos de teclado"]
        );
    }

    #[test]
    fn enabled_desktop_items_have_no_unwired_shortcut() {
        const WIRED: &[&str] = &["Super+Space"];
        let toplevels = vec![toplevel(false), toplevel(true)];
        let mut actions = Vec::new();
        all_actions(&malaysia_menu_items(&toplevels), &mut actions);
        all_actions(&file_menu_items(), &mut actions);
        all_actions(&edit_menu_items(), &mut actions);
        all_actions(&view_menu_items(), &mut actions);
        all_actions(&go_menu_items(), &mut actions);
        all_actions(&window_menu_items(&[], &toplevels), &mut actions);
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
    fn desktop_show_all_unminimizes_minimized_windows() {
        let toplevels = vec![
            ToplevelInfo {
                title: "A".into(),
                app_id: "a".into(),
                active: true,
                maximized: false,
                minimized: false,
                restorable: false,
                app_menu: None,
                gtk_menus: None,
            },
            ToplevelInfo {
                title: "B".into(),
                app_id: "b".into(),
                active: false,
                maximized: false,
                minimized: true,
                restorable: false,
                app_menu: None,
                gtk_menus: None,
            },
        ];
        let minimized = Rc::new(RefCell::new(Vec::<usize>::new()));
        let unminimized = Rc::new(RefCell::new(Vec::<usize>::new()));
        let min = minimized.clone();
        let unmin = unminimized.clone();
        on_action_with(
            "desktop-show-all".to_string(),
            &toplevels,
            move |index| min.borrow_mut().push(index),
            move |index| unmin.borrow_mut().push(index),
            |_| {},
            |_, _| {},
            |_| {},
            |_| {},
        );
        assert_eq!(*unminimized.borrow(), vec![1]);
        assert!(
            minimized.borrow().is_empty(),
            "desktop-show-all must not minimize windows"
        );
    }

    #[test]
    fn desktop_show_all_does_nothing_when_no_window_is_minimized() {
        let toplevels = vec![ToplevelInfo {
            title: "A".into(),
            app_id: "a".into(),
            active: true,
            maximized: false,
            minimized: false,
            restorable: false,
            app_menu: None,
            gtk_menus: None,
        }];
        on_action_with(
            "desktop-show-all".to_string(),
            &toplevels,
            |_| panic!("must not minimize"),
            |_| panic!("must not unminimize"),
            |_| panic!("must not activate"),
            |_, _| panic!("must not spawn"),
            |_| panic!("must not open"),
            |_| panic!("must not delegate"),
        );
    }

    #[test]
    fn window_front_all_unminimizes_minimized_windows() {
        let toplevels = vec![
            ToplevelInfo {
                title: "A".into(),
                app_id: "a".into(),
                active: true,
                maximized: false,
                minimized: false,
                restorable: false,
                app_menu: None,
                gtk_menus: None,
            },
            ToplevelInfo {
                title: "B".into(),
                app_id: "b".into(),
                active: false,
                maximized: false,
                minimized: true,
                restorable: false,
                app_menu: None,
                gtk_menus: None,
            },
        ];
        let unminimized = Rc::new(RefCell::new(Vec::<usize>::new()));
        let unmin = unminimized.clone();
        on_action_with(
            "window-front-all".to_string(),
            &toplevels,
            |_| {},
            move |index| unmin.borrow_mut().push(index),
            |_| {},
            |_, _| {},
            |_| {},
            |_| {},
        );
        assert_eq!(*unminimized.borrow(), vec![1]);
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

    #[test]
    fn on_action_with_ignores_malaysia_settings() {
        on_action_with(
            "malaysia-settings".to_string(),
            &[],
            |_| panic!("must not minimize"),
            |_| panic!("must not unminimize"),
            |_| panic!("must not activate"),
            |_, _| panic!("must not spawn"),
            |_| panic!("must not open"),
            |_| panic!("must not delegate"),
        );
    }

    #[test]
    fn edit_search_and_go_applications_spawn_the_launcher() {
        for id in ["edit-search", "go-applications"] {
            let spawned = Rc::new(RefCell::new(String::new()));
            let s = spawned.clone();
            on_action_with(
                id.to_string(),
                &[],
                |_| {},
                |_| {},
                |_| {},
                move |binary, _args| *s.borrow_mut() = binary.to_string(),
                |_| {},
                |_| {},
            );
            assert_eq!(*spawned.borrow(), "launcher", "{id} should spawn launcher");
        }
    }

    #[test]
    fn help_topics_use_their_documentation_urls() {
        let items = help_menu_items();
        let docs = items
            .iter()
            .find(|item| label(item) == Some("Documentación de MalaysiaDreams"))
            .expect("missing docs item");
        let shortcuts = items
            .iter()
            .find(|item| label(item) == Some("Atajos de teclado"))
            .expect("missing shortcuts item");

        assert!(is_enabled(docs));
        assert!(is_enabled(shortcuts));

        let docs_url = help_topics::topics()
            .iter()
            .find(|topic| topic.title == "Documentación de MalaysiaDreams")
            .map(|topic| topic.url)
            .expect("docs topic missing");
        let shortcuts_url = help_topics::topics()
            .iter()
            .find(|topic| topic.title == "Atajos de teclado")
            .map(|topic| topic.url)
            .expect("shortcuts topic missing");

        let docs_id = action_id(docs).expect("docs must be an action");
        let shortcuts_id = action_id(shortcuts).expect("shortcuts must be an action");
        assert_eq!(docs_id, format!("app-help-topic:{docs_url}"));
        assert_eq!(shortcuts_id, format!("app-help-topic:{shortcuts_url}"));
    }

    fn find_search<'a>(
        entries: &'a [SearchableEntry],
        wanted_label: &str,
    ) -> Option<&'a SearchableEntry> {
        entries
            .iter()
            .find(|entry| label(&entry.item) == Some(wanted_label))
    }

    #[test]
    fn search_finds_desktop_menu_entries_with_title_indices() {
        let toplevels = vec![toplevel(false), toplevel(true)];
        let entries = searchable_menu_items(Rc::new(vec![]), Rc::new(toplevels));

        let settings = find_search(&entries, "Ajustes…").expect("Ajustes missing");
        assert_eq!(settings.path, "Escritorio");
        assert_eq!(settings.location.as_ref().unwrap().title_index, 0);

        let new_window = find_search(&entries, "Nueva ventana").expect("Nueva ventana missing");
        assert_eq!(new_window.path, "Archivo");
        assert_eq!(new_window.location.as_ref().unwrap().title_index, 1);

        let undo = find_search(&entries, "Deshacer").expect("Deshacer missing");
        assert_eq!(undo.path, "Edición");
        assert_eq!(undo.location.as_ref().unwrap().title_index, 2);

        let as_icons = find_search(&entries, "Como iconos").expect("Como iconos missing");
        assert_eq!(as_icons.path, "Visualización");
        assert_eq!(as_icons.location.as_ref().unwrap().title_index, 3);

        let documents = find_search(&entries, "Documentos").expect("Documentos missing");
        assert_eq!(documents.path, "Ir");
        assert_eq!(documents.location.as_ref().unwrap().title_index, 4);

        let minimize = find_search(&entries, "Minimizar").expect("Minimizar missing");
        assert_eq!(minimize.path, "Ventana");
        assert_eq!(minimize.location.as_ref().unwrap().title_index, 5);

        let show_desktop =
            find_search(&entries, "Mostrar escritorio").expect("Mostrar escritorio missing");
        assert_eq!(show_desktop.path, "Ventana");
        assert_eq!(show_desktop.location.as_ref().unwrap().title_index, 5);

        let left = find_search(&entries, "Izquierda").expect("Izquierda missing");
        assert_eq!(left.path, "Ventana › Trasladar y redimensionar");
        assert!(!left.location.as_ref().unwrap().indices.is_empty());

        assert!(
            entries.iter().all(|entry| {
                action_id(&entry.item)
                    .map(|id| !id.starts_with("app-help-topic:"))
                    .unwrap_or(true)
            }),
            "Ayuda menu must not contribute app-help-topic entries to menu search"
        );
    }

    #[test]
    fn search_includes_help_topics() {
        let topics = focused::help_search_topics();
        let docs = topics
            .iter()
            .find(|entry| label(&entry.item) == Some("Documentación de MalaysiaDreams"))
            .expect("docs topic missing");
        assert_eq!(docs.path, "Temas de ayuda");
        assert!(matches!(docs.source, SearchSource::Help));
        assert!(docs.location.is_none());
    }

    #[test]
    fn is_help_action_recognizes_help_ids() {
        assert!(is_help_action("app-help-topic:https://example.com"));
        assert!(is_help_action("app-help-topics-index"));
        assert!(!is_help_action("help-report"));
        assert!(!is_help_action("malaysia-settings"));
    }

    #[test]
    fn desktop_help_title_index_points_to_ayuda() {
        let labels: Vec<&str> = desktop_menus(Rc::new(vec![]), Rc::new(vec![]))
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        assert_eq!(desktop_help_title_index(&labels), Some(6));
    }
}
