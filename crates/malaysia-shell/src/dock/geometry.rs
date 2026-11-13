use ctdesign::{
    DOCK_MAGNIFY_RADIUS, DOCK_PADDING, DOCK_PADDING_SCALE, DOCK_SEPARATOR, DOCK_SIDE_MARGIN,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Slot {
    Tile,
    Separator,
    // A tile as many tiles wide (a minimized window's, the Downloads folder's): it magnifies like a tile and keeps its shape.
    Wide(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    pub start: f32,
    pub length: f32,
    pub size: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layout {
    pub tile: f32,
    pub padding: f32,
    pub thickness: f32,
    pub bar_start: f32,
    pub bar_length: f32,
    pub placed: Vec<Placed>,
}

pub(crate) fn padding(tile: f32) -> f32 {
    DOCK_PADDING + DOCK_PADDING_SCALE * tile
}

pub(crate) fn thickness(tile: f32) -> f32 {
    tile + 2.0 * padding(tile)
}

fn units(slots: &[Slot]) -> f32 {
    slots
        .iter()
        .map(|slot| match slot {
            Slot::Tile => 1.0,
            Slot::Separator => DOCK_SEPARATOR,
            Slot::Wide(width) => *width,
        })
        .sum()
}

// The Dock never crosses the screen edge: with many items its largest size drops.
pub(crate) fn fit(requested: f32, main_length: f32, slots: &[Slot]) -> f32 {
    if main_length <= 0.0 {
        return requested;
    }
    let available = main_length * (1.0 - 2.0 * DOCK_SIDE_MARGIN) - 2.0 * DOCK_PADDING;
    let fitted = available / (units(slots).max(1.0) + 2.0 * DOCK_PADDING_SCALE);
    requested.min(fitted).max(1.0)
}

fn falloff(distance_in_tiles: f32) -> f32 {
    if distance_in_tiles >= DOCK_MAGNIFY_RADIUS {
        return 0.0;
    }
    (1.0 + (std::f32::consts::PI * distance_in_tiles / DOCK_MAGNIFY_RADIUS).cos()) / 2.0
}

pub(crate) fn layout(
    slots: &[Slot],
    tile: f32,
    largesize: f32,
    center: f32,
    pointer: Option<f32>,
    magnify: f32,
) -> Layout {
    let base_start = center - units(slots) * tile / 2.0;
    let mut cursor = base_start;
    let sized: Vec<(f32, f32)> = slots
        .iter()
        .map(|slot| match slot {
            Slot::Separator => {
                cursor += tile * DOCK_SEPARATOR;
                (tile * DOCK_SEPARATOR, tile)
            }
            Slot::Tile | Slot::Wide(_) => {
                let width = match slot {
                    Slot::Wide(width) => *width,
                    _ => 1.0,
                };
                let base_center = cursor + tile * width / 2.0;
                cursor += tile * width;
                let grow = pointer
                    .map(|at| falloff((at - base_center).abs() / tile))
                    .unwrap_or(0.0)
                    * magnify;
                let size = tile + (largesize.max(tile) - tile) * grow;
                (size * width, size)
            }
        })
        .collect();
    let total: f32 = sized.iter().map(|(length, _)| length).sum();
    let mut start = center - total / 2.0;
    let placed = sized
        .into_iter()
        .map(|(length, size)| {
            let placed = Placed {
                start,
                length,
                size,
            };
            start += length;
            placed
        })
        .collect();
    let padding = padding(tile);
    Layout {
        tile,
        padding,
        thickness: tile + 2.0 * padding,
        bar_start: center - total / 2.0 - padding,
        bar_length: total + 2.0 * padding,
        placed,
    }
}

pub(crate) fn slot_at(layout: &Layout, main: f32) -> Option<usize> {
    layout
        .placed
        .iter()
        .position(|placed| (placed.start..placed.start + placed.length).contains(&main))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macos_screenshot_dock() -> Vec<Slot> {
        let mut slots = vec![Slot::Tile; 19];
        slots.push(Slot::Separator);
        slots.extend([Slot::Tile, Slot::Tile]);
        slots
    }

    #[test]
    fn the_screen_width_caps_the_largest_size() {
        let tile = fit(128.0, 1920.0, &macos_screenshot_dock());
        assert!((82.0..84.0).contains(&tile), "{tile}");
    }

    #[test]
    fn a_size_that_fits_is_kept() {
        assert_eq!(fit(16.0, 1920.0, &macos_screenshot_dock()), 16.0);
        assert_eq!(fit(48.0, 0.0, &macos_screenshot_dock()), 48.0);
    }

    #[test]
    fn the_icon_under_the_pointer_reaches_largesize_and_far_icons_stay() {
        let slots = vec![Slot::Tile; 9];
        let tile = 48.0;
        let center = 500.0;
        let middle = center;
        let layout = layout(&slots, tile, 96.0, center, Some(middle), 1.0);
        assert_eq!(layout.placed[4].size, 96.0);
        assert_eq!(layout.placed[0].size, tile);
        assert!(layout.placed[3].size > tile && layout.placed[3].size < 96.0);
    }

    #[test]
    fn the_bar_stays_centred_and_grows_while_magnified() {
        let slots = vec![Slot::Tile, Slot::Separator, Slot::Tile];
        let rest = layout(&slots, 48.0, 96.0, 300.0, None, 1.0);
        let grown = layout(&slots, 48.0, 96.0, 300.0, Some(300.0), 1.0);
        for layout in [&rest, &grown] {
            assert!((layout.bar_start + layout.bar_length / 2.0 - 300.0).abs() < 0.01);
        }
        assert!(grown.bar_length > rest.bar_length);
        assert_eq!(rest.thickness, grown.thickness);
    }

    #[test]
    fn a_window_slot_is_as_wide_as_its_share_of_tiles_and_magnifies_with_the_same_height() {
        let slots = [Slot::Tile, Slot::Wide(1.5), Slot::Tile];
        let rest = layout(&slots, 40.0, 80.0, 100.0, None, 1.0);
        assert_eq!(rest.placed[1].length, 60.0);
        assert_eq!(rest.placed[1].size, 40.0);
        let grown = layout(&slots, 40.0, 80.0, 100.0, Some(100.0), 1.0);
        assert_eq!(grown.placed[1].size, 80.0);
        assert_eq!(grown.placed[1].length, 120.0);
    }

    #[test]
    fn the_centre_of_a_slot_does_not_depend_on_its_width() {
        let centre = |width: f32| {
            let layout = layout(
                &[Slot::Tile, Slot::Tile, Slot::Wide(width), Slot::Tile],
                40.0,
                40.0,
                300.0,
                None,
                0.0,
            );
            layout.placed[2].start + layout.placed[2].length / 2.0
        };
        assert!((centre(1.0) - centre(2.5)).abs() < 0.01);
    }

    #[test]
    fn a_point_is_in_the_slot_that_covers_it() {
        let layout = layout(&[Slot::Tile, Slot::Tile], 40.0, 40.0, 40.0, None, 0.0);
        assert_eq!(slot_at(&layout, 1.0), Some(0));
        assert_eq!(slot_at(&layout, 41.0), Some(1));
        assert_eq!(slot_at(&layout, 81.0), None);
    }
}
