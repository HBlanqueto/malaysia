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

use barrier_kit::ProbeExt;
use barrier_kit::platform::{Decorations, WindowOptions, run_window};
use barrier_kit::theme;
use barrier_kit::titlebar::Titlebar;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Direction, Size};
use vow_appearance::{AppearanceConfig, Resolved, load_appearance, load_dominant};
use vow_design::{Rgba, SETTINGS};

use crate::groups::content_scroll;
use crate::sidebar::sidebar;
use crate::state::rgba;
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
            Some(Callback::new(|_| std::process::exit(0))),
            Callback::new(|_| {
                barrier_kit::platform::WindowControls::get().minimize();
            }),
            Callback::new(|_| {
                barrier_kit::platform::WindowControls::get().toggle_vertical_maximize();
            }),
        )
    }
}

/// Vista completa de Ajustes. Si se pasa un estado explícito se usa para
/// tests; si no, se lee el `Resolved` reactivo del runner para que la vista
/// se actualice cuando cambie la apariencia.
pub fn app_view(
    state: Option<(AppearanceConfig, Option<Rgba>, Resolved)>,
    titlebar: Titlebar,
    on_close: Option<Callback<(), ()>>,
    on_minimize: Callback<(), ()>,
    on_maximize: Callback<(), ()>,
) -> impl IntoElement {
    let search = use_state(String::new);
    let appearance_error = use_state(|| None::<String>);
    let color_error = use_state(|| None::<String>);
    let windows_error = use_state(|| None::<String>);

    let (config, dominant, resolved) = match state {
        Some((config, dominant, resolved)) => (config, dominant, resolved),
        None => {
            let resolved = barrier_kit::theme::resolved();
            (load_appearance(), load_dominant(), resolved)
        }
    };
    let query = search.read().to_lowercase();

    rect()
        .width(Size::px(SETTINGS.window_width as f32))
        .height(Size::fill())
        .probe("window")
        .background(rgba(resolved.surfaces.window))
        .direction(Direction::Horizontal)
        .child(sidebar(
            search,
            &resolved,
            on_close,
            on_minimize,
            on_maximize,
        ))
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
    use std::collections::BTreeMap;
    use std::env;
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use barrier_kit::menu::{MenuItem, MenuView};
    use serde::Deserialize;

    use barrier_kit::platform::{DecorationResponse, WindowStatus};
    use barrier_kit::titlebar::Titlebar;
    use freya_components::theming::hooks::use_init_theme;
    use freya_core::integration::AppComponent;
    use freya_core::prelude::{Fill, Label, Rect as FreyaRect, State};
    use freya_testing::TestingRunner;
    use torin::prelude::Size2D;
    use vow_appearance::{
        AppearanceConfig, ColorScheme, WindowColorMode, WindowTintScope, resolve, write_appearance,
    };
    use vow_design::{ACCENT_SWATCHES, BG_WINDOW_LIGHT};

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

    #[derive(Clone)]
    struct FullApp {
        config: AppearanceConfig,
        dominant: Option<vow_design::Rgba>,
    }

    impl App for FullApp {
        fn render(&self) -> impl IntoElement {
            let resolved = resolve(&self.config, self.dominant);
            let dark = matches!(resolved.scheme, ColorScheme::Dark | ColorScheme::DarkTint);
            use_init_theme(|| barrier_kit::theme::for_scheme(dark));
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
                None,
                Callback::new(|_| {}),
                Callback::new(|_| {}),
            )
        }
    }

    fn install_geist(test: &mut TestingRunner) {
        test.set_default_fonts(&[std::borrow::Cow::Borrowed("Geist")]);
    }

    fn run_app(config: AppearanceConfig, dominant: Option<vow_design::Rgba>) -> TestingRunner {
        let resolved = resolve(&config, dominant);
        let app = AppComponent::new(FullApp { config, dominant });
        let (mut test, _) = TestingRunner::new(
            app,
            Size2D::new(760.0, 620.0),
            move |runner| runner.provide_root_context(|| State::create(resolved)),
            1.0,
        );
        install_geist(&mut test);
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

    fn find_exact_label(test: &TestingRunner, text: &str) -> Option<freya_testing::TestingNode> {
        test.find_many(|node, el| {
            let matches = Label::try_downcast(el).is_some_and(|label| label.text == text)
                || el.accessibility().builder.label() == Some(text);
            matches.then_some(node)
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
    fn accent_purple_tints_window_and_group_surfaces() {
        let purple = ACCENT_SWATCHES
            .iter()
            .find(|(name, _)| *name == "Morado")
            .map(|(_, color)| *color)
            .expect("purple swatch exists");
        let dark_accent = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            accent_color: Some([purple.r, purple.g, purple.b]),
            window_color: WindowColorMode::Accent,
            ..AppearanceConfig::default()
        };
        let resolved = resolve(&dark_accent, None);

        with_temp_config(|_dir| {
            let mut test = run_app(dark_accent, None);
            let (image, width) = render(&mut test);
            // (700, 600) is in the content margin, away from cards and text.
            let window_pixel = pixel(&image, width, 700, 600);
            compare(
                window_pixel,
                resolved.surfaces.window,
                "window background must use resolved.surfaces.window",
            );
            // A point inside the first card container, below the theme row.
            let group_pixel = pixel(&image, width, 300, 220);
            compare(
                group_pixel,
                resolved.surfaces.surface,
                "group background must use resolved.surfaces.surface",
            );

            let mut neutral_test = run_app(AppearanceConfig::default(), None);
            let neutral_resolved = resolve(&AppearanceConfig::default(), None);
            let (neutral_image, _) = render(&mut neutral_test);
            let neutral_window_pixel = pixel(&neutral_image, width, 700, 600);
            assert_ne!(
                neutral_window_pixel, window_pixel,
                "accent window background must differ from neutral"
            );
            compare(
                neutral_window_pixel,
                neutral_resolved.surfaces.window,
                "neutral window background must match neutral surfaces.window",
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
                .filter(|name| find_exact_label(&test, name).is_some())
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
    fn tint_scope_is_disabled_only_in_neutral_mode() {
        assert!(state::tint_scope_disabled(WindowColorMode::Neutral));
        assert!(!state::tint_scope_disabled(WindowColorMode::Accent));
        assert!(!state::tint_scope_disabled(WindowColorMode::Wallpaper));
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

                let mut test = run_app(initial, None);
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

    const MOCKUP_LAYOUT: &str =
        include_str!("../../../docs/mockups/ajustes-del-sistema/apariencia.layout.json");

    #[derive(Deserialize)]
    struct MockupLayout {
        variants: BTreeMap<String, Vec<MockupNode>>,
    }

    #[derive(Deserialize)]
    struct MockupNode {
        id: String,
        rect: [f32; 4],
        #[serde(default)]
        diff: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        background: Option<String>,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        radius: Option<f32>,
        #[serde(default)]
        font: Option<[f32; 3]>,
    }

    fn mockup_layout() -> MockupLayout {
        serde_json::from_str(MOCKUP_LAYOUT).expect("layout.json must parse")
    }

    fn find_by_author_id(test: &TestingRunner, id: &str) -> Option<freya_testing::TestingNode> {
        test.find_many(|node, el| {
            (el.accessibility().builder.author_id() == Some(id)).then_some(node)
        })
        .into_iter()
        .next()
    }

    fn parse_color(value: &str) -> Option<[f32; 4]> {
        let value = value.trim();
        if let Some(rest) = value
            .strip_prefix("rgba(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let parts: Vec<f32> = rest
                .split(',')
                .map(|part| part.trim().parse())
                .collect::<Result<_, _>>()
                .ok()?;
            return (parts.len() == 4).then_some([parts[0], parts[1], parts[2], parts[3]]);
        }
        if let Some(rest) = value.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<f32> = rest
                .split(',')
                .map(|part| part.trim().parse())
                .collect::<Result<_, _>>()
                .ok()?;
            return (parts.len() == 3).then_some([parts[0], parts[1], parts[2], 1.0]);
        }
        if let Some(rest) = value
            .strip_prefix("color(srgb ")
            .and_then(|s| s.strip_suffix(')'))
        {
            let parts: Vec<f32> = rest
                .split_whitespace()
                .map(|part| part.trim().parse())
                .collect::<Result<_, _>>()
                .ok()?;
            if parts.len() == 3 {
                return Some([parts[0] * 255.0, parts[1] * 255.0, parts[2] * 255.0, 1.0]);
            }
        }
        None
    }

    fn fill_color(fill: Fill) -> Option<[f32; 4]> {
        match fill {
            Fill::Color(color) => Some([
                color.r() as f32,
                color.g() as f32,
                color.b() as f32,
                color.a() as f32 / 255.0,
            ]),
            _ => None,
        }
    }

    fn background_of(el: &dyn freya_core::element::ElementExt) -> Option<[f32; 4]> {
        let rect = FreyaRect::try_downcast(el)?;
        fill_color(rect.style.background.clone())
    }

    fn text_color_of(el: &dyn freya_core::element::ElementExt) -> Option<[f32; 4]> {
        if let Some(label) = Label::try_downcast(el) {
            return label.text_style_data.color.and_then(fill_color);
        }
        let rect = FreyaRect::try_downcast(el)?;
        rect.text_style_data.color.and_then(fill_color)
    }

    fn radius_of(el: &dyn freya_core::element::ElementExt) -> Option<f32> {
        let rect = FreyaRect::try_downcast(el)?;
        Some(rect.style.corner_radius.top_left)
    }

    fn font_of(el: &dyn freya_core::element::ElementExt) -> Option<[f32; 3]> {
        if let Some(label) = Label::try_downcast(el) {
            let size = *label.text_style_data.font_size? as f32;
            let weight = i32::from(label.text_style_data.font_weight?) as f32;
            let line_height = label.line_height? * size;
            return Some([size, line_height, weight]);
        }
        let rect = FreyaRect::try_downcast(el)?;
        let size = *rect.text_style_data.font_size? as f32;
        let weight = i32::from(rect.text_style_data.font_weight?) as f32;
        Some([size, size, weight])
    }

    fn close(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() <= tolerance
    }

    fn compare_color(actual: [f32; 4], expected: [f32; 4]) -> bool {
        close(actual[0], expected[0], 8.0)
            && close(actual[1], expected[1], 8.0)
            && close(actual[2], expected[2], 8.0)
            && close(actual[3], expected[3], 0.05)
    }

    fn type_query(test: &mut TestingRunner, query: &str) {
        let search = find_label(test, "Buscar").expect("search field exists");
        click_node(test, search);
        test.write_text(query);
        test.sync_and_update();
    }

    fn variant_config(variant: &str) -> (AppearanceConfig, Option<&str>, bool) {
        let light = AppearanceConfig {
            color_scheme: ColorScheme::Light,
            ..AppearanceConfig::default()
        };
        match variant {
            "claro" => (light, None, false),
            "claro-morado" => {
                let purple = ACCENT_SWATCHES
                    .iter()
                    .find(|(name, _)| *name == "Morado")
                    .map(|(_, color)| *color)
                    .expect("purple swatch exists");
                (
                    AppearanceConfig {
                        accent_color: Some([purple.r, purple.g, purple.b]),
                        ..light
                    },
                    None,
                    false,
                )
            }
            "oscuro" => (
                AppearanceConfig {
                    color_scheme: ColorScheme::Dark,
                    ..AppearanceConfig::default()
                },
                None,
                false,
            ),
            "busqueda-acento" => (light, Some("acento"), false),
            "sin-resultados" => (light, Some("zzz"), false),
            "semaforos-menu" => (light, None, true),
            other => panic!("unknown mockup variant {other}"),
        }
    }

    fn decode_png(bytes: &[u8]) -> (Vec<u8>, usize, usize) {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
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
        (rgba, info.width as usize, info.height as usize)
    }

    fn write_png(path: &std::path::Path, image: &[u8], width: u32, height: u32) {
        let file = fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(image)
            .unwrap();
    }

    fn reference_png(variant: &str) -> &'static [u8] {
        match variant {
            "claro" => {
                include_bytes!("../../../docs/mockups/ajustes-del-sistema/apariencia.claro.png")
            }
            "claro-morado" => include_bytes!(
                "../../../docs/mockups/ajustes-del-sistema/apariencia.claro-morado.png"
            ),
            "oscuro" => {
                include_bytes!("../../../docs/mockups/ajustes-del-sistema/apariencia.oscuro.png")
            }
            "busqueda-acento" => include_bytes!(
                "../../../docs/mockups/ajustes-del-sistema/apariencia.busqueda-acento.png"
            ),
            "sin-resultados" => include_bytes!(
                "../../../docs/mockups/ajustes-del-sistema/apariencia.sin-resultados.png"
            ),
            "semaforos-menu" => include_bytes!(
                "../../../docs/mockups/ajustes-del-sistema/apariencia.semaforos-menu.png"
            ),
            other => panic!("no reference PNG for {other}"),
        }
    }

    fn inside_rect(x: f32, y: f32, rect: &[f32; 4]) -> bool {
        x >= rect[0] && y >= rect[1] && x < rect[0] + rect[2] && y < rect[1] + rect[3]
    }

    fn compare_pixels(
        variant: &str,
        nodes: &[MockupNode],
        app_png: &[u8],
        out_dir: &std::path::Path,
    ) -> String {
        let (actual, width, height) = decode_png(app_png);
        let (expected, expected_width, expected_height) = decode_png(reference_png(variant));
        assert_eq!(
            (width, height),
            (expected_width, expected_height),
            "{variant}: render size differs from the mockup"
        );

        let mut ignore: Vec<[f32; 4]> = nodes
            .iter()
            .filter(|node| node.diff.as_deref() == Some("ignore"))
            .map(|node| node.rect)
            .collect();
        if variant == "semaforos-menu"
            && let Some(menu) = nodes
                .iter()
                .find(|node| node.id == "group.ventanas.semaforos.menu")
        {
            ignore.push(menu.rect);
        }

        let mut diff_image = vec![0u8; width * height * 4];
        let mut changed = 0usize;
        let mut total_error = 0u64;
        let mut compared = 0usize;
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 4;
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                if ignore.iter().any(|rect| inside_rect(px, py, rect)) {
                    diff_image[index + 3] = 255;
                    continue;
                }
                let mut max_diff = 0u8;
                let mut sum = 0u32;
                for channel in 0..3 {
                    let diff = actual[index + channel].abs_diff(expected[index + channel]);
                    max_diff = max_diff.max(diff);
                    sum += u32::from(diff);
                }
                if max_diff > 8 {
                    changed += 1;
                }
                total_error += u64::from(sum);
                compared += 1;
                diff_image[index] = max_diff;
                diff_image[index + 1] = max_diff;
                diff_image[index + 2] = max_diff;
                diff_image[index + 3] = 255;
            }
        }

        write_png(
            &out_dir.join(format!("{variant}.app.png")),
            &actual,
            width as u32,
            height as u32,
        );
        write_png(
            &out_dir.join(format!("{variant}.diff.png")),
            &diff_image,
            width as u32,
            height as u32,
        );
        let percent = changed as f64 * 100.0 / compared.max(1) as f64;
        let mean = total_error as f64 / (compared.max(1) * 3) as f64;
        println!("{variant}: {percent:.2}% píxeles con canal max > 8; error medio {mean:.3}");
        String::new()
    }

    #[test]
    fn mockup_contract() {
        let layout = mockup_layout();
        let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mockup-diff");
        fs::create_dir_all(&out_dir).unwrap();

        let failures = with_temp_config(|_dir| {
            let mut failures = Vec::new();
            for (variant, nodes) in &layout.variants {
                let (config, query, scroll) = variant_config(variant);
                let mut test = run_app(config, None);
                if let Some(query) = query {
                    type_query(&mut test, query);
                }
                if scroll {
                    test.scroll((400.0, 300.0), (0.0, 300.0));
                    test.sync_and_update();
                }

                for node in nodes {
                    if node.layer.as_deref() == Some("popup")
                        || node.id.starts_with("group.ventanas.semaforos.menu")
                    {
                        continue;
                    }
                    let Some(actual) = find_by_author_id(&test, &node.id) else {
                        failures.push(format!(
                            "{variant} {}: falta (esperado {:?})",
                            node.id, node.rect
                        ));
                        continue;
                    };
                    let area = actual.layout().area;
                    let got = [
                        area.origin.x,
                        area.origin.y,
                        area.size.width,
                        area.size.height,
                    ];
                    let ignored = node.id.starts_with("sidebar.account.");
                    let mut mismatches = Vec::new();
                    for index in 0..4 {
                        if ignored && index == 2 {
                            continue;
                        }
                        let tolerance = if index == 2 && node.text.is_some() {
                            2.0
                        } else {
                            1.0
                        };
                        if (got[index] - node.rect[index]).abs() > tolerance {
                            mismatches.push(format!(
                                "{}: esperado {:.1}, obtenido {:.1}",
                                ["x", "y", "ancho", "alto"][index],
                                node.rect[index],
                                got[index]
                            ));
                        }
                    }
                    if !mismatches.is_empty() {
                        failures.push(format!("{variant} {}: {}", node.id, mismatches.join("; ")));
                    }

                    let element = actual.element();
                    let mut style = Vec::new();
                    if let Some(expected) = node.background.as_deref().and_then(parse_color) {
                        match background_of(element.as_ref()) {
                            Some(got) if compare_color(got, expected) => {}
                            Some(got) => style
                                .push(format!("fondo: esperado {expected:?}, obtenido {got:?}")),
                            None => style.push("fondo no expuesto".to_string()),
                        }
                    }
                    // The search placeholder is drawn by the freya TextField;
                    // its probe is only the text box and exposes no style.
                    let text_style = node.id != "sidebar.search.text";
                    if text_style
                        && let Some(expected) = node.color.as_deref().and_then(parse_color)
                    {
                        let got = text_color_of(element.as_ref()).or_else(|| {
                            node.text
                                .as_ref()
                                .and_then(|text| find_exact_label(&test, text))
                                .and_then(|label| text_color_of(label.element().as_ref()))
                        });
                        match got {
                            Some(got) if compare_color(got, expected) => {}
                            Some(got) => style
                                .push(format!("color: esperado {expected:?}, obtenido {got:?}")),
                            None => style.push("color no expuesto".to_string()),
                        }
                    }
                    if let Some(expected) = node.radius {
                        let area = actual.layout().area;
                        let half = area.size.width.min(area.size.height) / 2.0;
                        let expected = expected.min(half);
                        match radius_of(element.as_ref()) {
                            Some(got) if close(got.min(half), expected, 0.5) => {}
                            Some(got) => style
                                .push(format!("radio: esperado {expected:.1}, obtenido {got:.1}")),
                            None => style.push("radio no expuesto".to_string()),
                        }
                    }
                    if text_style && let Some(expected) = node.font {
                        let got = font_of(element.as_ref()).or_else(|| {
                            node.text
                                .as_ref()
                                .and_then(|text| find_exact_label(&test, text))
                                .and_then(|label| font_of(label.element().as_ref()))
                        });
                        match got {
                            Some(got)
                                if close(got[0], expected[0], 0.0)
                                    && close(got[2], expected[2], 0.0)
                                    && close(got[1], expected[1], 1.0) => {}
                            Some(got) => style
                                .push(format!("fuente: esperado {expected:?}, obtenido {got:?}")),
                            None => style.push("fuente no expuesta".to_string()),
                        }
                    }
                    if !style.is_empty() {
                        failures.push(format!("{variant} {}: {}", node.id, style.join("; ")));
                    }
                }

                let png = test.render();
                compare_pixels(variant, nodes, png.as_bytes(), &out_dir);
            }
            failures
        });

        assert!(
            failures.is_empty(),
            "contrato del mockup incumplido:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn traffic_menu_matches_mockup_items() {
        let layout = mockup_layout();
        let nodes = &layout.variants["semaforos-menu"];
        let menu = nodes
            .iter()
            .find(|node| node.id == "group.ventanas.semaforos.menu")
            .expect("menu rect");
        let expected: Vec<[f32; 4]> = nodes
            .iter()
            .filter(|node| {
                node.id.starts_with("group.ventanas.semaforos.menu.")
                    && node.id != "group.ventanas.semaforos.menu"
            })
            .map(|node| {
                [
                    node.rect[0] - menu.rect[0],
                    node.rect[1] - menu.rect[1],
                    node.rect[2],
                    node.rect[3],
                ]
            })
            .collect();

        let resolved = vow_appearance::resolve(&vow_appearance::load_appearance(), None);
        let items =
            crate::controls::traffic_items(crate::state::TrafficLightStyle::Auto, &resolved, None);
        assert!(
            matches!(items.get(1), Some(MenuItem::Separator)),
            "the separator must follow Automático"
        );
        assert_eq!(
            items.iter().filter_map(MenuItem::label).collect::<Vec<_>>(),
            vec!["Automático", "Clásico", "Acento", "Fondo"]
        );
        for item in &items {
            if let MenuItem::Action { icon, .. } = item {
                assert!(
                    icon.is_some(),
                    "every traffic item must carry a swatch icon"
                );
            }
        }
        let expected_offset = -((vow_design::MENU_POPOVER.padding
            + vow_design::MENU_POPOVER.item_padding_start
            + vow_design::MENU_POPOVER.leading_column)
            .round() as i32);
        assert_eq!(
            crate::controls::traffic_menu_offset(),
            (
                expected_offset,
                -(vow_design::MENU_POPOVER.padding.round() as i32)
            )
        );
        assert_eq!(crate::controls::traffic_menu_offset(), (-27, -5));

        let app = freya_core::integration::AppComponent::new(
            MenuView::new(items, |_| {}, || {})
                .with_surface(barrier_kit::theme::SurfaceKind::Toplevel)
                .with_popover_metrics(),
        );
        let (mut test, _) =
            TestingRunner::new(app, Size2D::new(menu.rect[2], menu.rect[3]), |_| {}, 1.0);
        install_geist(&mut test);
        test.sync_and_update();

        let item_width = menu.rect[2] - 2.0 * vow_design::MENU_POPOVER.padding;
        let mut rows: Vec<[f32; 4]> = test.find_many(|node, _el| {
            let area = node.layout().area;
            let width = area.size.width;
            let height = area.size.height;
            if (width - item_width).abs() <= 1.0
                && (height - vow_design::MENU_POPOVER.item_height).abs() <= 1.0
            {
                Some([
                    area.origin.x,
                    area.origin.y,
                    area.size.width,
                    area.size.height,
                ])
            } else {
                None
            }
        });
        rows.sort_by(|a, b| a[1].partial_cmp(&b[1]).unwrap());

        let mut failures = Vec::new();
        if rows.len() != expected.len() {
            failures.push(format!(
                "se esperaban {} ítems, se encontraron {}",
                expected.len(),
                rows.len()
            ));
        }
        for (index, expected) in expected.iter().enumerate() {
            let Some(actual) = rows.get(index) else {
                failures.push(format!("item {index}: falta"));
                continue;
            };
            for axis in 0..4 {
                if (actual[axis] - expected[axis]).abs() > 1.0 {
                    failures.push(format!(
                        "item {index} {}: esperado {:.1}, obtenido {:.1}",
                        ["x", "y", "ancho", "alto"][axis],
                        expected[axis],
                        actual[axis]
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
