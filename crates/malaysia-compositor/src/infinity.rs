use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use nix::sys::socket::{getsockopt, sockopt};
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource};
use smithay::utils::{Logical, Point, Rectangle, Size};
use tracing::info;
use vow_design::{TILE_GAP, TILE_MARGIN};
use malaysia_scene::geometry::{Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize};
use malaysia_scene::scene::WindowId;

use crate::appmenu::AppMenuAddress;
use crate::gtk::GtkMenusAddress;
use crate::state::{ClientState, State};
use crate::window::round;

pub(crate) mod protocol {
    pub use infinity_protocol::server::infinity_window_manager_v1::infinity_window_manager_v1;
}

use protocol::infinity_window_manager_v1::InfinityWindowManagerV1;
use protocol::infinity_window_manager_v1::{Layout, PairLayout};

#[derive(Debug, Clone)]
pub struct InfinityGlobalData;

pub(crate) fn is_trusted_client(stream: &UnixStream) -> bool {
    static TRUSTED: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    let Ok(credentials) = getsockopt(stream, sockopt::PeerCredentials) else {
        return false;
    };
    let pid = credentials.pid();
    let Ok(exe) = std::fs::canonicalize(format!("/proc/{pid}/exe")) else {
        return false;
    };
    let trusted = TRUSTED.get_or_init(trusted_shell_exes);
    let trusted_shell = trusted.contains(&exe) && !preloads_code(pid);
    tracing::debug!(pid, exe = ?exe, trusted = ?trusted, trusted_shell, "infinity trust check");
    trusted_shell
}

// The shell binary is only trustworthy if the dynamic loader did not inject code into it.
fn preloads_code(pid: i32) -> bool {
    std::fs::read(format!("/proc/{pid}/environ")).map_or(true, |environ| {
        environ
            .split(|byte| *byte == 0)
            .any(|var| var.starts_with(b"LD_PRELOAD=") || var.starts_with(b"LD_AUDIT="))
    })
}

fn trusted_shell_exes() -> Vec<PathBuf> {
    let mut exes = Vec::new();
    if let Some(path) = std::env::var_os("MALAYSIA_SHELL_EXE")
        && let Ok(path) = std::fs::canonicalize(path)
    {
        exes.push(path);
    }
    if let Some(dir) = crate::spawn::bin_dir() {
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
        if manager.version() >= 5 {
            let (index, count, desktop, focus_bar) = state.space_state();
            manager.space(index, count, desktop, focus_bar);
        }
        state.infinity_managers.push(manager);
        let ids: Vec<WindowId> = state.windows.iter().map(|window| window.id()).collect();
        for id in ids {
            state.send_menus_to_client(id, client.id());
        }
    }

    fn can_view(&self, client: &Client) -> bool {
        client
            .get_data::<ClientState>()
            .is_some_and(|state| state.trusted_shell)
    }
}

impl smithay::wayland::Dispatch2<InfinityWindowManagerV1, State> for InfinityGlobalData {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _resource: &InfinityWindowManagerV1,
        request: <InfinityWindowManagerV1 as Resource>::Request,
        dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        use protocol::infinity_window_manager_v1::Request;
        if state.locked {
            return;
        }
        match request {
            Request::Arrange { handle, layout } => {
                if let Some(id) = window_id_from_handle(&handle) {
                    state.infinity_arrange(id, layout.into_result().unwrap_or(Layout::Fill));
                }
            }
            Request::ArrangePair { layout } => {
                state.infinity_arrange_pair(layout.into_result().unwrap_or(PairLayout::SideBySide));
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
            Request::ForceQuit { handle } => {
                if let Some(id) = window_id_from_handle(&handle) {
                    state.infinity_force_quit(id, dhandle);
                }
            }
            _ => {}
        }
    }

    fn destroyed(&self, state: &mut State, _client: ClientId, resource: &InfinityWindowManagerV1) {
        let id = resource.id();
        state.infinity_managers.retain(|m| m.id() != id);
    }
}

fn window_id_from_handle(handle: &ZwlrForeignToplevelHandleV1) -> Option<WindowId> {
    handle.data::<WindowId>().copied()
}

impl InfinityGlobalData {
    pub fn create_global(display_handle: &DisplayHandle) -> GlobalId {
        display_handle.create_global::<State, InfinityWindowManagerV1, _>(5, InfinityGlobalData)
    }
}

