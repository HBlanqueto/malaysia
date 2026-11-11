use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Style {
    #[default]
    Regular,
    Italic,
    Rounded,
    Dots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fill {
    #[default]
    Solid,
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Numerals {
    #[default]
    Latn,
    Arab,
    Beng,
    Mymr,
    Knda,
    Deva,
    Guru,
    Gujr,
    Khmr,
    Mlym,
    Mtei,
    Olck,
    Orya,
    Telu,
}

impl Numerals {
    fn zero(self) -> u32 {
        match self {
            Self::Latn => 0x0030,
            Self::Arab => 0x0660,
            Self::Beng => 0x09E6,
            Self::Mymr => 0x1040,
            Self::Knda => 0x0CE6,
            Self::Deva => 0x0966,
            Self::Guru => 0x0A66,
            Self::Gujr => 0x0AE6,
            Self::Khmr => 0x17E0,
            Self::Mlym => 0x0D66,
            Self::Mtei => 0xABF0,
            Self::Olck => 0x1C50,
            Self::Orya => 0x0B66,
            Self::Telu => 0x0C66,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct ClockFace {
    clock_style: Style,
    clock_fill: Fill,
    clock_numerals: Numerals,
}

impl ClockFace {
    pub fn load() -> Self {
        bk::app::load_defaults("lock")
    }

    pub fn register(&self) -> &'static str {
        let (family, bytes) = self.font();
        if !bk::register_font(bytes) {
            tracing::warn!(family, "clock font not loaded");
        }
        family
    }

    pub fn localized(&self, time: &str) -> String {
        time.chars()
            .map(|ch| {
                ch.to_digit(10)
                    .and_then(|digit| char::from_u32(self.clock_numerals.zero() + digit))
                    .unwrap_or(ch)
            })
            .collect()
    }

    fn font(&self) -> (&'static str, &'static [u8]) {
        match (self.clock_fill, self.clock_style) {
            (Fill::Solid, Style::Regular) => (
                "Malaysia Clock Regular",
                include_bytes!("../fonts/MalaysiaClock-Regular.otf"),
            ),
            (Fill::Solid, Style::Italic) => (
                "Malaysia Clock Italic",
                include_bytes!("../fonts/MalaysiaClock-Italic.otf"),
            ),
            (Fill::Solid, Style::Rounded) => (
                "Malaysia Clock Rounded",
                include_bytes!("../fonts/MalaysiaClock-Rounded.otf"),
            ),
            (Fill::Solid, Style::Dots) => (
                "Malaysia Clock Dots",
                include_bytes!("../fonts/MalaysiaClock-Dots.otf"),
            ),
            (Fill::Outline, Style::Regular) => (
                "Malaysia Clock Outline Regular",
                include_bytes!("../fonts/MalaysiaClockOutline-Regular.otf"),
            ),
            (Fill::Outline, Style::Italic) => (
                "Malaysia Clock Outline Italic",
                include_bytes!("../fonts/MalaysiaClockOutline-Italic.otf"),
            ),
            (Fill::Outline, Style::Rounded) => (
                "Malaysia Clock Outline Rounded",
                include_bytes!("../fonts/MalaysiaClockOutline-Rounded.otf"),
            ),
            (Fill::Outline, Style::Dots) => (
                "Malaysia Clock Outline Dots",
                include_bytes!("../fonts/MalaysiaClockOutline-Dots.otf"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_is_written_with_the_chosen_numerals_and_keeps_the_colon() {
        let face: ClockFace =
            toml::from_str("clock_numerals = \"beng\"\nidle_timeout_seconds = 300").unwrap();
        assert_eq!(face.localized("10:28"), "১০:২৮");
    }
}
