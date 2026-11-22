use ctappearance::ColorScheme;
use ctappearance::material::{Material, blur_region, looks, over};
use ctdesign::{MaterialLook, ShellGlass};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, ExportMem, ImportMem, Texture};
use smithay::utils::{Buffer as BufferCoords, Physical, Rectangle, Size};

use super::gaussian::{Backdrop, Programs, compile, low_sigma, reduction_for};
use super::look::{Look, Post};
use crate::test_support::try_test_renderer;

const MATERIALS: [Material; 14] = [
    Material::Titlebar,
    Material::Selection,
    Material::Menu,
    Material::Popover,
    Material::Sidebar,
    Material::HeaderView,
    Material::Sheet,
    Material::WindowBackground,
    Material::HudWindow,
    Material::FullScreenUi,
    Material::ToolTip,
    Material::ContentBackground,
    Material::UnderWindowBackground,
    Material::UnderPageBackground,
];
const MATERIAL_SAMPLES: &str = include_str!("../../tests/data/material-samples.csv");
const SHELL_SAMPLES: &str = include_str!("../../tests/data/shell-glass-samples.csv");
const EDGE_AT: [i32; 7] = [-40, -20, -10, 0, 10, 20, 40];
const MOST_ERROR: f32 = 2.0;
const MOST_RMS: f32 = 0.8;

struct Bench {
    renderer: GlesRenderer,
    programs: Programs,
}

impl Bench {
    fn new() -> Option<Self> {
        let mut renderer = try_test_renderer()?;
        let programs = compile(&mut renderer)?;
        Some(Self { renderer, programs })
    }

    fn blur(
        &mut self,
        (width, height): (i32, i32),
        pixels: &[u8],
        region: Rectangle<i32, Physical>,
        look: Look,
        scale: f64,
    ) -> Vec<u8> {
        let source = self
            .renderer
            .import_memory(
                pixels,
                Fourcc::Abgr8888,
                Size::<i32, BufferCoords>::from((width, height)),
                false,
            )
            .unwrap();
        let mut backdrop = Backdrop::new(&mut self.renderer, source.size(), &look, scale).unwrap();
        backdrop
            .run(
                &mut self.renderer,
                &self.programs,
                &source,
                region,
                &look,
                scale,
            )
            .unwrap();
        self.read(backdrop.result())
    }

    fn read(&mut self, texture: &GlesTexture) -> Vec<u8> {
        let mut texture = texture.clone();
        let size = texture.size();
        let target = self.renderer.bind(&mut texture).unwrap();
        let mapping = self
            .renderer
            .copy_framebuffer(&target, Rectangle::from_size(size), Fourcc::Abgr8888)
            .unwrap();
        self.renderer.map_texture(&mapping).unwrap().to_vec()
    }
}

fn whole(width: i32, height: i32) -> Rectangle<i32, Physical> {
    Rectangle::from_size(Size::from((width, height)))
}

fn material_look(material: Material, dark: bool) -> MaterialLook {
    let scheme = if dark {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    };
    looks(material, scheme).active
}

fn material(look: MaterialLook) -> Look {
    Look {
        sigma: look.sigma,
        halo: None,
        post: Post::Material(look),
    }
}

fn plain(sigma: f32) -> Look {
    Look {
        sigma,
        halo: None,
        post: Post::Plain,
    }
}

fn gray(levels: impl Iterator<Item = f32>) -> Vec<u8> {
    levels
        .flat_map(|level| {
            let byte = (level * 255.0).round() as u8;
            [byte, byte, byte, 255]
        })
        .collect()
}

fn rows(kind: &'static str) -> impl Iterator<Item = (usize, bool, Vec<u8>)> {
    MATERIAL_SAMPLES.lines().filter_map(move |line| {
        let mut fields = line.split(',');
        (fields.next()? == kind).then(|| {
            let numbers: Vec<u8> = fields.map(|field| field.parse().unwrap()).collect();
            (
                usize::from(numbers[0]),
                numbers[1] == 1,
                numbers[2..].to_vec(),
            )
        })
    })
}

struct Errors {
    worst: f32,
    squares: f32,
    count: usize,
}

impl Errors {
    fn new() -> Self {
        Self {
            worst: 0.0,
            squares: 0.0,
            count: 0,
        }
    }

    fn add(&mut self, error: f32) {
        self.worst = self.worst.max(error.abs());
        self.squares += error * error;
        self.count += 1;
    }

    fn rms(&self) -> f32 {
        (self.squares / self.count.max(1) as f32).sqrt()
    }
}

fn column_at(pixels: &[u8], width: i32, at: (i32, i32)) -> [u8; 3] {
    let index = ((at.1 * width + at.0) * 4) as usize;
    [pixels[index], pixels[index + 1], pixels[index + 2]]
}

