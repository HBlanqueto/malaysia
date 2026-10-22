//! The current wallpaper's dominant colour, for Neptunia's `wallpaper` window
//! tint.
//!
//! The shell decodes the image for display; the compositor only needs one
//! colour, so it reads the same store and reduces the PNG to a histogram. A
//! missing or unreadable store falls back to the default wallpaper, and a
//! decode failure is not fatal: the theme falls back to the accent.

use std::path::PathBuf;

use serde::Deserialize;
use yuki_design::{Rgba, WALLPAPER, WALLPAPER_DIR};

use crate::config::data_dir;

#[derive(Debug, Default, Deserialize)]
struct Store {
    #[serde(default)]
    dir: String,
    #[serde(default)]
    desktops: Desktops,
}

#[derive(Debug, Default, Deserialize)]
struct Desktops {
    #[serde(default)]
    default: Option<String>,
}

/// The dominant opaque colour of the current wallpaper, if it can be read.
pub(crate) fn dominant_color() -> Option<Rgba> {
    let path = current_path()?;
    let file = std::fs::File::open(&path).ok()?;
    let mut reader = png::Decoder::new(file).read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    let rgba = to_rgba(info.color_type, &buffer[..info.buffer_size()])?;
    dominant(&rgba, info.width as usize, info.height as usize)
}

fn current_path() -> Option<PathBuf> {
    let store: Store = std::fs::read_to_string(data_dir().join("wallpaper.domain"))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    if let Some(url) = &store.desktops.default
        && let Some(path) = file_url_to_path(url)
        && path.is_file()
    {
        return Some(path);
    }
    let dir = if store.dir.is_empty() {
        PathBuf::from(WALLPAPER_DIR)
    } else {
        PathBuf::from(&store.dir)
    };
    let default = dir.join(WALLPAPER);
    if default.is_file() {
        return Some(default);
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect();
    entries.sort();
    entries.into_iter().next()
}

fn to_rgba(color: png::ColorType, bytes: &[u8]) -> Option<Vec<u8>> {
    let mut rgba = Vec::with_capacity(bytes.len() / 3 * 4);
    match color {
        png::ColorType::Rgba => return Some(bytes.to_vec()),
        png::ColorType::Rgb => {
            for pixel in bytes.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &value in bytes {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for pixel in bytes.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        png::ColorType::Indexed => return None,
    }
    Some(rgba)
}

/// The most frequent 4-bit RGB bucket, averaged within that bucket.
fn dominant(rgba: &[u8], width: usize, height: usize) -> Option<Rgba> {
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return None;
    }
    let mut counts = [0u32; 4096];
    let mut sums = [[0u64; 3]; 4096];
    let step = ((width * height) / 100_000).max(1);
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let index = (y * width + x) * 4;
            let [r, g, b, a] = [
                rgba[index],
                rgba[index + 1],
                rgba[index + 2],
                rgba[index + 3],
            ];
            if a < 128 {
                continue;
            }
            let bucket =
                (usize::from(r >> 4) << 8) | (usize::from(g >> 4) << 4) | usize::from(b >> 4);
            counts[bucket] += 1;
            sums[bucket][0] += u64::from(r);
            sums[bucket][1] += u64::from(g);
            sums[bucket][2] += u64::from(b);
        }
    }
    let bucket = counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, count)| *count)
        .map(|(bucket, _)| bucket)?;
    let count = counts[bucket];
    if count == 0 {
        return None;
    }
    let [r, g, b] = sums[bucket];
    Some(Rgba {
        r: r as f32 / (count as f32 * 255.0),
        g: g as f32 / (count as f32 * 255.0),
        b: b as f32 / (count as f32 * 255.0),
        a: 1.0,
    })
}

fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    Some(PathBuf::from(
        String::from_utf8_lossy(&percent_decode(rest.as_bytes())).into_owned(),
    ))
}

fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    out
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solid_image_has_that_colour() {
        let pixels = [10u8, 20, 30, 255].repeat(16);
        assert_eq!(
            dominant(&pixels, 4, 4),
            Some(Rgba {
                r: 10.0 / 255.0,
                g: 20.0 / 255.0,
                b: 30.0 / 255.0,
                a: 1.0
            })
        );
    }

    #[test]
    fn the_most_frequent_bucket_wins() {
        let mut pixels = Vec::new();
        for _ in 0..12 {
            pixels.extend_from_slice(&[200, 10, 10, 255]);
        }
        for _ in 0..4 {
            pixels.extend_from_slice(&[10, 10, 200, 255]);
        }
        let colour = dominant(&pixels, 4, 4).expect("a dominant colour");
        assert!(colour.r > 0.7);
        assert!(colour.b < 0.2);
    }

    #[test]
    fn transparent_pixels_are_ignored() {
        let mut pixels = vec![0u8; 16];
        pixels.extend_from_slice(&[255, 255, 255, 255].repeat(4));
        assert_eq!(
            dominant(&pixels, 4, 2),
            Some(Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0
            })
        );
    }
}
