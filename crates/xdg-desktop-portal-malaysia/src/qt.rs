use std::fs;
use std::io;
use std::path::PathBuf;

use ctappearance::{AppearanceConfig, Toolkit, palette, resolve};
use ctdesign::{BODY, Font, MONO, Rgba};
use serde::Deserialize;

use crate::gtk::{is_symlink, set_ini_key, write_if_changed};

const SCHEME_FILE: &str = "malaysia.conf";

#[derive(Debug, Deserialize)]
#[serde(default)]
struct BridgeOptions {
    qt_bridge: bool,
}

impl Default for BridgeOptions {
    fn default() -> Self {
        Self { qt_bridge: true }
    }
}

fn enabled() -> bool {
    fs::read_to_string(ctappearance::appearance_path())
        .ok()
        .and_then(|text| toml::from_str::<BridgeOptions>(&text).ok())
        .is_none_or(|options| options.qt_bridge)
}

pub struct Bridge {
    dir: PathBuf,
}

impl Bridge {
    pub fn from_env() -> Option<Self> {
        let dir = ctappearance::xdg::config_home()?.join("qt6ct");
        Some(Self { dir })
    }

    #[cfg(test)]
    fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn apply(&self, appearance: &AppearanceConfig, dominant: Option<Rgba>) -> io::Result<()> {
        let scheme = self.dir.join("colors").join(SCHEME_FILE);
        write_if_changed(&scheme, &color_scheme(appearance, dominant))?;
        let conf = self.dir.join("qt6ct.conf");
        if is_symlink(&conf) {
            tracing::warn!(path = %conf.display(), "qt6ct.conf is a symlink; the Qt bridge leaves it alone");
            return Ok(());
        }
        let scale = appearance.text_size_for(Toolkit::Qt);
        let mut text = fs::read_to_string(&conf).unwrap_or_default();
        for (section, key, value) in [
            ("Appearance", "style", "Fusion".to_string()),
            ("Appearance", "custom_palette", "true".to_string()),
            (
                "Appearance",
                "color_scheme_path",
                scheme.display().to_string(),
            ),
            ("Fonts", "general", font(BODY, scale)),
            ("Fonts", "fixed", font(MONO, scale)),
        ] {
            text = set_ini_key(&text, section, key, &value);
        }
        write_if_changed(&conf, &text)
    }

    pub fn sync(&self, appearance: Option<&AppearanceConfig>, dominant: Option<Rgba>) {
        if !enabled() {
            return;
        }
        let default = AppearanceConfig::default();
        if let Err(error) = self.apply(appearance.unwrap_or(&default), dominant) {
            tracing::warn!(%error, "Qt bridge could not update qt6ct");
        }
    }
}

// QFont::fromString reads Qt 6's 16 fields: family, points, pixels, hint, weight, flags.
fn font(face: Font, scale: f32) -> String {
    let points = face.size as f32 * 0.75 * scale;
    format!(
        "\"{},{},-1,5,{},0,0,0,0,0,0,0,0,0,0,1\"",
        face.family,
        (points * 100.0).round() / 100.0,
        face.weight
    )
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
    fn the_qt_text_size_scales_the_point_size_and_keeps_other_keys() {
        let dir = temp_dir("fonts");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("qt6ct.conf"), "[Appearance]\nicon_theme=Papirus\n").unwrap();
        let appearance = AppearanceConfig {
            text_sizes: ctappearance::TextSizes {
                qt: Some(2.0),
                ..Default::default()
            },
            ..AppearanceConfig::default()
        };
        Bridge::at(dir.clone()).apply(&appearance, None).unwrap();
        let conf = fs::read_to_string(dir.join("qt6ct.conf")).unwrap();
        assert!(conf.contains("icon_theme=Papirus"), "{conf}");
        assert!(conf.contains("custom_palette=true"), "{conf}");
        assert!(
            conf.contains(&format!("general=\"{},19.5,", BODY.family)),
            "{conf}"
        );
        assert!(dir.join("colors").join(SCHEME_FILE).exists());
    }
}
