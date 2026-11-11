use std::time::{Duration, Instant};

use smithay::backend::input::TabletToolDescriptor;
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::delegate_dispatch2;
use smithay::reexports::calloop::Interest;
use smithay::wayland::dmabuf::{get_dmabuf, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::drm_syncobj::{DrmSyncobjCachedState, DrmSyncobjHandler, DrmSyncobjState};
use smithay::wayland::foreign_toplevel_list::{ForeignToplevelListHandler, ForeignToplevelListState};
use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::Seat;
use smithay::input::SeatHandler;
use smithay::input::tablet::TabletSeatHandler;
use smithay::input::SeatState;
use smithay::input::pointer::{CursorImageStatus, Focus};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
    Mode as KdeMode, OrgKdeKwinServerDecoration,
};
use smithay::reexports::wayland_server::Client;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_buffer;
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::WEnum;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::output::{Output, WeakOutput};
use smithay::utils::{Buffer as BufferCoords, Logical, Rectangle, Serial, Size};
use malaysia_scene::scene::WindowId;
use smithay::wayland::foreign_toplevel_list::ForeignToplevelHandle;
use smithay::wayland::image_capture_source::{
    ImageCaptureSource, ImageCaptureSourceHandler, OutputCaptureSourceHandler,
    OutputCaptureSourceState, ToplevelCaptureSourceHandler, ToplevelCaptureSourceState,
};
use smithay::wayland::image_copy_capture::{
    BufferConstraints, CaptureFailureReason, DmabufConstraints, Frame, ImageCopyCaptureHandler,
    ImageCopyCaptureState, Session, SessionRef,
};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    Blocker, BufferAssignment, CompositorClientState, CompositorHandler, CompositorState, Damage,
    SurfaceAttributes, add_blocker, add_pre_commit_hook, with_states,
};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};
use smithay::wayland::pointer_constraints::PointerConstraintsHandler;
use smithay::wayland::selection::{SelectionHandler, SelectionTarget};
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler,
};
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};
use smithay::wayland::shell::wlr_layer::{Layer, LayerSurface, WlrLayerShellHandler, WlrLayerShellState};
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::{
    Configure, PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};

use malaysia_scene::scene::Decoration;

use crate::capture::CaptureTarget;
use crate::delegate_appmenu;
use crate::delegate_foreign_management;
use crate::delegate_gtk;
use crate::state::{ClientState, State};

