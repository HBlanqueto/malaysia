use std::fs::{DirBuilder, OpenOptions};
use std::io::BufWriter;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use ctappearance::{
    WallpaperAppearance, WallpaperKind, file_uri, frame_path, modified_seconds, thumbnail_is_fresh,
    thumbnail_path,
};
use ctdesign::{SETTINGS, WALLPAPER_FRAME, WALLPAPER_THUMBNAIL};

use crate::dynamic::Dynamic;
use crate::image::{DecodedImage, decode_heif, is_heif};
use crate::shader::Shader;
use crate::video::Video;

pub fn spawn(files: Vec<PathBuf>, device: wgpu::Device, queue: wgpu::Queue) {
    let spawned = std::thread::Builder::new()
        .name("wallpaper thumbnails".into())
        .spawn(move || {
            for file in files {
                if let Err(error) = previews(&file, &device, &queue) {
                    tracing::warn!(?file, %error, "cannot make the wallpaper's thumbnail");
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot start the thumbnail thread");
    }
}

fn previews(file: &Path, device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<()> {
    let Some(thumbnail) = thumbnail_path(file) else {
        return Ok(());
    };
    if thumbnail_is_fresh(&thumbnail, file) {
        return Ok(());
    }
    let image = match WallpaperKind::of(file) {
        Some(WallpaperKind::Image) if is_heif(file) => heif(file)?,
        Some(WallpaperKind::Video) => Video::open(file, None)?
            .take()
            .ok_or_else(|| anyhow::anyhow!("the video gave no frame"))?,
        Some(WallpaperKind::Shader) => {
            let size = (
                WALLPAPER_THUMBNAIL,
                WALLPAPER_THUMBNAIL * SETTINGS.wallpaper.tile_height
                    / SETTINGS.wallpaper.tile_width,
            );
            Shader::new(device, &std::fs::read_to_string(file)?)?.snapshot(device, queue, size)?
        }
        _ => return Ok(()),
    };
    write_png(
        &thumbnail,
        file,
        &image.downscale_to_fit(WALLPAPER_THUMBNAIL),
    )
}

// The frames are written before the thumbnail, so a fresh thumbnail means fresh frames.
fn heif(file: &Path) -> anyhow::Result<DecodedImage> {
    let bytes = std::fs::read(file)?;
    let Some(dynamic) = Dynamic::of(&bytes) else {
        return decode_heif(&bytes, None);
    };
    let light = decode_heif(&bytes, Some(dynamic.light))?;
    let dark = decode_heif(&bytes, Some(dynamic.dark))?;
    for (appearance, image) in [
        (WallpaperAppearance::Light, &light),
        (WallpaperAppearance::Dark, &dark),
    ] {
        if let Some(frame) = frame_path(file, appearance) {
            write_png(&frame, file, &image.downscale_to_fit(WALLPAPER_FRAME))?;
        }
    }
    Ok(light)
}

fn write_png(path: &Path, file: &Path, image: &DecodedImage) -> anyhow::Result<()> {
    let folder = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("the thumbnail has no folder"))?;
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)?;
    let partial = path.with_extension("png.part");
    let output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&partial)?;
    let (width, height) = image.size;
    let mut encoder = png::Encoder::new(BufWriter::new(output), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.add_text_chunk("Thumb::URI".into(), file_uri(file))?;
    let modified = modified_seconds(file).unwrap_or_default();
    encoder.add_text_chunk("Thumb::MTime".into(), modified.to_string())?;
    let rgba8: Vec<u8> = (0..(width * height) as usize)
        .flat_map(|pixel| image.rgba8_at(pixel * 4))
        .collect();
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba8)?;
    writer.finish()?;
    std::fs::rename(partial, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Pixels;

    #[test]
    fn a_written_thumbnail_is_fresh_for_its_file() {
        let dir = std::env::temp_dir().join(format!("malaysia-thumbnail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tide.frag");
        std::fs::write(&file, "void mainImage(out vec4 c, in vec2 p) {}").unwrap();
        let thumbnail = dir.join("cache").join("thumbnail.png");
        let image = DecodedImage {
            size: (2, 1),
            pixels: Pixels::Srgb8(vec![255; 8]),
        };
        write_png(&thumbnail, &file, &image).unwrap();
        assert!(thumbnail_is_fresh(&thumbnail, &file));
        std::fs::remove_dir_all(&dir).ok();
    }
}
