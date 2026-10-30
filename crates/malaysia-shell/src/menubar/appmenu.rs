//! The desktop menus of the menubar.
//!
//! With no focused window the bar shows the Escritorio menu next to the shared
//! Archivo, Edición, Visualización, Ir, Ventana and Ayuda menus. Each title owns
//! one menu and every action reuses a shell helper that already exists.

/// The menubar title shown when no application is focused.
pub const TITLE: &str = "Escritorio";

use std::env;
use std::path::{Path, PathBuf};

use barrier_kit::menu::{MenuIcon, MenuItem};
use barrier_kit::platform::{ToplevelInfo, Toplevels};

use crate::desktop::DesktopEntry;
use crate::menubar::{
    create_desktop_folder, focused, icons, items, resolve_app_name, spawn_component, system,
    xdg_open, xdg_user_dir,
};

/// The Escritorio menu (about, settings, empty trash, services, hide/show).
pub fn malaysia_menu_items(toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let other_unminimized = toplevels.iter().any(|window| !window.minimized);
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
        // shortcut: no global shortcut is wired yet; the action minimizes every
        // window that is not already minimized.
        items::action(
            "desktop-hide-others",
            "Ocultar otros",
            None,
            other_unminimized,
            None,
        ),
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
        // Unminimize every minimized window. This used to call minimize_all(),
        // which is the opposite of what "Mostrar todo" should do.
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
        id if id.starts_with("app-help-topic:") || id == "app-help-topics-index" => {
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
        all_actions(&malaysia_menu_items(&[]), &mut actions);
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
