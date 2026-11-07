use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::{Format, Fourcc, Modifier};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{
    DrmDevice, DrmDeviceFd, DrmEventMetadata, DrmEventTime, DrmNode, DrmSurface, NodeType, Planes,
};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::element::RenderElementStates;
use smithay::backend::renderer::element::utils::select_dmabuf_feedback;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::multigpu::GpuManager;

use crate::renderer::GpuApi;
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session as SessionTrait};
use smithay::backend::udev::{all_gpus, primary_gpu};
use smithay::output::{Mode as WlMode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::RegistrationToken;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::signals::{Signal, Signals};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction};
use smithay::reexports::drm::control::connector::{self, Interface};
use smithay::reexports::drm::control::{Device as ControlDevice, ModeTypeFlags, crtc};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::desktop::utils::{
    OutputPresentationFeedback, send_dmabuf_feedback_surface_tree, surface_primary_scanout_output,
};
use smithay::reexports::wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::TrancheFlags;
use smithay::reexports::wayland_server::Display;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{DeviceFd, Transform};
use smithay::wayland::dmabuf::{DmabufFeedback, DmabufFeedbackBuilder};
use smithay::wayland::drm_syncobj::supports_syncobj_eventfd;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::wayland::presentation::Refresh;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{error, info, warn};

use crate::cursor::Cursors;
use crate::state::{ClientState, State};
use ctappearance::watch::AppearanceWatch;

pub(crate) const fn render_transform() -> Transform {
    Transform::Flipped180
}

type Compositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

struct Udev {
    loop_handle: LoopHandle<'static, Udev>,
    display: Display<State>,
    state: State,
    session: LibSeatSession,
    drm: DrmDevice,
    compositor: Compositor,
    gpu: GpuManager<GpuApi>,
    render_node: DrmNode,
    target_node: DrmNode,
    cursor: Cursors,
    output: Output,
    active: bool,
    redraw_state: RedrawState,
    presented_feedback: Option<OutputPresentationFeedback>,
    last_vblank: Option<Duration>,
    loop_signal: LoopSignal,
    capture_requested: bool,
    vblank_count: u64,
    frames_queued: u64,
    frame_was_empty: bool,
    appearance_watch: Option<AppearanceWatch>,
    render_feedback: DmabufFeedback,
    scanout_feedback: DmabufFeedback,
    frame_flags: FrameFlags,
}

#[derive(Debug, Clone, Default)]
pub struct DrmOptions {
    pub device: Option<PathBuf>,
    pub no_input: bool,
}

