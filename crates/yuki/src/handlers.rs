//! Protocol handler implementations and their delegation glue.
//!
//! Smithay's `delegate_dispatch2!` macro wires a state type to the protocol
//! objects it serves, but it only works if the matching `*Handler` trait is
//! implemented. The impls live here, next to the macro, so `state.rs` stays a
//! plain data description and every handler is in one place.
//!
//! The globals split across two traits each: the handler trait is the
//! compositor's behaviour, the macro is the plumbing that turns client
//! requests into calls on that behaviour.

use std::time::Duration;

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
use smithay::utils::Serial;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    Blocker, BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
    SurfaceAttributes, add_blocker, add_pre_commit_hook, with_states,
};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};
use smithay::wayland::pointer_constraints::PointerConstraintsHandler;
use smithay::wayland::selection::SelectionHandler;
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

use yuki_scene::scene::Decoration;

use crate::config::DecorationPolicy;
use crate::delegate_foreign_management;
use crate::state::{ClientState, State};

/// How long a client-created activation token stays usable.
const ACTIVATION_TOKEN_MAX_AGE: Duration = Duration::from_secs(30);

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // The invariant: session.rs inserts every accepted client with
        // ClientState, so a client being dispatched cannot lack it. If this
        // fires, look at the socket handler, not at this call site.
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted with ClientState in the socket handler")
            .compositor_state
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        // A commit that attaches a dmabuf is held until the buffer is ready to
        // read (Bloque 7b). One blocker path serves both syncs: the explicit
        // acquire point when the client set one, otherwise the implicit fence.
        // The resume happens from the event loop, so the compositor never
        // blocks on a client; a frame is never shown half-rendered.
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
                // Explicit sync wins when present; the acquire point names
                // exactly when the client finished writing. Otherwise the
                // dmabuf's own implicit fence.
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
        // At `trace` level, name the buffer type each committed surface uses.
        // This is how a GPU client (wezterm, Firefox) is verified from the
        // compositor side: `--log-level trace` and look for `Dma` against
        // `Shm` (Bloque 7a).
        log_committed_buffer_type(surface);
        // Let Smithay move the pending buffer into the surface's current state
        // first; the window tracker then reads that state to decide on mapping.
        on_commit_buffer_handler::<Self>(surface);
        // A layer surface gets configured and placed here too (ADR 0023).
        self.layers.commit(surface, self.logical_output_size());
        if let Some(mapped) = self.windows.commit(surface) {
            self.focus_window(mapped);
            self.foreign_window_mapped(mapped);
            self.inject_test_shake();
            self.inject_test_press(mapped);
        }
        self.popup_commit(surface);
        self.request_redraw();
    }
}

/// Which commit blocker a pending dmabuf needs, if any (Bloque 7b).
///
/// This is the single decision that both syncs share: a commit only blocks
/// when it attaches a dmabuf, and the explicit acquire point wins over the
/// implicit fence when both are present. (An acquire point without a buffer is
/// a protocol error smithay already rejects.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockerKind {
    /// Wait on the client's explicit acquire point (`wp_linux_drm_syncobj`).
    Explicit,
    /// Wait on the dmabuf's own implicit fence.
    Implicit,
}

/// The blocker kind for a commit with `has_dmabuf` and `has_acquire_point`.
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

/// Inserts a blocker's event source into the loop and, on success, blocks the
/// surface's commit until the source fires.
///
/// The source is one-shot: when the dmabuf (implicit) or the acquire point
/// (explicit) becomes readable, the callback resumes the commit with
/// `CompositorClientState::blocker_cleared`. The callback only wakes the main
/// loop through a channel, so the blocker loop does not depend on the backend.
///
/// Returns whether the source was inserted; a failed insertion (e.g. the loop
/// is shutting down) leaves the commit unblocked rather than forever blocked.
fn insert_blocker_source(
    state: &mut State,
    surface: &WlSurface,
    client: Client,
    source: crate::blockers::BlockerSource,
    blocker: impl Blocker + Send + 'static,
) -> bool {
    let inserted = state.blocker_sink.insert(source, client);
    if inserted {
        add_blocker(surface, blocker);
    }
    inserted
}

/// Logs the buffer type of the surface's just-committed buffer, at `trace`.
///
/// The pending buffer is read *before* `on_commit_buffer_handler` consumes it.
/// Only buffers that were actually attached (a new buffer assignment) produce
/// a line, so a `--log-level trace` run names which clients present through
/// dmabuf and which through shm. It is the compositor-side observability of
/// Bloque 7a; the path coverage itself is verified with WAYLAND_DEBUG per
/// client (Bloque 7b).
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
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {
        // Buffers are not retained yet; nothing to release.
    }
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
        // The import into the GLES renderer happens in the session, which owns
        // the renderer (ADR 0012); this handler only queues the buffer. The
        // session drains the queue right after dispatching clients and answers
        // the notifier there (see `session::dispatch_clients`).
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

