//! The wallpaper component: the desktop picture on the `Background` layer.
//!
//! Like macOS (`com.apple.wallpaper`), the *current selection* lives in the
//! wallpaper domain store — `~/.local/share/yuki/wallpaper.domain`
//! (`~/Library/Application Support`). This component watches that store and
//! re-renders when it changes. The image is drawn with CelestialKit's GLES
//! renderer (ADR 0024) scaled to cover the output (macOS default `fill`).

use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use sctk::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry,
    globals::GlobalData,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, Dispatch, Proxy, QueueHandle, delegate_dispatch,
            globals::registry_queue_init,
            protocol::{wl_buffer::WlBuffer, wl_output, wl_surface::WlSurface},
        },
        protocols::wp::{
            fractional_scale::v1::client::{
                wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
                wp_fractional_scale_v1::{Event as FractionalScaleEvent, WpFractionalScaleV1},
            },
            viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
        },
        protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1},
    },
    registry::{ProvidesRegistryState, RegistryState},
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure, LayerSurfaceData,
    },
};

use celestial_kit::gl::{GlesRenderer, Texture};
use yuki_design::Rgba;

use crate::present::EglPresenter;
use crate::theme::data_dir;

/// The decoded wallpaper, ready to upload as an RGBA8 texture.
#[derive(Clone)]
struct DecodedImage {
    size: (u32, u32),
    rgba: Vec<u8>,
}

struct Wallpaper {
    registry: RegistryState,
    compositor: CompositorState,
    output: OutputState,
    layer_shell: Option<LayerShell>,
    viewporter: Option<WpViewporter>,
    fractional: Option<WpFractionalScaleManagerV1>,

    surface: Option<WlSurface>,
    layer_surface: Option<LayerSurface>,
    viewport: Option<WpViewport>,

    renderer: Option<GlesRenderer>,
    texture: Option<Texture>,
    image: Option<DecodedImage>,
    presenter: crate::present::EglPresenter,

    /// The preferred fractional scale of the surface (1.0 until told otherwise).
    scale: f64,
    logical_size: (u32, u32),
    buffer_size: (u32, u32),

    store_path: PathBuf,
    state_path: PathBuf,
    current: Option<PathBuf>,
    /// The store's mtime at the last check; `None` until the first check, so
    /// a session without a store still loads the default once.
    last_modified: Option<Option<SystemTime>>,
}

/// Connects to the compositor and runs the wallpaper component until the
/// compositor closes it or the connection dies.
pub fn run() -> anyhow::Result<()> {
    let store_path = data_dir().join("wallpaper.domain");
    let state_path = data_dir().join("wallpaper.state");

    let conn = Connection::connect_to_env()
        .map_err(|_| anyhow::anyhow!("WAYLAND_DISPLAY is not set to a reachable socket"))?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let mut state = Wallpaper {
        registry: RegistryState::new(&globals),
        compositor: CompositorState::bind(&globals, &qh)?,
        output: OutputState::new(&globals, &qh),
        layer_shell: LayerShell::bind(&globals, &qh).ok(),
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        fractional: globals.bind(&qh, 1..=1, ()).ok(),
        surface: None,
        layer_surface: None,
        viewport: None,
        renderer: None,
        texture: None,
        image: None,
        presenter: EglPresenter::new(),
        scale: 1.0,
        logical_size: (0, 0),
        buffer_size: (0, 0),
        store_path,
        state_path,
        current: None,
        last_modified: None,
    };

    if state.layer_shell.is_none() {
        anyhow::bail!("the compositor does not advertise zwlr_layer_shell_v1");
    }

    state.create_surfaces(&qh);
    state.check_store();

    let mut event_loop: calloop::EventLoop<Wallpaper> = calloop::EventLoop::try_new()?;
    WaylandSource::new(conn.clone(), event_queue).insert(event_loop.handle())?;
    event_loop
        .handle()
        .insert_source(
            calloop::timer::Timer::from_duration(Duration::from_millis(500)),
            move |_, _, state: &mut Wallpaper| {
                state.check_store();
                calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(500))
            },
        )
        .expect("insert the wallpaper store timer");

    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}

