use bk::prelude::CornerRadius;
use bk::style::continuous_corner;
use ctappearance::corner::{concentric, coverage};
use ctdesign::{
    CORNER_NOTIFICATION, CORNER_NOTIFICATION_CENTER, CORNER_SMOOTHING, CORNER_SPOTLIGHT,
    CornerShape, NOTIFICATION_PADDING,
};
use freya_engine::prelude::{
    AlphaType, Color, ColorType, ImageInfo, Paint, RRect, Rect, raster_n32_premul,
};

const TILE: usize = 48;
const BODY: f32 = 120.0;
const OVERSAMPLE: i32 = 4;
const RMS: f32 = 0.02;
const MAX: f32 = 0.2;

fn alpha_of(corner: CornerRadius) -> Vec<f32> {
    let side = BODY as i32 * OVERSAMPLE;
    let mut surface = raster_n32_premul((side, side)).expect("raster surface");
    let rect = Rect::from_xywh(0.0, 0.0, BODY, BODY);
    let rounded = RRect::new_rect_xy(rect, corner.top_left(), corner.top_left());
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::WHITE);
    let canvas = surface.canvas();
    canvas.scale((OVERSAMPLE as f32, OVERSAMPLE as f32));
    canvas.draw_path(&corner.smoothed_path(rounded), &paint);
    let info = ImageInfo::new((side, side), ColorType::RGBA8888, AlphaType::Unpremul, None);
    let mut bytes = vec![0_u8; side as usize * side as usize * 4];
    assert!(surface.read_pixels(&info, &mut bytes, side as usize * 4, (0, 0)));
    let samples = OVERSAMPLE as usize;
    (0..TILE * TILE)
        .map(|index| {
            let (column, row) = (index % TILE * samples, index / TILE * samples);
            let sum: f32 = (0..samples * samples)
                .map(|sample| {
                    let (x, y) = (column + sample % samples, row + sample / samples);
                    f32::from(bytes[(y * side as usize + x) * 4 + 3])
                })
                .sum();
            sum / (255.0 * (samples * samples) as f32)
        })
        .collect()
}

fn rms_and_max(drawn: &[f32], radius: f32) -> (f32, f32) {
    let (mut sum, mut max) = (0.0_f32, 0.0_f32);
    for (index, alpha) in drawn.iter().enumerate() {
        let reference = coverage(
            radius,
            CORNER_SMOOTHING,
            (index % TILE) as f32,
            (index / TILE) as f32,
            0.0,
            0.0,
        );
        sum += (alpha - reference) * (alpha - reference);
        max = max.max((alpha - reference).abs());
    }
    ((sum / drawn.len() as f32).sqrt(), max)
}

fn assert_follows_the_continuous_corner(name: &str, corner: CornerRadius, radius: f32) {
    let drawn = alpha_of(corner);
    let (rms, max) = rms_and_max(&drawn, radius);
    eprintln!("{name}: rms {rms:.4} max {max:.3}");
    assert!(rms < RMS && max < MAX, "{name}: rms {rms} max {max}");
    let (wrong, _) = rms_and_max(&drawn, radius + 2.0);
    assert!(
        wrong > 1.5 * rms,
        "{name}: the radius is fixed by the data: {wrong} against {rms}"
    );
}

fn assert_shape(name: &str, shape: CornerShape) {
    assert_eq!(shape.smoothing, CORNER_SMOOTHING, "{name}");
    assert_follows_the_continuous_corner(name, continuous_corner(shape.fitted), shape.fitted);
}

#[test]
fn a_banner_and_a_notification_center_card_are_drawn_as_their_measured_corner() {
    assert_shape("banner", CORNER_NOTIFICATION);
    assert_shape("notification center", CORNER_NOTIFICATION_CENTER);
}

#[test]
fn the_launcher_panel_is_drawn_as_its_measured_corner() {
    assert_shape("spotlight", CORNER_SPOTLIGHT);
}

#[test]
fn the_thumbnail_of_a_notification_is_concentric_to_its_card() {
    let inner = concentric(
        CORNER_NOTIFICATION.radius,
        NOTIFICATION_PADDING,
        NOTIFICATION_PADDING,
        0.0,
    );
    assert_eq!(inner, CORNER_NOTIFICATION.radius - NOTIFICATION_PADDING);
    assert_follows_the_continuous_corner("thumbnail", continuous_corner(inner), inner);
}