pub fn run(options: DrmOptions) -> Result<()> {
    let mut event_loop = EventLoop::<Udev>::try_new().context("creating the event loop")?;
    let loop_signal = event_loop.get_signal();
    let display = Display::<State>::new().context("creating the Wayland display")?;

    let (mut session, session_notifier) =
        LibSeatSession::new().map_err(|error| anyhow!("opening the libseat session: {error}"))?;
    let seat = session.seat();

    let scanout_node = match options.device.as_deref() {
        Some(path) => {
            let node = DrmNode::from_path(path)
                .map_err(|error| anyhow!("{} is not a DRM node: {error}", path.display()))?;
            if node.ty() != NodeType::Primary {
                bail!(
                    "--drm-device must name a KMS primary node (cardN), not {}",
                    path.display()
                );
            }
            node
        }
        None => {
            let path = primary_gpu(&seat)
                .map_err(|error| anyhow!("querying the primary GPU: {error}"))?
                .or_else(|| {
                    all_gpus(&seat)
                        .ok()
                        .and_then(|gpus| gpus.into_iter().next())
                })
                .ok_or_else(|| anyhow!("the seat has no DRM device"))?;
            let node = DrmNode::from_path(&path)
                .map_err(|error| anyhow!("{} is not a DRM node: {error}", path.display()))?;
            match node.node_with_type(NodeType::Primary) {
                Some(Ok(primary)) => primary,
                _ => node,
            }
        }
    };
    let scanout_path = scanout_node
        .dev_path()
        .or_else(|| options.device.clone())
        .ok_or_else(|| anyhow!("the DRM device has no dev path"))?;

    let render_node = if options.device.is_some() {
        find_render_node(&seat, &scanout_node)?
    } else {
        scanout_node
    };
    let render_path = if render_node == scanout_node {
        scanout_path.clone()
    } else {
        render_node
            .dev_path()
            .ok_or_else(|| anyhow!("the render node has no dev path"))?
    };
    info!(
        node = ?scanout_node,
        path = %scanout_path.display(),
        "DRM scanout device"
    );
    info!(
        node = ?render_node,
        path = %render_path.display(),
        "DRM render device"
    );

    let scanout_fd = open_drm_device(&mut session, &scanout_path)?;
    let render_fd = if render_node == scanout_node {
        scanout_fd.clone()
    } else {
        open_drm_device(&mut session, &render_path)?
    };

    let (mut drm, drm_notifier) = DrmDevice::new(scanout_fd.clone(), true)
        .map_err(|error| anyhow!("opening DRM: {error}"))?;
    let render_gbm = GbmDevice::new(render_fd.clone()).context("creating the render GBM device")?;
    let scanout_gbm =
        GbmDevice::new(scanout_fd.clone()).context("creating the scanout GBM device")?;

    let mut api = GpuApi::default();
    api.add_node(render_node, render_gbm.clone())
        .map_err(|error| anyhow!("creating EGL on the render node: {error}"))?;
    let scanout_render_node = scanout_node
        .node_with_type(NodeType::Render)
        .and_then(Result::ok)
        .filter(|node| render_node != scanout_node && *node != render_node)
        .filter(|node| api.add_node(*node, scanout_gbm.clone()).is_ok());
    let target_node = scanout_render_node.unwrap_or(render_node);
    info!(node = ?target_node, "DRM composition target");
    let mut gpu =
        GpuManager::new(api).map_err(|error| anyhow!("creating the GPU manager: {error}"))?;
    let render_formats: Vec<_> = gpu
        .single_renderer(&target_node)
        .map_err(|error| anyhow!("creating GLES on the composition target: {error}"))?
        .as_mut()
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect();
    let mut multi = gpu
        .single_renderer(&render_node)
        .map_err(|error| anyhow!("creating GLES: {error}"))?;
    let renderer = multi.as_mut();
    let (connector, crtc, mode) = find_panel(&drm)?;
    info!(
        connector = %format!("{}-{}", connector.interface().as_str(), connector.interface_id()),
        mode = %format!("{}x{}@{}", mode.size().0, mode.size().1, mode.vrefresh()),
        crtc = ?crtc,
        "mode chosen"
    );
    let output = make_output(&connector, mode)?;
    let surface = drm
        .create_surface(crtc, mode, &[connector.handle()])
        .map_err(|error| anyhow!("creating the DRM surface: {error}"))?;

    let overlay_planes = crate::config::Config::load().debug.overlay_planes;
    let mut planes = surface.planes().clone();
    if !overlay_planes {
        planes.overlay.clear();
    }
    let scanout_feedback =
        scanout_feedback(&surface, &planes, renderer, render_node, scanout_node)?;

    let allocator_gbm = if target_node == render_node {
        render_gbm.clone()
    } else {
        scanout_gbm.clone()
    };
    let allocator = GbmAllocator::new(
        allocator_gbm,
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(scanout_gbm.clone(), scanout_node.into());
    let compositor = Compositor::new(
        &output,
        surface,
        Some(planes),
        allocator,
        exporter,
        [Fourcc::Xrgb8888, Fourcc::Argb8888],
        render_formats,
        drm.cursor_size(),
        Some(scanout_gbm),
    )
    .map_err(|error| anyhow!("creating the DRM compositor: {error}"))?;

    let feedback = DmabufFeedbackBuilder::new(
        feedback_node(render_node).dev_id(),
        renderer.dmabuf_formats(),
    )
    .build()
    .map_err(|error| anyhow!("building the dmabuf feedback: {error}"))?;
    let mut frame_flags =
        FrameFlags::ALLOW_PRIMARY_PLANE_SCANOUT_ANY | FrameFlags::ALLOW_CURSOR_PLANE_SCANOUT;
    if overlay_planes {
        frame_flags.insert(FrameFlags::ALLOW_OVERLAY_PLANE_SCANOUT);
    }
    let syncobj = supports_syncobj_eventfd(&render_fd).then_some(render_fd);
    let loop_handle = event_loop.handle();
    let mut state = State::new_with_output(
        loop_signal.clone(),
        &display.handle(),
        smithay::utils::Size::from((i32::from(mode.size().0), i32::from(mode.size().1))),
        output.clone(),
        feedback.clone(),
        syncobj,
        Box::new(loop_handle.clone()),
        Box::new(crate::idle::IdleTimers::new(loop_handle.clone())),
    )
    .map_err(|error| anyhow!("building the compositor state: {error}"))?;
    if state.idle.locks_automatically() {
        crate::sleep::watch(&loop_handle);
    }

    state.rounded_clip = crate::effects::compile_rounded_clip(renderer);
    state.shadow = crate::effects::compile_shadow(renderer);
    state.border = crate::effects::compile_border(renderer);
    drop(multi);

    let cursor = Cursors::load(state.scale.fractional_scale().ceil() as i32);

    let disconnect_queue = crate::state::ClientDisconnectQueue::default();
    let (disconnect_ping, disconnect_source) = smithay::reexports::calloop::ping::make_ping()
        .map_err(|error| anyhow!("creating the disconnect ping: {error}"))?;
    loop_handle
        .insert_source(disconnect_source, {
            let queue = disconnect_queue.clone();
            move |_, _, data: &mut Udev| {
                for client_id in queue.drain() {
                    data.state.client_disconnected(client_id);
                }
            }
        })
        .map_err(|error| anyhow!("inserting the disconnect source: {error}"))?;
    crate::session::watch_overdue_frame_callbacks(&loop_handle);

    let mut udev = Udev {
        loop_handle: loop_handle.clone(),
        display,
        state,
        session: session.clone(),
        drm,
        compositor,
        gpu,
        render_node,
        target_node,
        cursor,
        output,
        active: true,
        redraw_state: RedrawState::Idle,
        presented_feedback: None,
        last_vblank: None,
        loop_signal: loop_signal.clone(),
        capture_requested: false,
        vblank_count: 0,
        frames_queued: 0,
        frame_was_empty: false,
        appearance_watch: None,
        render_feedback: feedback,
        scanout_feedback,
        frame_flags,
    };

    let appearance_watch = AppearanceWatch::new()
        .map_err(|error| anyhow!("creating the appearance watcher: {error}"))?;
    let appearance_fd = appearance_watch
        .as_fd()
        .try_clone_to_owned()
        .map_err(|error| anyhow!("duplicating the appearance watcher fd: {error}"))?;
    udev.appearance_watch = Some(appearance_watch);
    loop_handle
        .insert_source(
            Generic::new(appearance_fd, Interest::READ, Mode::Level),
            |_, _, data: &mut Udev| {
                if let Some(watch) = data.appearance_watch.as_ref()
                    && let Ok(events) = watch.drain()
                    && !events.is_empty()
                {
                    let now = Instant::now();
                    let output_size = data.state.output_size;
                    let scale = data.state.scale.fractional_scale();
                    let restyled = events.iter().any(|event| event.restyles());
                    if restyled {
                        let Ok(mut renderer) = data.gpu.single_renderer(&data.render_node) else {
                            return Ok(PostAction::Continue);
                        };
                        data.state.theme_fade = crate::theme_fade::start_if_allowed(
                            &mut data.state,
                            renderer.as_mut(),
                            &data.cursor,
                            output_size,
                            scale,
                            now,
                        );
                    }
                    data.state.refresh_theme(restyled);
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| anyhow!("inserting the theme watcher: {error}"))?;

    loop_handle
        .insert_source(
            drm_notifier,
            move |event, metadata, data: &mut Udev| match event {
                smithay::backend::drm::DrmEvent::VBlank(_crtc) => data.on_vblank(metadata.as_ref()),
                smithay::backend::drm::DrmEvent::Error(error) => {
                    error!(%error, "DRM event error");
                    data.on_drm_error();
                }
            },
        )
        .map_err(|error| anyhow!("inserting the DRM source: {error}"))?;

    let mut libinput = if options.no_input {
        info!("input disabled by --no-input");
        None
    } else {
        let mut libinput = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
            session.clone().into(),
        );
        libinput
            .udev_assign_seat(&seat)
            .map_err(|_| anyhow!("libinput could not take seat {seat}"))?;
        Some(libinput)
    };
    if let Some(libinput) = &libinput {
        let libinput_backend = LibinputInputBackend::new(libinput.clone());
        loop_handle
            .insert_source(libinput_backend, |event, _, data: &mut Udev| {
                data.state.process_input_event(event);
                data.after_input();
            })
            .map_err(|error| anyhow!("inserting the libinput source: {error}"))?;
    }

    loop_handle
        .insert_source(
            session_notifier,
            move |event, _, data: &mut Udev| match event {
                SessionEvent::PauseSession => {
                    info!("session paused; releasing DRM and libinput");
                    if let Some(libinput) = libinput.as_mut() {
                        libinput.suspend();
                    }
                    data.drm.pause();
                    data.active = false;
                }
                SessionEvent::ActivateSession => {
                    info!("session resumed");
                    if let Some(libinput) = libinput.as_mut()
                        && libinput.resume().is_err()
                    {
                        error!("resuming libinput");
                    }
                    if let Err(error) = data.drm.activate(false) {
                        error!(%error, "activating DRM");
                        return;
                    }
                    data.compositor.reset_buffers();
                    data.active = true;
                    if let RedrawState::WaitingForEstimatedVBlank(token)
                    | RedrawState::WaitingForEstimatedVBlankAndQueued(token) =
                        std::mem::take(&mut data.redraw_state)
                    {
                        data.loop_handle.remove(token);
                    }
                    data.queue_redraw();
                }
            },
        )
        .map_err(|error| anyhow!("inserting the session source: {error}"))?;

    let socket = ListeningSocketSource::new_auto().context("creating the Wayland socket")?;
    let socket_name = socket.socket_name().to_string_lossy().into_owned();
    info!(socket = %socket_name, "listening on the Wayland socket");
    udev.state.set_wayland_display(socket_name.clone());
    let _xwayland = crate::xwayland::start(&socket_name);
    loop_handle
        .insert_source(socket, {
            let queue = disconnect_queue.clone();
            let ping = disconnect_ping.clone();
            move |stream, _, data: &mut Udev| {
                let trusted_shell = crate::infinity::is_trusted_client(&stream);
                let mut handle = data.display.handle();
                match handle.insert_client(
                    stream,
                    Arc::new(ClientState::new(queue.clone(), ping.clone(), trusted_shell)),
                ) {
                    Ok(_) => info!("client connected"),
                    Err(error) => warn!(%error, "rejected a client"),
                }
            }
        })
        .map_err(|error| anyhow!("inserting the socket source: {error}"))?;

    let shell_started = if options.device.is_some() || options.no_input {
        info!("shell target skipped for the development DRM options");
        false
    } else {
        // window.domain outlives the session: the menubar must not start on the last session's app.
        udev.state.publish_active(None);
        start_shell_session(&socket_name)
    };

    let signals = Signals::new(&[Signal::SIGUSR1, Signal::SIGTERM, Signal::SIGINT])
        .map_err(|error| anyhow!("installing the signal source: {error}"))?;
    loop_handle
        .insert_source(signals, |signal, _, data: &mut Udev| {
            match signal.signal() {
                Signal::SIGUSR1 => {
                    info!("SIGUSR1: debug capture requested");
                    data.capture_requested = true;
                    data.queue_redraw();
                }
                Signal::SIGTERM | Signal::SIGINT => {
                    info!("shutdown signal received");
                    data.loop_signal.stop();
                }
                _ => {}
            }
        })
        .map_err(|error| anyhow!("inserting the signal source: {error}"))?;

    let display_fd = udev
        .display
        .as_fd()
        .try_clone_to_owned()
        .context("duplicating the display fd")?;
    loop_handle
        .insert_source(
            Generic::new(display_fd, Interest::READ, Mode::Level),
            |_, _, data: &mut Udev| {
                data.dispatch_clients();
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| anyhow!("inserting the display source: {error}"))?;

    udev.queue_redraw();
    udev.redraw();
    info!("direct session ready");
    let result = event_loop.run(None, &mut udev, |udev| {
        udev.state.activity_noted = false;
        let listeners = std::mem::take(&mut udev.state.security_listeners);
        crate::security::listen(
            &udev.loop_handle,
            listeners,
            &disconnect_queue,
            &disconnect_ping,
        );
        udev.redraw_queued_and_flush();
    });
    if shell_started {
        stop_shell_session();
    }
    result.context("running the event loop")?;
    Ok(())
}

fn start_shell_session(socket: &str) -> bool {
    if let Some(config_home) = ctappearance::xdg::config_home()
        && remove_stale_unit_links(&config_home.join("systemd/user"), Path::new(SYSTEM_UNITS))
    {
        match systemctl(&["--user", "daemon-reload"], None) {
            Ok(output) if output.status.success() => {}
            Ok(output) => warn!(
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "could not reload the user manager"
            ),
            Err(error) => warn!(%error, "could not reload the user manager"),
        }
    }

    let import = systemctl(
        &[
            "--user",
            "import-environment",
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_ID",
            "MALAYSIA_BIN_DIR",
        ],
        Some(socket),
    );
    if let Ok(output) = &import
        && !output.status.success()
    {
        error!(
            stderr = %String::from_utf8_lossy(&output.stderr).trim(),
            "could not import WAYLAND_DISPLAY into the user manager"
        );
    }

    match systemctl(
        &["--user", "start", "malaysia-session.target"],
        Some(socket),
    ) {
        Ok(output) if output.status.success() => {
            info!("malaysia-session.target started");
            true
        }
        Ok(output) => {
            error!(
                status = %output.status,
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "{}",
                missing_target_help()
            );
            false
        }
        Err(error) => {
            error!(%error, "{}", missing_target_help());
            false
        }
    }
}

const SYSTEM_UNITS: &str = "/etc/systemd/user";

// ~/.config/systemd/user outranks /etc/systemd/user: a link there to another package pins an old shell.
fn remove_stale_unit_links(user_units: &Path, system_units: &Path) -> bool {
    if !system_units.join("malaysia-session.target").exists() {
        return false;
    }
    let mut removed = false;
    for dir in [
        user_units.to_path_buf(),
        user_units.join("malaysia-session.target.wants"),
    ] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if !is_stale_unit_link(&path, system_units) {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    info!(path = %path.display(), "removed a stale Malaysia unit link");
                    removed = true;
                }
                Err(error) => {
                    warn!(%error, path = %path.display(), "could not remove a stale unit link")
                }
            }
        }
    }
    removed
}

