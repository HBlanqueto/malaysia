use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, WaylandSurfaceTexture,
};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesPixelProgram, GlesRenderer, GlesTexProgram, GlesTexture, Uniform,
    UniformName, UniformType,
};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::{Renderer, Texture};

use crate::renderer::MalaysiaRenderer;
use smithay::utils::{
    Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ArrowEdge {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Arrow {
    edge: ArrowEdge,
    tip: f32,
    height: f32,
    tip_radius: f32,
    fillet: f32,
    slope: f32,
}

impl Arrow {
    pub(crate) fn popover(
        edge: ArrowEdge,
        tip: f64,
        metrics: ctdesign::PopoverMetrics,
        scale: f64,
    ) -> Self {
        let physical = |value: f32| (f64::from(value) * scale) as f32;
        Self {
            edge,
            tip: (tip * scale) as f32,
            height: physical(metrics.arrow_height),
            tip_radius: physical(metrics.arrow_tip_radius),
            fillet: physical(metrics.arrow_fillet),
            slope: metrics.arrow_width / 2.0 / metrics.arrow_height,
        }
    }

    pub(crate) fn is_some(self) -> bool {
        self.height > 0.0
    }

    fn band(self) -> i32 {
        self.height.ceil() as i32
    }

    pub(crate) fn uniforms(self, factor: f32) -> [Uniform<'static>; 3] {
        [
            Uniform::new(
                "arrow",
                [
                    self.tip * factor,
                    self.height * factor,
                    self.tip_radius * factor,
                    self.fillet * factor,
                ],
            ),
            Uniform::new("arrow_slope", self.slope),
            Uniform::new("arrow_edge", self.edge as u8 as f32),
        ]
    }
}

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

fn crop_of(
    src: Rectangle<f64, BufferCoords>,
    geometry: Rectangle<i32, Physical>,
    crop: Rectangle<i32, Physical>,
) -> Option<(Rectangle<f64, BufferCoords>, Rectangle<i32, Physical>)> {
    let region = geometry.intersection(crop)?;
    let [x, y, w, h] = frame_to_texture_geometry(src, geometry, region);
    Some((
        Rectangle::new(
            (f64::from(x), f64::from(y)).into(),
            (f64::from(w), f64::from(h)).into(),
        ),
        region,
    ))
}

fn texture_radius(
    src: Rectangle<f64, BufferCoords>,
    dst: Rectangle<i32, Physical>,
    radius: i32,
) -> f32 {
    let scale = src.size.w / f64::from(dst.size.w);
    (f64::from(radius) * scale) as f32
}

fn opaque_inside_frame(
    regions: impl IntoIterator<Item = Rectangle<i32, Physical>>,
    frame: Rectangle<i32, Physical>,
    radius: i32,
) -> Vec<Rectangle<i32, Physical>> {
    let inside = regions
        .into_iter()
        .filter_map(|rect| rect.intersection(frame));
    Rectangle::subtract_rects_many(inside, rounded_corners(frame, radius))
}

fn rounded_corners(frame: Rectangle<i32, Physical>, radius: i32) -> [Rectangle<i32, Physical>; 4] {
    let size = Size::from((radius, radius));
    let right = frame.loc.x + frame.size.w - radius;
    let bottom = frame.loc.y + frame.size.h - radius;
    [
        Rectangle::new(frame.loc, size),
        Rectangle::new(Point::from((right, frame.loc.y)), size),
        Rectangle::new(Point::from((frame.loc.x, bottom)), size),
        Rectangle::new(Point::from((right, bottom)), size),
    ]
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

pub(crate) fn uniform_rect(rect: Rectangle<i32, Physical>) -> [f32; 4] {
    [
        rect.loc.x as f32,
        rect.loc.y as f32,
        rect.size.w as f32,
        rect.size.h as f32,
    ]
}

pub(crate) type Holes = [Option<Rectangle<i32, Physical>>; 2];

pub(crate) type ShaderQuad = (Rectangle<i32, Logical>, Vec<Uniform<'static>>, Holes);

pub(crate) fn beyond_reach(
    frame: Rectangle<i32, Physical>,
    radius: i32,
    arrow: Arrow,
    depth: i32,
) -> Holes {
    let corner = radius + arrow.band() + 3;
    let edge = if arrow.is_some() { corner } else { depth + 3 };
    let inside = |x: i32, y: i32| {
        let (w, h) = (frame.size.w - 2 * x, frame.size.h - 2 * y);
        (w > 0 && h > 0)
            .then(|| Rectangle::new(frame.loc + Point::from((x, y)), Size::from((w, h))))
    };
    [
        inside(edge, corner),
        (corner != edge).then(|| inside(corner, edge)).flatten(),
    ]
}

#[derive(Debug, Clone)]
pub(crate) struct Hollow {
    element: PixelShaderElement,
    holes: Holes,
}

impl Hollow {
    pub(crate) fn new(element: PixelShaderElement, holes: Holes) -> Self {
        Self { element, holes }
    }
}

impl Element for Hollow {
    fn id(&self) -> &Id {
        self.element.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.element.current_commit()
    }

    fn src(&self) -> Rectangle<f64, BufferCoords> {
        self.element.src()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.element.geometry(scale)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.element.opaque_regions(scale)
    }

    fn alpha(&self) -> f32 {
        self.element.alpha()
    }

    fn kind(&self) -> Kind {
        self.element.kind()
    }
}

impl RenderElement<GlesRenderer> for Hollow {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, BufferCoords>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&smithay::utils::user_data::UserDataMap>,
    ) -> Result<(), GlesError> {
        if self.holes.iter().all(Option::is_none) {
            return self
                .element
                .draw(frame, src, dst, damage, opaque_regions, cache);
        }
        let ring = Rectangle::subtract_rects_many(
            damage.iter().copied(),
            self.holes.into_iter().flatten(),
        );
        if ring.is_empty() {
            return Ok(());
        }
        self.element
            .draw(frame, src, dst, &ring, opaque_regions, cache)
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

#[allow(clippy::large_enum_variant)]
enum Source<R: Renderer> {
    Surface(WaylandSurfaceRenderElement<R>),
    Crop {
        element: WaylandSurfaceRenderElement<R>,
        id: Id,
        src: Rectangle<f64, BufferCoords>,
    },
    Texture {
        texture: GlesTexture,
        id: Id,
        commit: CommitCounter,
        src: Rectangle<f64, BufferCoords>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shape {
    pub(crate) rect: Rectangle<i32, Physical>,
    pub(crate) radius: i32,
    pub(crate) exponent: f32,
}

pub(crate) struct RoundedTextureElement<R: Renderer> {
    source: Source<R>,
    texture_size: Size<i32, BufferCoords>,
    dst: Rectangle<i32, Physical>,
    remapped: bool,
    shape: Shape,
    arrow: Arrow,
    program: GlesTexProgram,
    extra: Vec<Uniform<'static>>,
}

impl<R: MalaysiaRenderer> RoundedTextureElement<R> {
    pub(crate) fn with_arrow(mut self, arrow: Arrow) -> Self {
        self.arrow = arrow;
        self
    }

    pub(crate) fn with_uniforms(mut self, extra: Vec<Uniform<'static>>) -> Self {
        self.extra = extra;
        self
    }

    pub(crate) fn from_surface_element(
        element: WaylandSurfaceRenderElement<R>,
        shape: Shape,
        scale: f64,
        program: GlesTexProgram,
    ) -> Result<Self, Box<WaylandSurfaceRenderElement<R>>> {
        let texture_size = match element.texture() {
            WaylandSurfaceTexture::Texture(texture) => texture.size(),
            WaylandSurfaceTexture::SolidColor(_) => return Err(Box::new(element)),
        };
        Ok(Self {
            texture_size,
            dst: element.geometry(Scale::from(scale)),
            source: Source::Surface(element),
            remapped: false,
            shape,
            arrow: Arrow::default(),
            program,
            extra: Vec::new(),
        })
    }

    pub(crate) fn from_surface_crop(
        element: WaylandSurfaceRenderElement<R>,
        crop: Rectangle<i32, Physical>,
        id: Id,
        shape: Shape,
        scale: f64,
        program: GlesTexProgram,
    ) -> Option<Self> {
        let WaylandSurfaceTexture::Texture(texture) = element.texture() else {
            return None;
        };
        let texture_size = texture.size();
        let (src, dst) = crop_of(element.src(), element.geometry(Scale::from(scale)), crop)?;
        Some(Self {
            texture_size,
            dst,
            source: Source::Crop { element, id, src },
            remapped: false,
            shape,
            arrow: Arrow::default(),
            program,
            extra: Vec::new(),
        })
    }

    pub(crate) fn mapped(
        mut self,
        from: Rectangle<i32, Physical>,
        to: Rectangle<i32, Physical>,
    ) -> Self {
        self.dst = map_rect(self.dst, from, to);
        self.remapped = true;
        self
    }

    pub(crate) fn from_texture(
        texture: GlesTexture,
        id: Id,
        commit: CommitCounter,
        src: Rectangle<f64, BufferCoords>,
        dst: Rectangle<i32, Physical>,
        shape: Shape,
        program: GlesTexProgram,
    ) -> Self {
        Self {
            texture_size: texture.size(),
            source: Source::Texture {
                texture,
                id,
                commit,
                src,
            },
            dst,
            remapped: false,
            shape,
            arrow: Arrow::default(),
            program,
            extra: Vec::new(),
        }
    }

    fn uniforms(&self) -> Vec<Uniform<'static>> {
        let src = self.src();
        let [arrow, slope, edge] = self.arrow.uniforms(texture_radius(src, self.dst, 1));
        let mut uniforms = vec![
            Uniform::new(
                "texture_size",
                [self.texture_size.w as f32, self.texture_size.h as f32],
            ),
            Uniform::new(
                "geometry",
                frame_to_texture_geometry(src, self.dst, self.shape.rect),
            ),
            Uniform::new("radius", texture_radius(src, self.dst, self.shape.radius)),
            Uniform::new("exponent", self.shape.exponent),
            arrow,
            slope,
            edge,
        ];
        uniforms.extend(self.extra.iter().cloned());
        uniforms
    }
}

impl<R: MalaysiaRenderer> Element for RoundedTextureElement<R> {
    fn id(&self) -> &Id {
        match &self.source {
            Source::Surface(element) => element.id(),
            Source::Crop { id, .. } | Source::Texture { id, .. } => id,
        }
    }

    fn current_commit(&self) -> CommitCounter {
        match &self.source {
            Source::Surface(element) | Source::Crop { element, .. } => element.current_commit(),
            Source::Texture { commit, .. } => *commit,
        }
    }

    fn src(&self) -> Rectangle<f64, BufferCoords> {
        match &self.source {
            Source::Surface(element) => element.src(),
            Source::Crop { src, .. } | Source::Texture { src, .. } => *src,
        }
    }

    fn transform(&self) -> Transform {
        match &self.source {
            Source::Surface(element) | Source::Crop { element, .. } => element.transform(),
            Source::Texture { .. } => Transform::Normal,
        }
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.dst
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        match &self.source {
            Source::Surface(element) if !self.remapped => element.damage_since(scale, commit),
            _ if commit == Some(self.current_commit()) => DamageSet::default(),
            _ => DamageSet::from_slice(&[Rectangle::from_size(self.dst.size)]),
        }
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        let Source::Surface(element) = &self.source else {
            return OpaqueRegions::default();
        };
        if self.remapped || self.arrow.is_some() {
            return OpaqueRegions::default();
        }
        let frame = Rectangle::new(self.shape.rect.loc - self.dst.loc, self.shape.rect.size);
        OpaqueRegions::from_slice(&opaque_inside_frame(
            element.opaque_regions(scale),
            frame,
            self.shape.radius,
        ))
    }

    fn alpha(&self) -> f32 {
        match &self.source {
            Source::Surface(element) | Source::Crop { element, .. } => element.alpha(),
            Source::Texture { .. } => 1.0,
        }
    }

    fn kind(&self) -> Kind {
        match &self.source {
            Source::Surface(element) => element.kind(),
            Source::Crop { .. } | Source::Texture { .. } => Kind::Unspecified,
        }
    }
}

impl<R: MalaysiaRenderer> RenderElement<R> for RoundedTextureElement<R> {
    fn draw(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, BufferCoords>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&smithay::utils::user_data::UserDataMap>,
    ) -> Result<(), R::Error> {
        match &self.source {
            Source::Surface(element) | Source::Crop { element, .. } => {
                R::gles_frame(frame)
                    .override_default_tex_program(self.program.clone(), self.uniforms());
                let drawn = element.draw(frame, src, dst, damage, opaque_regions, cache);
                R::gles_frame(frame).clear_tex_program_override();
                drawn
            }
            Source::Texture { texture, .. } => R::gles_frame(frame)
                .render_texture_from_to(
                    texture,
                    src,
                    dst,
                    damage,
                    opaque_regions,
                    Transform::Normal,
                    1.0,
                    Some(&self.program),
                    &self.uniforms(),
                )
                .map_err(R::from_gles_error),
        }
    }
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

    use super::{
        compile_dock_glass, compile_dock_solid, compile_glass, compile_glass_rim,
        compile_glass_shadow, compile_knob_glass, crop_of, frame_to_texture_geometry,
        opaque_inside_frame, shader_area, texture_radius,
    };
    use crate::test_support::{assert_hole_paints_nothing, try_test_renderer};

    #[test]
    fn the_glass_shaders_compile() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        assert!(compile_glass(&mut renderer).is_some());
        assert!(compile_glass_rim(&mut renderer).is_some());
        assert!(compile_dock_glass(&mut renderer).is_some());
        assert!(compile_dock_solid(&mut renderer).is_some());
        assert!(compile_knob_glass(&mut renderer).is_some());
        assert!(compile_glass_shadow(&mut renderer).is_some());
    }

    #[test]
    fn the_hole_of_a_border_leaves_every_pixel_it_paints() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let program = super::compile_border(&mut renderer).expect("the border shader compiles");
        let frame = Rectangle::<i32, Physical>::new((40, 30).into(), (300, 200).into());
        let line = ctdesign::Rgba::rgba8(255, 255, 255, 0.6);
        let glow = ctdesign::Rgba::rgba8(255, 255, 255, 0.4);
        let popover =
            |scale| super::Arrow::popover(super::ArrowEdge::Left, 60.0, ctdesign::POPOVER, scale);
        for scale in [1.0, 1.25, 2.0] {
            for arrow in [super::Arrow::default(), popover(scale)] {
                for (outer, glow) in [(true, ctdesign::Rgba::TRANSPARENT), (false, glow)] {
                    let radius = (20.0 * scale) as i32;
                    let border = crate::scene_elements::border_shader(
                        frame,
                        radius,
                        scale,
                        4.0,
                        outer,
                        (line, line, glow),
                        arrow,
                    );
                    assert_hole_paints_nothing(&mut renderer, &program, border, scale);
                }
            }
        }
    }

    #[test]
    fn the_holes_leave_the_corners_and_a_band_along_the_sides() {
        let frame = Rectangle::<i32, Physical>::new((10, 20).into(), (100, 60).into());
        let none = super::Arrow::default();
        let [wide, tall] = super::beyond_reach(frame, 8, none, 0);
        assert_eq!(wide, Some(Rectangle::new((13, 31).into(), (94, 38).into())));
        assert_eq!(tall, Some(Rectangle::new((21, 23).into(), (78, 54).into())));
        let small = Rectangle::<i32, Physical>::new((0, 0).into(), (20, 20).into());
        assert_eq!(super::beyond_reach(small, 8, none, 0), [None, None]);
    }

    #[test]
    fn opaque_regions_stop_at_the_frame_and_skip_the_rounded_corners() {
        let frame = Rectangle::<i32, Physical>::new(Point::from((10, 10)), Size::from((100, 50)));
        let opaque = opaque_inside_frame([Rectangle::from_size(Size::from((200, 200)))], frame, 8);
        let area: i32 = opaque.iter().map(|rect| rect.size.w * rect.size.h).sum();
        assert_eq!(area, 100 * 50 - 4 * 8 * 8);
        assert!(opaque.iter().all(|rect| frame.contains_rect(*rect)));
        assert!(!opaque.iter().any(|rect| rect.contains(frame.loc)));
    }

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
    fn a_crop_maps_to_the_same_share_of_the_source_and_stops_at_the_geometry() {
        let src = Rectangle::from_size(Size::from((400.0, 200.0)));
        let geometry = Rectangle::<i32, Physical>::new((100, 100).into(), (200, 100).into());
        let inside = Rectangle::<i32, Physical>::new((150, 120).into(), (100, 50).into());
        let (cropped, region) = crop_of(src, geometry, inside).unwrap();
        assert_eq!(region, inside);
        assert_eq!(
            cropped,
            Rectangle::new((100.0, 40.0).into(), (200.0, 100.0).into())
        );
        let past_the_corner = Rectangle::<i32, Physical>::new((280, 180).into(), (60, 60).into());
        let (cropped, region) = crop_of(src, geometry, past_the_corner).unwrap();
        assert_eq!(region, Rectangle::new((280, 180).into(), (20, 20).into()));
        assert_eq!(
            cropped,
            Rectangle::new((360.0, 160.0).into(), (40.0, 40.0).into())
        );
        let apart = Rectangle::<i32, Physical>::new((400, 400).into(), (10, 10).into());
        assert!(crop_of(src, geometry, apart).is_none());
    }

    #[test]
    fn texture_radius_scales_with_the_source_to_dest_ratio() {
        let src = Rectangle::from_size(Size::from((400.0, 200.0)));
        let dst = Rectangle::new(Point::from((0, 0)), Size::from((200, 100)));
        assert!((texture_radius(src, dst, 16) - 32.0).abs() < 0.01);
    }
}

fn map_rect(
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
    compile_shaped_texture(renderer, grade_graphics::shaders::ROUNDED_CLIP, &[])
        .inspect_err(|error| tracing::warn!(?error, "failed to compile rounded-clip shader"))
        .ok()
}

pub(crate) fn compile_veil(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    compile_shaped_texture(renderer, grade_graphics::shaders::VEIL, &[])
        .inspect_err(|error| tracing::warn!(?error, "failed to compile veil shader"))
        .ok()
}

pub(crate) fn compile_edge_fade(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    compile_shaped_texture(
        renderer,
        grade_graphics::shaders::EDGE_FADE,
        &[
            UniformName::new("strip", UniformType::_4f),
            UniformName::new("along", UniformType::_3f),
            UniformName::new("stops_at", UniformType::_4f),
            UniformName::new("stops_alpha", UniformType::_4f),
        ],
    )
    .inspect_err(|error| tracing::warn!(?error, "failed to compile edge-fade shader"))
    .ok()
}

fn compile_shaped_texture(
    renderer: &mut GlesRenderer,
    shader: &str,
    extra: &[UniformName<'_>],
) -> Result<GlesTexProgram, GlesError> {
    let mut uniforms = vec![
        UniformName::new("texture_size", UniformType::_2f),
        UniformName::new("geometry", UniformType::_4f),
        UniformName::new("radius", UniformType::_1f),
        UniformName::new("exponent", UniformType::_1f),
        UniformName::new("arrow", UniformType::_4f),
        UniformName::new("arrow_slope", UniformType::_1f),
        UniformName::new("arrow_edge", UniformType::_1f),
    ];
    uniforms.extend(extra.iter().cloned());
    renderer.compile_custom_texture_shader(shader, &uniforms)
}

fn glass_fill_uniforms() -> Vec<UniformName<'static>> {
    vec![
        UniformName::new("base", UniformType::_3f),
        UniformName::new("chroma", UniformType::_1f),
        UniformName::new("chroma_cap", UniformType::_1f),
        UniformName::new("lightness", UniformType::_1f),
        UniformName::new("panel", UniformType::_4f),
        UniformName::new("outer", UniformType::_4f),
        UniformName::new("edge", UniformType::_4f),
        UniformName::new("edge_cap", UniformType::_1f),
        UniformName::new("edge_offset", UniformType::_1f),
        UniformName::new("content", UniformType::_2f),
    ]
}

pub(crate) fn compile_glass(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    let mut uniforms = glass_fill_uniforms();
    uniforms.extend([
        UniformName::new("solid", UniformType::_3f),
        UniformName::new("settle", UniformType::_1f),
    ]);
    compile_wallpaper_shape(renderer, grade_graphics::shaders::GLASS, &uniforms)
        .inspect_err(|error| tracing::warn!(?error, "failed to compile glass shader"))
        .ok()
}

pub(crate) fn compile_glass_shadow(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    compile_wallpaper_shape(
        renderer,
        grade_graphics::shaders::GLASS_SHADOW,
        &[
            UniformName::new("panel", UniformType::_4f),
            UniformName::new("panel_radius", UniformType::_1f),
            UniformName::new("panel_exponent", UniformType::_1f),
            UniformName::new("shadow", UniformType::_3f),
            UniformName::new("shade", UniformType::_1f),
        ],
    )
    .inspect_err(|error| tracing::warn!(?error, "failed to compile glass shadow shader"))
    .ok()
}

pub(crate) fn compile_glass_rim(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    let mut uniforms = glass_fill_uniforms();
    uniforms.extend([
        UniformName::new("rim", UniformType::_3f),
        UniformName::new("rim_cap", UniformType::_1f),
        UniformName::new("opacity", UniformType::_1f),
        UniformName::new("width", UniformType::_1f),
    ]);
    compile_wallpaper_shape(renderer, grade_graphics::shaders::GLASS_RIM, &uniforms)
        .inspect_err(|error| tracing::warn!(?error, "failed to compile glass rim shader"))
        .ok()
}

fn dock_frame_uniforms() -> [UniformName<'static>; 13] {
    [
        UniformName::new("glass", UniformType::_4f),
        UniformName::new("rim", UniformType::_2f),
        UniformName::new("corner", UniformType::_3f),
        UniformName::new("rim_side", UniformType::_1f),
        UniformName::new("outline", UniformType::_4f),
        UniformName::new("glow_near", UniformType::_3f),
        UniformName::new("glow_far", UniformType::_3f),
        UniformName::new("tone", UniformType::_4f),
        UniformName::new("tone_outline", UniformType::_2f),
        UniformName::new("measured", UniformType::_1f),
        UniformName::new("plate", UniformType::_4f),
        UniformName::new("plate_edge", UniformType::_4f),
        UniformName::new("plate_ramp", UniformType::_4f),
    ]
}

pub(crate) fn compile_dock_glass(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    let mut uniforms = Vec::from(dock_frame_uniforms());
    uniforms.push(UniformName::new("lens", UniformType::_2f));
    compile_wallpaper_shape(renderer, grade_graphics::shaders::DOCK_GLASS, &uniforms)
        .inspect_err(|error| tracing::warn!(?error, "failed to compile Dock glass shader"))
        .ok()
}

pub(crate) fn compile_knob_glass(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    compile_wallpaper_shape(
        renderer,
        grade_graphics::shaders::KNOB_GLASS,
        &[
            UniformName::new("lens", UniformType::_2f),
            UniformName::new("tint", UniformType::_4f),
        ],
    )
    .inspect_err(|error| tracing::warn!(?error, "failed to compile knob glass shader"))
    .ok()
}

pub(crate) fn compile_dock_solid(renderer: &mut GlesRenderer) -> Option<GlesPixelProgram> {
    let mut uniforms = vec![
        UniformName::new("rect", UniformType::_4f),
        UniformName::new("radius", UniformType::_1f),
        UniformName::new("scale", UniformType::_1f),
        UniformName::new("exponent", UniformType::_1f),
        UniformName::new("arrow", UniformType::_4f),
        UniformName::new("arrow_slope", UniformType::_1f),
        UniformName::new("arrow_edge", UniformType::_1f),
    ];
    uniforms.extend(dock_frame_uniforms());
    renderer
        .compile_custom_pixel_shader(grade_graphics::shaders::DOCK_SOLID, &uniforms)
        .inspect_err(|error| tracing::warn!(?error, "failed to compile Dock solid shader"))
        .ok()
}

fn compile_wallpaper_shape(
    renderer: &mut GlesRenderer,
    shader: &str,
    own: &[UniformName<'static>],
) -> Result<GlesTexProgram, smithay::backend::renderer::gles::GlesError> {
    let mut uniforms = vec![
        UniformName::new("texture_size", UniformType::_2f),
        UniformName::new("geometry", UniformType::_4f),
        UniformName::new("radius", UniformType::_1f),
        UniformName::new("exponent", UniformType::_1f),
        UniformName::new("arrow", UniformType::_4f),
        UniformName::new("arrow_slope", UniformType::_1f),
        UniformName::new("arrow_edge", UniformType::_1f),
    ];
    uniforms.extend(own.iter().cloned());
    renderer.compile_custom_texture_shader(shader, &uniforms)
}

pub(crate) fn compile_shadow(renderer: &mut GlesRenderer) -> Option<GlesPixelProgram> {
    renderer
        .compile_custom_pixel_shader(
            grade_graphics::shaders::SHADOW,
            &[
                UniformName::new("rect", UniformType::_4f),
                UniformName::new("radius", UniformType::_1f),
                UniformName::new("blur", UniformType::_1f),
                UniformName::new("offset", UniformType::_2f),
                UniformName::new("scale", UniformType::_1f),
                UniformName::new("exponent", UniformType::_1f),
                UniformName::new("color", UniformType::_4f),
                UniformName::new("arrow", UniformType::_4f),
                UniformName::new("arrow_slope", UniformType::_1f),
                UniformName::new("arrow_edge", UniformType::_1f),
            ],
        )
        .inspect_err(|error| tracing::warn!(?error, "failed to compile shadow shader"))
        .ok()
}

pub(crate) fn compile_border(renderer: &mut GlesRenderer) -> Option<GlesPixelProgram> {
    renderer
        .compile_custom_pixel_shader(
            grade_graphics::shaders::BORDER,
            &[
                UniformName::new("rect", UniformType::_4f),
                UniformName::new("radius", UniformType::_1f),
                UniformName::new("width", UniformType::_1f),
                UniformName::new("side", UniformType::_1f),
                UniformName::new("scale", UniformType::_1f),
                UniformName::new("exponent", UniformType::_1f),
                UniformName::new("color", UniformType::_4f),
                UniformName::new("edge_color", UniformType::_4f),
                UniformName::new("glow", UniformType::_4f),
                UniformName::new("glow_depth", UniformType::_1f),
                UniformName::new("arrow", UniformType::_4f),
                UniformName::new("arrow_slope", UniformType::_1f),
                UniformName::new("arrow_edge", UniformType::_1f),
            ],
        )
        .inspect_err(|error| tracing::warn!(?error, "failed to compile border shader"))
        .ok()
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
