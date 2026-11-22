use ctappearance::corner::coverage;
use ctdesign::{
    CORNER_ALERT, CORNER_MENU, CORNER_NOTIFICATION, CORNER_NOTIFICATION_CENTER, CORNER_POPOVER,
    CORNER_SMOOTHING, CORNER_WINDOW, CornerShape, DIALOG, DOCK_RADIUS, WINDOW_CORNER,
};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{Bind, ExportMem, Frame as _, ImportMem, Offscreen, Renderer};
use smithay::utils::{Buffer as BufferCoords, Physical, Rectangle, Scale, Size, Transform};

use crate::blur::Glass;
use crate::effects::{RoundedTextureElement, Shape, compile_rounded_clip};
use crate::test_support::try_test_renderer;
use crate::window_style::{
    PopoverShape, alert_extent, card_corner, card_radius, popover_corner, popover_metrics,
    window_extent,
};

const SIDE: i32 = 200;
const TILE: usize = 48;
const RMS: f32 = 0.02;
const MAX_CAPTURE: f32 = 51.0 / 255.0;
const MAX_STAND_IN: f32 = 0.27;
const SAMPLES: &str = include_str!("../tests/data/corner-samples.csv");

fn sample(name: &str) -> (usize, Vec<f32>) {
    let line = SAMPLES
        .lines()
        .find(|line| line.split(',').next() == Some(name))
        .expect("sample");
    let fields: Vec<&str> = line.split(',').skip(1).collect();
    let size = fields[0].parse().unwrap();
    let alpha = fields[3..]
        .iter()
        .map(|value| value.parse::<f32>().unwrap() / 255.0)
        .collect();
    (size, alpha)
}

fn alpha_of_top_left_corner(
    renderer: &mut GlesRenderer,
    (extent, exponent): (f64, f32),
    height: i32,
) -> Vec<f32> {
    let program = compile_rounded_clip(renderer).expect("the rounded clip compiles");
    let white = vec![255_u8; (SIDE * SIDE * 4) as usize];
    let texture: GlesTexture = renderer
        .import_memory(&white, Fourcc::Abgr8888, (SIDE, SIDE).into(), false)
        .expect("the texture imports");
    let size = Size::<i32, Physical>::from((SIDE, SIDE));
    let area = Rectangle::from_size(size);
    let element = RoundedTextureElement::<GlesRenderer>::from_texture(
        texture,
        Id::new(),
        CommitCounter::default(),
        Rectangle::from_size((f64::from(SIDE), f64::from(SIDE)).into()),
        area,
        Shape {
            rect: Rectangle::new((0, 0).into(), (SIDE, height).into()),
            radius: extent.round() as i32,
            exponent,
        },
        program,
    );
    let mut target = Offscreen::<GlesTexture>::create_buffer(
        renderer,
        Fourcc::Abgr8888,
        Size::from((SIDE, SIDE)),
    )
    .expect("the target is created");
    {
        let mut framebuffer = renderer.bind(&mut target).unwrap();
        let mut frame = renderer
            .render(&mut framebuffer, size, Transform::Normal)
            .unwrap();
        frame.clear([0.0; 4].into(), &[area]).unwrap();
        element
            .draw(
                &mut frame,
                element.src(),
                element.geometry(Scale::from(1.0)),
                &[area],
                &[],
                None,
            )
            .unwrap();
        let _synced = frame.finish().unwrap();
    }
    let framebuffer = renderer.bind(&mut target).unwrap();
    let mapping = renderer
        .copy_framebuffer(
            &framebuffer,
            Rectangle::from_size(Size::<i32, BufferCoords>::from((SIDE, SIDE))),
            Fourcc::Abgr8888,
        )
        .unwrap();
    let bytes = renderer.map_texture(&mapping).unwrap().to_vec();
    (0..TILE * TILE)
        .map(|index| {
            let (column, row) = (index % TILE, index / TILE);
            f32::from(bytes[(row * SIDE as usize + column) * 4 + 3]) / 255.0
        })
        .collect()
}

fn compare(
    drawn: &[f32],
    (columns, rows): (usize, usize),
    reference: impl Fn(usize, usize) -> f32,
) -> (f32, f32) {
    let (mut sum, mut max) = (0.0_f32, 0.0_f32);
    for row in 0..rows {
        for column in 0..columns {
            let error = drawn[row * TILE + column] - reference(column, row);
            sum += error * error;
            max = max.max(error.abs());
        }
    }
    ((sum / (columns * rows) as f32).sqrt(), max)
}