fn is_stale_unit_link(path: &Path, system_units: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let ours = name.starts_with("malaysia-") || name == "xdg-desktop-portal-malaysia.service";
    let link = path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink());
    if !ours || !link {
        return false;
    }
    let Ok(target) = std::fs::canonicalize(path) else {
        return true;
    };
    let system_target = std::fs::canonicalize(system_units.join(unit_template(name))).ok();
    target.starts_with("/nix/store") && Some(target) != system_target
}

fn unit_template(name: &str) -> String {
    match (name.split_once('@'), name.rsplit_once('.')) {
        (Some((prefix, _)), Some((_, kind))) => format!("{prefix}@.{kind}"),
        _ => name.to_string(),
    }
}

fn stop_shell_session() {
    let _ = systemctl(&["--user", "stop", "malaysia-session.target"], None);
}

fn systemctl(args: &[&str], socket: Option<&str>) -> std::io::Result<std::process::Output> {
    let mut command = crate::spawn::command("systemctl", &crate::config::Config::default());
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(socket) = socket {
        command.env("WAYLAND_DISPLAY", socket);
        command.env("XDG_CURRENT_DESKTOP", "Malaysia");
        // The shell units start from MALAYSIA_BIN_DIR: Infinity trusts only the shell next to this binary.
        if let Some(dir) = crate::spawn::bin_dir() {
            command.env("MALAYSIA_BIN_DIR", dir);
        }
    }
    crate::spawn::spawn(command, &crate::config::Config::default())?.wait_with_output()
}

