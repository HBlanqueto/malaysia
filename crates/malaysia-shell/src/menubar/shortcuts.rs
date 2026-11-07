use std::sync::LazyLock;

use bk::menu::MenuItem;
use rust_embed::RustEmbed;
use serde::Deserialize;

use crate::desktop::DesktopEntry;
use crate::localize::LOADER;
use crate::menubar::find_entry;

#[derive(RustEmbed)]
#[folder = "shortcuts/"]
struct Catalog;

#[derive(Deserialize)]
struct AppShortcuts {
    ids: Vec<String>,
    menus: Vec<ShortcutMenu>,
}

#[derive(Deserialize)]
struct ShortcutMenu {
    label: String,
    items: Vec<Vec<String>>,
}

static CATALOG: LazyLock<Vec<AppShortcuts>> = LazyLock::new(|| {
    Catalog::iter()
        .filter_map(|name| {
            let file = Catalog::get(&name)?;
            let text = std::str::from_utf8(&file.data).ok()?;
            toml::from_str(text)
                .inspect_err(|error| tracing::warn!(%name, %error, "shortcut menu skipped"))
                .ok()
        })
        .collect()
});

pub fn menus(app_id: &str, entries: &[DesktopEntry]) -> Vec<MenuItem> {
    let desktop_id = find_entry(app_id, entries).map(|entry| entry.id.as_str());
    let Some(app) = CATALOG.iter().find(|app| {
        app.ids.iter().any(|id| {
            id.eq_ignore_ascii_case(app_id)
                || desktop_id.is_some_and(|desktop| id.eq_ignore_ascii_case(desktop))
        })
    }) else {
        return Vec::new();
    };
    app.menus
        .iter()
        .map(|menu| {
            MenuItem::submenu(
                label(&menu.label),
                menu.items.iter().map(|item| shortcut_item(item)).collect(),
            )
        })
        .collect()
}

fn shortcut_item(item: &[String]) -> MenuItem {
    match item {
        [key, keys] => {
            MenuItem::action(format!("app-shortcut:{keys}"), label(key)).with_shortcut(keys.clone())
        }
        _ => MenuItem::Separator,
    }
}

fn label(key: &str) -> String {
    LOADER.get(&format!("shortcut-{key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_file_parses() {
        assert_eq!(CATALOG.len(), Catalog::iter().count());
    }

    #[test]
    fn every_label_has_spanish_and_english_text() {
        let ftl = |lang: &str| {
            std::fs::read_to_string(format!(
                "{}/i18n/{lang}/malaysia-shell.ftl",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap()
        };
        let (es, en) = (ftl("es"), ftl("en"));
        for app in CATALOG.iter() {
            let keys = app.menus.iter().flat_map(|menu| {
                std::iter::once(&menu.label)
                    .chain(menu.items.iter().filter_map(|item| item.first()))
            });
            for key in keys {
                let message = format!("\nshortcut-{key} =");
                assert!(
                    es.contains(&message) && en.contains(&message),
                    "missing {message}"
                );
            }
        }
    }
}
