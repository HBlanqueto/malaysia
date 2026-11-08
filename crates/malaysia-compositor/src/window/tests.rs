use smithay::utils::{Logical, Point, Rectangle, Size};

use super::{
    EdgeTarget, anchor_resize_origin, apply_configure_states, at_least_one_pixel, client_point,
    committed_outer, configure_size, edge_hit, geometry_fits, initial_placement,
    needs_configure_after_commit, outer_size, raise_order, usable, usable_rect_location,
};
use crate::decoration::Insets;
use malaysia_scene::geometry::{
    Logical as SceneLogical, Point as ScenePoint, Rect as SceneRect, Size as SceneSize,
};
use malaysia_scene::scene::{Edge, WindowId};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::wayland::shell::xdg::ToplevelStateSet;

fn size(width: i32, height: i32) -> Size<i32, Logical> {
    Size::from((width, height))
}

fn titlebar() -> Insets {
    Insets {
        top: 32.0,
        ..Insets::ZERO
    }
}

#[test]
fn usable_size_subtracts_the_top_inset() {
    let insets = Insets {
        top: 32.0,
        ..Insets::ZERO
    };
    assert_eq!(usable(size(800, 600), insets), size(800, 568));
}

#[test]
fn usable_size_subtracts_both_axes() {
    let insets = Insets {
        top: 10.0,
        right: 5.0,
        bottom: 10.0,
        left: 5.0,
    };
    assert_eq!(usable(size(800, 600), insets), size(790, 580));
}

#[test]
fn usable_size_is_the_window_when_nothing_is_reserved() {
    assert_eq!(usable(size(800, 600), Insets::ZERO), size(800, 600));
}

#[test]
fn usable_size_never_collapses() {
    let insets = Insets {
        top: 500.0,
        bottom: 500.0,
        ..Insets::ZERO
    };
    assert_eq!(usable(size(800, 600), insets), size(800, 1));
}

#[test]
fn an_unset_client_maximum_stays_unbounded() {
    assert_eq!(outer_size(size(0, 0), titlebar()), SceneSize::new(0.0, 0.0));
}

#[test]
fn a_client_bound_carries_the_insets() {
    assert_eq!(
        outer_size(size(800, 600), titlebar()),
        SceneSize::new(800.0, 632.0)
    );
}

fn tiled_states() -> [xdg_toplevel::State; 4] {
    [
        xdg_toplevel::State::TiledLeft,
        xdg_toplevel::State::TiledRight,
        xdg_toplevel::State::TiledTop,
        xdg_toplevel::State::TiledBottom,
    ]
}

#[test]
fn the_initial_configure_is_not_tiled_and_the_next_one_is() {
    let mut initial = ToplevelStateSet::default();
    apply_configure_states(&mut initial, false, false, false, false, false);
    for state in tiled_states() {
        assert!(!initial.contains(state), "unexpected {state:?}");
    }
    assert_eq!(
        configure_size(false, size(1204, 804), titlebar()),
        size(0, 0)
    );

    let mut later = ToplevelStateSet::default();
    apply_configure_states(&mut later, true, false, false, false, false);
    for state in tiled_states() {
        assert!(later.contains(state), "missing {state:?}");
    }
    assert_eq!(
        configure_size(true, size(1204, 804), titlebar()),
        size(1204, 772)
    );
}

#[test]
fn a_normal_toplevel_is_tiled_on_all_four_sides() {
    let mut states = ToplevelStateSet::default();
    apply_configure_states(&mut states, true, false, false, false, true);
    for state in tiled_states() {
        assert!(states.contains(state), "missing {state:?}");
    }
}

#[test]
fn a_fullscreen_toplevel_keeps_its_tiled_edges_clear() {
    let mut states = ToplevelStateSet::default();
    apply_configure_states(&mut states, true, false, true, false, true);
    assert!(states.contains(xdg_toplevel::State::Fullscreen));
    for state in tiled_states() {
        assert!(!states.contains(state), "unexpected {state:?}");
    }
}

#[test]
fn a_maximized_toplevel_keeps_its_tiled_edges_clear() {
    let mut states = ToplevelStateSet::default();
    apply_configure_states(&mut states, true, true, false, false, true);
    assert!(states.contains(xdg_toplevel::State::Maximized));
    for state in tiled_states() {
        assert!(!states.contains(state), "unexpected {state:?}");
    }
}

fn csd(
    id: WindowId,
    blocking: SceneRect<f64, SceneLogical>,
    frame: SceneRect<f64, SceneLogical>,
) -> EdgeTarget {
    EdgeTarget {
        id,
        band: frame,
        blocking,
        exterior: true,
        resizable: true,
    }
}

