//! Compositor-side window effects: rounded frame, border and shadow.

use smithay::backend::renderer::Texture;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement};
use smithay::backend::renderer::gles::{
    GlesPixelProgram, GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::{
    Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};

fn frame_to_texture_geometry(
    src: Rectangle<f64, BufferCoords>,
    dst: Rectangle<i32, Physical>,
    frame_rect: Rectangle<i32, Physical>,
) -> [f32; 4] {
    let scale_x = src.size.w / f64::from(dst.size.w);
    let scale_y = src.size.h / f64::from(dst.size.h);
    let x = src.loc.x + f64::from(frame_rect.loc.x - dst.loc.x) * scale_x;
    let y = src.loc.y + f64::from(frame_rect.loc.y - dst.loc.y) * scale_y;
    let w = f64::from(frame_rect.size.w) * scale_x;
    let h = f64::from(frame_rect.size.h) * scale_y;
    [x as f32, y as f32, w as f32, h as f32]
}

fn texture_radius(
    src: Rectangle<f64, BufferCoords>,
    dst: Rectangle<i32, Physical>,
    radius: i32,
) -> f32 {
    let scale = src.size.w / f64::from(dst.size.w);
    (f64::from(radius) * scale) as f32
}

pub(crate) fn shader_area(
    frame: Rectangle<i32, Physical>,
    margin: i32,
    scale: f64,
) -> Rectangle<i32, Logical> {
    let left = (f64::from(frame.loc.x - margin) / scale).floor() as i32;
    let top = (f64::from(frame.loc.y - margin) / scale).floor() as i32;
    let right = (f64::from(frame.loc.x + frame.size.w + margin) / scale).ceil() as i32;
    let bottom = (f64::from(frame.loc.y + frame.size.h + margin) / scale).ceil() as i32;
    Rectangle::new(
        Point::from((left, top)),
        Size::from((right - left, bottom - top)),
    )
}

pub(crate) struct RoundedTextureElement {
    id: Id,
    commit_counter: CommitCounter,
    texture: GlesTexture,
    texture_size: Size<i32, BufferCoords>,
    src: Rectangle<f64, BufferCoords>,
    dst: Rectangle<i32, Physical>,
    transform: Transform,
    alpha: f32,
    kind: Kind,
    frame_rect: Rectangle<i32, Physical>,
    radius: i32,
    exponent: f32,
    program: GlesTexProgram,
}

impl RoundedTextureElement {
    pub(crate) fn from_surface_element(
        element: &WaylandSurfaceRenderElement<GlesRenderer>,
        frame_rect: Rectangle<i32, Physical>,
        radius: i32,
        exponent: f32,
        scale: f64,
        program: GlesTexProgram,
    ) -> Option<Self> {
        let texture = match element.texture() {
            WaylandSurfaceRenderElementTexture::Texture(tex) => tex.clone(),
            WaylandSurfaceRenderElementTexture::SolidColor(_) => return None,
        };
        let texture_size = texture.size();
        Some(Self {
            id: element.id().clone(),
            commit_counter: element.current_commit(),
            texture,
            texture_size,
            src: element.src(),
            dst: element.geometry(Scale::from(scale)),
            transform: element.transform(),
            alpha: element.alpha(),
            kind: element.kind(),
            frame_rect,
            radius,
            exponent,
            program,
        })
    }

    pub(crate) fn mapped(
        mut self,
        from: Rectangle<i32, Physical>,
        to: Rectangle<i32, Physical>,
    ) -> Self {
        self.dst = map_rect(self.dst, from, to);
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_texture(
        texture: GlesTexture,
        id: Id,
        src: Rectangle<f64, BufferCoords>,
        dst: Rectangle<i32, Physical>,
        transform: Transform,
        alpha: f32,
        kind: Kind,
        frame_rect: Rectangle<i32, Physical>,
        radius: i32,
        exponent: f32,
        program: GlesTexProgram,
    ) -> Self {
        let texture_size = texture.size();
        Self {
            id,
            commit_counter: CommitCounter::default(),
            texture,
            texture_size,
            src,
            dst,
            transform,
            alpha,
            kind,
            frame_rect,
            radius,
            exponent,
            program,
        }
    }

    fn texture_geometry(&self) -> [f32; 4] {
        frame_to_texture_geometry(self.src, self.dst, self.frame_rect)
    }

    fn rounded_radius(&self) -> f32 {
        texture_radius(self.src, self.dst, self.radius)
    }
}

impl Element for RoundedTextureElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit_counter
    }

    fn src(&self) -> Rectangle<f64, BufferCoords> {
        self.src
    }

    fn transform(&self) -> Transform {
        self.transform
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.dst
    }

    fn damage_since(
        &self,
        _scale: Scale<f64>,
        _commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        DamageSet::from_slice(&[Rectangle::from_size(self.dst.size)])
    }

    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn alpha(&self) -> f32 {
        self.alpha
    }

    fn kind(&self) -> Kind {
        self.kind
    }
}

