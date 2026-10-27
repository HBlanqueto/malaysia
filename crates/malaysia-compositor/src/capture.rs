//! Captures: the debug PNG dump (Bloque 7.1) and the screencopy protocol.
//!
//! `SIGUSR1` sets a flag, and the next frame re-renders the output to an
//! offscreen buffer (the same elements the window pass drew), reads it back,
//! flips it vertically (GL reads bottom-up) and saves a PNG. It is a
//! development tool for attaching captures to reports.
//!
//! The same offscreen render serves `ext-image-copy-capture-v1`: the frame
//! handler queues a [`PendingCapture`] and the backend renders it into the
//! client's SHM or DMA-BUF buffer (Bloque 11).

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use smithay::backend::allocator::{Format, Fourcc, Modifier};
use smithay::backend::drm::DrmNode;
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, BufferType, ExportMem, ImportDma, Offscreen, buffer_type};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{Buffer as BufferCoord, Logical, Physical, Rectangle, Scale, Size, Transform};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::image_copy_capture::Frame;
use smithay::wayland::shm::with_buffer_contents_mut;

use crate::cursor::OutputRenderElement;

/// DMA-BUF allocation info for capture, derived from the renderer.
///
/// `State` never sees the renderer (ADR 0012), so the backend fills this in
/// once after binding the renderer; `capture_constraints` serves it to clients.
pub(crate) struct CaptureDma {
    pub node: DrmNode,
    pub formats: Vec<(Fourcc, Vec<Modifier>)>,
}

/// What a queued capture frame should draw.
pub(crate) enum CaptureTarget {
    /// The whole output, at its mode size.
    Output { size: Size<i32, Physical> },
    /// One toplevel, at its usable (client surface) size.
    Window {
        id: malaysia_scene::scene::WindowId,
        size: Size<i32, Physical>,
    },
}

/// A capture frame waiting for the next render pass.
///
/// `ImageCopyCaptureHandler::frame` runs on `State`, which has no renderer, so
/// it queues the frame here and the backend completes it in `draw`/`render`.
pub(crate) struct PendingCapture {
    pub frame: Frame,
    pub target: CaptureTarget,
    /// Whether the client asked for the cursor to be composited.
    pub draw_cursor: bool,
}

/// Groups the renderer's formats by fourcc, keeping every modifier.
fn group_modifiers(formats: impl IntoIterator<Item = Format>) -> Vec<(Fourcc, Vec<Modifier>)> {
    let mut grouped: Vec<(Fourcc, Vec<Modifier>)> = Vec::new();
    for format in formats {
        match grouped.iter_mut().find(|(code, _)| *code == format.code) {
            Some((_, modifiers)) => modifiers.push(format.modifier),
            None => grouped.push((format.code, vec![format.modifier])),
        }
    }
    grouped
}

/// Derives the DMA-BUF constraints for capture from the renderer's device.
pub(crate) fn dmabuf_constraints(renderer: &GlesRenderer) -> Option<CaptureDma> {
    let node = EGLDevice::device_for_display(renderer.egl_context().display())
        .ok()
        .and_then(|device| device.try_get_render_node().ok().flatten())?;
    let formats = group_modifiers(renderer.dmabuf_formats().iter().copied());
    Some(CaptureDma { node, formats })
}

/// Captures the composed output to `$XDG_RUNTIME_DIR/malaysia/capture-*.png`.
///
/// The output is re-rendered offscreen (reading the presented buffer is
/// unreliable), with the backend's render transform and output scale so the
/// picture matches the screen. Returns the saved path and logs it.
pub(crate) fn capture_output(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement],
    size: Size<i32, Physical>,
    transform: Transform,
    scale: f64,
) -> std::io::Result<PathBuf> {
    let (w, h, flipped) = render_and_read(
        renderer,
        elements,
        size,
        transform,
        scale,
        [0.0, 0.0, 0.0, 1.0],
    )?;
    let dir = capture_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("capture-{}.png", stamp()));
    write_png(&path, &encode_png(w, h, &flipped)?)?;
    tracing::info!(path = %path.display(), "debug capture written");
    Ok(path)
}

/// Renders the whole output and encodes it as PNG bytes.
///
/// `clear` is the background: transparent for a window so only its content is
/// kept, opaque for an output.
pub(crate) fn screenshot_png(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement],
    size: Size<i32, Physical>,
    transform: Transform,
    scale: f64,
    clear: [f32; 4],
) -> std::io::Result<Vec<u8>> {
    render_png(renderer, elements, size, transform, scale, clear)
}

