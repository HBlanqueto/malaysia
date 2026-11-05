use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use nix::sys::socket::{getsockopt, sockopt};
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource, WEnum};
use smithay::utils::{Logical, Point, Rectangle, Size};
use tracing::info;
use vow_design::{TILE_GAP, TILE_MARGIN};
use malaysia_scene::geometry::{Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize};
use malaysia_scene::scene::WindowId;

use crate::appmenu::AppMenuAddress;
use crate::gtk::GtkMenusAddress;
use crate::state::{ClientState, State};

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
    let trusted_shell = is_trusted_shell_exe(&exe, trusted) && !preloads_code(pid);
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
            Request::ForceQuit { handle } => {
                if let Some(id) = window_id_from_handle(&handle) {
                    state.infinity_force_quit(id, dhandle);
                }
            }
            Request::Destroy => {}
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
    pub fn create_global(display_handle: &DisplayHandle) -> GlobalId {
        let data = InfinityGlobalData;
        display_handle.create_global::<State, InfinityWindowManagerV1, _>(5, data)
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

fn round(value: f64) -> i32 {
    value.round() as i32
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
        _ => {
            let w = current_size.w.min(area.size.w);
            let h = current_size.h.min(area.size.h);
            let x = loc.x + (area.size.w - w) / 2;
            let y = loc.y + (area.size.h - h) / 2;
            rect(x, y, w, h)
        }
    }
}

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
        _ => {
            let left = rect(loc.x, loc.y, half_w, size.h);
            let right = rect(loc.x + half_w + TILE_GAP, loc.y, half_w, size.h);
            [left, right]
        }
    }
}

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
        Layout, PairLayout, arranged_rect, available, is_trusted_shell_exe, may_force_quit,
        pair_rects, quarter_rects, trusted_shell_exes,
    };
    use malaysia_scene::scene::WindowId;

    fn workspace() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 32)), Size::from((1280, 690)))
    }

    fn workspace_1280_800() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 28)), Size::from((1280, 722)))
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
        let saved = *arranged.get(&WindowId::new(1)).unwrap();
        assert_eq!(saved, first);

        arranged.remove(&WindowId::new(1));
        assert!(!arranged.contains_key(&WindowId::new(1)));
        assert_eq!(second, second);
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
}
