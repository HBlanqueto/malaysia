//! Compositor-side window effects: the rounded shape and the drop shadow.
//!
//! ADR 0020 applies these to every window, client- or server-decorated. The
//! shape clips the client surface to its `window_geometry`, not to the buffer:
//! a CSD client may attach a larger buffer (its own margins), and only the
//! window rectangle is the window. The shadow is a CPU-rasterized, premultiplied
//! RGBA bitmap cached per window by size, focus and scale.

use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, Kind, RenderElement};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::{ImportAll, Renderer, Texture};
use smithay::utils::{
    Buffer as BufferCoords, Physical, Point, Rectangle, Scale, Size, Transform,
    user_data::UserDataMap,
};
use yuki_design::{SHADOW, WINDOW};

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

/// Rasterizes the Neptunia window shadow for a logical window size.
///
/// Returns the premultiplied RGBA pixels, their pixel size and the margin the
/// bitmap extends around the window on every side.
pub(crate) fn rasterize_shadow(
    width: i32,
    height: i32,
    scale: f64,
    focused: bool,
) -> (Vec<u8>, i32, i32, i32) {
    let layers = [SHADOW.ambient, SHADOW.contact];
    let blur = layers
        .iter()
        .map(|layer| f64::from(layer.blur) * scale)
        .fold(0.0, f64::max);
    let offset = layers
        .iter()
        .map(|layer| f64::from(layer.y_offset.abs()) * scale)
        .fold(0.0, f64::max);
    let margin = (blur + offset).ceil() as i32 + 2;
    let w = (width.max(1) as f64 * scale).round() as i32 + margin * 2;
    let h = (height.max(1) as f64 * scale).round() as i32 + margin * 2;
    let window_w = (width.max(1) as f64 * scale).round() as i32;
    let window_h = (height.max(1) as f64 * scale).round() as i32;
    let radius = (f64::from(WINDOW.corner_radius) * scale).round() as i32;
    let focus = if focused { 1.0 } else { SHADOW.unfocused_scale };

    let mut alpha = vec![0.0f32; (w * h) as usize];
    for layer in layers {
        let mask = rounded_mask(
            w,
            h,
            margin,
            margin + (f64::from(layer.y_offset) * scale).round() as i32,
            window_w,
            window_h,
            radius,
        );
        let blurred = blur_mask(
            &mask,
            w,
            h,
            (f64::from(layer.blur) * scale / 2.0).round() as usize,
        );
        let weight = layer.opacity * focus;
        for (out, value) in alpha.iter_mut().zip(blurred) {
            *out += value * weight;
        }
    }

    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for (index, value) in alpha.into_iter().enumerate() {
        let a = value.clamp(0.0, 1.0);
        if a <= 0.0 {
            continue;
        }
        let offset = index * 4;
        pixels[offset] = (13.0 * a).round() as u8;
        pixels[offset + 1] = (18.0 * a).round() as u8;
        pixels[offset + 2] = (25.0 * a).round() as u8;
        pixels[offset + 3] = (a * 255.0).round() as u8;
    }
    (pixels, w, h, margin)
}

/// A binary mask of a rounded rectangle inside a larger bitmap.
fn rounded_mask(w: i32, h: i32, x: i32, y: i32, width: i32, height: i32, radius: i32) -> Vec<f32> {
    let radius = radius.min(width / 2).min(height / 2).max(0);
    let mut mask = vec![0.0f32; (w * h) as usize];
    for row in 0..height {
        for col in 0..width {
            let px = x + col;
            let py = y + row;
            if px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }
            if inside_rounded(col, row, width, height, radius) {
                mask[(py * w + px) as usize] = 1.0;
            }
        }
    }
    mask
}

/// Whether a pixel centre is inside the rounded rectangle.
fn inside_rounded(x: i32, y: i32, width: i32, height: i32, radius: i32) -> bool {
    let low = radius;
    let cx = x.clamp(low, (width - radius - 1).max(low));
    let cy = y.clamp(low, (height - radius - 1).max(low));
    let dx = f64::from(x - cx);
    let dy = f64::from(y - cy);
    dx * dx + dy * dy <= f64::from(radius).powi(2)
}

/// Three passes of a separable box blur, close enough to the token's Gaussian.
fn blur_mask(mask: &[f32], w: i32, h: i32, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return mask.to_vec();
    }
    let mut current = mask.to_vec();
    let mut scratch = vec![0.0f32; mask.len()];
    for pass in 0..3 {
        if pass % 2 == 0 {
            blur_horizontal(&current, &mut scratch, w, h, radius);
        } else {
            blur_vertical(&current, &mut scratch, w, h, radius);
        }
        std::mem::swap(&mut current, &mut scratch);
    }
    current
}

fn blur_horizontal(src: &[f32], dst: &mut [f32], w: i32, h: i32, radius: usize) {
    let window = (radius * 2 + 1) as f32;
    let width = w as usize;
    for row in 0..h {
        let base = (row * w) as usize;
        let mut sum: f32 = src[base] * radius as f32
            + (0..=radius)
                .map(|i| src[base + i.min(width - 1)])
                .sum::<f32>();
        for col in 0..width {
            dst[base + col] = sum / window;
            let leaving = col.saturating_sub(radius);
            let entering = (col + radius + 1).min(width - 1);
            sum += src[base + entering] - src[base + leaving];
        }
    }
}

fn blur_vertical(src: &[f32], dst: &mut [f32], w: i32, h: i32, radius: usize) {
    let window = (radius * 2 + 1) as f32;
    let width = w as usize;
    let height = h as usize;
    for col in 0..width {
        let mut sum: f32 = src[col] * radius as f32
            + (0..=radius)
                .map(|i| src[i.min(height - 1) * width + col])
                .sum::<f32>();
        for row in 0..height {
            dst[row * width + col] = sum / window;
            let leaving = row.saturating_sub(radius);
            let entering = (row + radius + 1).min(height - 1);
            sum += src[entering * width + col] - src[leaving * width + col];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{inside_rounded, rounded_bands};
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

    #[test]
    fn the_corner_circle_excludes_the_outer_pixel() {
        assert!(!inside_rounded(0, 0, 32, 32, 16));
        assert!(inside_rounded(16, 0, 32, 32, 16));
        assert!(inside_rounded(15, 15, 32, 32, 16));
    }
}
