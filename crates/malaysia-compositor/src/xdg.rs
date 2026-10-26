//! XDG user directories.
//!
//! Screenshots (and anything else the compositor saves for the user) default to
//! the user's Desktop directory. That name is localised — "Desktop",
//! "Escritorio", "Schreibtisch" — so it is never hardcoded: it comes from the
//! XDG user-dirs configuration, and only falls back to `$HOME/Desktop` when the
//! user has none configured.

use std::path::{Path, PathBuf};

/// The user's Desktop directory, resolved through XDG.
///
/// Reads `$XDG_CONFIG_HOME/user-dirs.dirs` (default `~/.config/user-dirs.dirs`)
/// for `XDG_DESKTOP_DIR`, expanding `$HOME`. Falls back to `$HOME/Desktop`.
pub fn desktop_dir() -> PathBuf {
    let explicit = std::env::var("XDG_DESKTOP_DIR")
        .ok()
        .filter(|path| !path.is_empty());
    if let Some(path) = explicit {
        return PathBuf::from(path);
    }
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(dir) = config_home
        .map(|dir| dir.join("user-dirs.dirs"))
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| parse_desktop_dir(&text))
    {
        return dir;
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Desktop"))
        .unwrap_or_else(|| PathBuf::from("Desktop"))
}

/// Extracts `XDG_DESKTOP_DIR` from the contents of a `user-dirs.dirs` file.
///
/// The value is quoted and may use `$HOME`; other lines and comments are
/// ignored. Returns `None` when the key is absent or empty.
fn parse_desktop_dir(text: &str) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "XDG_DESKTOP_DIR" {
            continue;
        }
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            return None;
        }
        let expanded = match value.strip_prefix("$HOME/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|| PathBuf::from(value)),
            None => PathBuf::from(value),
        };
        return Some(expanded);
    }
    None
}

/// Whether `path` is a usable directory to save into.
pub fn is_directory(path: &Path) -> bool {
    path.is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_dir_comes_from_xdg_not_a_fixed_name() {
        // The localized value in the file wins, whatever the language.
        let spanish = "XDG_DESKTOP_DIR=\"$HOME/Escritorio\"\n";
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        assert_eq!(parse_desktop_dir(spanish), Some(home.join("Escritorio")));
        let english = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\n";
        assert_eq!(parse_desktop_dir(english), Some(home.join("Desktop")));
        // An absolute, non-$HOME path is kept as-is.
        assert_eq!(
            parse_desktop_dir("XDG_DESKTOP_DIR=\"/mnt/desktop\"\n"),
            Some(PathBuf::from("/mnt/desktop"))
        );
        assert_eq!(parse_desktop_dir("XDG_DESKTOP_DIR=\"\"\n"), None);
        assert_eq!(
            parse_desktop_dir("XDG_DOCUMENTS_DIR=\"$HOME/Docs\"\n"),
            None
        );
    }
}
