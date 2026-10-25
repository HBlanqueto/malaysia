//! Compositor-side window effects: the rounded shape.
//!
//! ADR 0020 applies these to every window, client- or server-decorated. The
//! shape clips the client surface to its `window_geometry`, not to the buffer:
//! a CSD client may attach a larger buffer (its own margins), and only the
//! window rectangle is the window. The shadow moved to the GPU shadow shader in
//! `grade-graphics` (see `chrome.rs`).

use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, Kind, RenderElement};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::{ImportAll, Renderer, Texture};
use smithay::utils::{
    Buffer as BufferCoords, Physical, Point, Rectangle, Scale, Size, Transform,
    user_data::UserDataMap,
};

/// A client surface clipped to a rounded window rectangle.
///
/// The clip is applied per horizontal band, so the corners are rounded without
/// a shader. The source rectangle is cropped with the same ratio, which keeps
/// the client's pixels aligned to their window geometry.
pub(crate) struct ClippedSurface<R: Renderer> {
    inner: WaylandSurfaceRenderElement<R>,
    visible: Rectangle<i32, Physical>,
    radius: i32,
    scale: Scale<f64>,
}

impl<R: Renderer + ImportAll> ClippedSurface<R> {
    /// Wraps `inner` so it only paints inside `visible`, with rounded corners.
    pub(crate) fn new(
        inner: WaylandSurfaceRenderElement<R>,
        visible: Rectangle<i32, Physical>,
        radius: i32,
        scale: Scale<f64>,
    ) -> Option<Self> {
        let visible = visible.intersection(inner.geometry(scale))?;
        if visible.size.w <= 0 || visible.size.h <= 0 {
            return None;
        }
        Some(Self {
            inner,
            visible,
            radius,
            scale,
        })
    }

    /// The horizontal bands that make up the rounded rectangle.
    fn bands(&self) -> Vec<Rectangle<i32, Physical>> {
        rounded_bands(self.visible, self.radius)
    }

    /// Crops the full source to the part shown by `band`.
    fn crop(
        &self,
        src: Rectangle<f64, BufferCoords>,
        full: Rectangle<i32, Physical>,
        band: Rectangle<i32, Physical>,
    ) -> Rectangle<f64, BufferCoords> {
        let sx = src.size.w / f64::from(full.size.w);
        let sy = src.size.h / f64::from(full.size.h);
        Rectangle::new(
            Point::new(
                src.loc.x + f64::from(band.loc.x - full.loc.x) * sx,
                src.loc.y + f64::from(band.loc.y - full.loc.y) * sy,
            ),
            Size::new(f64::from(band.size.w) * sx, f64::from(band.size.h) * sy),
        )
    }
}

impl<R: Renderer + ImportAll> Element for ClippedSurface<R> {
    fn id(&self) -> &smithay::backend::renderer::element::Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn src(&self) -> Rectangle<f64, BufferCoords> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.visible
            .intersection(self.inner.geometry(scale))
            .unwrap_or_else(|| Rectangle::new(Point::from((0, 0)), Size::from((0, 0))))
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        _commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        DamageSet::from_slice(&[Rectangle::from_size(self.geometry(scale).size)])
    }

    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl<R> RenderElement<R> for ClippedSurface<R>
where
    R: Renderer + ImportAll,
    R::TextureId: Texture + 'static,
{
    fn draw(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, BufferCoords>,
        _dst: Rectangle<i32, Physical>,
        _damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        _cache: Option<&UserDataMap>,
    ) -> Result<(), R::Error> {
        let full = self.inner.geometry(self.scale);
        if self.inner.transform() != Transform::Normal {
            let damage = [Rectangle::from_size(full.size)];
            return self.inner.draw(frame, src, full, &damage, &[], None);
        }
        for band in self.bands() {
            let Some(band) = band.intersection(full) else {
                continue;
            };
            if band.size.w <= 0 || band.size.h <= 0 {
                continue;
            }
            let damage = [Rectangle::from_size(band.size)];
            self.inner
                .draw(frame, self.crop(src, full, band), band, &damage, &[], None)?;
        }
        Ok(())
    }
}

/// Splits a rounded rectangle into one-pixel bands in the corner zones and a
/// single band in the middle.
fn rounded_bands(rect: Rectangle<i32, Physical>, radius: i32) -> Vec<Rectangle<i32, Physical>> {
    let radius = radius.min(rect.size.w / 2).min(rect.size.h / 2).max(0);
    if radius == 0 {
        return vec![rect];
    }
    let mut bands = Vec::with_capacity((radius * 2 + 1) as usize);
    for row in 0..radius {
        let dy = f64::from(radius) - (f64::from(row) + 0.5);
        let inset = (f64::from(radius) - (f64::from(radius).powi(2) - dy * dy).max(0.0).sqrt())
            .ceil() as i32;
        let left = rect.loc.x + inset;
        let right = rect.loc.x + rect.size.w - inset;
        if right > left {
            bands.push(Rectangle::new(
                Point::new(left, rect.loc.y + row),
                Size::new(right - left, 1),
            ));
        }
    }
    if rect.size.h > radius * 2 {
        bands.push(Rectangle::new(
            Point::new(rect.loc.x, rect.loc.y + radius),
            Size::new(rect.size.w, rect.size.h - radius * 2),
        ));
    }
    for row in 0..radius {
        let dy = f64::from(radius) - (f64::from(row) + 0.5);
        let inset = (f64::from(radius) - (f64::from(radius).powi(2) - dy * dy).max(0.0).sqrt())
            .ceil() as i32;
        let left = rect.loc.x + inset;
        let right = rect.loc.x + rect.size.w - inset;
        if right > left {
            bands.push(Rectangle::new(
                Point::new(left, rect.loc.y + rect.size.h - 1 - row),
                Size::new(right - left, 1),
            ));
        }
    }
    bands
}

#[cfg(test)]
mod tests {
    use super::rounded_bands;
    use smithay::utils::{Point, Rectangle, Size};

    #[test]
    fn a_rounded_rectangle_keeps_the_middle_and_clips_the_corners() {
        let rect = Rectangle::new(Point::from((10, 20)), Size::from((100, 60)));
        let bands = rounded_bands(rect, 16);
        assert!(
            bands
                .iter()
                .any(|band| band.loc == Point::from((10, 36))
                    && band.size == Size::from((100, 28))),
            "the middle band keeps the full width"
        );
        assert!(
            bands[0].loc.x > rect.loc.x && bands[0].size.w < rect.size.w,
            "the first row is inset on both corners"
        );
    }
}
