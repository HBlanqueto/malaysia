mod app;
mod apps;
mod bluetooth;
mod crop;
mod energy;
mod freedesktop;
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

    use ctappearance::{
        AppearanceConfig, ColorScheme, WindowColorMode, WindowTintScope, resolve, write_appearance,
    };
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
        // SAFETY: `ENV_LOCK` serialises every test that touches the environment.
        unsafe { env::set_var("MALAYSIA_CONFIG_DIR", &dir) };
        let result = f(dir.clone());
        // SAFETY: `ENV_LOCK` serialises every test that touches the environment.
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
        test.set_default_fonts(&[std::borrow::Cow::Borrowed("Geist")]);
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
            let node =
                find_label(&test, tr!("tint-scope-desktop")).expect("desktop radio label exists");
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
            {
                let mode = WindowColorMode::Accent;
                let initial = AppearanceConfig {
                    window_color: mode,
                    window_tint_scope: WindowTintScope::Windows,
                    ..Default::default()
                };
                write_appearance(&path, &initial).unwrap();

                let mut test = run_app(initial);
                let node = find_label(&test, tr!("tint-scope-desktop"))
                    .expect("desktop radio label exists");
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

    #[test]
    fn initials_takes_first_two_words() {
        assert_eq!(state::initials("ana lopez"), "AL");
        assert_eq!(state::initials("pedro"), "P");
    }
}
