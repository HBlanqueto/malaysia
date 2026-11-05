use std::env;
use std::path::PathBuf;

use barrier_kit::menu::{CheckState, MenuIcon, MenuItem};
use barrier_kit::shell::{ToplevelInfo, Toplevels};

use crate::desktop::DesktopEntry;
use crate::localize::tr;
use crate::menubar::remote::Title;
use crate::menubar::{
    create_desktop_folder, focused, help_topics, icons, resolve_app_name, system,
};
use crate::spawn::{spawn_component, xdg_open};

fn malaysia_menu_items(toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let other_unminimized = toplevels.iter().any(|window| !window.minimized);
    vec![
        MenuItem::action("malaysia-about", tr!("desktop-about")).with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("malaysia-settings", tr!("menu-settings"))
            .with_icon(MenuIcon::svg(icons::GEAR)),
        MenuItem::Separator,
        MenuItem::action("malaysia-empty-trash", tr!("desktop-empty-trash"))
            .with_shortcut("Shift+Super+BackSpace")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::TRASH)),
        MenuItem::Separator,
        focused::services_submenu(),
        MenuItem::Separator,
        MenuItem::action("desktop-hide", tr!("desktop-hide"))
            .with_shortcut("Super+H")
            .with_enabled(false),
        MenuItem::action("app-hide-others", tr!("menu-hide-others"))
            .with_enabled(other_unminimized),
        MenuItem::action("app-show-all", tr!("menu-show-all")).with_enabled(any_minimized),
    ]
}

fn file_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("file-new-window", tr!("file-new-window")),
        MenuItem::action("file-new-folder-desktop", tr!("file-new-folder-desktop")),
        MenuItem::action(
            "file-new-folder-selection",
            tr!("file-new-folder-selection"),
        )
        .with_shortcut("Ctrl+Super+N")
        .with_enabled(false),
        MenuItem::action("file-new-tab", tr!("file-new-tab"))
            .with_shortcut("Super+T")
            .with_enabled(false),
        MenuItem::action("file-open", tr!("file-open"))
            .with_shortcut("Super+O")
            .with_enabled(false),
        MenuItem::submenu(tr!("file-open-with"), Vec::new()).with_enabled(false),
        MenuItem::action("file-close-window", tr!("menu-close-window"))
            .with_shortcut("Super+W")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("file-get-info", tr!("file-get-info"))
            .with_shortcut("Super+I")
            .with_enabled(false),
        MenuItem::action("file-rename", tr!("file-rename")).with_enabled(false),
        MenuItem::action("file-compress", tr!("file-compress")).with_enabled(false),
        MenuItem::action("file-duplicate", tr!("file-duplicate"))
            .with_shortcut("Super+D")
            .with_enabled(false),
        MenuItem::action("file-make-alias", tr!("file-make-alias"))
            .with_shortcut("Ctrl+Super+A")
            .with_enabled(false),
        MenuItem::action("file-quick-look", tr!("file-quick-look"))
            .with_shortcut("Super+Y")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("file-share", tr!("file-share"))
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::SHARE)),
        MenuItem::Separator,
        MenuItem::action("file-move-to-trash", tr!("file-move-to-trash"))
            .with_shortcut("Super+BackSpace")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::TRASH)),
        MenuItem::action("file-eject", tr!("file-eject"))
            .with_shortcut("Super+E")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::EJECT)),
        MenuItem::Separator,
        MenuItem::action("file-print", tr!("file-print"))
            .with_shortcut("Super+P")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::PRINTER)),
    ]
}

fn edit_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("edit-undo", tr!("edit-undo"))
            .with_shortcut("Super+Z")
            .with_enabled(false),
        MenuItem::action("edit-redo", tr!("edit-redo"))
            .with_shortcut("Shift+Super+Z")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("edit-cut", tr!("edit-cut"))
            .with_shortcut("Super+X")
            .with_enabled(false),
        MenuItem::action("edit-copy", tr!("edit-copy"))
            .with_shortcut("Super+C")
            .with_enabled(false),
        MenuItem::action("edit-paste", tr!("edit-paste"))
            .with_shortcut("Super+V")
            .with_enabled(false),
        MenuItem::action("edit-select-all", tr!("edit-select-all"))
            .with_shortcut("Super+A")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("edit-show-clipboard", tr!("edit-show-clipboard")).with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("edit-search", tr!("menu-search"))
            .with_shortcut("Super+Space")
            .with_icon(MenuIcon::svg(icons::SEARCH)),
        MenuItem::Separator,
        MenuItem::action("edit-emoji", tr!("edit-emoji"))
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::EMOJI)),
    ]
}

