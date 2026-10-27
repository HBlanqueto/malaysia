//! Single inotify watcher shared by compositor, shell and portal.
//!
//! Watches the configuration and data directories (not the files themselves)
//! so atomic renames are handled correctly. The raw fd is owned by this struct
//! and can be registered with calloop or any other fd-poller.

use std::io;
use std::os::fd::{AsFd, AsRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, InotifyEvent};

use crate::{config_dir, data_dir};

/// Kind of appearance-related change detected by the watcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceEvent {
    /// `appearance.domain` changed.
    Appearance,
    /// `wallpaper.state` changed.
    Wallpaper,
    /// `wallpaper.domain` changed (the selected picture or scale).
    WallpaperSelection,
    /// Accent colour inside `appearance.domain` changed.
    Accent,
}

/// A single inotify instance watching `appearance.domain`, `wallpaper.domain`
/// and `wallpaper.state`.
///
/// The raw fd stays valid for the lifetime of this struct. The type is
/// `Send + Sync` so it can be moved into a calloop event source.
#[derive(Debug)]
pub struct AppearanceWatch {
    inotify: Inotify,
}

impl AppearanceWatch {
    /// Creates a watcher for the default config and data directories.
    pub fn new() -> io::Result<Self> {
        Self::new_for(config_dir(), data_dir())
    }

    /// Creates a watcher for explicit directories.
    pub fn new_for(config_dir: PathBuf, data_dir: PathBuf) -> io::Result<Self> {
        // Watch the directories, not the files, so atomic renames trigger
        // IN_MOVED_TO. Create them first so a fresh install or a wiped cache
        // does not fail before the first write.
        std::fs::create_dir_all(&config_dir)?;
        std::fs::create_dir_all(&data_dir)?;
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).map_err(into_io_error)?;
        inotify
            .add_watch(&config_dir, Self::flags())
            .map_err(into_io_error)?;
        inotify
            .add_watch(&data_dir, Self::flags())
            .map_err(into_io_error)?;

        Ok(Self { inotify })
    }

    fn flags() -> AddWatchFlags {
        // IN_DELETE covers clearing `wallpaper.state` (no dominant colour);
        // IN_DELETE_SELF would only fire for the watched directory itself.
        AddWatchFlags::IN_CLOSE_WRITE | AddWatchFlags::IN_MOVED_TO | AddWatchFlags::IN_DELETE
    }

    /// Drains every pending inotify event and returns the distinct appearance
    /// events that occurred.
    ///
    /// Call this from the calloop callback (or equivalent) when the fd becomes
    /// readable. Empty events mean the fd was readable but held no matching
    /// file changes.
    pub fn drain(&self) -> io::Result<Vec<AppearanceEvent>> {
        let mut events = Vec::with_capacity(2);
        loop {
            match self.inotify.read_events() {
                Ok(batch) => {
                    if batch.is_empty() {
                        break;
                    }
                    for event in batch {
                        Self::classify(&event, &mut events);
                    }
                }
                Err(nix::Error::EAGAIN) => break,
                Err(err) => return Err(into_io_error(err)),
            }
        }
        Ok(events)
    }

    fn classify(event: &InotifyEvent, out: &mut Vec<AppearanceEvent>) {
        let Some(name) = event.name.as_deref() else {
            return;
        };
        let name_bytes = name.as_bytes();
        if name_bytes == b"appearance.domain" {
            push_unique(out, AppearanceEvent::Appearance);
            push_unique(out, AppearanceEvent::Accent);
        } else if name_bytes == b"wallpaper.domain" {
            push_unique(out, AppearanceEvent::WallpaperSelection);
        } else if name_bytes == b"wallpaper.state" {
            push_unique(out, AppearanceEvent::Wallpaper);
        }
    }
}

impl AsFd for AppearanceWatch {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.inotify.as_fd()
    }
}

impl AsRawFd for AppearanceWatch {
    fn as_raw_fd(&self) -> RawFd {
        self.inotify.as_fd().as_raw_fd()
    }
}

fn push_unique(events: &mut Vec<AppearanceEvent>, event: AppearanceEvent) {
    if !events.contains(&event) {
        events.push(event);
    }
}

