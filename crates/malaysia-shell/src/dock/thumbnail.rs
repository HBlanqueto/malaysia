use bk::shell::WindowImage;

// The largest tile is DOCK_TILESIZE_MAX at 2×; a bigger picture only costs memory.
const LONGEST_SIDE: u32 = ctdesign::DOCK_TILESIZE_MAX as u32 * 2;

pub(crate) fn png(image: &WindowImage) -> Option<Vec<u8>> {
    let small = shrink(image, LONGEST_SIDE);
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, small.width, small.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .ok()?
        .write_image_data(&small.rgba)
        .ok()?;
    Some(bytes)
}

fn shrink(image: &WindowImage, longest: u32) -> WindowImage {
    let factor = (longest as f32 / image.width.max(image.height).max(1) as f32).min(1.0);
    let width = ((image.width as f32 * factor).round() as u32).max(1);
    let height = ((image.height as f32 * factor).round() as u32).max(1);
    let rgba = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let source_x = (x * image.width / width).min(image.width - 1);
            let source_y = (y * image.height / height).min(image.height - 1);
            let at = ((source_y * image.width + source_x) * 4) as usize;
            [
                image.rgba[at],
                image.rgba[at + 1],
                image.rgba[at + 2],
                image.rgba[at + 3],
            ]
        })
        .collect();
    WindowImage {
        width,
        height,
        rgba,
    }
}

fn aspect(image: (u32, u32)) -> f32 {
    image.0.max(1) as f32 / image.1.max(1) as f32
}

// The picture is as tall as an icon's body and as wide as its window's shape asks.
pub(crate) fn body(image: (u32, u32), slot: f32) -> (f32, f32) {
    let height = slot * ctdesign::DOCK_ICON_BODY;
    (height * aspect(image), height)
}

// A tile's width in tiles: the picture's, plus the margin an icon has around its body.
pub(crate) fn units(image: (u32, u32)) -> f32 {
    ctdesign::DOCK_ICON_BODY * aspect(image) + (1.0 - ctdesign::DOCK_ICON_BODY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_window_shrinks_to_the_longest_side_and_keeps_its_shape() {
        let image = WindowImage {
            width: 1600,
            height: 1000,
            rgba: vec![255; 1600 * 1000 * 4],
        };
        let small = shrink(&image, 256);
        assert_eq!((small.width, small.height), (256, 160));
        assert_eq!(small.rgba.len(), 256 * 160 * 4);
    }

    #[test]
    fn a_picture_is_as_tall_as_the_icon_body_and_keeps_its_shape() {
        let (width, height) = body((1600, 1000), 56.0);
        assert!((height - 56.0 * ctdesign::DOCK_ICON_BODY).abs() < 1e-4);
        assert!((width / height - 1.6).abs() < 1e-4);
    }

    #[test]
    fn a_square_picture_takes_one_tile_and_a_wide_one_takes_more() {
        assert!((units((500, 500)) - 1.0).abs() < 1e-6);
        assert!(units((1600, 1000)) > 1.0);
    }
}