const ACTIVATION_TOKEN_MAX_AGE: Duration = Duration::from_secs(30);

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted with ClientState in the socket handler")
            .compositor_state
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        add_pre_commit_hook::<Self, _>(surface, |state, _dh, surface| {
            let (has_dmabuf, dmabuf, acquire) = with_states(surface, |states| {
                let acquire = states
                    .cached_state
                    .get::<DrmSyncobjCachedState>()
                    .pending()
                    .acquire_point
                    .clone();
                let dmabuf = states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .pending()
                    .buffer
                    .as_ref()
                    .and_then(|assignment| match assignment {
                        BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).cloned().ok(),
                        _ => None,
                    });
                let has_dmabuf = dmabuf.is_some();
                (has_dmabuf, dmabuf, acquire)
            });
            let Some(client) = surface.client() else {
                return;
            };

            match pending_blocker(has_dmabuf, acquire.is_some()) {
                Some(BlockerKind::Explicit) => {
                    if let Some(acquire) = acquire
                        && let Ok((blocker, source)) = acquire.generate_blocker()
                    {
                        insert_blocker_source(
                            state,
                            surface,
                            client,
                            crate::blockers::BlockerSource::Sync(source),
                            blocker,
                        );
                    }
                }
                Some(BlockerKind::Implicit) => {
                    if let Some(dmabuf) = dmabuf
                        && let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ)
                    {
                        insert_blocker_source(
                            state,
                            surface,
                            client,
                            crate::blockers::BlockerSource::Dmabuf(source),
                            blocker,
                        );
                    }
                }
                None => {}
            }
        });
    }

    fn commit(&mut self, surface: &WlSurface) {
        log_committed_buffer_type(surface);
        // Smithay 65aa86fa renderer/utils/mod.rs:221-226 drops empty damage; a new buffer needs full damage.
        with_states(surface, |states| {
            let mut cached = states.cached_state.get::<SurfaceAttributes>();
            let attrs = cached.current();
            if let Some(BufferAssignment::NewBuffer(buffer)) = &attrs.buffer
                && attrs.damage.is_empty()
                && let Some(size) = smithay::backend::renderer::buffer_dimensions(buffer)
            {
                attrs
                    .damage
                    .push(Damage::Buffer(Rectangle::from_size(size)));
            }
        });
        on_commit_buffer_handler::<Self>(surface);
        if let Some(layer) = self.layers.commit(surface, self.logical_output_size()) {
            self.layer_surface_mapped(&layer);
        }
        let workspace = {
            let (origin, size) = self.workspace();
            Rectangle::new(origin, size)
        };
        if let Some(mapped) = self.windows.commit(surface, workspace) {
            self.space_window_mapped(mapped);
            if self.takes_focus_on_press(mapped) {
                self.raise_and_focus(mapped);
            } else {
                self.windows.raise(mapped);
                self.sync_panels();
            }
            self.foreign_window_mapped(mapped);
            self.inject_test_shake();
            self.inject_test_press(mapped);
            self.inject_test_screenshot();
        }
        if let Some(id) = self.windows.id_of(surface)
            && self.focused_window() == Some(id)
            && !self.windows.is_presented(id, &self.minimized)
        {
            self.clear_keyboard_focus();
        }
        self.popup_commit(surface);
        self.request_redraw();
        self.theme_fade_mark_committed(surface.id());
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        self.clear_surface_appmenu(surface);
        self.clear_surface_gtk_menus(surface);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockerKind {
    Explicit,
    Implicit,
}

fn pending_blocker(has_dmabuf: bool, has_acquire_point: bool) -> Option<BlockerKind> {
    if !has_dmabuf {
        return None;
    }
    Some(if has_acquire_point {
        BlockerKind::Explicit
    } else {
        BlockerKind::Implicit
    })
}

fn insert_blocker_source(
    state: &mut State,
    surface: &WlSurface,
    client: Client,
    source: crate::blockers::BlockerSource,
    blocker: impl Blocker + Send + 'static,
) {
    if state.blocker_sink.insert(source, client) {
        add_blocker(surface, blocker);
    }
}

fn log_committed_buffer_type(surface: &WlSurface) {
    use smithay::backend::renderer::buffer_type;
    use smithay::wayland::compositor::{BufferAssignment, SurfaceAttributes, with_states};
    let buffer_type = with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceAttributes>()
            .current()
            .buffer
            .as_ref()
            .and_then(|assignment| match assignment {
                BufferAssignment::NewBuffer(buffer) => buffer_type(buffer),
                _ => None,
            })
    });
    if let Some(buffer_type) = buffer_type {
        tracing::trace!(surface = ?surface.id(), ?buffer_type, "surface committed a buffer");
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        self.pending_dmabuf_imports
            .push(crate::state::PendingDmabufImport { dmabuf, notifier });
    }
}

impl DrmSyncobjHandler for State {
    fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState> {
        self.syncobj_state.as_mut()
    }
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl ImageCaptureSourceHandler for State {
    fn source_destroyed(&mut self, _source: ImageCaptureSource) {}
}

impl OutputCaptureSourceHandler for State {
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState {
        &mut self.output_capture_source_state
    }

    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        source.user_data().insert_if_missing(|| output.downgrade());
    }
}

impl ToplevelCaptureSourceHandler for State {
    fn toplevel_capture_source_state(&mut self) -> &mut ToplevelCaptureSourceState {
        &mut self.toplevel_capture_source_state
    }

