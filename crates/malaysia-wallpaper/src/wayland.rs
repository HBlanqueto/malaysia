use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::os::fd::AsFd;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use calloop::EventLoop;
use calloop::PostAction;
use calloop::generic::Generic;
use calloop::ping::{Ping, PingSource, make_ping};
use calloop::signals::{Signal, Signals};
use calloop::timer::{TimeoutAction, Timer};
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

use crate::dynamic::Dynamic;
use crate::framing::Framing;
use crate::image::{
    DEFAULT_WALLPAPER_BYTES, DecodedImage, decode_heif, decode_image_bytes, dominant, grid, is_heif,
};
use crate::render::{RenderContext, WallpaperTexture};
use crate::shader::Shader;
use crate::source;
use crate::video::Video;
use ctappearance::tinting::WallpaperGrid;
use ctappearance::watch::{AppearanceEvent, AppearanceWatch};
use ctappearance::{
    ALL_DESKTOPS, WallpaperKind, WallpaperOutput, WallpaperSelection, WallpaperShuffle,
    WallpaperSource, WallpaperState, desktop_key, file_path, file_uri, read_wallpaper_selection,
};
use ctdesign::{BACKDROP, Rgba, WALLPAPER_FPS};
use infinity_protocol::client::style::infinity_window_style_v1::infinity_window_style_v1::InfinityWindowStyleV1;

const NAMESPACE: &str = ctdesign::WALLPAPER_LAYER_NAMESPACE;
const SCALE_DENOMINATOR: f64 = 120.0;
const WINDOW_STYLE_DESKTOPS: u32 = 5;
const MINUTE: Duration = Duration::from_secs(60);

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
    loop_handle: Option<calloop::LoopHandle<'static, App>>,
    render: RenderContext,
    outputs: HashMap<ObjectId, OutputSurface>,
    store_path: PathBuf,
    state_path: PathBuf,
    selection: WallpaperSelection,
    state: WallpaperState,
    shown: BTreeMap<String, Shown>,
    window_style: Option<InfinityWindowStyleV1>,
    wake: Ping,
    wake_source: Option<PingSource>,
    dark: bool,
    reduced_motion: bool,
    appearance_watch: AppearanceWatch,
}

struct Shown {
    file: Option<PathBuf>,
    dynamic: Option<Dynamic>,
    image: usize,
    content: Content,
    grid: Option<WallpaperGrid>,
    dominant: Option<Rgba>,
    paused: bool,
}

struct Clock {
    started: Instant,
    frozen: Option<f32>,
}

impl Clock {
    fn seconds(&self) -> f32 {
        self.frozen
            .unwrap_or_else(|| self.started.elapsed().as_secs_f32())
    }

    fn run(&mut self, running: bool) {
        match (running, self.frozen) {
            (true, Some(at)) => {
                self.started = Instant::now() - Duration::from_secs_f32(at);
                self.frozen = None;
            }
            (false, None) => self.frozen = Some(self.seconds()),
            _ => {}
        }
    }
}

enum Content {
    Image(WallpaperTexture),
    Video(Video, WallpaperTexture),
    Shader(Shader, Clock),
}

impl Shown {
    fn new(
        file: Option<&Path>,
        dynamic: Option<Dynamic>,
        image: usize,
        still: &DecodedImage,
        content: Content,
    ) -> Self {
        Self {
            file: file.map(Path::to_path_buf),
            dynamic,
            image,
            content,
            grid: grid(still),
            dominant: dominant(still),
            paused: false,
        }
    }

    fn moving(&self, reduced_motion: bool) -> bool {
        !reduced_motion
            && !self.paused
            && matches!(self.content, Content::Video(..) | Content::Shader(..))
    }
}

struct OutputSurface {
    wgpu_surface: wgpu::Surface<'static>,
    output: WlOutput,
    desktop: Option<u32>,
    surface: WlSurface,
    layer_surface: LayerSurface,
    viewport: WpViewport,
    fractional_scale: WpFractionalScaleV1,
    logical_size: (u32, u32),
    buffer_size: (u32, u32),
    scale: f64,
    configured: bool,
    surface_format: wgpu::TextureFormat,
    pending_redraw: bool,
    awaiting_frame: bool,
    dirty: bool,
    first_frame_presented: bool,
}

