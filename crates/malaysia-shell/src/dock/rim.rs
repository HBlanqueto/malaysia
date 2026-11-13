use ctappearance::WallpaperState;
use ctappearance::dock::DockOrientation;
use ctappearance::tinting::WallpaperGrid;
use ctdesign::{DockSurface, Rgba, dock_divider};

type Area = (f32, f32, f32, f32);

const NEUTRAL: Rgba = Rgba {
    r: 0.5,
    g: 0.5,
    b: 0.5,
    a: 1.0,
};

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
