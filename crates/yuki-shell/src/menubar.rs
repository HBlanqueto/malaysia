//! The menu bar component: the bar at the top of the screen.
//!
//! A `Top` layer-shell surface with an exclusive zone, like macOS's menu bar.
//! The first version shows the **active app's name** (published by the
//! compositor in the `window.domain` state store) on the left and the
//! **clock** on the right. No per-app global menus yet.
//!
//! Rendering is CPU-side for now: the bar is filled and the text is composited
//! with `celestial-kit::text` (cosmic-text) into an RGBA frame, presented by
//! `wl_shm` — the same path as the wallpaper.

use std::{
    ffi::c_void,
    path::PathBuf,
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
use celestial_kit::text::Text;

/// The bar height, in logical pixels.
const BAR_HEIGHT: i32 = 28;
/// The font size, in logical pixels.
const FONT_SIZE: f32 = 13.0;
/// The bar background, translucent (macOS vibrancy will come with blur).
const BAR_BG: [u8; 4] = [0x1c, 0x1c, 0x1f, 0xe0];
/// The text colour.
const TEXT_COLOR: [u8; 4] = [0xf2, 0xf2, 0xf5, 0xff];
/// The margin from the edges, in logical pixels.
const MARGIN: i32 = 10;

/// Duration until the next minute boundary, for the aligned clock timer.
fn seconds_to_next_minute() -> Duration {
    use chrono::Timelike;
    let seconds = chrono::Local::now().second();
    Duration::from_secs(if seconds == 0 {
        60
    } else {
        (60 - seconds) as u64
    })
}

struct Menubar {
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
    presenter: crate::present::EglPresenter,
    texture: Option<Texture>,
    text: Text,

    scale: f64,
    logical_size: (u32, u32),
    buffer_size: (u32, u32),

    window_store: PathBuf,
    last_window_modified: Option<SystemTime>,
    active_title: String,
    clock_text: String,
    /// The last title/clock uploaded, to know which region changed.
    last_title: String,
    last_clock: String,
    /// The full frame, kept so a redraw uploads only the damaged region.
    frame: Vec<u8>,
    needs_redraw: bool,
}

/// Connects to the compositor and runs the menu bar until closed.
pub fn run() -> anyhow::Result<()> {
    let window_store = data_dir().join("window.domain");

    let conn = Connection::connect_to_env()
        .map_err(|_| anyhow::anyhow!("WAYLAND_DISPLAY is not set to a reachable socket"))?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let mut state = Menubar {
        registry: RegistryState::new(&globals),
        compositor: CompositorState::bind(&globals, &qh)?,
        output: OutputState::new(&globals, &qh),
        layer_shell: LayerShell::bind(&globals, &qh).ok(),
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        fractional: globals.bind(&qh, 1..=1, ()).ok(),
        surface: None,
        layer_surface: None,
        viewport: None,
        presenter: crate::present::EglPresenter::new(),
        text: Text::new(FONT_SIZE),
        renderer: None,
        texture: None,
        scale: 1.0,
        logical_size: (0, 0),
        buffer_size: (0, 0),
        window_store,
        last_window_modified: None,
        active_title: String::new(),
        clock_text: String::new(),
        last_title: String::new(),
        last_clock: String::new(),
        frame: Vec::new(),
        needs_redraw: false,
    };

    if state.layer_shell.is_none() {
        anyhow::bail!("the compositor does not advertise zwlr_layer_shell_v1");
    }

    state.create_surfaces(&qh);
    state.update_clock();

    let mut event_loop: calloop::EventLoop<Menubar> = calloop::EventLoop::try_new()?;
    WaylandSource::new(conn.clone(), event_queue).insert(event_loop.handle())?;
    let clock_conn = conn.clone();
    // The clock shows minutes only (`%H:%M`), so a frame is committed at most
    // once per minute, on a timer aligned to the minute boundary.
    event_loop
        .handle()
        .insert_source(
            calloop::timer::Timer::from_duration(seconds_to_next_minute()),
            move |_, _, state: &mut Menubar| {
                state.update_clock();
                if state.needs_redraw {
                    state.needs_redraw = false;
                    state.render(&clock_conn);
                }
                calloop::timer::TimeoutAction::ToDuration(seconds_to_next_minute())
            },
        )
        .expect("insert the clock timer");
    let store_conn = conn.clone();
    event_loop
        .handle()
        .insert_source(
            calloop::timer::Timer::from_duration(Duration::from_millis(500)),
            move |_, _, state: &mut Menubar| {
                state.check_window_store(&store_conn);
                calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(500))
            },
        )
        .expect("insert the window-store timer");

    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}

