//! Debug PNG capture of the composed output (Bloque 7.1).
//!
//! A development tool, not a protocol: `SIGUSR1` sets a flag, and the next
//! frame re-renders the output to an offscreen buffer (the same elements the
//! window pass drew), reads it back, flips it vertically (GL reads bottom-up)
//! and saves a PNG. It exists to attach captures to reports; a real capture
//! protocol (`ext-image-copy-capture-v1` / `wlr-screencopy`) is planned for
//! Bloque 11.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Scale, Size};

use crate::cursor::OutputRenderElement;
use crate::session::winit_render_transform;

/// Captures the composed output to `$XDG_RUNTIME_DIR/yuki/capture-*.png`.
///
/// The output is re-rendered offscreen (reading the winit surface's back
/// buffer is unreliable), with the same render transform as the window so the
/// picture matches the screen. Returns the saved path and logs it.
pub(crate) fn capture_output(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
) -> std::io::Result<PathBuf> {
    eprintln!("capture: {} elements", elements.len());
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
    let mut tracker = OutputDamageTracker::new(
        size,
        Scale::from((1.0f64, 1.0f64)),
        winit_render_transform(),
    );
    let result = tracker
        .render_output(renderer, &mut target, 0, elements, [0.0, 0.0, 0.0, 1.0])
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

    let dir = capture_dir();
    fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("capture-{stamp}.png"));

    let file = fs::File::create(&path)?;
    let mut encoder = png::Encoder::new(file, w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&flipped)?;
    writer.finish()?;

    tracing::info!(path = %path.display(), "debug capture written");
    Ok(path)
}

/// The directory for captures: `$XDG_RUNTIME_DIR/yuki` (or `/tmp/yuki`).
fn capture_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf());
    base.join("yuki")
}
