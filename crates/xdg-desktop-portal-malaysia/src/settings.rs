//! `org.freedesktop.impl.portal.Settings`, backed by `appearance.domain`.
//!
//! The values are read-only for apps. The portal watches the domain file and
//! emits `SettingChanged` when it changes, so toolkits can follow dark/light,
//! the accent and the accessibility switches without restarting.

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::sync::{Arc, RwLock};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use vow_appearance::{
    AppearanceConfig, ColorScheme, WindowColorMode, read_appearance, read_dominant, resolve,
};
use vow_design::Rgba;
use zbus::blocking::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

/// The only namespace this backend publishes.
pub const NAMESPACE: &str = "org.freedesktop.appearance";

const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

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
        let dominant = if appearance.window_color == WindowColorMode::Wallpaper {
            read_dominant()
        } else {
            None
        };
        Self::from_parts(appearance, dominant)
    }

    fn from_parts(appearance: &AppearanceConfig, dominant: Option<Rgba>) -> Self {
        let resolved = resolve(appearance, dominant);
        let color_scheme = match resolved.scheme {
            ColorScheme::Light => 2,
            ColorScheme::Dark | ColorScheme::DarkTint => 1,
            ColorScheme::Auto => 2,
        };
        Self {
            color_scheme,
            accent: (
                f64::from(resolved.accent.r),
                f64::from(resolved.accent.g),
                f64::from(resolved.accent.b),
            ),
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

/// Watches `appearance.domain` and `wallpaper.state` and emits `SettingChanged`
/// for each key that changes.
pub fn watch(connection: Connection, values: Arc<RwLock<Values>>) {
    std::thread::spawn(move || {
        let watch = match vow_appearance::watch::AppearanceWatch::new() {
            Ok(watch) => watch,
            Err(error) => {
                tracing::error!(%error, "cannot watch appearance files; settings will not update");
                return;
            }
        };
        let mut poll_fds = [PollFd::new(watch.as_fd(), PollFlags::POLLIN)];
        loop {
            if let Err(error) = poll(&mut poll_fds, PollTimeout::NONE) {
                tracing::warn!(%error, "appearance poll failed; retrying");
                continue;
            }
            if !poll_fds[0].any().unwrap_or(false) {
                continue;
            }
            let events = match watch.drain() {
                Ok(events) => events,
                Err(error) => {
                    tracing::warn!(%error, "cannot drain appearance watcher");
                    continue;
                }
            };
            if events.is_empty() {
                continue;
            }
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
    use vow_design::{DARK, LIGHT};

    #[test]
    fn the_color_scheme_maps_to_the_portal_values() {
        let auto = AppearanceConfig::default();
        assert_eq!(Values::from_appearance(Some(&auto)).color_scheme, 2);
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
        let tint = AppearanceConfig {
            color_scheme: ColorScheme::DarkTint,
            ..AppearanceConfig::default()
        };
        assert_eq!(Values::from_appearance(Some(&tint)).color_scheme, 1);
    }

    #[test]
    fn wallpaper_mode_publishes_the_dominant_accent() {
        let appearance = AppearanceConfig {
            window_color: WindowColorMode::Wallpaper,
            ..AppearanceConfig::default()
        };
        let wallpaper = Rgba {
            r: 0.1,
            g: 0.5,
            b: 0.9,
            a: 1.0,
        };
        let accent = Values::from_parts(&appearance, Some(wallpaper)).accent;
        // The portal accent is derived from the dominant for contrast, not the
        // raw dominant.
        assert_ne!(
            accent,
            (
                f64::from(wallpaper.r),
                f64::from(wallpaper.g),
                f64::from(wallpaper.b)
            )
        );
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
