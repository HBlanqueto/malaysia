use bk::menu::{CheckState, MenuIcon, MenuItem, SearchSource, SearchableEntry};
use bk::shell::{InfinityLayout, InfinityPairLayout, ToplevelInfo, Toplevels};

use crate::localize::tr;
use crate::menubar::{help_topics, icons};
use crate::spawn::{spawn_component_with, xdg_open};

const ISSUES_URL: &str = "https://github.com/HBlanqueto/malaysia/issues";

pub fn app_menu_items(app_name: &str, app_id: &str, toplevels: &[ToplevelInfo]) -> Vec<MenuItem> {
    let has_windows = app_windows(toplevels, app_id).next().is_some();
    let other_unminimized = toplevels
        .iter()
        .any(|window| window.app_id != app_id && !window.minimized);
    let any_minimized = toplevels.iter().any(|window| window.minimized);
    vec![
        MenuItem::action(
            format!("app-about:{app_id}"),
            tr!("app-about", app = app_name),
        )
        .with_enabled(!app_id.is_empty())
        .with_icon(MenuIcon::svg(icons::INFO)),
        MenuItem::Separator,
        MenuItem::action("app-settings", tr!("menu-settings"))
            .with_enabled(false)
            .with_icon(MenuIcon::svg(icons::GEAR)),
        MenuItem::Separator,
        services_submenu(),
        MenuItem::Separator,
        MenuItem::action(
            format!("app-hide:{app_id}"),
            tr!("app-hide", app = app_name),
        )
        .with_shortcut("Super+H")
        .with_enabled(has_windows),
        MenuItem::action(format!("app-hide-others:{app_id}"), tr!("menu-hide-others"))
            .with_shortcut("Alt+Super+H")
            .with_enabled(other_unminimized),
        MenuItem::action("app-show-all", tr!("menu-show-all")).with_enabled(any_minimized),
        MenuItem::Separator,
        MenuItem::action(
            format!("app-quit:{app_id}"),
            tr!("app-quit", app = app_name),
        )
        .with_shortcut("Super+Q")
        .with_enabled(has_windows),
    ]
}

pub fn window_menu_items(
    app_name: &str,
    app_id: &str,
    toplevels: &[ToplevelInfo],
    full_screen: bool,
) -> Vec<MenuItem> {
    let app: Vec<&ToplevelInfo> = app_windows(toplevels, app_id).collect();
    let active = active_window(&app);
    let mut items = window_controls(active, full_screen);
    items.extend([
        MenuItem::Separator,
        MenuItem::action(
            active_id("app-window-close", active),
            tr!("menu-close-window"),
        )
        .with_shortcut("Super+W")
        .with_enabled(active.is_some()),
        MenuItem::Separator,
        MenuItem::action(
            format!("app-window-front:{app_id}"),
            tr!("menu-bring-all-to-front"),
        )
        .with_enabled(!app.is_empty()),
        MenuItem::Separator,
    ]);
    items.extend(app.iter().map(|window| {
        let checked = active.is_some_and(|active| active.id == window.id);
        window_item(window, app_name, CheckState::Checkbox(checked))
    }));
    items
}

pub(crate) fn window_controls(active: Option<&ToplevelInfo>, full_screen: bool) -> Vec<MenuItem> {
    let has_active = active.is_some();
    let full_screen_label = if full_screen {
        tr!("window-exit-full-screen")
    } else {
        tr!("window-enter-full-screen")
    };
    vec![
        MenuItem::action(
            active_id("app-window-minimize", active),
            tr!("window-minimize"),
        )
        .with_shortcut("Super+M")
        .with_enabled(has_active),
        MenuItem::action(
            active_id("app-window-maximize", active),
            tr!("window-maximize"),
        )
        .with_enabled(has_active),
        MenuItem::action(active_id("app-window-fill", active), tr!("window-fill"))
            .with_shortcut("Ctrl+Super+Return")
            .with_enabled(has_active),
        MenuItem::action(active_id("app-window-center", active), tr!("window-center"))
            .with_shortcut("Ctrl+Super+C")
            .with_enabled(has_active),
        MenuItem::Separator,
        MenuItem::submenu(tr!("window-move-resize"), move_resize_items(active))
            .with_icon(MenuIcon::svg(icons::GRID)),
        MenuItem::action(
            active_id("app-window-fullscreen", active),
            full_screen_label,
        )
        .with_shortcut("Ctrl+Super+F")
        .with_enabled(has_active),
    ]
}

