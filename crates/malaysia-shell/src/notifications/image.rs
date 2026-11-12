use std::io::{BufWriter, Read};
use std::os::fd::AsFd;
use std::path::PathBuf;

use zbus::zvariant::{OwnedValue, Structure, Value};

use super::store::Notification;

fn dir() -> PathBuf {
    ctappearance::data_dir().join("notifications")
}

fn target(id: u32) -> Option<PathBuf> {
    let dir = dir();
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(id.to_string()))
}

// Desktop Notifications spec: image-data wins over image-path; the older names are deprecated aliases.
pub(crate) fn from_hints(
    id: u32,
    hints: &std::collections::HashMap<String, OwnedValue>,
) -> Option<String> {
    ["image-data", "image_data", "icon_data"]
        .iter()
        .find_map(|key| hints.get(*key))
        .and_then(|value| save_pixels(id, value))
        .or_else(|| {
            ["image-path", "image_path"]
                .iter()
                .find_map(|key| hints.get(*key))
                .and_then(|value| String::try_from(value.clone()).ok())
                .and_then(|path| local_file(&path))
        })
}

// A serialized GIcon: ("themed", names) names the app icon, so only files and bytes are content.
pub(crate) fn from_gicon(id: u32, value: &OwnedValue) -> Option<String> {
    let structure: Structure = value.try_clone().ok()?.try_into().ok()?;
    let [Value::Str(kind), Value::Value(inner)] = structure.fields() else {
        return None;
    };
    match (kind.as_str(), &**inner) {
        ("file", Value::Str(uri)) => local_file(uri),
        ("bytes", bytes) => save_bytes(id, Vec::<u8>::try_from(bytes.try_clone().ok()?).ok()?),
        ("file-descriptor", Value::Fd(fd)) => {
            let mut bytes = Vec::new();
            std::fs::File::from(fd.as_fd().try_clone_to_owned().ok()?)
                .read_to_end(&mut bytes)
                .ok()?;
            save_bytes(id, bytes)
        }
        _ => None,
    }
}

fn local_file(path: &str) -> Option<String> {
    let path = path.strip_prefix("file://").unwrap_or(path);
    std::path::Path::new(path)
        .is_file()
        .then(|| path.to_string())
}

fn save_bytes(id: u32, bytes: Vec<u8>) -> Option<String> {
    let path = target(id)?;
    std::fs::write(&path, bytes).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn save_pixels(id: u32, value: &OwnedValue) -> Option<String> {
    let (width, height, rowstride, alpha, bits, channels, data): (
        i32,
        i32,
        i32,
        bool,
        i32,
        i32,
        Vec<u8>,
    ) = value.try_clone().ok()?.try_into().ok()?;
    let row = usize::try_from(width).ok()? * usize::try_from(channels).ok()?;
    let rowstride = usize::try_from(rowstride).ok()?;
    if bits != 8 || channels != if alpha { 4 } else { 3 } || row == 0 || row > rowstride {
        return None;
    }
    let packed: Vec<u8> = data
        .chunks(rowstride)
        .take(usize::try_from(height).ok()?)
        .filter(|line| line.len() >= row)
        .flat_map(|line| &line[..row])
        .copied()
        .collect();
    if packed.len() != row * usize::try_from(height).ok()? {
        return None;
    }
    let path = target(id)?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(std::fs::File::create(&path).ok()?),
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
    );
    encoder.set_color(if alpha {
        png::ColorType::Rgba
    } else {
        png::ColorType::Rgb
    });
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .ok()?
        .write_image_data(&packed)
        .ok()?;
    Some(path.to_string_lossy().into_owned())
}

pub(crate) fn prune<'a>(kept: impl Iterator<Item = &'a Notification> + Clone) {
    let Ok(entries) = std::fs::read_dir(dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let used = kept
            .clone()
            .any(|notification| notification.image.as_deref() == path.to_str());
        if !used {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_without_a_row_are_refused() {
        let empty = Structure::from((0i32, 1i32, 0i32, false, 8i32, 3i32, Vec::<u8>::new()));
        let value = OwnedValue::try_from(Value::from(empty)).unwrap();
        assert_eq!(save_pixels(1, &value), None);
    }
}