    fn toplevel_source_created(
        &mut self,
        source: ImageCaptureSource,
        toplevel: ForeignToplevelHandle,
    ) {
        if let Some(id) = toplevel.user_data().get::<WindowId>().copied() {
            source.user_data().insert_if_missing(|| id);
        }
    }
}

impl ImageCopyCaptureHandler for State {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.image_copy_capture_state
    }

    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        let size = match self.capture_target(source)? {
            CaptureTarget::Output { size } | CaptureTarget::Window { size, .. } => size,
        };
        Some(BufferConstraints {
            size: Size::<i32, BufferCoords>::from((size.w, size.h)),
            shm: vec![wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888],
            dma: self.capture_dma.as_ref().map(|dma| DmabufConstraints {
                node: dma.node,
                formats: dma.formats.clone(),
            }),
        })
    }

    fn new_session(&mut self, session: Session) {
        if let Some(constraints) = self.capture_constraints(&session.source()) {
            session.update_constraints(constraints);
        }
        self.capture_sessions.push(session);
    }

    fn session_destroyed(&mut self, session: SessionRef) {
        self.capture_sessions.retain(|known| *known != session);
    }

    fn frame(&mut self, session: &SessionRef, frame: Frame) {
        if self.locked {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        }
        let Some(target) = self.capture_target(&session.source()) else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };
        self.pending_captures.push(crate::capture::PendingCapture {
            frame,
            target,
            draw_cursor: session.draw_cursor(),
        });
        self.request_redraw();
    }
}

impl State {
    fn capture_target(&self, source: &ImageCaptureSource) -> Option<CaptureTarget> {
        if let Some(output) = source
            .user_data()
            .get::<WeakOutput>()
            .and_then(WeakOutput::upgrade)
        {
            let mode = output.current_mode()?;
            return Some(CaptureTarget::Output { size: mode.size });
        }
        let id = *source.user_data().get::<WindowId>()?;
        let rect = self.windows.usable_rect(id)?;
        let size = rect
            .size
            .to_f64()
            .to_physical(self.scale.fractional_scale())
            .to_i32_round();
        Some(CaptureTarget::Window { id, size })
    }

    pub(crate) fn activate_with_token(
        &mut self,
        token: XdgActivationToken,
        data: XdgActivationTokenData,
        surface: WlSurface,
        source: &str,
    ) {
        self.activation_state.remove_token(&token);
        if data.timestamp.elapsed() > ACTIVATION_TOKEN_MAX_AGE {
            tracing::info!("ignored an expired activation token");
            return;
        }
        let Some(id) = self.windows.id_of(&surface) else {
            tracing::info!("ignored activation: the surface is not a tracked toplevel");
            return;
        };
        tracing::info!(window = id.get(), "activating a window through {source}");
        self.activate_window(id);
    }
}

impl PointerConstraintsHandler for State {
    fn new_constraint(
        &mut self,
        surface: &WlSurface,
        pointer: &smithay::input::pointer::PointerHandle<Self>,
    ) {
        if pointer.current_focus().as_ref() != Some(surface) {
            return;
        }
        smithay::wayland::pointer_constraints::with_pointer_constraint(
            surface,
            pointer,
            |constraint| {
                if let Some(constraint) = constraint {
                    constraint.activate();
                }
            },
        );
    }
}

impl TabletSeatHandler for State {
    type ToolFocus = WlSurface;

    fn tablet_tool_image(&mut self, _tool: &TabletToolDescriptor, image: CursorImageStatus) {
        tracing::debug!(?image, "client set the pointer cursor");
        self.client_cursor = image.clone();
        self.cursor_status = image;
        self.request_redraw();
    }
}

