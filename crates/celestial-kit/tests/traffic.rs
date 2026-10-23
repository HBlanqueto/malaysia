//! Pixel checks for the client-side traffic lights (ADR 0035).

use celestial_kit::traffic::traffic_lights_view;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use freya_testing::TestingRunner;
use std::io::Cursor;
use torin::prelude::Size2D;
use yuki_appearance::TrafficLightStyle;
use yuki_appearance::traffic::{TrafficInput, visual};
use yuki_design::{ButtonKind, Rgba};

const LIGHTS: [Rgba; 3] = [
    Rgba::rgb8(255, 95, 87),
    Rgba::rgb8(254, 188, 46),
    Rgba::rgb8(40, 200, 64),
];
const INACTIVE: Rgba = Rgba::rgb8(0xd3, 0xd3, 0xd5);

struct Case {
    input: TrafficInput,
}

impl App for Case {
    fn render(&self) -> impl IntoElement {
        traffic_lights_view(self.input, LIGHTS, TrafficLightStyle::Standard, INACTIVE)
    }
}

fn render(input: TrafficInput, scale: f64) -> (Vec<u8>, usize) {
    let app = AppComponent::new(Case { input });
    let (mut test, _) = TestingRunner::new(app, Size2D::new(80.0, 40.0), |_| {}, scale);
    test.sync_and_update();
    let _ = test.render();
    test.sync_and_update();
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

fn sample(input: TrafficInput, scale: f64) -> [u8; 4] {
    let (image, width) = render(input, scale);
    let x = (6.0 * scale).round() as usize;
    let y = (1.0 * scale).round() as usize;
    pixel(&image, width, x, y)
}

fn compare(actual: [u8; 4], expected: Rgba) {
    let expected = expected.to_rgba8();
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(actual.abs_diff(expected) <= 1, "{actual:?} != {expected:?}");
    }
}

#[test]
fn traffic_lights_match_the_shared_visual_colours() {
    let states = [
        TrafficInput {
            focused: true,
            group_hovered: false,
            pressed: None,
            zoom_supported: true,
        },
        TrafficInput {
            focused: true,
            group_hovered: true,
            pressed: None,
            zoom_supported: true,
        },
        TrafficInput {
            focused: true,
            group_hovered: false,
            pressed: Some(ButtonKind::Close),
            zoom_supported: true,
        },
        TrafficInput {
            focused: false,
            group_hovered: true,
            pressed: None,
            zoom_supported: true,
        },
    ];
    for scale in [1.0, 2.0] {
        for input in states {
            let expected = visual(LIGHTS, TrafficLightStyle::Standard, INACTIVE, input)[0].fill;
            compare(sample(input, scale), expected);
        }
    }
}

#[test]
fn hover_paints_the_close_glyph() {
    let input = TrafficInput {
        focused: true,
        group_hovered: true,
        pressed: None,
        zoom_supported: true,
    };
    let (image, width) = render(input, 1.0);
    let center = pixel(&image, width, 6, 6);
    let fill = pixel(&image, width, 6, 1);
    assert_ne!(center, fill, "the close glyph must cover the centre");
}

#[test]
fn the_shared_module_has_the_same_group_footprint() {
    let (_, _, width, height) = yuki_appearance::traffic::group_rect();
    assert_eq!(width, 3.0 * 12.0 + 2.0 * 8.0);
    assert_eq!(height, 12.0);
    assert_eq!(yuki_appearance::traffic::button_rect(1).2, 12.0);
}