#[test]
fn the_downsample_factor_and_the_low_resolution_sigma_follow_the_measured_kernel() {
    assert_eq!(reduction_for(27.6), 8);
    assert_eq!(reduction_for(59.2), 8);
    assert_eq!(reduction_for(400.0), 8);
    assert_eq!(reduction_for(9.0), 2);
    assert_eq!(reduction_for(3.5), 1);
    assert_eq!(reduction_for(2.0), 1);
    assert!((low_sigma(27.6, 8) - 3.41).abs() < 0.01);
    assert_eq!(low_sigma(3.5, 1), 3.5);
}

#[test]
fn the_blurred_step_matches_the_reference_gaussian_at_every_measured_sigma() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    for sigma in [27.6, 59.2, 9.0, 3.5, 2.0] {
        let width = 400;
        let line: Vec<f32> = (0..width)
            .map(|x| if x < width / 2 { 1.0 } else { 0.0 })
            .collect();
        let row = gray(line.iter().copied());
        let pixels: Vec<u8> = (0..8).flat_map(|_| row.clone()).collect();
        let shown = bench.blur((width, 8), &pixels, whole(width, 8), plain(sigma), 1.0);
        let expected = blur_region(&line, sigma);
        let mut errors = Errors::new();
        for x in 0..width {
            let got = f32::from(column_at(&shown, width, (x, 4))[1]);
            errors.add(got - expected[x as usize] * 255.0);
        }
        eprintln!(
            "step sigma {sigma}: worst {:.2} rms {:.2}",
            errors.worst,
            errors.rms()
        );
        assert!(
            errors.worst <= MOST_ERROR,
            "sigma {sigma}: {}",
            errors.worst
        );
        assert!(errors.rms() <= MOST_RMS, "sigma {sigma}: {}", errors.rms());
    }
}

#[test]
fn every_material_shows_the_gray_ramp_of_the_captures_within_the_parity_tolerance() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let ramp = || (0..1024).map(|row| (row / 4) as f32 / 255.0);
    let mut checked = 0;
    for (index, material_id) in MATERIALS.into_iter().enumerate() {
        for dark in [false, true] {
            let look = material_look(material_id, dark);
            let samples: Vec<_> = rows("ramp")
                .filter(|(at, shade, levels)| {
                    *at == index
                        && *shade == dark
                        && levels[0] == levels[1]
                        && levels[1] == levels[2]
                })
                .collect();
            if samples.is_empty() {
                continue;
            }
            let strip: Vec<u8> = ramp()
                .flat_map(|level| gray(std::iter::repeat_n(level, 40)))
                .collect();
            let shown = bench.blur((40, 1024), &strip, whole(40, 1024), material(look), 1.0);
            let mut errors = Errors::new();
            for (_, _, levels) in &samples {
                let at = (20, i32::from(levels[0]) * 4 + 2);
                let got = column_at(&shown, 40, at);
                for channel in 0..3 {
                    errors.add(f32::from(got[channel]) - f32::from(levels[3 + channel]));
                }
                checked += 1;
            }
            eprintln!(
                "{material_id:?} dark {dark}: {} samples, worst {:.2} rms {:.2}",
                samples.len(),
                errors.worst,
                errors.rms()
            );
            assert!(
                errors.worst <= MOST_ERROR,
                "{material_id:?} dark {dark}: worst {}",
                errors.worst
            );
            assert!(
                errors.rms() <= MOST_RMS,
                "{material_id:?} dark {dark}: rms {}",
                errors.rms()
            );
        }
    }
    assert!(checked > 100, "only {checked} gray samples checked");
}

#[test]
fn every_material_shows_the_colour_samples_of_the_captures_within_the_parity_tolerance() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let mut errors = Errors::new();
    let mut checked = 0;
    for (index, dark, levels) in rows("ramp") {
        if levels[0] == levels[1] && levels[1] == levels[2] {
            continue;
        }
        let look = material_look(MATERIALS[index], dark);
        let tile: Vec<u8> = (0..64)
            .flat_map(|_| [levels[0], levels[1], levels[2], 255])
            .collect();
        let shown = bench.blur((8, 8), &tile, whole(8, 8), material(look), 1.0);
        let got = column_at(&shown, 8, (4, 4));
        for channel in 0..3 {
            errors.add(f32::from(got[channel]) - f32::from(levels[3 + channel]));
        }
        checked += 1;
    }
    eprintln!(
        "{checked} colour samples: worst {:.2} rms {:.2}",
        errors.worst,
        errors.rms()
    );
    assert!(checked > 10);
    assert!(errors.worst <= MOST_ERROR, "worst {}", errors.worst);
    assert!(errors.rms() <= MOST_RMS, "rms {}", errors.rms());
}

