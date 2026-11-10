use std::fs;
use std::io;
use std::path::PathBuf;

use ctappearance::{AppearanceConfig, Toolkit, ini, palette, resolve};
use ctdesign::{BODY, Font, MONO, Rgba};

use crate::gtk::{is_symlink, write_if_changed};

// qt5ct and qt6ct watch only their own folder, so the palette lives there and not in colors/.
const SCHEME_FILE: &str = "malaysia-colors.conf";
const ICON_THEMES: [&str; 2] = ["breeze", "breeze-dark"];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Qt {
    Five,
    Six,
}

impl Qt {
    fn tool(self) -> &'static str {
        match self {
            Self::Five => "qt5ct",
            Self::Six => "qt6ct",
        }
    }
}

pub struct Bridge {
    config_home: PathBuf,
}

impl Bridge {
    pub fn from_env() -> Option<Self> {
        let config_home = ctappearance::xdg::config_home()?;
        Some(Self { config_home })
    }

    #[cfg(test)]
    fn at(config_home: PathBuf) -> Self {
        Self { config_home }
    }

    fn apply(&self, appearance: &AppearanceConfig, dominant: Option<Rgba>) -> io::Result<()> {
        let scheme = color_scheme(appearance, dominant);
        let icons = ICON_THEMES[usize::from(appearance.color_scheme.is_dark())];
        for qt in [Qt::Six, Qt::Five] {
            self.apply_to(qt, appearance, &scheme, icons)?;
        }
        Ok(())
    }

    fn apply_to(
        &self,
        qt: Qt,
        appearance: &AppearanceConfig,
        scheme_text: &str,
        icons: &str,
    ) -> io::Result<()> {
        let dir = self.config_home.join(qt.tool());
        let scheme = dir.join(SCHEME_FILE);
        write_if_changed(&scheme, scheme_text)?;
        let conf = dir.join(format!("{}.conf", qt.tool()));
        if is_symlink(&conf) {
            tracing::warn!(path = %conf.display(), "the Qt bridge leaves a symlinked config alone");
            return Ok(());
        }
        let scale = appearance.text_size_for(Toolkit::Qt);
        let mut text = fs::read_to_string(&conf).unwrap_or_default();
        if ini::value(&text, "Appearance", "icon_theme")
            .is_none_or(|theme| ICON_THEMES.contains(&theme))
        {
            text = ini::set(&text, "Appearance", "icon_theme", Some(icons));
        }
        for (section, key, value) in [
            ("Appearance", "style", "Fusion".to_string()),
            ("Appearance", "custom_palette", "true".to_string()),
            (
                "Appearance",
                "color_scheme_path",
                scheme.display().to_string(),
            ),
            ("Fonts", "general", font(qt, BODY, scale)),
            ("Fonts", "fixed", font(qt, MONO, scale)),
        ] {
            text = ini::set(&text, section, key, Some(&value));
        }
        write_if_changed(&conf, &text)
    }

