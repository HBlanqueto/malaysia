//! `org.freedesktop.impl.portal.Settings`, backed by `appearance.domain`.
//!
//! The values are read-only for apps. The portal watches the domain file and
//! emits `SettingChanged` when it changes, so toolkits can follow dark/light,
//! the accent and the accessibility switches without restarting.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use yuki::config::{AppearanceConfig, ColorScheme, config_dir};
use yuki_design::{DARK, LIGHT};
use zbus::blocking::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

/// The only namespace this backend publishes.
pub const NAMESPACE: &str = "org.freedesktop.appearance";

const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const WATCH_INTERVAL: Duration = Duration::from_millis(250);

/// The four standardized values, already mapped for D-Bus.
#[derive(Debug, Clone, PartialEq)]
pub struct Values {
    color_scheme: u32,
    accent: (f64, f64, f64),
    contrast: u32,
    reduced_motion: u32,
}

impl Values {
    /// Reads the current `appearance.domain`, or the defaults when it is absent.
    pub fn load() -> Self {
        match read_appearance() {
            Ok(appearance) => Self::from_appearance(appearance.as_ref()),
            Err(error) => {
                tracing::warn!(%error, "appearance.domain ignored; using defaults");
                Self::from_appearance(None)
            }
        }
    }

    fn from_appearance(appearance: Option<&AppearanceConfig>) -> Self {
        let default = AppearanceConfig::default();
        let appearance = appearance.unwrap_or(&default);
        let color_scheme = match appearance.color_scheme {
            ColorScheme::Auto => 0,
            ColorScheme::Dark => 1,
            ColorScheme::Light => 2,
        };
        // macOS's default accent is the system blue; use the palette that
        // matches the requested scheme when the user has not chosen one.
        let palette = if color_scheme == 2 { &LIGHT } else { &DARK };
        let accent = appearance.accent_color.map_or(
            (
                f64::from(palette.accent.r),
                f64::from(palette.accent.g),
                f64::from(palette.accent.b),
            ),
            |[r, g, b]| {
                (
                    f64::from(r).clamp(0.0, 1.0),
                    f64::from(g).clamp(0.0, 1.0),
                    f64::from(b).clamp(0.0, 1.0),
                )
            },
        );
        Self {
            color_scheme,
            accent,
            contrast: u32::from(appearance.contrast),
            reduced_motion: u32::from(appearance.reduced_motion),
        }
    }

    fn value(&self, key: &str) -> Option<OwnedValue> {
        let value = match key {
            "color-scheme" => Value::new(self.color_scheme),
            "accent-color" => Value::new(self.accent),
            "contrast" => Value::new(self.contrast),
            "reduced-motion" => Value::new(self.reduced_motion),
            _ => return None,
        };
        Some(owned(value))
    }

    fn all(&self) -> HashMap<String, OwnedValue> {
        ["color-scheme", "accent-color", "contrast", "reduced-motion"]
            .into_iter()
            .filter_map(|key| self.value(key).map(|value| (key.to_string(), value)))
            .collect()
    }

    /// The keys that differ from `old`, with their new values.
    fn changed(&self, old: &Self) -> Vec<(&'static str, OwnedValue)> {
        ["color-scheme", "accent-color", "contrast", "reduced-motion"]
            .into_iter()
            .filter_map(|key| {
                let new = self.value(key)?;
                (Some(&new) != old.value(key).as_ref()).then_some((key, new))
            })
            .collect()
    }
}

/// The D-Bus interface served at `/org/freedesktop/portal/desktop`.
pub struct Settings {
    values: Arc<RwLock<Values>>,
}

