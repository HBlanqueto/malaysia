//! The current wallpaper's dominant colour, for Neptunia's `wallpaper` window
//! tint.
//!
//! The shell already decodes the image to show it, so it computes the dominant
//! colour and writes it to `~/.local/share/yuki/wallpaper.state`. The
//! compositor never decodes a picture: it reads that file, and the session
//! watches it (and `appearance.domain`) to rebuild [`crate::chrome::ChromeTheme`]
//! without a restart.

use serde::Deserialize;
use yuki_design::Rgba;

use crate::config::data_dir;

#[derive(Debug, Deserialize)]
struct State {
    dominant: Option<[f32; 3]>,
}

/// The dominant opaque colour written by `yuki-shell wallpaper`, if any.
pub fn dominant_color() -> Option<Rgba> {
    let text = std::fs::read_to_string(data_dir().join("wallpaper.state")).ok()?;
    dominant_from_str(&text)
}

fn dominant_from_str(text: &str) -> Option<Rgba> {
    let state: State = toml::from_str(text).ok()?;
    let [r, g, b] = state.dominant?;
    Some(Rgba {
        r: r.clamp(0.0, 1.0),
        g: g.clamp(0.0, 1.0),
        b: b.clamp(0.0, 1.0),
        a: 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_file_supplies_the_dominant_colour() {
        assert_eq!(
            dominant_from_str("dominant = [0.1, 0.5, 0.9]\n"),
            Some(Rgba {
                r: 0.1,
                g: 0.5,
                b: 0.9,
                a: 1.0,
            })
        );
    }

    #[test]
    fn a_missing_or_empty_state_is_none() {
        assert_eq!(dominant_from_str(""), None);
        assert_eq!(dominant_from_str("dominant = []\n"), None);
    }

    #[test]
    fn out_of_range_channels_are_clamped() {
        assert_eq!(
            dominant_from_str("dominant = [2.0, -1.0, 0.5]\n"),
            Some(Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.5,
                a: 1.0,
            })
        );
    }
}
