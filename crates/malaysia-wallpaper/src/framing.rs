use ctappearance::WallpaperScale;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Framing {
    pub scale: WallpaperScale,
    pub crop: Option<[f32; 4]>,
}

pub fn uv_window(framing: Framing, image: (u32, u32), surface: (u32, u32)) -> ([f32; 2], [f32; 2]) {
    let (image_w, image_h) = (image.0 as f32, image.1 as f32);
    let (surface_w, surface_h) = (surface.0 as f32, surface.1 as f32);
    let [x, y, w, h] = match (framing.scale, framing.crop) {
        (WallpaperScale::Custom, Some(crop)) => crop,
        _ => [0.0, 0.0, 1.0, 1.0],
    };
    let (region_w, region_h) = (w * image_w, h * image_h);
    let scale = match framing.scale {
        WallpaperScale::Stretch => return ([0.0, 0.0], [1.0, 1.0]),
        WallpaperScale::Fit => (surface_w / region_w).min(surface_h / region_h),
        WallpaperScale::Center => 1.0,
        WallpaperScale::Fill | WallpaperScale::Custom => {
            (surface_w / region_w).max(surface_h / region_h)
        }
    };
    let size = [surface_w / scale / image_w, surface_h / scale / image_h];
    let centre = [x + w / 2.0, y + h / 2.0];
    ([centre[0] - size[0] / 2.0, centre[1] - size[1] / 2.0], size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framing(scale: WallpaperScale, crop: Option<[f32; 4]>) -> Framing {
        Framing { scale, crop }
    }

    #[test]
    fn fill_crops_a_square_image_to_a_wide_screen() {
        let window = uv_window(framing(WallpaperScale::Fill, None), (100, 100), (200, 100));
        assert_eq!(window, ([0.0, 0.25], [1.0, 0.5]));
    }

    #[test]
    fn fit_leaves_bands_beside_a_square_image_on_a_wide_screen() {
        let window = uv_window(framing(WallpaperScale::Fit, None), (100, 100), (200, 100));
        assert_eq!(window, ([-0.5, 0.0], [2.0, 1.0]));
    }

    #[test]
    fn center_shows_one_image_pixel_per_surface_pixel() {
        let window = uv_window(
            framing(WallpaperScale::Center, None),
            (400, 200),
            (200, 100),
        );
        assert_eq!(window, ([0.25, 0.25], [0.5, 0.5]));
    }

    #[test]
    fn stretch_shows_the_whole_image() {
        let window = uv_window(
            framing(WallpaperScale::Stretch, None),
            (100, 100),
            (200, 100),
        );
        assert_eq!(window, ([0.0, 0.0], [1.0, 1.0]));
    }

    #[test]
    fn a_custom_crop_fills_the_screen_around_its_centre() {
        let crop = Some([0.5, 0.5, 0.25, 0.25]);
        let window = uv_window(
            framing(WallpaperScale::Custom, crop),
            (100, 100),
            (100, 100),
        );
        assert_eq!(window, ([0.5, 0.5], [0.25, 0.25]));
    }

    #[test]
    fn custom_without_a_crop_fills_the_screen() {
        let custom = uv_window(
            framing(WallpaperScale::Custom, None),
            (100, 100),
            (200, 100),
        );
        let fill = uv_window(framing(WallpaperScale::Fill, None), (100, 100), (200, 100));
        assert_eq!(custom, fill);
    }
}