fn into_io_error(err: nix::Error) -> io::Error {
    io::Error::from(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_dirs() -> (PathBuf, PathBuf) {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let config = std::env::temp_dir().join(format!("vow-appearance-watch-config-{pid}-{n}"));
        let data = std::env::temp_dir().join(format!("vow-appearance-watch-data-{pid}-{n}"));
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        (config, data)
    }

    fn write(path: &Path, text: &str) {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text).unwrap();
        std::fs::rename(&tmp, path).unwrap();
    }

    #[test]
    fn creates_watched_directories_when_missing() {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let config_dir =
            std::env::temp_dir().join(format!("vow-appearance-watch-missing-config-{pid}-{n}"));
        let data_dir =
            std::env::temp_dir().join(format!("vow-appearance-watch-missing-data-{pid}-{n}"));
        let _ = std::fs::remove_dir_all(&config_dir);
        let _ = std::fs::remove_dir_all(&data_dir);
        assert!(!config_dir.exists());
        assert!(!data_dir.exists());

        let watch = AppearanceWatch::new_for(config_dir.clone(), data_dir.clone()).unwrap();
        assert!(config_dir.exists());
        assert!(data_dir.exists());

        let path = config_dir.join("appearance.domain");
        write(&path, "color_scheme = \"dark\"\n");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut found = false;
        while Instant::now() < deadline {
            if let Ok(events) = watch.drain()
                && events.contains(&AppearanceEvent::Appearance)
            {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            found,
            "appearance.domain rename was not detected after creating dirs"
        );
        let _ = std::fs::remove_dir_all(&config_dir);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn detects_appearance_domain_rename() {
        let (config_dir, data_dir) = tmp_dirs();
        let watch = AppearanceWatch::new_for(config_dir.clone(), data_dir.clone()).unwrap();
        let path = config_dir.join("appearance.domain");

        write(&path, "color_scheme = \"dark\"\n");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut found = false;
        while Instant::now() < deadline {
            if let Ok(events) = watch.drain()
                && events.contains(&AppearanceEvent::Appearance)
            {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(found, "appearance.domain rename was not detected");
    }

    #[test]
    fn detects_wallpaper_state_close_write() {
        let (config_dir, data_dir) = tmp_dirs();
        let watch = AppearanceWatch::new_for(config_dir.clone(), data_dir.clone()).unwrap();
        let path = data_dir.join("wallpaper.state");

        write(&path, "dominant = [0.8, 0.2, 0.1]\n");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut found = false;
        while Instant::now() < deadline {
            if let Ok(events) = watch.drain()
                && events.contains(&AppearanceEvent::Wallpaper)
            {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(found, "wallpaper.state close_write was not detected");
    }

    #[test]
    fn detects_wallpaper_domain_rename() {
        let (config_dir, data_dir) = tmp_dirs();
        let watch = AppearanceWatch::new_for(config_dir, data_dir.clone()).unwrap();
        let path = data_dir.join("wallpaper.domain");

        write(&path, "[desktops]\ndefault = \"file:///Solid Gray.png\"\n");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Ok(events) = watch.drain()
                && events.contains(&AppearanceEvent::WallpaperSelection)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("wallpaper.domain rename was not detected");
    }

    #[test]
    fn detects_wallpaper_state_delete() {
        let (config_dir, data_dir) = tmp_dirs();
        let watch = AppearanceWatch::new_for(config_dir, data_dir.clone()).unwrap();
        let path = data_dir.join("wallpaper.state");

        write(&path, "dominant = [0.8, 0.2, 0.1]\n");
        // Drain the rename that created the state so only the delete is pending.
        let _ = watch.drain();

        std::fs::remove_file(&path).unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Ok(events) = watch.drain()
                && events.contains(&AppearanceEvent::Wallpaper)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("wallpaper.state delete was not detected");
    }

    #[test]
    fn latency_from_write_to_event_is_under_50ms() {
        let (config_dir, data_dir) = tmp_dirs();
        let watch = AppearanceWatch::new_for(config_dir.clone(), data_dir.clone()).unwrap();
        let path = config_dir.join("appearance.domain");

        // Warm up.
        write(&path, "color_scheme = \"light\"\n");
        let _ = watch.drain();

        let start = Instant::now();
        write(&path, "color_scheme = \"dark\"\n");

        let deadline = start + Duration::from_millis(50);
        let mut latency = None;
        while Instant::now() < deadline {
            if let Ok(events) = watch.drain()
                && events.contains(&AppearanceEvent::Appearance)
            {
                latency = Some(start.elapsed());
                break;
            }
            std::thread::sleep(Duration::from_micros(100));
        }

        let elapsed = latency.expect("event did not arrive within 50 ms");
        assert!(
            elapsed < Duration::from_millis(50),
            "latency {elapsed:?} is not under 50 ms"
        );
    }
}
