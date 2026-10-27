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
                if let Some(id) = state.windows.id_of(surface) {
                    // Same activation path as xdg_activation_v1 (Block 11).
                    state.activate_window(id);
                }
            }
            protocol::gtk_surface1::Request::RequestFocus { startup_id } => {
                debug!(?startup_id, "gtk_surface request_focus");
                if let Some(id) = state.windows.id_of(surface) {
                    // Same activation path as xdg_activation_v1 (Block 11).
                    state.activate_window(id);
                }
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
    use super::{Gesture, GtkDbusProperties, decode_gesture};

    fn props(bus: &str, menubar: &str, appmenu: &str) -> GtkDbusProperties {
        GtkDbusProperties {
            unique_bus_name: bus.to_string(),
            menubar_path: menubar.to_string(),
            app_menu_path: appmenu.to_string(),
            ..Default::default()
        }
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
}