impl Menubar {
    /// Creates the layer surface (Top, exclusive zone) and the scale objects.
    fn create_surfaces(&mut self, qh: &QueueHandle<Self>) {
        let surface = self.compositor.create_surface(qh);
        self.surface = Some(surface.clone());

        let layer_shell = self.layer_shell.as_ref().expect("layer-shell present");
        let layer_surface = layer_shell.create_layer_surface(
            qh,
            surface.clone(),
            Layer::Top,
            Some("yuki-menubar"),
            None,
        );
        // Full width, bar height; anchored to the top edge, reserving its
        // height so windows never go under it.
        layer_surface.set_size(0, BAR_HEIGHT as u32);
        layer_surface.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_exclusive_zone(BAR_HEIGHT);
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

        surface.commit();
    }

    /// Applies a configure from the compositor.
    fn configure(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        configure: LayerSurfaceConfigure,
    ) {
        let (width, height) = configure.new_size;
        if width == 0 || height == 0 {
            return;
        }
        self.logical_size = (width, height);

        let scale = self.scale.max(1.0);
        let (bw, bh) = (
            (width as f64 * scale).round().max(1.0) as u32,
            (height as f64 * scale).round().max(1.0) as u32,
        );
        self.buffer_size = (bw, bh);

        if let Some(surface) = &self.surface {
            surface.set_buffer_scale(scale.round().max(1.0) as i32);
        }
        if let Some(viewport) = &self.viewport {
            viewport.set_source(0.0, 0.0, bw as f64, bh as f64);
            viewport.set_destination(width as i32, height as i32);
        }

        self.render(conn);
    }

    /// Rebuilds the frame and presents it by EGL (Bloque 7c).
    ///
    /// Only the damaged region — the clock on the right, the active title on
    /// the left — is uploaded to the texture (`glTexSubImage2D`); when nothing
    /// changed no frame is committed at all.
    fn render(&mut self, conn: &Connection) {
        self.needs_redraw = false;
        let (w, h) = self.buffer_size;
        if w == 0 || h == 0 {
            return;
        }
        let stride = (w * 4) as usize;
        if self.frame.len() != stride * h as usize {
            self.frame = vec![0u8; stride * h as usize];
        }
        let frame = &mut self.frame;

        // The bar background (static; not re-uploaded after the first frame).
        fill_rect(frame, (w, h), (0, 0, w, h), BAR_BG);

        // The active app's name on the left.
        let baseline = (h as i32 - (FONT_SIZE * 1.3).round() as i32) / 2;
        self.text
            .set_text(&self.active_title, FONT_SIZE, Some(240.0));
        let (title_w, _) = self.text.measure();
        self.text.draw(
            frame,
            stride,
            w as i32,
            h as i32,
            (MARGIN, baseline),
            TEXT_COLOR,
        );

        // The clock on the right, right-aligned at the measured text width.
        self.text.set_text(&self.clock_text, FONT_SIZE, None);
        let (cw, _) = self.text.measure();
        let cx = (w as i32 - MARGIN) - cw.round() as i32;
        self.text.draw(
            frame,
            stride,
            w as i32,
            h as i32,
            (cx, baseline),
            TEXT_COLOR,
        );

        // What changed since the last upload: the measured title run on the
        // left and the measured clock run on the right.
        let mut damaged = None;
        if self.active_title != self.last_title {
            damaged = Some(rect_union(
                damaged,
                (MARGIN as u32, 0, title_w.round().max(1.0) as u32 + 4, h),
            ));
            self.last_title = self.active_title.clone();
        }
        if self.clock_text != self.last_clock {
            damaged = Some(rect_union(
                damaged,
                (cx as u32, 0, cw.round().max(1.0) as u32 + 4, h),
            ));
            self.last_clock = self.clock_text.clone();
        }
        let Some(damaged) = damaged else {
            return; // nothing changed since the last commit
        };

        // Create the GLES renderer on the first render and present the frame
        // as a texture over the EGL window (Bloque 7c).
        if self.renderer.is_none() {
            let display_ptr = conn.display().id().as_ptr().cast::<c_void>();
            // Safety: the display handle is the live Wayland connection's.
            self.renderer =
                Some(unsafe { GlesRenderer::new(display_ptr) }.expect("create the GLES renderer"));
        }
        let (Some(renderer), Some(surface)) = (&mut self.renderer, &self.surface) else {
            return;
        };
        // One texture per size; the first upload is the whole frame, later
        // redraws upload only the damaged rectangle.
        let created = match &self.texture {
            Some(texture) if texture.size == (w, h) => false,
            _ => match renderer.create_texture_rgba((w, h), frame) {
                Ok(texture) => {
                    self.texture = Some(texture);
                    true
                }
                Err(_) => return,
            },
        };
        let Some(texture) = &self.texture else {
            return;
        };
        if created {
            if renderer.upload_texture_rgba(texture, frame).is_err() {
                return;
            }
        } else if renderer
            .upload_texture_region_rgba(texture, frame, damaged.0, damaged.1, damaged.2, damaged.3)
            .is_err()
        {
            return;
        }
        if let Err(error) = self.presenter.present(renderer, surface, texture, (w, h)) {
            eprintln!("menubar: present failed: {error}");
        }
    }