impl State {
    pub(crate) fn infinity_force_quit(&mut self, id: WindowId, dhandle: &DisplayHandle) {
        let Some(client) = self
            .windows
            .surface_of(id)
            .and_then(|surface| surface.client())
        else {
            return;
        };
        let Ok(credentials) = client.get_credentials(dhandle) else {
            tracing::warn!(window = id.get(), "force quit: no client credentials");
            return;
        };
        let pid = credentials.pid;
        if !may_force_quit(pid, std::process::id()) {
            tracing::warn!(window = id.get(), pid, "force quit refused");
            return;
        }
        match nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGKILL,
        ) {
            Ok(()) => info!(window = id.get(), pid, "force quit: SIGKILL sent"),
            Err(error) => tracing::warn!(window = id.get(), pid, %error, "force quit failed"),
        }
    }

    pub(crate) fn infinity_arrange(&mut self, id: WindowId, layout: Layout) {
        info!(window = id.get(), ?layout, "infinity arrange");
        let Some(outer) = self.windows.outer(id) else {
            return;
        };
        if self.in_focus_mode(id) {
            info!(
                window = id.get(),
                "arrange refused: a full-screen window stays put"
            );
            return;
        }
        if self.maximized.contains_key(&id) {
            self.set_maximized(id, false);
        }
        self.arranged.entry(id).or_insert_with(|| outer);
        let workspace = self.workspace_rect();
        let current_size = Size::from((round(outer.width()), round(outer.height())));
        let target = arranged_rect(workspace, layout, current_size);
        self.apply_infinity_geometry(id, target);
        self.send_restorable(id);
    }

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

    pub(crate) fn infinity_restore(&mut self, id: WindowId) {
        let Some(saved) = self.arranged.remove(&id) else {
            return;
        };
        info!(window = id.get(), outer = ?saved, "infinity restore");
        self.apply_infinity_geometry(id, saved);
        self.send_restorable(id);
    }

    pub(crate) fn space_state(
        &self,
    ) -> (
        u32,
        u32,
        u32,
        protocol::infinity_window_manager_v1::FocusBar,
    ) {
        use protocol::infinity_window_manager_v1::FocusBar;
        let current = self.spaces.current();
        let desktop = self.spaces.desktop_number(current.id).unwrap_or(0);
        let focus_bar = match current.kind {
            malaysia_scene::spaces::SpaceKind::Desktop => FocusBar::None,
            malaysia_scene::spaces::SpaceKind::Focus { .. } if self.focus_reveal.is_shown() => {
                FocusBar::Revealed
            }
            malaysia_scene::spaces::SpaceKind::Focus { .. } => FocusBar::Hidden,
        };
        (
            self.spaces.current_index() as u32,
            self.spaces.len() as u32,
            desktop as u32,
            focus_bar,
        )
    }

    pub(crate) fn send_space_state(&mut self) {
        let state = self.space_state();
        if self.last_space_state == Some(state) {
            return;
        }
        self.last_space_state = Some(state);
        let (index, count, desktop, focus_bar) = state;
        self.infinity_managers.retain(|manager| manager.is_alive());
        for manager in &self.infinity_managers {
            if manager.version() >= 5 {
                manager.space(index, count, desktop, focus_bar);
            }
        }
    }

    pub(crate) fn infinity_set_fullscreen(&mut self, id: WindowId, fullscreen: bool) {
        info!(window = id.get(), fullscreen, "infinity set fullscreen");
        if fullscreen {
            self.enter_focus_mode(id, false);
        } else {
            self.exit_focus_mode(id);
        }
    }

    fn apply_infinity_geometry(&mut self, id: WindowId, target: SceneRect<f64, SceneLogical>) {
        if self.in_focus_mode(id) {
            info!(
                window = id.get(),
                "infinity geometry refused: a full-screen window stays put"
            );
            return;
        }
        info!(window = id.get(), outer = ?target, "infinity geometry");
        self.windows.set_infinity_geometry(id, target);
        self.foreign_state_changed(id);
        self.request_redraw();
    }

    fn workspace_rect(&self) -> Rectangle<i32, Logical> {
        let (origin, size) = self.workspace();
        Rectangle::new(origin, size)
    }

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

    pub(crate) fn send_app_menu(&mut self, id: WindowId) {
        let address = self.appmenu_address(id).unwrap_or_default();
        self.infinity_managers.retain(|manager| manager.is_alive());
        self.send_window_event(id, None, 2, |manager, handle| {
            app_menu_event(manager, handle, &address)
        });
    }

    pub(crate) fn send_gtk_menus(&mut self, id: WindowId) {
        let address = self.gtk_menus_address(id).unwrap_or_default();
        self.infinity_managers.retain(|manager| manager.is_alive());
        self.send_window_event(id, None, 3, |manager, handle| {
            gtk_menus_event(manager, handle, &address)
        });
    }

    pub(crate) fn send_menus_to_client(&self, id: WindowId, client: ClientId) {
        if let Some(address) = self.appmenu_address(id) {
            self.send_window_event(id, Some(&client), 2, |manager, handle| {
                app_menu_event(manager, handle, &address)
            });
        }
        if let Some(address) = self.gtk_menus_address(id) {
            self.send_window_event(id, Some(&client), 3, |manager, handle| {
                gtk_menus_event(manager, handle, &address)
            });
        }
    }

    fn send_window_event(
        &self,
        id: WindowId,
        client: Option<&ClientId>,
        since: u32,
        send: impl Fn(&InfinityWindowManagerV1, &ZwlrForeignToplevelHandleV1),
    ) {
        for manager in &self.infinity_managers {
            let Some(owner) = manager.client().map(|owner| owner.id()) else {
                continue;
            };
            if manager.version() < since || client.is_some_and(|client| *client != owner) {
                continue;
            }
            if let Some(handle) = self.foreign.management.handle_for_client(id, owner) {
                send(manager, handle);
            }
        }
    }
}

