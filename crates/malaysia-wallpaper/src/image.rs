//! Wallpaper image loading and dominant-colour extraction.
//!
//! Moved from `malaysia-shell/src/wallpaper.rs` with the GL/Freya parts
//! stripped out. The decoded image is uploaded to the GPU by the caller and
//! the CPU RGBA is dropped immediately afterwards.

use std::path::{Path, PathBuf};

use vow_design::Rgba;

/// The embedded default wallpaper: a sRGB JPEG shipped with the package.
pub const DEFAULT_WALLPAPER_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../share/wallpapers/bandura.jpg"
));

/// A decoded picture, ready to become a GPU texture.
#[derive(Clone, PartialEq)]
pub struct DecodedImage {
    /// Width and height in pixels.
    pub size: (u32, u32),
    /// Top-down RGBA8 pixels.
    pub rgba: Vec<u8>,
}

/// The image selection on disk and the decoded picture, if any.
#[derive(Clone, Default, PartialEq)]
pub struct Wallpaper {
    /// The selected file path and scale string from the store.
    pub selection: Option<(PathBuf, String)>,
    /// The decoded image, kept only until the GPU texture is uploaded.
    pub image: Option<LoadedImage>,
}

/// A successfully decoded image that has not been uploaded yet.
#[derive(Clone, PartialEq)]
pub struct LoadedImage {
    /// The decoded picture.
    pub decoded: DecodedImage,
}

/// Loads the selection, decodes it, and writes the dominant colour for the
/// compositor and the portal.
pub fn load_snapshot(store_path: &Path, state_path: &Path) -> Wallpaper {
    reload(&Wallpaper::default(), store_path, state_path)
}

/// Re-reads the store. Keeps `current` when the new selection cannot be read
/// or decoded, so a bad path never leaves the screen black.
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
                // shortcut: only "fill" is implemented; add a scale enum and shader UV
                // branch when another scale is needed.
                tracing::warn!(scale, "scale is not implemented yet; using 'fill'");
            }
            tracing::info!(?path, "loading wallpaper");
            match decode_image(&path) {
                Ok(decoded) => (Some((path, scale)), decoded),
                Err(error) => {
                    tracing::error!(?path, %error, "failed to decode wallpaper");
                    if current.image.is_some() {
                        return current.clone();
                    }
                    load_default_image()
                }
            }
        }
        None => load_default_image(),
    };

    let dominant = dominant(&decoded);
    if let Err(error) = write_state(state_path, dominant) {
        tracing::error!(%error, "failed to write the state");
    }
    Wallpaper {
        image: Some(LoadedImage { decoded }),
        selection,
    }
}

/// Decodes the embedded default image. This is the fallback when there is no
/// valid selection or when a startup selection cannot be decoded.
fn load_default_image() -> (Option<(PathBuf, String)>, DecodedImage) {
    match decode_image_bytes(DEFAULT_WALLPAPER_BYTES) {
        Ok(decoded) => (None, decoded),
        Err(error) => {
            // The embedded asset is part of the build; failing here is a bug.
            panic!("embedded default wallpaper is corrupt: {error}")
        }
    }
}

// --- wallpaper store -------------------------------------------------------

/// The `wallpaper.domain` store, as macOS keeps `com.apple.wallpaper`.
#[derive(Debug, Default, serde::Deserialize)]
struct WallpaperStore {
    /// Kept for forward compatibility with existing `wallpaper.domain` files;
    /// `malaysia-wallpaper` no longer uses this field because the default is
    /// embedded.
    #[allow(dead_code)]
    #[serde(default)]
    dir: String,
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

/// Reads the current wallpaper image path from the store.
///
/// Returns `Some((path, scale))` only when the store has a valid
/// `[desktops].default` entry. Any other case (no store, no selection, or a
/// selection pointing to a missing file) returns `None`; the caller must then
/// fall back to the embedded default image.
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

/// Turns a `file://` URL into a path, decoding `%XX` escapes.
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

/// Decodes a PNG or JPEG file into top-down RGBA8 pixels.
pub fn decode_image(path: &Path) -> anyhow::Result<DecodedImage> {
    decode_image_bytes(&std::fs::read(path)?)
}

/// Decodes a PNG or JPEG in memory into top-down RGBA8 pixels.
pub fn decode_image_bytes(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        decode_jpeg(bytes)
    } else {
        anyhow::bail!("unsupported image format (expected PNG or JPEG)")
    }
}

/// Decodes a PNG, normalizing 16-bit and indexed images to 8-bit colour.
pub fn decode_png(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;

    let width = info.width;
    let height = info.height;
    let bytes = &buf[..info.buffer_size()];
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    match info.color_type {
        png::ColorType::Rgb => {
            for px in bytes.chunks(3) {
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
            for px in bytes.chunks(2) {
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        color => anyhow::bail!("unsupported PNG colour type {color:?}"),
    }
    Ok(DecodedImage {
        size: (width, height),
        rgba,
    })
}

/// Decodes a JPEG into RGBA8 with `zune-jpeg`.
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
        rgba,
    })
}

