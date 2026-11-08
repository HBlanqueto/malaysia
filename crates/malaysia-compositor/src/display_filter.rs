use ctappearance::{AppearanceConfig, ColorFilter};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::element::TextureShaderElement;
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType, UniformValue,
};
use smithay::backend::renderer::{Bind, Offscreen, Texture};
use smithay::utils::{Buffer as BufferCoord, Physical, Point, Scale, Size, Transform};

use crate::cursor::{Cursors, OutputRenderElement};
use crate::renderer::{MalaysiaRenderer, PrimaryGpu};
use crate::state::State;

type Matrix = [[f32; 3]; 3];

const IDENTITY: Matrix = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

// Machado, Oliveira and Fernandes 2009, severity 1.0:
// https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html
const PROTANOPIA: Matrix = [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
];
const DEUTERANOPIA: Matrix = [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881],
];
const TRITANOPIA: Matrix = [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900],
];

// Daltonization moves what the viewer cannot tell apart into channels they see
// (Fidaner, Lin and Ozguven 2005, "Analysis of Color Blindness").
const SHIFT_RED_GREEN: Matrix = [[0.0, 0.0, 0.0], [0.7, 1.0, 0.0], [0.7, 0.0, 1.0]];
const SHIFT_BLUE_YELLOW: Matrix = [[1.0, 0.0, 0.7], [0.0, 1.0, 0.7], [0.0, 0.0, 0.0]];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DisplayFilter {
    matrix: Matrix,
    invert: bool,
    contrast: f32,
}

impl DisplayFilter {
    const NONE: Self = Self {
        matrix: IDENTITY,
        invert: false,
        contrast: 1.0,
    };

    pub(crate) fn from_appearance(appearance: &AppearanceConfig) -> Option<Self> {
        let matrix = if appearance.color_filters {
            mix(
                filter_matrix(appearance.color_filter),
                appearance.color_filter_intensity,
            )
        } else {
            IDENTITY
        };
        let filter = Self {
            matrix,
            invert: appearance.invert_colors,
            contrast: appearance.display_contrast,
        };
        (filter != Self::NONE).then_some(filter)
    }

    fn uniforms(&self) -> Vec<Uniform<'static>> {
        let m = self.matrix;
        let column_major = [
            m[0][0], m[1][0], m[2][0], m[0][1], m[1][1], m[2][1], m[0][2], m[1][2], m[2][2],
        ];
        vec![
            Uniform::new(
                "color_matrix",
                UniformValue::Matrix3x3 {
                    matrices: vec![column_major],
                    transpose: false,
                },
            ),
            Uniform::new("invert", if self.invert { 1.0f32 } else { 0.0 }),
            Uniform::new("contrast", self.contrast),
        ]
    }
}

fn filter_matrix(filter: ColorFilter) -> Matrix {
    match filter {
        ColorFilter::Grayscale => [LUMA; 3],
        ColorFilter::RedGreen => daltonize(PROTANOPIA, SHIFT_RED_GREEN),
        ColorFilter::GreenRed => daltonize(DEUTERANOPIA, SHIFT_RED_GREEN),
        ColorFilter::BlueYellow => daltonize(TRITANOPIA, SHIFT_BLUE_YELLOW),
    }
}

fn daltonize(simulation: Matrix, shift: Matrix) -> Matrix {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            let lost = |k: usize| IDENTITY[k][column] - simulation[k][column];
            IDENTITY[row][column] + (0..3).map(|k| shift[row][k] * lost(k)).sum::<f32>()
        })
    })
}

fn mix(matrix: Matrix, amount: f32) -> Matrix {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            IDENTITY[row][column] + (matrix[row][column] - IDENTITY[row][column]) * amount
        })
    })
}

pub(crate) fn compile(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    renderer
        .compile_custom_texture_shader(
            grade_graphics::shaders::DISPLAY_FILTER,
            &[
                UniformName::new("color_matrix", UniformType::Matrix3x3),
                UniformName::new("invert", UniformType::_1f),
                UniformName::new("contrast", UniformType::_1f),
            ],
        )
        .inspect_err(|error| tracing::warn!(?error, "failed to compile display-filter shader"))
        .ok()
}

