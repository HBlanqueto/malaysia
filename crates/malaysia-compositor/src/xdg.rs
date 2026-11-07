use std::path::PathBuf;

pub fn desktop_dir() -> PathBuf {
    let explicit = std::env::var("XDG_DESKTOP_DIR")
        .ok()
        .filter(|path| !path.is_empty());
    if let Some(path) = explicit {
        return PathBuf::from(path);
    }
    ctappearance::xdg::user_dir("DESKTOP")
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Desktop")))
        .unwrap_or_else(|| PathBuf::from("Desktop"))
}
