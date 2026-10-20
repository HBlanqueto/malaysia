//! The direct session: DRM/KMS atomic, libseat and libinput.
//!
//! Bloque 8a is deliberately small: one primary GPU, the internal panel in its
//! preferred mode, one primary plane, no hotplug and no direct scanout. The
//! shared [`State`] and every protocol handler are the same as the winit
//! backend; only presentation and input differ.

#![allow(unsafe_code)]

use std::os::fd::AsFd;
use std::path::PathBuf;
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
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::drm::control::connector::{self, Interface};
use smithay::reexports::drm::control::{Device as ControlDevice, ModeTypeFlags, crtc};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_server::Display;
use smithay::utils::{DeviceFd, Transform};
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use smithay::wayland::drm_syncobj::supports_syncobj_eventfd;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{error, info, warn};

use crate::cursor::Cursors;
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
}

/// Runs Yuki on the primary DRM device.
///
/// No fallback: the caller asked for `--drm`, so a missing seat, GPU or eDP
/// connector is a startup error, not a silent switch to winit.
pub fn run() -> Result<()> {
    let mut event_loop = EventLoop::<Udev>::try_new().context("creating the event loop")?;
    let loop_signal = event_loop.get_signal();
    let display = Display::<State>::new().context("creating the Wayland display")?;

    let (mut session, session_notifier) =
        LibSeatSession::new().map_err(|error| anyhow!("opening the libseat session: {error}"))?;
    let seat = session.seat();

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
    // udev may hand us the render node; libseat and DRM master need the
    // primary node for the same device.
    let node = match node.node_with_type(NodeType::Primary) {
        Some(Ok(primary)) => primary,
        _ => node,
    };
    let path = node.dev_path().unwrap_or(path);
    info!(?node, ?path, "using the primary DRM device");

    let fd = session
        .open(
            &path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|error| anyhow!("opening {}: {error}", path.display()))?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));

    let (mut drm, drm_notifier) =
        DrmDevice::new(drm_fd.clone(), true).map_err(|error| anyhow!("opening DRM: {error}"))?;
    let gbm = GbmDevice::new(drm_fd.clone()).context("creating the GBM device")?;

    // SAFETY: the EGL display and context belong to this one thread and are
    // never shared; the same is true of the GLES renderer. This is the scoped
    // exception to ADR 0003 recorded in ADR 0030.
    let egl_display = unsafe { EGLDisplay::new(gbm.clone()) }
        .map_err(|error| anyhow!("creating the EGL display: {error}"))?;
    let egl_context =
        EGLContext::new(&egl_display).map_err(|error| anyhow!("creating EGL: {error}"))?;
    let renderer = unsafe { GlesRenderer::new(egl_context) }
        .map_err(|error| anyhow!("creating GLES: {error}"))?;
    let (connector, crtc, mode) = find_panel(&drm)?;
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
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(gbm.clone(), node.into());
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
        Some(gbm.clone()),
    )
    .map_err(|error| anyhow!("creating the DRM compositor: {error}"))?;

    let feedback = DmabufFeedbackBuilder::new(node.dev_id(), renderer.dmabuf_formats())
        .build()
        .map_err(|error| anyhow!("building the dmabuf feedback: {error}"))?;
    let syncobj = supports_syncobj_eventfd(&drm_fd).then_some(drm_fd);
    let loop_handle = event_loop.handle();
    let state = State::new_with_output(
        loop_signal.clone(),
        &display.handle(),
        smithay::utils::Size::from((i32::from(mode.size().0), i32::from(mode.size().1))),
        output.clone(),
        feedback,
        syncobj,
        Box::new(loop_handle.clone()),
    )
    .map_err(|error| anyhow!("building the compositor state: {error}"))?;

    let mut udev = Udev {
        loop_handle: loop_handle.clone(),
        display,
        state,
        session: session.clone(),
        drm,
        compositor,
        renderer,
        cursor: Cursors::load(State::OUTPUT_SCALE),
        output,
        start_time: Instant::now(),
        active: true,
        repaint_timer: None,
    };

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
    let mut libinput =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput
        .udev_assign_seat(&seat)
        .map_err(|_| anyhow!("libinput could not take seat {seat}"))?;
    let libinput_backend = LibinputInputBackend::new(libinput.clone());
    loop_handle
        .insert_source(libinput_backend, |event, _, data: &mut Udev| {
            data.state.process_input_event(event);
            data.after_input();
        })
        .map_err(|error| anyhow!("inserting the libinput source: {error}"))?;

    // Session pause/resume is the VT switch path.
    loop_handle
        .insert_source(
            session_notifier,
            move |event, _, data: &mut Udev| match event {
                SessionEvent::PauseSession => {
                    info!("session paused; releasing DRM and libinput");
                    libinput.suspend();
                    data.drm.pause();
                    data.active = false;
                }
                SessionEvent::ActivateSession => {
                    info!("session resumed");
                    if libinput.resume().is_err() {
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
    loop_handle
        .insert_source(socket, |stream, _, data: &mut Udev| {
            let mut handle = data.display.handle();
            match handle.insert_client(stream, Arc::new(ClientState::default())) {
                Ok(_) => info!("client connected"),
                Err(error) => warn!(%error, "rejected a client"),
            }
        })
        .map_err(|error| anyhow!("inserting the socket source: {error}"))?;

    let shell_started = start_shell_session(&socket_name);
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
        if let Err(error) = self.compositor.frame_submitted() {
            warn!(%error, "frame submitted");
        }
        crate::session::send_frame_callbacks_for_state(&self.state, self.start_time.elapsed());
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
        );
        match self.compositor.render_frame(
            &mut self.renderer,
            &elements,
            [0.06, 0.06, 0.09, 1.0],
            FrameFlags::empty(),
        ) {
            Ok(result) if !result.is_empty => {
                if let Err(error) = self.compositor.queue_frame(()) {
                    warn!(%error, "queueing a frame");
                    self.schedule_repaint();
                }
            }
            Ok(_) => self.schedule_repaint(),
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