impl PointerConstraintsHandler for State {}

impl TabletSeatHandler for State {
    type ToolFocus = WlSurface;

    fn tablet_tool_image(&mut self, _tool: &TabletToolDescriptor, image: CursorImageStatus) {
        // Tablets share the compositor's single pointer image today. The
        // request is already serial-checked by Smithay before this point.
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
        // A token is only useful if it was created by a client that actually
        // has keyboard focus, and it names the latest key/button press. This
        // is the compositor's focus-stealing policy: without both, an app
        // could activate itself from the background.
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
        self.activation_state.remove_token(&token);
        if data.timestamp.elapsed() > ACTIVATION_TOKEN_MAX_AGE {
            tracing::info!("ignored an expired activation token");
            return;
        }
        let Some(id) = self.windows.id_of(&surface) else {
            tracing::info!("ignored activation: the surface is not a tracked toplevel");
            return;
        };
        tracing::info!(
            window = id.get(),
            "activating a window through xdg-activation"
        );
        self.activate_window(id);
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
        // Smithay only reaches here for a `wl_pointer.set_cursor` whose serial
        // matches the pointer's last enter serial, which this compositor
        // generated from its single serial counter (see `cursor.rs`). No second
        // counter is involved. Store the image; the draw path renders it.
        tracing::debug!(?image, "client set the pointer cursor");
        self.client_cursor = image.clone();
        self.cursor_status = image;
        self.request_redraw();
    }
}

impl SelectionHandler for State {
    type SelectionUserData = ();
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
        // The surface is configured on its first commit, when its anchors and
        // size are known (see `Layers::commit`).
        self.layers.add(surface);
        self.request_redraw();
    }

    fn layer_destroyed(&mut self, surface: LayerSurface) {
        tracing::info!(surface = ?surface.wl_surface().id(), "layer surface destroyed");
        self.layers.destroy(surface.wl_surface());
        self.request_redraw();
    }
}

impl smithay::wayland::fractional_scale::FractionalScaleHandler for State {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        // Advertise the output's fractional scale. The shell renders its
        // buffers at this scale and lets the viewport land them on the logical
        // size; clients without fractional-scale support still get the
        // integer value in `wl_output`.
        let scale = self.scale.fractional_scale();
        smithay::wayland::compositor::with_states(&surface, |states| {
            smithay::wayland::fractional_scale::with_fractional_scale(states, |state| {
                state.set_preferred_scale(scale);
            });
        });
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // Send the initial configure now, before the client attaches a buffer:
        // a toplevel that never receives one waits forever, with no error.
        // The window is sized and placed within the workspace (below the menu
        // bar, above the future dock).
        let (origin, size) = self.workspace();
        self.windows.add_toplevel(surface, size, origin);
        self.request_redraw();
    }

    fn ack_configure(&mut self, surface: WlSurface, configure: Configure) {
        self.windows.ack_configure(&surface, configure);
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        // The serial must name the pointer's implicit grab: a button press
        // that is still held, made on this client's surface. A request with
        // any other serial is ignored rather than starting a drag.
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
            // No drag motion for a resize: the probe asserts the request was
            // accepted and the resizing configure, not a new size.
            self.inject_test_release();
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.forget_window(surface.wl_surface());
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        // Constraint adjustment, a configure before mapping, and tracking all
        // happen in `popup.rs` (Bloque 5).
        self.add_popup(surface, positioner);
    }

    fn grab(&mut self, surface: PopupSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        // An explicit grab means the popup must be dismissed when the pointer
        // is pressed outside it. The serial must name the pointer's implicit
        // grab (a button press still held) made on this client's surface; a
        // stale serial is rejected.
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
            .is_some_and(|(focus, _)| focus.id().same_client_as(&surface.wl_surface().id()))
        {
            tracing::info!("rejected popup grab: the press was not on this client's surface");
            return;
        }
        tracing::info!(popup = ?surface.wl_surface().id(), "popup grab accepted");
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

/// The decoration a client asks for with an xdg-decoration mode.
///
/// The protocol has no "unset" value here: `request_mode` carries one of the
/// two, and the compositor answers with the one it chose.
fn decoration_for_xdg_mode(mode: Mode) -> Decoration {
    match mode {
        Mode::ClientSide => Decoration::Client,
        // Server-side, and any value this build does not know: the compositor
        // chooses, and it chooses server-side.
        _ => Decoration::Server,
    }
}

/// The decoration a client asks for with a KDE decoration mode.
///
/// `None` means "do not decorate me at all", which for Yuki is the same as
/// self-decorating: no chrome. `Server` and `Client` map directly.
fn decoration_for_kde_mode(mode: KdeMode) -> Decoration {
    match mode {
        KdeMode::Server => Decoration::Server,
        // Client-side, undecorated, and any unknown value: no compositor
        // chrome, so an odd client cannot end up with a second titlebar.
        _ => Decoration::Client,
    }
}

/// Applies the configured decoration policy (ADR 0021) to a requested mode.
///
/// `Default` honours the client negotiation (ADR 0019/0020); the two forcing
/// values override it for theming experiments. `ForceServer` draws Yuki's
/// chrome on every window, even a GTK headerbar; `ForceClient` draws none.
fn apply_decoration_policy(policy: DecorationPolicy, requested: Decoration) -> Decoration {
    match policy {
        DecorationPolicy::ForceServer => Decoration::Server,
        DecorationPolicy::ForceClient => Decoration::Client,
        DecorationPolicy::Default => requested,
    }
}

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        // The client created the decoration object but has not chosen a mode;
        // the protocol leaves the choice to the compositor, which chooses
        // server-side. The configure goes out before the client attaches, so it
        // never draws a titlebar of its own.
        self.windows.set_decoration(
            toplevel.wl_surface(),
            apply_decoration_policy(self.config.window.decorations, Decoration::Server),
        );
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        // An explicit choice is honoured: a client that asks for client-side
        // chrome keeps it, and one that accepts server-side gets Yuki's.
        self.windows.set_decoration(
            toplevel.wl_surface(),
            apply_decoration_policy(
                self.config.window.decorations,
                decoration_for_xdg_mode(mode),
            ),
        );
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        // The client withdrew its choice, so the compositor chooses again:
        // server-side, as for a fresh decoration object.
        self.windows.set_decoration(
            toplevel.wl_surface(),
            apply_decoration_policy(self.config.window.decorations, Decoration::Server),
        );
    }
}

