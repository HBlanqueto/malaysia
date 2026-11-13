use ctappearance::WallpaperState;
use ctappearance::dock::DockOrientation;
use ctappearance::tinting::WallpaperGrid;
use ctdesign::{DOCK_RIM_BRIGHT, DOCK_RIM_DARK};

// macOS draws the Dock's edge as light over the wallpaper: faint over a dark one, nearly white
// over a light or flat one. `bar` is the bar's rectangle in the Dock's surface, which spans the
// output along the Dock's edge.
pub(crate) fn strength(
    wallpaper: &WallpaperState,
    desktop: Option<u32>,
    orientation: DockOrientation,
    surface: (f32, f32),
    bar: (f32, f32, f32, f32),
) -> f32 {
    lightness_behind(wallpaper, desktop, orientation, surface, bar).map_or(1.0, strength_for)
}

fn strength_for(lightness: f32) -> f32 {
    DOCK_RIM_DARK + (DOCK_RIM_BRIGHT - DOCK_RIM_DARK) * lightness.clamp(0.0, 1.0)
}

fn lightness_behind(
    wallpaper: &WallpaperState,
    desktop: Option<u32>,
    orientation: DockOrientation,
    surface: (f32, f32),
    bar: (f32, f32, f32, f32),
) -> Option<f32> {
    let output = wallpaper
        .outputs
        .first()
        .filter(|output| output.width > 0 && output.height > 0)?;
    let aspect = f64::from(output.width) / f64::from(output.height);
    let (surface_width, surface_height) = (f64::from(surface.0), f64::from(surface.1));
    let (width, height) = match orientation {
        DockOrientation::Bottom => (surface_width, surface_width / aspect),
        DockOrientation::Left | DockOrientation::Right => (surface_height * aspect, surface_height),
    };
    let (x, y) = match orientation {
        DockOrientation::Left => (0.0, 0.0),
        DockOrientation::Bottom => (0.0, height - surface_height),
        DockOrientation::Right => (width - surface_width, 0.0),
    };
    let behind = WallpaperGrid::behind(
        wallpaper.grid_for(desktop)?,
        (width, height),
        (
            x + f64::from(bar.0),
            y + f64::from(bar.1),
            f64::from(bar.2),
            f64::from(bar.3),
        ),
    )?;
    Some(behind.to_hsl().2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_is_fainter_than_the_palette_over_black_and_stronger_over_white() {
        assert_eq!(strength_for(0.0), DOCK_RIM_DARK);
        assert_eq!(strength_for(1.0), DOCK_RIM_BRIGHT);
        const { assert!(DOCK_RIM_DARK < 1.0 && DOCK_RIM_BRIGHT > 1.0) };
    }

    #[test]
    fn the_line_grows_with_the_lightness_of_the_wallpaper() {
        let steps: Vec<f32> = (0..=10).map(|i| strength_for(i as f32 / 10.0)).collect();
        assert!(steps.windows(2).all(|pair| pair[0] < pair[1]), "{steps:?}");
        assert_eq!(strength_for(7.0), strength_for(1.0));
    }
}
