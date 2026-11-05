use std::path::{Path, PathBuf};

use vow_design::Rgba;

pub const DEFAULT_WALLPAPER_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../share/wallpapers/malaysia-dream.heic"
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

#[derive(Clone, Default, PartialEq)]
pub struct Wallpaper {
    pub selection: Option<(PathBuf, String)>,
    pub image: Option<DecodedImage>,
}

pub fn reload(current: &Wallpaper, store_path: &Path, state_path: &Path) -> Wallpaper {
    let selection = match load_wallpaper(store_path) {
        Ok(selection) => selection,
        Err(error) => {
            tracing::error!(%error, "cannot read wallpaper store");
            return current.clone();
        }
    };

    let (selection, decoded) = match selection {
        Some((path, scale)) => {
            if current
                .selection
                .as_ref()
                .is_some_and(|(old_path, old_scale)| old_path == &path && old_scale == &scale)
            {
                return current.clone();
            }
            if !scale.is_empty() && scale != "fill" {
                tracing::warn!(scale, "scale is not implemented yet; using 'fill'");
            }
            tracing::info!(?path, "loading wallpaper");
            match decode_image(&path) {
                Ok(decoded) => (Some((path, scale)), decoded),
                Err(error) => {
                    tracing::error!(?path, %error, "failed to decode wallpaper");
                    if current.selection.is_some() {
                        return current.clone();
                    }
                    load_default_image()
                }
            }
        }
        None => load_default_image(),
    };

    if let Err(error) = vow_appearance::write_dominant(state_path, dominant(&decoded)) {
        tracing::error!(%error, "failed to write the state");
    }
    Wallpaper {
        image: Some(decoded),
        selection,
    }
}