impl KdeDecorationHandler for State {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.kde_decoration_state
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
        // GTK3 and GTK4 ask through this protocol; an explicit client-side
        // request means the window has its own headerbar and must keep it. The
        // reply repeats the client's mode, because that is the mode chosen.
        decoration.mode(mode);
        self.windows.set_decoration(
            surface,
            apply_decoration_policy(
                self.config.window.decorations,
                decoration_for_kde_mode(mode),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
    use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::Mode as KdeMode;

    use super::{
        BlockerKind, apply_decoration_policy, decoration_for_kde_mode, decoration_for_xdg_mode,
        pending_blocker,
    };
    use crate::config::DecorationPolicy;
    use yuki_scene::scene::Decoration;

    #[test]
    fn an_xdg_client_that_accepts_server_side_gets_server_chrome() {
        assert_eq!(
            decoration_for_xdg_mode(Mode::ServerSide),
            Decoration::Server
        );
    }

    #[test]
    fn an_xdg_client_that_requests_client_side_keeps_its_own_chrome() {
        assert_eq!(
            decoration_for_xdg_mode(Mode::ClientSide),
            Decoration::Client
        );
    }

    #[test]
    fn a_kde_client_that_requests_server_side_gets_server_chrome() {
        assert_eq!(decoration_for_kde_mode(KdeMode::Server), Decoration::Server);
    }

    #[test]
    fn a_kde_client_that_requests_client_side_keeps_its_own_chrome() {
        assert_eq!(decoration_for_kde_mode(KdeMode::Client), Decoration::Client);
    }

    #[test]
    fn a_kde_client_that_asks_for_no_decoration_gets_none_from_yuki() {
        assert_eq!(decoration_for_kde_mode(KdeMode::None), Decoration::Client);
    }

    #[test]
    fn the_default_policy_honours_the_client_request() {
        assert_eq!(
            apply_decoration_policy(DecorationPolicy::Default, Decoration::Server),
            Decoration::Server
        );
        assert_eq!(
            apply_decoration_policy(DecorationPolicy::Default, Decoration::Client),
            Decoration::Client
        );
    }

    #[test]
    fn forcing_server_gives_chrome_to_even_a_client_side_request() {
        assert_eq!(
            apply_decoration_policy(DecorationPolicy::ForceServer, Decoration::Client),
            Decoration::Server
        );
    }

    #[test]
    fn forcing_client_draws_no_yuki_chrome_at_all() {
        assert_eq!(
            apply_decoration_policy(DecorationPolicy::ForceClient, Decoration::Server),
            Decoration::Client
        );
    }

    #[test]
    fn a_commit_without_a_dmabuf_never_blocks() {
        assert_eq!(pending_blocker(false, false), None, "no dmabuf, no blocker");
        assert_eq!(
            pending_blocker(false, true),
            None,
            "an acquire point without a buffer is a protocol error, not a blocker"
        );
    }

    #[test]
    fn explicit_sync_wins_over_the_implicit_fence() {
        assert_eq!(pending_blocker(true, true), Some(BlockerKind::Explicit));
        assert_eq!(pending_blocker(true, false), Some(BlockerKind::Implicit));
    }
}

delegate_dispatch2!(State);