#[test]
fn every_material_shows_the_white_and_black_step_of_the_captures_within_the_parity_tolerance() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let mut checked = 0;
    for (index, dark, levels) in rows("step") {
        let look = material_look(MATERIALS[index], dark);
        let (left, right) = (f32::from(levels[0]) / 255.0, f32::from(levels[1]) / 255.0);
        let width = 200;
        let line: Vec<f32> = (0..width)
            .map(|x| if x < width / 2 { left } else { right })
            .collect();
        let row = gray(line.iter().copied());
        let pixels: Vec<u8> = (0..8).flat_map(|_| row.clone()).collect();
        let shown = bench.blur((width, 8), &pixels, whole(width, 8), material(look), 1.0);
        let mut errors = Errors::new();
        for (at, measured) in EDGE_AT.iter().zip(&levels[2..]) {
            let got = column_at(&shown, width, (width / 2 + at, 4))[1];
            errors.add(f32::from(got) - f32::from(*measured));
        }
        eprintln!(
            "{:?} dark {dark} step: worst {:.2} rms {:.2}",
            MATERIALS[index],
            errors.worst,
            errors.rms()
        );
        assert!(
            errors.worst <= MOST_ERROR,
            "{:?} dark {dark}",
            MATERIALS[index]
        );
        assert!(
            errors.rms() <= MOST_RMS,
            "{:?} dark {dark}",
            MATERIALS[index]
        );
        checked += 1;
    }
    assert_eq!(checked, rows("step").count());
}

#[test]
fn the_solids_of_reduce_transparency_are_one_colour_over_any_backdrop() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let mut checked = 0;
    for (index, dark, levels) in rows("reduced") {
        let set = looks(
            MATERIALS[index],
            if dark {
                ColorScheme::Dark
            } else {
                ColorScheme::Light
            },
        );
        for (look, measured) in [
            (set.reduced, &levels[..3]),
            (set.reduced_contrast, &levels[3..]),
        ] {
            assert_eq!(look.sigma, 0.0);
            for backdrop in [0u8, 128, 255] {
                let tile: Vec<u8> = (0..64)
                    .flat_map(|_| [backdrop, backdrop, backdrop, 255])
                    .collect();
                let shown = bench.blur((8, 8), &tile, whole(8, 8), material(look), 1.0);
                let got = column_at(&shown, 8, (4, 4));
                for channel in 0..3 {
                    let error = (f32::from(got[channel]) - f32::from(measured[channel])).abs();
                    assert!(
                        error <= 1.0,
                        "{:?} dark {dark} over {backdrop}: {error}",
                        MATERIALS[index]
                    );
                }
            }
            checked += 1;
        }
    }
    assert!(checked > 20);
}

fn shell_glass(token: &str) -> Look {
    use ctdesign::{
        SHELL_GLASS_APPS_DARK, SHELL_GLASS_CONTROL_CENTER_DARK,
        SHELL_GLASS_CONTROL_CENTER_PANEL_DARK, SHELL_GLASS_DOCK_DARK, SHELL_GLASS_MENU_DARK,
        SHELL_GLASS_NOTIFICATION_CARD_DARK, SHELL_GLASS_SPACES_BAR_DARK,
        SHELL_GLASS_SPOTLIGHT_DARK,
    };
    let glass: ShellGlass = match token {
        "menu" => SHELL_GLASS_MENU_DARK,
        "spotlight" => SHELL_GLASS_SPOTLIGHT_DARK,
        "apps" => SHELL_GLASS_APPS_DARK,
        "notification-card" => SHELL_GLASS_NOTIFICATION_CARD_DARK,
        "control-center" => SHELL_GLASS_CONTROL_CENTER_DARK,
        "control-center-panel" => SHELL_GLASS_CONTROL_CENTER_PANEL_DARK,
        "dock" => SHELL_GLASS_DOCK_DARK,
        "spaces-bar" => return material(SHELL_GLASS_SPACES_BAR_DARK),
        other => panic!("no glass `{other}`"),
    };
    Look {
        sigma: glass.sigma,
        halo: (glass.halo_weight > 0.0).then_some((glass.halo_sigma, glass.halo_weight)),
        post: Post::Shell(glass),
    }
}

