use std::borrow::Cow;
use std::path::Path;

use crate::dynamic::Dynamic;
use crate::image::{
    DEFAULT_WALLPAPER_BYTES, DecodedImage, decode_heif, decode_image_bytes, is_heif,
};

pub(crate) struct Decoded {
    pub(crate) dynamic: Option<Dynamic>,
    pub(crate) image: usize,
    pub(crate) source: (u32, u32),
    pub(crate) pixels: DecodedImage,
}

pub(crate) fn decode_wallpaper(
    file: Option<&Path>,
    pick: impl FnOnce(&Dynamic) -> usize,
    texels_needed: impl FnOnce((u32, u32)) -> Option<(u32, u32)>,
) -> anyhow::Result<Decoded> {
    let bytes = match file {
        Some(file) => Cow::Owned(std::fs::read(file)?),
        None => Cow::Borrowed(DEFAULT_WALLPAPER_BYTES),
    };
    let dynamic = file
        .is_none_or(is_heif)
        .then(|| Dynamic::of(&bytes))
        .flatten();
    let image = dynamic.as_ref().map_or(0, pick);
    let decoded = match dynamic {
        Some(_) => decode_heif(&bytes, Some(image))?,
        None => decode_image_bytes(&bytes)?,
    };
    let source = decoded.size;
    let downscaled = texels_needed(source).and_then(|need| match decoded.downscale_to_need(need) {
        Cow::Owned(pixels) => Some(pixels),
        Cow::Borrowed(_) => None,
    });
    Ok(Decoded {
        dynamic,
        image,
        source,
        pixels: downscaled.unwrap_or(decoded),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_is_halved_down_to_what_the_outputs_need() {
        let decoded = decode_wallpaper(None, |_| 0, |_| Some((1500, 1500))).expect("the default");
        assert_eq!(decoded.source, (6016, 6016));
        assert_eq!(decoded.pixels.size, (1504, 1504));
    }

    #[test]
    fn an_image_no_larger_than_needed_keeps_its_size() {
        let decoded = decode_wallpaper(None, |_| 0, Some).expect("the default");
        assert_eq!(decoded.pixels.size, decoded.source);
    }
}
