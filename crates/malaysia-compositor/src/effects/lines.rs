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

pub struct Lines {
    id: Id,
    commit: CommitCounter,
    area: Rectangle<i32, Physical>,
    width: f32,
    shape: Shape,
    outline: [f32; 4],
    inner_line: [f32; 4],
    program: GlesPixelProgram,
}

impl Lines {
    pub fn new(
        drawn: &Drawn,
        shape: Shape,
        width: i32,
        main: bool,
        dark: bool,
        program: GlesPixelProgram,
    ) -> Lines {
        let area = Rectangle::new(
            shape.bounds.loc - Point::from((width, width)),
            shape.bounds.size + Size::from((2 * width, 2 * width)),
        );
        let lines = if dark {
            ctdesign::FRAME_LINES_DARK
        } else {
            ctdesign::FRAME_LINES_LIGHT
        };
        let (outline, inner_line) = if main {
            (lines.outer, lines.inner)
        } else {
            (lines.outer_inactive, lines.inner_inactive)
        };
        Lines {
            id: drawn.lines.clone(),
            commit: drawn.commit,
            area,
            width: width as f32,
            shape,
            outline: outline.premultiplied(),
            inner_line: inner_line.premultiplied(),
            program,
        }
    }
}

impl Element for Lines {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_size(Size::from((
            self.area.size.w as f64,
            self.area.size.h as f64,
        )))
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.area
    }
}

impl RenderElement<GlesRenderer> for Lines {
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
            1.0,
            &[
                Uniform::new("width", self.width),
                Uniform::new("radius", self.shape.radius),
                Uniform::new("outline", self.outline),
                Uniform::new("inner_line", self.inner_line),
                Uniform::new("curve", self.shape.curve.bezier),
                Uniform::new("rise", self.shape.curve.rise),
            ],
        )
    }
}
