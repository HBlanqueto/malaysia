//! Estado compartido de Ajustes: config, filtro, cuenta y helpers de color.

pub use vow_appearance::{
    AppearanceConfig, ColorScheme, Resolved, TrafficLightStyle, WindowColorMode, WindowTintScope,
    load_appearance, palette, resolve, write_appearance,
};
pub use vow_design::Rgba;

/// Convierte un [`Rgba`] de `vow-design` en un [`Color`] de Freya.
pub fn rgba(color: Rgba) -> freya_core::prelude::Color {
    let [r, g, b, a] = color.to_rgba8();
    freya_core::prelude::Color::from_argb(a, r, g, b)
}

/// Escribe la config y devuelve el error como string si falla.
pub fn save_config(config: &AppearanceConfig) -> Option<String> {
    write_appearance(&vow_appearance::appearance_path(), config)
        .err()
        .map(|e| e.to_string())
}

/// Nombre completo de la cuenta para mostrar.
pub fn account_name() -> String {
    full_account_name().unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| "Usuario".into())
    })
}

/// Returns the user's full name from `passwd` (the `pw_gecos` field), or `None`
/// when the lookup fails. The first comma-separated entry is the display name.
fn full_account_name() -> Option<String> {
    // SAFETY: `getpwuid` returns a pointer to static libc data; we only read
    // it and copy the resulting C string before the next call.
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

/// Iniciales de hasta dos palabras del nombre.
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// Decide si `query` aparece en `haystack` tras normalizar tildes y mayúsculas.
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

/// Fondo de tarjeta: el color dominante del wallpaper, o el acento si no hay.
pub fn card_background(dominant: Option<Rgba>, accent: Rgba) -> Rgba {
    dominant.unwrap_or(accent)
}