pub(crate) fn window_item(window: &ToplevelInfo, app_name: &str, check: CheckState) -> MenuItem {
    let label = if window.title.is_empty() || window.title == app_name {
        app_name.to_string()
    } else {
        format!("{app_name} — {}", window.title)
    };
    MenuItem::action(format!("app-window-activate:{}", window.id), label).with_check(check)
}

pub(crate) fn services_submenu() -> MenuItem {
    MenuItem::submenu(
        tr!("menu-services"),
        vec![
            MenuItem::action("app-services-none", tr!("menu-services-none")).with_enabled(false),
            MenuItem::Separator,
            MenuItem::action("app-services-settings", tr!("menu-services-settings"))
                .with_enabled(false),
        ],
    )
}

pub fn help_menu_items(app_name: &str) -> Vec<MenuItem> {
    vec![
        MenuItem::SearchField {
            placeholder: tr!("menu-search").to_string(),
        },
        MenuItem::action("app-help", tr!("app-help", app = app_name)).with_enabled(false),
        MenuItem::Separator,
        MenuItem::action("app-help-shortcuts", tr!("help-shortcuts")).with_enabled(false),
        MenuItem::action("app-help-docs", tr!("help-docs")).with_enabled(false),
        MenuItem::action("app-help-report", tr!("menu-report-issue")),
    ]
}