    // Off, Qt goes back to the style's palette; the style, icons and fonts stay as they are.
    fn remove(&self) -> io::Result<()> {
        for qt in [Qt::Six, Qt::Five] {
            let dir = self.config_home.join(qt.tool());
            let conf = dir.join(format!("{}.conf", qt.tool()));
            if !is_symlink(&conf)
                && let Ok(text) = fs::read_to_string(&conf)
                && ini::value(&text, "Appearance", "color_scheme_path")
                    .is_some_and(|path| path.ends_with(SCHEME_FILE))
            {
                let text = ini::set(&text, "Appearance", "custom_palette", None);
                write_if_changed(
                    &conf,
                    &ini::set(&text, "Appearance", "color_scheme_path", None),
                )?;
            }
            match fs::remove_file(dir.join(SCHEME_FILE)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }

    pub fn sync(&self, appearance: Option<&AppearanceConfig>, dominant: Option<Rgba>) {
        let default = AppearanceConfig::default();
        let appearance = appearance.unwrap_or(&default);
        let result = if appearance.qt_bridge {
            self.apply(appearance, dominant)
        } else {
            self.remove()
        };
        if let Err(error) = result {
            tracing::warn!(%error, "Qt bridge could not update qt5ct and qt6ct");
        }
    }
}

// QFont::toString: Qt 6 writes 16 fields with CSS weights, Qt 5 10 fields with its 0–99 weights.
fn font(qt: Qt, face: Font, scale: f32) -> String {
    let points = (face.size as f32 * 0.75 * scale * 100.0).round() / 100.0;
    match qt {
        Qt::Six => format!(
            "\"{},{points},-1,5,{},0,0,0,0,0,0,0,0,0,0,1\"",
            face.family, face.weight
        ),
        Qt::Five => format!(
            "\"{},{points},-1,5,{},0,0,0,0,0\"",
            face.family,
            qt5_weight(face.weight)
        ),
    }
}

fn qt5_weight(weight: u16) -> u16 {
    match weight {
        ..=150 => 0,
        151..=250 => 12,
        251..=350 => 25,
        351..=450 => 50,
        451..=550 => 57,
        551..=650 => 63,
        651..=750 => 75,
        751..=850 => 81,
        _ => 87,
    }
}

// QPalette::ColorRole order, Accent last (Qt 6.6); qt6ct reads it in Qt6CT::loadColorScheme.
fn color_scheme(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> String {
    let resolved = resolve(appearance, dominant);
    let base = palette(resolved.scheme);
    let surfaces = resolved.surfaces;
    let roles = |highlight: Rgba, ink: Rgba| {
        [
            ink,
            surfaces.control,
            surfaces.raised,
            surfaces.surface,
            base.line_strong,
            base.line,
            ink,
            resolved.on_accent,
            ink,
            surfaces.surface,
            surfaces.window,
            base.scrim,
            highlight,
            resolved.on_accent,
            resolved.link,
            resolved.link,
            surfaces.sunken,
            surfaces.window,
            surfaces.raised,
            base.ink_strong,
            base.ink_muted,
            highlight,
        ]
        .map(argb)
        .join(", ")
    };
    format!(
        "[ColorScheme]\nactive_colors={}\ninactive_colors={}\ndisabled_colors={}\n",
        roles(resolved.accent, base.ink_strong),
        roles(base.accent_inactive, base.ink_strong),
        roles(base.accent_inactive, base.ink_faint),
    )
}

fn argb(color: Rgba) -> String {
    let [r, g, b, a] = color.to_rgba8();
    format!("#{a:02x}{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctappearance::ColorScheme;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("malaysia-qt-bridge-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_scheme_lists_every_qt_role_in_each_group() {
        let scheme = color_scheme(&AppearanceConfig::default(), None);
        for group in ["active_colors", "inactive_colors", "disabled_colors"] {
            let line = scheme.lines().find(|line| line.starts_with(group)).unwrap();
            assert_eq!(line.split(", ").count(), 22, "{group}");
        }
    }

    #[test]
    fn dark_and_light_schemes_differ_in_the_window_color() {
        let window = |scheme: ColorScheme| {
            let scheme = color_scheme(
                &AppearanceConfig {
                    color_scheme: scheme,
                    ..AppearanceConfig::default()
                },
                None,
            );
            scheme
                .lines()
                .nth(1)
                .unwrap()
                .split(", ")
                .nth(10)
                .unwrap()
                .to_string()
        };
        assert_ne!(window(ColorScheme::Light), window(ColorScheme::Dark));
    }

    #[test]
    fn icons_follow_the_scheme_unless_the_user_chose_another_theme() {
        let home = temp_dir("icons");
        let dark = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            ..AppearanceConfig::default()
        };
        Bridge::at(home.clone()).apply(&dark, None).unwrap();
        let conf = || fs::read_to_string(home.join("qt6ct/qt6ct.conf")).unwrap();
        assert!(conf().contains("icon_theme=breeze-dark"), "{}", conf());
        Bridge::at(home.clone())
            .apply(&AppearanceConfig::default(), None)
            .unwrap();
        assert!(conf().contains("icon_theme=breeze\n"), "{}", conf());
        let custom = conf().replace("icon_theme=breeze", "icon_theme=Papirus");
        fs::write(home.join("qt6ct/qt6ct.conf"), custom).unwrap();
        Bridge::at(home.clone()).apply(&dark, None).unwrap();
        assert!(conf().contains("icon_theme=Papirus"), "{}", conf());
    }

    #[test]
    fn turning_the_bridge_off_drops_its_palette_and_keeps_the_rest() {
        let home = temp_dir("off");
        let bridge = Bridge::at(home.clone());
        bridge.apply(&AppearanceConfig::default(), None).unwrap();
        let off = AppearanceConfig {
            qt_bridge: false,
            ..AppearanceConfig::default()
        };
        bridge.sync(Some(&off), None);
        let conf = fs::read_to_string(home.join("qt6ct/qt6ct.conf")).unwrap();
        assert!(!conf.contains("color_scheme_path"), "{conf}");
        assert!(!conf.contains("custom_palette"), "{conf}");
        assert!(conf.contains("icon_theme=breeze"), "{conf}");
        assert!(!home.join("qt6ct").join(SCHEME_FILE).exists());
    }

    #[test]
    fn the_qt_text_size_scales_the_point_size_and_keeps_other_keys() {
        let home = temp_dir("fonts");
        let dir = home.join("qt6ct");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("qt6ct.conf"), "[Appearance]\nicon_theme=Papirus\n").unwrap();
        let appearance = AppearanceConfig {
            text_sizes: ctappearance::TextSizes {
                qt: Some(2.0),
                ..Default::default()
            },
            ..AppearanceConfig::default()
        };
        Bridge::at(home.clone()).apply(&appearance, None).unwrap();
        let conf = fs::read_to_string(dir.join("qt6ct.conf")).unwrap();
        assert!(conf.contains("icon_theme=Papirus"), "{conf}");
        assert!(conf.contains("custom_palette=true"), "{conf}");
        assert!(
            conf.contains(&format!("general=\"{},19.5,", BODY.family)),
            "{conf}"
        );
        assert!(dir.join(SCHEME_FILE).exists());
        let qt5 = fs::read_to_string(home.join("qt5ct/qt5ct.conf")).unwrap();
        assert!(
            qt5.contains(&format!(
                "general=\"{},19.5,-1,5,50,0,0,0,0,0\"",
                BODY.family
            )),
            "{qt5}"
        );
    }
}
