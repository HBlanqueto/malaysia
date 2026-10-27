//! `gtk_shell1`: GTK's private Wayland shell extension.
//!
//! This module only stores the D-Bus properties GTK 3/4 announces via
//! `gtk_surface1.set_dbus_properties` and forwards them to the shell through
//! Infinity v3. It never talks D-Bus and never draws a menu (ADR 0045).
//!
//! The protocol file is `protocols/gtk-shell.xml`, copied unmodified from GTK
//! commit efcc16deb0e0373655e2e4aed139ff7de3096134. MalaysiaDreams advertises version 5.

use malaysia_scene::scene::WindowId;
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId, ObjectId};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::wayland::xdg_activation::XdgActivationToken;
use tracing::{debug, info, warn};

use crate::appmenu::AddressStore;
use crate::state::State;

/// Generated server-side bindings for the GTK shell protocol.
pub(crate) mod protocol {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
    #![allow(missing_docs, clippy::all)]

    use wayland_server;
    use wayland_server::protocol::*;

    pub mod __interfaces {
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("../../protocols/gtk-shell.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("../../protocols/gtk-shell.xml");
}

use protocol::gtk_shell1::GtkShell1;
use protocol::gtk_surface1::GtkSurface1;

/// The six D-Bus properties GTK sets on a surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GtkDbusProperties {
    pub application_id: String,
    pub app_menu_path: String,
    pub menubar_path: String,
    pub window_object_path: String,
    pub application_object_path: String,
    pub unique_bus_name: String,
}

impl GtkDbusProperties {
    fn is_empty(&self) -> bool {
        self.application_id.is_empty()
            && self.app_menu_path.is_empty()
            && self.menubar_path.is_empty()
            && self.window_object_path.is_empty()
            && self.application_object_path.is_empty()
            && self.unique_bus_name.is_empty()
    }
}

/// Per-surface GTK state: the D-Bus properties and the modal flag.
#[derive(Debug, Clone, Default)]
struct GtkSurfaceState {
    /// Which `gtk_surface1` object owns this entry, so a second object for the
    /// same surface can be detected and the newest wins. `None` only before the
    /// first `set_dbus_properties`.
    owner: Option<ObjectId>,
    modal: bool,
    props: GtkDbusProperties,
}

/// The `gtk_shell1` global and the live per-surface state.
pub struct GtkShellState {
    _global: GlobalId,
    surfaces: AddressStore<ObjectId, GtkSurfaceState>,
}

impl GtkShellState {
    /// Creates the global, visible to every client (GTK needs it).
    pub fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<State, GtkShell1, ()>(5, ()),
            surfaces: AddressStore::default(),
        }
    }

    fn set_props(&mut self, surface: &WlSurface, owner: ObjectId, props: GtkDbusProperties) {
        if !surface.is_alive() {
            return;
        }
        if let Some(existing) = self.surfaces.get(&surface.id())
            && existing.owner != Some(owner.clone())
        {
            warn!(surface = ?surface.id(), "gtk_surface1: second object for surface, newest wins");
        }
        self.surfaces.set(
            surface.id(),
            GtkSurfaceState {
                owner: Some(owner),
                modal: false,
                props,
            },
        );
    }

    fn set_modal(&mut self, surface: &WlSurface, owner: ObjectId, modal: bool) {
        let key = surface.id();
        if let Some(state) = self.surfaces.get_mut(&key)
            && state.owner == Some(owner)
        {
            state.modal = modal;
        }
    }

    fn clear(&mut self, surface: &WlSurface, owner: &ObjectId) -> bool {
        if let Some(state) = self.surfaces.get(&surface.id())
            && state.owner != Some(owner.clone())
        {
            return false;
        }
        self.surfaces.clear(&surface.id())
    }

    fn clear_surface(&mut self, surface: &WlSurface) -> bool {
        self.surfaces.clear(&surface.id())
    }

    fn props(&self, surface: &WlSurface) -> Option<&GtkDbusProperties> {
        self.surfaces.get(&surface.id()).map(|state| &state.props)
    }
}

impl GlobalDispatch<GtkShell1, (), State> for GtkShellState {
    fn bind(
        _state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<GtkShell1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        let shell = data_init.init(resource, ());
        // 14f-1 sends 0; 14f-2 will send 3 (global_app_menu | global_menu_bar).
        shell.capabilities(0);
    }
}

