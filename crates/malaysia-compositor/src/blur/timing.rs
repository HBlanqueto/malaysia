use std::time::{Duration, Instant};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{ImportMem, Texture};
use smithay::utils::{Buffer as BufferCoords, Physical, Rectangle, Size};

use super::gaussian::{Backdrop, compile, reach_in_pixels};
use super::look::{GlassPost, GlassShape, Look, Post};
use crate::test_support::try_test_renderer;

const FRAMES: usize = 40;
const WARM_UP: usize = 3;
const OUTPUT: (f64, f64) = (1280.0, 800.0);

#[allow(unsafe_code)]
fn finish(renderer: &mut GlesRenderer) {
    renderer.with_context(|gl| unsafe { gl.Finish() }).unwrap();
}

#[allow(unsafe_code)]
fn renderer_name(renderer: &mut GlesRenderer) -> String {
    renderer
        .with_context(|gl| unsafe {
            std::ffi::CStr::from_ptr(
                gl.GetString(smithay::backend::renderer::gles::ffi::RENDERER) as *const _,
            )
            .to_string_lossy()
            .into_owned()
        })
        .unwrap()
}

fn is_software(name: &str) -> bool {
    ["llvmpipe", "softpipe", "swrast", "software"]
        .iter()
        .any(|software| name.to_lowercase().contains(software))
}

fn stripes(width: i32, height: i32) -> Vec<u8> {
    (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                let level = (((x / 24 + y / 24) % 2) * 160 + 40) as u8;
                [level, level / 2, 255 - level, 255]
            })
        })
        .collect()
}

fn median(runs: &mut [Duration]) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

struct Case {
    name: &'static str,
    look: Look,
    area: (f64, f64),
    region: (f64, f64, f64, f64),
}

fn glass_island(
    glass: ctappearance::liquid_glass::Glass,
    state: ctappearance::liquid_glass::GlassState,
    (width, height): (f32, f32),
    shaped: bool,
) -> Look {
    use ctappearance::{ColorScheme, liquid_glass};
    let set = liquid_glass::looks(glass, ColorScheme::Dark);
    let tint = liquid_glass::tint(glass, ColorScheme::Dark);
    let shown = if state == liquid_glass::GlassState::Active {
        liquid_glass::tinted(&set, &tint, 0.0, width.min(height)).look
    } else {
        liquid_glass::look(&set, state)
    };
    let look = Look {
        sigma: liquid_glass::blur_sigma(&shown, f32::MIN),
        halo: None,
        post: Post::Glass(GlassPost {
            look: shown,
            rim: &ctdesign::LIQUID_GLASS_REGULAR_DARK.rim,
            shape: None,
        }),
    };
    if shaped {
        look.shaped(GlassShape {
            rect: [0.0, 0.0, width, height],
            radius: 24.0,
            exponent: ctdesign::CIRCLE_EXPONENT,
        })
    } else {
        look
    }
}