impl XdgActivationHandler for State {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        self.activation_state
            .retain_tokens(|_, old| old.timestamp.elapsed() <= ACTIVATION_TOKEN_MAX_AGE);
        let Some(surface) = data.surface.as_ref() else {
            return false;
        };
        let Some(focus) = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus())
        else {
            return false;
        };
        if !surface.id().same_client_as(&focus.id()) {
            return false;
        }
        let Some((serial, _seat)) = data.serial else {
            return false;
        };
        let recent = self.activation_serial_is_recent(serial);
        if !recent {
            tracing::info!(
                serial = ?serial,
                "rejected activation token: the press is not the latest key/button"
            );
        }
        recent
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        self.activate_with_token(token, data, surface, "xdg-activation");
    }
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        tracing::debug!(?image, "client set the pointer cursor");
        self.client_cursor = image.clone();
        self.cursor_status = image;
        self.request_redraw();
    }
}

impl smithay::wayland::input_method::InputMethodHandler for State {
    fn new_popup(&mut self, surface: smithay::wayland::input_method::PopupSurface) {
        self.ime_popups.retain(|popup| popup.alive());
        self.ime_popups.push(surface);
        self.request_redraw();
    }

    fn popup_repositioned(&mut self, _surface: smithay::wayland::input_method::PopupSurface) {
        self.request_redraw();
    }

    fn dismiss_popup(&mut self, surface: smithay::wayland::input_method::PopupSurface) {
        self.ime_popups
            .retain(|popup| *popup != surface && popup.alive());
        self.request_redraw();
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        crate::window::window_geometry(parent).unwrap_or_default()
    }
}

impl smithay::wayland::text_input::TextInputActivation for State {}

impl SelectionHandler for State {
    type SelectionUserData = std::sync::Arc<[u8]>;