impl Dispatch<GtkShell1, (), State> for GtkShellState {
    fn request(
        _state: &mut State,
        _client: &Client,
        _resource: &GtkShell1,
        request: protocol::gtk_shell1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            protocol::gtk_shell1::Request::GetGtkSurface {
                gtk_surface,
                surface,
            } => {
                data_init.init(gtk_surface, surface);
            }
            protocol::gtk_shell1::Request::SetStartupId { startup_id } => {
                debug!(?startup_id, "gtk_shell set_startup_id");
            }
            protocol::gtk_shell1::Request::SystemBell { surface } => {
                debug!(surface = ?surface.map(|s| s.id()), "gtk_shell system_bell");
            }
            protocol::gtk_shell1::Request::NotifyLaunch { startup_id } => {
                debug!(%startup_id, "gtk_shell notify_launch");
            }
        }
    }
}

/// Decodes a `titlebar_gesture` value into the action the compositor performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gesture {
    Double,
    Right,
    Middle,
}

fn decode_gesture(value: u32) -> Option<Gesture> {
    match value {
        1 => Some(Gesture::Double),
        2 => Some(Gesture::Right),
        3 => Some(Gesture::Middle),
        _ => None,
    }
}

impl Dispatch<GtkSurface1, WlSurface, State> for GtkShellState {
    fn request(
        state: &mut State,
        _client: &Client,
        resource: &GtkSurface1,
        request: protocol::gtk_surface1::Request,
        surface: &WlSurface,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        let owner = resource.id();
        match request {
            protocol::gtk_surface1::Request::SetDbusProperties {
                application_id,
                app_menu_path,
                menubar_path,
                window_object_path,
                application_object_path,
                unique_bus_name,
            } => {
                let props = GtkDbusProperties {
                    application_id: application_id.unwrap_or_default(),
                    app_menu_path: app_menu_path.unwrap_or_default(),
                    menubar_path: menubar_path.unwrap_or_default(),
                    window_object_path: window_object_path.unwrap_or_default(),
                    application_object_path: application_object_path.unwrap_or_default(),
                    unique_bus_name: unique_bus_name.unwrap_or_default(),
                };
                if props.is_empty() {
                    state.clear_surface_gtk_menus(surface);
                } else {
                    state.set_surface_gtk_menus(surface, owner, props);
                }
            }
            protocol::gtk_surface1::Request::SetModal => {
                state.gtk.set_modal(surface, owner, true);
                if let Some(id) = state.windows.id_of(surface) {
                    info!(window = id.get(), "gtk_surface set_modal");
                }
            }
            protocol::gtk_surface1::Request::UnsetModal => {
                state.gtk.set_modal(surface, owner, false);
                if let Some(id) = state.windows.id_of(surface) {
                    info!(window = id.get(), "gtk_surface unset_modal");
                }
            }
            protocol::gtk_surface1::Request::Present { time } => {
                debug!(time, "gtk_surface present");
                state.handle_gtk_present(surface);
            }
            protocol::gtk_surface1::Request::RequestFocus { startup_id } => {
                debug!(?startup_id, "gtk_surface request_focus");
                state.handle_gtk_request_focus(surface, startup_id);
            }
            protocol::gtk_surface1::Request::Release => {
                state.clear_surface_gtk_menus_by_owner(surface, &owner);
            }
            protocol::gtk_surface1::Request::TitlebarGesture {
                serial: _,
                seat: _,
                gesture,
            } => {
                let Some(id) = state.windows.id_of(surface) else {
                    return;
                };
                let gesture: u32 = gesture.into();
                let Some(gesture) = decode_gesture(gesture) else {
                    resource.post_error(
                        protocol::gtk_surface1::Error::InvalidGesture,
                        "invalid titlebar gesture",
                    );
                    return;
                };
                match gesture {
                    Gesture::Double => {
                        info!(window = id.get(), "gtk_surface titlebar double_click");
                        state.apply_titlebar_double_click(id);
                    }
                    Gesture::Right => {
                        // No compositor window menu yet; log only.
                        info!(window = id.get(), "gtk_surface titlebar right_click");
                    }
                    Gesture::Middle => {
                        info!(window = id.get(), "gtk_surface titlebar middle_click");
                    }
                }
            }
            protocol::gtk_surface1::Request::SetA11yProperties {
                a11y_dbus_name,
                toplevel_object_path,
            } => {
                debug!(%a11y_dbus_name, %toplevel_object_path, "gtk_surface set_a11y_properties");
            }
        }
    }

    fn destroyed(
        state: &mut State,
        _client: ClientId,
        resource: &GtkSurface1,
        surface: &WlSurface,
    ) {
        state.clear_surface_gtk_menus_by_owner(surface, &resource.id());
    }
}

