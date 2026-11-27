use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::gles::{
    GlesError, GlesPixelProgram, GlesRenderer, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;

use super::Shape;
use super::shaded::Shaded;

pub fn compile(renderer: &mut GlesRenderer) -> Result<GlesPixelProgram, GlesError> {
    renderer.compile_custom_pixel_shader(
        concat!(
            include_str!("badge.frag"),
            include_str!("corner/coverage.glsl")
        ),
        &[
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("fill", UniformType::_4f),
            UniformName::new("fill_right", UniformType::_4f),
            UniformName::new("cross_color", UniformType::_4f),
            UniformName::new("cross_reach", UniformType::_1f),
            UniformName::new("cross_width", UniformType::_1f),
            UniformName::new("curve", UniformType::_4f),
            UniformName::new("rise", UniformType::_1f),
        ],
    )
}

pub type Badge = Shaded<8>;

const FILL_RIGHT: usize = 7;

impl Badge {
    pub fn new(
        id: Id,
        shape: Shape,
        fill: [f32; 4],
        cross_color: [f32; 4],
        cross_reach: f32,
        cross_width: f32,
        program: GlesPixelProgram,
    ) -> Badge {
        Shaded {
            id,
            commit: CommitCounter::default(),
            area: shape.bounds,
            opacity: 1.0,
            program,
            uniforms: [
                Uniform::new("radius", shape.radius),
                Uniform::new("fill", fill),
                Uniform::new("cross_color", cross_color),
                Uniform::new("cross_reach", cross_reach),
                Uniform::new("cross_width", cross_width),
                Uniform::new("curve", shape.curve.bezier),
                Uniform::new("rise", shape.curve.rise),
                Uniform::new("fill_right", fill),
            ],
        }
    }

    pub fn fade_to(&mut self, fill_right: [f32; 4]) {
        self.uniforms[FILL_RIGHT] = Uniform::new("fill_right", fill_right);
    }
}