fn missing_target_help() -> &'static str {
    "malaysia-session.target is not installed in the user manager; the session runs without a shell. \
     Install it with the NixOS module (programs.malaysia.enable)."
}

impl Udev {
    fn dispatch_clients(&mut self) {
        if let Err(error) = self.display.dispatch_clients(&mut self.state) {
            error!(%error, "dispatching clients");
            return;
        }
        match self.gpu.single_renderer(&self.render_node) {
            Ok(mut renderer) => {
                crate::session::import_pending_dmabufs(&mut self.state, renderer.as_mut())
            }
            Err(error) => warn!(%error, "no renderer to import dmabufs"),
        }
        if let Err(error) = self.display.flush_clients() {
            warn!(%error, "flushing clients");
        }
    }

    fn after_input(&mut self) {
        if let Some(vt) = self.state.take_vt_switch()
            && let Err(error) = self.session.change_vt(i32::from(vt))
        {
            warn!(%error, "switching to VT {vt}");
        }
    }

    // niri a8562a1 src/niri.rs:819: every source ends here, so nothing waits for the next input.
    fn redraw_queued_and_flush(&mut self) {
        if self.state.take_redraw_request() {
            self.queue_redraw();
        }
        if matches!(
            self.redraw_state,
            RedrawState::Queued | RedrawState::WaitingForEstimatedVBlankAndQueued(_)
        ) {
            self.redraw();
        }
        if let Err(error) = self.display.flush_clients() {
            warn!(%error, "flushing clients");
        }
    }