    /// Updates the clock text; a changed clock marks the frame dirty.
    fn update_clock(&mut self) {
        let now = chrono::Local::now();
        let text = now.format("%H:%M").to_string();
        if text != self.clock_text {
            self.clock_text = text;
            self.needs_redraw = true;
        }
    }

    /// Re-reads the active window store if it changed.
    fn check_window_store(&mut self, conn: &Connection) {
        let modified = std::fs::metadata(&self.window_store)
            .and_then(|m| m.modified())
            .ok();
        if modified == self.last_window_modified {
            return;
        }
        self.last_window_modified = modified;
        match load_active_window(&self.window_store) {
            Ok(title) if title != self.active_title => {
                self.active_title = title;
                self.render(conn);
            }
            Ok(_) => {}
            Err(error) => eprintln!("menubar: window store: {error}"),
        }
    }
}

impl ProvidesRegistryState for Menubar {
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

impl OutputHandler for Menubar {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl CompositorHandler for Menubar {
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

impl LayerShellHandler for Menubar {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        eprintln!("menubar: layer surface closed by the compositor");
        std::process::exit(0);
    }

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.configure(conn, qh, configure);
    }
}

impl Dispatch<WpViewporter, (), Menubar> for Menubar {
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

impl Dispatch<WpViewport, WlSurface, Menubar> for Menubar {
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

impl Dispatch<WpFractionalScaleManagerV1, (), Menubar> for Menubar {
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

impl Dispatch<WpFractionalScaleV1, WlSurface, Menubar> for Menubar {
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

impl Dispatch<WlBuffer, (), Menubar> for Menubar {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // `wl_buffer.release`: the compositor is done with this buffer.
    }
}

delegate_compositor!(Menubar);
delegate_output!(Menubar);
delegate_registry!(Menubar);

delegate_dispatch!(
    Menubar: [zwlr_layer_shell_v1::ZwlrLayerShellV1: GlobalData] => LayerShell
);
delegate_dispatch!(
    Menubar: [zwlr_layer_surface_v1::ZwlrLayerSurfaceV1: LayerSurfaceData] => LayerShell
);

// --- helpers ---------------------------------------------------------------

/// The state/data directory: `YUKI_DATA_DIR`, else `$XDG_DATA_HOME/yuki`.
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

/// Reads the active window title from the compositor's store.
fn load_active_window(store_path: &PathBuf) -> anyhow::Result<String> {
    #[derive(serde::Deserialize)]
    struct WindowStore {
        #[serde(default)]
        active: Active,
    }
    #[derive(Default, serde::Deserialize)]
    struct Active {
        #[serde(default)]
        title: String,
    }
    let text = std::fs::read_to_string(store_path)?;
    let store: WindowStore = toml::from_str(&text)?;
    Ok(store.active.title)
}

/// Fills a rectangle in a top-down RGBA frame, src-over.
fn fill_rect(frame: &mut [u8], frame_size: (u32, u32), rect: (i32, i32, u32, u32), color: [u8; 4]) {
    let (w, h) = frame_size;
    let (x, y, rw, rh) = rect;
    let stride = (w * 4) as usize;
    for row in y..(y + rh as i32) {
        if row < 0 || row >= h as i32 {
            continue;
        }
        for col in x..(x + rw as i32) {
            if col < 0 || col >= w as i32 {
                continue;
            }
            let pi = row as usize * stride + col as usize * 4;
            let a = color[3] as f32 / 255.0;
            frame[pi] = (color[0] as f32 * a + frame[pi] as f32 * (1.0 - a)) as u8;
            frame[pi + 1] = (color[1] as f32 * a + frame[pi + 1] as f32 * (1.0 - a)) as u8;
            frame[pi + 2] = (color[2] as f32 * a + frame[pi + 2] as f32 * (1.0 - a)) as u8;
            frame[pi + 3] = (a * 255.0 + frame[pi + 3] as f32 * (1.0 - a)) as u8;
        }
    }
}

/// Bounding box of `a` and `b` (a may be `None`).
fn rect_union(a: Option<(u32, u32, u32, u32)>, b: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let Some(a) = a else {
        return b;
    };
    let x1 = a.0.min(b.0);
    let y1 = a.1.min(b.1);
    let x2 = (a.0 + a.2).max(b.0 + b.2);
    let y2 = (a.1 + a.3).max(b.1 + b.3);
    (x1, y1, x2 - x1, y2 - y1)
}
