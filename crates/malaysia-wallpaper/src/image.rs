use std::borrow::Cow;
use std::path::Path;

use ctappearance::tinting::WallpaperGrid;
use ctdesign::Rgba;

pub const DEFAULT_WALLPAPER_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../share/wallpapers/collapse.heic"
));

#[derive(Clone, PartialEq)]
pub enum Pixels {
    Srgb8(Vec<u8>),
    Srgb16(Vec<u16>),
}

#[derive(Clone, PartialEq)]
pub struct DecodedImage {
    pub size: (u32, u32),
    pub pixels: Pixels,
}

pub fn is_heif(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("heic") || ext.eq_ignore_ascii_case("heif"))
}

pub fn decode_image_bytes(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    let start = std::time::Instant::now();
    let result = if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        decode_jpeg(bytes)
    } else if bytes.get(4..8) == Some(b"ftyp") {
        decode_heif(bytes, None)
    } else {
        anyhow::bail!("unsupported image format (expected PNG, JPEG or HEIF)")
    };
    tracing::info!(elapsed_ms = start.elapsed().as_millis(), "image decoded");
    result
}

#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn decode_png(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;

    let width = info.width;
    let height = info.height;
    let bytes = &buf[..info.buffer_size()];

    if info.bit_depth == png::BitDepth::Sixteen {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        match info.color_type {
            png::ColorType::Rgb => {
                for px in bytes.chunks_exact(6) {
                    rgba.extend_from_slice(&[
                        u16::from_be_bytes([px[0], px[1]]),
                        u16::from_be_bytes([px[2], px[3]]),
                        u16::from_be_bytes([px[4], px[5]]),
                        u16::MAX,
                    ]);
                }
            }
            png::ColorType::Rgba => {
                for px in bytes.chunks_exact(8) {
                    rgba.extend_from_slice(&[
                        u16::from_be_bytes([px[0], px[1]]),
                        u16::from_be_bytes([px[2], px[3]]),
                        u16::from_be_bytes([px[4], px[5]]),
                        u16::from_be_bytes([px[6], px[7]]),
                    ]);
                }
            }
            png::ColorType::Grayscale => {
                for px in bytes.chunks_exact(2) {
                    let v = u16::from_be_bytes([px[0], px[1]]);
                    rgba.extend_from_slice(&[v, v, v, u16::MAX]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for px in bytes.chunks_exact(4) {
                    let v = u16::from_be_bytes([px[0], px[1]]);
                    let a = u16::from_be_bytes([px[2], px[3]]);
                    rgba.extend_from_slice(&[v, v, v, a]);
                }
            }
            color => anyhow::bail!("unsupported PNG colour type {color:?}"),
        }
        return Ok(DecodedImage {
            size: (width, height),
            pixels: Pixels::Srgb16(rgba),
        });
    }

    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    match info.color_type {
        png::ColorType::Rgb => {
            for px in bytes.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        png::ColorType::Rgba => rgba.extend_from_slice(bytes),
        png::ColorType::Grayscale => {
            for &value in bytes {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for px in bytes.chunks_exact(2) {
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        color => anyhow::bail!("unsupported PNG colour type {color:?}"),
    }
    Ok(DecodedImage {
        size: (width, height),
        pixels: Pixels::Srgb8(rgba),
    })
}

pub fn decode_jpeg(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };

    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let rgba = decoder.decode()?;
    let info = decoder
        .info()
        .ok_or_else(|| anyhow::anyhow!("the JPEG headers were not decoded"))?;
    Ok(DecodedImage {
        size: (u32::from(info.width), u32::from(info.height)),
        pixels: Pixels::Srgb8(rgba),
    })
}

pub fn decode_heif(bytes: &[u8], image: Option<usize>) -> anyhow::Result<DecodedImage> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let context = HeifContext::read_from_bytes(bytes)?;
    let handle = match image {
        Some(index) => {
            let id = context
                .image_ids()
                .get(index)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("the HEIF file has no image {index}"))?;
            context.image_handle(id)?
        }
        None => context.primary_image_handle()?,
    };
    let image = LibHeif::new().decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)?;
    let plane = image
        .planes()
        .interleaved
        .ok_or_else(|| anyhow::anyhow!("the HEIF image has no interleaved RGBA plane"))?;
    let row = plane.width as usize * 4;
    let mut rgba = Vec::with_capacity(row * plane.height as usize);
    for line in plane.data.chunks(plane.stride) {
        rgba.extend_from_slice(&line[..row]);
    }
    Ok(DecodedImage {
        size: (plane.width, plane.height),
        pixels: Pixels::Srgb8(rgba),
    })
}

impl DecodedImage {
    pub fn rgba8_at(&self, index: usize) -> [u8; 4] {
        match &self.pixels {
            Pixels::Srgb8(v) => [v[index], v[index + 1], v[index + 2], v[index + 3]],
            Pixels::Srgb16(v) => [
                (v[index] >> 8) as u8,
                (v[index + 1] >> 8) as u8,
                (v[index + 2] >> 8) as u8,
                (v[index + 3] >> 8) as u8,
            ],
        }
    }

    pub fn downscale_to_fit(&self, max: u32) -> Cow<'_, Self> {
        let mut current = Cow::Borrowed(self);
        while current.size.0 > max || current.size.1 > max {
            let (width, height) = current.size;
            current = Cow::Owned(current.downscale_2x2((width / 2).max(1), (height / 2).max(1)));
        }
        current
    }

    fn downscale_2x2(&self, new_w: u32, new_h: u32) -> Self {
        let pixels = match &self.pixels {
            Pixels::Srgb8(pixels) => {
                let linear: [f32; 256] =
                    std::array::from_fn(|value| decode_srgb(value as f32 / 255.0));
                Pixels::Srgb8(average_2x2(
                    pixels,
                    self.size,
                    (new_w, new_h),
                    255.0,
                    |value| linear[usize::from(value)],
                    |v| v as u8,
                ))
            }
            Pixels::Srgb16(pixels) => Pixels::Srgb16(average_2x2(
                pixels,
                self.size,
                (new_w, new_h),
                65535.0,
                |value| decode_srgb(f32::from(value) / 65535.0),
                |v| v as u16,
            )),
        };
        DecodedImage {
            size: (new_w, new_h),
            pixels,
        }
    }
}