impl Settings {
    /// Serves the values held by `values`; the watcher updates them.
    pub fn new(values: Arc<RwLock<Values>>) -> Self {
        Self { values }
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Settings")]
impl Settings {
    fn read(&self, namespace: &str, key: &str) -> Result<OwnedValue, PortalError> {
        if namespace != NAMESPACE {
            return Err(PortalError::NotFound(format!(
                "unknown settings namespace {namespace}"
            )));
        }
        self.values
            .read()
            .expect("the settings lock is not poisoned")
            .value(key)
            .ok_or_else(|| PortalError::NotFound(format!("unknown settings key {key}")))
    }

    fn read_all(&self, namespaces: Vec<String>) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut result = HashMap::new();
        if namespaces.is_empty() || namespaces.iter().any(|pattern| namespace_matches(pattern)) {
            result.insert(
                NAMESPACE.to_string(),
                self.values
                    .read()
                    .expect("the settings lock is not poisoned")
                    .all(),
            );
        }
        result
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        2
    }

    #[zbus(signal)]
    async fn setting_changed(
        emitter: &SignalEmitter<'_>,
        namespace: &str,
        key: &str,
        value: OwnedValue,
    ) -> zbus::Result<()>;
}

/// The portal error apps see for a namespace or key that is not published.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.portal.Error", impl_display = true)]
enum PortalError {
    NotFound(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

/// Watches `appearance.domain` and emits `SettingChanged` for each key.
pub fn watch(connection: Connection, values: Arc<RwLock<Values>>) {
    std::thread::spawn(move || {
        let path = appearance_path();
        let mut last = mtime(&path);
        loop {
            std::thread::sleep(WATCH_INTERVAL);
            let now = mtime(&path);
            if now == last {
                continue;
            }
            last = now;
            let appearance = match read_appearance() {
                Ok(appearance) => appearance,
                Err(error) => {
                    tracing::warn!(%error, "appearance.domain unreadable; keeping the last values");
                    continue;
                }
            };
            let new = Values::from_appearance(appearance.as_ref());
            let old = {
                let mut guard = values.write().expect("the settings lock is not poisoned");
                std::mem::replace(&mut *guard, new.clone())
            };
            let Ok(interface) = connection
                .object_server()
                .interface::<_, Settings>(OBJECT_PATH)
            else {
                tracing::warn!("the Settings interface is not registered");
                continue;
            };
            for (key, value) in new.changed(&old) {
                if let Err(error) = zbus::block_on(Settings::setting_changed(
                    interface.signal_emitter(),
                    NAMESPACE,
                    key,
                    value,
                )) {
                    tracing::warn!(%error, key, "could not emit SettingChanged");
                }
            }
        }
    });
}

fn appearance_path() -> PathBuf {
    config_dir().join("appearance.domain")
}

fn read_appearance() -> Result<Option<AppearanceConfig>, toml::de::Error> {
    let Ok(text) = std::fs::read_to_string(appearance_path()) else {
        return Ok(None);
    };
    toml::from_str(&text).map(Some)
}

fn mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

fn namespace_matches(pattern: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => NAMESPACE.starts_with(prefix),
        None => pattern == NAMESPACE,
    }
}

fn owned(value: Value<'static>) -> OwnedValue {
    OwnedValue::try_from(value).expect("a zvariant value is always ownable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_color_scheme_maps_to_the_portal_values() {
        let auto = AppearanceConfig::default();
        assert_eq!(Values::from_appearance(Some(&auto)).color_scheme, 0);
        let dark = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            ..AppearanceConfig::default()
        };
        assert_eq!(Values::from_appearance(Some(&dark)).color_scheme, 1);
        let light = AppearanceConfig {
            color_scheme: ColorScheme::Light,
            ..AppearanceConfig::default()
        };
        assert_eq!(Values::from_appearance(Some(&light)).color_scheme, 2);
    }

    #[test]
    fn the_accent_falls_back_to_the_scheme_palette() {
        let light = AppearanceConfig {
            color_scheme: ColorScheme::Light,
            ..AppearanceConfig::default()
        };
        let light = Values::from_appearance(Some(&light)).accent;
        assert_eq!(light.0, f64::from(LIGHT.accent.r));
        let dark = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            ..AppearanceConfig::default()
        };
        let dark = Values::from_appearance(Some(&dark)).accent;
        assert_eq!(dark.2, f64::from(DARK.accent.b));
    }

    #[test]
    fn an_explicit_accent_is_clamped() {
        let appearance = AppearanceConfig {
            accent_color: Some([2.0, -1.0, 0.5]),
            ..AppearanceConfig::default()
        };
        assert_eq!(
            Values::from_appearance(Some(&appearance)).accent,
            (1.0, 0.0, 0.5)
        );
    }

    #[test]
    fn namespaces_use_trailing_globs() {
        assert!(namespace_matches(""));
        assert!(namespace_matches(NAMESPACE));
        assert!(namespace_matches("org.freedesktop.*"));
        assert!(!namespace_matches("org.gnome.*"));
        assert!(!namespace_matches("org.freedesktop.appearance.extra"));
    }

    #[test]
    fn only_changed_keys_are_reported() {
        let old = Values::from_appearance(None);
        let mut new = old.clone();
        new.contrast = 1;
        let changed: Vec<_> = new.changed(&old).into_iter().map(|(key, _)| key).collect();
        assert_eq!(changed, ["contrast"]);
    }
}
