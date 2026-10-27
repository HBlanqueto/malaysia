//! The compositor's active window, published for the shell as state.
//!
//! The menu bar shows the active app's name (macOS: the frontmost app). The
//! active window is internal to the compositor, so it is published through the
//! state store — `~/.local/share/malaysia/window.domain`, the `~/Library/Application
//! Support` equivalent — which the shell components watch. It is the same
//! pattern as the wallpaper store: the compositor owns the state, the shell
//! reads it.
//!
//! This is the first-version mechanism; a proper notification channel (the
//! D-Bus surface of ADR 0021) will replace the file when it lands.

use std::path::Path;

/// The active window's identity, as the shell wants it.
#[derive(Debug, Clone)]
pub struct ActiveWindow {
    /// The window title.
    pub title: String,
    /// The window's app id.
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

/// Writes the store at an explicit directory (testable without touching env).
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
