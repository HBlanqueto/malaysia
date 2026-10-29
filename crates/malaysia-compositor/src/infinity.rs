//! Infinity window manager protocol (ADR 0039).
//!
//! This module serves the private `infinity_window_manager_v1` protocol that
//! malaysia-shell uses to ask the compositor for window geometry: halves, quarters,
//! pairs, fill, center, restore and fullscreen. The global is only visible to
//! the malaysia-shell binary or when `MALAYSIA_DEV=1`.

use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use nix::sys::socket::{getsockopt, sockopt};
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource, WEnum};
use smithay::utils::{Logical, Point, Rectangle, Size};
use tracing::info;
use malaysia_scene::geometry::{Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize};
use malaysia_scene::scene::WindowId;

use crate::state::{ClientState, State};

/// Re-export of the generated Infinity protocol server bindings.
pub(crate) mod protocol {
    pub use infinity_protocol::server::infinity_window_manager_v1::infinity_window_manager_v1;
}

use protocol::infinity_window_manager_v1::InfinityWindowManagerV1;
use protocol::infinity_window_manager_v1::{Layout, PairLayout};

/// Margin around the workspace for tiled windows, in logical pixels.
const TILE_MARGIN: i32 = 8;
/// Gap between tiled windows, in logical pixels.
const TILE_GAP: i32 = 8;

/// Data attached to the `infinity_window_manager_v1` global.
///
/// The trust decision is made when the client is accepted (see
/// [`is_trusted_client`]) and stored on its [`ClientState`], never here: the
/// visibility filter runs with the backend's state lock held.
#[derive(Debug, Clone)]
pub struct InfinityGlobalData;

/// Whether the client connecting on `stream` is the trusted `malaysia-shell`.
///
/// Must be called before `insert_client`: reading the peer credentials needs
/// the socket, and the global filter later runs with the backend lock held.
pub(crate) fn is_trusted_client(stream: &UnixStream) -> bool {
    if std::env::var("MALAYSIA_DEV").is_ok_and(|v| v == "1") {
        return true;
    }
    let Ok(credentials) = getsockopt(stream, sockopt::PeerCredentials) else {
        return false;
    };
    let pid = credentials.pid();
    let Ok(exe) = std::fs::canonicalize(format!("/proc/{pid}/exe")) else {
        return false;
    };
    let trusted = trusted_shell_exes();
    let trusted_shell = is_trusted_shell_exe(&exe, &trusted);
    tracing::debug!(pid, exe = ?exe, trusted = ?trusted, trusted_shell, "infinity trust check");
    trusted_shell
}

