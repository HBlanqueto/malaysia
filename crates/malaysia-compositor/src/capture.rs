use crate::config::ScreenshotTarget;
use crate::scene_elements::{build_output_elements, build_window_elements};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use smithay::backend::allocator::{Format, Fourcc, Modifier};
use smithay::backend::drm::DrmNode;
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{
    Bind, BufferType, ContextId, ExportMem, ImportDma, Offscreen, Renderer, buffer_type,
};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{Buffer as BufferCoord, Logical, Physical, Rectangle, Scale, Size, Transform};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::image_copy_capture::{CaptureFailureReason, Frame};
use smithay::wayland::shm::with_buffer_contents_mut;

use crate::cursor::{Cursors, OutputRenderElement};
use crate::state::State;

pub(crate) struct CaptureDma {
    pub node: DrmNode,
    pub formats: Vec<(Fourcc, Vec<Modifier>)>,
}

pub(crate) enum CaptureTarget {
    Output {
        size: Size<i32, Physical>,
    },
    Window {
        id: malaysia_scene::scene::WindowId,
        size: Size<i32, Physical>,
    },
}

pub(crate) struct PendingCapture {
    pub frame: Frame,
    pub target: CaptureTarget,
    pub draw_cursor: bool,
}

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

pub(crate) fn dmabuf_constraints(renderer: &GlesRenderer) -> Option<CaptureDma> {
    let node = EGLDevice::device_for_display(renderer.egl_context().display())
        .ok()
        .and_then(|device| device.try_get_render_node().ok().flatten())?;
    let formats = group_modifiers(renderer.dmabuf_formats().iter().copied());
    Some(CaptureDma { node, formats })
}

pub(crate) fn capture_output(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
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

pub(crate) fn screenshot_png(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    transform: Transform,
    scale: f64,
    clear: [f32; 4],
) -> std::io::Result<Vec<u8>> {
    let (w, h, rgba) = render_and_read(renderer, elements, size, transform, scale, clear)?;
    encode_png(w, h, &rgba)
}

pub(crate) fn screenshot_region_png(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
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
        ctdesign::DESKTOP_CLEAR.premultiplied(),
    )?;
    let (width, height, cropped) = crop_region(region, scale, w, h, &rgba)?;
    encode_png(width, height, &cropped)
}

pub(crate) fn save_png(png: &[u8]) -> std::io::Result<PathBuf> {
    let path = screenshot_path()?;
    write_png(&path, png)?;
    tracing::info!(path = %path.display(), "screenshot saved");
    Ok(path)
}