impl Drop for OutputSurface {
    fn drop(&mut self) {
        self.viewport.destroy();
        self.fractional_scale.destroy();
    }
}

impl App {
    pub fn new() -> anyhow::Result<(Self, EventQueue<Self>)> {
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
        let window_style = globals
            .bind::<InfinityWindowStyleV1, _, _>(
                &qh,
                WINDOW_STYLE_DESKTOPS..=WINDOW_STYLE_DESKTOPS,
                (),
            )
            .ok();
        if window_style.is_none() {
            warn!(
                "no infinity_window_style_v1 version 5: every desktop shows the shared wallpaper"
            );
        }

        let appearance_watch = AppearanceWatch::new()?;

        let render = block_on(RenderContext::new())
            .map_err(|e| anyhow::anyhow!("failed to create Vulkan render context: {e}"))?;

        let (wake, wake_source) = make_ping()?;
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
            loop_handle: None,
            render,
            outputs: HashMap::new(),
            store_path: ctappearance::wallpaper_selection_path(),
            state_path: ctappearance::wallpaper_state_path(),
            selection: WallpaperSelection::default(),
            state: ctappearance::load_wallpaper_state(),
            shown: BTreeMap::new(),
            window_style,
            wake,
            wake_source: Some(wake_source),
            dark: false,
            reduced_motion: false,
            appearance_watch,
        };
        app.reload(true);
        app.make_thumbnails();

        event_queue.roundtrip(&mut app)?;

