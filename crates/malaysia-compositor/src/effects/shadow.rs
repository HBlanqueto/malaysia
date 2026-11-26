use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesPixelProgram, GlesRenderer, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Size};

use super::{Drawn, Shape};

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

pub struct Shadow {
    id: Id,
    commit: CommitCounter,
    area: Rectangle<i32, Physical>,
    origin: [f32; 2],
    width: f32,
    shape: Shape,
    sigma: f32,
    y_offset: f32,
    spread: f32,
    opacity: f32,
    program: GlesPixelProgram,
}

impl Shadow {
    pub fn new(
        drawn: &Drawn,
        shape: Shape,
        width: i32,
        scale: f64,
        main: bool,
        shadows: ctdesign::WindowShadow,
        program: GlesPixelProgram,
    ) -> Shadow {
        let shadow = if main { shadows.main } else { shadows.inactive };
        let scale = scale as f32;
        let sigma = shadow.sigma * scale;
        let y_offset = shadow.y_offset * scale;
        let spread = ctdesign::WINDOW_SHADOW_SPREAD * scale;
        let reach = (3.0 * sigma + spread).ceil() as i32;
        let above = reach - y_offset.floor() as i32;
        let below = reach + y_offset.ceil() as i32;
        let area = Rectangle::new(
            shape.bounds.loc - Point::from((reach, above)),
            shape.bounds.size + Size::from((2 * reach, above + below)),
        );
        Shadow {
            id: drawn.shadow.clone(),
            commit: drawn.commit,
            area,
            origin: [reach as f32, above as f32],
            width: width as f32,
            shape,
            sigma,
            y_offset,
            spread,
            opacity: shadow.opacity,
            program,
        }
    }
}

impl Element for Shadow {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_size((self.area.size.w as f64, self.area.size.h as f64).into())
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.area
    }
}

impl RenderElement<GlesRenderer> for Shadow {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        _cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        frame.render_pixel_shader_to(
            &self.program,
            src,
            dst,
            Size::from((self.area.size.w, self.area.size.h)),
            Some(damage),
            self.opacity,
            &[
                Uniform::new("origin", self.origin),
                Uniform::new(
                    "bounds",
                    [
                        self.shape.bounds.size.w as f32,
                        self.shape.bounds.size.h as f32,
                    ],
                ),
                Uniform::new("radius", self.shape.radius),
                Uniform::new("width", self.width),
                Uniform::new("sigma", self.sigma),
                Uniform::new("y_offset", self.y_offset),
                Uniform::new("spread", self.spread),
                Uniform::new("curve", self.shape.curve.bezier),
                Uniform::new("rise", self.shape.curve.rise),
            ],
        )
    }
}
