use ctdesign::BoxShadow;
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
            include_str!("shadow.frag"),
            include_str!("corner/coverage.glsl")
        ),
        &[
            UniformName::new("origin", UniformType::_2f),
            UniformName::new("bounds", UniformType::_2f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("width", UniformType::_1f),
            UniformName::new("sigma", UniformType::_1f),
            UniformName::new("y_offset", UniformType::_1f),
            UniformName::new("spread", UniformType::_1f),
            UniformName::new("curve", UniformType::_4f),
            UniformName::new("rise", UniformType::_1f),
        ],
    )
}

pub type Shadow = Shaded<9>;

impl Shadow {
    pub fn new(
        id: Id,
        commit: CommitCounter,
        shape: Shape,
        width: i32,
        shadow: BoxShadow,
        spread: f32,
        program: GlesPixelProgram,
    ) -> Shadow {
        let BoxShadow {
            sigma,
            y_offset,
            opacity,
        } = shadow;
        let reach = (3.0 * sigma + spread).ceil() as i32;
        let above = reach - y_offset.floor() as i32;
        let below = reach + y_offset.ceil() as i32;
        let area = shape.bounds + FrameExtents::new(reach, reach, above, below);
        let size = shape.bounds.size;
        Shaded {
            id,
            commit,
            area,
            opacity,
            program,
            uniforms: [
                Uniform::new("origin", [reach as f32, above as f32]),
                Uniform::new("bounds", [size.w as f32, size.h as f32]),
                Uniform::new("radius", shape.radius),
                Uniform::new("width", width as f32),
                Uniform::new("sigma", sigma),
                Uniform::new("y_offset", y_offset),
                Uniform::new("spread", spread),
                Uniform::new("curve", shape.curve.bezier),
                Uniform::new("rise", shape.curve.rise),
            ],
        }
    }
}

pub fn scaled(shadow: BoxShadow, scale: f64) -> BoxShadow {
    let scale = scale as f32;
    BoxShadow {
        sigma: shadow.sigma * scale,
        y_offset: shadow.y_offset * scale,
        opacity: shadow.opacity,
    }
}