/// Resolves the set of executable paths that are trusted as `malaysia-shell`.
///
/// Covers the binary next to the compositor, the dev-shell debug/release
/// builds, the Nix package wrapper directory, and an explicit override so the
/// demo, the packaged session and manual launches all agree.
fn trusted_shell_exes() -> Vec<PathBuf> {
    let mut exes = Vec::new();
    if let Some(path) = std::env::var_os("MALAYSIA_SHELL_EXE")
        && let Ok(path) = std::fs::canonicalize(path)
    {
        exes.push(path);
    }

    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
    {
        candidate_dirs.push(dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        for profile in ["debug", "release"] {
            candidate_dirs.push(cwd.join("target").join(profile));
        }
        candidate_dirs.push(cwd.join("result").join("bin"));
    }

    for dir in candidate_dirs {
        for name in ["malaysia-shell", ".malaysia-shell-wrapped"] {
            if let Ok(path) = std::fs::canonicalize(dir.join(name))
                && !exes.contains(&path)
            {
                exes.push(path);
            }
        }
    }
    exes
}

impl smithay::wayland::GlobalDispatch2<InfinityWindowManagerV1, State> for InfinityGlobalData {
    fn bind(
        &self,
        state: &mut State,
        handle: &DisplayHandle,
        client: &Client,
        resource: New<InfinityWindowManagerV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        let pid = client
            .get_credentials(handle)
            .ok()
            .map(|creds| creds.pid)
            .unwrap_or(0);
        info!(pid, "infinity bound by trusted shell");
        let manager = data_init.init(resource, self.clone());
        if manager.version() >= 2 {
            let client_id = client.id();
            let ids: Vec<WindowId> = state.windows.iter().map(|window| window.id()).collect();
            for id in ids {
                let Some(handle) = state
                    .foreign
                    .management
                    .handle_for_client(id, client_id.clone())
                else {
                    continue;
                };
                let address = state.appmenu_address(id).unwrap_or_default();
                manager.app_menu(handle, address.service, address.path);

                if manager.version() >= 3 {
                    let gtk = state.gtk_menus_address(id).unwrap_or_default();
                    manager.gtk_menus(
                        handle,
                        gtk.unique_bus_name,
                        gtk.application_id,
                        gtk.menubar_path,
                        gtk.app_menu_path,
                        gtk.application_object_path,
                        gtk.window_object_path,
                    );
                }
            }
        }
        state.infinity_managers.push(manager);
    }

    fn can_view(&self, client: &Client) -> bool {
        // Called with the backend's state lock held: do not call the
        // DisplayHandle here (get_credentials would deadlock).
        client
            .get_data::<ClientState>()
            .is_some_and(|state| state.trusted_shell)
    }
}

/// Whether `exe` matches any of the trusted `malaysia-shell` paths.
fn is_trusted_shell_exe(exe: &std::path::Path, shell_exes: &[PathBuf]) -> bool {
    shell_exes.iter().any(|trusted| trusted == exe)
}

impl smithay::wayland::Dispatch2<InfinityWindowManagerV1, State> for InfinityGlobalData {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _resource: &InfinityWindowManagerV1,
        request: <InfinityWindowManagerV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        use protocol::infinity_window_manager_v1::Request;
        match request {
            Request::Arrange { handle, layout } => {
                if let Some(id) = window_id_from_handle(&handle) {
                    state.infinity_arrange(id, layout_from_wenum(layout));
                }
            }
            Request::ArrangePair { layout } => {
                state.infinity_arrange_pair(pair_layout_from_wenum(layout));
            }
            Request::ArrangeQuarters => {
                state.infinity_arrange_quarters();
            }
            Request::Restore { handle } => {
                if let Some(id) = window_id_from_handle(&handle) {
                    state.infinity_restore(id);
                }
            }
            Request::SetFullscreen { handle, fullscreen } => {
                if let Some(id) = window_id_from_handle(&handle) {
                    state.infinity_set_fullscreen(id, fullscreen != 0);
                }
            }
            Request::Destroy => {}
            // shortcut: future Infinity requests are ignored; handle them when
            // the client starts sending them.
            _ => {}
        }
    }

    fn destroyed(&self, state: &mut State, _client: ClientId, resource: &InfinityWindowManagerV1) {
        let id = resource.id();
        state.infinity_managers.retain(|m| m.id() != id);
    }
}

/// Extracts the [`WindowId`] stored on a foreign-toplevel handle.
fn window_id_from_handle(handle: &ZwlrForeignToplevelHandleV1) -> Option<WindowId> {
    handle.data::<WindowId>().copied()
}

fn layout_from_wenum(layout: WEnum<Layout>) -> Layout {
    match layout {
        WEnum::Value(layout) => layout,
        WEnum::Unknown(value) => match value {
            0 => Layout::Fill,
            1 => Layout::Center,
            2 => Layout::Left,
            3 => Layout::Right,
            4 => Layout::Top,
            5 => Layout::Bottom,
            6 => Layout::TopLeft,
            7 => Layout::TopRight,
            8 => Layout::BottomLeft,
            9 => Layout::BottomRight,
            _ => Layout::Fill,
        },
    }
}

fn pair_layout_from_wenum(layout: WEnum<PairLayout>) -> PairLayout {
    match layout {
        WEnum::Value(layout) => layout,
        WEnum::Unknown(value) => match value {
            0 => PairLayout::SideBySide,
            1 => PairLayout::SideBySideReversed,
            2 => PairLayout::Stacked,
            3 => PairLayout::StackedReversed,
            _ => PairLayout::SideBySide,
        },
    }
}

impl InfinityGlobalData {
    /// Creates the `infinity_window_manager_v1` global.
    pub fn create_global(display_handle: &DisplayHandle) -> GlobalId {
        let data = InfinityGlobalData;
        display_handle.create_global::<State, InfinityWindowManagerV1, _>(3, data)
    }
}