fn decode_srgb(channel: f32) -> f32 {
    Rgba {
        r: channel,
        ..Rgba::TRANSPARENT
    }
    .to_linear()
    .r
}

fn average_2x2<T: Copy + Into<f32>>(
    pixels: &[T],
    (old_w, old_h): (u32, u32),
    (new_w, new_h): (u32, u32),
    max: f32,
    to_linear: impl Fn(T) -> f32,
    from: impl Fn(f32) -> T,
) -> Vec<T> {
    let mut out = Vec::with_capacity((new_w * new_h * 4) as usize);
    for y in 0..new_h {
        for x in 0..new_w {
            let mut sum = Rgba::TRANSPARENT;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(old_w - 1);
                let sy = (y * 2 + dy).min(old_h - 1);
                let offset = ((sy * old_w + sx) * 4) as usize;
                sum.r += to_linear(pixels[offset]);
                sum.g += to_linear(pixels[offset + 1]);
                sum.b += to_linear(pixels[offset + 2]);
                sum.a += pixels[offset + 3].into() / max;
            }
            let mean = Rgba::from_linear(Rgba {
                r: sum.r / 4.0,
                g: sum.g / 4.0,
                b: sum.b / 4.0,
                a: sum.a / 4.0,
            });
            out.extend([mean.r, mean.g, mean.b, mean.a].map(|value| from((value * max).round())));
        }
    }
    out
}

pub fn dominant(image: &DecodedImage) -> Option<Rgba> {
    let (width, height) = (image.size.0 as usize, image.size.1 as usize);
    let step = width
        .max(height)
        .div_ceil(ctdesign::BACKDROP.samples as usize)
        .max(1);
    let samples: Vec<u8> = (0..height)
        .step_by(step)
        .flat_map(|y| (0..width).step_by(step).map(move |x| (y * width + x) * 4))
        .flat_map(|index| image.rgba8_at(index))
        .collect();
    ctappearance::backdrop::dominant_from_pixels(&samples)
}

