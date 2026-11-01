//! Shared settings state: config, filter, account and color helpers.

pub use vow_appearance::{
    AppearanceConfig, ColorScheme, Resolved, TintParts, TintStrength, TrafficLightStyle,
    WindowColorMode, WindowTintScope, load_appearance, load_dominant, palette, resolve,
    write_appearance,
};
pub use vow_design::Rgba;

pub fn rgba(color: Rgba) -> freya_core::prelude::Color {
    let [r, g, b, a] = color.to_rgba8();
    freya_core::prelude::Color::from_argb(a, r, g, b)
}

pub fn save_config(config: &AppearanceConfig) -> Option<String> {
    write_appearance(&vow_appearance::appearance_path(), config)
        .err()
        .map(|e| e.to_string())
}

pub fn account_name() -> String {
    full_account_name().unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| "Usuario".into())
    })
}

fn full_account_name() -> Option<String> {
    // SAFETY: `getpwuid` returns static libc data that is only read here.
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    if pw.is_null() {
        return None;
    }
    let gecos = unsafe { std::ffi::CStr::from_ptr((*pw).pw_gecos) };
    gecos
        .to_str()
        .ok()?
        .split(',')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

pub fn matches_filter(query: &str, haystack: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let normalized = normalize(query);
    normalize(haystack)
        .split_whitespace()
        .any(|word| word.contains(&normalized))
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .replace('á', "a")
        .replace('é', "e")
        .replace('í', "i")
        .replace('ó', "o")
        .replace('ú', "u")
        .replace('ñ', "n")
}

pub fn card_background(dominant: Option<Rgba>, accent: Rgba) -> Rgba {
    dominant.unwrap_or(accent)
}

pub fn tint_scope_disabled(window_color: WindowColorMode) -> bool {
    window_color == WindowColorMode::Neutral
}
