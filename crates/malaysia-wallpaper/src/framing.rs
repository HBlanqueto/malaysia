use ctappearance::WallpaperScale;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Framing {
    pub(crate) scale: WallpaperScale,
    pub(crate) crop: Option<[f32; 4]>,
}

pub(crate) fn uv_window(
    framing: Framing,
    image: (u32, u32),
    surface: (u32, u32),
) -> ([f32; 2], [f32; 2]) {
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

pub(crate) fn texels_needed(
    framing: Framing,
    image: (u32, u32),
    surface: (u32, u32),
) -> (u32, u32) {
    let (_, size) = uv_window(framing, image, surface);
    (
        (surface.0 as f32 / size[0]).ceil() as u32,
        (surface.1 as f32 / size[1]).ceil() as u32,
    )
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

    #[test]
    fn a_filled_screen_needs_the_image_scaled_to_cover_it() {
        let fill = framing(WallpaperScale::Fill, None);
        assert_eq!(
            texels_needed(fill, (6016, 4016), (1920, 1080)),
            (1920, 1282)
        );
        assert_eq!(texels_needed(fill, (100, 100), (200, 100)), (200, 200));
    }

    #[test]
    fn a_centered_image_needs_all_of_its_pixels_and_a_stretched_one_only_the_screen() {
        let center = framing(WallpaperScale::Center, None);
        assert_eq!(
            texels_needed(center, (6016, 4016), (1920, 1080)),
            (6016, 4016)
        );
        let stretch = framing(WallpaperScale::Stretch, None);
        assert_eq!(
            texels_needed(stretch, (6016, 4016), (1920, 1080)),
            (1920, 1080)
        );
    }

    #[test]
    fn a_zoomed_crop_needs_more_texels_than_the_whole_image_would() {
        let crop = framing(WallpaperScale::Custom, Some([0.25, 0.25, 0.5, 0.5]));
        assert_eq!(texels_needed(crop, (4000, 3000), (1000, 750)), (2000, 1500));
    }
}