fn app_menu_event(
    manager: &InfinityWindowManagerV1,
    handle: &ZwlrForeignToplevelHandleV1,
    address: &AppMenuAddress,
) {
    manager.app_menu(handle, address.service.clone(), address.path.clone());
}

fn gtk_menus_event(
    manager: &InfinityWindowManagerV1,
    handle: &ZwlrForeignToplevelHandleV1,
    address: &GtkMenusAddress,
) {
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

fn may_force_quit(pid: i32, own_pid: u32) -> bool {
    pid > 1 && u32::try_from(pid).ok() != Some(own_pid)
}

fn available(workspace: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((workspace.loc.x + TILE_MARGIN, workspace.loc.y + TILE_MARGIN)),
        Size::from((
            (workspace.size.w - 2 * TILE_MARGIN).max(0),
            (workspace.size.h - 2 * TILE_MARGIN).max(0),
        )),
    )
}

struct Tiles {
    area: Rectangle<i32, Logical>,
    half_w: i32,
    half_h: i32,
}

impl Tiles {
    fn new(workspace: Rectangle<i32, Logical>) -> Self {
        let area = available(workspace);
        Self {
            area,
            half_w: (area.size.w - TILE_GAP) / 2,
            half_h: (area.size.h - TILE_GAP) / 2,
        }
    }

    fn rect(&self, dx: i32, dy: i32, w: i32, h: i32) -> SceneRect<f64, SceneLogical> {
        SceneRect::new(
            ScenePoint::new(
                f64::from(self.area.loc.x + dx),
                f64::from(self.area.loc.y + dy),
            ),
            SceneSize::new(f64::from(w), f64::from(h)),
        )
    }

    fn column(&self, right: bool, h: i32, dy: i32) -> SceneRect<f64, SceneLogical> {
        let dx = if right { self.half_w + TILE_GAP } else { 0 };
        self.rect(dx, dy, self.half_w, h)
    }

    fn row(&self, bottom: bool) -> SceneRect<f64, SceneLogical> {
        let dy = if bottom { self.half_h + TILE_GAP } else { 0 };
        self.rect(0, dy, self.area.size.w, self.half_h)
    }

    fn quarter(&self, right: bool, bottom: bool) -> SceneRect<f64, SceneLogical> {
        let dy = if bottom { self.half_h + TILE_GAP } else { 0 };
        self.column(right, self.half_h, dy)
    }

    fn half(&self, right: bool) -> SceneRect<f64, SceneLogical> {
        self.column(right, self.area.size.h, 0)
    }

    fn centered(&self, size: Size<i32, Logical>) -> SceneRect<f64, SceneLogical> {
        let w = size.w.min(self.area.size.w);
        let h = size.h.min(self.area.size.h);
        self.rect((self.area.size.w - w) / 2, (self.area.size.h - h) / 2, w, h)
    }
}

fn arranged_rect(
    workspace: Rectangle<i32, Logical>,
    layout: Layout,
    current_size: Size<i32, Logical>,
) -> SceneRect<f64, SceneLogical> {
    let tiles = Tiles::new(workspace);
    match layout {
        Layout::Fill => tiles.rect(0, 0, tiles.area.size.w, tiles.area.size.h),
        Layout::Left => tiles.half(false),
        Layout::Right => tiles.half(true),
        Layout::Top => tiles.row(false),
        Layout::Bottom => tiles.row(true),
        Layout::TopLeft => tiles.quarter(false, false),
        Layout::TopRight => tiles.quarter(true, false),
        Layout::BottomLeft => tiles.quarter(false, true),
        Layout::BottomRight => tiles.quarter(true, true),
        _ => tiles.centered(current_size),
    }
}