pub(crate) fn element<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    output_size: Size<i32, Physical>,
    target: &mut Option<GlesTexture>,
) -> Option<OutputRenderElement<R>> {
    let filter = DisplayFilter::from_appearance(&state.config.appearance)?;
    let program = state.display_filter.clone()?;
    let scale = state.scale.fractional_scale();
    let elements = crate::scene_elements::build_output_elements(state, renderer, cursor, true);
    let buffer_size = Size::<i32, BufferCoord>::from((output_size.w, output_size.h));
    if target
        .as_ref()
        .is_none_or(|texture| texture.size() != buffer_size)
    {
        *target =
            Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, buffer_size).ok();
    }
    let mut texture = target.clone()?;
    {
        let mut framebuffer = renderer.bind(&mut texture).ok()?;
        OutputDamageTracker::new(output_size, Scale::from(scale), Transform::Normal)
            .render_output(
                renderer,
                &mut framebuffer,
                0,
                &elements,
                ctdesign::DESKTOP_CLEAR.premultiplied(),
            )
            .ok()?;
    }
    let buffer = TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None);
    let shown = TextureRenderElement::from_texture_buffer(
        Point::from((0.0, 0.0)),
        &buffer,
        None,
        None,
        None,
        Kind::Unspecified,
    );
    Some(PrimaryGpu(TextureShaderElement::new(shown, program, filter.uniforms())).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    impl DisplayFilter {
        fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
            let filtered = self
                .matrix
                .map(|row| (row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2]).clamp(0.0, 1.0));
            filtered.map(|channel| {
                let channel = if self.invert { 1.0 - channel } else { channel };
                ((channel - 0.5) * self.contrast + 0.5).clamp(0.0, 1.0)
            })
        }
    }

    fn appearance() -> AppearanceConfig {
        AppearanceConfig::default()
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn the_default_appearance_needs_no_filter_pass() {
        assert_eq!(DisplayFilter::from_appearance(&appearance()), None);
    }

    #[test]
    fn grayscale_keeps_white_and_black_and_drops_the_hue() {
        let filter = DisplayFilter::from_appearance(&AppearanceConfig {
            color_filters: true,
            color_filter: ColorFilter::Grayscale,
            color_filter_intensity: 1.0,
            ..appearance()
        })
        .unwrap();
        assert!(close(filter.apply([1.0, 1.0, 1.0]), [1.0, 1.0, 1.0]));
        assert!(close(filter.apply([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]));
        let red = filter.apply([1.0, 0.0, 0.0]);
        assert!(close(red, [LUMA[0]; 3]));
    }

    #[test]
    fn colour_blindness_filters_keep_grays_and_change_red() {
        for color_filter in [
            ColorFilter::RedGreen,
            ColorFilter::GreenRed,
            ColorFilter::BlueYellow,
        ] {
            let filter = DisplayFilter::from_appearance(&AppearanceConfig {
                color_filters: true,
                color_filter,
                color_filter_intensity: 1.0,
                ..appearance()
            })
            .unwrap();
            assert!(
                close(filter.apply([0.5, 0.5, 0.5]), [0.5, 0.5, 0.5]),
                "{color_filter:?}"
            );
            assert!(
                !close(filter.apply([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]),
                "{color_filter:?}"
            );
        }
    }

    #[test]
    fn intensity_blends_the_filter_with_the_original_colour() {
        let half = DisplayFilter::from_appearance(&AppearanceConfig {
            color_filters: true,
            color_filter: ColorFilter::Grayscale,
            color_filter_intensity: 0.5,
            ..appearance()
        })
        .unwrap();
        assert!(close(
            half.apply([1.0, 0.0, 0.0]),
            [0.5 + 0.5 * LUMA[0], 0.5 * LUMA[0], 0.5 * LUMA[0]]
        ));
    }

    #[test]
    fn invert_and_contrast_work_around_middle_gray() {
        let filter = DisplayFilter::from_appearance(&AppearanceConfig {
            invert_colors: true,
            display_contrast: 2.0,
            ..appearance()
        })
        .unwrap();
        assert!(close(filter.apply([0.0, 0.5, 0.75]), [1.0, 0.5, 0.0]));
    }
}