fn continuous(corner: CornerShape, radius: f32) -> impl Fn(usize, usize) -> f32 {
    move |x, y| coverage(radius, corner.smoothing, x as f32, y as f32, 0.0, 0.0)
}

fn rows_clear_of_the_opposite_corner(height: i32, corner: CornerShape) -> usize {
    let tail = (1.0 + corner.smoothing) * corner.fitted;
    TILE.min((height as f32 - tail).floor() as usize)
}

fn assert_follows_the_continuous_corner(
    name: &str,
    shape: (f64, f32),
    height: i32,
    corner: CornerShape,
) {
    let Some(mut renderer) = try_test_renderer() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let drawn = alpha_of_top_left_corner(&mut renderer, shape, height);
    let region = (TILE, rows_clear_of_the_opposite_corner(height, corner));
    let (rms, max) = compare(&drawn, region, continuous(corner, corner.fitted));
    eprintln!("{name}: rms {rms:.4} max {max:.3}");
    assert!(
        rms < RMS && max < MAX_STAND_IN,
        "{name}: rms {rms} max {max}"
    );
    let (wrong, _) = compare(&drawn, region, continuous(corner, corner.fitted + 2.0));
    assert!(
        wrong > 1.5 * rms,
        "{name}: the radius is fixed by the data: {wrong} against {rms}"
    );
}

fn assert_follows_the_capture(name: &str, shape: (f64, f32)) {
    let Some(mut renderer) = try_test_renderer() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let drawn = alpha_of_top_left_corner(&mut renderer, shape, SIDE);
    let (size, alpha) = sample(name);
    let (rms, max) = compare(&drawn, (size, size), |x, y| alpha[y * size + x]);
    eprintln!("{name}: rms {rms:.4} max {max:.4} against the capture");
    assert!(
        rms < RMS && max <= MAX_CAPTURE + 1e-6,
        "{name}: rms {rms} max {max}"
    );
}

fn window() -> (f64, f32) {
    (
        window_extent(None, CORNER_WINDOW.radius as u32),
        WINDOW_CORNER.exponent,
    )
}

#[test]
fn a_window_is_drawn_as_the_macos_window_capture() {
    assert_follows_the_capture("window-16", window());
    assert_follows_the_continuous_corner("window", window(), SIDE, CORNER_WINDOW);
}

#[test]
fn a_sheet_is_drawn_as_the_macos_sheet_capture() {
    assert_follows_the_capture("sheet-16", window());
}

#[test]
fn a_popover_is_drawn_as_its_measured_corner() {
    let shape = popover_corner(
        PopoverShape::Popover,
        popover_metrics(PopoverShape::Popover),
    );
    assert_follows_the_continuous_corner("popover", shape, SIDE, CORNER_POPOVER);
}

#[test]
fn a_menu_is_drawn_as_its_measured_corner() {
    let shape = popover_corner(PopoverShape::Menu, popover_metrics(PopoverShape::Menu));
    assert_follows_the_continuous_corner("menu", shape, SIDE, CORNER_MENU);
}

#[test]
fn an_alert_is_drawn_as_its_frame_corner() {
    let shape = (alert_extent(), DIALOG.superellipse_exponent);
    assert_follows_the_continuous_corner("alert", shape, SIDE, CORNER_ALERT);
}

fn card(glass: Glass, panel_open: bool, height: i32) -> (f64, f32) {
    card_corner(card_radius(glass, panel_open, height), height)
}

#[test]
fn a_notification_banner_is_drawn_as_its_measured_corner() {
    assert_follows_the_continuous_corner(
        "banner",
        card(Glass::Notifications, false, 58),
        58,
        CORNER_NOTIFICATION,
    );
}

#[test]
fn a_notification_center_widget_is_drawn_as_its_measured_corner() {
    assert_follows_the_continuous_corner(
        "notification center",
        card(Glass::Notifications, true, 120),
        120,
        CORNER_NOTIFICATION_CENTER,
    );
}

#[test]
fn the_dock_corner_is_a_share_of_its_thickness_and_follows_the_continuous_corner() {
    for thickness in [64, 84, 112] {
        let measured = CornerShape {
            radius: DOCK_RADIUS * thickness as f32,
            fitted: DOCK_RADIUS * thickness as f32,
            smoothing: CORNER_SMOOTHING,
        };
        assert_follows_the_continuous_corner(
            "dock",
            card(Glass::Dock, false, thickness),
            thickness,
            measured,
        );
    }
}