        Ok((app, event_queue))
    }

    pub fn run(&mut self, event_queue: EventQueue<Self>) -> anyhow::Result<()> {
        let mut event_loop: EventLoop<'static, Self> = EventLoop::try_new()?;
        self.loop_signal = Some(event_loop.get_signal());
        let loop_handle = event_loop.handle();
        self.loop_handle = Some(loop_handle.clone());

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
                            if events.contains(&AppearanceEvent::Defaults) {
                                app.make_thumbnails();
                            }
                            if events.iter().any(|event| {
                                matches!(
                                    event,
                                    AppearanceEvent::WallpaperSelection
                                        | AppearanceEvent::Appearance
                                        | AppearanceEvent::Defaults
                                )
                            }) {
                                app.reload(false);
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

        if let Some(wake_source) = self.wake_source.take() {
            loop_handle
                .insert_source(wake_source, |_, _, app: &mut App| app.show_video_frames())
                .map_err(|e| anyhow::anyhow!("failed to add video wake source: {e}"))?;
        }
        loop_handle
            .insert_source(Timer::from_duration(MINUTE), |_, _, app: &mut App| {
                app.reload(false);
                TimeoutAction::ToDuration(MINUTE)
            })
            .map_err(|e| anyhow::anyhow!("failed to add shuffle timer: {e}"))?;
        let frame = Duration::from_secs(1) / WALLPAPER_FPS;
        loop_handle
            .insert_source(Timer::from_duration(frame), move |_, _, app: &mut App| {
                app.animate_shaders();
                TimeoutAction::ToDuration(frame)
            })
            .map_err(|e| anyhow::anyhow!("failed to add animation timer: {e}"))?;

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

    fn reload(&mut self, starting: bool) {
        let previous_selection = self.selection.clone();
        match read_wallpaper_selection(&self.store_path) {
            Ok(selection) => self.selection = selection,
            Err(error) => error!(%error, "cannot read the wallpaper selection"),
        }
        let appearance = ctappearance::load_appearance();
        self.dark = appearance.color_scheme.resolved().is_dark();
        self.reduced_motion = appearance.reduced_motion;
        let now = epoch_seconds();
        let shuffle = (starting && self.selection.shuffle == WallpaperShuffle::Login)
            || source::due(self.selection.shuffle, self.state.shuffled_at, now);
        let own = self
            .window_style
            .as_ref()
            .map(|_| self.selection.own_desktops().map(Some).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut changed = self.selection != previous_selection;
        let mut shown = BTreeMap::new();
        for desktop in std::iter::once(None).chain(own) {
            let key = desktop_key(desktop);
            let previous = self.shown.remove(&key);
            let file = self.file_for(desktop, shuffle, now);
            let (next, loaded) = self.show(desktop, file, previous);
            changed |= loaded;
            shown.insert(key, next);
        }
        changed |= !self.shown.is_empty();
        self.shown = shown;
        self.state
            .shown
            .retain(|key, _| self.shown.contains_key(key));
        if self.state.shown.is_empty() {
            self.state.shuffled_at = None;
        }
        for (key, shown) in &mut self.shown {
            shown.paused = self.selection.paused_for(key.parse().ok());
            let running = !self.reduced_motion && !shown.paused;
            match &mut shown.content {
                Content::Video(video, _) => video.set_playing(running),
                Content::Shader(_, clock) => clock.run(running),
                Content::Image(_) => {}
            }
        }
        if changed {
            self.sync_surfaces();
            self.write_state();
            self.schedule_redraw_all();
        }
    }

    fn file_for(&mut self, desktop: Option<u32>, shuffle: bool, now: u64) -> Option<PathBuf> {
        let key = desktop_key(desktop);
        let source = match self.selection.source(desktop) {
            None | Some(WallpaperSource::BuiltIn) => None,
            Some(WallpaperSource::File(path)) => {
                return Some(path).inspect(|_| {
                    self.state.shown.remove(&key);
                });
            }
            Some(shuffled) => Some(shuffled),
        };
        let Some(source) = source else {
            self.state.shown.remove(&key);
            return None;
        };
        let choices = source::choices(&source);
        let previous = self.state.shown.get(&key).and_then(|uri| file_path(uri));
        if !shuffle && let Some(previous) = previous.as_ref().filter(|file| choices.contains(file))
        {
            return Some(previous.clone());
        }
        let Some(next) = source::pick(&choices, previous.as_deref(), source::seed()) else {
            self.state.shown.remove(&key);
            return None;
        };
        self.state.shown.insert(key, file_uri(&next));
        self.state.shuffled_at = Some(now);
        Some(next)
    }

    fn image_of(&self, dynamic: Option<&Dynamic>, desktop: Option<u32>) -> usize {
        dynamic.map_or(0, |dynamic| {
            dynamic.image(
                self.selection.appearance_for(desktop),
                self.dark,
                day_fraction(),
            )
        })
    }

    fn show(
        &mut self,
        desktop: Option<u32>,
        file: Option<PathBuf>,
        previous: Option<Shown>,
    ) -> (Shown, bool) {
        let unchanged = previous.as_ref().is_some_and(|previous| {
            previous.file == file
                && previous.image == self.image_of(previous.dynamic.as_ref(), desktop)
        });
        if unchanged && let Some(previous) = previous {
            return (previous, false);
        }
        match self.load(desktop, file.as_deref()) {
            Ok(shown) => (shown, true),
            Err(error) => {
                error!(?file, %error, "cannot show the wallpaper");
                match previous {
                    Some(previous) => (previous, false),
                    None => (
                        self.load(desktop, None).unwrap_or_else(|error| {
                            panic!("the embedded wallpaper is corrupt: {error}")
                        }),
                        true,
                    ),
                }
            }
        }
    }

    fn load(&mut self, desktop: Option<u32>, file: Option<&Path>) -> anyhow::Result<Shown> {
        let kind = file.map_or(Some(WallpaperKind::Image), WallpaperKind::of);
        info!(?file, ?kind, "loading wallpaper");
        match (kind, file) {
            (Some(WallpaperKind::Video), Some(file)) => {
                let video = Video::open(file, Some(self.wake.clone()))?;
                let first = video
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("the video gave no frame"))?;
                let texture = self.render.upload_texture(&first, false);
                Ok(Shown::new(
                    Some(file),
                    None,
                    0,
                    &first,
                    Content::Video(video, texture),
                ))
            }
            (Some(WallpaperKind::Shader), Some(file)) => {
                let device = &self.render.device;
                let mut shader = Shader::new(device, &std::fs::read_to_string(file)?)?;
                let still = shader.snapshot(device, &self.render.queue, self.sample_size())?;
                Ok(Shown::new(
                    Some(file),
                    None,
                    0,
                    &still,
                    Content::Shader(
                        shader,
                        Clock {
                            started: Instant::now(),
                            frozen: None,
                        },
                    ),
                ))
            }
            (Some(WallpaperKind::Image), _) => {
                let bytes = match file {
                    Some(file) => Cow::Owned(std::fs::read(file)?),
                    None => Cow::Borrowed(DEFAULT_WALLPAPER_BYTES),
                };
                let dynamic = file
                    .is_none_or(is_heif)
                    .then(|| Dynamic::of(&bytes))
                    .flatten();
                let image = self.image_of(dynamic.as_ref(), desktop);
                let decoded = match dynamic {
                    Some(_) => decode_heif(&bytes, Some(image))?,
                    None => decode_image_bytes(&bytes)?,
                };
                let texture = self.render.upload_texture(&decoded, true);
                Ok(Shown::new(
                    file,
                    dynamic,
                    image,
                    &decoded,
                    Content::Image(texture),
                ))
            }
            _ => anyhow::bail!("not a wallpaper"),
        }
    }

    fn sample_size(&self) -> (u32, u32) {
        let (width, height) = self
            .outputs
            .values()
            .map(|output| output.logical_size)
            .max()
            .unwrap_or((1, 1));
        let samples = BACKDROP.samples;
        if width >= height {
            (samples, (samples * height / width.max(1)).max(1))
        } else {
            ((samples * width / height.max(1)).max(1), samples)
        }
    }

    fn make_thumbnails(&self) {
        crate::thumbnails::spawn(
            source::wallpaper_files(),
            self.render.device.clone(),
            self.render.queue.clone(),
        );
    }

    fn write_state(&mut self) {
        let shared = self.shown.get(ALL_DESKTOPS);
        let mut outputs: Vec<WallpaperOutput> = self
            .output_state
            .outputs()
            .filter_map(|output| {
                let info = self.output_state.info(&output)?;
                let (logical_width, logical_height) = info.logical_size?;
                let mode = info.modes.iter().find(|mode| mode.current)?.dimensions;
                let (width, height) = if (mode.0 > mode.1) == (logical_width > logical_height) {
                    mode
                } else {
                    (mode.1, mode.0)
                };
                Some(WallpaperOutput {
                    name: info.name?,
                    width: width.max(0) as u32,
                    height: height.max(0) as u32,
                })
            })
            .collect();
        outputs.sort_by(|a, b| a.name.cmp(&b.name));
        outputs.dedup_by(|a, b| a.name == b.name);
        let state = WallpaperState {
            dominant: shared
                .and_then(|shown| shown.dominant)
                .map(|color| [color.r, color.g, color.b]),
            grid: shared.and_then(|shown| shown.grid.clone()),
            grids: self
                .shown
                .iter()
                .filter(|(key, _)| key.as_str() != ALL_DESKTOPS)
                .filter_map(|(key, shown)| Some((key.clone(), shown.grid.clone()?)))
                .collect(),
            outputs,
            shown: self.state.shown.clone(),
            shuffled_at: self.state.shuffled_at,
        };
        if state == ctappearance::load_wallpaper_state() {
            self.state = state;
            return;
        }
        if let Err(error) = ctappearance::write_wallpaper_state(&self.state_path, &state) {
            error!(%error, "failed to write the wallpaper state");
        }
        self.state = state;
    }

    fn sync_surfaces(&mut self) {
        let desktops: Vec<Option<u32>> = self.shown.keys().map(|key| key.parse().ok()).collect();
        self.outputs
            .retain(|_, surface| desktops.contains(&surface.desktop));
        let outputs: Vec<WlOutput> = self.output_state.outputs().collect();
        for output in outputs {
            for &desktop in &desktops {
                let exists = self
                    .outputs
                    .values()
                    .any(|surface| surface.output == output && surface.desktop == desktop);
                if !exists && let Err(error) = self.new_output_surface(output.clone(), desktop) {
                    error!(%error, "failed to create wallpaper surface for output");
                }
            }
        }
    }

    fn show_video_frames(&mut self) {
        let mut updated = Vec::new();
        for (key, shown) in &mut self.shown {
            let Content::Video(video, texture) = &mut shown.content else {
                continue;
            };
            let Some(frame) = video.take() else {
                continue;
            };
            if !self.render.update_texture(texture, &frame) {
                *texture = self.render.upload_texture(&frame, false);
            }
            updated.push(key.clone());
        }
        for key in updated {
            self.request_frames(&key);
        }
    }

    fn animate_shaders(&mut self) {
        let reduced_motion = self.reduced_motion;
        let keys: Vec<String> = self
            .shown
            .iter()
            .filter(|(_, shown)| {
                shown.moving(reduced_motion) && matches!(shown.content, Content::Shader(..))
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            self.request_frames(&key);
        }
    }

    fn request_frames(&mut self, key: &str) {
        let ready: Vec<WlSurface> = self
            .outputs
            .values_mut()
            .filter(|output| output.configured && desktop_key(output.desktop) == key)
            .filter_map(|output| {
                if output.awaiting_frame {
                    output.dirty = true;
                    None
                } else {
                    Some(output.surface.clone())
                }
            })
            .collect();
        for surface in ready {
            self.render_or_stop(&surface);
        }
    }

    fn render_or_stop(&mut self, surface: &WlSurface) {
        if let Err(error) = self.render_surface(surface) {
            error!(%error, "render failed");
            if let Some(signal) = self.loop_signal.as_ref() {
                signal.stop();
            }
        }
    }

    fn draw_all_outputs(&mut self) {
        for output in self.outputs.values_mut() {
            output.pending_redraw = false;
        }
        let surfaces: Vec<WlSurface> = self
            .outputs
            .values()
            .filter(|o| o.configured)
            .map(|o| o.surface.clone())
            .collect();
        for surface in surfaces {
            self.render_or_stop(&surface);
        }
    }

    fn schedule_redraw_all(&mut self) {
        let any_new =
            self.outputs
                .values_mut()
                .filter(|o| o.configured)
                .fold(false, |any, output| {
                    if output.pending_redraw {
                        any
                    } else {
                        output.pending_redraw = true;
                        true
                    }
                });
        if any_new && let Some(handle) = self.loop_handle.as_ref() {
            handle.insert_idle(|app| app.draw_all_outputs());
        }
    }

    fn schedule_redraw(&mut self, surface: WlSurface) {
        let surface_id = surface.id();
        let Some(output) = self.outputs.get_mut(&surface_id) else {
            return;
        };
        if output.pending_redraw {
            return;
        }
        output.pending_redraw = true;
        if let Some(handle) = self.loop_handle.as_ref() {
            handle.insert_idle(move |app| {
                if let Some(output) = app.outputs.get_mut(&surface_id) {
                    output.pending_redraw = false;
                }
                app.render_or_stop(&surface);
            });
        }
    }

    fn new_output_surface(&mut self, output: WlOutput, desktop: Option<u32>) -> anyhow::Result<()> {
        let surface = self.compositor.create_surface(&self.qh);
        let layer_surface = self.layer_shell.create_layer_surface(
            &self.qh,
            surface.clone(),
            Layer::Background,
            Some(NAMESPACE),
            Some(&output),
        );
        layer_surface.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(1, 1);
        if let (Some(window_style), Some(desktop)) = (self.window_style.as_ref(), desktop) {
            window_style.set_desktop(&surface, desktop);
        }

        let viewport = self
            .viewporter
            .get_viewport(&surface, &self.qh, surface.clone());
        let fractional_scale =
            self.fractional_manager
                .get_fractional_scale(&surface, &self.qh, surface.clone());

        self.set_opaque_region(&surface, 1, 1);
        layer_surface.commit();

        let display_ptr = self.connection.display().id().as_ptr() as *mut c_void;
        let surface_ptr = surface.id().as_ptr() as *mut c_void;
        let display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(display_ptr).unwrap(),
        ));
        let window_handle =
            RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::new(surface_ptr).unwrap()));
        // SAFETY: `OutputSurface` drops `wgpu_surface` before `surface`, and the connection outlives both.
        let wgpu_surface = unsafe { self.render.create_surface(display_handle, window_handle)? };

        let surface_id = surface.id();
        let output_surface = OutputSurface {
            wgpu_surface,
            output: output.clone(),
            desktop,
            surface,
            layer_surface,
            viewport,
            fractional_scale,
            logical_size: (1, 1),
            buffer_size: (1, 1),
            scale: 1.0,
            configured: false,
            surface_format: wgpu::TextureFormat::Rgba8UnormSrgb,
            pending_redraw: false,
            awaiting_frame: false,
            dirty: false,
            first_frame_presented: false,
        };
        info!(output = ?output.id(), ?desktop, "wallpaper surface added");
        self.outputs.insert(surface_id, output_surface);
        Ok(())
    }

    fn destroy_output_surface(&mut self, output: &WlOutput) {
        self.outputs.retain(|_, surface| surface.output != *output);
        info!("output removed, wallpaper surfaces destroyed");
    }

    fn configure_output(&mut self, surface_id: &ObjectId) {
        let (logical_w, logical_h, scale) = {
            let Some(output) = self.outputs.get(surface_id) else {
                return;
            };
            (output.logical_size.0, output.logical_size.1, output.scale)
        };
        let max = self.render.max_texture_dimension_2d;
        let mut buffer_w = ((f64::from(logical_w) * scale).round() as u32).max(1);
        let mut buffer_h = ((f64::from(logical_h) * scale).round() as u32).max(1);
        if buffer_w > max || buffer_h > max {
            warn!(
                logical_w,
                logical_h,
                scale,
                buffer_w,
                buffer_h,
                max,
                "output buffer exceeds adapter texture limit; scaling down"
            );
            if buffer_w > max {
                buffer_h =
                    (f64::from(buffer_h) * f64::from(max) / f64::from(buffer_w)).round() as u32;
                buffer_w = max;
            }
            if buffer_h > max {
                buffer_w =
                    (f64::from(buffer_w) * f64::from(max) / f64::from(buffer_h)).round() as u32;
                buffer_h = max;
            }
            buffer_w = buffer_w.max(1);
            buffer_h = buffer_h.max(1);
        }

        let surface = {
            let Some(output) = self.outputs.get(surface_id) else {
                return;
            };
            output.surface.clone()
        };
        self.set_opaque_region(&surface, logical_w, logical_h);

        let format = {
            let Some(output) = self.outputs.get(surface_id) else {
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
                    caps.formats
                        .into_iter()
                        .find(|f| f.is_srgb())
                        .unwrap_or(wgpu::TextureFormat::Rgba8UnormSrgb)
                })
        };

        info!(
            output = ?surface_id,
            logical_w, logical_h, scale, buffer_w, buffer_h,
            ?format,
            "output configured"
        );

        let Some(output) = self.outputs.get_mut(surface_id) else {
            return;
        };
        output.surface_format = format;
        output.buffer_size = (buffer_w, buffer_h);
        output.layer_surface.set_size(logical_w, logical_h);
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
            color_space: wgpu::SurfaceColorSpace::Auto,
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
        let Some(output) = self.outputs.get(&id) else {
            return Ok(());
        };
        if !output.configured {
            return Ok(());
        }
        let desktop = output.desktop;
        let name = self
            .output_state
            .info(&output.output)
            .and_then(|info| info.name)
            .unwrap_or_default();
        let framing = Framing {
            scale: self.selection.scale_for(desktop),
            crop: self.selection.crop_for(desktop, &name),
        };
        let key = desktop_key(desktop);
        let presented = match self.draw_surface(&id, &key, framing) {
            Ok(()) => true,
            Err(wgpu::CurrentSurfaceTexture::Outdated) => {
                self.configure_output(&id);
                match self.draw_surface(&id, &key, framing) {
                    Ok(()) => true,
                    Err(error) => {
                        error!(?error, "render failed after reconfigure");
                        false
                    }
                }
            }
            Err(wgpu::CurrentSurfaceTexture::Lost) => anyhow::bail!("wgpu surface lost"),
            Err(error) => anyhow::bail!("wgpu render error: {error:?}"),
        };
        let reduced_motion = self.reduced_motion;
        let animated = self
            .shown
            .get(&key)
            .is_some_and(|shown| shown.moving(reduced_motion));
        let Some(output) = self.outputs.get_mut(&id) else {
            return Ok(());
        };
        if !presented {
            return Ok(());
        }
        if animated {
            output.surface.frame(&self.qh, output.surface.clone());
            output.awaiting_frame = true;
        }
        if !output.first_frame_presented {
            output.first_frame_presented = true;
            info!(?desktop, "first frame presented");
        }
        output.surface.commit();
        Ok(())
    }

    fn draw_surface(
        &mut self,
        id: &ObjectId,
        key: &str,
        framing: Framing,
    ) -> Result<(), wgpu::CurrentSurfaceTexture> {
        let Some(output) = self.outputs.get(id) else {
            return Ok(());
        };
        let Some(shown) = self.shown.get_mut(key) else {
            output.layer_surface.commit();
            return Ok(());
        };
        let format = output.surface_format;
        match &mut shown.content {
            Content::Image(texture) | Content::Video(_, texture) => {
                self.render
                    .set_uniforms(framing, output.logical_size, texture.size);
                self.render.present(&output.wgpu_surface, |render, view| {
                    render.draw(view, format, texture);
                })
            }
            Content::Shader(shader, clock) => {
                let seconds = if self.reduced_motion {
                    0.0
                } else {
                    clock.seconds()
                };
                let size = output.buffer_size;
                self.render.present(&output.wgpu_surface, |render, view| {
                    let drawn =
                        shader.draw(&render.device, &render.queue, view, format, size, seconds);
                    if let Err(error) = drawn {
                        warn!(%error, "the shader wallpaper cannot draw");
                    }
                })
            }
        }
    }
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn day_fraction() -> f64 {
    use chrono::Timelike;
    f64::from(chrono::Local::now().num_seconds_from_midnight()) / 86_400.0
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

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
        self.sync_surfaces();
        self.write_state();
    }

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
        self.write_state();
    }

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        self.destroy_output_surface(&output);
        self.write_state();
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
        let Some(output) = self.outputs.get_mut(&surface.id()) else {
            return;
        };
        output.awaiting_frame = false;
        if std::mem::take(&mut output.dirty) {
            self.render_or_stop(surface);
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
        self.outputs.remove(&layer.wl_surface().id());
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
        let Some(output) = self.outputs.get_mut(&surface_id) else {
            return;
        };
        output.logical_size = (configure.new_size.0.max(1), configure.new_size.1.max(1));
        self.configure_output(&surface_id);

        if self.outputs.get(&surface_id).is_some_and(|o| o.configured) {
            self.schedule_redraw(surface.clone());
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
        let surface_id = surface.id();
        let Some(output) = state.outputs.get_mut(&surface_id) else {
            return;
        };
        if (output.scale - scale).abs() > f64::EPSILON {
            output.scale = scale;
            state.configure_output(&surface_id);
            state.schedule_redraw(surface.clone());
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

sctk::reexports::client::delegate_noop!(App: ignore WlRegion);
sctk::reexports::client::delegate_noop!(App: ignore InfinityWindowStyleV1);

delegate_registry!(App);
delegate_compositor!(App);
delegate_output!(App);

sctk::delegate_layer!(App);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paused_shader_clock_resumes_where_it_stopped() {
        let mut clock = Clock {
            started: Instant::now() - Duration::from_secs(10),
            frozen: None,
        };
        clock.run(false);
        let stopped = clock.seconds();
        assert!((stopped - 10.0).abs() < 0.5);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(clock.seconds(), stopped);
        clock.run(true);
        assert!((clock.seconds() - stopped).abs() < 0.05);
    }
}