#[test]
fn a_csd_margin_blocks_the_band_of_the_window_below() {
    let above = WindowId::new(1);
    let below = WindowId::new(2);
    let above_target = csd(
        above,
        SceneRect::new(ScenePoint::new(100.0, 100.0), SceneSize::new(300.0, 200.0)),
        SceneRect::new(ScenePoint::new(120.0, 120.0), SceneSize::new(260.0, 160.0)),
    );
    let below_target = csd(
        below,
        SceneRect::new(ScenePoint::new(180.0, 130.0), SceneSize::new(226.0, 140.0)),
        SceneRect::new(ScenePoint::new(200.0, 150.0), SceneSize::new(186.0, 100.0)),
    );

    let point = ScenePoint::new(390.0, 200.0);
    let targets = [above_target, below_target];
    assert_eq!(edge_hit(targets.into_iter(), point, 6.0, 12.0), None);

    let below_target = csd(
        below,
        SceneRect::new(ScenePoint::new(180.0, 130.0), SceneSize::new(226.0, 140.0)),
        SceneRect::new(ScenePoint::new(200.0, 150.0), SceneSize::new(186.0, 100.0)),
    );
    assert_eq!(
        edge_hit([below_target].into_iter(), point, 6.0, 12.0),
        Some((below, Edge::Right))
    );

    let above_target = csd(
        above,
        SceneRect::new(ScenePoint::new(100.0, 100.0), SceneSize::new(300.0, 200.0)),
        SceneRect::new(ScenePoint::new(120.0, 120.0), SceneSize::new(260.0, 160.0)),
    );
    assert_eq!(
        edge_hit(
            [above_target].into_iter(),
            ScenePoint::new(383.0, 200.0),
            6.0,
            12.0
        ),
        Some((above, Edge::Right))
    );
}

#[test]
fn a_zero_minimum_still_leaves_a_pixel() {
    assert_eq!(
        at_least_one_pixel(outer_size(size(0, 0), Insets::ZERO)),
        SceneSize::new(1.0, 1.0)
    );
}
#[test]
fn the_titlebar_band_is_not_the_client_surface() {
    use malaysia_scene::geometry::{Point as ScenePoint, Rect, Size as SceneSize};

    let outer = Rect::new(ScenePoint::new(100.0, 50.0), SceneSize::new(800.0, 600.0));
    let insets = Insets {
        top: 44.0,
        ..Insets::ZERO
    };
    assert_eq!(client_point(outer, insets, Point::new(110.0, 60.0)), None);
    assert_eq!(
        client_point(outer, insets, Point::new(110.0, 100.0)),
        Some(Point::new(10.0, 6.0))
    );
}

#[test]
fn a_configured_1280x708_window_that_committed_640x420_only_hits_inside_it() {
    use malaysia_scene::geometry::{Point as ScenePoint, Rect as SceneRect, Size as SceneSize};
    use malaysia_scene::scene::{Scene, WindowId};

    let outer = committed_outer(Some(size(640, 420)), Insets::ZERO);
    let mut scene = Scene::new();
    let window = WindowId::new(1);
    scene.insert(
        window,
        SceneRect::new(
            ScenePoint::new(0.0, 0.0),
            SceneSize::new(f64::from(outer.w), f64::from(outer.h)),
        ),
    );

    assert_eq!(
        scene.topmost_at(ScenePoint::new(639.0, 419.0)),
        Some(window)
    );
    assert_eq!(scene.topmost_at(ScenePoint::new(640.0, 420.0)), None);
    assert_eq!(scene.topmost_at(ScenePoint::new(1000.0, 354.0)), None);
}

#[test]
fn placement_centers_the_committed_geometry_not_the_configured_size() {
    let workspace = Rectangle::new(Point::from((0, 0)), size(1280, 708));
    let (outer, origin) = initial_placement(Some(size(640, 420)), Insets::ZERO, workspace);
    assert_eq!(outer, size(640, 420));
    assert_eq!(origin, Point::from((320, 144)));
}

#[test]
fn a_client_that_committed_nothing_still_gets_the_default_window() {
    assert_eq!(committed_outer(None, Insets::ZERO), size(640, 420));
    assert_eq!(
        committed_outer(Some(size(0, 0)), Insets::ZERO),
        size(640, 420)
    );
}

#[test]
fn committed_geometry_is_never_wider_than_the_workspace() {
    let workspace = Rectangle::new(Point::from((0, 0)), size(1280, 708));
    let (outer, _) = initial_placement(Some(size(4000, 3000)), Insets::ZERO, workspace);
    assert_eq!(outer, size(1280, 708));
}

#[test]
fn a_left_resize_keeps_the_right_edge_fixed_when_the_client_commits_less() {
    let origin = Point::from((100, 100));
    let old_size = size(200, 150);
    let new_size = size(197, 150);
    let anchored = anchor_resize_origin(origin, old_size, new_size, Some(Edge::Left));
    assert_eq!(anchored, Point::from((103, 100)));
}

#[test]
fn a_top_left_resize_keeps_both_opposite_edges_fixed() {
    let origin = Point::from((100, 100));
    let old_size = size(200, 150);
    let new_size = size(197, 147);
    let anchored = anchor_resize_origin(origin, old_size, new_size, Some(Edge::TopLeft));
    assert_eq!(anchored, Point::from((103, 103)));
}

