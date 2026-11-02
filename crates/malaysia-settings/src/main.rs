//! Malaysia system settings window.

mod app;
mod previews;
mod state;

use barrier_kit::platform::{Decorations, WindowOptions, run_window};
use barrier_kit::theme;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use vow_appearance::{load_appearance, load_dominant};
use vow_design::SETTINGS;

use crate::app::settings_window;

fn main() {
    run_window(
        AppComponent::new(SettingsApp),
        WindowOptions::new("Ajustes del sistema", "settings")
            .decorations(Decorations::Client)
            .size((SETTINGS.window_width, SETTINGS.window_height))
            .min_size(Some((SETTINGS.window_width, SETTINGS.window_min_height)))
            .max_size(Some((SETTINGS.window_width, 0))),
    )
    .expect("malaysia-settings");
}

struct SettingsApp;

impl App for SettingsApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(theme::shell);
        settings_window(&load_appearance(), load_dominant(), &theme::resolved())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::env;
    use std::fs;
    use std::sync::Mutex;

    use freya_components::theming::hooks::use_init_theme;
    use freya_core::integration::AppComponent;
    use freya_core::prelude::{Fill, Label, State};
    use freya_testing::TestingRunner;
    use serde::Deserialize;
    use torin::prelude::Size2D;
    use vow_appearance::{
        AppearanceConfig, ColorScheme, WindowColorMode, WindowTintScope, resolve, write_appearance,
    };
    use vow_design::ACCENT_SWATCHES;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_config<F: FnOnce(std::path::PathBuf) -> R, R>(f: F) -> R {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "malaysia-settings-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let old = env::var_os("MALAYSIA_CONFIG_DIR");
        unsafe { env::set_var("MALAYSIA_CONFIG_DIR", &dir) };
        let result = f(dir.clone());
        match old {
            Some(v) => unsafe { env::set_var("MALAYSIA_CONFIG_DIR", v) },
            None => unsafe { env::remove_var("MALAYSIA_CONFIG_DIR") },
        }
        let _ = fs::remove_dir_all(&dir);
        result
    }

    struct TestApp {
        config: AppearanceConfig,
    }

    impl App for TestApp {
        fn render(&self) -> impl IntoElement {
            let resolved = resolve(&self.config, None);
            let dark = matches!(resolved.scheme, ColorScheme::Dark | ColorScheme::DarkTint);
            use_init_theme(|| theme::for_scheme(dark));
            settings_window(&self.config, None, &resolved)
        }
    }

    fn run_app(config: AppearanceConfig) -> TestingRunner {
        let resolved = resolve(&config, None);
        let (mut test, _) = TestingRunner::new(
            AppComponent::new(TestApp { config }),
            Size2D::new(SETTINGS.window_width as f32, SETTINGS.window_height as f32),
            move |runner| runner.provide_root_context(|| State::create(resolved)),
            1.0,
        );
        test.set_default_fonts(&[std::borrow::Cow::Borrowed("Geist")]);
        test.sync_and_update();
        test
    }

    fn find_label(test: &TestingRunner, text: &str) -> Option<freya_testing::TestingNode> {
        test.find_many(|node, el| {
            if Label::try_downcast(el).is_some_and(|label| label.text.contains(text))
                || el.accessibility().builder.label() == Some(text)
            {
                Some(node)
            } else {
                None
            }
        })
        .into_iter()
        .next()
    }

    fn click_node(test: &mut TestingRunner, node: freya_testing::TestingNode) {
        let layout = node.layout();
        let area = layout.area;
        let x = area.origin.x + area.size.width / 2.0;
        let y = area.origin.y + area.size.height / 2.0;
        test.click_cursor((x as f64, y as f64));
        test.sync_and_update();
    }

    #[test]
    fn choosing_dark_writes_color_scheme_dark() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig::default();
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let node = find_label(&test, "Oscuro").expect("dark theme card label exists");
            click_node(&mut test, node);

            let text = fs::read_to_string(&path).unwrap();
            assert!(
                text.contains("color_scheme = \"dark\""),
                "choosing dark must write the dark scheme: {text}"
            );
        });
    }

    #[test]
    fn choosing_purple_accent_writes_three_decimals() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let purple = ACCENT_SWATCHES
                .iter()
                .find(|(name, _)| *name == "Morado")
                .map(|(_, color)| color)
                .expect("purple swatch exists");
            let initial = AppearanceConfig {
                accent_color: Some([purple.r, purple.g, purple.b]),
                ..Default::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let node = find_label(&test, "Morado").expect("purple accent swatch label exists");
            click_node(&mut test, node);

            let config = vow_appearance::load_appearance();
            let accent = config.accent_color.expect("accent color must be written");
            assert!(
                (accent[0] - 0.557).abs() < 0.001
                    && (accent[1] - 0.357).abs() < 0.001
                    && (accent[2] - 0.847).abs() < 0.001,
                "choosing purple must round to three decimals: {accent:?}"
            );
        });
    }

    #[test]
    fn pressing_dark_preserves_dark_tint() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig {
                color_scheme: ColorScheme::DarkTint,
                ..Default::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let node = find_label(&test, "Oscuro").expect("dark theme card label exists");
            click_node(&mut test, node);

            let config = vow_appearance::load_appearance();
            assert_eq!(
                config.color_scheme,
                ColorScheme::DarkTint,
                "pressing Dark while in dark-tint must keep dark-tint"
            );
        });
    }

    #[test]
    fn tint_scope_radios_are_disabled_in_neutral_mode() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig {
                window_color: WindowColorMode::Neutral,
                window_tint_scope: WindowTintScope::Windows,
                ..Default::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let node = find_label(&test, "Todo el escritorio").expect("desktop radio label exists");
            click_node(&mut test, node);

            let text = fs::read_to_string(&path).unwrap();
            assert!(
                text.contains("window_tint_scope = \"windows\""),
                "clicking a disabled radio must not change the scope: {text}"
            );
        });
    }

    #[test]
    fn radios_are_enabled_in_tinted_modes() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            for mode in [WindowColorMode::Accent, WindowColorMode::Wallpaper] {
                let initial = AppearanceConfig {
                    window_color: mode,
                    window_tint_scope: WindowTintScope::Windows,
                    ..Default::default()
                };
                write_appearance(&path, &initial).unwrap();

                let mut test = run_app(initial);
                let node =
                    find_label(&test, "Todo el escritorio").expect("desktop radio label exists");
                click_node(&mut test, node);

                let text = fs::read_to_string(&path).unwrap();
                assert!(
                    text.contains("window_tint_scope = \"desktop\""),
                    "clicking an enabled radio must change the scope for {mode:?}: {text}"
                );
            }
        });
    }

    #[test]
    fn write_error_is_displayed() {
        with_temp_config(|dir| {
            let initial = AppearanceConfig::default();
            fs::create_dir(dir.join("appearance.domain")).unwrap();

            let mut test = run_app(initial);
            let node = find_label(&test, "Oscuro").expect("dark theme card label exists");
            click_node(&mut test, node);

            assert!(
                find_label(&test, "No se pudo guardar").is_some()
                    || find_label(&test, "Permission denied").is_some()
                    || find_label(&test, "denegado").is_some(),
                "the write error message must be rendered"
            );
        });
    }

    #[test]
    fn initials_takes_first_two_words() {
        assert_eq!(state::initials("ana lopez"), "AL");
        assert_eq!(state::initials("pedro"), "P");
    }

    const MOCKUP: &str =
        include_str!("../../../docs/mockups/ajustes-del-sistema/apariencia.layout.json");

    #[derive(Deserialize)]
    struct Mockup {
        variants: BTreeMap<String, Vec<MockupNode>>,
    }

    #[derive(Deserialize)]
    struct MockupNode {
        id: String,
        rect: [f32; 4],
        text: Option<String>,
        name: Option<String>,
        color: Option<String>,
        font: Option<[f32; 3]>,
        diff: Option<String>,
    }

    struct Found {
        rect: [f32; 4],
        color: Option<[u8; 3]>,
        font: Option<[f32; 2]>,
    }

    fn variant_config(variant: &str) -> (AppearanceConfig, Option<&'static str>) {
        let light = AppearanceConfig {
            color_scheme: ColorScheme::Light,
            ..Default::default()
        };
        match variant {
            "claro" => (light, None),
            "oscuro" => (
                AppearanceConfig {
                    color_scheme: ColorScheme::Dark,
                    ..Default::default()
                },
                None,
            ),
            "claro-morado" => {
                let purple = ACCENT_SWATCHES[1].1;
                (
                    AppearanceConfig {
                        accent_color: Some([purple.r, purple.g, purple.b]),
                        ..light
                    },
                    None,
                )
            }
            "busqueda-acento" => (light, Some("acento")),
            other => panic!("variante sin configuración: {other}"),
        }
    }

    fn parse_rgb(css: &str) -> Option<[u8; 3]> {
        let inner = css.split_once('(')?.1.trim_end_matches(')');
        let mut parts = inner.split(',').map(|part| part.trim().parse::<f32>().ok());
        Some([
            parts.next()?? as u8,
            parts.next()?? as u8,
            parts.next()?? as u8,
        ])
    }

    fn find(test: &TestingRunner, node: &MockupNode) -> Vec<Found> {
        test.find_many(|found, el| {
            let area = found.layout().area;
            let rect = [
                area.origin.x,
                area.origin.y,
                area.size.width,
                area.size.height,
            ];
            if let Some(label) = Label::try_downcast(el) {
                if node.text.as_deref() != Some(label.text.as_ref()) {
                    return None;
                }
                let style = &label.text_style_data;
                let color = match style.color.clone() {
                    Some(Fill::Color(color)) => Some([color.r(), color.g(), color.b()]),
                    _ => None,
                };
                let font = style
                    .font_size
                    .zip(style.font_weight)
                    .map(|(size, weight)| [*size as f32, i32::from(weight) as f32]);
                return Some(Found { rect, color, font });
            }
            (node.text.is_none() && el.accessibility().builder.label() == node.name.as_deref())
                .then_some(Found {
                    rect,
                    color: None,
                    font: None,
                })
        })
    }

    fn mismatches(node: &MockupNode, found: &Found) -> Vec<String> {
        let mut problems = Vec::new();
        for (index, axis) in ["x", "y", "ancho", "alto"].iter().enumerate() {
            let tolerance = if index == 2 && node.text.is_some() {
                2.0
            } else {
                1.0
            };
            if (found.rect[index] - node.rect[index]).abs() > tolerance {
                problems.push(format!(
                    "{axis}: esperado {}, obtenido {:.1}",
                    node.rect[index], found.rect[index]
                ));
            }
        }
        if let (Some(expected), Some(got)) =
            (node.color.as_deref().and_then(parse_rgb), found.color)
            && expected.iter().zip(got).any(|(a, b)| a.abs_diff(b) > 2)
        {
            problems.push(format!("color: esperado {expected:?}, obtenido {got:?}"));
        }
        if let (Some(expected), Some(got)) = (node.font, found.font)
            && (expected[0] != got[0] || expected[2] != got[1])
        {
            problems.push(format!("fuente: esperado {expected:?}, obtenido {got:?}"));
        }
        problems
    }

    #[test]
    fn the_settings_window_matches_its_mockup() {
        let mockup: Mockup = serde_json::from_str(MOCKUP).expect("layout.json válido");
        let mut failures = Vec::new();
        for (variant, nodes) in &mockup.variants {
            let (config, query) = variant_config(variant);
            let mut test = run_app(config);
            if let Some(query) = query {
                let search = find_label(&test, "Buscar").expect("hay buscador");
                click_node(&mut test, search);
                test.write_text(query);
                test.sync_and_update();
            }
            for node in nodes {
                if node.diff.is_some() || (node.text.is_none() && node.name.is_none()) {
                    continue;
                }
                let best = find(&test, node)
                    .into_iter()
                    .map(|found| (mismatches(node, &found), found))
                    .min_by_key(|(problems, _)| problems.len());
                match best {
                    None => failures.push(format!("{variant} {}: no está", node.id)),
                    Some((problems, _)) if !problems.is_empty() => {
                        failures.push(format!("{variant} {}: {}", node.id, problems.join("; ")))
                    }
                    Some(_) => {}
                }
            }
        }
        assert!(
            failures.is_empty(),
            "Ajustes no cumple su mockup:\n{}",
            failures.join("\n")
        );
    }
}