    fn queue_redraw(&mut self) {
        self.redraw_state = std::mem::take(&mut self.redraw_state).queued();
    }

    fn redraw(&mut self) {
        let result = if self.active {
            self.render()
        } else {
            RenderResult::Skipped
        };
        match result {
            RenderResult::Submitted => {
                if let RedrawState::WaitingForEstimatedVBlankAndQueued(token) = std::mem::replace(
                    &mut self.redraw_state,
                    RedrawState::WaitingForVBlank {
                        redraw_needed: false,
                    },
                ) {
                    self.loop_handle.remove(token);
                }
                // niri a8562a1 src/backend/tty.rs:2014: client buffers are latched once queued.
                self.state.frame_callback_cycle = self.state.frame_callback_cycle.wrapping_add(1);
            }
            RenderResult::NoDamage => {
                self.queue_estimated_vblank_timer();
            }
            RenderResult::Skipped => {
                self.redraw_state = match std::mem::take(&mut self.redraw_state) {
                    RedrawState::WaitingForEstimatedVBlank(token)
                    | RedrawState::WaitingForEstimatedVBlankAndQueued(token) => {
                        RedrawState::WaitingForEstimatedVBlank(token)
                    }
                    _ => RedrawState::Idle,
                };
            }
        }
        let now = Duration::from(self.state.clock.now());
        crate::session::send_frame_callbacks_for_state(&self.state, now);
    }

    fn queue_estimated_vblank_timer(&mut self) {
        match std::mem::take(&mut self.redraw_state) {
            RedrawState::Queued => {}
            RedrawState::WaitingForEstimatedVBlank(token)
            | RedrawState::WaitingForEstimatedVBlankAndQueued(token) => {
                self.redraw_state = RedrawState::WaitingForEstimatedVBlank(token);
                return;
            }
            RedrawState::Idle | RedrawState::WaitingForVBlank { .. } => {
                unreachable!("an estimated vblank is only queued from a queued redraw")
            }
        }
        let delay = crate::session::next_vblank_delay(
            self.last_vblank,
            Duration::from(self.state.clock.now()),
            crate::session::refresh_interval(&self.state),
        );
        match self.loop_handle.insert_source(
            Timer::from_duration(delay),
            |_, _, data: &mut Udev| {
                data.on_estimated_vblank();
                TimeoutAction::Drop
            },
        ) {
            Ok(token) => self.redraw_state = RedrawState::WaitingForEstimatedVBlank(token),
            Err(error) => warn!(%error, "scheduling the estimated vblank"),
        }
    }

    fn on_estimated_vblank(&mut self) {
        self.state.frame_callback_cycle = self.state.frame_callback_cycle.wrapping_add(1);
        match std::mem::take(&mut self.redraw_state) {
            RedrawState::WaitingForEstimatedVBlank(_) => {}
            RedrawState::WaitingForEstimatedVBlankAndQueued(_) => {
                self.redraw_state = RedrawState::Queued;
                return;
            }
            RedrawState::Idle | RedrawState::Queued | RedrawState::WaitingForVBlank { .. } => {
                unreachable!("the estimated vblank timer outlived its redraw state")
            }
        }
        if self.state.animating(Instant::now()) {
            self.queue_redraw();
        } else {
            let now = Duration::from(self.state.clock.now());
            crate::session::send_frame_callbacks_for_state(&self.state, now);
        }
    }