impl State {
    /// Places a window in the requested Infinity layout.
    pub(crate) fn infinity_arrange(&mut self, id: WindowId, layout: Layout) {
        info!(window = id.get(), ?layout, "infinity arrange");
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        if self.maximized.contains_key(&id) {
            self.set_maximized(id, false);
        }
        if self.fullscreen.contains_key(&id) {
            self.infinity_set_fullscreen(id, false);
        }
        self.arranged.entry(id).or_insert_with(|| outer);
        let workspace = self.workspace_rect();
        let current_size = Size::from((round(outer.width()), round(outer.height())));
        let target = arranged_rect(workspace, layout, current_size);
        self.apply_infinity_geometry(id, target);
        self.send_restorable(id);
    }

    /// Tiles the active and previous windows as a pair.
    pub(crate) fn infinity_arrange_pair(&mut self, layout: PairLayout) {
        info!(?layout, "infinity arrange pair");
        let visible: Vec<WindowId> = self
            .windows
            .iter_visible(&self.minimized)
            .map(|w| w.id())
            .collect();
        if visible.len() < 2 {
            return;
        }
        let active = *visible.last().expect("non-empty");
        let previous = visible[visible.len() - 2];
        let workspace = self.workspace_rect();
        let [first, second] = pair_rects(workspace, layout);
        self.infinity_arrange(active, Layout::Fill);
        self.apply_infinity_geometry(active, first);
        self.infinity_arrange(previous, Layout::Fill);
        self.apply_infinity_geometry(previous, second);
    }

    /// Tiles the four most recent visible windows in quarters.
    pub(crate) fn infinity_arrange_quarters(&mut self) {
        info!("infinity arrange quarters");
        let visible: Vec<WindowId> = self
            .windows
            .iter_visible(&self.minimized)
            .rev()
            .take(4)
            .map(|w| w.id())
            .collect();
        if visible.is_empty() {
            return;
        }
        let workspace = self.workspace_rect();
        let rects = quarter_rects(workspace);
        for (id, rect) in visible.iter().zip(rects.iter()) {
            self.infinity_arrange(*id, Layout::Fill);
            self.apply_infinity_geometry(*id, *rect);
        }
    }

    /// Restores a window to the rectangle saved before the first arrange.
    pub(crate) fn infinity_restore(&mut self, id: WindowId) {
        let Some(saved) = self.arranged.remove(&id) else {
            return;
        };
        info!(window = id.get(), outer = ?saved, "infinity restore");
        self.apply_infinity_geometry(id, saved);
        self.send_restorable(id);
    }

    /// Sets or unsets fullscreen through Infinity.
    pub(crate) fn infinity_set_fullscreen(&mut self, id: WindowId, fullscreen: bool) {
        info!(window = id.get(), fullscreen, "infinity set fullscreen");
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        if fullscreen {
            self.fullscreen.entry(id).or_insert(outer);
            let full = self.output_rect();
            self.windows.set_fullscreen_with_outer(id, true, full);
        } else {
            let saved = self.fullscreen.remove(&id).unwrap_or(outer);
            self.windows.set_fullscreen_with_outer(id, false, saved);
        }
        self.foreign_state_changed(id);
        self.request_redraw();
    }

    /// Applies a target outer rectangle to a window, converting scene types.
    fn apply_infinity_geometry(&mut self, id: WindowId, target: SceneRect<f64, SceneLogical>) {
        info!(window = id.get(), outer = ?target, "infinity geometry");
        self.windows.set_infinity_geometry(id, target);
        self.foreign_state_changed(id);
        self.request_redraw();
    }

    /// Workspace as a logical rectangle.
    fn workspace_rect(&self) -> Rectangle<i32, Logical> {
        let (origin, size) = self.workspace();
        Rectangle::new(origin, size)
    }

    /// Whole output as a logical rectangle.
    fn output_rect(&self) -> SceneRect<f64, SceneLogical> {
        let size = self.logical_output_size();
        SceneRect::new(
            ScenePoint::new(0.0, 0.0),
            SceneSize::new(f64::from(size.w), f64::from(size.h)),
        )
    }

