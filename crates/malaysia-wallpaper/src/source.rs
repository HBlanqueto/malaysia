use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use ctappearance::{
    WallpaperShuffle, WallpaperSource, read_wallpaper_config, wallpaper_config_path,
};

pub fn choices(source: &WallpaperSource) -> Vec<PathBuf> {
    match source {
        WallpaperSource::ShuffleColors => ctappearance::wallpaper_colors(),
        WallpaperSource::ShuffleCollection(name) => read_wallpaper_config(&wallpaper_config_path())
            .ok()
            .and_then(|config| {
                config
                    .collection(name)
                    .map(|collection| collection.wallpapers())
            })
            .unwrap_or_default(),
        WallpaperSource::BuiltIn | WallpaperSource::File(_) => Vec::new(),
    }
}

pub fn pick(choices: &[PathBuf], previous: Option<&Path>, seed: u64) -> Option<PathBuf> {
    let fresh: Vec<&PathBuf> = choices
        .iter()
        .filter(|choice| Some(choice.as_path()) != previous)
        .collect();
    let pool = if fresh.is_empty() {
        choices.iter().collect()
    } else {
        fresh
    };
    let index = (seed % pool.len().max(1) as u64) as usize;
    pool.get(index).map(|&choice| choice.clone())
}

pub fn seed() -> u64 {
    std::collections::hash_map::RandomState::new().hash_one(std::time::SystemTime::now())
}

pub fn due(shuffle: WallpaperShuffle, shuffled_at: Option<u64>, now: u64) -> bool {
    shuffle
        .seconds()
        .zip(shuffled_at)
        .is_some_and(|(every, at)| now >= at.saturating_add(every))
}

pub fn wallpaper_files() -> Vec<PathBuf> {
    let system = ctappearance::system_wallpapers_dir()
        .map(|dir| ctappearance::wallpapers_in(&dir))
        .unwrap_or_default();
    let collections = read_wallpaper_config(&wallpaper_config_path())
        .map(|config| config.collections)
        .unwrap_or_default();
    system
        .into_iter()
        .chain(ctappearance::wallpaper_colors())
        .chain(
            collections
                .iter()
                .flat_map(|collection| collection.wallpapers()),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_shuffle_never_repeats_the_previous_wallpaper() {
        let choices = files(&["a.png", "b.png"]);
        for seed in 0..8 {
            assert_eq!(
                pick(&choices, Some(Path::new("a.png")), seed),
                Some(PathBuf::from("b.png"))
            );
        }
    }

    #[test]
    fn a_shuffle_of_one_wallpaper_keeps_it() {
        let choices = files(&["a.png"]);
        assert_eq!(
            pick(&choices, Some(Path::new("a.png")), 3),
            Some(PathBuf::from("a.png"))
        );
        assert_eq!(pick(&[], None, 3), None);
    }

    #[test]
    fn an_hourly_shuffle_is_due_an_hour_after_the_last() {
        assert!(!due(WallpaperShuffle::Hourly, Some(1000), 1000 + 3599));
        assert!(due(WallpaperShuffle::Hourly, Some(1000), 1000 + 3600));
    }

    #[test]
    fn a_login_shuffle_is_never_due_while_running() {
        assert!(!due(WallpaperShuffle::Login, Some(0), u64::MAX));
    }
}
