use bk::shell::MinimizeTarget;
use ctappearance::dock::DockConfig;
use ctdesign::{DOCK_EDGE_MARGIN, DOCK_RADIUS, DOCK_REVEAL_STRIP};

use super::geometry::{self, Slot};
use super::items::{self, Item};
use super::{Frame, Regions, region};

pub(crate) fn surface_regions(
    frame: Frame,
    layout: &geometry::Layout,
    (cross0, visible, reach): (f32, f32, f32),
    autohide: bool,
    lifted: bool,
) -> Regions {
    let bar = frame.area(
        layout.bar_start,
        cross0,
        layout.bar_length,
        layout.thickness,
    );
    let strip = frame.area(
        layout.bar_start,
        0.0,
        layout.bar_length,
        DOCK_REVEAL_STRIP as f32,
    );
    let column = frame.area(
        layout.bar_start,
        0.0,
        layout.bar_length,
        (cross0 + reach).max(0.0),
    );
    let input = if lifted {
        vec![(0.0, 0.0, frame.width, frame.height)]
    } else if !autohide {
        vec![column]
    } else if visible < 0.01 {
        vec![strip]
    } else {
        vec![strip, column]
    };
    (
        input.into_iter().map(region).collect(),
        if visible > 0.01 {
            vec![region(bar)]
        } else {
            Vec::new()
        },
        (layout.thickness * DOCK_RADIUS).round() as u32,
    )
}

pub(crate) fn minimize_targets(
    list: &[Item],
    slots: &[Slot],
    layout: &geometry::Layout,
    (frame, config): (Frame, &DockConfig),
    (pointer, magnify): (Option<f32>, f32),
) -> Vec<MinimizeTarget> {
    let mut future = None;
    items::minimize_slots(list, config.minimize_to_application)
        .into_iter()
        .map(|(window, index)| {
            let has_tile = matches!(list.get(index), Some(Item::Window(tile)) if tile.id == window);
            let layout: &geometry::Layout = if has_tile || config.minimize_to_application {
                layout
            } else {
                future.get_or_insert_with(|| {
                    future_layout(slots, index, (frame, config), (pointer, magnify))
                })
            };
            let slot = layout.placed[index];
            let rect = region(frame.area(
                slot.start + slot.length / 2.0 - slot.size / 2.0,
                DOCK_EDGE_MARGIN + layout.padding,
                slot.size,
                slot.size,
            ));
            MinimizeTarget { window, rect }
        })
        .collect()
}

fn future_layout(
    slots: &[Slot],
    trash: usize,
    (frame, config): (Frame, &DockConfig),
    (pointer, magnify): (Option<f32>, f32),
) -> geometry::Layout {
    let mut slots = slots.to_vec();
    slots.insert(trash, Slot::Wide(1.0));
    let tile = geometry::fit(config.tilesize(), frame.main_length(), &slots);
    geometry::layout(
        &slots,
        tile,
        config.largesize(),
        frame.main_length() / 2.0,
        pointer,
        magnify,
    )
}

#[cfg(test)]
mod tests {
    use bk::shell::ToplevelInfo;
    use ctappearance::dock::DockOrientation;

    use super::super::items::AppTile;
    use super::*;

    const TILE: f32 = 48.0;

    fn frame() -> Frame {
        Frame {
            orientation: DockOrientation::Bottom,
            width: 1000.0,
            height: 200.0,
        }
    }

    fn layout_of(slots: &[Slot]) -> geometry::Layout {
        geometry::layout(slots, TILE, TILE, 500.0, None, 0.0)
    }

    fn window(id: u64, minimized: bool) -> ToplevelInfo {
        ToplevelInfo {
            id,
            minimized,
            ..ToplevelInfo::default()
        }
    }

    fn app(windows: Vec<ToplevelInfo>) -> Item {
        Item::App(AppTile {
            id: "app".to_string(),
            kept: false,
            windows,
        })
    }