/// Renders the output, crops a logical region and encodes it as PNG bytes.
///
/// The whole output is rendered and then cropped, so the region sees exactly
/// what is on screen. `region` is in logical coordinates; `scale` converts it
/// to the physical pixels of the rendered buffer.
pub(crate) fn screenshot_region_png(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement],
    size: Size<i32, Physical>,
    transform: Transform,
    region: Rectangle<i32, Logical>,
    scale: f64,
) -> std::io::Result<Vec<u8>> {
    let (w, h, rgba) = render_and_read(
        renderer,
        elements,
        size,
        transform,
        scale,
        [0.06, 0.06, 0.09, 1.0],
    )?;
    let (width, height, cropped) = crop_region(region, scale, w, h, &rgba)?;
    encode_png(width, height, &cropped)
}

/// Writes PNG bytes to the screenshot path and logs it.
pub(crate) fn save_png(png: &[u8]) -> std::io::Result<PathBuf> {
    let path = screenshot_path()?;
    write_png(&path, png)?;
    tracing::info!(path = %path.display(), "screenshot saved");
    Ok(path)
}

/// Renders `elements` and encodes them as PNG.
fn render_png(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement],
    size: Size<i32, Physical>,
    transform: Transform,
    scale: f64,
    clear: [f32; 4],
) -> std::io::Result<Vec<u8>> {
    let (w, h, rgba) = render_and_read(renderer, elements, size, transform, scale, clear)?;
    encode_png(w, h, &rgba)
}

/// Crops a logical region out of a physical RGBA frame.
fn crop_region(
    region: Rectangle<i32, Logical>,
    scale: f64,
    w: usize,
    h: usize,
    rgba: &[u8],
) -> std::io::Result<(usize, usize, Vec<u8>)> {
    // Round to the nearest physical pixel; the clamp below guarantees the
    // crop never reads outside the output/window buffer.
    let x = (f64::from(region.loc.x) * scale).round() as i32;
    let y = (f64::from(region.loc.y) * scale).round() as i32;
    let width = (f64::from(region.size.w) * scale).round() as i32;
    let height = (f64::from(region.size.h) * scale).round() as i32;
    let x = x.clamp(0, w as i32);
    let y = y.clamp(0, h as i32);
    let width = width.min(w as i32 - x);
    let height = height.min(h as i32 - y);
    if width <= 0 || height <= 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "empty screenshot region",
        ));
    }
    let row_bytes = width as usize * 4;
    let mut cropped = Vec::with_capacity(row_bytes * height as usize);
    for row in y as usize..(y + height) as usize {
        let start = (row * w + x as usize) * 4;
        cropped.extend_from_slice(&rgba[start..start + row_bytes]);
    }
    Ok((width as usize, height as usize, cropped))
}