fn pair_rects(
    workspace: Rectangle<i32, Logical>,
    layout: PairLayout,
) -> [SceneRect<f64, SceneLogical>; 2] {
    let tiles = Tiles::new(workspace);
    match layout {
        PairLayout::SideBySideReversed => [tiles.half(true), tiles.half(false)],
        PairLayout::Stacked => [tiles.row(false), tiles.row(true)],
        PairLayout::StackedReversed => [tiles.row(true), tiles.row(false)],
        _ => [tiles.half(false), tiles.half(true)],
    }
}

fn quarter_rects(workspace: Rectangle<i32, Logical>) -> [SceneRect<f64, SceneLogical>; 4] {
    let tiles = Tiles::new(workspace);
    [
        tiles.quarter(false, false),
        tiles.quarter(true, false),
        tiles.quarter(false, true),
        tiles.quarter(true, true),
    ]
}

#[cfg(test)]
mod tests {
    use malaysia_scene::geometry::{Logical as SceneLogical, Rect as SceneRect};
    use smithay::utils::{Logical, Point, Rectangle, Size};

    use super::{
        Layout, PairLayout, arranged_rect, available, may_force_quit, pair_rects, quarter_rects,
        trusted_shell_exes,
    };

    fn workspace() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 32)), Size::from((1280, 690)))
    }

    fn xywh(rect: SceneRect<f64, SceneLogical>) -> (f64, f64, f64, f64) {
        (rect.x(), rect.y(), rect.width(), rect.height())
    }

    fn arranged(layout: Layout) -> (f64, f64, f64, f64) {
        xywh(arranged_rect(workspace(), layout, Size::from((640, 420))))
    }

    #[test]
    fn force_quit_never_targets_the_compositor_or_init() {
        assert!(may_force_quit(4242, 100));
        assert!(!may_force_quit(100, 100));
        assert!(!may_force_quit(1, 100));
        assert!(!may_force_quit(0, 100));
        assert!(!may_force_quit(-1, 100));
    }

    #[test]
    fn the_available_area_leaves_the_tile_margin() {
        let rect = available(workspace());
        assert_eq!(rect.loc, Point::from((8, 40)));
        assert_eq!(rect.size, Size::from((1264, 674)));
        assert_eq!(arranged(Layout::Fill), (8.0, 40.0, 1264.0, 674.0));
    }

    #[test]
    fn center_keeps_the_current_size_clamped_to_the_available_area() {
        assert_eq!(arranged(Layout::Center), (320.0, 167.0, 640.0, 420.0));
        let large = arranged_rect(workspace(), Layout::Center, Size::from((2000, 1000)));
        assert_eq!((large.width(), large.height()), (1264.0, 674.0));
    }

    #[test]
    fn halves_and_quarters_are_split_by_the_tile_gap() {
        assert_eq!(arranged(Layout::Left), (8.0, 40.0, 628.0, 674.0));
        assert_eq!(arranged(Layout::Right), (644.0, 40.0, 628.0, 674.0));
        assert_eq!(arranged(Layout::TopLeft), (8.0, 40.0, 628.0, 333.0));
        assert_eq!(arranged(Layout::BottomRight), (644.0, 381.0, 628.0, 333.0));
        assert_eq!(
            quarter_rects(workspace()).map(xywh),
            [
                (8.0, 40.0, 628.0, 333.0),
                (644.0, 40.0, 628.0, 333.0),
                (8.0, 381.0, 628.0, 333.0),
                (644.0, 381.0, 628.0, 333.0),
            ]
        );
    }

    #[test]
    fn pairs_put_the_active_window_first() {
        let [left, right] = pair_rects(workspace(), PairLayout::SideBySide);
        assert_eq!((left.x(), right.x()), (8.0, 644.0));
        let [top, bottom] = pair_rects(workspace(), PairLayout::Stacked);
        assert_eq!(xywh(top), (8.0, 40.0, 1264.0, 333.0));
        assert_eq!(bottom.y(), 381.0);
        let [first, second] = pair_rects(workspace(), PairLayout::StackedReversed);
        assert_eq!((first.y(), second.y()), (381.0, 40.0));
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
            exes.contains(&std::fs::canonicalize(&fake).unwrap()),
            "MALAYSIA_SHELL_EXE must be trusted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
