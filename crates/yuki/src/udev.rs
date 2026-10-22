//! The direct session: DRM/KMS atomic, libseat and libinput.
//!
//! Bloque 8a is deliberately small: one primary GPU, the internal panel in its
//! preferred mode, one primary plane, no hotplug and no direct scanout. The
//! shared [`State`] and every protocol handler are the same as the winit
//! backend; only presentation and input differ. The development-only
//! `--drm-device`/`--no-input` pair can move scanout to vkms while rendering
//! on the boot GPU's render node.

#![allow(unsafe_code)]

use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmNode, NodeType};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::gles::GlesRenderer;
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
use smithay::reexports::wayland_server::Display;
use smithay::utils::{DeviceFd, Rectangle, Size, Transform};
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use smithay::wayland::drm_syncobj::supports_syncobj_eventfd;
use smithay::wayland::image_copy_capture::CaptureFailureReason;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{error, info, warn};

use crate::cursor::{Cursors, OutputRenderElement};
use crate::state::{ClientState, State};

type Compositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

struct Udev {
    loop_handle: LoopHandle<'static, Udev>,
    display: Display<State>,
    state: State,
    session: LibSeatSession,
    drm: DrmDevice,
    compositor: Compositor,
    renderer: GlesRenderer,
    cursor: Cursors,
    output: Output,
    start_time: Instant,
    active: bool,
    repaint_timer: Option<RegistrationToken>,
    loop_signal: LoopSignal,
    capture_requested: bool,
    vblank_count: u64,
    frames_queued: u64,
    frame_was_empty: bool,
}

/// Development-only overrides for the direct session.
#[derive(Debug, Clone, Default)]
pub struct DrmOptions {
    /// A specific KMS node instead of the seat's primary GPU.
    pub device: Option<PathBuf>,
    /// Do not open libinput.
    pub no_input: bool,
}

/// Runs Yuki on the primary DRM device, or on a development `--drm-device`.
///
/// No fallback: the caller asked for `--drm`, so a missing seat, GPU or eDP
/// connector is a startup error, not a silent switch to winit. With a device
/// override, GBM/EGL run on a render node and only scanout uses the named KMS
/// node; libinput and the shell target are skipped.
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
            // udev may hand us the render node; libseat and DRM master need
            // the primary node for the same device.
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

    // A development override keeps rendering on the boot GPU's render node
    // while scanout goes to the named KMS node (vkms). Production keeps the
    // previous one-device behaviour.
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

    // SAFETY: the EGL display and context belong to this one thread and are
    // never shared; the same is true of the GLES renderer. This is the scoped
    // exception to ADR 0003 recorded in ADR 0030.
    let egl_display = unsafe { EGLDisplay::new(render_gbm.clone()) }
        .map_err(|error| anyhow!("creating the EGL display: {error}"))?;
    let egl_context =
        EGLContext::new(&egl_display).map_err(|error| anyhow!("creating EGL: {error}"))?;
    let renderer = unsafe { GlesRenderer::new(egl_context) }
        .map_err(|error| anyhow!("creating GLES: {error}"))?;
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

    // Bloque 8a is one primary plane. Clearing overlay and cursor here makes
    // DrmCompositor unable to hand anything else to KMS.
    let mut planes = surface.planes().clone();
    planes.overlay.clear();
    planes.cursor.clear();

    let allocator = GbmAllocator::new(
        render_gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(scanout_gbm.clone(), scanout_node.into());
    let render_formats: Vec<_> = renderer
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect();
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

    let feedback = DmabufFeedbackBuilder::new(render_node.dev_id(), renderer.dmabuf_formats())
        .build()
        .map_err(|error| anyhow!("building the dmabuf feedback: {error}"))?;
    let syncobj = supports_syncobj_eventfd(&render_fd).then_some(render_fd);
    let loop_handle = event_loop.handle();
    let state = State::new_with_output(
        loop_signal.clone(),
        &display.handle(),
        smithay::utils::Size::from((i32::from(mode.size().0), i32::from(mode.size().1))),
        output.clone(),
        feedback,
        syncobj,
        Box::new(loop_handle.clone()),
        Box::new(crate::idle::IdleTimers::new(loop_handle.clone())),
    )
    .map_err(|error| anyhow!("building the compositor state: {error}"))?;

    let cursor = Cursors::load(state.scale.fractional_scale().ceil() as i32);
    let mut udev = Udev {
        loop_handle: loop_handle.clone(),
        display,
        state,
        session: session.clone(),
        drm,
        compositor,
        renderer,
        cursor,
        output,
        start_time: Instant::now(),
        active: true,
        repaint_timer: None,
        loop_signal: loop_signal.clone(),
        capture_requested: false,
        vblank_count: 0,
        frames_queued: 0,
        frame_was_empty: false,
    };

    // Rebuild the chrome theme when appearance.domain or wallpaper.state
    // changes, without a restart.
    loop_handle
        .insert_source(
            Timer::from_duration(Duration::from_millis(500)),
            |_, _, data: &mut Udev| {
                data.state.refresh_theme();
                if data.state.take_redraw_request() {
                    data.render();
                }
                TimeoutAction::ToDuration(Duration::from_millis(500))
            },
        )
        .map_err(|error| anyhow!("inserting the theme watcher: {error}"))?;

    // DRM events: VBlank is the pacing clock.
    loop_handle
        .insert_source(
            drm_notifier,
            move |event, _metadata, data: &mut Udev| match event {
                smithay::backend::drm::DrmEvent::VBlank(_crtc) => data.on_vblank(),
                smithay::backend::drm::DrmEvent::Error(error) => {
                    error!(%error, "DRM event error");
                }
            },
        )
        .map_err(|error| anyhow!("inserting the DRM source: {error}"))?;

    // libinput is the only input source in 8a; no hotplug backend yet.
    // `--no-input` skips it entirely, so a vkms test never opens /dev/input.
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

    // Session pause/resume is the VT switch path.
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
                    data.render();
                }
            },
        )
        .map_err(|error| anyhow!("inserting the session source: {error}"))?;

    let socket = ListeningSocketSource::new_auto().context("creating the Wayland socket")?;
    let socket_name = socket.socket_name().to_string_lossy().into_owned();
    info!(socket = %socket_name, "listening on the Wayland socket");
    udev.state.set_wayland_display(socket_name.clone());
    loop_handle
        .insert_source(socket, |stream, _, data: &mut Udev| {
            let mut handle = data.display.handle();
            match handle.insert_client(stream, Arc::new(ClientState::default())) {
                Ok(_) => info!("client connected"),
                Err(error) => warn!(%error, "rejected a client"),
            }
        })
        .map_err(|error| anyhow!("inserting the socket source: {error}"))?;

    // A development device override is isolated: it must not start the
    // user's shell units on the test output.
    let shell_started = if options.device.is_some() || options.no_input {
        info!("shell target skipped for the development DRM options");
        false
    } else {
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
                    data.render();
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
                if data.state.take_redraw_request() {
                    data.render();
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| anyhow!("inserting the display source: {error}"))?;

    udev.render();
    info!("direct session ready");
    let result = event_loop.run(None, &mut udev, |_| {});
    if shell_started {
        stop_shell_session();
    }
    result.context("running the event loop")?;
    Ok(())
}

