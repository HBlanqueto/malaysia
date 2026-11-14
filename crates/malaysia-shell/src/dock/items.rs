use bk::shell::ToplevelInfo;
use ctappearance::dock::DockConfig;
use ctdesign::DOCK_RECENTS;

use crate::desktop::DesktopEntry;
use crate::menubar::find_entry;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppTile {
    pub id: String,
    pub kept: bool,
    pub windows: Vec<ToplevelInfo>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Item {
    App(AppTile),
    Separator,
    Downloads,
    Window(ToplevelInfo),
    Trash,
    // Where a dragged icon would land, as wide as its share of a tile while it opens or closes.
    Gap(f32),
}

pub(crate) fn app_key(toplevel: &ToplevelInfo, entries: &[DesktopEntry]) -> String {
    if toplevel.app_id.is_empty() {
        return toplevel.title.clone();
    }
    find_entry(&toplevel.app_id, entries)
        .map(|entry| entry.id.clone())
        .unwrap_or_else(|| toplevel.app_id.clone())
}

pub(crate) fn running(
    toplevels: &[ToplevelInfo],
    entries: &[DesktopEntry],
) -> Vec<(String, Vec<ToplevelInfo>)> {
    let mut groups: Vec<(String, Vec<ToplevelInfo>)> = Vec::new();
    for toplevel in toplevels {
        let key = app_key(toplevel, entries);
        match groups.iter_mut().find(|(id, _)| *id == key) {
            Some((_, windows)) => windows.push(toplevel.clone()),
            None => groups.push((key, vec![toplevel.clone()])),
        }
    }
    groups
}

pub(crate) fn items(
    config: &DockConfig,
    kept: &[String],
    recent: &[String],
    running: &[(String, Vec<ToplevelInfo>)],
    tiles: &[ToplevelInfo],
) -> Vec<Item> {
    let windows_of = |id: &str| {
        running
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, windows)| windows.clone())
            .unwrap_or_default()
    };
    let tile = |id: &str| {
        Item::App(AppTile {
            id: id.to_string(),
            kept: kept.iter().any(|kept| kept == id),
            windows: windows_of(id),
        })
    };
    let unkept: Vec<&str> = running
        .iter()
        .map(|(id, _)| id.as_str())
        .filter(|id| !kept.iter().any(|kept| kept == id))
        .collect();
    let mut items: Vec<Item> = Vec::new();
    if config.static_only {
        items.extend(
            kept.iter()
                .map(String::as_str)
                .filter(|id| running.iter().any(|(key, _)| key == id))
                .chain(unkept.iter().copied())
                .map(tile),
        );
    } else {
        items.extend(kept.iter().map(|id| tile(id)));
        let recents: Vec<&str> = if config.show_recents {
            recent
                .iter()
                .map(String::as_str)
                .filter(|id| !kept.iter().any(|kept| kept == id) && !unkept.contains(id))
                .take(DOCK_RECENTS.saturating_sub(unkept.len()))
                .collect()
        } else {
            Vec::new()
        };
        if config.show_recents && !(unkept.is_empty() && recents.is_empty()) {
            items.push(Item::Separator);
        }
        items.extend(unkept.iter().chain(recents.iter()).map(|id| tile(id)));
    }
    items.push(Item::Separator);
    if !config.static_only {
        items.push(Item::Downloads);
    }
    items.extend(tiles.iter().cloned().map(Item::Window));
    items.push(Item::Trash);
    items
}

// macOS: a window minimizes into its app's icon, or into the slot its own tile takes before the Trash.
pub(crate) fn minimize_slots(items: &[Item], into_app: bool) -> Vec<(u64, usize)> {
    let trash = items.iter().position(|item| *item == Item::Trash);
    items
        .iter()
        .enumerate()
        .flat_map(|(index, item)| match item {
            Item::App(app) => app
                .windows
                .iter()
                .filter_map(|window| {
                    if into_app {
                        Some((window.id, index))
                    } else {
                        trash
                            .filter(|_| !window.minimized)
                            .map(|trash| (window.id, trash))
                    }
                })
                .collect(),
            Item::Window(window) if window.minimized => vec![(window.id, index)],
            _ => Vec::new(),
        })
        .collect()
}

pub(crate) fn dropped(kept: &[String], id: &str, before: Option<&str>, keep: bool) -> Vec<String> {
    let mut next: Vec<String> = kept.iter().filter(|kept| *kept != id).cloned().collect();
    if keep {
        let at = before
            .and_then(|before| next.iter().position(|kept| kept == before))
            .unwrap_or(next.len());
        next.insert(at, id.to_string());
    }
    next
}

