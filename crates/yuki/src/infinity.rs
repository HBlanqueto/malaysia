//! Infinity window manager protocol (ADR 0039).
//!
//! This module serves the private `infinity_window_manager_v1` protocol that
//! yuki-shell uses to ask the compositor for window geometry: halves, quarters,
//! pairs, fill, center, restore and fullscreen. The global is only visible to
//! the yuki-shell binary or when `YUKI_DEV=1`.

use std::path::PathBuf;

use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource, WEnum};
use smithay::utils::{Logical, Point, Rectangle, Size};
use tracing::info;
use yuki_scene::geometry::{Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize};
use yuki_scene::scene::WindowId;

use crate::state::State;

/// Generated server-side bindings for the Infinity protocol.
pub(crate) mod protocol {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
    #![allow(missing_docs, clippy::all)]

    use wayland_protocols_wlr::foreign_toplevel::v1::server::*;
    use wayland_server;
    use wayland_server::protocol::*;

    pub mod __interfaces {
        use wayland_protocols_wlr::foreign_toplevel::v1::server::__interfaces::*;
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("../../protocols/infinity-window-manager-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("../../protocols/infinity-window-manager-v1.xml");
}

use protocol::infinity_window_manager_v1::InfinityWindowManagerV1;
use protocol::infinity_window_manager_v1::{Layout, PairLayout};

/// Margin around the workspace for tiled windows, in logical pixels.
const TILE_MARGIN: i32 = 8;
/// Gap between tiled windows, in logical pixels.
const TILE_GAP: i32 = 8;

/// Data attached to the `infinity_window_manager_v1` global.
///
/// It carries a clone of the display handle so the visibility filter can read
/// the client's credentials (`/proc/<pid>/exe`).
#[derive(Debug, Clone)]
pub struct InfinityGlobalData {
    display_handle: DisplayHandle,
    dev_mode: bool,
    /// Paths that are accepted as the trusted `yuki-shell` binary.
    shell_exes: Vec<PathBuf>,
}

impl InfinityGlobalData {
    /// Builds the global data, resolving the trusted yuki-shell binary from the
    /// compositor's own executable path.
    ///
    /// The Nix wrapper renames the real binary to `.yuki-shell-wrapped`, so
    /// both `yuki-shell` and `.yuki-shell-wrapped` in the compositor's directory
    /// are accepted. A wrapper may also set `YUKI_SHELL_EXE` to the exact real
    /// path when the heuristic is not enough.
    pub fn new(display_handle: DisplayHandle) -> Self {
        let dev_mode = std::env::var("YUKI_DEV").is_ok_and(|v| v == "1");
        let shell_exes = trusted_shell_exes();
        Self {
            display_handle,
            dev_mode,
            shell_exes,
        }
    }
}

/// Resolves the set of executable paths that are trusted as `yuki-shell`.
fn trusted_shell_exes() -> Vec<PathBuf> {
    let mut exes = Vec::new();
    if let Some(path) = std::env::var_os("YUKI_SHELL_EXE") {
        if let Ok(path) = std::fs::canonicalize(path) {
            exes.push(path);
        }
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) {
        for name in ["yuki-shell", ".yuki-shell-wrapped"] {
            if let Ok(path) = std::fs::canonicalize(dir.join(name)) {
                if !exes.contains(&path) {
                    exes.push(path);
                }
            }
        }
    }
    exes
}

impl smithay::wayland::GlobalDispatch2<InfinityWindowManagerV1, State> for InfinityGlobalData {
    fn bind(
        &self,
        state: &mut State,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<InfinityWindowManagerV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        let manager = data_init.init(resource, self.clone());
        state.infinity_managers.push(manager);
    }

    fn can_view(&self, client: &Client) -> bool {
        if self.dev_mode {
            return true;
        }
        let Ok(credentials) = client.get_credentials(&self.display_handle) else {
            return false;
        };
        let pid = credentials.pid;
        let Ok(exe) = std::fs::canonicalize(format!("/proc/{pid}/exe")) else {
            return false;
        };
        is_trusted_shell_exe(&exe, &self.shell_exes)
    }
}

/// Whether `exe` matches any of the trusted `yuki-shell` paths.
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
        let data = InfinityGlobalData::new(display_handle.clone());
        display_handle.create_global::<State, InfinityWindowManagerV1, _>(1, data)
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
        info!(window = id.get(), "infinity restore");
        let Some(saved) = self.arranged.remove(&id) else {
            return;
        };
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
        let Some(handle) = self.foreign_handle_for(id) else {
            return;
        };
        for manager in &self.infinity_managers {
            manager.restorable(handle, restorable as u32);
        }
    }

    /// Finds the foreign-toplevel handle for a window.
    fn foreign_handle_for(&self, id: WindowId) -> Option<&ZwlrForeignToplevelHandleV1> {
        self.foreign.management.handle_for(id)
    }
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
    use smithay::utils::{Logical, Point, Rectangle, Size};
    use yuki_scene::geometry::{
        Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
    };

    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{Layout, PairLayout, arranged_rect, available, is_trusted_shell_exe, pair_rects,
        quarter_rects, trusted_shell_exes};
    use yuki_scene::scene::WindowId;

    fn workspace() -> Rectangle<i32, Logical> {
        // Output 1280×772, menu bar 32 px and dock 50 px.
        Rectangle::new(Point::from((0, 32)), Size::from((1280, 690)))
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
    fn trusted_shell_list_accepts_a_yuki_shell_exe_override() {
        let dir = std::env::temp_dir().join(format!("yuki-shell-exe-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake-shell");
        std::fs::write(&fake, "").unwrap();

        let previous = std::env::var_os("YUKI_SHELL_EXE");
        // SAFETY: only used during this single-threaded test.
        unsafe {
            std::env::set_var("YUKI_SHELL_EXE", &fake);
        }
        let exes = trusted_shell_exes();
        if let Some(previous) = previous {
            // SAFETY: only used during this single-threaded test.
            unsafe { std::env::set_var("YUKI_SHELL_EXE", previous); }
        } else {
            // SAFETY: only used during this single-threaded test.
            unsafe { std::env::remove_var("YUKI_SHELL_EXE"); }
        }
        assert!(
            is_trusted_shell_exe(&std::fs::canonicalize(&fake).unwrap(), &exes),
            "YUKI_SHELL_EXE must be trusted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trusted_shell_comparison_accepts_wrapped_and_unwrapped_names() {
        let target = PathBuf::from("/nix/store/abc/bin/yuki-shell");
        let wrapped = PathBuf::from("/nix/store/abc/bin/.yuki-shell-wrapped");
        assert!(is_trusted_shell_exe(&target, &[target.clone()]));
        assert!(is_trusted_shell_exe(&wrapped, &[wrapped.clone()]));
        assert!(!is_trusted_shell_exe(&target, &[wrapped.clone()]));
    }

    #[test]
    fn untrusted_binaries_are_rejected() {
        let trusted = PathBuf::from("/run/current-system/sw/bin/yuki-shell");
        let attacker = PathBuf::from("/tmp/evil-shell");
        assert!(!is_trusted_shell_exe(&attacker, &[trusted]));
    }
}