pub fn help_search_topics() -> Vec<SearchableEntry> {
    help_topics::topics()
        .iter()
        .map(|topic| topic.item(None))
        .chain([MenuItem::action(
            "app-help-topics-index",
            tr!("help-show-all-topics"),
        )])
        .map(|item| SearchableEntry {
            path: tr!("help-topics").to_string(),
            item,
            location: None,
            source: SearchSource::Help,
        })
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
        "app-shortcut" => {
            if let Some(window) = list.iter().find(|window| window.active) {
                toplevels.send_shortcut(window.id, argument);
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
        "app-window-fullscreen" => {
            toplevels.set_fullscreen(id, !toplevels.focus_bar.peek().over_focus());
        }
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
        MenuItem::header(tr!("move-halves")),
        MenuItem::action(active_id("app-move-left", active), tr!("move-left"))
            .with_shortcut("Ctrl+Super+Left")
            .with_enabled(has_active)
            .with_icon(MenuIcon::svg(icons::HALF_LEFT)),
        MenuItem::action(active_id("app-move-right", active), tr!("move-right"))
            .with_shortcut("Ctrl+Super+Right")
            .with_enabled(has_active)
            .with_icon(MenuIcon::svg(icons::HALF_RIGHT)),
        MenuItem::action(active_id("app-move-up", active), tr!("move-top"))
            .with_shortcut("Ctrl+Super+Up")
            .with_enabled(has_active)
            .with_icon(MenuIcon::svg(icons::HALF_TOP)),
        MenuItem::action(active_id("app-move-down", active), tr!("move-bottom"))
            .with_shortcut("Ctrl+Super+Down")
            .with_enabled(has_active)
            .with_icon(MenuIcon::svg(icons::HALF_BOTTOM)),
        MenuItem::Separator,
        MenuItem::header(tr!("move-quarters")),
        MenuItem::action(
            active_id("app-quarter-top-left", active),
            tr!("move-top-left"),
        )
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::QUARTER_TOP_LEFT)),
        MenuItem::action(
            active_id("app-quarter-top-right", active),
            tr!("move-top-right"),
        )
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::QUARTER_TOP_RIGHT)),
        MenuItem::action(
            active_id("app-quarter-bottom-left", active),
            tr!("move-bottom-left"),
        )
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::QUARTER_BOTTOM_LEFT)),
        MenuItem::action(
            active_id("app-quarter-bottom-right", active),
            tr!("move-bottom-right"),
        )
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::QUARTER_BOTTOM_RIGHT)),
        MenuItem::Separator,
        MenuItem::header(tr!("move-arrange")),
        MenuItem::action(
            active_id("app-layout-side-by-side", active),
            tr!("move-left-right"),
        )
        .with_shortcut("Ctrl+Shift+Super+Left")
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::HALVES_LEFT_RIGHT)),
        MenuItem::action(
            active_id("app-layout-reversed-side-by-side", active),
            tr!("move-right-left"),
        )
        .with_shortcut("Ctrl+Shift+Super+Right")
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::HALVES_RIGHT_LEFT)),
        MenuItem::action(
            active_id("app-layout-stacked", active),
            tr!("move-top-bottom"),
        )
        .with_shortcut("Ctrl+Shift+Super+Up")
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::HALVES_TOP_BOTTOM)),
        MenuItem::action(
            active_id("app-layout-reversed-stacked", active),
            tr!("move-bottom-top"),
        )
        .with_shortcut("Ctrl+Shift+Super+Down")
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::HALVES_BOTTOM_TOP)),
        MenuItem::action(
            active_id("app-layout-quarters", active),
            tr!("move-in-quarters"),
        )
        .with_enabled(has_active)
        .with_icon(MenuIcon::svg(icons::GRID)),
        MenuItem::Separator,
        MenuItem::action(active_id("app-move-restore", active), tr!("move-return"))
            .with_shortcut("Ctrl+Super+R")
            .with_enabled(can_restore),
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

    fn is_enabled(item: &MenuItem) -> bool {
        match item {
            MenuItem::Action { enabled, .. } | MenuItem::Submenu { enabled, .. } => *enabled,
            _ => false,
        }
    }

    fn labels(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| item.label().map(ToOwned::to_owned))
            .collect()
    }

    fn find<'a>(items: &'a [MenuItem], wanted: &str) -> &'a MenuItem {
        items
            .iter()
            .find(|item| item.label() == Some(wanted))
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
        assert!(!enabled(&items, tr!("menu-show-all")));
        assert!(!enabled(&items, tr!("menu-hide-others")));
    }

    #[test]
    fn window_menu_lists_only_the_app_windows_with_the_active_checked() {
        let items = window_menu_items("Foot", "foot", &toplevels(), false);
        let listed: Vec<&MenuItem> = items
            .iter()
            .filter(|item| matches!(item, MenuItem::Action { id, .. } if id.starts_with("app-window-activate:")))
            .collect();
        assert_eq!(listed.len(), 2);
        let names: Vec<&str> = listed.iter().filter_map(|item| item.label()).collect();
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
    fn window_menu_controls_are_enabled() {
        let items = window_menu_items("Foot", "foot", &toplevels(), false);
        assert!(enabled(&items, tr!("window-minimize")));
        assert!(enabled(&items, tr!("window-maximize")));
        assert!(enabled(&items, tr!("window-fill")));
        assert!(enabled(&items, tr!("window-center")));
        assert!(enabled(&items, tr!("window-move-resize")));
        assert!(enabled(&items, tr!("window-enter-full-screen")));
        assert!(enabled(&items, tr!("menu-close-window")));
        assert!(enabled(&items, tr!("menu-bring-all-to-front")));
    }

    #[test]
    fn move_and_resize_submenu_has_headers_and_enabled_geometry_items() {
        let items = window_menu_items("Foot", "foot", &toplevels(), false);
        let Some(MenuItem::Submenu { items: layout, .. }) = items
            .iter()
            .find(|item| item.label() == Some(tr!("window-move-resize")))
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
        let items = window_menu_items("Foot", "foot", &toplevels, false);
        let Some(MenuItem::Submenu { items: layout, .. }) = items
            .iter()
            .find(|item| item.label() == Some(tr!("window-move-resize")))
        else {
            panic!("missing the layout submenu");
        };
        assert!(is_enabled(&layout[19]));
    }
}
