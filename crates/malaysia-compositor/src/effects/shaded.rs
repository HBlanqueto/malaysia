use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesPixelProgram, GlesRenderer, Uniform,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Size};

pub struct Shaded<const N: usize> {
    pub(super) id: Id,
    pub(super) commit: CommitCounter,
    pub(super) area: Rectangle<i32, Physical>,
    pub(super) opacity: f32,
    pub(super) program: GlesPixelProgram,
    pub(super) uniforms: [Uniform<'static>; N],
}

impl<const N: usize> Element for Shaded<N> {
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

impl<const N: usize> RenderElement<GlesRenderer> for Shaded<N> {
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
            &self.uniforms,
        )
    }
}