pub(crate) fn remember_closed(
    recent: &[String],
    closed: &[String],
    kept: &[String],
) -> Vec<String> {
    let mut next: Vec<String> = closed
        .iter()
        .filter(|id| !kept.contains(id))
        .cloned()
        .collect();
    next.extend(recent.iter().filter(|id| !closed.contains(id)).cloned());
    next.truncate(DOCK_RECENTS);
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64, app_id: &str, minimized: bool) -> ToplevelInfo {
        ToplevelInfo {
            id,
            title: format!("{app_id} {id}"),
            app_id: app_id.to_string(),
            minimized,
            ..ToplevelInfo::default()
        }
    }

    fn ids(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                Item::App(app) => format!("app:{}", app.id),
                Item::Separator => "separator".to_string(),
                Item::Downloads => "downloads".to_string(),
                Item::Window(window) => format!("window:{}", window.id),
                Item::Trash => "trash".to_string(),
                Item::Gap(_) => "gap".to_string(),
            })
            .collect()
    }

    #[test]
    fn windows_of_one_app_share_one_tile() {
        let groups = running(
            &[window(1, "firefox", false), window(2, "firefox", false)],
            &[],
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 2);
    }

    #[test]
    fn kept_apps_come_first_then_open_apps_and_recents_then_the_stacks() {
        let groups = running(&[window(1, "mail", false), window(2, "maps", false)], &[]);
        let kept = vec!["files".to_string(), "mail".to_string()];
        let recent = vec!["calc".to_string(), "notes".to_string(), "music".to_string()];
        let items = items(&DockConfig::default(), &kept, &recent, &groups, &[]);
        assert_eq!(
            ids(&items),
            [
                "app:files",
                "app:mail",
                "separator",
                "app:maps",
                "app:calc",
                "app:notes",
                "separator",
                "downloads",
                "trash"
            ]
        );
    }

    #[test]
    fn static_only_shows_just_the_open_apps() {
        let groups = running(&[window(1, "mail", false)], &[]);
        let config = DockConfig {
            static_only: true,
            ..DockConfig::default()
        };
        let items = items(&config, &["files".to_string()], &[], &groups, &[]);
        assert_eq!(ids(&items), ["app:mail", "separator", "trash"]);
    }

    #[test]
    fn the_tiles_of_windows_go_between_downloads_and_the_trash() {
        let groups = running(&[window(7, "mail", true)], &[]);
        let tiles = [window(7, "mail", true), window(8, "mail", false)];
        let items = items(&DockConfig::default(), &[], &[], &groups, &tiles);
        assert_eq!(
            ids(&items)[ids(&items).len() - 4..],
            ["downloads", "window:7", "window:8", "trash"]
        );
    }

    #[test]
    fn a_window_minimizes_into_its_own_tile_or_into_the_app_icon() {
        let groups = running(&[window(1, "mail", false), window(2, "mail", true)], &[]);
        let tiles = [window(2, "mail", true)];
        let slots = items(&DockConfig::default(), &[], &[], &groups, &tiles);
        let trash = slots.len() - 1;
        let tile = slots
            .iter()
            .position(|item| matches!(item, Item::Window(window) if window.id == 2))
            .unwrap();
        assert_eq!(minimize_slots(&slots, false), [(1, trash), (2, tile)]);
        let config = DockConfig {
            minimize_to_application: true,
            ..DockConfig::default()
        };
        let slots = items(&config, &[], &[], &groups, &[]);
        let app = slots
            .iter()
            .position(|item| matches!(item, Item::App(_)))
            .unwrap();
        assert_eq!(minimize_slots(&slots, true), [(1, app), (2, app)]);
    }

    #[test]
    fn dropping_moves_an_app_before_another_or_takes_it_out() {
        let kept: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        assert_eq!(dropped(&kept, "c", Some("a"), true), ["c", "a", "b"]);
        assert_eq!(dropped(&kept, "new", None, true), ["a", "b", "c", "new"]);
        assert_eq!(dropped(&kept, "b", None, false), ["a", "c"]);
    }

    #[test]
    fn a_closed_app_becomes_the_newest_recent_and_kept_apps_never_do() {
        let recent = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let next = remember_closed(
            &recent,
            &["c".to_string(), "kept".to_string()],
            &["kept".to_string()],
        );
        assert_eq!(next, ["c", "a", "b"]);
    }
}
