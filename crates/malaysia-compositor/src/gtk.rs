use malaysia_scene::scene::WindowId;
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId, ObjectId};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::wayland::xdg_activation::XdgActivationToken;
use tracing::{debug, info, warn};

use crate::state::State;
use std::collections::HashMap;

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

    fn to_menus_address(&self) -> Option<GtkMenusAddress> {
        if self.unique_bus_name.is_empty()
            || (self.menubar_path.is_empty() && self.app_menu_path.is_empty())
        {
            return None;
        }
        Some(GtkMenusAddress {
            unique_bus_name: self.unique_bus_name.clone(),
            application_id: self.application_id.clone(),
            menubar_path: self.menubar_path.clone(),
            app_menu_path: self.app_menu_path.clone(),
            window_object_path: self.window_object_path.clone(),
            application_object_path: self.application_object_path.clone(),
        })
    }
}

#[derive(Debug, Clone)]
struct GtkSurfaceState {
    owner: ObjectId,
    props: GtkDbusProperties,
}

pub struct GtkShellState {
    _global: GlobalId,
    surfaces: HashMap<ObjectId, GtkSurfaceState>,
}

impl GtkShellState {
    pub fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<State, GtkShell1, ()>(5, ()),
            surfaces: HashMap::new(),
        }
    }

    fn set_props(&mut self, surface: &WlSurface, owner: ObjectId, props: GtkDbusProperties) {
        if !surface.is_alive() {
            return;
        }
        if let Some(existing) = self.surfaces.get(&surface.id())
            && existing.owner != owner
        {
            warn!(surface = ?surface.id(), "gtk_surface1: second object for surface, newest wins");
        }
        self.surfaces
            .insert(surface.id(), GtkSurfaceState { owner, props });
    }

    fn clear(&mut self, surface: &WlSurface, owner: &ObjectId) -> bool {
        if let Some(state) = self.surfaces.get(&surface.id())
            && state.owner != *owner
        {
            return false;
        }
        self.surfaces.remove(&surface.id()).is_some()
    }

    fn clear_surface(&mut self, surface: &WlSurface) -> bool {
        self.surfaces.remove(&surface.id()).is_some()
    }

    fn props(&self, surface: &WlSurface) -> Option<&GtkDbusProperties> {
        self.surfaces.get(&surface.id()).map(|state| &state.props)
    }
}

fn capabilities(app_menus: bool) -> u32 {
    if app_menus { 3 } else { 0 }
}

