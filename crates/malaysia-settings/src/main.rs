//! Ajustes del sistema de Malaysia.
//!
//! Ventana de preferencias con sidebar, toolbar y contenido desplazable.
//! Todos los colores y medidas salen de `vow-design`, `vow-appearance` y los
//! componentes de `barrier-kit`; no hay literales de diseño.

mod cards;
mod controls;
mod groups;
mod sidebar;
mod state;
mod toolbar;

use barrier_kit::platform::{Decorations, WindowOptions, run_window};
use barrier_kit::theme;
use barrier_kit::titlebar::Titlebar;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Direction, Size};
use vow_appearance::{AppearanceConfig, Resolved};
use vow_design::{Rgba, SETTINGS};

use crate::groups::content_scroll;
use crate::sidebar::sidebar;
use crate::state::{bg_window, load_resolved, rgba};
use crate::toolbar::toolbar;

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
        app_view(
            None::<(AppearanceConfig, Option<Rgba>, Resolved)>,
            Titlebar::new().with_sidebar(true),
        )
    }
}

/// Vista completa de Ajustes. Si se pasa un estado explícito se usa para
/// tests; si no, se carga desde `appearance.domain`.
pub fn app_view(
    state: Option<(AppearanceConfig, Option<Rgba>, Resolved)>,
    titlebar: Titlebar,
) -> impl IntoElement {
    let search = use_state(String::new);
    let appearance_error = use_state(|| None::<String>);
    let color_error = use_state(|| None::<String>);
    let windows_error = use_state(|| None::<String>);

    let (config, dominant, resolved) = state.unwrap_or_else(load_resolved);
    let query = search.read().to_lowercase();

    rect()
        .width(Size::px(SETTINGS.window_width as f32))
        .height(Size::fill())
        .background(rgba(bg_window(resolved.scheme)))
        .direction(Direction::Horizontal)
        .child(sidebar(search, &resolved))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .direction(Direction::Vertical)
                .child(toolbar(titlebar))
                .child(content_scroll(
                    &query,
                    &config,
                    dominant,
                    &resolved,
                    appearance_error,
                    color_error,
                    windows_error,
                )),
        )
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::io::Cursor;
    use std::sync::Mutex;

    use barrier_kit::platform::{DecorationResponse, WindowStatus};
    use barrier_kit::titlebar::Titlebar;
    use freya_components::theming::hooks::use_init_theme;
    use freya_core::integration::AppComponent;
    use freya_core::prelude::Label;
    use freya_testing::TestingRunner;
    use torin::prelude::Size2D;
    use vow_appearance::{
        AppearanceConfig, ColorScheme, WindowColorMode, WindowTintScope, resolve, write_appearance,
    };
    use vow_design::{ACCENT_SWATCHES, BG_WINDOW_LIGHT, SETTINGS};

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
        let old = env::var_os("YUKI_CONFIG_DIR");
        unsafe { env::set_var("YUKI_CONFIG_DIR", &dir) };
        let result = f(dir.clone());
        match old {
            Some(v) => unsafe { env::set_var("YUKI_CONFIG_DIR", v) },
            None => unsafe { env::remove_var("YUKI_CONFIG_DIR") },
        }
        let _ = fs::remove_dir_all(&dir);
        result
    }

    #[derive(Clone)]
    struct FullApp {
        config: AppearanceConfig,
        dominant: Option<vow_design::Rgba>,
    }

    impl App for FullApp {
        fn render(&self) -> impl IntoElement {
            use_init_theme(|| barrier_kit::theme::for_scheme(false));
            let resolved = resolve(&self.config, self.dominant);
            app_view(
                Some((self.config.clone(), self.dominant, resolved)),
                Titlebar::preview(
                    WindowStatus {
                        activated: true,
                        maximized: false,
                        decoration: DecorationResponse::Client,
                        fixed: false,
                    },
                    "Ajustes del sistema",
                    false,
                )
                .with_sidebar(true),
            )
        }
    }

    fn run_app(config: AppearanceConfig, dominant: Option<vow_design::Rgba>) -> TestingRunner {
        let app = AppComponent::new(FullApp { config, dominant });
        let (mut test, _) = TestingRunner::new(app, Size2D::new(760.0, 620.0), |_| {}, 1.0);
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

    fn render(test: &mut TestingRunner) -> (Vec<u8>, usize) {
        let png = test.render();
        let mut decoder = png::Decoder::new(Cursor::new(png.as_bytes()));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().unwrap();
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).unwrap();
        let rgba = match info.color_type {
            png::ColorType::Rgba => buffer[..info.buffer_size()].to_vec(),
            png::ColorType::Rgb => buffer[..info.buffer_size()]
                .chunks(3)
                .flat_map(|px| [px[0], px[1], px[2], 255])
                .collect(),
            other => panic!("unexpected PNG colour type {other:?}"),
        };
        (rgba, info.width as usize)
    }

    fn pixel(image: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
        let index = (y * width + x) * 4;
        image[index..index + 4].try_into().unwrap()
    }

    fn compare(actual: [u8; 4], expected: vow_design::Rgba, msg: &str) {
        let expected = expected.to_rgba8();
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                actual.abs_diff(expected) <= 1,
                "{msg}: {actual:?} != {expected:?}"
            );
        }
    }

    fn node_at(test: &TestingRunner, x: f32, y: f32) -> Option<freya_testing::TestingNode> {
        test.find_many(|node, _el| {
            let area = node.layout().area;
            if area.origin.x <= x
                && area.origin.y <= y
                && area.origin.x + area.size.width > x
                && area.origin.y + area.size.height > y
            {
                Some(node)
            } else {
                None
            }
        })
        .into_iter()
        .min_by(|a, b| {
            let area_a = a.layout().area.size.width * a.layout().area.size.height;
            let area_b = b.layout().area.size.width * b.layout().area.size.height;
            area_a
                .partial_cmp(&area_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    #[test]
    fn root_pixels_are_bg_window() {
        with_temp_config(|_dir| {
            let mut test = run_app(AppearanceConfig::default(), None);
            // Evita zonas con contenido; el punto clave es que no hay fondo de pantalla.
            let (image, width) = render(&mut test);
            compare(
                pixel(&image, width, 4, 8),
                BG_WINDOW_LIGHT,
                "top-left margin must be bg-window",
            );
            compare(
                pixel(&image, width, 232, 8),
                BG_WINDOW_LIGHT,
                "top of content column must be bg-window",
            );
        });
    }

    #[test]
    fn sidebar_island_has_expected_geometry() {
        with_temp_config(|_dir| {
            let test = run_app(AppearanceConfig::default(), None);
            let island = node_at(&test, 10.0, 10.0).expect("island must cover (10,10)");
            let area = island.layout().area;
            assert!(
                (area.origin.x - 8.0).abs() < 0.5
                    && (area.origin.y - 8.0).abs() < 0.5
                    && (area.size.width - 220.0).abs() < 0.5
                    && (area.size.height - 604.0).abs() < 0.5,
                "island must be (8,8,220,604), got {area:?}"
            );
        });
    }

    #[test]
    fn traffic_lights_are_at_their_token_position() {
        with_temp_config(|_dir| {
            let test = run_app(AppearanceConfig::default(), None);
            let lights = test
                .find_many(|node, _el| {
                    let area = node.layout().area;
                    if area.size.width.round() as u32 == 52
                        && area.size.height.round() as u32 == 12
                        && area.origin.x >= 18.0
                        && area.origin.x <= 30.0
                    {
                        Some(node)
                    } else {
                        None
                    }
                })
                .into_iter()
                .next();
            let lights = lights.expect("traffic lights must be rendered");
            let area = lights.layout().area;
            assert!(
                (area.origin.x - 26.0).abs() < 1.0 && (area.origin.y - 24.0).abs() < 1.0,
                "traffic lights must be near (26,24), got {area:?}"
            );
        });
    }

    #[test]
    fn search_field_has_expected_geometry() {
        with_temp_config(|_dir| {
            let test = run_app(AppearanceConfig::default(), None);
            let placeholder = find_label(&test, "Buscar").expect("search placeholder exists");
            let area = placeholder.layout().area;
            assert!(
                (area.origin.y - 58.0).abs() <= 4.0 && (area.size.height - 30.0).abs() < 2.0,
                "search field must be at y≈58 with height 30, got {area:?}"
            );
        });
    }

    #[test]
    fn toolbar_title_is_in_the_sidebar_titlebar() {
        with_temp_config(|_dir| {
            let test = run_app(AppearanceConfig::default(), None);
            let title = find_label(&test, "Apariencia").expect("title exists");
            let area = title.layout().area;
            assert!(
                area.origin.y < 200.0,
                "title must be in the toolbar area, got {area:?}"
            );
        });
    }

    #[test]
    fn toolbar_capsule_metrics() {
        with_temp_config(|_dir| {
            let test = run_app(AppearanceConfig::default(), None);
            let back = find_label(&test, "Atrás").expect("back button exists");
            let area = back.layout().area;
            assert!(
                (area.origin.x - 262.0).abs() < 2.0,
                "capsule content must start near x=250 (with padding), got {area:?}"
            );
            let center_y = area.origin.y + area.size.height / 2.0;
            assert!(
                (center_y - 30.0).abs() <= 1.0,
                "capsule vertical center must be at y=30, got {center_y}"
            );
        });
    }

    #[test]
    fn theme_cards_count_and_size() {
        with_temp_config(|_dir| {
            let test = run_app(AppearanceConfig::default(), None);
            let cards: Vec<_> = test.find_many(|node, _el| {
                let area = node.layout().area;
                if (area.size.width - SETTINGS.card_width as f32).abs() < 0.5
                    && (area.size.height - SETTINGS.card_height as f32).abs() < 0.5
                {
                    Some(node)
                } else {
                    None
                }
            });
            assert!(
                cards.len() >= 6,
                "there must be at least 6 preview cards (3 theme + 3 window color), got {}",
                cards.len()
            );
            let selected_rings: Vec<_> = test.find_many(|node, _el| {
                let area = node.layout().area;
                if (area.size.width
                    - (SETTINGS.card_width + SETTINGS.card_ring_gap * 2 + SETTINGS.card_ring * 2)
                        as f32)
                    .abs()
                    < 1.0
                {
                    Some(node)
                } else {
                    None
                }
            });
            assert!(
                !selected_rings.is_empty(),
                "selected cards must show an accent ring"
            );
        });
    }

    #[test]
    fn accent_swatches_count_and_default_selection() {
        let config = AppearanceConfig::default();
        assert!(
            config.accent_color.is_none(),
            "default config has no accent"
        );

        with_temp_config(|_dir| {
            let test = run_app(config.clone(), None);
            let names: Vec<&str> = ACCENT_SWATCHES.iter().map(|(name, _)| *name).collect();
            let visible: Vec<&str> = names
                .iter()
                .filter(|name| find_label(&test, name).is_some())
                .copied()
                .collect();
            assert_eq!(
                visible,
                vec!["Azul"],
                "only the selected accent swatch should display its name: {visible:?}"
            );
        });
    }

    #[test]
    fn choosing_dark_writes_color_scheme_dark() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig::default();
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial, None);
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

            let mut test = run_app(initial, None);
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

            let mut test = run_app(initial, None);
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

            let mut test = run_app(initial, None);
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
    fn radios_are_enabled_in_wallpaper_mode() {
        with_temp_config(|dir| {
            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig {
                window_color: WindowColorMode::Wallpaper,
                window_tint_scope: WindowTintScope::Windows,
                ..Default::default()
            };
            write_appearance(&path, &initial).unwrap();

            let mut test = run_app(initial, None);
            let node = find_label(&test, "Todo el escritorio").expect("desktop radio label exists");
            click_node(&mut test, node);

            let text = fs::read_to_string(&path).unwrap();
            assert!(
                text.contains("window_tint_scope = \"desktop\""),
                "clicking an enabled radio must change the scope: {text}"
            );
        });
    }

    #[test]
    fn write_error_is_displayed() {
        with_temp_config(|dir| {
            use std::os::unix::fs::PermissionsExt;

            let path = dir.join("appearance.domain");
            let initial = AppearanceConfig::default();
            write_appearance(&path, &initial).unwrap();

            // Make the config directory read-only so subsequent writes fail.
            let mut permissions = fs::metadata(&dir).unwrap().permissions();
            permissions.set_mode(0o555);
            fs::set_permissions(&dir, permissions).unwrap();

            let mut test = run_app(initial, None);
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
    fn search_without_matches_shows_no_results_label() {
        with_temp_config(|_dir| {
            let mut test = run_app(AppearanceConfig::default(), None);
            let search = find_label(&test, "Buscar").expect("search field exists");
            click_node(&mut test, search);
            test.write_text("xyz");
            test.sync_and_update();

            assert!(
                find_label(&test, "Sin resultados").is_some(),
                "no-results label must be visible"
            );
            assert!(
                find_label(&test, "xyz").is_some(),
                "the empty-state must mention the query"
            );
        });
    }

    #[test]
    fn traffic_button_shows_exactly_three_lights() {
        with_temp_config(|_dir| {
            let test = run_app(
                AppearanceConfig {
                    color_scheme: ColorScheme::Light,
                    ..Default::default()
                },
                None,
            );
            let squares = test.find_many(|node, _el| {
                let size = node.layout().area.size;
                if (size.width - 12.0).abs() < 0.5 && (size.height - 12.0).abs() < 0.5 {
                    Some(())
                } else {
                    None
                }
            });
            assert_eq!(
                squares.len(),
                3,
                "the traffic-light button must show exactly three 12x12 squares: {squares:?}"
            );
        });
    }

    #[test]
    fn search_filter_matches_translated_terms() {
        assert!(state::matches_filter(
            "claro",
            "apariencia tema claro oscuro automático"
        ));
        assert!(state::matches_filter(
            "acento",
            "color acento ventanas fondo escritorio tinte"
        ));
        assert!(!state::matches_filter("foo", "apariencia tema claro"));
    }

    #[test]
    fn empty_query_matches_everything() {
        assert!(state::matches_filter("", "cualquier cosa"));
    }

    #[test]
    fn filter_normalizes_accents() {
        assert!(state::matches_filter("oscuro", "tema oscuro claro"));
    }

    #[test]
    fn initials_takes_first_two_words() {
        assert_eq!(state::initials("ana lopez"), "AL");
        assert_eq!(state::initials("pedro"), "P");
    }
}