    fn on_drm_error(&mut self) {
        // A lost page flip would stall the redraw loop forever; a late one lands in on_vblank.
        if matches!(self.redraw_state, RedrawState::WaitingForVBlank { .. }) {
            self.redraw_state = RedrawState::Idle;
            self.queue_redraw();
        }
    }

    fn on_vblank(&mut self, metadata: Option<&DrmEventMetadata>) {
        if !self.active {
            return;
        }
        self.vblank_count += 1;
        let redraw_needed = match std::mem::take(&mut self.redraw_state) {
            RedrawState::WaitingForVBlank { redraw_needed } => redraw_needed,
            // Rogue vblanks happen after resume or DPMS: https://github.com/niri-wm/niri/issues/556
            unexpected => {
                warn!(?unexpected, "vblank without a queued frame");
                if let RedrawState::WaitingForEstimatedVBlank(token)
                | RedrawState::WaitingForEstimatedVBlankAndQueued(token) = unexpected
                {
                    self.loop_handle.remove(token);
                }
                true
            }
        };
        if let Err(error) = self.compositor.frame_submitted() {
            warn!(%error, "frame submitted");
        }
        let (time, clock) = match metadata.map(|metadata| metadata.time) {
            Some(DrmEventTime::Monotonic(time)) => (time, wp_presentation_feedback::Kind::HwClock),
            _ => (
                Duration::from(self.state.clock.now()),
                wp_presentation_feedback::Kind::empty(),
            ),
        };
        self.last_vblank = Some(time);
        if let Some(mut feedback) = self.presented_feedback.take() {
            feedback.presented::<_, smithay::utils::Monotonic>(
                time,
                Refresh::fixed(crate::session::refresh_interval(&self.state)),
                self.vblank_count,
                wp_presentation_feedback::Kind::Vsync
                    | wp_presentation_feedback::Kind::HwCompletion
                    | clock,
            );
        }
        if redraw_needed || self.state.animating(Instant::now()) {
            self.queue_redraw();
        } else {
            crate::session::send_frame_callbacks_for_state(&self.state, time);
        }
    }

    fn render(&mut self) -> RenderResult {
        let renderer = if self.target_node == self.render_node {
            self.gpu.single_renderer(&self.render_node)
        } else {
            self.gpu.renderer(
                &self.render_node,
                &self.target_node,
                self.compositor.format(),
            )
        };
        let mut renderer = match renderer {
            Ok(renderer) => renderer,
            Err(error) => {
                warn!(%error, "no renderer on the render node");
                return RenderResult::Skipped;
            }
        };
        let elements = crate::session::build_output_elements(
            &mut self.state,
            &mut renderer,
            &self.cursor,
            true,
        );
        let gles = renderer.as_mut();
        self.state.init_capture_dma(gles);
        let capture_elements = if self.capture_requested || !self.state.pending_captures.is_empty()
        {
            crate::session::build_output_elements(&mut self.state, gles, &self.cursor, true)
        } else {
            Vec::new()
        };
        if self.capture_requested {
            self.capture_requested = false;
            if let Some(mode) = self.output.current_mode() {
                let size = smithay::utils::Size::from((mode.size.w, mode.size.h));
                if let Err(error) = crate::capture::capture_output(
                    gles,
                    &capture_elements,
                    size,
                    render_transform(),
                    self.state.scale.fractional_scale(),
                ) {
                    warn!(%error, "debug capture failed");
                }
            }
        }
        if let (Some(request), Some(mode)) = (
            self.state.take_screenshot_request(),
            self.output.current_mode(),
        ) {
            let size = smithay::utils::Size::from((mode.size.w, mode.size.h));
            crate::session::log_screenshot(crate::session::save_screenshot(
                &mut self.state,
                gles,
                &self.cursor,
                size,
                render_transform(),
                request,
            ));
        }
        crate::capture::serve_pending(&mut self.state, gles, &self.cursor, &capture_elements);
        if self.state.theme_fade.is_some() {
            self.compositor.reset_buffer_ages();
        }
        match self.compositor.render_frame(
            &mut renderer,
            &elements,
            ctdesign::DESKTOP_CLEAR.premultiplied(),
            self.frame_flags,
        ) {
            Ok(mut result) => {
                crate::session::update_primary_scanout_outputs(&self.state, &result.states);
                let mut feedback =
                    crate::session::take_presentation_feedback(&self.state, &result.states);
                send_dmabuf_feedbacks(
                    &self.state,
                    &self.output,
                    &result.states,
                    &self.render_feedback,
                    &self.scanout_feedback,
                );
                let now = Instant::now();
                if let Some(fade) = self.state.theme_fade.as_mut() {
                    match fade.tick(now) {
                        crate::theme_fade::FadeTick::Done => self.state.theme_fade = None,
                        _ => {
                            result.is_empty = false;
                        }
                    }
                }
                if result.is_empty {
                    feedback.discarded();
                    if !self.frame_was_empty {
                        self.frame_was_empty = true;
                        tracing::debug!("frame skipped: no damage");
                    }
                    return RenderResult::NoDamage;
                }
                self.frames_queued += 1;
                self.frame_was_empty = false;
                tracing::trace!(
                    frames_queued = self.frames_queued,
                    "frame queued with damage"
                );
                match self.compositor.queue_frame(()) {
                    Ok(()) => {
                        self.presented_feedback = Some(feedback);
                        RenderResult::Submitted
                    }
                    Err(error) => {
                        feedback.discarded();
                        warn!(%error, "queueing a frame");
                        RenderResult::NoDamage
                    }
                }
            }
            Err(error) => {
                warn!(%error, "rendering a frame");
                RenderResult::NoDamage
            }
        }
    }
}