impl Wallpaper {
    /// Creates the layer surface and the viewport/fractional-scale objects on it.
    fn create_surfaces(&mut self, qh: &QueueHandle<Self>) {
        let surface = self.compositor.create_surface(qh);
        self.surface = Some(surface.clone());

        let layer_shell = self.layer_shell.as_ref().expect("layer-shell present");
        let layer_surface = layer_shell.create_layer_surface(
            qh,
            surface.clone(),
            Layer::Background,
            Some("yuki-wallpaper"),
            None,
        );
        // Anchored to every edge with no explicit size: the compositor gives us
        // the full output in the first configure.
        layer_surface.set_size(0, 0);
        layer_surface.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_exclusive_zone(0);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        self.layer_surface = Some(layer_surface);

        if let Some(viewporter) = &self.viewporter {
            self.viewport =
                Some(viewporter.get_viewport::<WlSurface, Self>(&surface, qh, surface.clone()));
        }
        if let Some(fractional) = &self.fractional {
            let _fractional_scale =
                fractional.get_fractional_scale::<WlSurface, Self>(&surface, qh, surface.clone());
        }

        // Commit once so the compositor sends the first configure; the render
        // happens on that configure (the anchors/size are known only then).
        surface.commit();
    }

    /// Applies a configure from the compositor and redraws.
    fn configure(&mut self, conn: &Connection, configure: LayerSurfaceConfigure, _serial: u32) {
        let Some(_layer_surface) = self.layer_surface.clone() else {
            return;
        };
        // sctk already acknowledged this configure (see `LayerShell`'s dispatch).

        let (width, height) = configure.new_size;
        if width == 0 || height == 0 {
            return; // not a real size yet
        }
        self.logical_size = (width, height);

        let scale = self.scale.max(1.0);
        let (bw, bh) = (
            (width as f64 * scale).round().max(1.0) as u32,
            (height as f64 * scale).round().max(1.0) as u32,
        );
        self.buffer_size = (bw, bh);

        // The integer part of the scale goes into wl_surface, the fractional
        // remainder is handled by viewport.
        // With a viewport, the destination size is the logical size and the
        // source is the real buffer size; leaving buffer_scale at 1 avoids
        // applying the scale twice (and keeps the source inside the buffer as
        // Smithay checks it). Without a viewport, fall back to the integer
        // buffer scale.
        if let Some(surface) = &self.surface {
            let buffer_scale = if self.viewport.is_some() {
                1
            } else {
                scale.round().max(1.0) as i32
            };
            surface.set_buffer_scale(buffer_scale);
        }
        if let Some(viewport) = &self.viewport {
            viewport.set_source(0.0, 0.0, bw as f64, bh as f64);
            viewport.set_destination(width as i32, height as i32);
        }

        // Create the GLES renderer on the first real configure.
        if self.renderer.is_none() {
            let display_ptr = conn.display().id().as_ptr().cast::<c_void>();
            // Safety: the display handle is the live Wayland connection's.
            let renderer =
                unsafe { GlesRenderer::new(display_ptr) }.expect("create the GLES renderer");
            self.renderer = Some(renderer);
        }

        // Upload the image if we decoded one before the context existed.
        if self.texture.is_none()
            && let Some(image) = self.image.clone()
        {
            self.upload_image(&image);
        }

        self.present();
    }

    /// (Re)uploads the decoded image as a texture.
    fn upload_image(&mut self, image: &DecodedImage) {
        self.texture = self
            .renderer
            .as_ref()
            .and_then(|renderer| renderer.create_texture_rgba(image.size, &image.rgba).ok());
    }

    /// Renders the current texture over an EGL window on the surface and
    /// presents it (no-op until both the renderer and the texture exist).
    ///
    /// Rendering is GLES (ADR 0024); presentation goes through EGL (Bloque
    /// 7c) — the swap becomes a dmabuf the compositor textures directly, so
    /// nothing is read back or copied through shared memory.
    fn present(&mut self) {
        let (Some(renderer), Some(texture)) = (&mut self.renderer, &self.texture) else {
            return;
        };
        let (w, h) = self.buffer_size;
        let Some(surface) = &self.surface else {
            return;
        };
        if let Err(error) = self.presenter.present(renderer, surface, texture, (w, h)) {
            eprintln!("wallpaper: present failed: {error}");
        }
    }

    /// Re-reads the wallpaper store if it changed on disk and re-renders.
    fn check_store(&mut self) {
        let modified = std::fs::metadata(&self.store_path)
            .and_then(|m| m.modified())
            .ok();
        if self.last_modified == Some(modified) {
            return;
        }
        self.last_modified = Some(modified);

        match load_wallpaper(&self.store_path) {
            Ok(Some((path, scale))) if self.current.as_deref() != Some(path.as_path()) => {
                if !scale.is_empty() && scale != "fill" {
                    eprintln!("wallpaper: scale '{scale}' is not implemented yet; using 'fill'");
                }
                eprintln!("wallpaper: {path:?}");
                self.current = Some(path.clone());
                match decode_image(&path) {
                    Ok(image) => {
                        let dominant = dominant(&image);
                        if let Err(error) = write_state(&self.state_path, dominant) {
                            eprintln!("wallpaper: failed to write the state: {error:#}");
                        }
                        self.upload_image(&image);
                        self.image = Some(image);
                        self.present();
                    }
                    Err(error) => eprintln!("wallpaper: failed to decode {path:?}: {error:#}"),
                }
            }
            Ok(_) => {}
            Err(error) => eprintln!("wallpaper: {error:#}"),
        }
    }
}