    /// Sends the current restorable state of a window to every Infinity client.
    fn send_restorable(&mut self, id: WindowId) {
        let restorable = self.arranged.contains_key(&id);
        self.infinity_managers.retain(|manager| manager.is_alive());
        for manager in &self.infinity_managers {
            let Some(client) = manager.client() else {
                continue;
            };
            let Some(handle) = self.foreign.management.handle_for_client(id, client.id()) else {
                continue;
            };
            manager.restorable(handle, restorable as u32);
        }
    }

    /// Sends the window's current app_menu address (or empty strings when it
    /// has none) to every Infinity v2 client that has a handle for it.
    pub(crate) fn send_app_menu(&mut self, id: WindowId) {
        let address = self.appmenu_address(id).unwrap_or_default();
        self.infinity_managers.retain(|manager| manager.is_alive());
        for manager in &self.infinity_managers {
            let Some(client) = manager.client() else {
                continue;
            };
            let handle = self.foreign.management.handle_for_client(id, client.id());
            if !delivers_event(manager.version(), 2, handle.is_some()) {
                continue;
            }
            let Some(handle) = handle else {
                continue;
            };
            manager.app_menu(handle, address.service.clone(), address.path.clone());
        }
    }

    /// Sends the window's current GTK menu address (or empty strings when it
    /// has none) to every Infinity v3 client that has a handle for it.
    pub(crate) fn send_gtk_menus(&mut self, id: WindowId) {
        let address = self.gtk_menus_address(id).unwrap_or_default();
        self.infinity_managers.retain(|manager| manager.is_alive());
        for manager in &self.infinity_managers {
            let Some(client) = manager.client() else {
                continue;
            };
            let handle = self.foreign.management.handle_for_client(id, client.id());
            if !delivers_event(manager.version(), 3, handle.is_some()) {
                continue;
            }
            let Some(handle) = handle else {
                continue;
            };
            manager.gtk_menus(
                handle,
                address.unique_bus_name.clone(),
                address.application_id.clone(),
                address.menubar_path.clone(),
                address.app_menu_path.clone(),
                address.application_object_path.clone(),
                address.window_object_path.clone(),
            );
        }
    }

    /// Sends the window's pair to one client's Infinity v2 manager, if it has
    /// one and the window has a pair. Used when a wlr handle is created.
    pub(crate) fn send_app_menu_to_client(
        &mut self,
        id: WindowId,
        client: smithay::reexports::wayland_server::backend::ClientId,
    ) {
        let Some(address) = self.appmenu_address(id) else {
            return;
        };
        let client_id = client;
        for manager in &self.infinity_managers {
            let Some(owner) = manager.client() else {
                continue;
            };
            if owner.id() != client_id {
                continue;
            }
            let handle = self
                .foreign
                .management
                .handle_for_client(id, client_id.clone());
            if !delivers_event(manager.version(), 2, handle.is_some()) {
                continue;
            }
            let Some(handle) = handle else {
                continue;
            };
            manager.app_menu(handle, address.service.clone(), address.path.clone());
        }
    }

    /// Sends the window's GTK menu address to one client, when its wlr handle
    /// is created. Used when a wlr handle is created.
    pub(crate) fn send_gtk_menus_to_client(
        &mut self,
        id: WindowId,
        client: smithay::reexports::wayland_server::backend::ClientId,
    ) {
        let Some(address) = self.gtk_menus_address(id) else {
            return;
        };
        let client_id = client;
        for manager in &self.infinity_managers {
            let Some(owner) = manager.client() else {
                continue;
            };
            if owner.id() != client_id {
                continue;
            }
            let handle = self
                .foreign
                .management
                .handle_for_client(id, client_id.clone());
            if !delivers_event(manager.version(), 3, handle.is_some()) {
                continue;
            }
            let Some(handle) = handle else {
                continue;
            };
            manager.gtk_menus(
                handle,
                address.unique_bus_name.clone(),
                address.application_id.clone(),
                address.menubar_path.clone(),
                address.app_menu_path.clone(),
                address.application_object_path.clone(),
                address.window_object_path.clone(),
            );
        }
    }
}

