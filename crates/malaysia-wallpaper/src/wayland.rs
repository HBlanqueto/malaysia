//! Wayland layer-shell client: one Background surface per output.

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::os::raw::c_void;
use std::ptr::NonNull;

use calloop::EventLoop;
use calloop::PostAction;
use calloop::generic::Generic;
use calloop::signals::{Signal, Signals};
use calloop_wayland_source::WaylandSource;
use futures_lite::future::block_on;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use sctk::compositor::{CompositorHandler, CompositorState};
use sctk::delegate_compositor;
use sctk::delegate_output;
use sctk::delegate_registry;
use sctk::globals::GlobalData;
use sctk::output::{OutputHandler, OutputState};
use sctk::reexports::client::backend::ObjectId;
use sctk::reexports::client::protocol::{
    wl_output::WlOutput, wl_region::WlRegion, wl_surface::WlSurface,
};
use sctk::reexports::client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, globals::registry_queue_init,
};
use sctk::registry::{ProvidesRegistryState, RegistryState};
use sctk::registry_handlers;
use sctk::shell::WaylandSurface;
use sctk::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use tracing::{error, info, warn};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

use crate::image::{Wallpaper, load_snapshot, reload};
use crate::render::{RenderContext, WallpaperTexture};
use vow_appearance::data_dir;
use vow_appearance::watch::{AppearanceEvent, AppearanceWatch};

const NAMESPACE: &str = "malaysia-wallpaper";
const SCALE_DENOMINATOR: f64 = 120.0;

/// State shared by every Wayland/output callback and the event loop.
pub struct App {
    compositor: CompositorState,
    layer_shell: LayerShell,
    output_state: OutputState,
    registry_state: RegistryState,
    fractional_manager: WpFractionalScaleManagerV1,
    viewporter: WpViewporter,
    qh: QueueHandle<App>,
    connection: Connection,
    loop_signal: Option<calloop::LoopSignal>,
    render: RenderContext,
    /// Current wallpaper texture shared by all outputs.
    texture: Option<WallpaperTexture>,
    /// Per-output surface state.
    outputs: HashMap<ObjectId, OutputSurface>,
    /// Wallpaper domain store path.
    store_path: std::path::PathBuf,
    /// Wallpaper state path (dominant colour).
    state_path: std::path::PathBuf,
    /// Last loaded wallpaper selection.
    wallpaper: Wallpaper,
    /// inotify watcher for `wallpaper.domain`.
    appearance_watch: AppearanceWatch,
}

struct OutputSurface {
    output: WlOutput,
    surface: WlSurface,
    layer_surface: LayerSurface,
    viewport: WpViewport,
    #[allow(dead_code)]
    fractional_scale: Option<WpFractionalScaleV1>,
    /// Logical size from the last configure.
    logical_size: (u32, u32),
    /// Fractional scale value from wp_fractional_scale_v1, or 1.0.
    scale: f64,
    /// The wgpu surface for this output.
    wgpu_surface: wgpu::Surface<'static>,
    /// True once the wgpu surface has been configured.
    configured: bool,
    /// Surface format chosen from the adapter's supported formats.
    surface_format: wgpu::TextureFormat,
}

