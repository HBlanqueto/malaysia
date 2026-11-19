use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ctappearance::{AppearanceConfig, ini, xdg};
use ctdesign::{ACCENT_SWATCHES, Rgba};
use serde::Deserialize;

use crate::gtk::write_if_changed;
use crate::settings::{gnome_setting, set_gnome_setting};

const THEME: &str = "Malaysia";
const TINTED: &str = "Malaysia-Tinted";
const PALETTE: &str = "folder-palette.json";

#[derive(Deserialize)]
struct Palette {
    icons: Vec<String>,
    light: Colors<String>,
    tinted: Colors<[f32; 2]>,
}

#[derive(Deserialize)]
struct Colors<T> {
    back: [T; 2],
    front: [T; 2],
    glyph: T,
    hi: f64,
}

impl Palette {
    fn replacements(&self, tint: Rgba) -> Vec<(String, String)> {
        let (hue, saturation, _) = tint.to_hsl();
        let shade = |[lightness, share]: [f32; 2]| {
            let [r, g, b, _] =
                Rgba::from_hsl(hue, (saturation * share).min(1.0), lightness, 1.0).to_rgba8();
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let (light, tinted) = (&self.light, &self.tinted);
        let mut pairs: Vec<(String, String)> = light
            .back
            .iter()
            .chain(&light.front)
            .chain([&light.glyph])
            .cloned()
            .zip(
                tinted
                    .back
                    .iter()
                    .chain(&tinted.front)
                    .chain([&tinted.glyph])
                    .map(|spec| shade(*spec)),
            )
            .collect();
        pairs.push((
            format!("stroke-opacity=\"{}\"", light.hi),
            format!("stroke-opacity=\"{}\"", tinted.hi),
        ));
        pairs
    }
}

pub(crate) struct Folders {
    icons: PathBuf,
}

impl Folders {
    pub(crate) fn from_env() -> Option<Self> {
        Some(Self {
            icons: xdg::data_home()?.join("icons"),
        })
    }

    pub(crate) fn sync(&self, appearance: Option<&AppearanceConfig>) {
        let written = match (appearance.and_then(tinted), installed()) {
            (Some((tint, name)), Some(installed)) => {
                self.write(&installed, &name, tint).map(|()| name)
            }
            _ => Ok(THEME.to_string()),
        };
        let result = written.and_then(|theme| {
            let previous = current_theme();
            self.remove_stale(&[theme.as_str(), previous.as_deref().unwrap_or_default()])
        });
        if let Err(error) = result {
            tracing::warn!(%error, icons = %self.icons.display(), "could not update the tinted folders");
        }
    }

    fn write(&self, installed: &Path, name: &str, tint: Rgba) -> io::Result<()> {
        let palette: Palette = serde_json::from_str(&fs::read_to_string(installed.join(PALETTE))?)
            .map_err(io::Error::other)?;
        let pairs = palette.replacements(tint);
        let dir = self.icons.join(name);
        for icon in &palette.icons {
            let svg = fs::read_to_string(installed.join(icon))?;
            let tinted = pairs
                .iter()
                .fold(svg, |svg, (from, to)| svg.replace(from, to));
            write_if_changed(&dir.join(icon), &tinted)?;
        }
        let index = [("Name", name), ("Inherits", THEME), ("Hidden", "true")]
            .into_iter()
            .fold(
                fs::read_to_string(installed.join("index.theme"))?,
                |index, (key, value)| ini::set(&index, "Icon Theme", key, Some(value)),
            );
        write_if_changed(&dir.join("index.theme"), &index)
    }