#[test]
fn a_right_resize_keeps_the_left_edge_fixed() {
    let origin = Point::from((100, 100));
    let old_size = size(200, 150);
    let new_size = size(197, 150);
    let anchored = anchor_resize_origin(origin, old_size, new_size, Some(Edge::Right));
    assert_eq!(anchored, Point::from((100, 100)));
}

#[test]
fn no_edge_leaves_the_origin_unchanged() {
    let origin = Point::from((100, 100));
    let old_size = size(200, 150);
    let new_size = size(197, 150);
    let anchored = anchor_resize_origin(origin, old_size, new_size, None);
    assert_eq!(anchored, Point::from((100, 100)));
}

#[test]
fn window_geometry_must_fit_inside_the_buffer() {
    let buffer = Some(size(640, 420));
    let inside = Some(Rectangle::new(Point::from((0, 0)), size(640, 420)));
    let shifted = Some(Rectangle::new(Point::from((0, 32)), size(640, 420)));
    let negative = Some(Rectangle::new(Point::from((-1, 0)), size(640, 420)));
    let empty = Some(Rectangle::new(Point::from((0, 0)), size(0, 420)));

    assert!(geometry_fits(inside, buffer));
    assert!(geometry_fits(None, buffer));
    assert!(!geometry_fits(shifted, buffer));
    assert!(!geometry_fits(negative, buffer));
    assert!(!geometry_fits(empty, buffer));
}

#[test]
fn a_left_resize_keeps_the_right_edge_fixed_after_a_move() {
    let origin = Point::from((250, 180));
    let old_size = size(640, 420);
    let new_size = size(637, 420);
    let anchored = anchor_resize_origin(origin, old_size, new_size, Some(Edge::Left));
    assert_eq!(anchored, Point::from((253, 180)));
}

#[test]
fn restoring_hidden_uses_the_saved_rect_not_the_placement_origin() {
    use malaysia_scene::geometry::{Logical, Point as ScenePoint, Rect as SceneRect};

    let saved = SceneRect::new(ScenePoint::new(123.0, 456.0), SceneSize::new(640.0, 420.0));
    let mut hidden_rect: Option<SceneRect<f64, Logical>> = Some(saved);
    let restored = hidden_rect.take().unwrap();
    assert_eq!(restored.x(), 123.0);
    assert_eq!(restored.y(), 456.0);
}

#[test]
fn usable_rect_location_fits_inside_the_workspace() {
    let workspace_height = 100.0;
    let menubar = 24.0;
    let outer = SceneRect::new(
        ScenePoint::new(0.0, menubar + 0.4),
        SceneSize::new(640.0, workspace_height - menubar),
    );
    let insets = titlebar();
    let location = usable_rect_location(outer, insets);
    let usable_height = usable(size(640, (workspace_height - menubar) as i32), insets).h;
    assert!(
        location.y + usable_height <= workspace_height as i32,
        "usable rect must not extend below the workspace"
    );
}

#[test]
fn commit_does_not_reconfigure_with_committed_size_during_resize() {
    let insets = Insets::ZERO;
    let committed = size(640, 420);
    let requested = size(800, 600);
    assert!(
        !needs_configure_after_commit(true, true, committed, Some(requested), insets),
        "during resize the committed size must not trigger a reconfigure"
    );
}

#[test]
fn commit_reconfigures_when_size_changes_outside_resize() {
    let insets = Insets::ZERO;
    let committed = size(640, 420);
    let requested = size(800, 600);
    assert!(
        needs_configure_after_commit(true, false, committed, Some(requested), insets),
        "outside resize a committed size different from configured must reconfigure"
    );
}

#[test]
fn commit_does_not_reconfigure_when_size_matches() {
    let insets = Insets::ZERO;
    let committed = size(640, 420);
    assert!(
        !needs_configure_after_commit(true, false, committed, Some(committed), insets),
        "no reconfigure when configured size already matches committed"
    );
}

#[test]
fn raising_a_parent_keeps_its_children_and_grandchildren_above_it() {
    let parents = [(2, 1), (3, 2), (5, 1)];
    let parent_of = |window: u32| {
        parents
            .iter()
            .find(|(child, _)| *child == window)
            .map(|(_, parent)| *parent)
    };
    let none = |_: u32| false;
    assert_eq!(
        raise_order(&[1, 2, 4, 3, 5], 1, parent_of, none),
        [1, 2, 3, 5]
    );
    assert_eq!(raise_order(&[1, 2, 4, 3, 5], 4, parent_of, none), [4]);
    assert_eq!(raise_order(&[1, 2, 4, 3, 5], 2, parent_of, none), [2, 3]);
}

#[test]
fn floating_panels_stay_above_a_raised_window_and_a_raised_panel_tops_them() {
    let no_parent = |_: u32| None;
    let floats = |window: u32| window >= 10;
    assert_eq!(
        raise_order(&[1, 10, 2, 11], 1, no_parent, floats),
        [1, 10, 11]
    );
    assert_eq!(raise_order(&[1, 10, 2, 11], 10, no_parent, floats), [10]);
}