impl App {
    /// Connects to the compositor, binds globals and creates the render context.
    ///
    /// The returned event queue must be passed to [`App::run`].
    pub fn new() -> anyhow::Result<(Self, EventQueue<Self>)> {
        let store_path = data_dir().join("wallpaper.domain");
        let state_path = data_dir().join("wallpaper.state");
        let wallpaper = load_snapshot(&store_path, &state_path);

        let connection = Connection::connect_to_env()?;
        let (globals, mut event_queue) = registry_queue_init(&connection)?;
        let qh = event_queue.handle();

        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|e| anyhow::anyhow!("wl_compositor not available: {e}"))?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|e| anyhow::anyhow!("zwlr_layer_shell_v1 not available: {e}"))?;
        let output_state = OutputState::new(&globals, &qh);
        let registry_state = RegistryState::new(&globals);

        let fractional_manager = globals
            .bind::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, GlobalData)
            .map_err(|e| anyhow::anyhow!("wp_fractional_scale_manager_v1 not available: {e}"))?;
        let viewporter = globals
            .bind::<WpViewporter, _, _>(&qh, 1..=1, GlobalData)
            .map_err(|e| anyhow::anyhow!("wp_viewporter not available: {e}"))?;

        let appearance_watch = AppearanceWatch::new()?;

        let render = block_on(RenderContext::new())
            .map_err(|e| anyhow::anyhow!("failed to create Vulkan render context: {e}"))?;

        let mut app = Self {
            compositor,
            layer_shell,
            output_state,
            registry_state,
            fractional_manager,
            viewporter,
            qh,
            connection,
            loop_signal: None,
            render,
            texture: None,
            outputs: HashMap::new(),
            store_path,
            state_path,
            wallpaper,
            appearance_watch,
        };

        // Upload the initial wallpaper if there is one.
        if let Some(loaded) = app.wallpaper.image.take() {
            app.texture = Some(app.render.upload_texture(&loaded.decoded));
        }

        // Dispatch the first round of globals so outputs are known.
        event_queue.roundtrip(&mut app)?;

        Ok((app, event_queue))
    }

    /// Runs the event loop until SIGTERM/SIGINT.
    pub fn run(&mut self, event_queue: EventQueue<Self>) -> anyhow::Result<()> {
        let mut event_loop: EventLoop<'static, Self> = EventLoop::try_new()?;
        self.loop_signal = Some(event_loop.get_signal());
        let loop_handle = event_loop.handle();

        let wayland_source = WaylandSource::new(self.connection.clone(), event_queue);
        wayland_source
            .insert(loop_handle.clone())
            .map_err(|e| anyhow::anyhow!("failed to add Wayland source: {e}"))?;

        let appearance_fd = self
            .appearance_watch
            .as_fd()
            .try_clone_to_owned()
            .map_err(|e| anyhow::anyhow!("failed to clone appearance watch fd: {e}"))?;
        loop_handle
            .insert_source(
                Generic::new(appearance_fd, calloop::Interest::READ, calloop::Mode::Level),
                |_, _, app: &mut App| {
                    match app.appearance_watch.drain() {
                        Ok(events) => {
                            if events.contains(&AppearanceEvent::WallpaperSelection) {
                                app.reload_wallpaper();
                            }
                        }
                        Err(error) => {
                            warn!(%error, "failed to drain appearance watch");
                        }
                    }
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| anyhow::anyhow!("failed to add appearance watch source: {e}"))?;

        let signals = Signals::new(&[Signal::SIGTERM, Signal::SIGINT])
            .map_err(|e| anyhow::anyhow!("failed to create signal source: {e}"))?;
        let signal_handle = self.loop_signal.clone();
        loop_handle
            .insert_source(signals, move |signal, _, _app: &mut App| {
                if matches!(signal.signal(), Signal::SIGTERM | Signal::SIGINT)
                    && let Some(handle) = signal_handle.as_ref()
                {
                    handle.stop();
                }
            })
            .map_err(|e| anyhow::anyhow!("failed to add signal source: {e}"))?;

        info!("malaysia-wallpaper running");
        event_loop.run(None, self, |app| {
            if let Err(error) = app.connection.flush() {
                warn!(%error, "failed to flush Wayland connection");
            }
        })?;
        Ok(())
    }

    fn reload_wallpaper(&mut self) {
        let next = reload(&self.wallpaper, &self.store_path, &self.state_path);
        if next.selection == self.wallpaper.selection {
            return;
        }
        self.wallpaper = next;
        if let Some(loaded) = self.wallpaper.image.take() {
            self.texture = Some(self.render.upload_texture(&loaded.decoded));
        }
        self.draw_all_outputs();
    }

    /// Renders and commits every configured output immediately.
    fn draw_all_outputs(&mut self) {
        let surfaces: Vec<WlSurface> = self
            .outputs
            .values()
            .filter(|o| o.configured)
            .map(|o| o.surface.clone())
            .collect();
        for surface in surfaces {
            if let Err(error) = self.render_surface(&surface) {
                error!(%error, "render failed");
                if let Some(signal) = self.loop_signal.as_ref() {
                    signal.stop();
                }
            }
        }
    }

    fn new_output_surface(&mut self, output: WlOutput) -> anyhow::Result<()> {
        let surface = self.compositor.create_surface(&self.qh);
        let layer_surface = self.layer_shell.create_layer_surface(
            &self.qh,
            surface.clone(),
            Layer::Background,
            Some(NAMESPACE),
            Some(&output),
        );
        layer_surface.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_exclusive_zone(0);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(1, 1);

        let viewport = self
            .viewporter
            .get_viewport(&surface, &self.qh, surface.clone());
        let fractional_scale = Some(self.fractional_manager.get_fractional_scale(
            &surface,
            &self.qh,
            surface.clone(),
        ));

        // Initial opaque region and commit without a buffer.
        self.set_opaque_region(&surface, 1, 1);
        layer_surface.commit();

        let display_ptr = self.connection.display().id().as_ptr() as *mut c_void;
        let surface_ptr = surface.id().as_ptr() as *mut c_void;
        let display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(display_ptr).unwrap(),
        ));
        let window_handle =
            RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::new(surface_ptr).unwrap()));
        let wgpu_surface = unsafe { self.render.create_surface(display_handle, window_handle)? };

        let output_surface = OutputSurface {
            output: output.clone(),
            surface,
            layer_surface,
            viewport,
            fractional_scale,
            logical_size: (1, 1),
            scale: 1.0,
            wgpu_surface,
            configured: false,
            // Real format is selected in configure_output from surface capabilities.
            surface_format: wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        info!(output = ?output.id(), "output added");
        self.outputs.insert(output.id(), output_surface);
        Ok(())
    }

    fn destroy_output_surface(&mut self, output: &WlOutput) {
        if let Some(output) = self.outputs.remove(&output.id()) {
            output.viewport.destroy();
            output.surface.destroy();
            info!("output removed, wallpaper surface destroyed");
        }
    }

    fn configure_output(&mut self, output_id: &ObjectId) {
        let (logical_w, logical_h, scale) = {
            let Some(output) = self.outputs.get(output_id) else {
                return;
            };
            (output.logical_size.0, output.logical_size.1, output.scale)
        };
        let buffer_w = ((f64::from(logical_w) * scale).round() as u32).max(1);
        let buffer_h = ((f64::from(logical_h) * scale).round() as u32).max(1);

        let surface = {
            let Some(output) = self.outputs.get(output_id) else {
                return;
            };
            output.surface.clone()
        };
        self.set_opaque_region(&surface, logical_w, logical_h);

        let format = {
            let Some(output) = self.outputs.get(output_id) else {
                return;
            };
            let caps = output.wgpu_surface.get_capabilities(&self.render.adapter);
            let preferred = [
                wgpu::TextureFormat::Bgra8UnormSrgb,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ];
            preferred
                .into_iter()
                .find(|f| caps.formats.contains(f))
                .unwrap_or_else(|| {
                    // Fall back to the first sRGB-capable format the adapter offers.
                    caps.formats
                        .into_iter()
                        .find(|f| f.is_srgb())
                        .unwrap_or(wgpu::TextureFormat::Rgba8UnormSrgb)
                })
        };

        info!(
            output = ?output_id,
            logical_w, logical_h, scale, buffer_w, buffer_h,
            ?format,
            "output configured"
        );

        let Some(output) = self.outputs.get_mut(output_id) else {
            return;
        };
        output.surface_format = format;
        output
            .viewport
            .set_destination(logical_w as i32, logical_h as i32);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: buffer_w,
            height: buffer_h,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        output.wgpu_surface.configure(&self.render.device, &config);
        output.configured = true;
    }

    fn set_opaque_region(&self, surface: &WlSurface, width: u32, height: u32) {
        let region = self.compositor.wl_compositor().create_region(&self.qh, ());
        region.add(0, 0, width as i32, height as i32);
        surface.set_opaque_region(Some(&region));
        region.destroy();
    }

    fn render_surface(&mut self, surface: &WlSurface) -> anyhow::Result<()> {
        let id = surface.id();
        let output_id = {
            let Some(output) = self.outputs.values().find(|o| o.surface.id() == id) else {
                return Ok(());
            };
            if !output.configured {
                return Ok(());
            }
            output.output.id()
        };

        let Some(texture) = self.texture.as_ref() else {
            let output = self.outputs.get(&output_id).expect("output exists");
            output.layer_surface.commit();
            return Ok(());
        };

        let (format, render_result) = {
            let output = self.outputs.get(&output_id).expect("output exists");
            self.render
                .set_uniforms(output.logical_size.0, output.logical_size.1, texture.size);
            let format = output.surface_format;
            (
                format,
                self.render.render(&output.wgpu_surface, format, texture),
            )
        };

        match render_result {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Outdated) => {
                self.configure_output(&output_id);
                let output = self.outputs.get(&output_id).expect("output exists");
                let texture = self.texture.as_ref().expect("texture exists");
                if let Err(error) = self.render.render(&output.wgpu_surface, format, texture) {
                    error!(%error, "render failed after reconfigure");
                }
            }
            Err(wgpu::SurfaceError::Lost) => {
                anyhow::bail!("wgpu surface lost");
            }
            Err(error) => {
                anyhow::bail!("wgpu render error: {error}");
            }
        }
        let output = self.outputs.get(&output_id).expect("output exists");
        output.layer_surface.commit();
        Ok(())
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers!(OutputState);
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        if let Err(error) = self.new_output_surface(output) {
            error!(%error, "failed to create wallpaper surface for output");
        }
    }

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        self.destroy_output_surface(&output);
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_factor: i32,
    ) {
        // Fractional scale is authoritative; wl_output integer scale is ignored.
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_transform: sctk::reexports::client::protocol::wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        _time: u32,
    ) {
        if let Err(error) = self.render_surface(surface) {
            error!(%error, "render failed");
            if let Some(signal) = self.loop_signal.as_ref() {
                signal.stop();
            }
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let id = layer.wl_surface().id();
        self.outputs.retain(|_, o| o.surface.id() != id);
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let surface = layer.wl_surface();
        let surface_id = surface.id();
        let output_id = {
            let Some(output) = self
                .outputs
                .values_mut()
                .find(|o| o.surface.id() == surface_id)
            else {
                return;
            };
            let width = configure.new_size.0.max(1);
            let height = configure.new_size.1.max(1);
            output.logical_size = (width, height);
            output.output.id()
        };
        self.configure_output(&output_id);

        if self.outputs.get(&output_id).is_some_and(|o| o.configured)
            && let Err(error) = self.render_surface(surface)
        {
            error!(%error, "render failed after configure");
            if let Some(signal) = self.loop_signal.as_ref() {
                signal.stop();
            }
        }
    }
}