fn view_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("view-as-icons", tr!("view-as-icons"))
            .with_shortcut("Super+1")
            .with_enabled(false)
            .with_check(CheckState::Checkbox(true)),
        MenuItem::action("view-as-list", tr!("view-as-list"))
            .with_shortcut("Super+2")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::VIEW_LIST)),
        MenuItem::action("view-as-columns", tr!("view-as-columns"))
            .with_shortcut("Super+3")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::VIEW_COLUMNS)),
        MenuItem::action("view-as-gallery", tr!("view-as-gallery"))
            .with_shortcut("Super+4")
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::VIEW_GALLERY)),
        MenuItem::Separator,
        MenuItem::action("view-toggle-dock", tr!("view-toggle-dock")).with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("view-change-wallpaper", tr!("view-change-wallpaper")).with_enabled(false),
        MenuItem::action("view-show-view-options", tr!("view-show-view-options"))
            .with_shortcut("Super+J")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("view-fullscreen", tr!("view-fullscreen"))
            .with_shortcut("Ctrl+Super+F")
            .with_enabled(false),
    ]
}

fn go_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("go-back", tr!("go-back"))
            .with_shortcut("Super+[")
            .with_enabled(false),
        MenuItem::action("go-forward", tr!("go-forward"))
            .with_shortcut("Super+]")
            .with_enabled(false),
        MenuItem::action("go-enclosing-folder", tr!("go-enclosing-folder"))
            .with_shortcut("Super+Up")
            .with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("go-documents", tr!("go-documents"))
            .with_icon(MenuIcon::svg(icons::DOCUMENTS)),
        MenuItem::action("go-desktop", tr!("go-desktop")).with_icon(MenuIcon::svg(icons::DESKTOP)),
        MenuItem::action("go-downloads", tr!("go-downloads"))
            .with_icon(MenuIcon::svg(icons::DOWNLOADS)),
        MenuItem::action("go-home", tr!("go-home")).with_icon(MenuIcon::svg(icons::HOUSE)),
        MenuItem::action("go-pictures", tr!("go-pictures"))
            .with_icon(MenuIcon::svg(icons::PICTURES)),
        MenuItem::action("go-music", tr!("go-music")).with_icon(MenuIcon::svg(icons::MUSIC)),
        MenuItem::action("go-videos", tr!("go-videos")).with_icon(MenuIcon::svg(icons::VIDEOS)),
        MenuItem::action("go-applications", tr!("go-applications"))
            .with_icon(MenuIcon::svg(icons::APPS)),
        MenuItem::action("go-trash", tr!("go-trash")).with_icon(MenuIcon::svg(icons::TRASH)),
        MenuItem::Separator,
        MenuItem::submenu(tr!("go-recent-folders"), Vec::new()).with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("go-to-folder", tr!("go-to-folder"))
            .with_shortcut("Shift+Super+G")
            .with_enabled(false),
        MenuItem::action("go-connect-to-server", tr!("go-connect-to-server"))
            .with_shortcut("Super+K")
            .with_enabled(false),
    ]
}

fn window_menu_items(entries: &[DesktopEntry], toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    let mut items = focused::window_controls(None);
    items.extend([
        MenuItem::Separator,
        MenuItem::action("app-show-all", tr!("menu-bring-all-to-front"))
            .with_enabled(any_minimized),
        MenuItem::action("window-show-desktop", tr!("window-show-desktop")),
        MenuItem::Separator,
    ]);
    if toplevels.is_empty() {
        items.push(MenuItem::action("window-none", tr!("window-none")).with_enabled(false));
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
            placeholder: tr!("menu-search").to_string(),
        },
        topics[0].item(Some(MenuIcon::svg(icons::BOOK))),
        topics[1].item(Some(MenuIcon::svg(icons::KEYBOARD))),
        MenuItem::Separator,
        MenuItem::action("app-help-report", tr!("menu-report-issue")),
    ]
}

pub fn desktop_menus(entries: &[DesktopEntry], toplevels: &[ToplevelInfo]) -> Vec<Title> {
    [
        (tr!("desktop"), malaysia_menu_items(toplevels)),
        (tr!("menu-file"), file_menu_items()),
        (tr!("edit"), edit_menu_items()),
        (tr!("view"), view_menu_items()),
        (tr!("go"), go_menu_items()),
        (tr!("menu-window"), window_menu_items(entries, toplevels)),
        (tr!("menu-help"), help_menu_items()),
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
    if let Some(path) = vow_appearance::xdg::user_dir(kind)
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
            tr!("menu-hide-others")
        ));
        assert!(!enabled(
            &malaysia_menu_items(&not_minimized),
            tr!("menu-show-all")
        ));

        let minimized = vec![toplevel(true)];
        assert!(!enabled(
            &malaysia_menu_items(&minimized),
            tr!("menu-hide-others")
        ));
        assert!(enabled(
            &malaysia_menu_items(&minimized),
            tr!("menu-show-all")
        ));
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
            tr!("menu-bring-all-to-front")
        ));

        let minimized = vec![toplevel(true)];
        assert!(enabled(
            &window_menu_items(&[], &minimized),
            tr!("menu-bring-all-to-front")
        ));
    }
}
