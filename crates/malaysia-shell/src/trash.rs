use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

// Trash specification 1.0: the home trash is $XDG_DATA_HOME/Trash, with `files`, `info` and
// `directorysizes`.
fn home_trash() -> Option<PathBuf> {
    ctappearance::xdg::data_home().map(|home| home.join("Trash"))
}

// Trash specification 1.0, "Trash directories": a volume's trash is `$topdir/.Trash/$uid` when
// `.Trash` is a sticky directory that is not a link, and `$topdir/.Trash-$uid` otherwise.
fn volume_trash(topdir: &Path, uid: u32) -> Option<PathBuf> {
    let shared = topdir.join(".Trash");
    let shared_is_sticky_directory =
        fs::symlink_metadata(&shared).is_ok_and(|meta| meta.is_dir() && meta.mode() & 0o1000 != 0);
    let trash = if shared_is_sticky_directory {
        shared.join(uid.to_string())
    } else {
        topdir.join(format!(".Trash-{uid}"))
    };
    trash.is_dir().then_some(trash)
}

// proc(5): field 5 of mountinfo is the mount point, with " \t\n\\" written as octal escapes.
fn mount_points() -> Vec<PathBuf> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    mountinfo
        .lines()
        .filter_map(|line| line.split(' ').nth(4))
        .map(|point| {
            point
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\012", "\n")
                .replace("\\134", "\\")
                .into()
        })
        .collect()
}

fn trashes() -> Vec<PathBuf> {
    let uid = fs::metadata("/proc/self").map_or(0, |meta| meta.uid());
    home_trash()
        .into_iter()
        .chain(
            mount_points()
                .iter()
                .filter_map(|topdir| volume_trash(topdir, uid)),
        )
        .collect()
}

fn has_items(trash: &Path) -> bool {
    fs::read_dir(trash.join("files")).is_ok_and(|mut entries| entries.next().is_some())
}

pub(crate) fn is_empty() -> bool {
    !trashes().iter().any(|trash| has_items(trash))
}

pub(crate) fn empty() {
    trashes().iter().for_each(|trash| empty_in(trash));
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

    #[test]
    fn a_volume_uses_the_shared_trash_only_when_it_is_a_sticky_directory_and_otherwise_its_own() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("malaysia-volume-{}", std::process::id()));
        let own = root.join(".Trash-1000");
        fs::create_dir_all(&own).unwrap();
        fs::create_dir_all(root.join(".Trash/1000")).unwrap();
        assert_eq!(volume_trash(&root, 1000), Some(own.clone()));

        fs::set_permissions(root.join(".Trash"), fs::Permissions::from_mode(0o1777)).unwrap();
        assert_eq!(volume_trash(&root, 1000), Some(root.join(".Trash/1000")));
        assert_eq!(volume_trash(&root, 1001), None);

        fs::remove_dir_all(root.join(".Trash")).unwrap();
        std::os::unix::fs::symlink(&own, root.join(".Trash")).unwrap();
        assert_eq!(volume_trash(&root, 1000), Some(own));
        fs::remove_dir_all(root).unwrap();
    }
}
