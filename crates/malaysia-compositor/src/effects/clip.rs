use smithay::backend::renderer::Texture;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, WaylandSurfaceTexture,
};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement};
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesRenderer, GlesTexProgram, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Transform};

use super::Shape;

pub fn compile(renderer: &mut GlesRenderer) -> Result<GlesTexProgram, GlesError> {
    renderer.compile_custom_texture_shader(
        concat!(
            include_str!("clip.frag"),
            include_str!("corner/coverage.glsl")
        ),
        &[
            UniformName::new("frame_from_texture", UniformType::_4f),
            UniformName::new("frame_size", UniformType::_2f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("curve", UniformType::_4f),
            UniformName::new("rise", UniformType::_1f),
        ],
    )
}

pub struct Clipped {
    pub(super) surface: WaylandSurfaceRenderElement<GlesRenderer>,
    pub(super) shape: Shape,
    pub(super) factor: f64,
    pub(super) program: GlesTexProgram,
}

fn scaled_about(
    rectangle: Rectangle<i32, Physical>,
    origin: Point<i32, Physical>,
    factor: f64,
) -> Rectangle<f64, Physical> {
    let origin = origin.to_f64();
    Rectangle::new(
        origin + (rectangle.loc.to_f64() - origin).upscale(factor),
        rectangle.size.to_f64().upscale(factor),
    )
}

fn round_edges(rectangle: Rectangle<f64, Physical>) -> Rectangle<i32, Physical> {
    Rectangle::from_extremities(
        rectangle.loc.to_i32_round(),
        (rectangle.loc + rectangle.size).to_i32_round(),
    )
}

impl Clipped {
    fn bands_without_corners(&self, origin: Point<i32, Physical>) -> [Rectangle<i32, Physical>; 3] {
        let corner = (self.shape.curve.reach() * self.shape.radius).ceil() as i32;
        let loc = self.shape.bounds.loc - origin;
        let (width, height) = (self.shape.bounds.size.w, self.shape.bounds.size.h);
        let inner_width = (width - 2 * corner).max(0);
        let inner_height = (height - 2 * corner).max(0);
        [
            Rectangle::new(loc + Point::from((0, corner)), (width, inner_height).into()),
            Rectangle::new(loc + Point::from((corner, 0)), (inner_width, corner).into()),
            Rectangle::new(
                loc + Point::from((corner, height - corner)),
                (inner_width, corner).into(),
            ),
        ]
    }
}

impl Element for Clipped {
    fn id(&self) -> &Id {
        self.surface.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.surface.current_commit()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.surface.src()
    }

    fn transform(&self) -> Transform {
        self.surface.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        round_edges(scaled_about(
            self.surface.geometry(scale),
            self.shape.bounds.loc,
            self.factor,
        ))
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        let surface_location = self.surface.geometry(scale).loc;
        let location = self.geometry(scale).loc;
        self.surface
            .damage_since(scale, commit)
            .into_iter()
            .map(|mut damage| {
                damage.loc += surface_location;
                let mut scaled =
                    scaled_about(damage, self.shape.bounds.loc, self.factor).to_i32_up::<i32>();
                scaled.loc -= location;
                scaled
            })
            .collect()
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        if self.factor != 1.0 {
            return OpaqueRegions::default();
        }
        let bands = self.bands_without_corners(self.surface.geometry(scale).loc);
        self.surface
            .opaque_regions(scale)
            .into_iter()
            .flat_map(|region| bands.map(|band| region.intersection(band)))
            .flatten()
            .filter(|region| !region.is_empty())
            .collect()
    }

    fn alpha(&self) -> f32 {
        self.surface.alpha()
    }

    fn kind(&self) -> Kind {
        self.surface.kind()
    }
}

impl RenderElement<GlesRenderer> for Clipped {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let (WaylandSurfaceTexture::Texture(texture), Transform::Normal) =
            (self.surface.texture(), self.surface.transform())
        else {
            return self
                .surface
                .draw(frame, src, dst, damage, opaque_regions, cache);
        };
        let texture_size = texture.size().to_f64();
        let dst_per_src_x = dst.size.w as f64 / src.size.w;
        let dst_per_src_y = dst.size.h as f64 / src.size.h;
        let scale_x = (texture_size.w * dst_per_src_x) as f32;
        let scale_y = (texture_size.h * dst_per_src_y) as f32;
        let offset_x =
            ((dst.loc.x - self.shape.bounds.loc.x) as f64 - src.loc.x * dst_per_src_x) as f32;
        let offset_y =
            ((dst.loc.y - self.shape.bounds.loc.y) as f64 - src.loc.y * dst_per_src_y) as f32;
        let size = self.shape.bounds.size;
        frame.render_texture_from_to(
            texture,
            src,
            dst,
            damage,
            opaque_regions,
            Transform::Normal,
            self.surface.alpha(),
            Some(&self.program),
            &[
                Uniform::new("frame_from_texture", [scale_x, scale_y, offset_x, offset_y]),
                Uniform::new("frame_size", [size.w as f32, size.h as f32]),
                Uniform::new("radius", self.shape.radius),
                Uniform::new("curve", self.shape.curve.bezier),
                Uniform::new("rise", self.shape.curve.rise),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_scaled_about_its_origin_rounds_both_edges_so_neighbours_stay_adjacent() {
        let origin = Point::from((5, 5));
        let left = Rectangle::new((15, 5).into(), (10, 10).into());
        let right = Rectangle::new((25, 5).into(), (10, 10).into());
        let left = round_edges(scaled_about(left, origin, 0.33));
        let right = round_edges(scaled_about(right, origin, 0.33));
        assert_eq!(left.loc.x + left.size.w, right.loc.x);
    }
}
