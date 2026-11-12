use std::fs;
use std::path::{Path, PathBuf};

// Trash specification 1.0: the home trash is $XDG_DATA_HOME/Trash, with `files`, `info` and
// `directorysizes`.
fn home_trash() -> Option<PathBuf> {
    ctappearance::xdg::data_home().map(|home| home.join("Trash"))
}

fn has_items(trash: &Path) -> bool {
    fs::read_dir(trash.join("files")).is_ok_and(|mut entries| entries.next().is_some())
}

pub(crate) fn is_empty() -> bool {
    !home_trash().is_some_and(|trash| has_items(&trash))
}

pub(crate) fn empty() {
    if let Some(trash) = home_trash() {
        empty_in(&trash);
    }
}

fn empty_in(trash: &Path) {
    for directory in ["files", "info"] {
        let Ok(entries) = fs::read_dir(trash.join(directory)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // `file_type` does not follow links: a trashed link goes, its target stays.
            let removed = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            if let Err(error) = removed {
                tracing::warn!(path = %path.display(), %error, "trash: could not erase");
            }
        }
    }
    let _ = fs::remove_file(trash.join("directorysizes"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emptying_erases_files_and_info_and_keeps_what_a_trashed_link_points_to() {
        let root = std::env::temp_dir().join(format!("malaysia-trash-{}", std::process::id()));
        let trash = root.join("Trash");
        let kept = root.join("kept");
        fs::create_dir_all(trash.join("files/folder")).unwrap();
        fs::create_dir_all(trash.join("info")).unwrap();
        fs::create_dir_all(&kept).unwrap();
        fs::write(kept.join("document"), "").unwrap();
        fs::write(trash.join("files/folder/inside"), "").unwrap();
        fs::write(trash.join("files/note"), "").unwrap();
        fs::write(trash.join("info/note.trashinfo"), "").unwrap();
        fs::write(trash.join("directorysizes"), "").unwrap();
        std::os::unix::fs::symlink(&kept, trash.join("files/link")).unwrap();
        assert!(has_items(&trash));

        empty_in(&trash);

        assert!(!has_items(&trash));
        assert_eq!(fs::read_dir(trash.join("info")).unwrap().count(), 0);
        assert!(!trash.join("directorysizes").exists());
        assert!(kept.join("document").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
