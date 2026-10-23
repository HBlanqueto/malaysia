//! The wallpaper component: the desktop picture on the `Background` layer.
//!
//! Like macOS (`com.apple.wallpaper`), the *current selection* lives in the
//! wallpaper domain store. This component watches that store and re-renders
//! when it changes. The picture is drawn by CelestialKit on Freya (ADR 0034)
//! as a Skia image scaled to cover the output (macOS default `fill`).

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use celestial_kit::platform::{Anchors, KeyboardInteractivity, Layer, SurfaceSpec, run_layer};
use freya_core::elements::image::ImageHandle;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use freya_engine::prelude::AlphaType;
use torin::prelude::Size;
use yuki_design::Rgba;

use yuki_appearance::data_dir;

/// The decoded wallpaper, ready to become a Freya image.
#[derive(Clone, PartialEq)]
struct DecodedImage {
    size: (u32, u32),
    rgba: Vec<u8>,
}

/// What the component draws and the files it last read.
#[derive(Clone, PartialEq)]
struct Snapshot {
    image: Option<ImageHandle>,
    current: Option<PathBuf>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            image: None,
            current: None,
        }
    }
}

struct WallpaperApp {
    state: State<Snapshot>,
    store_path: PathBuf,
    state_path: PathBuf,
}

impl App for WallpaperApp {
    fn render(&self) -> impl IntoElement {
        let state = self.state;
        let store_path = self.store_path.clone();
        let state_path = self.state_path.clone();

        use_future(move || {
            let store_path = store_path.clone();
            let state_path = state_path.clone();
            let mut state = state;
            async move {
                loop {
                    async_io::Timer::after(Duration::from_millis(500)).await;
                    let next =
                        load_snapshot(&store_path, &state_path, state.peek().current.clone());
                    if next != *state.peek() {
                        state.set(next);
                    }
                }
            }
        });

        let Some(handle) = state.read().image.clone() else {
            return rect()
                .width(Size::fill())
                .height(Size::fill())
                .into_element();
        };
        image(handle)
            .width(Size::fill())
            .height(Size::fill())
            .aspect_ratio(AspectRatio::Max)
            .image_cover(ImageCover::Center)
            .into_element()
    }
}

/// Connects to the compositor and runs the wallpaper component.
pub fn run() -> anyhow::Result<()> {
    let store_path = data_dir().join("wallpaper.domain");
    let state_path = data_dir().join("wallpaper.state");

    let snapshot = load_snapshot(&store_path, &state_path, None);
    let state = State::create_global(snapshot);
    let app = AppComponent::new(WallpaperApp {
        state,
        store_path,
        state_path,
    });

    run_layer(
        app,
        SurfaceSpec::Layer {
            namespace: "yuki-wallpaper".to_string(),
            layer: Layer::Background,
            anchors: Anchors::ALL,
            exclusive_zone: 0,
            keyboard: KeyboardInteractivity::None,
            size: None,
        },
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

/// Loads the selection, decodes it when it changed, and writes the dominant
/// colour for the compositor and the portal.
fn load_snapshot(store_path: &Path, state_path: &Path, current: Option<PathBuf>) -> Snapshot {
    match load_wallpaper(store_path) {
        Ok(Some((path, scale))) => {
            if !scale.is_empty() && scale != "fill" {
                eprintln!("wallpaper: scale '{scale}' is not implemented yet; using 'fill'");
            }
            if current.as_deref() == Some(path.as_path()) {
                return Snapshot {
                    image: None,
                    current: Some(path),
                };
            }
            eprintln!("wallpaper: {path:?}");
            match decode_image(&path) {
                Ok(decoded) => {
                    let dominant = dominant(&decoded);
                    if let Err(error) = write_state(state_path, dominant) {
                        eprintln!("wallpaper: failed to write the state: {error:#}");
                    }
                    let handle = ImageHandle::from_rgba(
                        decoded.size.0,
                        decoded.size.1,
                        Bytes::from(decoded.rgba),
                        AlphaType::Premul,
                    );
                    Snapshot {
                        image: handle,
                        current: Some(path),
                    }
                }
                Err(error) => {
                    eprintln!("wallpaper: failed to decode {path:?}: {error:#}");
                    Snapshot {
                        image: None,
                        current: Some(path),
                    }
                }
            }
        }
        Ok(_) => Snapshot::default(),
        Err(error) => {
            eprintln!("wallpaper: {error:#}");
            Snapshot::default()
        }
    }
}

// --- wallpaper store -------------------------------------------------------

/// The `wallpaper.domain` store, as macOS keeps `com.apple.wallpaper`.
#[derive(Debug, Default, serde::Deserialize)]
struct WallpaperStore {
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
/// The store's selection wins. Without one (or without a store at all, as on
/// a fresh account) the default is [`yuki_design::WALLPAPER`] inside the
/// catalog folder; a custom folder that lacks it falls back to its first image.
fn load_wallpaper(store_path: &Path) -> anyhow::Result<Option<(PathBuf, String)>> {
    let store: WallpaperStore = match std::fs::read_to_string(store_path) {
        Ok(text) => toml::from_str(&text)?,
        Err(_) => WallpaperStore::default(),
    };
    if let Some(url) = &store.desktops.default
        && let Some(path) = file_url_to_path(url)
    {
        return Ok(Some((path, store.scale)));
    }
    let dir = if store.dir.is_empty() {
        PathBuf::from(yuki_design::WALLPAPER_DIR)
    } else {
        PathBuf::from(&store.dir)
    };
    let default = dir.join(yuki_design::WALLPAPER);
    if default.is_file() {
        return Ok(Some((default, store.scale)));
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg"
                    )
                })
        })
        .collect();
    entries.sort();
    Ok(entries.first().cloned().map(|p| (p, store.scale)))
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