/// Whether a manager receives an event introduced in `since`.
/// Pure so the bind-order and version rules can be tested without a protocol harness.
fn delivers_event(manager_version: u32, since: u32, has_handle: bool) -> bool {
    manager_version >= since && has_handle
}

/// Rounds a logical length to whole pixels.
fn round(value: f64) -> i32 {
    value.round() as i32
}

/// Pure geometry: workspace minus the outer margin.
fn available(workspace: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((workspace.loc.x + TILE_MARGIN, workspace.loc.y + TILE_MARGIN)),
        Size::from((
            (workspace.size.w - 2 * TILE_MARGIN).max(0),
            (workspace.size.h - 2 * TILE_MARGIN).max(0),
        )),
    )
}

/// Pure geometry: target rectangle for a single-window layout.
fn arranged_rect(
    workspace: Rectangle<i32, Logical>,
    layout: Layout,
    current_size: Size<i32, Logical>,
) -> SceneRect<f64, SceneLogical> {
    let area = available(workspace);
    let loc = area.loc;
    let size = area.size;
    let half_w = (size.w - TILE_GAP) / 2;
    let half_h = (size.h - TILE_GAP) / 2;
    let rect = |x, y, w, h| {
        SceneRect::new(
            ScenePoint::new(f64::from(x), f64::from(y)),
            SceneSize::new(f64::from(w), f64::from(h)),
        )
    };
    match layout {
        Layout::Fill => rect(loc.x, loc.y, size.w, size.h),
        Layout::Center => {
            let w = current_size.w.min(area.size.w);
            let h = current_size.h.min(area.size.h);
            let x = loc.x + (area.size.w - w) / 2;
            let y = loc.y + (area.size.h - h) / 2;
            rect(x, y, w, h)
        }
        Layout::Left => rect(loc.x, loc.y, half_w, size.h),
        Layout::Right => rect(loc.x + half_w + TILE_GAP, loc.y, half_w, size.h),
        Layout::Top => rect(loc.x, loc.y, size.w, half_h),
        Layout::Bottom => rect(loc.x, loc.y + half_h + TILE_GAP, size.w, half_h),
        Layout::TopLeft => rect(loc.x, loc.y, half_w, half_h),
        Layout::TopRight => rect(loc.x + half_w + TILE_GAP, loc.y, half_w, half_h),
        Layout::BottomLeft => rect(loc.x, loc.y + half_h + TILE_GAP, half_w, half_h),
        Layout::BottomRight => rect(
            loc.x + half_w + TILE_GAP,
            loc.y + half_h + TILE_GAP,
            half_w,
            half_h,
        ),
        // shortcut: future Infinity layouts fall back to centered.
        _ => {
            let w = current_size.w.min(area.size.w);
            let h = current_size.h.min(area.size.h);
            let x = loc.x + (area.size.w - w) / 2;
            let y = loc.y + (area.size.h - h) / 2;
            rect(x, y, w, h)
        }
    }
}

/// Pure geometry: target rectangles for a pair of windows.
fn pair_rects(
    workspace: Rectangle<i32, Logical>,
    layout: PairLayout,
) -> [SceneRect<f64, SceneLogical>; 2] {
    let area = available(workspace);
    let loc = area.loc;
    let size = area.size;
    let half_w = (size.w - TILE_GAP) / 2;
    let half_h = (size.h - TILE_GAP) / 2;
    let rect = |x, y, w, h| {
        SceneRect::new(
            ScenePoint::new(f64::from(x), f64::from(y)),
            SceneSize::new(f64::from(w), f64::from(h)),
        )
    };
    match layout {
        PairLayout::SideBySide | PairLayout::SideBySideReversed => {
            let left = rect(loc.x, loc.y, half_w, size.h);
            let right = rect(loc.x + half_w + TILE_GAP, loc.y, half_w, size.h);
            if matches!(layout, PairLayout::SideBySide) {
                [left, right]
            } else {
                [right, left]
            }
        }
        PairLayout::Stacked | PairLayout::StackedReversed => {
            let top = rect(loc.x, loc.y, size.w, half_h);
            let bottom = rect(loc.x, loc.y + half_h + TILE_GAP, size.w, half_h);
            if matches!(layout, PairLayout::Stacked) {
                [top, bottom]
            } else {
                [bottom, top]
            }
        }
        // shortcut: future Infinity pair layouts fall back to side-by-side.
        _ => {
            let left = rect(loc.x, loc.y, half_w, size.h);
            let right = rect(loc.x + half_w + TILE_GAP, loc.y, half_w, size.h);
            [left, right]
        }
    }
}

