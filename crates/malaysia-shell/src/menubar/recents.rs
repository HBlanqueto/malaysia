use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use barrier_kit::app::{read_toml, write_toml};
use barrier_kit::menu::{MenuIcon, MenuItem};
use vow_appearance::data_dir;

use crate::desktop::DesktopEntry;
use crate::launcher::{IconData, load_icon_sized};
use crate::menubar::{find_entry, icons};

const MAX_APPS: usize = 10;
const MAX_DOCUMENTS: usize = 10;
const MENU_ICON_SIZES: &[&str] = &[
    "scalable", "32x32", "48x48", "24x24", "64x64", "16x16", "128x128",
];

pub const APP_PREFIX: &str = "recent-app:";
pub const DOC_PREFIX: &str = "recent-doc:";
pub const CLEAR_ID: &str = "recent-clear";

#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Recents {
    apps: Vec<String>,
    cleared_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct Document {
    path: PathBuf,
    name: String,
    modified: i64,
}

fn store_path() -> PathBuf {
    data_dir().join("recents.domain")
}

impl Recents {
    fn push_app(&mut self, app_id: &str) -> bool {
        let app_id = app_id.trim();
        if app_id.is_empty() || self.apps.first().is_some_and(|first| first == app_id) {
            return false;
        }
        self.apps.retain(|id| id != app_id);
        self.apps.insert(0, app_id.to_string());
        self.apps.truncate(MAX_APPS);
        true
    }
}

pub fn record_app(app_id: &str) {
    let path = store_path();
    let mut recents: Recents = read_toml(&path);
    if recents.push_app(app_id) {
        let _ = write_toml(&path, &recents);
    }
}

pub fn clear() {
    let path = store_path();
    let recents = Recents {
        apps: Vec::new(),
        cleared_at: chrono::Utc::now().timestamp(),
    };
    let _ = write_toml(&path, &recents);
}

fn xbel_path() -> PathBuf {
    crate::desktop::data_home().join("recently-used.xbel")
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')?;
    Some(&tag[start..start + end])
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn timestamp(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp())
        .unwrap_or(0)
}

fn parse_xbel(text: &str) -> Vec<Document> {
    let mut documents: Vec<Document> = text
        .split("<bookmark ")
        .skip(1)
        .filter_map(|chunk| {
            let tag = &chunk[..chunk.find('>')?];
            let tag = format!(" {tag}");
            let href = unescape_xml(attribute(&tag, "href")?);
            let path = PathBuf::from(percent_decode(href.strip_prefix("file://")?));
            let name = path.file_name()?.to_string_lossy().into_owned();
            let modified = ["visited", "modified", "added"]
                .iter()
                .filter_map(|key| attribute(&tag, key))
                .map(timestamp)
                .max()
                .unwrap_or(0);
            Some(Document {
                path,
                name,
                modified,
            })
        })
        .collect();
    documents.sort_by_key(|doc| std::cmp::Reverse(doc.modified));
    documents.dedup_by(|a, b| a.path == b.path);
    documents
}

fn documents(cleared_at: i64) -> Vec<Document> {
    let text = std::fs::read_to_string(xbel_path()).unwrap_or_default();
    parse_xbel(&text)
        .into_iter()
        .filter(|doc| doc.modified > cleared_at && doc.path.is_file())
        .take(MAX_DOCUMENTS)
        .collect()
}

fn menu_icon(icon: Option<IconData>) -> Option<MenuIcon> {
    let icon = icon?;
    match icon.svg {
        Some(svg) => String::from_utf8(svg).ok().map(MenuIcon::svg),
        None => std::fs::read(&icon.path).ok().and_then(MenuIcon::image),
    }
}

fn document_icon(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "heic" | "bmp" | "tif" | "tiff" => {
            icons::FILE_IMAGE
        }
        "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "pdf" => icons::FILE_TEXT,
        _ => icons::FILE,
    }
}

pub fn menu_items(entries: &[DesktopEntry]) -> Vec<MenuItem> {
    thread_local! {
        static ICONS: RefCell<HashMap<String, Option<MenuIcon>>> = RefCell::default();
    }
    let recents: Recents = read_toml(&store_path());
    build_items(&recents, &documents(recents.cleared_at), entries, |entry| {
        ICONS.with_borrow_mut(|icons| {
            icons
                .entry(entry.id.clone())
                .or_insert_with(|| {
                    menu_icon(load_icon_sized(entry.icon.as_deref(), MENU_ICON_SIZES))
                })
                .clone()
        })
    })
}