fn crop_region(
    region: Rectangle<i32, Logical>,
    scale: f64,
    w: usize,
    h: usize,
    rgba: &[u8],
) -> std::io::Result<(usize, usize, Vec<u8>)> {
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

fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

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

fn write_png(path: &Path, png: &[u8]) -> std::io::Result<()> {
    fs::write(path, png)
}

fn render_and_read(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    transform: Transform,
    scale: f64,
    clear: [f32; 4],
) -> std::io::Result<(usize, usize, Vec<u8>)> {
    let size_buf: Size<i32, BufferCoord> = Size::from((size.w, size.h));
    let region = Rectangle::from_size(size_buf);

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

pub(crate) fn serve_pending(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    elements: &[OutputRenderElement<GlesRenderer>],
) {
    let scale = state.scale.fractional_scale();
    for pending in std::mem::take(&mut state.pending_captures) {
        if state.locked {
            pending.frame.fail(CaptureFailureReason::Unknown);
            continue;
        }
        let (size, window_elements) = match &pending.target {
            CaptureTarget::Output { size } => (*size, None),
            CaptureTarget::Window { id, size } => {
                let Some(surface) = state.windows.surface_of(*id).cloned() else {
                    pending.frame.fail(CaptureFailureReason::Unknown);
                    continue;
                };
                (
                    *size,
                    Some(crate::scene_elements::build_window_elements(
                        renderer, &surface, scale,
                    )),
                )
            }
        };
        let without_cursor: Vec<OutputRenderElement<GlesRenderer>>;
        let drawn: &[OutputRenderElement<GlesRenderer>] = match &pending.target {
            CaptureTarget::Output { .. } if !pending.draw_cursor => {
                without_cursor =
                    crate::scene_elements::build_output_elements(state, renderer, cursor, false);
                &without_cursor
            }
            _ => window_elements.as_deref().unwrap_or(elements),
        };
        let damage = Rectangle::from_size(Size::from((size.w, size.h)));
        match capture_into_buffer(
            renderer,
            drawn,
            size,
            scale,
            ctdesign::DESKTOP_CLEAR.premultiplied(),
            &pending.frame.buffer(),
        ) {
            Ok(()) => pending.frame.success(
                Transform::Normal,
                Some(vec![damage]),
                Duration::from(state.clock.now()),
            ),
            Err(error) => {
                tracing::warn!(%error, "screencopy frame failed");
                pending.frame.fail(CaptureFailureReason::Unknown);
            }
        }
    }
}

pub(crate) fn capture_into_buffer(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
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
    elements: &[OutputRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    scale: f64,
    clear: [f32; 4],
    buffer: &WlBuffer,
) -> Result<(), String> {
    let dmabuf = get_dmabuf(buffer).map_err(|error| format!("not a dmabuf: {error}"))?;
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

#[allow(unsafe_code)]
fn capture_into_shm(
    renderer: &mut GlesRenderer,
    elements: &[OutputRenderElement<GlesRenderer>],
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
        let mut offscreen = shm_offscreen(renderer, fourcc, size_buf)?;
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

            // SAFETY: `ptr` and `len` cover the client's whole SHM pool, which stays mapped here.
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

struct ShmOffscreen {
    context: ContextId<GlesTexture>,
    fourcc: Fourcc,
    size: Size<i32, BufferCoord>,
    texture: GlesTexture,
}

thread_local! {
    static SHM_OFFSCREEN: RefCell<Option<ShmOffscreen>> = const { RefCell::new(None) };
}

// niri a8562a1 src/render_helpers/offscreen.rs: one texture per size, format and GL context.
fn shm_offscreen(
    renderer: &mut GlesRenderer,
    fourcc: Fourcc,
    size: Size<i32, BufferCoord>,
) -> Result<GlesTexture, String> {
    let context = renderer.context_id();
    SHM_OFFSCREEN.with_borrow_mut(|cached| {
        if let Some(cached) = cached.as_ref().filter(|cached| {
            cached.context == context && cached.fourcc == fourcc && cached.size == size
        }) {
            return Ok(cached.texture.clone());
        }
        let texture: GlesTexture = renderer
            .create_buffer(fourcc, size)
            .map_err(|error| format!("create_buffer failed: {error}"))?;
        *cached = Some(ShmOffscreen {
            context,
            fourcc,
            size,
            texture: texture.clone(),
        });
        Ok(texture)
    })
}

fn capture_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf());
    base.join("malaysia")
}

pub(crate) fn save_screenshot(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    output_size: Size<i32, Physical>,
    transform: Transform,
    request: crate::state::ScreenshotRequest,
) -> std::io::Result<ScreenshotOutcome> {
    let scale = state.scale.fractional_scale();
    let png = match request {
        crate::state::ScreenshotRequest::Focused => match state.focused_window() {
            Some(id) => capture_window_png(state, renderer, id, transform),
            None => capture_output_png(state, renderer, cursor, output_size, transform, scale),
        },
        crate::state::ScreenshotRequest::Output => {
            capture_output_png(state, renderer, cursor, output_size, transform, scale)
        }
        crate::state::ScreenshotRequest::Window(id) => {
            capture_window_png(state, renderer, id, transform)
        }
        crate::state::ScreenshotRequest::Region(region) => {
            let elements = screenshot_elements(state, renderer, cursor);
            crate::capture::screenshot_region_png(
                renderer,
                &elements,
                output_size,
                transform,
                region,
                scale,
            )
        }
    }?;

    if matches!(state.config.screenshot.target, ScreenshotTarget::Clipboard) {
        state.set_clipboard_png(png);
        Ok(ScreenshotOutcome::Copied)
    } else {
        crate::capture::save_png(&png)?;
        Ok(ScreenshotOutcome::Saved)
    }
}

pub(crate) enum ScreenshotOutcome {
    Saved,
    Copied,
}

pub(crate) fn screenshot_elements(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
) -> Vec<OutputRenderElement<GlesRenderer>> {
    let include_cursor = state.config.screenshot.show_pointer;
    build_output_elements(state, renderer, cursor, include_cursor)
}

pub(crate) fn capture_window_png(
    state: &State,
    renderer: &mut GlesRenderer,
    id: malaysia_scene::scene::WindowId,
    transform: Transform,
) -> std::io::Result<Vec<u8>> {
    let scale = state.scale.fractional_scale();
    let surface = state
        .windows
        .surface_of(id)
        .cloned()
        .ok_or_else(|| std::io::Error::other("screenshot: the window is gone"))?;
    let rect = state
        .windows
        .usable_rect(id)
        .ok_or_else(|| std::io::Error::other("screenshot: the window has no rect"))?;
    let size = rect.size.to_f64().to_physical(scale).to_i32_round();
    let elements = build_window_elements(renderer, &surface, scale);
    crate::capture::screenshot_png(
        renderer,
        &elements,
        size,
        transform,
        scale,
        [0.0, 0.0, 0.0, 0.0],
    )
}

pub(crate) fn capture_output_png(
    state: &mut State,
    renderer: &mut GlesRenderer,
    cursor: &Cursors,
    size: Size<i32, Physical>,
    transform: Transform,
    scale: f64,
) -> std::io::Result<Vec<u8>> {
    let elements = screenshot_elements(state, renderer, cursor);
    crate::capture::screenshot_png(
        renderer,
        &elements,
        size,
        transform,
        scale,
        ctdesign::DESKTOP_CLEAR.premultiplied(),
    )
}

pub(crate) fn log_screenshot(result: std::io::Result<ScreenshotOutcome>) {
    match result {
        Ok(ScreenshotOutcome::Copied) => tracing::info!("screenshot copied to the clipboard"),
        Ok(ScreenshotOutcome::Saved) => {}
        Err(error) => tracing::warn!(%error, "screenshot failed"),
    }
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

    #[test]
    fn crop_region_stays_inside_buffer_and_keeps_small_selections() {
        let scale = 1.25;
        let w = 200;
        let h = 200;
        let rgba = vec![0u8; w * h * 4];

        let oversized = Rectangle::<i32, Logical>::new(Point::from((0, 0)), Size::from((200, 200)));
        let (width, height, _) = crop_region(oversized, scale, w, h, &rgba).unwrap();
        assert_eq!(width, w);
        assert_eq!(height, h);

        let tiny = Rectangle::<i32, Logical>::new(Point::from((1, 1)), Size::from((1, 1)));
        let (width, height, _) = crop_region(tiny, scale, w, h, &rgba).unwrap();
        assert_eq!(width, 1);
        assert_eq!(height, 1);
    }
}