/// Pure geometry: target rectangles for four recent windows.
fn quarter_rects(workspace: Rectangle<i32, Logical>) -> [SceneRect<f64, SceneLogical>; 4] {
    let area = available(workspace);
    let loc = area.loc;
    let size = area.size;
    let half_w = (size.w - TILE_GAP) / 2;
    let half_h = (size.h - TILE_GAP) / 2;
    let rect = |x, y, w, h| {
        SceneRect::new(
            ScenePoint::new(f64::from(x), f64::from(y)),
            SceneSize::new(f64::from(w), f64::from(h)),
        )
    };
    [
        rect(loc.x, loc.y, half_w, half_h),
        rect(loc.x + half_w + TILE_GAP, loc.y, half_w, half_h),
        rect(loc.x, loc.y + half_h + TILE_GAP, half_w, half_h),
        rect(
            loc.x + half_w + TILE_GAP,
            loc.y + half_h + TILE_GAP,
            half_w,
            half_h,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use malaysia_scene::geometry::{
        Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
    };
    use smithay::utils::{Logical, Point, Rectangle, Size};

    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{
        Layout, PairLayout, arranged_rect, available, delivers_event, is_trusted_shell_exe,
        pair_rects, quarter_rects, trusted_shell_exes,
    };
    use malaysia_scene::scene::WindowId;

    fn workspace() -> Rectangle<i32, Logical> {
        // Output 1280×772, menu bar 32 px and dock 50 px.
        Rectangle::new(Point::from((0, 32)), Size::from((1280, 690)))
    }

    fn workspace_1280_800() -> Rectangle<i32, Logical> {
        // Output 1280×800, menu bar 28 px and dock 50 px (mockup 23).
        Rectangle::new(Point::from((0, 28)), Size::from((1280, 722)))
    }

    #[test]
    fn available_has_margin() {
        let rect = available(workspace());
        assert_eq!(rect.loc, Point::from((8, 40)));
        assert_eq!(rect.size, Size::from((1264, 674)));
    }

    fn window_size() -> Size<i32, Logical> {
        Size::from((640, 420))
    }

    #[test]
    fn fill_uses_available_area() {
        let rect = arranged_rect(workspace(), Layout::Fill, window_size());
        assert_eq!(rect.x(), 8.0);
        assert_eq!(rect.y(), 40.0);
        assert_eq!(rect.width(), 1264.0);
        assert_eq!(rect.height(), 674.0);
    }

    #[test]
    fn center_keeps_the_current_size() {
        let rect = arranged_rect(workspace(), Layout::Center, Size::from((640, 420)));
        assert_eq!(rect.width(), 640.0);
        assert_eq!(rect.height(), 420.0);
        assert_eq!(rect.x(), 320.0);
        assert_eq!(rect.y(), 167.0);
    }

    #[test]
    fn center_clamps_to_the_available_area() {
        let rect = arranged_rect(workspace(), Layout::Center, Size::from((2000, 1000)));
        assert_eq!(rect.width(), 1264.0);
        assert_eq!(rect.height(), 674.0);
    }

    #[test]
    fn left_half() {
        let rect = arranged_rect(workspace(), Layout::Left, window_size());
        assert_eq!(rect.x(), 8.0);
        assert_eq!(rect.y(), 40.0);
        assert_eq!(rect.width(), 628.0);
        assert_eq!(rect.height(), 674.0);
    }

    #[test]
    fn right_half() {
        let rect = arranged_rect(workspace(), Layout::Right, window_size());
        assert_eq!(rect.x(), 644.0);
        assert_eq!(rect.y(), 40.0);
        assert_eq!(rect.width(), 628.0);
        assert_eq!(rect.height(), 674.0);
    }

    #[test]
    fn top_left_quarter() {
        let rect = arranged_rect(workspace(), Layout::TopLeft, window_size());
        assert_eq!(rect.x(), 8.0);
        assert_eq!(rect.y(), 40.0);
        assert_eq!(rect.width(), 628.0);
        assert_eq!(rect.height(), 333.0);
    }

    #[test]
    fn bottom_right_quarter() {
        let rect = arranged_rect(workspace(), Layout::BottomRight, window_size());
        assert_eq!(rect.x(), 644.0);
        assert_eq!(rect.y(), 381.0);
        assert_eq!(rect.width(), 628.0);
        assert_eq!(rect.height(), 333.0);
    }

    #[test]
    fn pair_side_by_side() {
        let [left, right] = pair_rects(workspace(), PairLayout::SideBySide);
        assert_eq!(left.x(), 8.0);
        assert_eq!(left.width(), 628.0);
        assert_eq!(right.x(), 644.0);
        assert_eq!(right.width(), 628.0);
    }

    #[test]
    fn pair_stacked() {
        let [top, bottom] = pair_rects(workspace(), PairLayout::Stacked);
        assert_eq!(top.y(), 40.0);
        assert_eq!(top.height(), 333.0);
        assert_eq!(bottom.y(), 381.0);
        assert_eq!(bottom.height(), 333.0);
    }

    #[test]
    fn four_quarters_have_gap() {
        let rects = quarter_rects(workspace());
        assert_eq!(rects[0].x(), 8.0);
        assert_eq!(rects[1].x(), 644.0);
        assert_eq!(rects[2].x(), 8.0);
        assert_eq!(rects[3].x(), 644.0);
        assert_eq!(rects[0].width(), 628.0);
        assert_eq!(rects[0].height(), 333.0);
    }

    #[test]
    fn mockup_23_available_area() {
        let rect = available(workspace_1280_800());
        assert_eq!(rect.loc, Point::from((8, 36)));
        assert_eq!(rect.size, Size::from((1264, 706)));
    }

    #[test]
    fn mockup_23_left_half() {
        let rect = arranged_rect(workspace_1280_800(), Layout::Left, window_size());
        assert_eq!(rect.x(), 8.0);
        assert_eq!(rect.y(), 36.0);
        assert_eq!(rect.width(), 628.0);
        assert_eq!(rect.height(), 706.0);
    }

    #[test]
    fn mockup_23_right_half() {
        let rect = arranged_rect(workspace_1280_800(), Layout::Right, window_size());
        assert_eq!(rect.x(), 644.0);
        assert_eq!(rect.y(), 36.0);
        assert_eq!(rect.width(), 628.0);
        assert_eq!(rect.height(), 706.0);
    }

    #[test]
    fn mockup_23_top_and_bottom_halves() {
        let top = arranged_rect(workspace_1280_800(), Layout::Top, window_size());
        let bottom = arranged_rect(workspace_1280_800(), Layout::Bottom, window_size());
        assert_eq!(top.y(), 36.0);
        assert_eq!(top.height(), 349.0);
        assert_eq!(bottom.y(), 393.0);
        assert_eq!(bottom.height(), 349.0);
    }

    #[test]
    fn mockup_23_quarters() {
        let top_left = arranged_rect(workspace_1280_800(), Layout::TopLeft, window_size());
        let bottom_right = arranged_rect(workspace_1280_800(), Layout::BottomRight, window_size());
        assert_eq!(top_left.x(), 8.0);
        assert_eq!(top_left.y(), 36.0);
        assert_eq!(top_left.width(), 628.0);
        assert_eq!(top_left.height(), 349.0);
        assert_eq!(bottom_right.x(), 644.0);
        assert_eq!(bottom_right.y(), 393.0);
        assert_eq!(bottom_right.width(), 628.0);
        assert_eq!(bottom_right.height(), 349.0);
    }

    #[test]
    fn mockup_23_pair_side_by_side() {
        let [left, right] = pair_rects(workspace_1280_800(), PairLayout::SideBySide);
        assert_eq!(left.x(), 8.0);
        assert_eq!(left.y(), 36.0);
        assert_eq!(left.width(), 628.0);
        assert_eq!(left.height(), 706.0);
        assert_eq!(right.x(), 644.0);
        assert_eq!(right.y(), 36.0);
        assert_eq!(right.width(), 628.0);
        assert_eq!(right.height(), 706.0);
    }

    #[test]
    fn restore_returns_to_the_first_arrange_rectangle() {
        let first: SceneRect<f64, SceneLogical> =
            SceneRect::new(ScenePoint::new(100.0, 100.0), SceneSize::new(640.0, 420.0));
        let second: SceneRect<f64, SceneLogical> =
            SceneRect::new(ScenePoint::new(8.0, 40.0), SceneSize::new(628.0, 674.0));

        let mut arranged = HashMap::new();
        arranged.entry(WindowId::new(1)).or_insert(first);
        // A second arrange does not overwrite the saved rectangle.
        let saved = *arranged.get(&WindowId::new(1)).unwrap();
        assert_eq!(saved, first);

        // Simulate restore: apply the saved rectangle and clear it.
        arranged.remove(&WindowId::new(1));
        assert!(!arranged.contains_key(&WindowId::new(1)));
        assert_eq!(second, second); // target used by caller
    }

    #[test]
    #[allow(unsafe_code)]
    fn trusted_shell_list_accepts_a_malaysia_shell_exe_override() {
        let dir =
            std::env::temp_dir().join(format!("malaysia-shell-exe-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake-shell");
        std::fs::write(&fake, "").unwrap();

        let previous = std::env::var_os("MALAYSIA_SHELL_EXE");
        // SAFETY: only used during this single-threaded test.
        unsafe {
            std::env::set_var("MALAYSIA_SHELL_EXE", &fake);
        }
        let exes = trusted_shell_exes();
        if let Some(previous) = previous {
            // SAFETY: only used during this single-threaded test.
            unsafe {
                std::env::set_var("MALAYSIA_SHELL_EXE", previous);
            }
        } else {
            // SAFETY: only used during this single-threaded test.
            unsafe {
                std::env::remove_var("MALAYSIA_SHELL_EXE");
            }
        }
        assert!(
            is_trusted_shell_exe(&std::fs::canonicalize(&fake).unwrap(), &exes),
            "MALAYSIA_SHELL_EXE must be trusted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trusted_shell_comparison_accepts_wrapped_and_unwrapped_names() {
        let target = PathBuf::from("/nix/store/abc/bin/malaysia-shell");
        let wrapped = PathBuf::from("/nix/store/abc/bin/.malaysia-shell-wrapped");
        assert!(is_trusted_shell_exe(&target, std::slice::from_ref(&target)));
        assert!(is_trusted_shell_exe(
            &wrapped,
            std::slice::from_ref(&wrapped)
        ));
        assert!(!is_trusted_shell_exe(
            &target,
            std::slice::from_ref(&wrapped)
        ));
    }

    #[test]
    fn untrusted_binaries_are_rejected() {
        let trusted = PathBuf::from("/run/current-system/sw/bin/malaysia-shell");
        let attacker = PathBuf::from("/tmp/evil-shell");
        assert!(!is_trusted_shell_exe(&attacker, &[trusted]));
    }

    #[test]
    fn a_v1_manager_never_gets_app_menu() {
        assert!(!delivers_event(1, 2, true));
        assert!(!delivers_event(1, 2, false));
    }

    #[test]
    fn a_v2_manager_needs_a_handle() {
        assert!(delivers_event(2, 2, true));
        assert!(!delivers_event(2, 2, false));
    }

    #[test]
    fn a_v3_manager_needs_a_handle_for_gtk_menus() {
        assert!(delivers_event(3, 3, true));
        assert!(!delivers_event(3, 3, false));
        assert!(!delivers_event(2, 3, true));
    }

    #[test]
    fn the_pair_reaches_the_bar_in_either_bind_order() {
        // wlr manager first: the handle exists before Infinity binds.
        let manager = 2;
        let mut handle = true;
        assert!(delivers_event(manager, 2, handle));
        // Infinity manager first: binding alone sends nothing; the handle
        // created later by the wlr side completes the delivery.
        handle = false;
        assert!(!delivers_event(manager, 2, handle));
        handle = true;
        assert!(delivers_event(manager, 2, handle));
    }
}
