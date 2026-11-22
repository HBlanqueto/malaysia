mod app;
mod apps;
mod bluetooth;
mod collection;
mod crop;
mod desktop;
mod dock;
mod energy;
mod freedesktop;
mod icons;
mod localize;
mod menubar;
mod network;
mod notifications;
mod previews;
mod state;
mod wallpaper;
mod wifi;

use bk::app::{Instance, Launch, launch};
use bk::prelude::*;
use bk::theme;
use ctappearance::{load_appearance, load_dominant};
use ctdesign::SETTINGS;
use freya_core::integration::AppComponent;

use crate::app::settings_window;
use crate::localize::tr;

fn main() {
    let instance = match launch(&bk::titlebar::app_id("settings")) {
        Ok(Launch::Forwarded) => return,
        Ok(Launch::Primary(instance)) => Some(instance),
        Err(_) => None,
    };
    run_window(
        AppComponent::new(SettingsApp { instance }),
        WindowOptions::new(tr!("window-title"), "settings")
            .size((SETTINGS.window_width, SETTINGS.window_height))
            .min_size(Some((SETTINGS.window_width, SETTINGS.window_min_height)))
            .max_size(Some((SETTINGS.window_width, 0))),
    )
    .expect("malaysia-settings");
}

struct SettingsApp {
    instance: Option<Instance>,
}

impl App for SettingsApp {
    fn render(&self) -> impl IntoElement {
        let instance = self.instance.clone();
        use_hook(move || {
            if let Some(instance) = &instance {
                instance.listen();
            }
        });
        theme::use_shell_theme();
        settings_window(&load_appearance(), load_dominant(), &theme::resolved())
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::sync::Mutex;

    use ctappearance::{AppearanceConfig, ColorScheme, resolve, write_appearance};
    use ctdesign::ACCENT_SWATCHES;
    use freya_components::theming::hooks::use_init_theme;
    use freya_core::integration::AppComponent;
    use freya_core::prelude::{Label, State};
    use freya_testing::TestingRunner;
    use torin::prelude::Size2D;

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
            use_init_theme(|| theme::for_scheme(resolved.scheme.is_dark()));
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
        test.set_default_fonts(&[std::borrow::Cow::Borrowed(ctdesign::BODY.family)]);
        test.sync_and_update();
        let appearance =
            find_label(&test, tr!("appearance")).expect("the Appearance pane is listed");
        click_node(&mut test, appearance);
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
    fn the_display_page_opens_from_accessibility_and_writes_invert_colors() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig::default();
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let accessibility =
                find_label(&test, tr!("accessibility")).expect("the Accessibility pane is listed");
            click_node(&mut test, accessibility);
            let display = find_label(&test, tr!("display")).expect("Vision lists Display");
            click_node(&mut test, display);
            let invert = test
                .find_many(|node, el| {
                    (el.accessibility().builder.label() == Some(tr!("invert-colors")))
                        .then_some(node)
                })
                .into_iter()
                .last()
                .expect("the Display page shows Invert colors");
            click_node(&mut test, invert);

            let text = fs::read_to_string(&path).unwrap();
            assert!(
                text.contains("invert_colors = true"),
                "the switch must write invert_colors: {text}"
            );
        });
    }

    #[test]
    fn reduce_transparency_is_a_switch_of_its_own_with_increase_contrast_on() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig {
                contrast: true,
                ..AppearanceConfig::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let accessibility =
                find_label(&test, tr!("accessibility")).expect("the Accessibility pane is listed");
            click_node(&mut test, accessibility);
            let display = find_label(&test, tr!("display")).expect("Vision lists Display");
            click_node(&mut test, display);
            let switch = test
                .find_many(|node, el| {
                    (el.accessibility().builder.label() == Some(tr!("reduce-transparency")))
                        .then_some(node)
                })
                .into_iter()
                .last()
                .expect("the Display page shows Reduce transparency");
            click_node(&mut test, switch);

            let text = fs::read_to_string(&path).unwrap();
            assert!(
                text.contains("reduce_transparency = true") && text.contains("contrast = true"),
                "the switch must write reduce_transparency and keep contrast: {text}"
            );
        });
    }

    #[test]
    fn choosing_dark_writes_color_scheme_dark() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig::default();
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let node = find_label(&test, tr!("scheme-dark")).expect("dark theme card label exists");
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
                .find(|(id, _)| *id == "purple")
                .map(|(_, color)| color)
                .expect("purple swatch exists");
            let initial = AppearanceConfig {
                accent_color: Some([purple.r, purple.g, purple.b]),
                ..Default::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let node = ["Purple", "Morado"]
                .into_iter()
                .find_map(|name| find_label(&test, name))
                .expect("purple accent swatch label exists");
            click_node(&mut test, node);

            let config = ctappearance::load_appearance();
            let accent = config.accent_color.expect("accent color must be written");
            assert!(
                (accent[0] - purple.r).abs() < 0.001
                    && (accent[1] - purple.g).abs() < 0.001
                    && (accent[2] - purple.b).abs() < 0.001,
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
            let node = find_label(&test, tr!("scheme-dark")).expect("dark theme card label exists");
            click_node(&mut test, node);

            let config = ctappearance::load_appearance();
            assert_eq!(
                config.color_scheme,
                ColorScheme::DarkTint,
                "pressing Dark while in dark-tint must keep dark-tint"
            );
        });
    }

    #[test]
    fn the_wallpaper_accent_switch_writes_the_key_and_clears_the_accent() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig {
                accent_color: Some([0.208, 0.518, 0.894]),
                ..Default::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial);
            let node = test
                .find_many(|node, el| {
                    (el.accessibility().builder.label() == Some(tr!("wallpaper-accent")))
                        .then_some(node)
                })
                .into_iter()
                .last()
                .expect("the wallpaper accent switch exists");
            click_node(&mut test, node);

            let text = fs::read_to_string(&path).unwrap();
            assert!(
                text.contains("wallpaper_accent = true"),
                "the switch must enable the wallpaper accent: {text}"
            );
            assert!(
                !text.contains("accent_color"),
                "the switch must clear the explicit accent: {text}"
            );
        });
    }

    #[test]
    fn write_error_is_displayed() {
        with_temp_config(|dir| {
            let initial = AppearanceConfig::default();
            fs::create_dir(dir.join("appearance.domain")).unwrap();

            let mut test = run_app(initial);
            let node = find_label(&test, tr!("scheme-dark")).expect("dark theme card label exists");
            click_node(&mut test, node);

            assert!(
                find_label(&test, tr!("save-error-title")).is_some()
                    || find_label(&test, "Permission denied").is_some()
                    || find_label(&test, "denegado").is_some(),
                "the write error message must be rendered"
            );
        });
    }
}