impl Dispatch<WpFractionalScaleManagerV1, GlobalData> for App {
    fn event(
        _state: &mut Self,
        _proxy: &WpFractionalScaleManagerV1,
        _event: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpViewporter, GlobalData> for App {
    fn event(
        _state: &mut Self,
        _proxy: &WpViewporter,
        _event: <WpViewporter as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, WlSurface> for App {
    fn event(
        state: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        surface: &WlSurface,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        let scale = f64::from(scale) / SCALE_DENOMINATOR;
        let id = surface.id();
        let Some(output) = state.outputs.get_mut(&id) else {
            return;
        };
        if (output.scale - scale).abs() > f64::EPSILON {
            output.scale = scale;
            let output_id = output.output.id();
            state.configure_output(&output_id);
            state.draw_all_outputs();
        }
    }
}

impl Dispatch<WpViewport, WlSurface> for App {
    fn event(
        _state: &mut Self,
        _proxy: &WpViewport,
        _event: <WpViewport as Proxy>::Event,
        _data: &WlSurface,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlRegion, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegion,
        _event: <WlRegion as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

delegate_registry!(App);
delegate_compositor!(App);
delegate_output!(App);

// `delegate_layer!` generates Dispatch for zwlr_layer_shell_v1 and
// zwlr_layer_surface_v1.
sctk::delegate_layer!(App);
