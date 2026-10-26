//! Estado compartido de Ajustes: config, filtro, cuenta y helpers de color.

pub use vow_appearance::{
    AppearanceConfig, ColorScheme, Resolved, TrafficLightStyle, WindowColorMode, WindowTintScope,
    load_appearance, palette, read_dominant, resolve, write_appearance,
};
pub use vow_design::Rgba;
use vow_design::{
    BG_CONTROL_DARK, BG_CONTROL_DARK_TINT, BG_CONTROL_LIGHT, BG_SURFACE_DARK, BG_SURFACE_DARK_TINT,
    BG_SURFACE_LIGHT, BG_WINDOW_DARK, BG_WINDOW_DARK_TINT, BG_WINDOW_LIGHT,
};

/// Convierte un [`Rgba`] de `vow-design` en un [`Color`] de Freya.
pub fn rgba(color: Rgba) -> freya_core::prelude::Color {
    let [r, g, b, a] = color.to_rgba8();
    freya_core::prelude::Color::from_argb(a, r, g, b)
}

/// Fondo de ventana para el esquema actual.
pub fn bg_window(scheme: ColorScheme) -> Rgba {
    match scheme {
        ColorScheme::DarkTint => BG_WINDOW_DARK_TINT,
        ColorScheme::Dark => BG_WINDOW_DARK,
        _ => BG_WINDOW_LIGHT,
    }
}

/// Fondo de control para el esquema actual.
pub fn bg_control(scheme: ColorScheme) -> Rgba {
    match scheme {
        ColorScheme::DarkTint => BG_CONTROL_DARK_TINT,
        ColorScheme::Dark => BG_CONTROL_DARK,
        _ => BG_CONTROL_LIGHT,
    }
}

/// Fondo de superficie para el esquema actual.
pub fn bg_surface(scheme: ColorScheme) -> Rgba {
    match scheme {
        ColorScheme::DarkTint => BG_SURFACE_DARK_TINT,
        ColorScheme::Dark => BG_SURFACE_DARK,
        _ => BG_SURFACE_LIGHT,
    }
}

/// Carga la apariencia y el color dominante actuales.
pub fn load_resolved() -> (AppearanceConfig, Option<Rgba>, Resolved) {
    let config = load_appearance();
    let dominant = read_dominant();
    let resolved = resolve(&config, dominant);
    (config, dominant, resolved)
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

/// Fondo de ventana teñido según el modo de color de ventanas.
pub fn tinted_bg_window(preview_resolved: &Resolved) -> Rgba {
    let base = match preview_resolved.scheme {
        ColorScheme::DarkTint => BG_WINDOW_DARK_TINT,
        ColorScheme::Dark => BG_WINDOW_DARK,
        _ => BG_WINDOW_LIGHT,
    };
    match preview_resolved.window_tint {
        Some((tint, factor)) => base.mix(tint, factor),
        None => base,
    }
}

/// Fondo hundido teñido según el modo de color de ventanas.
pub fn tinted_bg_sunken(preview_resolved: &Resolved) -> Rgba {
    let base = palette(preview_resolved.scheme).titlebar_bg.unfocused;
    match preview_resolved.window_tint {
        Some((tint, factor)) => base.mix(tint, factor * vow_design::LAYER_FACTOR_BG_SUNKEN),
        None => base,
    }
}