    fn centre_x(rect: (i32, i32, i32, i32)) -> f32 {
        rect.0 as f32 + rect.2 as f32 / 2.0
    }

    #[test]
    fn a_dock_that_stays_takes_the_pointer_from_the_screen_edge_up_to_its_reach() {
        let layout = layout_of(&[Slot::Tile, Slot::Tile]);
        let (input, glass, _) = surface_regions(frame(), &layout, (4.0, 1.0, 60.0), false, false);
        assert_eq!(input.len(), 1);
        let (_, y, _, height) = input[0];
        assert_eq!((y + height, height), (200, 64));
        assert_eq!(glass.len(), 1);
    }

    #[test]
    fn a_hidden_dock_takes_the_pointer_only_in_its_strip_as_wide_as_the_dock_and_has_no_glass() {
        let layout = layout_of(&[Slot::Tile, Slot::Tile]);
        let (input, glass, _) = surface_regions(frame(), &layout, (-70.0, 0.0, 60.0), true, false);
        assert_eq!(
            input,
            [(
                layout.bar_start.floor() as i32,
                200 - DOCK_REVEAL_STRIP,
                layout.bar_length.ceil() as i32,
                DOCK_REVEAL_STRIP
            )]
        );
        assert!(input[0].2 < 1000);
        assert!(glass.is_empty());
    }

    #[test]
    fn a_dock_coming_back_keeps_its_strip_and_a_dragged_icon_takes_the_whole_surface() {
        let layout = layout_of(&[Slot::Tile, Slot::Tile]);
        let (coming, _, _) = surface_regions(frame(), &layout, (-30.0, 0.5, 60.0), true, false);
        assert_eq!(coming.len(), 2);
        let (dragged, _, _) = surface_regions(frame(), &layout, (4.0, 1.0, 60.0), false, true);
        assert_eq!(dragged, [(0, 0, 1000, 200)]);
    }

    #[test]
    fn a_window_without_a_tile_minimizes_into_the_slot_its_tile_will_take_before_the_trash() {
        let list = [app(vec![window(1, false)]), Item::Separator, Item::Trash];
        let slots = [Slot::Tile, Slot::Separator, Slot::Tile];
        let layout = layout_of(&slots);
        let config = DockConfig::default();
        let targets = minimize_targets(&list, &slots, &layout, (frame(), &config), (None, 0.0));
        let future = future_layout(&slots, 2, (frame(), &config), (None, 0.0));
        let slot = future.placed[2];
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].window, 1);
        assert!((centre_x(targets[0].rect) - (slot.start + slot.length / 2.0)).abs() <= 1.0);
    }

    #[test]
    fn with_minimize_to_application_a_window_minimizes_into_its_apps_icon() {
        let list = [app(vec![window(1, false)]), Item::Separator, Item::Trash];
        let slots = [Slot::Tile, Slot::Separator, Slot::Tile];
        let layout = layout_of(&slots);
        let config = DockConfig {
            minimize_to_application: true,
            ..DockConfig::default()
        };
        let targets = minimize_targets(&list, &slots, &layout, (frame(), &config), (None, 0.0));
        let slot = layout.placed[0];
        assert_eq!(targets.len(), 1);
        assert!((centre_x(targets[0].rect) - (slot.start + slot.length / 2.0)).abs() <= 1.0);
    }

    #[test]
    fn a_minimized_window_goes_to_its_own_tile() {
        let list = [Item::Separator, Item::Window(window(2, true)), Item::Trash];
        let slots = [Slot::Separator, Slot::Wide(1.5), Slot::Tile];
        let layout = layout_of(&slots);
        let config = DockConfig::default();
        let targets = minimize_targets(&list, &slots, &layout, (frame(), &config), (None, 0.0));
        let slot = layout.placed[1];
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].window, 2);
        assert!((centre_x(targets[0].rect) - (slot.start + slot.length / 2.0)).abs() <= 1.0);
    }
}