impl RenderElement<GlesRenderer> for RoundedTextureElement {
    fn draw(
        &self,
        frame: &mut smithay::backend::renderer::gles::GlesFrame<'_, '_>,
        src: Rectangle<f64, BufferCoords>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        _cache: Option<&smithay::utils::user_data::UserDataMap>,
    ) -> Result<(), smithay::backend::renderer::gles::GlesError> {
        let uniforms = vec![
            Uniform::new(
                "texture_size",
                [self.texture_size.w as f32, self.texture_size.h as f32],
            ),
            Uniform::new("geometry", self.texture_geometry()),
            Uniform::new("radius", self.rounded_radius()),
            Uniform::new("exponent", self.exponent),
        ];
        frame.render_texture_from_to(
            &self.texture,
            src,
            dst,
            damage,
            opaque_regions,
            self.transform,
            self.alpha,
            Some(&self.program),
            &uniforms,
        )
    }
}

use smithay::backend::renderer::element::surface::WaylandSurfaceTexture as WaylandSurfaceRenderElementTexture;

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

    use super::{frame_to_texture_geometry, shader_area, texture_radius};

    #[test]
    fn surface_geometry_scales_the_logical_view_to_physical() {
        let view_dst = Size::<f64, Logical>::from((694.0, 488.0));
        let at_one: Size<i32, Physical> = view_dst.to_physical(Scale::from(1.0)).to_i32_round();
        let at_125: Size<i32, Physical> = view_dst.to_physical(Scale::from(1.25)).to_i32_round();
        assert_eq!(at_one, Size::from((694, 488)));
        assert_eq!(at_125, Size::from((868, 610)));
        assert_ne!(at_one, at_125);
    }

    #[test]
    fn texture_geometry_scales_the_frame_to_source_space() {
        let src = Rectangle::from_size(Size::from((400.0, 200.0)));
        let dst = Rectangle::new(Point::from((10, 20)), Size::from((200, 100)));
        let frame = Rectangle::new(Point::from((10, 20)), Size::from((200, 100)));
        let geo = frame_to_texture_geometry(src, dst, frame);
        assert_eq!(geo, [0.0, 0.0, 400.0, 200.0]);
    }

    #[test]
    fn texture_geometry_offsets_for_a_subsurface() {
        let src = Rectangle::from_size(Size::from((400.0, 200.0)));
        let dst = Rectangle::new(Point::from((40, 50)), Size::from((200, 100)));
        let frame = Rectangle::new(Point::from((10, 20)), Size::from((300, 200)));
        let geo = frame_to_texture_geometry(src, dst, frame);
        assert!((geo[0] - -60.0).abs() < 0.01);
        assert!((geo[1] - -60.0).abs() < 0.01);
        assert!((geo[2] - 600.0).abs() < 0.01);
        assert!((geo[3] - 400.0).abs() < 0.01);
    }

    #[test]
    fn the_shader_area_covers_the_frame_and_its_margin() {
        for scale in [1.0_f64, 1.25] {
            let frame = Rectangle::new(Point::from((10, 20)), Size::from((200, 100)));
            let area = shader_area(frame, 2, scale);
            let physical: Rectangle<i32, Physical> = area.to_physical_precise_round(scale);
            assert!(physical.loc.x <= frame.loc.x - 2, "scale {scale}");
            assert!(physical.loc.y <= frame.loc.y - 2, "scale {scale}");
            assert!(
                physical.loc.x + physical.size.w >= frame.loc.x + frame.size.w + 2,
                "scale {scale}"
            );
            assert!(
                physical.loc.y + physical.size.h >= frame.loc.y + frame.size.h + 2,
                "scale {scale}"
            );
        }
    }

    #[test]
    fn an_ssd_surface_is_clipped_at_the_frame_bottom() {
        let src = Rectangle::from_size(Size::from((400.0, 268.0)));
        let dst = Rectangle::new(Point::from((10, 52)), Size::from((400, 268)));
        let frame = Rectangle::new(Point::from((10, 20)), Size::from((400, 300)));
        let geo = frame_to_texture_geometry(src, dst, frame);
        assert!((geo[1] + geo[3] - src.size.h as f32).abs() < 0.01);
    }

    #[test]
    fn a_subsurface_at_the_bottom_shares_the_frame_corner() {
        let src = Rectangle::from_size(Size::from((200.0, 100.0)));
        let dst = Rectangle::new(Point::from((10, 220)), Size::from((200, 100)));
        let frame = Rectangle::new(Point::from((10, 20)), Size::from((200, 300)));
        let geo = frame_to_texture_geometry(src, dst, frame);
        assert!((geo[1] + geo[3] - src.size.h as f32).abs() < 0.01);
    }

    #[test]
    fn texture_radius_scales_with_the_source_to_dest_ratio() {
        let src = Rectangle::from_size(Size::from((400.0, 200.0)));
        let dst = Rectangle::new(Point::from((0, 0)), Size::from((200, 100)));
        assert!((texture_radius(src, dst, 16) - 32.0).abs() < 0.01);
    }
}