impl ProvidesRegistryState for Wallpaper {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    fn runtime_add_global(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: u32,
        _: &str,
        _: u32,
    ) {
    }
    fn runtime_remove_global(&mut self, _: &Connection, _: &QueueHandle<Self>, _: u32, _: &str) {}
}

impl OutputHandler for Wallpaper {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl CompositorHandler for Wallpaper {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Wallpaper {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        eprintln!("wallpaper: layer surface closed by the compositor");
        std::process::exit(0);
    }

    fn configure(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        serial: u32,
    ) {
        self.configure(conn, configure, serial);
    }
}

impl Dispatch<WpViewporter, (), Wallpaper> for Wallpaper {
    fn event(
        _: &mut Self,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_viewporter has no events")
    }
}

impl Dispatch<WpViewport, WlSurface, Wallpaper> for Wallpaper {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_viewport has no events")
    }
}

impl Dispatch<WpFractionalScaleManagerV1, (), Wallpaper> for Wallpaper {
    fn event(
        _: &mut Self,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_fractional_scale_manager_v1 has no events")
    }
}

impl Dispatch<WpFractionalScaleV1, WlSurface, Wallpaper> for Wallpaper {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        _: &WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let FractionalScaleEvent::PreferredScale { scale } = event {
            state.scale = scale as f64 / 120.0;
        }
    }
}

impl Dispatch<WlBuffer, (), Wallpaper> for Wallpaper {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // `wl_buffer.release`: the compositor is done with this buffer. We only
        // ever rewrite the same static image, so there is nothing to track.
    }
}

delegate_compositor!(Wallpaper);
delegate_output!(Wallpaper);
delegate_registry!(Wallpaper);

// The layer-shell protocol has no delegate macro of its own: the app state
// forwards its events to the `LayerShell` handle (see sctk's `wlr_layer`).
delegate_dispatch!(
    Wallpaper: [zwlr_layer_shell_v1::ZwlrLayerShellV1: GlobalData] => LayerShell
);
delegate_dispatch!(
    Wallpaper: [zwlr_layer_surface_v1::ZwlrLayerSurfaceV1: LayerSurfaceData] => LayerShell
);

// --- wallpaper store -------------------------------------------------------

/// The `wallpaper.domain` store, as macOS keeps `com.apple.wallpaper`.
#[derive(Debug, Default, serde::Deserialize)]
struct WallpaperStore {
    #[serde(default)]
    dir: String,
    #[serde(default)]
    scale: String,
    #[serde(default)]
    desktops: Desktops,
}

#[derive(Debug, Default, serde::Deserialize)]
struct Desktops {
    #[serde(default)]
    default: Option<String>,
}

/// Reads the current wallpaper image path from the store.
///
/// The store's selection wins. Without one (or without a store at all, as on
/// a fresh account) the default is [`yuki_design::WALLPAPER`] inside the
/// catalog folder; a custom folder that lacks it falls back to its first image.
fn load_wallpaper(store_path: &Path) -> anyhow::Result<Option<(PathBuf, String)>> {
    let store: WallpaperStore = match std::fs::read_to_string(store_path) {
        Ok(text) => toml::from_str(&text)?,
        Err(_) => WallpaperStore::default(),
    };
    if let Some(url) = &store.desktops.default
        && let Some(path) = file_url_to_path(url)
    {
        return Ok(Some((path, store.scale)));
    }
    let dir = if store.dir.is_empty() {
        PathBuf::from(yuki_design::WALLPAPER_DIR)
    } else {
        PathBuf::from(&store.dir)
    };
    let default = dir.join(yuki_design::WALLPAPER);
    if default.is_file() {
        return Ok(Some((default, store.scale)));
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg"
                    )
                })
        })
        .collect();
    entries.sort();
    Ok(entries.first().cloned().map(|p| (p, store.scale)))
}

/// Turns a `file://` URL into a path, decoding `%XX` escapes.
fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let bytes = percent_decode(rest.as_bytes());
    Some(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
}

fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodes a PNG or JPEG into top-down RGBA8 pixels.
fn decode_image(path: &Path) -> anyhow::Result<DecodedImage> {
    let bytes = std::fs::read(path)?;
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        decode_png(&bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        decode_jpeg(&bytes)
    } else {
        anyhow::bail!("unsupported image format (expected PNG or JPEG)")
    }
}

/// Decodes a PNG, normalizing 16-bit and indexed images to 8-bit colour.
fn decode_png(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;

    let width = info.width;
    let height = info.height;
    let bytes = &buf[..info.buffer_size()];
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    match info.color_type {
        png::ColorType::Rgb => {
            for px in bytes.chunks(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        png::ColorType::Rgba => rgba.extend_from_slice(bytes),
        png::ColorType::Grayscale => {
            for &value in bytes {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for px in bytes.chunks(2) {
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        color => anyhow::bail!("unsupported PNG colour type {color:?}"),
    }
    Ok(DecodedImage {
        size: (width, height),
        rgba,
    })
}

/// Decodes a JPEG into RGBA8 with `zune-jpeg`.
fn decode_jpeg(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };

    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let rgba = decoder.decode()?;
    let info = decoder
        .info()
        .ok_or_else(|| anyhow::anyhow!("the JPEG headers were not decoded"))?;
    Ok(DecodedImage {
        size: (u32::from(info.width), u32::from(info.height)),
        rgba,
    })
}

/// The dominant colour, sampled on a grid and weighted by chroma.
fn dominant(image: &DecodedImage) -> Option<Rgba> {
    const SAMPLE_TARGET: f64 = 65_536.0;
    const MIN_CHROMA: f32 = 0.12;
    const MIN_DOMINANT_SHARE: f32 = 0.01;

    let (width, height) = image.size;
    if width == 0 || height == 0 {
        return None;
    }
    let (width, height) = (width as usize, height as usize);
    let step = ((width * height) as f64 / SAMPLE_TARGET)
        .sqrt()
        .round()
        .max(1.0) as usize;
    let mut weights = [0.0f32; 4096];
    let mut sums = [[0.0f32; 3]; 4096];
    let mut sampled = 0usize;

    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            sampled += 1;
            let index = (y * width + x) * 4;
            let [r, g, b, a] = [
                image.rgba[index],
                image.rgba[index + 1],
                image.rgba[index + 2],
                image.rgba[index + 3],
            ];
            if a < 128 {
                continue;
            }
            let (r, g, b) = (
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            );
            let chroma = r.max(g).max(b) - r.min(g).min(b);
            if chroma < MIN_CHROMA {
                continue;
            }
            let bucket = (bucket(r) << 8) | (bucket(g) << 4) | bucket(b);
            weights[bucket] += chroma;
            sums[bucket][0] += r * chroma;
            sums[bucket][1] += g * chroma;
            sums[bucket][2] += b * chroma;
        }
    }

    let (bucket, weight) = weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))?;
    if *weight < sampled as f32 * MIN_DOMINANT_SHARE {
        return None;
    }
    Some(normalize_lightness([
        sums[bucket][0] / weight,
        sums[bucket][1] / weight,
        sums[bucket][2] / weight,
    ]))
}

/// The 4-bit histogram bucket of a `0.0..=1.0` channel.
fn bucket(channel: f32) -> usize {
    ((channel * 16.0) as usize).min(15)
}

/// Keeps the tint (and the traffic lights derived from it) in a readable
/// lightness band: HSL lightness 45–65 %.
fn normalize_lightness(rgb: [f32; 3]) -> Rgba {
    let (hue, saturation, lightness) = rgb_to_hsl(rgb);
    let [r, g, b] = hsl_to_rgb(hue, saturation, lightness.clamp(0.45, 0.65));
    Rgba { r, g, b, a: 1.0 }
}