fn load_default_image() -> (Option<(PathBuf, String)>, DecodedImage) {
    match decode_image_bytes(DEFAULT_WALLPAPER_BYTES) {
        Ok(decoded) => (None, decoded),
        Err(error) => {
            panic!("embedded default wallpaper is corrupt: {error}")
        }
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct WallpaperStore {
    #[serde(default)]
    scale: String,
    #[serde(default)]
    desktops: Desktops,
}

#[derive(Debug, Default, serde::Deserialize)]
struct Desktops {
    #[serde(default)]
    default: Option<String>,
}

pub fn load_wallpaper(store_path: &Path) -> anyhow::Result<Option<(PathBuf, String)>> {
    let store: WallpaperStore = match std::fs::read_to_string(store_path) {
        Ok(text) => toml::from_str(&text)?,
        Err(_) => WallpaperStore::default(),
    };
    Ok(store
        .desktops
        .default
        .as_ref()
        .and_then(|url| file_url_to_path(url))
        .map(|path| (path, store.scale)))
}

fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let bytes = percent_decode(rest.as_bytes());
    Some(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
}

fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn decode_image(path: &Path) -> anyhow::Result<DecodedImage> {
    decode_image_bytes(&std::fs::read(path)?)
}

pub fn decode_image_bytes(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    let start = std::time::Instant::now();
    let result = if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        decode_jpeg(bytes)
    } else if bytes.get(4..8) == Some(b"ftyp") {
        decode_heif(bytes)
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

pub fn decode_heif(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let context = HeifContext::read_from_bytes(bytes)?;
    let handle = context.primary_image_handle()?;
    let image = LibHeif::new().decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)?;
    let plane = image
        .planes()
        .interleaved
        .ok_or_else(|| anyhow::anyhow!("the HEIF image has no interleaved RGBA plane"))?;
    let row = plane.width as usize * 4;
    let rgba = plane
        .data
        .chunks(plane.stride)
        .flat_map(|line| &line[..row])
        .copied()
        .collect();
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

    pub fn downscale_to_fit(&self, max: u32) -> Self {
        let (mut width, mut height) = self.size;
        if width <= max && height <= max {
            return self.clone();
        }
        let mut current = self.clone();
        while width > max || height > max {
            let new_w = (width / 2).max(1);
            let new_h = (height / 2).max(1);
            current = current.downscale_2x2(new_w, new_h);
            width = new_w;
            height = new_h;
        }
        current
    }

    fn downscale_2x2(&self, new_w: u32, new_h: u32) -> Self {
        let pixels = match &self.pixels {
            Pixels::Srgb8(pixels) => {
                Pixels::Srgb8(average_2x2(pixels, self.size, (new_w, new_h), 255.0, |v| {
                    v as u8
                }))
            }
            Pixels::Srgb16(pixels) => Pixels::Srgb16(average_2x2(
                pixels,
                self.size,
                (new_w, new_h),
                65535.0,
                |v| v as u16,
            )),
        };
        DecodedImage {
            size: (new_w, new_h),
            pixels,
        }
    }
}

fn average_2x2<T: Copy + Into<f32>>(
    pixels: &[T],
    (old_w, old_h): (u32, u32),
    (new_w, new_h): (u32, u32),
    max: f32,
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
                let [r, g, b, a] =
                    [0, 1, 2, 3].map(|channel| pixels[offset + channel].into() / max);
                let linear = Rgba { r, g, b, a }.to_linear();
                sum.r += linear.r;
                sum.g += linear.g;
                sum.b += linear.b;
                sum.a += linear.a;
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
        .div_ceil(vow_design::BACKDROP.samples as usize)
        .max(1);
    let samples: Vec<u8> = (0..height)
        .step_by(step)
        .flat_map(|y| (0..width).step_by(step).map(move |x| (y * width + x) * 4))
        .flat_map(|index| image.rgba8_at(index))
        .collect();
    vow_appearance::backdrop::dominant_from_pixels(&samples)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Pixels, *};
    use vow_design::Rgba;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("malaysia-wallpaper-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, file: &str) -> PathBuf {
        let path = dir.join(file);
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn without_a_selection_the_default_wallpaper_is_used() {
        let dir = scratch("default");
        let store = dir.join("wallpaper.domain");
        let state = dir.join("wallpaper.state");
        let wallpaper = reload(&Wallpaper::default(), &store, &state);
        assert!(
            wallpaper.image.is_some(),
            "the embedded default should load"
        );
        assert!(wallpaper.selection.is_none());
    }

    #[test]
    fn the_store_selection_wins_over_the_default() {
        let dir = scratch("selection");
        touch(&dir, "bandura.jpg");
        let mint = touch(&dir, "Solid Mint.png");
        let store = dir.join("wallpaper.domain");
        std::fs::write(
            &store,
            format!(
                "dir = {:?}\n[desktops]\ndefault = \"file://{}\"\n",
                dir.to_str().unwrap(),
                mint.to_str().unwrap()
            ),
        )
        .unwrap();
        let (path, _) = load_wallpaper(&store).unwrap().unwrap();
        assert_eq!(path, mint);
    }

    #[test]
    fn a_folder_without_selection_uses_the_embedded_default() {
        let dir = scratch("custom");
        touch(&dir, "Black.png");
        touch(&dir, "Yellow.png");
        let store = dir.join("wallpaper.domain");
        std::fs::write(&store, format!("dir = {:?}\n", dir.to_str().unwrap())).unwrap();
        let state = dir.join("wallpaper.state");
        let wallpaper = reload(&Wallpaper::default(), &store, &state);
        assert!(
            wallpaper.image.is_some(),
            "the embedded default should load"
        );
        assert!(wallpaper.selection.is_none());
    }

    #[test]
    fn a_missing_selection_at_startup_uses_the_embedded_default() {
        let dir = scratch("missing");
        let store = dir.join("wallpaper.domain");
        let state = dir.join("wallpaper.state");
        std::fs::write(
            &store,
            "[desktops]\ndefault = \"file:///no/such/file.jpg\"\n",
        )
        .unwrap();
        let wallpaper = reload(&Wallpaper::default(), &store, &state);
        assert!(
            wallpaper.image.is_some(),
            "the embedded default should load"
        );
        assert!(wallpaper.selection.is_none());
    }

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn fixture(name: &str) -> DecodedImage {
        decode_image(&fixture_path(name)).unwrap()
    }

    #[test]
    fn editing_the_store_loads_the_new_selection() {
        let dir = scratch("reload");
        let store = dir.join("wallpaper.domain");
        let state = dir.join("wallpaper.state");
        let orange = fixture_path("orange.jpg");
        std::fs::write(
            &store,
            format!("[desktops]\ndefault = \"file://{}\"\n", orange.display()),
        )
        .unwrap();
        let first = reload(&Wallpaper::default(), &store, &state);
        assert_eq!(
            first.selection.as_ref().map(|(path, _)| path),
            Some(&orange)
        );
        assert!(first.image.is_some());

        let second = dir.join("second.png");
        std::fs::copy(fixture_path("orange-16.png"), &second).unwrap();
        std::fs::write(
            &store,
            format!(
                "scale = \"fit\"\n[desktops]\ndefault = \"file://{}\"\n",
                second.display()
            ),
        )
        .unwrap();
        let reloaded = reload(&first, &store, &state);
        assert_eq!(
            reloaded.selection.as_ref().map(|(path, _)| path),
            Some(&second)
        );
        assert_eq!(
            reloaded.selection.as_ref().map(|(_, scale)| scale.as_str()),
            Some("fit")
        );
        assert!(reloaded.image.is_some());
    }

    #[test]
    fn an_unreadable_wallpaper_keeps_the_previous_image() {
        let dir = scratch("unreadable");
        let store = dir.join("wallpaper.domain");
        let state = dir.join("wallpaper.state");
        let orange = fixture_path("orange.jpg");
        std::fs::write(
            &store,
            format!("[desktops]\ndefault = \"file://{}\"\n", orange.display()),
        )
        .unwrap();
        let mut first = reload(&Wallpaper::default(), &store, &state);
        assert!(first.image.take().is_some());

        let missing = dir.join("missing.png");
        std::fs::write(
            &store,
            format!("[desktops]\ndefault = \"file://{}\"\n", missing.display()),
        )
        .unwrap();
        assert!(reload(&first, &store, &state) == first);
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
    fn the_embedded_default_is_a_6016x6016_salmon_heic() {
        let image = decode_image_bytes(DEFAULT_WALLPAPER_BYTES).unwrap();
        assert_eq!(image.size, (6016, 6016));
        assert_eq!(image.rgba8_at(0), [0xe1, 0x6e, 0x79, 0xff]);
    }

    #[test]
    fn the_embedded_default_has_a_dominant_colour() {
        let image = decode_image_bytes(DEFAULT_WALLPAPER_BYTES).unwrap();
        let color = dominant(&image).expect("the default should have a dominant colour");
        assert!((color.r - 0.879878).abs() < 0.001);
        assert!((color.g - 0.420122).abs() < 0.001);
        assert!((color.b - 0.464189).abs() < 0.001);
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
