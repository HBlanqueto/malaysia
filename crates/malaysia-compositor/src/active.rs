//! Publishes the active window to the shell through the state store.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct ActiveWindow {
    pub title: String,
    pub app_id: String,
}

#[derive(serde::Serialize)]
struct Store<'a> {
    active: StoredActive<'a>,
}

#[derive(serde::Serialize)]
struct StoredActive<'a> {
    title: &'a str,
    app_id: &'a str,
}

pub(crate) fn publish_to(dir: &Path, active: Option<&ActiveWindow>) {
    let path = dir.join("window.domain");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let (title, app_id) = match active {
        Some(window) => (window.title.as_str(), window.app_id.as_str()),
        None => ("", ""),
    };
    let store = Store {
        active: StoredActive { title, app_id },
    };
    if let Ok(text) = toml::to_string(&store) {
        let _ = std::fs::write(&path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::publish_to;

    #[test]
    fn publishing_writes_the_active_window() {
        let dir = std::env::temp_dir().join(format!("malaysia-active-{}", std::process::id()));
        publish_to(
            &dir,
            Some(&super::ActiveWindow {
                title: "foot".into(),
                app_id: "foot".into(),
            }),
        );
        let text = std::fs::read_to_string(dir.join("window.domain")).expect("store written");
        assert!(text.contains("title = \"foot\""));
        publish_to(&dir, None);
        let text = std::fs::read_to_string(dir.join("window.domain")).expect("store written");
        assert!(text.contains("title = \"\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn titles_with_quotes_and_newlines_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("malaysia-active-escape-{}", std::process::id()));
        let title = "He said \"hi\"\napp_id = \"injected\"";
        publish_to(
            &dir,
            Some(&super::ActiveWindow {
                title: title.into(),
                app_id: "app".into(),
            }),
        );
        let text = std::fs::read_to_string(dir.join("window.domain")).expect("store written");
        let value: toml::Value = toml::from_str(&text).expect("the store is valid TOML");
        assert_eq!(value["active"]["title"].as_str(), Some(title));
        assert_eq!(value["active"]["app_id"].as_str(), Some("app"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
