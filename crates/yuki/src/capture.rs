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

use smithay::backend::allocator::{Fourcc, Modifier};
use smithay::backend::drm::DrmNode;
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, BufferType, ExportMem, ImportDma, Offscreen, buffer_type};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Scale, Size, Transform};
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
        id: yuki_scene::scene::WindowId,
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
}

/// Derives the DMA-BUF constraints for capture from the renderer's device.
pub(crate) fn dmabuf_constraints(renderer: &GlesRenderer) -> Option<CaptureDma> {
    let node = EGLDevice::device_for_display(renderer.egl_context().display())
        .ok()
        .and_then(|device| device.try_get_render_node().ok().flatten())?;
    let formats = renderer
        .dmabuf_formats()
        .iter()
        .map(|format| (format.code, vec![format.modifier]))
        .collect();
    Some(CaptureDma { node, formats })
}

/// Captures the composed output to `$XDG_RUNTIME_DIR/yuki/capture-*.png`.
///
/// The output is re-rendered offscreen (reading the presented buffer is
/// unreliable), with the backend's render transform so the picture matches the
/// screen. Returns the saved path and logs it.
pub(crate) fn capture_output(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    transform: Transform,
) -> std::io::Result<PathBuf> {
    eprintln!("capture: {} elements", elements.len());
    let (w, h, flipped) =
        render_and_read(renderer, elements, size, transform, [0.0, 0.0, 0.0, 1.0])?;
    let dir = capture_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("capture-{}.png", stamp()));
    write_png(&path, w, h, &flipped)?;
    tracing::info!(path = %path.display(), "debug capture written");
    Ok(path)
}

/// Saves a screenshot to the user's Desktop.
///
/// The destination is the XDG Desktop directory (localised, see
/// [`crate::xdg::desktop_dir`]), not a fixed name. `clear` is the background:
/// transparent for a window so only its content is kept, opaque for an output.
pub(crate) fn screenshot(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    transform: Transform,
    clear: [f32; 4],
) -> std::io::Result<PathBuf> {
    let (w, h, flipped) = render_and_read(renderer, elements, size, transform, clear)?;
    // `YUKI_SCREENSHOT_DIR` is a test seam (like `YUKI_SCALE`): it redirects
    // the save so a check does not touch the user's real Desktop.
    let dir = std::env::var_os("YUKI_SCREENSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(crate::xdg::desktop_dir);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("Screenshot-{}.png", stamp()));
    write_png(&path, w, h, &flipped)?;
    tracing::info!(path = %path.display(), "screenshot saved");
    Ok(path)
}

/// Milliseconds since the Unix epoch, for a unique file name.
fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Writes `rgba` as an RGBA8 PNG.
fn write_png(path: &Path, width: usize, height: usize, rgba: &[u8]) -> std::io::Result<()> {
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(())
}

/// Renders `elements` offscreen and reads them back top-down as RGBA8.
///
/// The GL framebuffer is bottom-up, so the readback rows are flipped. `clear`
/// is the background the render starts from.
fn render_and_read(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    transform: Transform,
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
    let mut tracker = OutputDamageTracker::new(size, Scale::from((1.0f64, 1.0f64)), transform);
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
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    clear: [f32; 4],
    buffer: &WlBuffer,
) -> Result<(), String> {
    match buffer_type(buffer) {
        Some(BufferType::Shm) => capture_into_shm(renderer, elements, size, clear, buffer),
        Some(BufferType::Dma) => capture_into_dmabuf(renderer, elements, size, clear, buffer),
        _ => Err("unsupported capture buffer type".into()),
    }
}

fn capture_into_dmabuf(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    clear: [f32; 4],
    buffer: &WlBuffer,
) -> Result<(), String> {
    let dmabuf = get_dmabuf(buffer).map_err(|error| format!("not a dmabuf: {error}"))?;
    // A clone shares the underlying buffer; binding needs a mutable handle.
    let mut dmabuf = dmabuf.clone();
    let mut target = renderer
        .bind(&mut dmabuf)
        .map_err(|error| format!("bind dmabuf failed: {error}"))?;
    let mut tracker =
        OutputDamageTracker::new(size, Scale::from((1.0f64, 1.0f64)), Transform::Flipped);
    let result = tracker
        .render_output(renderer, &mut target, 0, elements, clear)
        .map_err(|error| format!("capture render failed: {error}"))?;
    result
        .sync
        .wait()
        .map_err(|error| format!("capture sync failed: {error}"))
}

#[allow(unsafe_code)] // the SHM pool is the client's shared memory; Smithay only
// exposes it as a raw pointer for exactly this reason.
fn capture_into_shm(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    clear: [f32; 4],
    buffer: &WlBuffer,
) -> Result<(), String> {
    with_buffer_contents_mut(buffer, |ptr, _len, data| {
        let fourcc = match data.format {
            wl_shm::Format::Xrgb8888 => Fourcc::Xrgb8888,
            wl_shm::Format::Argb8888 => Fourcc::Argb8888,
            _ => return Err("unsupported shm capture format".to_string()),
        };
        let size_buf: Size<i32, BufferCoord> = Size::from((size.w, size.h));
        let mut offscreen: GlesTexture = renderer
            .create_buffer(fourcc, size_buf)
            .map_err(|error| format!("create_buffer failed: {error}"))?;
        let mut tracker =
            OutputDamageTracker::new(size, Scale::from((1.0f64, 1.0f64)), Transform::Flipped);
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

            let src_stride = size.w as usize * 4;
            let dst_stride = data.stride as usize;
            let needed = dst_stride * data.height as usize;
            // SAFETY: the pointer is valid for the duration of this closure and
            // the pool is the buffer's own; only this client's memory is written.
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr, needed) };
            for row in 0..size.h as usize {
                let src = &bytes[row * src_stride..row * src_stride + src_stride];
                let dst = &mut dst[row * dst_stride..row * dst_stride + src_stride];
                dst.copy_from_slice(src);
            }
        }
        Ok(())
    })
    .map_err(|error| format!("shm buffer access failed: {error}"))?
}

/// The directory for captures: `$XDG_RUNTIME_DIR/yuki` (or `/tmp/yuki`).
fn capture_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf());
    base.join("yuki")
}