#[derive(Debug, Default)]
enum RedrawState {
    #[default]
    Idle,
    Queued,
    WaitingForVBlank {
        redraw_needed: bool,
    },
    WaitingForEstimatedVBlank(RegistrationToken),
    WaitingForEstimatedVBlankAndQueued(RegistrationToken),
}

impl RedrawState {
    fn queued(self) -> Self {
        match self {
            Self::Idle => Self::Queued,
            Self::WaitingForEstimatedVBlank(token) => {
                Self::WaitingForEstimatedVBlankAndQueued(token)
            }
            Self::WaitingForVBlank { .. } => Self::WaitingForVBlank {
                redraw_needed: true,
            },
            queued @ (Self::Queued | Self::WaitingForEstimatedVBlankAndQueued(_)) => queued,
        }
    }
}

enum RenderResult {
    Submitted,
    NoDamage,
    Skipped,
}

fn open_drm_device(session: &mut LibSeatSession, path: &Path) -> Result<DrmDeviceFd> {
    let fd = session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|error| anyhow!("opening {}: {error}", path.display()))?;
    Ok(DrmDeviceFd::new(DeviceFd::from(fd)))
}

fn feedback_node(node: DrmNode) -> DrmNode {
    node.node_with_type(NodeType::Render)
        .and_then(Result::ok)
        .unwrap_or(node)
}

fn find_render_node(seat: &str, scanout: &DrmNode) -> Result<DrmNode> {
    let mut candidates = Vec::new();
    if let Ok(Some(path)) = primary_gpu(seat) {
        candidates.push(path);
    }
    if let Ok(gpus) = all_gpus(seat) {
        candidates.extend(gpus);
    }
    for path in candidates {
        let Ok(node) = DrmNode::from_path(&path) else {
            continue;
        };
        if node.dev_id() == scanout.dev_id() {
            continue;
        }
        if let Some(Ok(render)) = node.node_with_type(NodeType::Render) {
            return Ok(render);
        }
    }
    bail!(
        "no render node found for --drm-device ({}); a vkms test needs a render-capable GPU",
        scanout.dev_path().unwrap_or_default().display()
    )
}

fn find_panel(
    drm: &DrmDevice,
) -> Result<(
    connector::Info,
    crtc::Handle,
    smithay::reexports::drm::control::Mode,
)> {
    let resources = drm.resource_handles().context("reading DRM resources")?;
    let mut fallback = None;

    for &handle in resources.connectors() {
        let connector = drm
            .get_connector(handle, false)
            .with_context(|| format!("reading connector {handle:?}"))?;
        if connector.state() != connector::State::Connected || connector.modes().is_empty() {
            continue;
        }
        let mode = connector
            .modes()
            .iter()
            .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .copied()
            .unwrap_or(connector.modes()[0]);
        let crtc = connector_crtc(drm, &resources, &connector)?;
        if connector.interface() == Interface::EmbeddedDisplayPort {
            return Ok((connector, crtc, mode));
        }
        fallback.get_or_insert((connector, crtc, mode));
    }

    fallback.ok_or_else(|| anyhow!("no connected panel with a mode"))
}

fn connector_crtc(
    drm: &DrmDevice,
    resources: &smithay::reexports::drm::control::ResourceHandles,
    connector: &connector::Info,
) -> Result<crtc::Handle> {
    if let Some(encoder) = connector.current_encoder()
        && let Ok(encoder) = drm.get_encoder(encoder)
        && let Some(crtc) = encoder.crtc()
    {
        return Ok(crtc);
    }
    for &handle in connector.encoders() {
        if let Ok(encoder) = drm.get_encoder(handle)
            && let Some(crtc) = resources
                .filter_crtcs(encoder.possible_crtcs())
                .first()
                .copied()
        {
            return Ok(crtc);
        }
    }
    bail!("connector {:?} has no usable CRTC", connector.handle())
}

fn make_output(
    connector: &connector::Info,
    mode: smithay::reexports::drm::control::Mode,
) -> Result<Output> {
    let name = format!(
        "{}-{}",
        connector.interface().as_str(),
        connector.interface_id()
    );
    let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));
    let output = Output::new(
        name,
        PhysicalProperties {
            size: (phys_w as i32, phys_h as i32).into(),
            subpixel: Subpixel::Unknown,
            make: "DRM".to_string(),
            model: "Panel".to_string(),
            serial_number: String::new(),
        },
    );
    for mode in connector.modes() {
        output.add_mode(WlMode::from(*mode));
    }
    let mode = WlMode::from(mode);
    output.set_preferred(mode);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Integer(1)),
        Some((0, 0).into()),
    );
    Ok(output)
}