    fn send_selection(
        &mut self,
        _ty: SelectionTarget,
        mime_type: String,
        fd: std::os::fd::OwnedFd,
        _seat: Seat<Self>,
        user_data: &Self::SelectionUserData,
    ) {
        if mime_type != "image/png" {
            return;
        }
        let bytes = user_data.clone();
        let spawned = std::thread::Builder::new()
            .name("clipboard-send".into())
            .spawn(move || {
                if let Err(error) = write_before_deadline(fd, &bytes, CLIPBOARD_SEND_TIMEOUT) {
                    tracing::warn!(%error, "could not write the clipboard image");
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not start the clipboard writer");
        }
    }
}

const CLIPBOARD_SEND_TIMEOUT: Duration = Duration::from_secs(5);

fn write_before_deadline(
    fd: std::os::fd::OwnedFd,
    mut bytes: &[u8],
    timeout: Duration,
) -> std::io::Result<()> {
    use nix::fcntl::{FcntlArg, OFlag, fcntl};
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use std::io::Write;
    use std::os::fd::AsFd;
    let flags = OFlag::from_bits_retain(fcntl(&fd, FcntlArg::F_GETFL)?);
    fcntl(&fd, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    let deadline = Instant::now() + timeout;
    let mut file = std::fs::File::from(fd);
    while !bytes.is_empty() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let mut fds = [PollFd::new(file.as_fd(), PollFlags::POLLOUT)];
        let ready = poll(
            &mut fds,
            PollTimeout::try_from(remaining).unwrap_or(PollTimeout::MAX),
        )?;
        if ready == 0 {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        match file.write(bytes) {
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

impl DataDeviceHandler for State {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl WaylandDndGrabHandler for State {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        match type_ {
            GrabType::Pointer => {
                let Some(pointer) = seat.get_pointer() else {
                    source.cancel();
                    return;
                };
                let Some(start_data) = pointer.grab_start_data() else {
                    source.cancel();
                    return;
                };
                pointer.set_grab(
                    self,
                    DnDGrab::new_pointer(&self.display_handle, start_data, source, seat),
                    serial,
                    Focus::Keep,
                );
            }
            GrabType::Touch => {
                let Some(touch) = seat.get_touch() else {
                    source.cancel();
                    return;
                };
                let Some(start_data) = touch.grab_start_data() else {
                    source.cancel();
                    return;
                };
                touch.set_grab(
                    self,
                    DnDGrab::new_touch(&self.display_handle, start_data, source, seat),
                    serial,
                );
            }
        }
    }
}

impl DndGrabHandler for State {}

impl PrimarySelectionHandler for State {
    fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
        &mut self.primary_selection_state
    }
}

impl OutputHandler for State {}

impl ForeignToplevelListHandler for State {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.foreign.list
    }
}

delegate_foreign_management!(State);
delegate_appmenu!(State);

impl WlrLayerShellHandler for State {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
        layer: Layer,
        namespace: String,
    ) {
        tracing::info!(?layer, %namespace, surface = ?surface.wl_surface().id(), "new layer surface");
        if namespace == ctdesign::ALERT_LAYER_NAMESPACE {
            crate::window_style::mark_alert_layer(surface.wl_surface());
        }
        if namespace == ctdesign::NOTIFICATIONS_LAYER_NAMESPACE {
            crate::notification_panel::mark(surface.wl_surface());
        }
        if namespace == ctdesign::MENUBAR_LAYER_NAMESPACE {
            crate::lock_transition::mark(
                surface.wl_surface(),
                crate::lock_transition::Kept::Menubar,
            );
        }
        // ADR 0074: only the wallpaper of an unsandboxed client is drawn under the lock screen.
        if namespace == ctdesign::WALLPAPER_LAYER_NAMESPACE
            && layer == Layer::Background
            && surface
                .wl_surface()
                .client()
                .is_some_and(|client| crate::security::unrestricted(&client))
        {
            crate::lock_transition::mark(
                surface.wl_surface(),
                crate::lock_transition::Kept::Wallpaper,
            );
        }
        self.layers.add(surface);
        self.request_redraw();
    }

    fn layer_destroyed(&mut self, surface: LayerSurface) {
        tracing::info!(surface = ?surface.wl_surface().id(), "layer surface destroyed");
        if let Some(id) = crate::window_style::alert_layer(surface.wl_surface()) {
            self.chrome.remove(crate::chrome::FrameOwner::Alert(id));
        }
        if let Some(cards) = crate::window_style::cards(surface.wl_surface()) {
            self.chrome.remove_cards(cards.id);
        }
        self.layer_surface_destroyed(surface.wl_surface());
        self.layers.destroy(surface.wl_surface());
        self.request_redraw();
    }

    fn new_popup(&mut self, _parent: LayerSurface, popup: PopupSurface) {
        let positioner = popup.with_pending_state(|state| state.positioner);
        self.add_popup(popup, positioner);
    }
}

impl smithay::wayland::fractional_scale::FractionalScaleHandler for State {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.scale.fractional_scale();
        smithay::wayland::compositor::with_states(&surface, |states| {
            smithay::wayland::fractional_scale::with_fractional_scale(states, |state| {
                state.set_preferred_scale(scale);
            });
        });
    }
}

impl smithay::wayland::shell::xdg::dialog::XdgDialogHandler for State {}

impl smithay::wayland::background_effect::ExtBackgroundEffectHandler for State {
    fn capabilities(&self) -> smithay::wayland::background_effect::Capability {
        if crate::blur::drawn(self) {
            smithay::wayland::background_effect::Capability::Blur
        } else {
            smithay::wayland::background_effect::Capability::empty()
        }
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface().clone();
        self.windows.add_toplevel(surface);
        self.apply_initial_decoration(&wl_surface);
        self.request_redraw();
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        let requested = self.windows.requested_decoration(surface.wl_surface());
        self.apply_requested_decoration(surface.wl_surface(), requested);
        let id = self.windows.id_of(surface.wl_surface());
        if let Some(id) = id {
            self.foreign_identity_changed(id);
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self.windows.id_of(surface.wl_surface()) {
            self.foreign_identity_changed(id);
        }
    }

    fn ack_configure(&mut self, surface: WlSurface, configure: Configure) {
        self.windows.ack_configure(&surface, configure);
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        let Some(start) = self.held_grab(serial) else {
            tracing::info!(
                serial = ?serial,
                "rejected move request: no button press with this serial is held"
            );
            return;
        };
        if !start
            .focus
            .as_ref()
            .is_some_and(|(focus, _)| focus.id().same_client_as(&surface.wl_surface().id()))
        {
            tracing::info!(
                serial = ?serial,
                "rejected move request: the press was not on this client's surface"
            );
            return;
        }
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            tracing::info!(
                serial = ?serial,
                "rejected move request: the surface is not a tracked window"
            );
            return;
        };
        if let Some(location) = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
        {
            tracing::info!(serial = ?serial, "accepted move request");
            self.start_move(id, location);
            self.inject_test_drag(id);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let Some(start) = self.held_grab(serial) else {
            tracing::info!(
                serial = ?serial,
                "rejected resize request: no button press with this serial is held"
            );
            return;
        };
        if !start
            .focus
            .as_ref()
            .is_some_and(|(focus, _)| focus.id().same_client_as(&surface.wl_surface().id()))
        {
            tracing::info!(
                serial = ?serial,
                "rejected resize request: the press was not on this client's surface"
            );
            return;
        }
        let Some(edge) = crate::window::from_xdg_resize_edge(edges) else {
            return;
        };
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            tracing::info!(
                serial = ?serial,
                "rejected resize request: the surface is not a tracked window"
            );
            return;
        };
        if let Some(location) = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
        {
            tracing::info!(serial = ?serial, "accepted resize request");
            self.start_resize(id, edge, location);
            self.inject_test_release();
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.forget_window(surface.wl_surface());
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            return;
        };
        tracing::info!(window = id.get(), "maximize_request");
        if self.followed_double_click(id) {
            self.apply_titlebar_double_click(id);
        } else if self.in_focus_mode(id) {
            self.exit_focus_mode(id);
        } else if ctappearance::window::green_action(
            self.config.window.green_button,
            self.has_fixed_width(id),
        ) == ctappearance::window::TitlebarAction::Focus
        {
            self.enter_focus_mode(id, false);
        } else {
            self.set_maximized(id, true);
        }
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
    ) {
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            return;
        };
        tracing::info!(window = id.get(), "fullscreen_request");
        if self.in_focus_mode(id) {
            self.windows.set_focus_titlebar(id, false);
        } else {
            self.enter_focus_mode(id, true);
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            return;
        };
        tracing::info!(window = id.get(), "unfullscreen_request");
        self.exit_focus_mode(id);
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            return;
        };
        self.set_minimized(id, true);
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.windows.id_of(surface.wl_surface()) else {
            return;
        };
        tracing::info!(window = id.get(), "unmaximize_request");
        if self.in_focus_mode(id) {
            self.exit_focus_mode(id);
        } else {
            self.set_maximized(id, false);
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        self.add_popup(surface, positioner);
    }

    fn grab(&mut self, surface: PopupSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        let wl_surface = surface.wl_surface().clone();
        if let Some(parent) = surface.get_parent_surface()
            && self.popups.is_popup_surface(&parent)
        {
            let Some(chain_serial) = self.popups.grab_serial_of(&parent) else {
                tracing::info!(
                    popup = ?wl_surface.id(),
                    "rejected popup grab: the parent popup has no grab"
                );
                return;
            };
            let same_client = parent.id().same_client_as(&wl_surface.id());
            if !same_client {
                tracing::info!(
                    popup = ?wl_surface.id(),
                    "rejected popup grab: the parent and child are different clients"
                );
                return;
            }
            let last_delivered = self.client_last_input_serial(&wl_surface);
            if !child_grab_serial_ok(true, same_client, chain_serial, serial, last_delivered) {
                tracing::info!(
                    serial = ?serial,
                    chain = ?chain_serial,
                    "rejected popup grab: the serial was not delivered to this client"
                );
                return;
            }
            tracing::info!(
                popup = ?wl_surface.id(),
                parent = ?parent.id(),
                "popup grab accepted"
            );
            self.focus_popup_surface(&wl_surface, serial);
            return;
        }

        let Some(start) = self.held_grab(serial) else {
            tracing::info!(
                serial = ?serial,
                "rejected popup grab: no button press with this serial is held"
            );
            return;
        };
        if !start
            .focus
            .as_ref()
            .is_some_and(|(focus, _)| focus.id().same_client_as(&wl_surface.id()))
        {
            tracing::info!("rejected popup grab: the press was not on this client's surface");
            return;
        }
        tracing::info!(popup = ?wl_surface.id(), "popup grab accepted");
        self.focus_popup_surface(&wl_surface, serial);
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        self.popup_destroyed(surface.wl_surface());
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        self.reposition_popup(surface, positioner, token);
    }
}

fn child_grab_serial_ok(
    parent_grabbed: bool,
    same_client: bool,
    chain: Serial,
    serial: Serial,
    last_delivered: Option<Serial>,
) -> bool {
    parent_grabbed
        && same_client
        && (serial == chain
            || (serial.is_no_older_than(&chain)
                && last_delivered.is_some_and(|last| serial <= last)))
}

fn decoration_for_xdg_mode(mode: Mode) -> Decoration {
    match mode {
        Mode::ClientSide => Decoration::Client,
        _ => Decoration::Server,
    }
}

fn decoration_for_kde_mode(mode: KdeMode) -> Decoration {
    match mode {
        KdeMode::Server => Decoration::Server,
        _ => Decoration::Client,
    }
}

impl State {
    fn apply_requested_decoration(&mut self, surface: &WlSurface, requested: Option<Decoration>) {
        if let Some(requested) = requested {
            self.windows.set_requested_decoration(surface, requested);
        }
        let effective = self.effective_decoration(surface, requested);
        self.windows.set_decoration(surface, effective);
    }