pub fn grid(image: &DecodedImage) -> Option<WallpaperGrid> {
    let small = image.downscale_to_fit(ctdesign::BACKDROP.samples);
    let (width, height) = (small.size.0 as usize, small.size.1 as usize);
    let rgba8: Vec<u8> = (0..width * height)
        .flat_map(|pixel| small.rgba8_at(pixel * 4))
        .collect();
    WallpaperGrid::from_rgba8(&rgba8, width, height)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Pixels, *};
    use ctdesign::Rgba;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn fixture(name: &str) -> DecodedImage {
        decode_image_bytes(&std::fs::read(fixture_path(name)).unwrap()).unwrap()
    }

    fn assert_orange(color: Rgba) {
        assert!(
            color.r > color.g && color.g > color.b,
            "expected orange, got {color:?}"
        );
        assert!(color.r > 0.5, "expected a bright orange, got {color:?}");
    }

    #[test]
    fn a_jpeg_wallpaper_decodes_to_its_dominant_colour() {
        let image = fixture("orange.jpg");
        assert_eq!(image.size, (16, 16));
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn a_sixteen_bit_png_decodes_to_its_dominant_colour() {
        let image = fixture("orange-16.png");
        assert_eq!(image.size, (16, 16));
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn an_indexed_png_decodes_to_its_dominant_colour() {
        let image = fixture("orange-indexed.png");
        assert_eq!(image.size, (16, 16));
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn a_gray_wallpaper_has_no_dominant_colour() {
        let image = DecodedImage {
            size: (16, 16),
            pixels: Pixels::Srgb8([128u8, 128, 128, 255].repeat(256)),
        };
        assert_eq!(dominant(&image), None);
    }

    #[test]
    fn an_orange_gradient_stays_orange() {
        let mut rgba = Vec::new();
        for y in 0..64 {
            for x in 0..64 {
                let t = (x + y) as f32 / 126.0;
                rgba.extend_from_slice(&[
                    ((0.55 + 0.4 * t) * 255.0) as u8,
                    ((0.25 + 0.3 * t) * 255.0) as u8,
                    ((0.05 + 0.15 * t) * 255.0) as u8,
                    255,
                ]);
            }
        }
        let image = DecodedImage {
            size: (64, 64),
            pixels: Pixels::Srgb8(rgba),
        };
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn a_dark_colour_is_lifted_into_the_lightness_band() {
        let image = DecodedImage {
            size: (16, 16),
            pixels: Pixels::Srgb8([77u8, 26, 0, 255].repeat(256)),
        };
        let color = dominant(&image).expect("a dominant colour");
        let (_, _, lightness) = color.to_hsl();
        assert!(
            (0.45..=0.65).contains(&lightness),
            "lightness {lightness} is outside the band"
        );
    }

    #[test]
    fn the_embedded_default_is_the_6016x6016_collapse_heic() {
        let image = decode_image_bytes(DEFAULT_WALLPAPER_BYTES).unwrap();
        assert_eq!(image.size, (6016, 6016));
        let [r, _, b, a] = image.rgba8_at(0);
        assert!(b > r && a == 0xff, "Collapse is blue");
    }

    #[test]
    fn the_embedded_default_has_a_dominant_colour() {
        let image = decode_image_bytes(DEFAULT_WALLPAPER_BYTES).unwrap();
        let color = dominant(&image).expect("the default should have a dominant colour");
        assert!(color.b > color.r, "expected blue, got {color:?}");
    }

    #[test]
    fn collapse_holds_a_dark_image_after_the_light_one() {
        let dark = decode_heif(DEFAULT_WALLPAPER_BYTES, Some(1)).unwrap();
        let light = decode_heif(DEFAULT_WALLPAPER_BYTES, Some(0)).unwrap();
        let luma = |image: &DecodedImage| dominant(image).map(|c| c.r + c.g + c.b);
        assert_eq!(dark.size, light.size);
        assert_ne!(luma(&dark), luma(&light));
    }

    #[test]
    fn a_sixteen_bit_png_keeps_sixteen_bits() {
        let image = fixture("orange-16.png");
        assert_eq!(image.size, (16, 16));
        assert!(matches!(image.pixels, Pixels::Srgb16(_)));
    }

    #[test]
    fn an_indexed_png_stays_eight_bit() {
        let image = fixture("orange-indexed.png");
        assert_eq!(image.size, (16, 16));
        assert!(matches!(image.pixels, Pixels::Srgb8(_)));
    }
}