    fn remove_stale(&self, keep: &[&str]) -> io::Result<()> {
        let Ok(entries) = fs::read_dir(&self.icons) else {
            return Ok(());
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(TINTED) && !keep.contains(&name.as_ref()) {
                fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    }
}

pub(crate) fn theme(appearance: &AppearanceConfig) -> Option<String> {
    tinted(appearance).map(|(_, name)| name)
}

fn tinted(appearance: &AppearanceConfig) -> Option<(Rgba, String)> {
    let [r, g, b] = appearance.icon_color.or(appearance.accent_color)?;
    let tint = Rgba { r, g, b, a: 1.0 };
    let [r, g, b, _] = tint.to_rgba8();
    let blue = ACCENT_SWATCHES[0].1.to_rgba8();
    ([r, g, b] != blue[..3]).then(|| (tint, format!("{TINTED}-{r:02x}{g:02x}{b:02x}")))
}

fn installed() -> Option<PathBuf> {
    xdg::data_home()
        .into_iter()
        .chain(xdg::data_dirs())
        .map(|dir| dir.join("icons").join(THEME))
        .find(|dir| dir.join(PALETTE).is_file())
}

fn current_theme() -> Option<String> {
    let current = gnome_setting("icon-theme")?;
    if current == THEME || current.starts_with(TINTED) {
        set_gnome_setting("icon-theme", None);
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE_JSON: &str = r##"{
        "icons": ["scalable/places/folder.svg"],
        "light": {"back": ["#7ebee8", "#6cb5e1"], "front": ["#87ccf2", "#66b1de"], "glyph": "#5596bc", "hi": 0.65},
        "tinted": {"back": [[0.46, 0.75], [0.34, 0.75]], "front": [[0.66, 0.85], [0.52, 0.85]], "glyph": [0.22, 0.8], "hi": 0.35}
    }"##;

    #[test]
    fn the_folders_take_the_colors_build_py_gives_the_same_tint() {
        let palette: Palette = serde_json::from_str(PALETTE_JSON).unwrap();
        let tint = Rgba::rgb8(0xe8, 0xa3, 0x3d);
        let tinted: Vec<String> = palette
            .replacements(tint)
            .into_iter()
            .map(|(_, to)| to)
            .collect();
        assert_eq!(
            tinted,
            [
                "#bb8330",
                "#8a6123",
                "#e2b46e",
                "#d79433",
                "#5b3f15",
                "stroke-opacity=\"0.35\""
            ]
        );
    }

    #[test]
    fn the_blue_accent_keeps_the_default_folders() {
        let [r, g, b, _] = ACCENT_SWATCHES[0].1.to_rgba8();
        let blue = AppearanceConfig {
            accent_color: Some([r, g, b].map(|c| f32::from(c) / 255.0)),
            ..AppearanceConfig::default()
        };
        assert_eq!(theme(&blue), None);
        assert_eq!(theme(&AppearanceConfig::default()), None);
    }

    #[test]
    fn the_tinted_theme_inherits_malaysia_and_recolors_each_folder() {
        let root = std::env::temp_dir().join(format!("malaysia-folders-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let installed = root.join("Malaysia");
        fs::create_dir_all(installed.join("scalable/places")).unwrap();
        fs::write(installed.join(PALETTE), PALETTE_JSON).unwrap();
        fs::write(
            installed.join("index.theme"),
            "[Icon Theme]\nName=Malaysia\nInherits=Adwaita,hicolor\n",
        )
        .unwrap();
        fs::write(
            installed.join("scalable/places/folder.svg"),
            r##"<stop stop-color="#7ebee8"/><path stroke-opacity="0.65"/>"##,
        )
        .unwrap();
        let folders = Folders {
            icons: root.clone(),
        };
        let name = "Malaysia-Tinted-e8a33d";
        folders
            .write(&installed, name, Rgba::rgb8(0xe8, 0xa3, 0x3d))
            .unwrap();
        let svg = fs::read_to_string(root.join(name).join("scalable/places/folder.svg")).unwrap();
        assert_eq!(
            svg,
            r##"<stop stop-color="#bb8330"/><path stroke-opacity="0.35"/>"##
        );
        let index = fs::read_to_string(root.join(name).join("index.theme")).unwrap();
        assert_eq!(ini::value(&index, "Icon Theme", "Inherits"), Some(THEME));
        assert_eq!(ini::value(&index, "Icon Theme", "Name"), Some(name));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_the_stale_tinted_themes_are_removed() {
        let root = std::env::temp_dir().join(format!("malaysia-stale-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let themes = [
            "Malaysia",
            "Papirus",
            "Malaysia-Tinted",
            "Malaysia-Tinted-804dcc",
            "Malaysia-Tinted-e8a33d",
            "Malaysia-Tinted-35c759",
        ];
        for theme in themes {
            fs::create_dir_all(root.join(theme)).unwrap();
        }
        let folders = Folders {
            icons: root.clone(),
        };
        folders
            .remove_stale(&["Malaysia-Tinted-e8a33d", "Malaysia-Tinted-804dcc"])
            .unwrap();
        let left: Vec<bool> = themes
            .iter()
            .map(|theme| root.join(theme).exists())
            .collect();
        assert_eq!(left, [true, true, false, true, true, false]);
        fs::remove_dir_all(&root).unwrap();
    }
}