/// The dominant colour, sampled on a grid and weighted by chroma.
pub fn dominant(image: &DecodedImage) -> Option<Rgba> {
    const SAMPLE_TARGET: f64 = 65_536.0;
    const MIN_CHROMA: f32 = 0.12;

    let (width, height) = image.size;
    if width == 0 || height == 0 {
        return None;
    }
    let (width, height) = (width as usize, height as usize);
    let step = ((width * height) as f64 / SAMPLE_TARGET)
        .sqrt()
        .round()
        .max(1.0) as usize;
    let mut weights = [0.0f32; 4096];
    let mut sums = [[0.0f32; 3]; 4096];

    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let index = (y * width + x) * 4;
            let [r, g, b, a] = [
                image.rgba[index],
                image.rgba[index + 1],
                image.rgba[index + 2],
                image.rgba[index + 3],
            ];
            if a < 128 {
                continue;
            }
            let (r, g, b) = (
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            );
            let chroma = r.max(g).max(b) - r.min(g).min(b);
            if chroma < MIN_CHROMA {
                continue;
            }
            let bucket = (bucket(r) << 8) | (bucket(g) << 4) | bucket(b);
            weights[bucket] += chroma;
            sums[bucket][0] += r * chroma;
            sums[bucket][1] += g * chroma;
            sums[bucket][2] += b * chroma;
        }
    }

    let (bucket, weight) = weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))?;
    if *weight == 0.0 {
        return None;
    }
    // We used to require `weight >= sampled * MIN_DOMINANT_SHARE`, but that
    // discarded real-world photographs (forest, landscape) whose chroma is
    // spread across many buckets. Keep the strongest chromatic bucket.
    Some(normalize_lightness([
        sums[bucket][0] / weight,
        sums[bucket][1] / weight,
        sums[bucket][2] / weight,
    ]))
}

/// The 4-bit histogram bucket of a `0.0..=1.0` channel.
fn bucket(channel: f32) -> usize {
    ((channel * 16.0) as usize).min(15)
}

/// Keeps the tint (and the traffic lights derived from it) in a readable
/// lightness band: HSL lightness 45–65 %.
fn normalize_lightness(rgb: [f32; 3]) -> Rgba {
    let (hue, saturation, lightness) = rgb_to_hsl(rgb);
    let [r, g, b] = hsl_to_rgb(hue, saturation, lightness.clamp(0.45, 0.65));
    Rgba { r, g, b, a: 1.0 }
}

fn rgb_to_hsl([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    if delta == 0.0 {
        return (0.0, 0.0, lightness);
    }
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == r {
        ((g - b) / delta + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    (hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    if saturation == 0.0 {
        return [lightness; 3];
    }
    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    [
        hue_to_rgb(p, q, hue + 1.0 / 3.0),
        hue_to_rgb(p, q, hue),
        hue_to_rgb(p, q, hue - 1.0 / 3.0),
    ]
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

/// Writes the dominant colour for the compositor and the portal, or clears it
/// when the wallpaper has no usable colour.
pub fn write_state(path: &Path, dominant: Option<Rgba>) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match dominant {
        Some(color) => {
            let temporary = path.with_extension("state.tmp");
            std::fs::write(
                &temporary,
                format!(
                    "# Generated by malaysia-wallpaper.\ndominant = [{:.6}, {:.6}, {:.6}]\n",
                    color.r, color.g, color.b
                ),
            )?;
            std::fs::rename(temporary, path)?;
        }
        None => match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
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
        let wallpaper = load_snapshot(&store, &state);
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
        let wallpaper = load_snapshot(&store, &state);
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
        let wallpaper = load_snapshot(&store, &state);
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
        let first = load_snapshot(&store, &state);
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
        let first = load_snapshot(&store, &state);
        assert!(first.image.is_some());

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
            rgba: [128u8, 128, 128, 255].repeat(256),
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
            rgba,
        };
        assert_orange(dominant(&image).expect("a dominant colour"));
    }

    #[test]
    fn a_dark_colour_is_lifted_into_the_lightness_band() {
        let image = DecodedImage {
            size: (16, 16),
            rgba: [77u8, 26, 0, 255].repeat(256),
        };
        let color = dominant(&image).expect("a dominant colour");
        let (_, _, lightness) = rgb_to_hsl([color.r, color.g, color.b]);
        assert!(
            (0.45..=0.65).contains(&lightness),
            "lightness {lightness} is outside the band"
        );
    }

    #[test]
    fn the_state_file_records_and_clears_the_dominant_colour() {
        let dir = scratch("state");
        let path = dir.join("wallpaper.state");
        let color = Rgba {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 1.0,
        };
        write_state(&path, Some(color)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("dominant = [0.200000, 0.400000, 0.600000]"));
        write_state(&path, None).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn the_embedded_default_wallpaper_decodes_to_1280x959() {
        let image = decode_image_bytes(DEFAULT_WALLPAPER_BYTES).unwrap();
        assert_eq!(image.size, (1280, 959));
    }

    #[test]
    fn the_embedded_default_has_a_dominant_colour() {
        let image = decode_image_bytes(DEFAULT_WALLPAPER_BYTES).unwrap();
        let color = dominant(&image).expect("bandura should have a dominant colour");
        // Obtained empirically on 2026-10-31 from the shipped JPEG.
        assert!((color.r - 0.820188).abs() < 0.001);
        assert!((color.g - 0.821487).abs() < 0.001);
        assert!((color.b - 0.078513).abs() < 0.001);
    }
}