impl State {
    /// Stores the GTK D-Bus properties announced for `surface` and publishes
    /// them if the surface already belongs to a mapped window.
    pub(crate) fn set_surface_gtk_menus(
        &mut self,
        surface: &WlSurface,
        owner: ObjectId,
        props: GtkDbusProperties,
    ) {
        let window = self.windows.id_of(surface);
        self.gtk.set_props(surface, owner, props.clone());
        if let Some(id) = window {
            info!(
                window = id.get(),
                bus = %props.unique_bus_name,
                menubar = %props.menubar_path,
                appmenu = %props.app_menu_path,
                "gtk_menus set"
            );
            self.send_gtk_menus(id);
        } else {
            debug!(
                surface = ?surface.id(),
                bus = %props.unique_bus_name,
                "gtk_menus set pending map"
            );
        }
    }

    /// Drops the GTK properties for `surface`; a mapped window gets an empty
    /// gtk_menus event.
    pub(crate) fn clear_surface_gtk_menus(&mut self, surface: &WlSurface) {
        if !self.gtk.clear_surface(surface) {
            return;
        }
        match self.windows.id_of(surface) {
            Some(id) => {
                info!(window = id.get(), "gtk_menus cleared");
                self.send_gtk_menus(id);
            }
            None => debug!(surface = ?surface.id(), "gtk_menus cleared pending map"),
        }
    }

    /// Drops the GTK properties only if `owner` matches the current surface
    /// owner, ignoring stale clears from a replaced `gtk_surface1`.
    pub(crate) fn clear_surface_gtk_menus_by_owner(
        &mut self,
        surface: &WlSurface,
        owner: &ObjectId,
    ) {
        if !self.gtk.clear(surface, owner) {
            return;
        }
        match self.windows.id_of(surface) {
            Some(id) => {
                info!(window = id.get(), "gtk_menus cleared by owner");
                self.send_gtk_menus(id);
            }
            None => debug!(surface = ?surface.id(), "gtk_menus cleared by owner pending map"),
        }
    }

    /// Handles GTK's `gtk_surface1.present` request, activating the surface only
    /// when its client already holds keyboard focus.
    pub(crate) fn handle_gtk_present(&mut self, surface: &WlSurface) {
        let focused = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus());
        let from_focused_client = focused
            .as_ref()
            .is_some_and(|focus| focus.id().same_client_as(&surface.id()));
        if from_focused_client {
            if let Some(id) = self.windows.id_of(surface) {
                self.activate_window(id);
            }
        } else {
            info!("ignored gtk present from a client that does not hold keyboard focus");
            // shortcut: urgency/attention is not signalled; improve by adding an
            // urgency hint or a visual attention request when the policy is defined.
        }
    }

    /// Handles GTK's `gtk_surface1.request_focus` request, activating the surface
    /// only when `startup_id` names a valid xdg-activation token.
    pub(crate) fn handle_gtk_request_focus(
        &mut self,
        surface: &WlSurface,
        startup_id: Option<String>,
    ) {
        let Some(startup_id) = startup_id else {
            info!("ignored gtk request_focus without a valid activation token");
            return;
        };
        let token = XdgActivationToken::from(startup_id);
        let Some(data) = self.activation_state.data_for_token(&token).cloned() else {
            info!("ignored gtk request_focus without a valid activation token");
            return;
        };
        self.activate_with_token(token, data, surface.clone(), "gtk-request-focus");
    }

    /// The stored GTK menu address for a tracked window, if any.
    pub(crate) fn gtk_menus_address(&self, id: WindowId) -> Option<GtkMenusAddress> {
        let surface = self.windows.surface_of(id)?;
        let props = self.gtk.props(surface)?;
        if props.unique_bus_name.is_empty() || props.menubar_path.is_empty() {
            return None;
        }
        Some(GtkMenusAddress {
            unique_bus_name: props.unique_bus_name.clone(),
            application_id: props.application_id.clone(),
            menubar_path: props.menubar_path.clone(),
            app_menu_path: props.app_menu_path.clone(),
            window_object_path: props.window_object_path.clone(),
            application_object_path: props.application_object_path.clone(),
        })
    }
}

/// Where a window's GTK `org.gtk.Menus` lives on D-Bus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GtkMenusAddress {
    pub unique_bus_name: String,
    pub application_id: String,
    pub menubar_path: String,
    pub app_menu_path: String,
    pub window_object_path: String,
    pub application_object_path: String,
}

