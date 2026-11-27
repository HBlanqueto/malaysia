use ctdesign::Rgba;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::gles::{
    GlesError, GlesPixelProgram, GlesRenderer, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::FrameExtents;

use super::Shape;
use super::shaded::Shaded;

pub fn compile(renderer: &mut GlesRenderer) -> Result<GlesPixelProgram, GlesError> {
    renderer.compile_custom_pixel_shader(
        concat!(
            include_str!("lines.frag"),
            include_str!("corner/coverage.glsl")
        ),
        &[
            UniformName::new("width", UniformType::_1f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("outline", UniformType::_4f),
            UniformName::new("inner_line", UniformType::_4f),
            UniformName::new("curve", UniformType::_4f),
            UniformName::new("rise", UniformType::_1f),
        ],
    )
}

pub type Lines = Shaded<6>;

impl Lines {
    pub fn new(
        id: Id,
        commit: CommitCounter,
        shape: Shape,
        width: i32,
        outline: Rgba,
        inner_line: Rgba,
        program: GlesPixelProgram,
    ) -> Lines {
        let area = shape.bounds + FrameExtents::new(width, width, width, width);
        Shaded {
            id,
            commit,
            area,
            opacity: 1.0,
            program,
            uniforms: [
                Uniform::new("width", width as f32),
                Uniform::new("radius", shape.radius),
                Uniform::new("outline", outline.premultiplied()),
                Uniform::new("inner_line", inner_line.premultiplied()),
                Uniform::new("curve", shape.curve.bezier),
                Uniform::new("rise", shape.curve.rise),
            ],
        }
    }
}