/// `Screenshot <date> at <time>.png` in the XDG Desktop directory.
///
/// `MALAYSIA_SCREENSHOT_DIR` is a test seam (like `MALAYSIA_SCALE`): it redirects the
/// save so a check does not touch the user's real Desktop. A name already taken
/// (two captures in the same second) gets a ` (2)`, ` (3)` suffix.
fn screenshot_path() -> std::io::Result<PathBuf> {
    let dir = std::env::var_os("MALAYSIA_SCREENSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(crate::xdg::desktop_dir);
    fs::create_dir_all(&dir)?;
    let now = chrono::Local::now();
    let base = now.format("Screenshot %Y-%m-%d at %H.%M.%S").to_string();
    let mut path = dir.join(format!("{base}.png"));
    let mut sequence = 2;
    while path.exists() {
        path = dir.join(format!("{base} ({sequence}).png"));
        sequence += 1;
    }
    Ok(path)
}

/// Milliseconds since the Unix epoch, for a unique file name.
fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Encodes RGBA8 pixels as a PNG.
fn encode_png(width: usize, height: usize, rgba: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(bytes)
}

/// Writes already-encoded PNG bytes.
fn write_png(path: &Path, png: &[u8]) -> std::io::Result<()> {
    fs::write(path, png)
}

/// Renders `elements` offscreen and reads them back top-down as RGBA8.
///
/// The GL framebuffer is bottom-up, so the readback rows are flipped. `clear`
/// is the background the render starts from.
fn render_and_read(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement],
    size: Size<i32, Physical>,
    transform: Transform,
    scale: f64,
    clear: [f32; 4],
) -> std::io::Result<(usize, usize, Vec<u8>)> {
    let size_buf: Size<i32, BufferCoord> = Size::from((size.w, size.h));
    let region = Rectangle::from_size(size_buf);

    // Re-render the same elements offscreen, using the window's transform so
    // the orientation matches what is presented.
    let mut offscreen: GlesTexture = renderer
        .create_buffer(Fourcc::Abgr8888, size_buf)
        .map_err(|error| std::io::Error::other(format!("create_buffer failed: {error}")))?;
    let mut target = renderer
        .bind(&mut offscreen)
        .map_err(|error| std::io::Error::other(format!("bind offscreen failed: {error}")))?;
    let mut tracker = OutputDamageTracker::new(size, Scale::from(scale), transform);
    let result = tracker
        .render_output(renderer, &mut target, 0, elements, clear)
        .map_err(|error| std::io::Error::other(format!("offscreen render failed: {error}")))?;
    // The readback needs the GPU to have finished the render, or the mapping
    // comes back empty (the window pass ignores this sync; the capture waits).
    result
        .sync
        .wait()
        .map_err(|error| std::io::Error::other(format!("offscreen sync failed: {error}")))?;

    let mapping = renderer
        .copy_framebuffer(&target, region, Fourcc::Abgr8888)
        .map_err(|error| std::io::Error::other(format!("copy_framebuffer failed: {error}")))?;
    let bytes = renderer
        .map_texture(&mapping)
        .map_err(|error| std::io::Error::other(format!("map_texture failed: {error}")))?;

    let (w, h) = (size.w as usize, size.h as usize);
    let stride = w * 4;
    let mut flipped = vec![0u8; bytes.len()];
    for row in 0..h {
        let src = (h - 1 - row) * stride;
        let dst = row * stride;
        flipped[dst..dst + stride].copy_from_slice(&bytes[src..src + stride]);
    }
    Ok((w, h, flipped))
}

/// Renders `elements` into the buffer attached to an `ext-image-copy-capture`
/// frame.
///
/// Both paths render with `Transform::Flipped` to cancel GL's bottom-up
/// framebuffer, so the buffer is top-down and the caller reports
/// `Transform::Normal`. Only `Normal` output orientation is handled; a rotated
/// output captures unrotated content.
pub(crate) fn capture_into_buffer(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement],
    size: Size<i32, Physical>,
    scale: f64,
    clear: [f32; 4],
    buffer: &WlBuffer,
) -> Result<(), String> {
    match buffer_type(buffer) {
        Some(BufferType::Shm) => capture_into_shm(renderer, elements, size, scale, clear, buffer),
        Some(BufferType::Dma) => {
            capture_into_dmabuf(renderer, elements, size, scale, clear, buffer)
        }
        _ => Err("unsupported capture buffer type".into()),
    }
}

fn capture_into_dmabuf(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement],
    size: Size<i32, Physical>,
    scale: f64,
    clear: [f32; 4],
    buffer: &WlBuffer,
) -> Result<(), String> {
    let dmabuf = get_dmabuf(buffer).map_err(|error| format!("not a dmabuf: {error}"))?;
    // A clone shares the underlying buffer; binding needs a mutable handle.
    let mut dmabuf = dmabuf.clone();
    let mut target = renderer
        .bind(&mut dmabuf)
        .map_err(|error| format!("bind dmabuf failed: {error}"))?;
    let mut tracker = OutputDamageTracker::new(size, Scale::from(scale), Transform::Flipped);
    let result = tracker
        .render_output(renderer, &mut target, 0, elements, clear)
        .map_err(|error| format!("capture render failed: {error}"))?;
    result
        .sync
        .wait()
        .map_err(|error| format!("capture sync failed: {error}"))
}