/// Delegates the gtk_shell1 interfaces to [`GtkShellState`].
#[macro_export]
macro_rules! delegate_gtk {
    ($ty:ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($ty: [
            $crate::gtk::protocol::gtk_shell1::GtkShell1: ()
        ] => $crate::gtk::GtkShellState);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            $crate::gtk::protocol::gtk_shell1::GtkShell1: ()
        ] => $crate::gtk::GtkShellState);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            $crate::gtk::protocol::gtk_surface1::GtkSurface1:
                smithay::reexports::wayland_server::protocol::wl_surface::WlSurface
        ] => $crate::gtk::GtkShellState);
    };
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use sctk::reexports::client::globals::{GlobalListContents, registry_queue_init};
    use sctk::reexports::client::protocol::{wl_compositor, wl_registry, wl_surface};
    use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
    use sctk::reexports::protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
    use smithay::backend::allocator::{Format, Fourcc, Modifier};
    use smithay::reexports::calloop::EventLoop;
    use smithay::reexports::wayland_server::{Display, Resource};
    use smithay::utils::{SERIAL_COUNTER, Size};
    use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
    use smithay::wayland::xdg_activation::XdgActivationTokenData;

    use super::{Gesture, GtkDbusProperties, decode_gesture};

    struct TestClient;

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TestClient {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<wl_compositor::WlCompositor, ()> for TestClient {
        fn event(
            _: &mut Self,
            _: &wl_compositor::WlCompositor,
            _: wl_compositor::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<xdg_wm_base::XdgWmBase, ()> for TestClient {
        fn event(
            _: &mut Self,
            wm_base: &xdg_wm_base::XdgWmBase,
            event: xdg_wm_base::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let xdg_wm_base::Event::Ping { serial } = event {
                wm_base.pong(serial);
            }
        }
    }

    impl Dispatch<xdg_surface::XdgSurface, ()> for TestClient {
        fn event(
            _: &mut Self,
            surface: &xdg_surface::XdgSurface,
            event: xdg_surface::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let xdg_surface::Event::Configure { serial } = event {
                surface.ack_configure(serial);
            }
        }
    }

    impl Dispatch<xdg_toplevel::XdgToplevel, ()> for TestClient {
        fn event(
            _: &mut Self,
            _: &xdg_toplevel::XdgToplevel,
            _: xdg_toplevel::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<wl_surface::WlSurface, ()> for TestClient {
        fn event(
            _: &mut Self,
            _: &wl_surface::WlSurface,
            _: wl_surface::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    fn props(bus: &str, menubar: &str, appmenu: &str) -> GtkDbusProperties {
        GtkDbusProperties {
            unique_bus_name: bus.to_string(),
            menubar_path: menubar.to_string(),
            app_menu_path: appmenu.to_string(),
            ..Default::default()
        }
    }

    fn test_state() -> (
        EventLoop<'static, crate::session::Session>,
        Display<super::State>,
        super::State,
    ) {
        let event_loop = EventLoop::<crate::session::Session>::try_new().expect("event loop");
        let display = Display::<super::State>::new().expect("display");
        let feedback = DmabufFeedbackBuilder::new(
            0,
            [Format {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::Invalid,
            }],
        )
        .build()
        .expect("feedback");
        let state = super::State::new(
            event_loop.get_signal(),
            &display.handle(),
            Size::from((1280, 720)),
            feedback,
            None,
            Box::new(event_loop.handle()),
            Box::new(()),
        )
        .expect("state");
        (event_loop, display, state)
    }

    fn insert_test_client(display: &Display<super::State>, server_side: UnixStream) {
        let disconnect_queue = crate::state::ClientDisconnectQueue::default();
        let (disconnect_ping, _) = smithay::reexports::calloop::ping::make_ping().expect("ping");
        display
            .handle()
            .insert_client(
                server_side,
                Arc::new(crate::state::ClientState::new(
                    disconnect_queue,
                    disconnect_ping,
                    false,
                )),
            )
            .expect("insert client");
    }

    /// Creates a client with one xdg toplevel and returns its wl_surface once
    /// the compositor has tracked it, plus a channel to shut the client thread
    /// down cleanly.
    fn create_toplevel(
        display: &mut Display<super::State>,
        state: &mut super::State,
    ) -> (
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        mpsc::Sender<()>,
    ) {
        let before: Vec<_> = state.windows.surfaces().map(|s| s.id()).collect();
        let (client_side, server_side) = UnixStream::pair().expect("socket pair");
        insert_test_client(display, server_side);
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        thread::spawn(move || {
            client_side
                .set_read_timeout(Some(Duration::from_millis(100)))
                .expect("read timeout");
            let conn = Connection::from_socket(client_side).expect("connection");
            let (globals, mut queue) = registry_queue_init::<TestClient>(&conn).expect("registry");
            let qh = queue.handle();
            let compositor = globals
                .bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=5, ())
                .expect("compositor");
            let wm_base = globals
                .bind::<xdg_wm_base::XdgWmBase, _, _>(&qh, 1..=6, ())
                .expect("xdg_wm_base");
            let surface = compositor.create_surface(&qh, ());
            let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
            let _toplevel = xdg_surface.get_toplevel(&qh, ());
            surface.commit();
            conn.flush().expect("flush");
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    return;
                }
                let _ = queue.blocking_dispatch(&mut TestClient);
            }
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            display.dispatch_clients(state).expect("dispatch");
            display.flush_clients().expect("flush");
            if let Some(surface) = state
                .windows
                .surfaces()
                .find(|s| !before.iter().any(|b| b == &s.id()))
            {
                return (surface.clone(), shutdown_tx);
            }
            assert!(Instant::now() < deadline, "toplevel was not created");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn make_token(
        state: &mut super::State,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        age: Duration,
    ) -> String {
        let (token, _) = state
            .activation_state
            .create_external_token(XdgActivationTokenData {
                surface: Some(surface.clone()),
                timestamp: Instant::now() - age,
                ..Default::default()
            });
        token.to_string()
    }

    fn set_keyboard_focus(
        state: &mut super::State,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = state.seat.get_keyboard() {
            keyboard.set_focus(state, Some(surface.clone()), serial);
        }
    }

    fn window_id(
        state: &super::State,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) -> malaysia_scene::scene::WindowId {
        state.windows.id_of(surface).expect("tracked toplevel")
    }

    #[test]
    fn empty_props_clear_the_entry() {
        assert!(GtkDbusProperties::default().is_empty());
        let mut p = props(":1.2", "/menubar", "/appmenu");
        assert!(!p.is_empty());
        p.unique_bus_name.clear();
        p.menubar_path.clear();
        p.app_menu_path.clear();
        assert!(p.is_empty());
    }

    #[test]
    fn known_gesture_values_decode() {
        assert_eq!(decode_gesture(1), Some(Gesture::Double));
        assert_eq!(decode_gesture(2), Some(Gesture::Right));
        assert_eq!(decode_gesture(3), Some(Gesture::Middle));
    }

    #[test]
    fn unknown_gesture_value_is_rejected() {
        assert_eq!(decode_gesture(0), None);
        assert_eq!(decode_gesture(4), None);
        assert_eq!(decode_gesture(u32::MAX), None);
    }

    #[test]
    fn gtk_request_focus_without_token_is_ignored() {
        let (_loop, mut display, mut state) = test_state();
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        state.handle_gtk_request_focus(&surface, None);
        assert!(state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_request_focus_with_unknown_token_is_ignored() {
        let (_loop, mut display, mut state) = test_state();
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        state.handle_gtk_request_focus(&surface, Some("not-a-token".to_string()));
        assert!(state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_request_focus_with_expired_token_is_ignored() {
        let (_loop, mut display, mut state) = test_state();
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        let token = make_token(&mut state, &surface, Duration::from_secs(60));
        let token_key = token.clone().into();
        assert!(state.activation_state.data_for_token(&token_key).is_some());
        state.handle_gtk_request_focus(&surface, Some(token.clone()));
        // The token is consumed by the activation path, which then rejects it for age.
        assert!(state.activation_state.data_for_token(&token_key).is_none());
        assert!(state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_request_focus_with_valid_token_activates() {
        let (_loop, mut display, mut state) = test_state();
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        let token = make_token(&mut state, &surface, Duration::ZERO);
        let token_key = token.clone().into();
        assert!(state.activation_state.data_for_token(&token_key).is_some());
        state.handle_gtk_request_focus(&surface, Some(token));
        // The activation path consumed the valid token and restored the window.
        assert!(state.activation_state.data_for_token(&token_key).is_none());
        assert!(!state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_present_from_focused_client_activates() {
        let (_loop, mut display, mut state) = test_state();
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        set_keyboard_focus(&mut state, &surface);
        state.handle_gtk_present(&surface);
        assert!(!state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_present_from_unfocused_client_is_ignored() {
        let (_loop, mut display, mut state) = test_state();
        let (focused_surface, shutdown_focused) = create_toplevel(&mut display, &mut state);
        let (other_surface, shutdown_other) = create_toplevel(&mut display, &mut state);
        let other_id = window_id(&state, &other_surface);
        state.set_minimized(other_id, true);
        set_keyboard_focus(&mut state, &focused_surface);
        state.handle_gtk_present(&other_surface);
        assert!(state.minimized.contains(&other_id));
        let _ = shutdown_focused.send(());
        let _ = shutdown_other.send(());
    }
}
