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

use crate::present::EglPresenter;

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
    current: Option<PathBuf>,
    /// The store's mtime at the last check; `None` until the first check, so
    /// a session without a store still loads the default once.
    last_modified: Option<Option<SystemTime>>,
}

/// Connects to the compositor and runs the wallpaper component until the
/// compositor closes it or the connection dies.
pub fn run() -> anyhow::Result<()> {
    let store_path = data_dir().join("wallpaper.domain");

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
                match decode_png(&path) {
                    Ok(image) => {
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

/// The preferences/data directory: `YUKI_DATA_DIR`, else `$XDG_DATA_HOME/yuki`
/// (`~/.local/share/yuki` by default) — the `~/Library/Application Support`
/// equivalent, where the wallpaper store lives.
fn data_dir() -> PathBuf {
    std::env::var_os("YUKI_DATA_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|h| h.join(".local/share"))
                })
                .unwrap_or_else(|| PathBuf::from(".local/share"));
            base.join("yuki")
        },
        PathBuf::from,
    )
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
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
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

/// Decodes a PNG into top-down RGBA8 pixels.
fn decode_png(path: &Path) -> anyhow::Result<DecodedImage> {
    let file = std::fs::File::open(path)?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;

    let width = info.width;
    let height = info.height;
    let bytes_per_pixel = reader.output_buffer_size() / (width * height) as usize;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    match info.color_type {
        png::ColorType::Rgb => {
            for px in buf.chunks(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        png::ColorType::Rgba => rgba = buf,
        png::ColorType::Grayscale => {
            for px in buf.chunks(1) {
                rgba.extend_from_slice(&[px[0], px[0], px[0], 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for px in buf.chunks(2) {
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        _ => anyhow::bail!("unsupported PNG colour type {:?}", info.color_type),
    }
    let _ = bytes_per_pixel;
    Ok(DecodedImage {
        size: (width, height),
        rgba,
    })
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

    #[test]
    fn the_default_wallpaper_ships_with_yuki() {
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/wallpapers/solid-colors")
            .join(yuki_design::WALLPAPER);
        assert!(shipped.is_file(), "{shipped:?} is missing");
    }
}