/// Validates the client's SHM buffer against the capture size and returns the
/// copy geometry: `(offset, stride, row_bytes, rows)`.
fn shm_layout(
    pool_len: usize,
    data_offset: i32,
    data_width: i32,
    data_height: i32,
    data_stride: i32,
    width: i32,
    height: i32,
) -> Result<(usize, usize, usize, usize), String> {
    if width <= 0 || height <= 0 {
        return Err(format!("invalid capture size {width}x{height}"));
    }
    // shortcut: a size mismatch fails with CaptureFailureReason::Unknown; switch to BufferConstraints if capture ever needs to scale.
    if data_width != width || data_height != height {
        return Err(format!(
            "shm buffer is {data_width}x{data_height}, capture is {width}x{height}"
        ));
    }
    let offset = usize::try_from(data_offset)
        .map_err(|_| format!("negative shm buffer offset {data_offset}"))?;
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| format!("invalid capture width {width}"))?;
    let stride = usize::try_from(data_stride)
        .map_err(|_| format!("negative shm buffer stride {data_stride}"))?;
    if stride < row_bytes {
        return Err(format!(
            "shm buffer stride {data_stride} is smaller than a row ({row_bytes} bytes)"
        ));
    }
    let rows = usize::try_from(height).map_err(|_| format!("invalid capture height {height}"))?;
    let end = stride
        .checked_mul(rows)
        .and_then(|bytes| offset.checked_add(bytes))
        .ok_or_else(|| "shm buffer size overflows usize".to_string())?;
    if end > pool_len {
        return Err(format!("shm buffer needs {end} bytes, pool has {pool_len}"));
    }
    Ok((offset, stride, row_bytes, rows))
}

/// Copies `height` rows of `width * 4` bytes from `src` into the client's SHM
/// pool, honouring its offset and stride. Validates everything before writing.
#[allow(clippy::too_many_arguments)]
fn copy_into_shm_pool(
    pool: &mut [u8],
    data_offset: i32,
    data_width: i32,
    data_height: i32,
    data_stride: i32,
    src: &[u8],
    width: i32,
    height: i32,
) -> Result<(), String> {
    let (offset, stride, row_bytes, rows) = shm_layout(
        pool.len(),
        data_offset,
        data_width,
        data_height,
        data_stride,
        width,
        height,
    )?;
    let src_len = row_bytes
        .checked_mul(rows)
        .ok_or_else(|| "rendered image size overflows usize".to_string())?;
    if src.len() < src_len {
        return Err(format!(
            "rendered image has {} bytes, needs {src_len}",
            src.len()
        ));
    }
    for row in 0..rows {
        let src_start = row * row_bytes;
        let dst_start = offset + row * stride;
        pool[dst_start..dst_start + row_bytes]
            .copy_from_slice(&src[src_start..src_start + row_bytes]);
    }
    Ok(())
}

#[allow(unsafe_code)] // the SHM pool is the client's shared memory; Smithay only
// exposes it as a raw pointer for exactly this reason.
fn capture_into_shm(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement],
    size: Size<i32, Physical>,
    scale: f64,
    clear: [f32; 4],
    buffer: &WlBuffer,
) -> Result<(), String> {
    with_buffer_contents_mut(buffer, |ptr, len, data| {
        let fourcc = match data.format {
            wl_shm::Format::Xrgb8888 => Fourcc::Xrgb8888,
            wl_shm::Format::Argb8888 => Fourcc::Argb8888,
            _ => return Err("unsupported shm capture format".to_string()),
        };
        // Reject an unusable buffer before spending GPU time on the render.
        shm_layout(
            len,
            data.offset,
            data.width,
            data.height,
            data.stride,
            size.w,
            size.h,
        )
        .map_err(|error| format!("invalid shm capture buffer: {error}"))?;
        let size_buf: Size<i32, BufferCoord> = Size::from((size.w, size.h));
        let mut offscreen: GlesTexture = renderer
            .create_buffer(fourcc, size_buf)
            .map_err(|error| format!("create_buffer failed: {error}"))?;
        let mut tracker = OutputDamageTracker::new(size, Scale::from(scale), Transform::Flipped);
        {
            let mut target = renderer
                .bind(&mut offscreen)
                .map_err(|error| format!("bind offscreen failed: {error}"))?;
            let result = tracker
                .render_output(renderer, &mut target, 0, elements, clear)
                .map_err(|error| format!("capture render failed: {error}"))?;
            result
                .sync
                .wait()
                .map_err(|error| format!("capture sync failed: {error}"))?;

            let mapping = renderer
                .copy_framebuffer(&target, Rectangle::from_size(size_buf), fourcc)
                .map_err(|error| format!("copy_framebuffer failed: {error}"))?;
            let bytes = renderer
                .map_texture(&mapping)
                .map_err(|error| format!("map_texture failed: {error}"))?;

            // SAFETY: `ptr` and `len` are the client's whole SHM pool, as
            // reported by Smithay, and the slice lives only for this closure.
            let pool = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            copy_into_shm_pool(
                pool,
                data.offset,
                data.width,
                data.height,
                data.stride,
                bytes,
                size.w,
                size.h,
            )
        }
    })
    .map_err(|error| format!("shm buffer access failed: {error}"))?
}