pub(crate) fn map_rect(
    rect: Rectangle<i32, Physical>,
    from: Rectangle<i32, Physical>,
    to: Rectangle<i32, Physical>,
) -> Rectangle<i32, Physical> {
    let sx = f64::from(to.size.w) / f64::from(from.size.w.max(1));
    let sy = f64::from(to.size.h) / f64::from(from.size.h.max(1));
    let along_x = |x: i32| (f64::from(to.loc.x) + f64::from(x - from.loc.x) * sx).round() as i32;
    let along_y = |y: i32| (f64::from(to.loc.y) + f64::from(y - from.loc.y) * sy).round() as i32;
    let (left, top) = (along_x(rect.loc.x), along_y(rect.loc.y));
    let right = along_x(rect.loc.x + rect.size.w);
    let bottom = along_y(rect.loc.y + rect.size.h);
    Rectangle::new(
        (left, top).into(),
        ((right - left).max(1), (bottom - top).max(1)).into(),
    )
}

pub(crate) fn compile_rounded_clip(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    let program = renderer.compile_custom_texture_shader(
        grade_graphics::shaders::ROUNDED_CLIP,
        &[
            UniformName::new("texture_size", UniformType::_2f),
            UniformName::new("geometry", UniformType::_4f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("exponent", UniformType::_1f),
        ],
    );
    match program {
        Ok(program) => Some(program),
        Err(error) => {
            tracing::warn!(?error, "failed to compile rounded-clip shader");
            None
        }
    }
}

pub(crate) fn compile_shadow(renderer: &mut GlesRenderer) -> Option<GlesPixelProgram> {
    let program = renderer.compile_custom_pixel_shader(
        grade_graphics::shaders::SHADOW,
        &[
            UniformName::new("size", UniformType::_2f),
            UniformName::new("rect", UniformType::_4f),
            UniformName::new("window_rect", UniformType::_4f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("blur", UniformType::_1f),
            UniformName::new("offset", UniformType::_2f),
            UniformName::new("scale", UniformType::_1f),
            UniformName::new("exponent", UniformType::_1f),
            UniformName::new("color", UniformType::_4f),
        ],
    );
    match program {
        Ok(program) => Some(program),
        Err(error) => {
            tracing::warn!(?error, "failed to compile shadow shader");
            None
        }
    }
}

pub(crate) fn compile_border(renderer: &mut GlesRenderer) -> Option<GlesPixelProgram> {
    let program = renderer.compile_custom_pixel_shader(
        grade_graphics::shaders::BORDER,
        &[
            UniformName::new("size", UniformType::_2f),
            UniformName::new("rect", UniformType::_4f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("width", UniformType::_1f),
            UniformName::new("side", UniformType::_1f),
            UniformName::new("scale", UniformType::_1f),
            UniformName::new("exponent", UniformType::_1f),
            UniformName::new("color", UniformType::_4f),
        ],
    );
    match program {
        Ok(program) => Some(program),
        Err(error) => {
            tracing::warn!(?error, "failed to compile border shader");
            None
        }
    }
}

#[cfg(test)]
mod map_tests {
    use super::map_rect;
    use smithay::utils::{Physical, Rectangle};

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Physical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    #[test]
    fn the_source_rectangle_lands_exactly_on_the_target() {
        let from = r(100, 50, 400, 300);
        let to = r(0, 0, 1280, 800);
        assert_eq!(map_rect(from, from, to), to);
    }

    #[test]
    fn a_shrinking_window_fills_the_animated_rectangle() {
        let content = r(0, 0, 600, 400);
        let animated = r(0, 0, 900, 600);
        assert_eq!(map_rect(content, content, animated), animated);
        assert_eq!(
            map_rect(r(300, 200, 100, 100), content, animated),
            r(450, 300, 150, 150)
        );
    }
}
