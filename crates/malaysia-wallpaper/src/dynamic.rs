use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ctappearance::WallpaperAppearance;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq)]
pub struct Dynamic {
    pub light: usize,
    pub dark: usize,
    times: Vec<(f64, usize)>,
}

#[derive(Deserialize)]
struct Pair {
    l: usize,
    d: usize,
}

#[derive(Deserialize)]
struct Time {
    t: f64,
    i: usize,
}

#[derive(Deserialize)]
struct Day {
    ap: Pair,
    #[serde(default)]
    ti: Vec<Time>,
}

impl Dynamic {
    pub fn of(file: &[u8]) -> Option<Self> {
        if let Some(day) = xmp_plist::<Day>(file, "h24") {
            let mut times: Vec<(f64, usize)> = day.ti.iter().map(|time| (time.t, time.i)).collect();
            times.sort_by(|a, b| a.0.total_cmp(&b.0));
            return Some(Self {
                light: day.ap.l,
                dark: day.ap.d,
                times,
            });
        }
        // Following the sun needs the user's location; a solar wallpaper follows the appearance.
        let pair = xmp_plist::<Pair>(file, "apr")
            .or_else(|| xmp_plist::<Day>(file, "solar").map(|day| day.ap))?;
        Some(Self {
            light: pair.l,
            dark: pair.d,
            times: Vec::new(),
        })
    }

    pub fn image(&self, still: Option<WallpaperAppearance>, dark: bool, day: f64) -> usize {
        match still {
            Some(WallpaperAppearance::Light) => self.light,
            Some(WallpaperAppearance::Dark) => self.dark,
            None if self.times.is_empty() => {
                if dark {
                    self.dark
                } else {
                    self.light
                }
            }
            None => self
                .times
                .iter()
                .rev()
                .find(|(time, _)| *time <= day)
                .or(self.times.last())
                .map_or(self.light, |&(_, image)| image),
        }
    }
}

fn xmp_plist<T: serde::de::DeserializeOwned>(file: &[u8], key: &str) -> Option<T> {
    let name = format!("apple_desktop:{key}");
    let start = file
        .windows(name.len())
        .position(|window| window == name.as_bytes())?
        + name.len();
    let rest = &file[start..];
    let rest = rest
        .strip_prefix(b"=\"")
        .or_else(|| rest.strip_prefix(b">"))?;
    let end = rest.iter().position(|&byte| byte == b'"' || byte == b'<')?;
    let plist = STANDARD.decode(&rest[..end]).ok()?;
    plist::from_bytes(&plist).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xmp(key: &str, value: plist::Value) -> Vec<u8> {
        let mut binary = Vec::new();
        value.to_writer_binary(&mut binary).unwrap();
        format!(
            "<x:xmpmeta><rdf:Description apple_desktop:{key}=\"{}\"/></x:xmpmeta>",
            STANDARD.encode(binary)
        )
        .into_bytes()
    }

    fn dict(entries: &[(&str, plist::Value)]) -> plist::Value {
        plist::Value::Dictionary(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        )
    }

    fn pair(light: u64, dark: u64) -> plist::Value {
        dict(&[("l", light.into()), ("d", dark.into())])
    }

    fn day() -> Dynamic {
        let time = |t: f64, i: u64| dict(&[("t", t.into()), ("i", i.into())]);
        let times = plist::Value::Array(vec![time(0.75, 2), time(0.25, 0), time(0.5, 1)]);
        Dynamic::of(&xmp("h24", dict(&[("ap", pair(0, 2)), ("ti", times)]))).unwrap()
    }

    #[test]
    fn an_appearance_wallpaper_shows_its_light_or_dark_image() {
        let dynamic = Dynamic::of(&xmp("apr", pair(0, 1))).unwrap();
        assert_eq!(dynamic.image(None, false, 0.5), 0);
        assert_eq!(dynamic.image(None, true, 0.5), 1);
    }

    #[test]
    fn a_time_wallpaper_shows_the_image_of_the_last_time_passed() {
        assert_eq!(day().image(None, false, 0.3), 0);
        assert_eq!(day().image(None, true, 0.6), 1);
        assert_eq!(day().image(None, false, 0.9), 2);
    }

    #[test]
    fn before_the_first_time_a_time_wallpaper_shows_the_last_image_of_the_day() {
        assert_eq!(day().image(None, false, 0.1), 2);
    }

    #[test]
    fn a_still_appearance_wins_over_the_time() {
        assert_eq!(day().image(Some(WallpaperAppearance::Dark), false, 0.3), 2);
        assert_eq!(day().image(Some(WallpaperAppearance::Light), true, 0.9), 0);
    }

    #[test]
    fn a_solar_wallpaper_follows_the_appearance() {
        let solar = Dynamic::of(&xmp("solar", dict(&[("ap", pair(3, 4))]))).unwrap();
        assert_eq!(solar.image(None, true, 0.5), 4);
    }

    #[test]
    fn a_file_without_apple_metadata_is_not_dynamic() {
        assert_eq!(Dynamic::of(b"<x:xmpmeta/>"), None);
    }

    #[test]
    fn collapse_is_light_then_dark() {
        let collapse = Dynamic::of(crate::image::DEFAULT_WALLPAPER_BYTES).unwrap();
        assert_eq!((collapse.light, collapse.dark), (0, 1));
    }
}