fn build_items(
    recents: &Recents,
    documents: &[Document],
    entries: &[DesktopEntry],
    icon: impl Fn(&DesktopEntry) -> Option<MenuIcon>,
) -> Vec<MenuItem> {
    let mut apps: Vec<&DesktopEntry> = recents
        .apps
        .iter()
        .filter_map(|id| find_entry(id, entries))
        .collect();
    apps.sort_by_key(|entry| entry.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);

    let mut out = Vec::new();
    if !apps.is_empty() {
        out.push(MenuItem::header("Aplicaciones"));
        for entry in apps {
            out.push(
                MenuItem::action(format!("{APP_PREFIX}{}", entry.id), entry.name.clone())
                    .with_icon(icon(entry)),
            );
        }
        out.push(MenuItem::Separator);
    }
    if !documents.is_empty() {
        let mut documents = documents.to_vec();
        documents.sort_by_key(|doc| doc.name.to_lowercase());
        out.push(MenuItem::header("Documentos"));
        for doc in &documents {
            out.push(
                MenuItem::action(
                    format!("{DOC_PREFIX}{}", doc.path.display()),
                    doc.name.clone(),
                )
                .with_icon(MenuIcon::svg(document_icon(&doc.path))),
            );
        }
        out.push(MenuItem::Separator);
    }
    out.push(MenuItem::action(CLEAR_ID, "Vaciar menú").with_enabled(!out.is_empty()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str) -> DesktopEntry {
        DesktopEntry {
            id: id.into(),
            name: name.into(),
            generic_name: None,
            keywords: Vec::new(),
            icon: None,
            exec: vec![id.trim_end_matches(".desktop").into()],
            path: None,
            terminal: false,
            startup_wm_class: None,
        }
    }

    #[test]
    fn recent_apps_move_to_the_front_and_stay_bounded() {
        let mut recents = Recents::default();
        for i in 0..12 {
            recents.push_app(&format!("app{i}"));
        }
        assert_eq!(recents.apps.len(), MAX_APPS);
        assert!(recents.push_app("app5"));
        assert_eq!(recents.apps[0], "app5");
        assert!(!recents.push_app("app5"));
        assert!(!recents.push_app(" "));
    }

    #[test]
    fn xbel_documents_are_decoded_and_sorted() {
        let xbel = r#"<?xml version="1.0"?>
<xbel version="1.0">
  <bookmark href="file:///home/u/Carta_Poder.pdf" added="2026-10-01T10:00:00Z" modified="2026-10-01T10:00:00Z" visited="2026-10-01T10:00:00Z">
  </bookmark>
  <bookmark href="file:///home/u/Acta%20de%20defunci%C3%B3n.pdf" added="2026-10-02T10:00:00.123456Z" modified="2026-10-02T10:00:00Z" visited="2026-10-03T10:00:00Z">
  </bookmark>
  <bookmark href="https://example.org/" added="2026-10-04T10:00:00Z" modified="2026-10-04T10:00:00Z" visited="2026-10-04T10:00:00Z"/>
</xbel>"#;
        let docs = parse_xbel(xbel);
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].name, "Acta de defunción.pdf");
        assert_eq!(docs[1].path, PathBuf::from("/home/u/Carta_Poder.pdf"));
        assert!(docs[0].modified > docs[1].modified);
    }

    #[test]
    fn a_percent_before_multibyte_text_is_kept_literally() {
        assert_eq!(percent_decode("%1ñ%C3%B3"), "%1ñó");
    }

    #[test]
    fn the_submenu_lists_apps_then_documents_then_clear() {
        let entries = vec![
            entry("org.telegram.desktop", "Telegram"),
            entry("firefox.desktop", "Firefox"),
        ];
        let recents = Recents {
            apps: vec![
                "org.telegram.desktop".into(),
                "firefox.desktop".into(),
                "missing".into(),
            ],
            cleared_at: 0,
        };
        let docs = vec![Document {
            path: "/tmp/a.pdf".into(),
            name: "a.pdf".into(),
            modified: 1,
        }];
        let items = build_items(&recents, &docs, &entries, |_| None);
        let labels: Vec<_> = items
            .iter()
            .map(|item| item.label().unwrap_or("-"))
            .collect();
        assert_eq!(
            labels,
            [
                "Aplicaciones",
                "Firefox",
                "Telegram",
                "-",
                "Documentos",
                "a.pdf",
                "-",
                "Vaciar menú"
            ]
        );
    }

    #[test]
    fn an_empty_menu_only_offers_a_disabled_clear() {
        let items = build_items(&Recents::default(), &[], &[], |_| None);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], MenuItem::Action { enabled: false, .. }));
    }
}