/// The directory for captures: `$XDG_RUNTIME_DIR/malaysia` (or `/tmp/malaysia`).
fn capture_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf());
    base.join("malaysia-compositor")
}

#[cfg(test)]
mod tests {
    use super::{copy_into_shm_pool, crop_region, group_modifiers};
    use smithay::backend::allocator::{Format, Fourcc, Modifier};
    use smithay::utils::{Logical, Point, Rectangle, Size};

    const UNTOUCHED: u8 = 0xaa;

    #[test]
    fn modifiers_are_grouped_by_fourcc() {
        let formats = [
            Format {
                code: Fourcc::Argb8888,
                modifier: Modifier::Linear,
            },
            Format {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::Linear,
            },
            Format {
                code: Fourcc::Argb8888,
                modifier: Modifier::Invalid,
            },
        ];

        assert_eq!(
            group_modifiers(formats),
            vec![
                (Fourcc::Argb8888, vec![Modifier::Linear, Modifier::Invalid],),
                (Fourcc::Xrgb8888, vec![Modifier::Linear]),
            ]
        );
    }

    fn src(bytes: usize) -> Vec<u8> {
        (1..=bytes).map(|byte| byte as u8).collect()
    }

    #[test]
    fn copy_respects_a_nonzero_offset() {
        let mut pool = vec![UNTOUCHED; 24];
        let image = src(16);

        copy_into_shm_pool(&mut pool, 8, 2, 2, 8, &image, 2, 2).unwrap();

        assert_eq!(pool[..8], [UNTOUCHED; 8]);
        assert_eq!(pool[8..16], image[..8]);
        assert_eq!(pool[16..24], image[8..]);
    }

    #[test]
    fn copy_leaves_row_padding_intact() {
        let mut pool = vec![UNTOUCHED; 24];
        let image = src(16);

        copy_into_shm_pool(&mut pool, 0, 2, 2, 12, &image, 2, 2).unwrap();

        assert_eq!(pool[0..8], image[..8]);
        assert_eq!(pool[8..12], [UNTOUCHED; 4]);
        assert_eq!(pool[12..20], image[8..]);
        assert_eq!(pool[20..24], [UNTOUCHED; 4]);
    }

    #[test]
    fn short_buffer_height_fails_without_writing() {
        let mut pool = vec![UNTOUCHED; 16];
        let before = pool.clone();

        let result = copy_into_shm_pool(&mut pool, 0, 2, 1, 8, &src(16), 2, 2);

        assert!(result.is_err());
        assert_eq!(pool, before);
    }

    #[test]
    fn short_stride_fails() {
        let result = copy_into_shm_pool(&mut [0; 16], 0, 2, 1, 4, &src(8), 2, 1);
        assert!(result.is_err());
    }

    #[test]
    fn buffer_larger_than_the_pool_fails() {
        let result = copy_into_shm_pool(&mut [0; 16], 4, 2, 2, 8, &src(16), 2, 2);
        assert!(result.is_err());
    }

    #[test]
    fn negative_offset_fails() {
        let result = copy_into_shm_pool(&mut [0; 16], -1, 2, 1, 8, &src(8), 2, 1);
        assert!(result.is_err());
    }

    /// The cropped physical rectangle must fit inside the buffer and a tiny
    /// logical selection must not collapse to empty. The clamp at
    /// `capture.rs:189-192` provides the guarantee for regions that touch or
    /// exceed the edge.
    #[test]
    fn crop_region_stays_inside_buffer_and_keeps_small_selections() {
        let scale = 1.25;
        let w = 200;
        let h = 200;
        let rgba = vec![0u8; w * h * 4];

        // A region larger than the buffer is clamped to the buffer bounds.
        let oversized = Rectangle::<i32, Logical>::new(Point::from((0, 0)), Size::from((200, 200)));
        let (width, height, _) = crop_region(oversized, scale, w, h, &rgba).unwrap();
        assert_eq!(width, w);
        assert_eq!(height, h);

        // A one-logical-pixel selection stays non-empty.
        let tiny = Rectangle::<i32, Logical>::new(Point::from((1, 1)), Size::from((1, 1)));
        let (width, height, _) = crop_region(tiny, scale, w, h, &rgba).unwrap();
        assert_eq!(width, 1);
        assert_eq!(height, 1);
    }
}
