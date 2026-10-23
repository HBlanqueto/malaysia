//! Golden images for the CelestialKit menu control.

use std::{
    fs,
    path::{Path, PathBuf},
};

use celestial_kit::menu::{CheckState, MenuItem, MenuView};
use freya_core::integration::AppComponent;
use freya_testing::TestingRunner;
use torin::prelude::Size2D;
use yuki_design::{MENU_DARK, MENU_LIGHT, MenuPalette};

const TOLERANCE: u8 = 3;

fn accent_items() -> Vec<MenuItem> {
    vec![
        MenuItem::Action {
            id: "images".to_string(),
            label: "Imágenes".to_string(),
            shortcut: Some("Ctrl+Shift+G".to_string()),
            enabled: true,
            check: CheckState::None,
        },
        MenuItem::Action {
            id: "videos".to_string(),
            label: "Vídeos".to_string(),
            shortcut: Some("Ctrl+Shift+Z".to_string()),
            enabled: false,
            check: CheckState::None,
        },
    ]
}

fn menu_items() -> Vec<MenuItem> {
    vec![
        // The first selectable item is focused by default, so this renders the
        // highlighted state.
        MenuItem::Action {
            id: "open".to_string(),
            label: "Open".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::None,
        },
        MenuItem::Separator,
        // Normal action with a shortcut.
        MenuItem::Action {
            id: "close".to_string(),
            label: "Close".to_string(),
            shortcut: Some("Ctrl+W".to_string()),
            enabled: true,
            check: CheckState::None,
        },
        MenuItem::Separator,
        // Checkbox and radio marks.
        MenuItem::Action {
            id: "hidden".to_string(),
            label: "Hidden Files".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Checkbox(true),
        },
        MenuItem::Action {
            id: "preview".to_string(),
            label: "Preview".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Checkbox(false),
        },
        MenuItem::Action {
            id: "list".to_string(),
            label: "List".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Radio(true),
        },
        MenuItem::Action {
            id: "icons".to_string(),
            label: "Icons".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Radio(false),
        },
        MenuItem::Separator,
        // Disabled item.
        MenuItem::Action {
            id: "delete".to_string(),
            label: "Delete".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        // Submenu with chevron.
        MenuItem::Submenu {
            label: "More".to_string(),
            enabled: true,
            items: vec![MenuItem::Submenu {
                label: "Level 2".to_string(),
                enabled: true,
                items: vec![MenuItem::Action {
                    id: "deep".to_string(),
                    label: "Deep item".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::None,
                }],
            }],
        },
    ]
}

fn render(dark: bool) -> Vec<u8> {
    let palette = if dark { MENU_DARK } else { MENU_LIGHT };
    let items = menu_items();
    let size = MenuView::preferred_size(&items, palette);
    let app = AppComponent::new(MenuView::new(items, |_| {}, || {}, palette));
    let (mut test, _) = TestingRunner::new(app, Size2D::new(size.0, size.1), |_| {}, 1.0);
    test.sync_and_update();
    test.render().as_ref().to_vec()
}

fn render_accents(dark: bool) -> Vec<u8> {
    let palette = if dark { MENU_DARK } else { MENU_LIGHT };
    let items = accent_items();
    let size = MenuView::preferred_size(&items, palette);
    let app = AppComponent::new(MenuView::new(items, |_| {}, || {}, palette));
    let (mut test, _) = TestingRunner::new(app, Size2D::new(size.0, size.1), |_| {}, 1.0);
    test.sync_and_update();
    test.render().as_ref().to_vec()
}

fn golden_path(dark: bool) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!(
            "menu-view-{}.png",
            if dark { "dark" } else { "light" }
        ))
}

fn golden_path_accents(dark: bool) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!(
            "menu-view-accents-{}.png",
            if dark { "dark" } else { "light" }
        ))
}

fn decode_bytes(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
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
    (info.width, info.height, rgba)
}

fn decode(path: &Path) -> (u32, u32, Vec<u8>) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    decode_bytes(&bytes)
}

/// The right `padding + border_width` columns are outside every row, so only
/// the panel background may appear there. A shortcut or label that does not
/// fit paints into that band; the previous measurer did exactly that.
fn assert_right_padding_is_background(png: &[u8], palette: MenuPalette, label: &str) {
    let (width, height, pixels) = decode_bytes(png);
    let border = palette.border_width.ceil() as u32;
    let padding = palette.padding.ceil() as u32;
    // Torin lays padding inside the border, so the text-free band is the last
    // `padding` columns; the last `border` column is the frame itself.
    let start = width.saturating_sub(padding);
    let end = width.saturating_sub(border);
    let radius = palette.radius.ceil() as u32 + 1;
    let background = palette.bg.to_rgba8();
    let mut offenders = Vec::new();
    for y in radius..height.saturating_sub(radius) {
        for x in start..end {
            let offset = ((y * width + x) * 4) as usize;
            let pixel = &pixels[offset..offset + 4];
            if pixel
                .iter()
                .zip(&background)
                .any(|(actual, expected)| actual.abs_diff(*expected) > TOLERANCE)
            {
                offenders.push((x, y));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{label}: {} text pixels in the right padding (first at {:?})",
        offenders.len(),
        offenders.first()
    );
}

fn compare(golden: &Path, actual_png: &[u8], dark: bool, name: &str) {
    let actual_path = std::env::temp_dir().join(format!(
        "yuki-menu-{name}-{}.png",
        if dark { "dark" } else { "light" }
    ));
    fs::write(&actual_path, actual_png).unwrap();
    let (gw, gh, golden_px) = decode(golden);
    let (aw, ah, actual_px) = decode(&actual_path);
    assert_eq!((gw, gh), (aw, ah), "menu image size changed");
    let bad = golden_px
        .iter()
        .zip(&actual_px)
        .filter(|(a, b)| a.abs_diff(**b) > TOLERANCE)
        .count();
    assert_eq!(
        bad,
        0,
        "menu ({}): {bad} channels differ by more than {TOLERANCE}; actual at {actual_path:?}",
        if dark { "dark" } else { "light" }
    );
}

#[test]
fn menu_matches_its_golden_images() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    for dark in [false, true] {
        let path = golden_path(dark);
        let actual = render(dark);
        if update || !path.exists() {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &actual).unwrap();
        } else {
            compare(&path, &actual, dark, "view");
        }
    }
}

#[test]
fn menu_renders_accents_and_long_shortcuts_without_wrapping() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    for dark in [false, true] {
        let path = golden_path_accents(dark);
        let actual = render_accents(dark);
        if update || !path.exists() {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &actual).unwrap();
        } else {
            compare(&path, &actual, dark, "accents");
        }
    }
}

#[test]
fn menu_text_stays_out_of_the_right_padding() {
    for dark in [false, true] {
        let palette = if dark { MENU_DARK } else { MENU_LIGHT };
        let theme = if dark { "dark" } else { "light" };
        assert_right_padding_is_background(&render(dark), palette, theme);
        assert_right_padding_is_background(&render_accents(dark), palette, theme);
    }
}
