//! Compositor-side window effects: the rounded frame.
//!
//! ADR 0020 applies these to every toplevel window, client- or server-decorated.
//! The shape clips the client surface to the window rectangle; a CSD client may
//! attach a larger buffer (its own margins), and only the window rectangle is
//! the window. The clip is done in the fragment stage with the ROUNDED_CLIP
//! texture shader from `grade-graphics`; subsurfaces share the same clip rect.

use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexProgram, GlesTexture, Uniform};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::Texture;
use smithay::utils::{Buffer as BufferCoords, Physical, Rectangle, Scale, Size, Transform};

/// Map a physical frame rectangle into a source texture's coordinate space.
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

/// Map a physical corner radius into a source texture's coordinate space.
fn texture_radius(
    src: Rectangle<f64, BufferCoords>,
    dst: Rectangle<i32, Physical>,
    radius: i32,
) -> f32 {
    let scale = src.size.w / f64::from(dst.size.w);
    (f64::from(radius) * scale) as f32
}

/// A surface or memory texture drawn with a rounded-rectangle clip.
///
/// This replaces the old CPU-banded `ClippedSurface`. One draw call per
/// surface, no offscreen buffer. The clip rectangle and radius are in the
/// coordinate space of the *window frame*; each subsurface gets the same frame
/// rectangle, so the corners line up across the whole window.
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
    /// The window frame rectangle in physical pixels; used for the clip.
    frame_rect: Rectangle<i32, Physical>,
    /// Corner radius in physical pixels.
    radius: i32,
    program: GlesTexProgram,
}

impl RoundedTextureElement {
    /// Wraps a Wayland surface element so it is clipped to `frame_rect`.
    pub(crate) fn from_surface_element(
        element: &WaylandSurfaceRenderElement<GlesRenderer>,
        frame_rect: Rectangle<i32, Physical>,
        radius: i32,
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
            dst: element.geometry(Scale::from(1.0)),
            transform: element.transform(),
            alpha: element.alpha(),
            kind: element.kind(),
            frame_rect,
            radius,
            program,
        })
    }

    /// Wraps an already-imported texture so it is clipped to `frame_rect`.
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

// Smithay re-exports the texture enum under a renderer-specific name.
use smithay::backend::renderer::element::surface::WaylandSurfaceTexture as WaylandSurfaceRenderElementTexture;

#[cfg(test)]
mod tests {
    use smithay::utils::{Buffer as BufferCoords, Physical, Point, Rectangle, Size};

    use super::{frame_to_texture_geometry, texture_radius};

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
        // Surface is shifted 30 physical px right/down; scaled by 2x -> -60 texture px.
        assert!((geo[0] - -60.0).abs() < 0.01);
        assert!((geo[1] - -60.0).abs() < 0.01);
        // Frame size 300x200 scaled by 2x -> 600x400.
        assert!((geo[2] - 600.0).abs() < 0.01);
        assert!((geo[3] - 400.0).abs() < 0.01);
    }

    #[test]
    fn texture_radius_scales_with_the_source_to_dest_ratio() {
        let src = Rectangle::from_size(Size::from((400.0, 200.0)));
        let dst = Rectangle::new(Point::from((0, 0)), Size::from((200, 100)));
        assert!((texture_radius(src, dst, 16) - 32.0).abs() < 0.01);
    }
}
