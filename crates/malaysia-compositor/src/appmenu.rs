use std::collections::HashMap;
use std::hash::Hash;

use malaysia_scene::scene::WindowId;
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId, ObjectId};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use tracing::{debug, info};
use wayland_protocols_plasma::appmenu::server::org_kde_kwin_appmenu::{self, OrgKdeKwinAppmenu};
use wayland_protocols_plasma::appmenu::server::org_kde_kwin_appmenu_manager::{
    self, OrgKdeKwinAppmenuManager,
};

use crate::state::State;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppMenuAddress {
    pub service: String,
    pub path: String,
}

#[derive(Debug)]
pub(crate) struct AddressStore<K, V> {
    addresses: HashMap<K, V>,
}

impl<K, V> Default for AddressStore<K, V> {
    fn default() -> Self {
        Self {
            addresses: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash, V> AddressStore<K, V> {
    pub(crate) fn set(&mut self, key: K, value: V) {
        self.addresses.insert(key, value);
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.addresses.get(key)
    }

    pub(crate) fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.addresses.get_mut(key)
    }

    pub(crate) fn clear(&mut self, key: &K) -> bool {
        self.addresses.remove(key).is_some()
    }
}

pub struct AppMenuState {
    _global: GlobalId,
    addresses: AddressStore<ObjectId, AppMenuAddress>,
}

impl AppMenuState {
    pub fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<State, OrgKdeKwinAppmenuManager, ()>(2, ()),
            addresses: AddressStore::default(),
        }
    }

    fn set(&mut self, surface: &WlSurface, address: AppMenuAddress) {
        if surface.is_alive() {
            self.addresses.set(surface.id(), address);
        }
    }

    fn clear(&mut self, surface: &WlSurface) -> bool {
        self.addresses.clear(&surface.id())
    }

    fn get(&self, surface: &WlSurface) -> Option<&AppMenuAddress> {
        self.addresses.get(&surface.id())
    }
}

impl GlobalDispatch<OrgKdeKwinAppmenuManager, (), State> for AppMenuState {
    fn bind(
        _state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<OrgKdeKwinAppmenuManager>,
        _global_data: &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<OrgKdeKwinAppmenuManager, (), State> for AppMenuState {
    fn request(
        _state: &mut State,
        _client: &Client,
        _resource: &OrgKdeKwinAppmenuManager,
        request: org_kde_kwin_appmenu_manager::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            org_kde_kwin_appmenu_manager::Request::Create { id, surface } => {
                data_init.init(id, surface);
            }
            org_kde_kwin_appmenu_manager::Request::Release => {}
            _ => {}
        }
    }
}

impl Dispatch<OrgKdeKwinAppmenu, WlSurface, State> for AppMenuState {
    fn request(
        state: &mut State,
        _client: &Client,
        _resource: &OrgKdeKwinAppmenu,
        request: org_kde_kwin_appmenu::Request,
        surface: &WlSurface,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            org_kde_kwin_appmenu::Request::SetAddress {
                service_name,
                object_path,
            } => state.set_surface_appmenu(surface, service_name, object_path),
            org_kde_kwin_appmenu::Request::Release => {
                state.clear_surface_appmenu(surface);
            }
            _ => {}
        }
    }

    fn destroyed(
        state: &mut State,
        _client: ClientId,
        _resource: &OrgKdeKwinAppmenu,
        surface: &WlSurface,
    ) {
        state.clear_surface_appmenu(surface);
    }
}

impl State {
    pub(crate) fn set_surface_appmenu(
        &mut self,
        surface: &WlSurface,
        service: String,
        path: String,
    ) {
        if service.is_empty() || path.is_empty() {
            self.clear_surface_appmenu(surface);
            return;
        }
        let address = AppMenuAddress { service, path };
        let window = self.windows.id_of(surface);
        self.appmenu.set(surface, address.clone());
        if let Some(id) = window {
            info!(
                window = id.get(),
                service = %address.service,
                path = %address.path,
                "appmenu set"
            );
            self.send_app_menu(id);
        } else {
            debug!(
                surface = ?surface.id(),
                service = %address.service,
                path = %address.path,
                "appmenu set pending map"
            );
        }
    }

    pub(crate) fn clear_surface_appmenu(&mut self, surface: &WlSurface) {
        if !self.appmenu.clear(surface) {
            return;
        }
        match self.windows.id_of(surface) {
            Some(id) => {
                info!(window = id.get(), "appmenu cleared");
                self.send_app_menu(id);
            }
            None => debug!(surface = ?surface.id(), "appmenu cleared pending map"),
        }
    }

    pub(crate) fn appmenu_address(&self, id: WindowId) -> Option<AppMenuAddress> {
        let surface = self.windows.surface_of(id)?;
        self.appmenu.get(surface).cloned()
    }
}

#[macro_export]
macro_rules! delegate_appmenu {
    ($ty:ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($ty: [
            wayland_protocols_plasma::appmenu::server::org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager: ()
        ] => $crate::appmenu::AppMenuState);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            wayland_protocols_plasma::appmenu::server::org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager: ()
        ] => $crate::appmenu::AppMenuState);
        smithay::reexports::wayland_server::delegate_dispatch!($ty: [
            wayland_protocols_plasma::appmenu::server::org_kde_kwin_appmenu::OrgKdeKwinAppmenu:
                smithay::reexports::wayland_server::protocol::wl_surface::WlSurface
        ] => $crate::appmenu::AppMenuState);
    };
}

#[cfg(test)]
mod tests {
    use super::{AddressStore, AppMenuAddress};

    fn address(service: &str, path: &str) -> AppMenuAddress {
        AppMenuAddress {
            service: service.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn set_before_map_is_kept_for_the_map() {
        let mut store = AddressStore::<u32, AppMenuAddress>::default();
        store.set(7, address(":1.2", "/MenuBar/1"));
        assert_eq!(store.get(&7), Some(&address(":1.2", "/MenuBar/1")));
    }

    #[test]
    fn set_after_map_replaces_the_address() {
        let mut store = AddressStore::<u32, AppMenuAddress>::default();
        store.set(7, address(":1.2", "/MenuBar/1"));
        store.set(7, address(":1.3", "/MenuBar/2"));
        assert_eq!(store.get(&7), Some(&address(":1.3", "/MenuBar/2")));
    }

    #[test]
    fn release_clears_the_address() {
        let mut store = AddressStore::<u32, AppMenuAddress>::default();
        store.set(7, address(":1.2", "/MenuBar/1"));
        assert!(store.clear(&7));
        assert_eq!(store.get(&7), None);
        assert!(!store.clear(&7));
    }

    #[test]
    fn surface_destruction_clears_the_address() {
        let mut store = AddressStore::<u32, AppMenuAddress>::default();
        store.set(9, address(":1.4", "/MenuBar/9"));
        assert!(store.clear(&9));
        assert_eq!(store.get(&9), None);
    }
}