#[test]
fn every_measured_gray_and_colour_of_the_shell_glass_comes_out_within_two_levels_unless_a_channel_clips()
 {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let mut checked = 0;
    for line in SHELL_SAMPLES.lines() {
        let mut fields = line.split(',');
        let token = fields.next().unwrap();
        let numbers: Vec<u8> = fields.skip(1).map(|field| field.parse().unwrap()).collect();
        let tile: Vec<u8> = (0..64)
            .flat_map(|_| [numbers[0], numbers[1], numbers[2], 255])
            .collect();
        let shown = bench.blur((8, 8), &tile, whole(8, 8), shell_glass(token), 1.0);
        let got = column_at(&shown, 8, (4, 4));
        let tolerance = if token == "spaces-bar" { 1.0 } else { 2.0 };
        for channel in 0..3 {
            let measured = numbers[3 + channel];
            if (3..=252).contains(&measured) {
                let error = (f32::from(got[channel]) - f32::from(measured)).abs();
                assert!(
                    error <= tolerance + 0.5,
                    "{line}: channel {channel} is {}",
                    got[channel]
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 700, "only {checked} channels checked");
}

#[test]
fn a_region_sees_only_itself_and_keeps_the_plateau_next_to_a_border_step() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let (width, height) = (320, 64);
    let region = Rectangle::<i32, Physical>::new((60, 0).into(), (200, height).into());
    let pixels: Vec<u8> = (0..height)
        .flat_map(|_| {
            (0..width).flat_map(|x| {
                let level = if (60..260).contains(&x) { 255 } else { 0 };
                [level, level, level, 255]
            })
        })
        .collect();
    for sigma in [27.6, 9.0, 3.5] {
        let shown = bench.blur((width, height), &pixels, region, plain(sigma), 1.0);
        let mut worst: f32 = 0.0;
        for x in 60..260 {
            worst = worst.max((255.0 - f32::from(column_at(&shown, width, (x, 32))[1])).abs());
        }
        eprintln!("border step sigma {sigma}: worst {worst:.2}");
        assert!(
            worst <= 1.0,
            "sigma {sigma}: the region leaks {worst} levels"
        );
    }
}

#[test]
fn the_halo_of_a_glass_adds_the_wide_blur_at_its_weight() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let width = 1600;
    let line: Vec<f32> = (0..width)
        .map(|x| if x < width / 2 { 1.0 } else { 0.0 })
        .collect();
    let row = gray(line.iter().copied());
    let pixels: Vec<u8> = (0..8).flat_map(|_| row.clone()).collect();
    let look = Look {
        sigma: 9.0,
        halo: Some((128.0, 0.2)),
        post: Post::Plain,
    };
    let shown = bench.blur((width, 8), &pixels, whole(width, 8), look, 1.0);
    let core = blur_region(&line, 9.0);
    let halo = blur_region(&line, 128.0);
    let mut errors = Errors::new();
    for x in 0..width {
        let expected = 0.8 * core[x as usize] + 0.2 * halo[x as usize];
        errors.add(f32::from(column_at(&shown, width, (x, 4))[1]) - expected * 255.0);
    }
    eprintln!("halo: worst {:.2} rms {:.2}", errors.worst, errors.rms());
    assert!(errors.worst <= MOST_ERROR, "worst {}", errors.worst);
    assert!(errors.rms() <= MOST_RMS, "rms {}", errors.rms());
}

#[test]
fn the_physical_sigma_grows_with_the_output_scale_so_the_logical_radius_is_kept() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let line = |width: i32| -> Vec<f32> {
        (0..width)
            .map(|x| if x < width / 2 { 1.0 } else { 0.0 })
            .collect()
    };
    let width = 800;
    let row = gray(line(width).into_iter());
    let pixels: Vec<u8> = (0..8).flat_map(|_| row.clone()).collect();
    let shown = bench.blur((width, 8), &pixels, whole(width, 8), plain(27.6), 2.0);
    let expected = blur_region(&line(width), 55.2);
    let mut errors = Errors::new();
    for x in 0..width {
        errors.add(f32::from(column_at(&shown, width, (x, 4))[1]) - expected[x as usize] * 255.0);
    }
    eprintln!("scale 2: worst {:.2} rms {:.2}", errors.worst, errors.rms());
    assert!(errors.worst <= MOST_ERROR, "worst {}", errors.worst);
}

#[test]
fn the_reference_function_and_the_pipeline_agree_on_a_flat_backdrop() {
    let Some(mut bench) = Bench::new() else {
        eprintln!("skipping: no EGL device");
        return;
    };
    let look = material_look(Material::Menu, true);
    let tile: Vec<u8> = (0..64).flat_map(|_| [100, 100, 100, 255]).collect();
    let shown = bench.blur((8, 8), &tile, whole(8, 8), material(look), 1.0);
    let expected = over(&look, [100.0 / 255.0; 3]);
    let got = column_at(&shown, 8, (4, 4));
    for channel in 0..3 {
        assert!((f32::from(got[channel]) - expected[channel] * 255.0).abs() <= 1.0);
    }
}
