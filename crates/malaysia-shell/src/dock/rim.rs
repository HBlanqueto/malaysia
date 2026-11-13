use bk::style::GlassRim;
use ctappearance::WallpaperState;
use ctappearance::dock::DockOrientation;
use ctappearance::tinting::WallpaperGrid;
use ctdesign::{DockSurface, Rgba, dock_divider, dock_rim};

type Area = (f32, f32, f32, f32);

// Pixels of the bar's edge that share one colour, and how far past the edge the backdrop is read.
const SEGMENT: f32 = 10.0;
const READ: f32 = 8.0;
const NEUTRAL: Rgba = Rgba {
    r: 0.5,
    g: 0.5,
    b: 0.5,
    a: 1.0,
};

// The Dock's frame, as macOS draws it: the rim's light tinted by the wallpaper just outside each
// stretch of the edge, and the outline. `bar` is the bar's rectangle in the Dock's surface, which spans the output along the
// Dock's edge. A Dock on the left or right takes the colour of the whole bar's surroundings.
pub(crate) fn glass_rim(
    wallpaper: &WallpaperState,
    desktop: Option<u32>,
    orientation: DockOrientation,
    surface: (f32, f32),
    bar: Area,
    dock: DockSurface,
) -> GlassRim {
    rim_around(
        backdrop(wallpaper, desktop, orientation, surface),
        orientation == DockOrientation::Bottom,
        bar,
        dock,
    )
}

// The divider's colour over the wallpaper behind its slot (`area`, in the Dock's surface).
pub(crate) fn divider(
    wallpaper: &WallpaperState,
    desktop: Option<u32>,
    orientation: DockOrientation,
    surface: (f32, f32),
    area: Area,
    dock: DockSurface,
) -> Rgba {
    let behind = backdrop(wallpaper, desktop, orientation, surface)(area);
    dock_divider(dock, behind.unwrap_or(NEUTRAL))
}

fn rim_around(
    backdrop: impl Fn(Area) -> Option<Rgba>,
    varies: bool,
    bar: Area,
    dock: DockSurface,
) -> GlassRim {
    let (x, y, width, height) = bar;
    let lit = |colour: Rgba| dock_rim(dock, colour);
    let read = |area: Area| lit(backdrop(area).unwrap_or(NEUTRAL));
    let around = read((x - READ, y - READ, width + 2.0 * READ, height + 2.0 * READ));
    let stretches = if varies {
        ((width / SEGMENT).round() as usize).clamp(8, 96)
    } else {
        1
    };
    let along = |edge_y: f32, inward: f32| -> Vec<Rgba> {
        let length = width / stretches as f32;
        (0..stretches)
            .map(|index| {
                if !varies {
                    return around;
                }
                let start = x + index as f32 * length;
                backdrop((start, edge_y, length, READ))
                    .or_else(|| backdrop((start, edge_y + inward, length, READ)))
                    .map_or(around, lit)
            })
            .collect()
    };
    let side = |edge_x: f32| {
        if varies {
            read((edge_x, y + height / 4.0, READ, height / 2.0))
        } else {
            around
        }
    };
    GlassRim {
        top: along(y - READ, READ),
        bottom: along(y + height, -READ),
        left: side(x - READ),
        right: side(x + width),
        outline: dock.outline,
    }
}

fn backdrop(
    wallpaper: &WallpaperState,
    desktop: Option<u32>,
    orientation: DockOrientation,
    surface: (f32, f32),
) -> impl Fn(Area) -> Option<Rgba> + '_ {
    move |(x, y, width, height)| {
        let output = wallpaper
            .outputs
            .first()
            .filter(|output| output.width > 0 && output.height > 0)?;
        let aspect = f64::from(output.width) / f64::from(output.height);
        let (surface_width, surface_height) = (f64::from(surface.0), f64::from(surface.1));
        let (wallpaper_width, wallpaper_height) = match orientation {
            DockOrientation::Bottom => (surface_width, surface_width / aspect),
            DockOrientation::Left | DockOrientation::Right => {
                (surface_height * aspect, surface_height)
            }
        };
        let (origin_x, origin_y) = match orientation {
            DockOrientation::Left => (0.0, 0.0),
            DockOrientation::Bottom => (0.0, wallpaper_height - surface_height),
            DockOrientation::Right => (wallpaper_width - surface_width, 0.0),
        };
        WallpaperGrid::behind(
            wallpaper.grid_for(desktop)?,
            (wallpaper_width, wallpaper_height),
            (
                origin_x + f64::from(x),
                origin_y + f64::from(y),
                f64::from(width),
                f64::from(height),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colour(r: f32, g: f32, b: f32) -> Rgba {
        Rgba { r, g, b, a: 1.0 }
    }

    // Dark on the left half of the bar, bright blue on the right half.
    fn split(area: Area) -> Option<Rgba> {
        Some(if area.0 + area.2 / 2.0 < 300.0 {
            colour(0.05, 0.08, 0.12)
        } else {
            colour(0.2, 0.5, 0.86)
        })
    }

    const BAR: Area = (100.0, 40.0, 400.0, 70.0);

    const DOCK: DockSurface = DockSurface {
        glass: Rgba {
            r: 0.15,
            g: 0.15,
            b: 0.15,
            a: 0.3,
        },
        outline: Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.6,
        },
        rim: [0.294, 0.64],
        divider: 0.263,
    };

    // In dark the rim lifts the glass less the brighter it is, so over the bright half it is fainter.
    #[test]
    fn each_stretch_of_the_edge_takes_the_light_of_the_wallpaper_beside_it() {
        let rim = rim_around(split, true, BAR, DOCK);
        assert_eq!(rim.top.len(), 40);
        assert!(rim.top.first().unwrap().a > rim.top.last().unwrap().a);
        assert!(rim.bottom.first().unwrap().a > rim.bottom.last().unwrap().a);
        assert!(rim.left.a > rim.right.a);
    }

    #[test]
    fn a_dock_on_the_side_has_one_colour_all_round() {
        let rim = rim_around(split, false, BAR, DOCK);
        assert_eq!(rim.top.len(), 1);
        assert_eq!(rim.top[0], rim.bottom[0]);
        assert_eq!(rim.top[0], rim.left);
        assert_eq!(rim.left, rim.right);
    }

    #[test]
    fn the_outline_is_the_docks() {
        assert_eq!(rim_around(split, true, BAR, DOCK).outline, DOCK.outline);
    }

    #[test]
    fn without_a_wallpaper_the_rim_is_neutral_and_still_there() {
        let rim = rim_around(|_| None, true, BAR, DOCK);
        assert!(
            rim.top
                .iter()
                .all(|colour| *colour == dock_rim(DOCK, NEUTRAL))
        );
        assert_eq!(rim.left, dock_rim(DOCK, NEUTRAL));
    }

    #[test]
    fn the_bottom_edge_falls_back_to_the_wallpaper_inside_it() {
        let screen_edge = |area: Area| (area.1 + area.3 <= 110.0).then(|| colour(0.2, 0.5, 0.86));
        let rim = rim_around(screen_edge, true, BAR, DOCK);
        assert_eq!(rim.bottom[0], dock_rim(DOCK, colour(0.2, 0.5, 0.86)));
    }
}