    fn apply_initial_decoration(&mut self, surface: &WlSurface) {
        let requested = self.windows.requested_decoration(surface);
        let effective = self.effective_decoration(surface, requested);
        self.windows.set_initial_decoration(surface, effective);
        self.windows.send_initial_configure(surface);
    }

    fn effective_decoration(
        &mut self,
        surface: &WlSurface,
        requested: Option<Decoration>,
    ) -> Decoration {
        let native = crate::window_style::is_kit_client(surface);
        if native && requested.is_none() && self.windows.mark_native_warning(surface) {
            tracing::warn!(
                app_id = %self.windows.app_id(surface),
                "native app without a decoration mode"
            );
        }
        crate::decoration::effective_decoration(self.config.window.decorations, requested, native)
    }
}

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.windows
            .set_requested_decoration(toplevel.wl_surface(), Decoration::Server);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        self.apply_requested_decoration(toplevel.wl_surface(), Some(decoration_for_xdg_mode(mode)));
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.apply_requested_decoration(toplevel.wl_surface(), Some(Decoration::Server));
    }
}

impl KdeDecorationHandler for State {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.kde_decoration_state
    }

    fn new_decoration(&mut self, surface: &WlSurface, decoration: &OrgKdeKwinServerDecoration) {
        decoration.mode(KdeMode::Server);
        self.apply_requested_decoration(surface, Some(Decoration::Server));
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        mode: WEnum<KdeMode>,
    ) {
        let WEnum::Value(mode) = mode else {
            return;
        };
        decoration.mode(mode);
        self.apply_requested_decoration(surface, Some(decoration_for_kde_mode(mode)));
    }

    fn release(&mut self, _decoration: &OrgKdeKwinServerDecoration, surface: &WlSurface) {
        self.apply_requested_decoration(surface, Some(Decoration::Server));
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod capture_lock_tests;

delegate_gtk!(State);

delegate_dispatch2!(State);