#[test]
#[ignore = "measures the frame time of the blur passes: run with --ignored --nocapture --test-threads=1"]
fn the_blur_passes_cost_this_much_per_frame() {
    let Some(mut renderer) = try_test_renderer() else {
        eprintln!("BLUR TIMING skipped: no EGL device");
        return;
    };
    let programs = compile(&mut renderer).unwrap();
    let name = renderer_name(&mut renderer);
    let label = if is_software(&name) {
        format!("{name}, software rendering")
    } else {
        name
    };
    let windows = Look {
        sigma: ctdesign::MATERIAL_UNDER_WINDOW_BACKGROUND_DARK.active.sigma,
        halo: None,
        post: Post::Material(ctdesign::MATERIAL_UNDER_WINDOW_BACKGROUND_DARK.active),
    };
    let shell = |glass: ctdesign::ShellGlass| Look {
        sigma: glass.sigma,
        halo: (glass.halo_weight > 0.0).then_some((glass.halo_sigma, glass.halo_weight)),
        post: Post::Shell(glass),
    };
    let menu = shell(ctdesign::SHELL_GLASS_MENU_DARK);
    use ctappearance::liquid_glass::{Glass, GlassState};
    let halo_reach = f64::from(reach_in_pixels(menu.halo.unwrap().0, 1.0));
    let cases = [
        Case {
            name: "windows: the whole output, 27.6 px Gaussian, material",
            look: windows,
            area: OUTPUT,
            region: (0.0, 0.0, OUTPUT.0, OUTPUT.1),
        },
        Case {
            name: "dock: a 640x84 pill in its strip, 2.0 px Gaussian, shell glass",
            look: shell(ctdesign::SHELL_GLASS_DOCK_DARK),
            area: (OUTPUT.0, 110.0),
            region: (320.0, 10.0, 640.0, 84.0),
        },
        Case {
            name: "menu: a 280x420 popup with the 128 px halo, shell glass",
            look: menu,
            area: (
                (280.0 + 2.0 * halo_reach).min(OUTPUT.0),
                (420.0 + 2.0 * halo_reach).min(OUTPUT.1),
            ),
            region: (halo_reach, halo_reach, 280.0, 420.0),
        },
        Case {
            name: "glass: a regular 200x160 island, one sigma (before the model's pass)",
            look: glass_island(Glass::Regular, GlassState::Active, (200.0, 160.0), false),
            area: (200.0, 160.0),
            region: (0.0, 0.0, 200.0, 160.0),
        },
        Case {
            name: "glass: a regular 200x160 island, lens, rim and depth blur",
            look: glass_island(Glass::Regular, GlassState::Active, (200.0, 160.0), true),
            area: (200.0, 160.0),
            region: (0.0, 0.0, 200.0, 160.0),
        },
        Case {
            name: "glass: a clear 200x160 island, lens and rim, no blur",
            look: glass_island(Glass::Clear, GlassState::Active, (200.0, 160.0), true),
            area: (200.0, 160.0),
            region: (0.0, 0.0, 200.0, 160.0),
        },
        Case {
            name: "glass: a regular 560x400 island, lens, rim and depth blur",
            look: glass_island(Glass::Regular, GlassState::Active, (560.0, 400.0), true),
            area: (560.0, 400.0),
            region: (0.0, 0.0, 560.0, 400.0),
        },
        Case {
            name: "glass: a clear 560x400 island inactive, 20 px blur (11 levels)",
            look: glass_island(Glass::Clear, GlassState::Inactive, (560.0, 400.0), true),
            area: (560.0, 400.0),
            region: (0.0, 0.0, 560.0, 400.0),
        },
        Case {
            name: "glass: a regular 560x400 island under Reduce Transparency, 28 px blur (13 levels)",
            look: glass_island(Glass::Regular, GlassState::Reduced, (560.0, 400.0), true),
            area: (560.0, 400.0),
            region: (0.0, 0.0, 560.0, 400.0),
        },
    ];
    for scale in [1.0, 2.0] {
        for case in &cases {
            let physical = |value: f64| (value * scale).round() as i32;
            let (width, height) = (physical(case.area.0), physical(case.area.1));
            let source = renderer
                .import_memory(
                    &stripes(width, height),
                    Fourcc::Abgr8888,
                    Size::<i32, BufferCoords>::from((width, height)),
                    false,
                )
                .unwrap();
            let region = Rectangle::<i32, Physical>::new(
                (physical(case.region.0), physical(case.region.1)).into(),
                (physical(case.region.2), physical(case.region.3)).into(),
            );
            let mut backdrop =
                Backdrop::new(&mut renderer, source.size(), &case.look, scale).unwrap();
            let mut runs = Vec::new();
            for frame in 0..WARM_UP + FRAMES {
                let start = Instant::now();
                backdrop
                    .run(&mut renderer, &programs, &source, region, &case.look, scale)
                    .unwrap();
                finish(&mut renderer);
                if frame >= WARM_UP {
                    runs.push(start.elapsed());
                }
            }
            let (min, max) = (
                runs.iter().min().copied().unwrap(),
                runs.iter().max().copied().unwrap(),
            );
            let middle = median(&mut runs);
            let milliseconds = |duration: Duration| duration.as_secs_f64() * 1000.0;
            eprintln!(
                "BLUR TIMING renderer {label}: {scale}x {}: {width}x{height} px source, median {:.2} ms (min {:.2}, max {:.2}) over {FRAMES} frames, {:.1} % of a 60 Hz frame and {:.1} % of a 120 Hz frame",
                case.name,
                milliseconds(middle),
                milliseconds(min),
                milliseconds(max),
                milliseconds(middle) / (1000.0 / 60.0) * 100.0,
                milliseconds(middle) / (1000.0 / 120.0) * 100.0,
            );
        }
    }
}