impl GlobalDispatch<GtkShell1, (), State> for GtkShellState {
    fn bind(
        state: &mut State,
        _handle: &DisplayHandle,
        client: &Client,
        resource: New<GtkShell1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        if let Some(client_state) = client.get_data::<crate::state::ClientState>() {
            client_state
                .gtk_client
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let shell = data_init.init(resource, ());
        shell.capabilities(capabilities(state.config.menubar.app_menus));
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
                if let Some(id) = state.windows.id_of(surface) {
                    info!(window = id.get(), "gtk_surface set_modal");
                }
            }
            protocol::gtk_surface1::Request::UnsetModal => {
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
                match gesture.into_result() {
                    Ok(protocol::gtk_surface1::Gesture::DoubleClick) => {
                        info!(window = id.get(), "gtk_surface titlebar double_click");
                        state.apply_titlebar_double_click(id);
                    }
                    // macOS titlebars have no right- or middle-click action; GTK delegates them here (v5).
                    Ok(gesture) => {
                        debug!(
                            window = id.get(),
                            ?gesture,
                            "gtk_surface titlebar gesture ignored"
                        );
                    }
                    Err(_) => resource.post_error(
                        protocol::gtk_surface1::Error::InvalidGesture,
                        "invalid titlebar gesture",
                    ),
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
        }
    }

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

    pub(crate) fn gtk_menus_address(&self, id: WindowId) -> Option<GtkMenusAddress> {
        let surface = self.windows.surface_of(id)?;
        let props = self.gtk.props(surface)?;
        props.to_menus_address()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GtkMenusAddress {
    pub unique_bus_name: String,
    pub application_id: String,
    pub menubar_path: String,
    pub app_menu_path: String,
    pub window_object_path: String,
    pub application_object_path: String,
}

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
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use sctk::reexports::client::globals::{GlobalListContents, registry_queue_init};
    use sctk::reexports::client::protocol::{wl_compositor, wl_registry, wl_surface};
    use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
    use sctk::reexports::protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

    use smithay::reexports::wayland_server::{Display, Resource};
    use smithay::utils::SERIAL_COUNTER;

    use smithay::wayland::xdg_activation::XdgActivationTokenData;

    use super::{GtkDbusProperties, capabilities};

    #[test]
    fn capabilities_follow_the_bar_app_menus() {
        assert_eq!(capabilities(true), 3);
        assert_eq!(capabilities(false), 0);
    }

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

    sctk::reexports::client::delegate_noop!(TestClient: ignore wl_compositor::WlCompositor);

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

    sctk::reexports::client::delegate_noop!(TestClient: ignore xdg_toplevel::XdgToplevel);

    sctk::reexports::client::delegate_noop!(TestClient: ignore wl_surface::WlSurface);

    fn props(bus: &str, menubar: &str, appmenu: &str) -> GtkDbusProperties {
        GtkDbusProperties {
            unique_bus_name: bus.to_string(),
            menubar_path: menubar.to_string(),
            app_menu_path: appmenu.to_string(),
            ..Default::default()
        }
    }

    fn create_toplevel(
        display: &mut Display<super::State>,
        state: &mut super::State,
    ) -> (
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        mpsc::Sender<()>,
    ) {
        let before: Vec<_> = state.windows.surfaces().map(|s| s.id()).collect();
        let (client_side, server_side) = UnixStream::pair().expect("socket pair");
        crate::test_support::insert_client(display, server_side);
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
    fn app_menu_only_is_enough_for_gtk_menus_address() {
        let address = props(":1.2", "", "/appmenu").to_menus_address();
        assert!(address.is_some());
        assert_eq!(address.unwrap().app_menu_path, "/appmenu");
    }

    #[test]
    fn menubar_only_is_enough_for_gtk_menus_address() {
        let address = props(":1.2", "/menubar", "").to_menus_address();
        assert!(address.is_some());
        assert_eq!(address.unwrap().menubar_path, "/menubar");
    }

    #[test]
    fn no_menu_paths_means_no_gtk_menus_address() {
        assert!(props(":1.2", "", "").to_menus_address().is_none());
    }

    #[test]
    fn gtk_request_focus_without_token_is_ignored() {
        let (_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        state.handle_gtk_request_focus(&surface, None);
        assert!(state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_request_focus_with_unknown_token_is_ignored() {
        let (_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        state.handle_gtk_request_focus(&surface, Some("not-a-token".to_string()));
        assert!(state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_request_focus_with_expired_token_is_ignored() {
        let (_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        let token = make_token(&mut state, &surface, Duration::from_secs(60));
        let token_key = token.clone().into();
        assert!(state.activation_state.data_for_token(&token_key).is_some());
        state.handle_gtk_request_focus(&surface, Some(token.clone()));
        assert!(state.activation_state.data_for_token(&token_key).is_none());
        assert!(state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_request_focus_with_valid_token_activates() {
        let (_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));
        let (surface, shutdown) = create_toplevel(&mut display, &mut state);
        let id = window_id(&state, &surface);
        state.set_minimized(id, true);
        let token = make_token(&mut state, &surface, Duration::ZERO);
        let token_key = token.clone().into();
        assert!(state.activation_state.data_for_token(&token_key).is_some());
        state.handle_gtk_request_focus(&surface, Some(token));
        assert!(state.activation_state.data_for_token(&token_key).is_none());
        assert!(!state.minimized.contains(&id));
        let _ = shutdown.send(());
    }

    #[test]
    fn gtk_present_from_focused_client_activates() {
        let (_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));
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
        let (_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));
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