/// Starts the shell target after the socket exists. The target's units read
/// `WAYLAND_DISPLAY` from the user manager, not from Yuki's process.
fn start_shell_session(socket: &str) -> bool {
    let import = systemctl(
        &[
            "--user",
            "import-environment",
            "WAYLAND_DISPLAY",
            "XDG_CURRENT_DESKTOP",
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

    match systemctl(&["--user", "start", "yuki-session.target"], Some(socket)) {
        Ok(output) if output.status.success() => {
            info!("yuki-session.target started");
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

fn stop_shell_session() {
    let _ = systemctl(&["--user", "stop", "yuki-session.target"], None);
}

fn systemctl(args: &[&str], socket: Option<&str>) -> std::io::Result<std::process::Output> {
    let mut command = crate::spawn::command("systemctl");
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(socket) = socket {
        command.env("WAYLAND_DISPLAY", socket);
        command.env("XDG_CURRENT_DESKTOP", "Yuki");
    }
    crate::spawn::spawn(command)?.wait_with_output()
}

fn missing_target_help() -> String {
    let units = unit_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "result/share/systemd/user".to_string());
    format!(
        "yuki-session.target no está instalado en el user manager; la sesión sigue sin shell.          Para probar: systemctl --user link {units}/yuki-session.target;          systemctl --user link {units}/yuki-shell@.service; systemctl --user daemon-reload;          systemctl --user enable --now yuki-shell@wallpaper.service yuki-shell@menubar.service yuki-shell@dock.service"
    )
}

fn unit_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.parent()?.join("share/systemd/user"))
}

impl Udev {
    fn dispatch_clients(&mut self) {
        if let Err(error) = self.display.dispatch_clients(&mut self.state) {
            error!(%error, "dispatching clients");
            return;
        }
        crate::session::import_pending_dmabufs(&mut self.state, &mut self.renderer);
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
        if self.state.take_redraw_request() {
            self.render();
        }
    }

    fn on_vblank(&mut self) {
        if !self.active {
            return;
        }
        self.vblank_count += 1;
        if self.vblank_count <= 5 || self.vblank_count.is_multiple_of(120) {
            info!(count = self.vblank_count, "vblank");
        }
        if let Err(error) = self.compositor.frame_submitted() {
            warn!(%error, "frame submitted");
        }
        let time = self.start_time.elapsed();
        crate::session::send_presentation_feedbacks_for_state(
            &self.state,
            time,
            self.vblank_count,
            true,
        );
        crate::session::send_frame_callbacks_for_state(&self.state, time);
        self.render();
    }

    fn render(&mut self) {
        if !self.active {
            return;
        }
        let elements = crate::session::build_output_elements(
            &mut self.state,
            &mut self.renderer,
            &self.cursor,
            true,
        );
        self.state.init_capture_dma(&self.renderer);
        if self.capture_requested {
            self.capture_requested = false;
            if let Some(mode) = self.output.current_mode() {
                let size = smithay::utils::Size::from((mode.size.w, mode.size.h));
                // The capture re-renders offscreen and then flips the GL
                // readback; Flipped180 keeps it upright, as in winit.
                let transform = smithay::utils::Transform::Flipped180;
                if let Err(error) = crate::capture::capture_output(
                    &mut self.renderer,
                    &elements,
                    size,
                    transform,
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
                &mut self.renderer,
                &self.cursor,
                size,
                Transform::Flipped180,
                request,
            ));
        }
        let capture_scale = self.state.scale.fractional_scale();
        for pending in std::mem::take(&mut self.state.pending_captures) {
            let (size, window_elements) = match &pending.target {
                crate::capture::CaptureTarget::Output { size } => (*size, None),
                crate::capture::CaptureTarget::Window { id, size } => {
                    let Some(surface) = self.state.windows.surface_of(*id).cloned() else {
                        pending.frame.fail(CaptureFailureReason::Unknown);
                        continue;
                    };
                    (
                        *size,
                        Some(crate::session::build_window_elements(
                            &mut self.renderer,
                            &surface,
                            capture_scale,
                        )),
                    )
                }
            };
            let drawn: &[OutputRenderElement<GlesRenderer>] =
                window_elements.as_deref().unwrap_or(&elements);
            let damage = Rectangle::from_size(Size::from((size.w, size.h)));
            match crate::capture::capture_into_buffer(
                &mut self.renderer,
                drawn,
                size,
                capture_scale,
                [0.06, 0.06, 0.09, 1.0],
                &pending.frame.buffer(),
            ) {
                Ok(()) => {
                    pending
                        .frame
                        .success(Transform::Normal, Some(vec![damage]), Duration::ZERO)
                }
                Err(error) => {
                    warn!(%error, "screencopy frame failed");
                    pending.frame.fail(CaptureFailureReason::Unknown);
                }
            }
        }
        match self.compositor.render_frame(
            &mut self.renderer,
            &elements,
            [0.06, 0.06, 0.09, 1.0],
            FrameFlags::empty(),
        ) {
            Ok(result) if !result.is_empty => {
                self.frames_queued += 1;
                self.frame_was_empty = false;
                info!(
                    frames_queued = self.frames_queued,
                    "frame queued with damage"
                );
                if let Err(error) = self.compositor.queue_frame(()) {
                    warn!(%error, "queueing a frame");
                    self.schedule_repaint();
                }
            }
            Ok(_) => {
                if !self.frame_was_empty {
                    self.frame_was_empty = true;
                    tracing::debug!("frame skipped: no damage");
                }
                crate::session::send_presentation_feedbacks_for_state(
                    &self.state,
                    self.start_time.elapsed(),
                    self.vblank_count,
                    false,
                );
                self.schedule_repaint();
            }
            Err(error) => {
                warn!(%error, "rendering a frame");
                self.schedule_repaint();
            }
        }
    }

    /// Polls again after one refresh when a frame had no damage. VBlank only
    /// arrives for queued frames, so an idle output needs this timer.
    fn schedule_repaint(&mut self) {
        if self.repaint_timer.is_some() {
            return;
        }
        let refresh = self
            .output
            .current_mode()
            .map(|mode| mode.refresh)
            .unwrap_or(60_000)
            .max(1) as u64;
        let delay = Duration::from_micros(1_000_000 / refresh);
        let timer = Timer::from_duration(delay);
        if let Err(error) = self
            .loop_handle
            .insert_source(timer, |_, _, data: &mut Udev| {
                data.render();
                TimeoutAction::Drop
            })
        {
            warn!(%error, "scheduling a repaint");
        }
    }
}

/// Opens a DRM node through the session.
fn open_drm_device(session: &mut LibSeatSession, path: &Path) -> Result<DrmDeviceFd> {
    let fd = session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|error| anyhow!("opening {}: {error}", path.display()))?;
    Ok(DrmDeviceFd::new(DeviceFd::from(fd)))
}

/// Finds a render node on a different device than `scanout`.
///
/// `primary_gpu` puts a boot VGA card first, so the normal case is the
/// amdgpu render node while `scanout` is vkms. The fallback scans the seat's
/// GPUs in case there is no boot VGA flag. Only paths are stat'ed here; this
/// never opens the primary card.
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

/// Picks the internal panel and its preferred mode, plus a usable CRTC.
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

impl crate::idle::IdleHost for Udev {
    fn idle_state(&mut self) -> &mut State {
        &mut self.state
    }

    fn request_redraw(&mut self) {
        self.schedule_repaint();
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