fn scanout_feedback(
    surface: &DrmSurface,
    planes: &Planes,
    renderer: &GlesRenderer,
    render_node: DrmNode,
    scanout_node: DrmNode,
) -> anyhow::Result<DmabufFeedback> {
    let render_formats = renderer.dmabuf_formats();
    let scanout_formats = |formats: &mut dyn Iterator<Item = &Format>| {
        let mut formats: Vec<Format> = formats
            .filter(|format| render_formats.contains(format))
            .copied()
            .collect();
        // Cross-device buffers scan out glitched unless Linear: niri a8562a1 src/backend/tty.rs (surface_dmabuf_feedback).
        if render_node != scanout_node {
            formats.retain(|format| format.modifier == Modifier::Linear);
        }
        formats
    };
    let primary = scanout_formats(&mut surface.plane_info().formats.iter());
    let overlay =
        scanout_formats(&mut planes.overlay.iter().flat_map(|plane| plane.formats.iter()));
    DmabufFeedbackBuilder::new(feedback_node(render_node).dev_id(), render_formats.clone())
        .add_preference_tranche(scanout_node.dev_id(), TrancheFlags::Scanout, primary, 4..=5)
        .add_preference_tranche(scanout_node.dev_id(), TrancheFlags::Scanout, overlay, 4..=5)
        .build()
        .map_err(|error| anyhow!("building the scanout dmabuf feedback: {error}"))
}

fn send_dmabuf_feedbacks(
    state: &State,
    output: &Output,
    render_states: &RenderElementStates,
    render_feedback: &DmabufFeedback,
    scanout_feedback: &DmabufFeedback,
) {
    let send = |surface: &WlSurface| {
        send_dmabuf_feedback_surface_tree(
            surface,
            output,
            surface_primary_scanout_output,
            |surface, _| {
                select_dmabuf_feedback(surface, render_states, render_feedback, scanout_feedback)
            },
        );
    };
    for surface in state.windows.surfaces() {
        send(surface);
    }
    for entry in state.layers.all() {
        send(entry.surface.wl_surface());
    }
}

impl crate::idle::IdleHost for Udev {
    fn idle_state(&mut self) -> &mut State {
        &mut self.state
    }

    fn request_redraw(&mut self) {
        self.queue_redraw();
    }
}

impl crate::blockers::BlockerSink for LoopHandle<'static, Udev> {
    fn insert(
        &mut self,
        source: crate::blockers::BlockerSource,
        client: smithay::reexports::wayland_server::Client,
    ) -> bool {
        self.insert_source(source, move |_, _, data: &mut Udev| {
            if let Some(client_state) = client.get_data::<ClientState>() {
                let dh = data.state.display_handle.clone();
                client_state
                    .compositor_state
                    .blocker_cleared(&mut data.state, &dh);
            }
            Ok(())
        })
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{remove_stale_unit_links, unit_template};
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("malaysia-units-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("user/malaysia-session.target.wants")).unwrap();
        std::fs::create_dir_all(dir.join("system")).unwrap();
        dir
    }

    #[test]
    fn an_instance_resolves_to_its_template() {
        assert_eq!(
            unit_template("malaysia-shell@menubar.service"),
            "malaysia-shell@.service"
        );
        assert_eq!(
            unit_template("malaysia-session.target"),
            "malaysia-session.target"
        );
    }

    #[test]
    fn only_broken_links_with_malaysia_names_are_removed() {
        let dir = scratch("stale");
        let user = dir.join("user");
        std::fs::write(dir.join("system/malaysia-session.target"), "").unwrap();
        symlink(
            "/missing/result/malaysia-shell@.service",
            user.join("malaysia-shell@.service"),
        )
        .unwrap();
        symlink(
            "/missing/result/malaysia-shell@.service",
            user.join("malaysia-session.target.wants/malaysia-shell@dock.service"),
        )
        .unwrap();
        symlink("/dev/null", user.join("malaysia-wallpaper.service")).unwrap();
        symlink("/missing/other.service", user.join("other.service")).unwrap();
        std::fs::write(user.join("malaysia-session.target"), "").unwrap();

        assert!(remove_stale_unit_links(&user, &dir.join("system")));
        assert!(
            user.join("malaysia-shell@.service")
                .symlink_metadata()
                .is_err()
        );
        assert!(
            user.join("malaysia-session.target.wants/malaysia-shell@dock.service")
                .symlink_metadata()
                .is_err()
        );
        assert!(
            user.join("malaysia-wallpaper.service")
                .symlink_metadata()
                .is_ok()
        );
        assert!(user.join("other.service").symlink_metadata().is_ok());
        assert!(user.join("malaysia-session.target").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_removed_when_the_system_does_not_install_the_units() {
        let dir = scratch("no-system");
        let user = dir.join("user");
        symlink(
            "/missing/result/malaysia-shell@.service",
            user.join("malaysia-shell@.service"),
        )
        .unwrap();

        assert!(!remove_stale_unit_links(&user, &dir.join("system")));
        assert!(
            user.join("malaysia-shell@.service")
                .symlink_metadata()
                .is_ok()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