fn rgb_to_hsl([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    if delta == 0.0 {
        return (0.0, 0.0, lightness);
    }
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == r {
        ((g - b) / delta + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    (hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    if saturation == 0.0 {
        return [lightness; 3];
    }
    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    [
        hue_to_rgb(p, q, hue + 1.0 / 3.0),
        hue_to_rgb(p, q, hue),
        hue_to_rgb(p, q, hue - 1.0 / 3.0),
    ]
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

/// Writes the dominant colour for the compositor and the portal, or clears it
/// when the wallpaper has no usable colour.
fn write_state(path: &Path, dominant: Option<Rgba>) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match dominant {
        Some(color) => {
            let temporary = path.with_extension("state.tmp");
            std::fs::write(
                &temporary,
                format!(
                    "# Generated by yuki-shell wallpaper.\ndominant = [{:.6}, {:.6}, {:.6}]\n",
                    color.r, color.g, color.b
                ),
            )?;
            std::fs::rename(temporary, path)?;
        }
        None => match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("yuki-wallpaper-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, file: &str) -> PathBuf {
        let path = dir.join(file);
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn without_a_selection_the_default_wallpaper_is_used() {
        let dir = scratch("default");
        touch(&dir, "Black.png");
        let aqua = touch(&dir, yuki_design::WALLPAPER);
        let store = dir.join("wallpaper.domain");
        std::fs::write(&store, format!("dir = {:?}\n", dir.to_str().unwrap())).unwrap();
        let (path, _) = load_wallpaper(&store).unwrap().unwrap();
        assert_eq!(path, aqua);
    }

    #[test]
    fn the_store_selection_wins_over_the_default() {
        let dir = scratch("selection");
        touch(&dir, yuki_design::WALLPAPER);
        let mint = touch(&dir, "Solid Mint.png");
        let store = dir.join("wallpaper.domain");
        std::fs::write(
            &store,
            format!(
                "dir = {:?}\n[desktops]\ndefault = \"file://{}\"\n",
                dir.to_str().unwrap(),
                mint.to_str().unwrap()
            ),
        )
        .unwrap();
        let (path, _) = load_wallpaper(&store).unwrap().unwrap();
        assert_eq!(path, mint);
    }

    #[test]
    fn a_folder_without_the_default_falls_back_to_its_first_image() {
        let dir = scratch("custom");
        let black = touch(&dir, "Black.png");
        touch(&dir, "Yellow.png");
        let store = dir.join("wallpaper.domain");
        std::fs::write(&store, format!("dir = {:?}\n", dir.to_str().unwrap())).unwrap();
        let (path, _) = load_wallpaper(&store).unwrap().unwrap();
        assert_eq!(path, black);
    }

    fn fixture(name: &str) -> DecodedImage {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        decode_image(&path).unwrap()
    }

    fn assert_orange(color: Rgba) {
        assert!(
            color.r > color.g && color.g > color.b,
            "expected orange, got {color:?}"
        );
        assert!(color.r > 0.5, "expected a bright orange, got {color:?}");
    }

    #[test]
    fn a_jpeg_wallpaper_decodes_to_its_dominant_colour() {
        let image = fixture("orange.jpg");
        assert_eq!(image.size, (16, 16));
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn a_sixteen_bit_png_decodes_to_its_dominant_colour() {
        let image = fixture("orange-16.png");
        assert_eq!(image.size, (16, 16));
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn an_indexed_png_decodes_to_its_dominant_colour() {
        let image = fixture("orange-indexed.png");
        assert_eq!(image.size, (16, 16));
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn a_gray_wallpaper_has_no_dominant_colour() {
        let image = DecodedImage {
            size: (16, 16),
            rgba: [128u8, 128, 128, 255].repeat(256),
        };
        assert_eq!(dominant(&image), None);
    }

    #[test]
    fn an_orange_gradient_stays_orange() {
        let mut rgba = Vec::new();
        for y in 0..64 {
            for x in 0..64 {
                let t = (x + y) as f32 / 126.0;
                rgba.extend_from_slice(&[
                    ((0.55 + 0.4 * t) * 255.0) as u8,
                    ((0.25 + 0.3 * t) * 255.0) as u8,
                    ((0.05 + 0.15 * t) * 255.0) as u8,
                    255,
                ]);
            }
        }
        let image = DecodedImage {
            size: (64, 64),
            rgba,
        };
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn a_dark_colour_is_lifted_into_the_lightness_band() {
        let image = DecodedImage {
            size: (16, 16),
            rgba: [77u8, 26, 0, 255].repeat(256),
        };
        let color = dominant(&image).expect("a dominant colour");
        let (_, _, lightness) = rgb_to_hsl([color.r, color.g, color.b]);
        assert!(
            (0.45..=0.65).contains(&lightness),
            "lightness {lightness} is outside the band"
        );
    }

    #[test]
    fn the_state_file_records_and_clears_the_dominant_colour() {
        let dir = scratch("state");
        let path = dir.join("wallpaper.state");
        let color = Rgba {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 1.0,
        };
        write_state(&path, Some(color)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("dominant = [0.200000, 0.400000, 0.600000]"));
        write_state(&path, None).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn the_default_wallpaper_ships_with_yuki() {
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/wallpapers/solid-colors")
            .join(yuki_design::WALLPAPER);
        assert!(shipped.is_file(), "{shipped:?} is missing");
    }
}