/// Decodes a PNG or JPEG into top-down RGBA8 pixels.
fn decode_image(path: &Path) -> anyhow::Result<DecodedImage> {
    let bytes = std::fs::read(path)?;
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        decode_png(&bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        decode_jpeg(&bytes)
    } else {
        anyhow::bail!("unsupported image format (expected PNG or JPEG)")
    }
}

/// Decodes a PNG, normalizing 16-bit and indexed images to 8-bit colour.
fn decode_png(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
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
fn decode_jpeg(bytes: &[u8]) -> anyhow::Result<DecodedImage> {
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
fn dominant(image: &DecodedImage) -> Option<Rgba> {
    const SAMPLE_TARGET: f64 = 65_536.0;
    const MIN_CHROMA: f32 = 0.12;
    const MIN_DOMINANT_SHARE: f32 = 0.01;

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
    let mut sampled = 0usize;

    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            sampled += 1;
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
    if *weight < sampled as f32 * MIN_DOMINANT_SHARE {
        return None;
    }
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
fn write_state(path: &Path, dominant: Option<Rgba>) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match dominant {
        Some(color) => {
            let temporary = path.with_extension("state.tmp");
            std::fs::write(
                &temporary,
                format!(
                    "# Generated by yuki-shell wallpaper.\ndominant = [{:.6}, {:.6}, {:.6}]\n",
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
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("yuki-wallpaper-{name}-{}", std::process::id()));
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
        touch(&dir, "Black.png");
        let aqua = touch(&dir, yuki_design::WALLPAPER);
        let store = dir.join("wallpaper.domain");
        std::fs::write(&store, format!("dir = {:?}\n", dir.to_str().unwrap())).unwrap();
        let (path, _) = load_wallpaper(&store).unwrap().unwrap();
        assert_eq!(path, aqua);
    }

    #[test]
    fn the_store_selection_wins_over_the_default() {
        let dir = scratch("selection");
        touch(&dir, yuki_design::WALLPAPER);
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
    fn a_folder_without_the_default_falls_back_to_its_first_image() {
        let dir = scratch("custom");
        let black = touch(&dir, "Black.png");
        touch(&dir, "Yellow.png");
        let store = dir.join("wallpaper.domain");
        std::fs::write(&store, format!("dir = {:?}\n", dir.to_str().unwrap())).unwrap();
        let (path, _) = load_wallpaper(&store).unwrap().unwrap();
        assert_eq!(path, black);
    }

    fn fixture(name: &str) -> DecodedImage {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        decode_image(&path).unwrap()
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
    fn the_default_wallpaper_ships_with_yuki() {
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/wallpapers/solid-colors")
            .join(yuki_design::WALLPAPER);
        assert!(shipped.is_file(), "{shipped:?} is missing");
    }
}
