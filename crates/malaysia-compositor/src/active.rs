//! The compositor's active window, published for the shell as state.
//!
//! The menu bar shows the active app's name (macOS: the frontmost app). The
//! active window is internal to the compositor, so it is published through the
//! state store — `~/.local/share/yuki/window.domain`, the `~/Library/Application
//! Support` equivalent — which the shell components watch. It is the same
//! pattern as the wallpaper store: the compositor owns the state, the shell
//! reads it.
//!
//! This is the first-version mechanism; a proper notification channel (the
//! D-Bus surface of ADR 0021) will replace the file when it lands.

use std::path::{Path, PathBuf};

/// The active window's identity, as the shell wants it.
#[derive(Debug, Clone)]
pub struct ActiveWindow {
    /// The window title.
    pub title: String,
    /// The window's app id.
    pub app_id: String,
}

/// The state/data directory: `YUKI_DATA_DIR`, else `$XDG_DATA_HOME/yuki`
/// (`~/.local/share/yuki` by default) — the `~/Library/Application Support`
/// equivalent.
fn data_dir() -> PathBuf {
    std::env::var_os("YUKI_DATA_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|h| h.join(".local/share"))
                })
                .unwrap_or_else(|| PathBuf::from(".local/share"));
            base.join("malaysia-compositor")
        },
        PathBuf::from,
    )
}

/// Publishes the active window (or its absence) to the store.
pub fn publish(active: Option<&ActiveWindow>) {
    publish_to(&data_dir(), active);
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
    let text = format!("[active]\ntitle = \"{}\"\napp_id = \"{}\"\n", title, app_id);
    let _ = std::fs::write(&path, text);
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
}
