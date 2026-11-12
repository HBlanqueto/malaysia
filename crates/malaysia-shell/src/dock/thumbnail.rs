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

pub(crate) fn fit(image: (u32, u32), box_side: f32) -> (f32, f32) {
    let (width, height) = (image.0.max(1) as f32, image.1.max(1) as f32);
    let scale = box_side / width.max(height);
    (width * scale, height * scale)
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
    fn a_picture_fits_its_box_by_the_longer_side() {
        assert_eq!(fit((1600, 1000), 64.0), (64.0, 40.0));
        assert_eq!(fit((500, 1000), 64.0), (32.0, 64.0));
    }
}
