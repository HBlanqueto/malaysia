use std::collections::HashMap;
use std::os::fd::AsFd;
use std::sync::{Arc, RwLock};

use ctappearance::{AppearanceConfig, Toolkit, load_dominant, read_appearance, resolve};
use ctdesign::Rgba;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use zbus::blocking::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

use crate::gtk::{self, Bridge};

pub const NAMESPACE: &str = "org.freedesktop.appearance";
pub const GNOME_NAMESPACE: &str = "org.gnome.desktop.interface";
pub const LIVE_NAMESPACE: &str = "org.malaysia.appearance";
pub const WM_NAMESPACE: &str = "org.gnome.desktop.wm.preferences";

const KEYS: [(&str, &str); 9] = [
    (NAMESPACE, "color-scheme"),
    (NAMESPACE, "accent-color"),
    (NAMESPACE, "contrast"),
    (NAMESPACE, "reduced-motion"),
    (GNOME_NAMESPACE, "color-scheme"),
    (GNOME_NAMESPACE, "text-scaling-factor"),
    (LIVE_NAMESPACE, "gtk4-css"),
    (LIVE_NAMESPACE, "accent-color"),
    (WM_NAMESPACE, "button-layout"),
];

const UNSET_ACCENT: (f64, f64, f64) = (-1.0, -1.0, -1.0);

pub(crate) const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

#[derive(Debug, Clone, PartialEq)]
pub struct Values {
    color_scheme: u32,
    accent: (f64, f64, f64),
    contrast: u32,
    reduced_motion: u32,
    text_scaling_factor: f64,
    live_css: String,
    live_accent: (f64, f64, f64),
}

impl Values {
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
        let dominant = load_dominant();
        Self::from_parts(appearance, dominant, appearance.gtk_bridge)
    }

    fn from_parts(appearance: &AppearanceConfig, dominant: Option<Rgba>, bridge: bool) -> Self {
        let resolved = resolve(appearance, dominant);
        let color_scheme = if resolved.scheme.is_dark() { 1 } else { 2 };
        let accent = (
            f64::from(resolved.accent.r),
            f64::from(resolved.accent.g),
            f64::from(resolved.accent.b),
        );
        let (live_css, live_accent) = if bridge {
            (gtk::live_css(appearance, dominant), accent)
        } else {
            (String::new(), UNSET_ACCENT)
        };
        Self {
            color_scheme,
            // An automatic accent is no preference, so an app keeps its own (freedesktop appearance).
            accent: if resolved.automatic_accent {
                UNSET_ACCENT
            } else {
                accent
            },
            contrast: u32::from(appearance.contrast),
            reduced_motion: u32::from(appearance.reduced_motion),
            text_scaling_factor: f64::from(appearance.text_size_for(Toolkit::Gtk)),
            live_css,
            live_accent,
        }
    }

    fn value(&self, namespace: &str, key: &str) -> Option<OwnedValue> {
        let value = match (namespace, key) {
            (NAMESPACE, "color-scheme") => Value::new(self.color_scheme),
            (NAMESPACE, "accent-color") => Value::new(self.accent),
            (NAMESPACE, "contrast") => Value::new(self.contrast),
            (NAMESPACE, "reduced-motion") => Value::new(self.reduced_motion),
            (GNOME_NAMESPACE, "color-scheme") => Value::new(self.gnome_color_scheme()),
            (GNOME_NAMESPACE, "text-scaling-factor") => Value::new(self.text_scaling_factor),
            (LIVE_NAMESPACE, "gtk4-css") => Value::new(self.live_css.clone()),
            (LIVE_NAMESPACE, "accent-color") => Value::new(self.live_accent),
            (WM_NAMESPACE, "button-layout") => Value::new(gtk::button_layout()),
            _ => return None,
        };
        Some(owned(value))
    }

    fn gnome_color_scheme(&self) -> &'static str {
        if self.color_scheme == 1 {
            "prefer-dark"
        } else {
            "default"
        }
    }

    fn all(&self, namespace: &str) -> HashMap<String, OwnedValue> {
        KEYS.into_iter()
            .filter(|(ns, _)| *ns == namespace)
            .filter_map(|(ns, key)| self.value(ns, key).map(|value| (key.to_string(), value)))
            .collect()
    }

    fn changed(&self, old: &Self) -> Vec<(&'static str, &'static str, OwnedValue)> {
        KEYS.into_iter()
            .filter_map(|(namespace, key)| {
                let new = self.value(namespace, key)?;
                (Some(&new) != old.value(namespace, key).as_ref()).then_some((namespace, key, new))
            })
            .collect()
    }
}

pub struct Settings {
    values: Arc<RwLock<Values>>,
}

impl Settings {
    pub fn new(values: Arc<RwLock<Values>>) -> Self {
        Self { values }
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Settings")]
impl Settings {
    fn read(&self, namespace: &str, key: &str) -> Result<OwnedValue, PortalError> {
        if !KEYS.iter().any(|(ns, _)| *ns == namespace) {
            return Err(PortalError::NotFound(format!(
                "unknown settings namespace {namespace}"
            )));
        }
        self.values
            .read()
            .expect("the settings lock is not poisoned")
            .value(namespace, key)
            .ok_or_else(|| PortalError::NotFound(format!("unknown settings key {key}")))
    }

    fn read_all(&self, namespaces: Vec<String>) -> HashMap<String, HashMap<String, OwnedValue>> {
        let values = self
            .values
            .read()
            .expect("the settings lock is not poisoned");
        let mut known: Vec<&str> = KEYS.iter().map(|(namespace, _)| *namespace).collect();
        known.dedup();
        known
            .into_iter()
            .filter(|namespace| {
                namespaces.is_empty()
                    || namespaces
                        .iter()
                        .any(|pattern| namespace_matches(pattern, namespace))
            })
            .map(|namespace| (namespace.to_string(), values.all(namespace)))
            .collect()
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

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.portal.Error", impl_display = true)]
enum PortalError {
    NotFound(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

pub fn watch(
    connection: Connection,
    values: Arc<RwLock<Values>>,
    bridge: Option<Bridge>,
    qt: Option<crate::qt::Bridge>,
    folders: Option<crate::folders::Folders>,
) {
    std::thread::spawn(move || {
        let watch = match ctappearance::watch::AppearanceWatch::new() {
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
            if !events.iter().any(|event| event.restyles()) {
                continue;
            }
            let appearance = match read_appearance() {
                Ok(appearance) => appearance,
                Err(error) => {
                    tracing::warn!(%error, "appearance.domain unreadable; keeping the last values");
                    continue;
                }
            };
            if let Some(bridge) = &bridge {
                bridge.sync(appearance.as_ref(), load_dominant());
            }
            if let Some(qt) = &qt {
                qt.sync(appearance.as_ref(), load_dominant());
            }
            if let Some(folders) = &folders {
                folders.sync(appearance.as_ref());
            }
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
            for (namespace, key, value) in new.changed(&old) {
                if let Err(error) = zbus::block_on(Settings::setting_changed(
                    interface.signal_emitter(),
                    namespace,
                    key,
                    value,
                )) {
                    tracing::warn!(%error, namespace, key, "could not emit SettingChanged");
                }
            }
        }
    });
}

fn namespace_matches(pattern: &str, namespace: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => namespace.starts_with(prefix),
        None => pattern == namespace,
    }
}

fn owned(value: Value<'static>) -> OwnedValue {
    OwnedValue::try_from(value).expect("a zvariant value is always ownable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctappearance::{ColorScheme, WindowColorMode};
    use ctdesign::{DARK, LIGHT};

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
    fn the_accent_falls_back_to_the_scheme_palette() {
        let light = AppearanceConfig {
            color_scheme: ColorScheme::Light,
            ..AppearanceConfig::default()
        };
        let light = Values::from_appearance(Some(&light)).live_accent;
        assert_eq!(light.0, f64::from(LIGHT.accent.r));
        let dark = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            ..AppearanceConfig::default()
        };
        let dark = Values::from_appearance(Some(&dark)).live_accent;
        assert_eq!(dark.2, f64::from(DARK.accent.b));
    }

    #[test]
    fn an_explicit_accent_is_clamped() {
        let appearance = AppearanceConfig {
            accent_color: Some([-1.0, 0.25, 2.0]),
            ..AppearanceConfig::default()
        };
        assert_eq!(
            Values::from_appearance(Some(&appearance)).accent,
            (0.0, 0.25, 1.0)
        );
    }

    #[test]
    fn namespaces_use_trailing_globs() {
        assert!(namespace_matches("", NAMESPACE));
        assert!(namespace_matches(NAMESPACE, NAMESPACE));
        assert!(namespace_matches("org.freedesktop.*", NAMESPACE));
        assert!(!namespace_matches("org.gnome.*", NAMESPACE));
        assert!(namespace_matches("org.gnome.*", GNOME_NAMESPACE));
        assert!(!namespace_matches(
            "org.freedesktop.appearance.extra",
            NAMESPACE
        ));
    }

    #[test]
    fn only_changed_keys_are_reported() {
        let old = Values::from_appearance(None);
        let mut new = old.clone();
        new.contrast = 1;
        let changed: Vec<_> = new
            .changed(&old)
            .into_iter()
            .map(|(namespace, key, _)| (namespace, key))
            .collect();
        assert_eq!(changed, [(NAMESPACE, "contrast")]);
    }

    #[test]
    fn the_gnome_namespace_mirrors_the_color_scheme() {
        let dark = AppearanceConfig {
            color_scheme: ColorScheme::DarkTint,
            ..AppearanceConfig::default()
        };
        let dark = Values::from_appearance(Some(&dark));
        assert_eq!(dark.gnome_color_scheme(), "prefer-dark");
        assert_eq!(
            Values::from_appearance(None).gnome_color_scheme(),
            "default"
        );

        let mut light = dark.clone();
        light.color_scheme = 2;
        let changed: Vec<_> = light
            .changed(&dark)
            .into_iter()
            .map(|(namespace, key, _)| (namespace, key))
            .collect();
        assert_eq!(
            changed,
            [
                (NAMESPACE, "color-scheme"),
                (GNOME_NAMESPACE, "color-scheme")
            ]
        );
        assert!(dark.all(GNOME_NAMESPACE).contains_key("color-scheme"));
        assert_eq!(dark.all(NAMESPACE).len(), 4);
    }

    #[test]
    fn the_live_namespace_follows_the_bridge() {
        let appearance = AppearanceConfig {
            window_color: WindowColorMode::Accent,
            accent_color: Some([0.125, 0.376, 0.816]),
            ..AppearanceConfig::default()
        };
        let on = Values::from_parts(&appearance, None, true);
        assert_eq!(on.live_css, gtk::live_css(&appearance, None));
        assert!(on.live_css.contains("window_bg_color"));
        assert_eq!(on.live_accent, on.accent);
        assert_eq!(on.all(LIVE_NAMESPACE).len(), 2);

        let off = Values::from_parts(&appearance, None, false);
        assert!(off.live_css.is_empty());
        assert_eq!(off.live_accent, UNSET_ACCENT);
        assert_eq!(off.accent, on.accent);

        let changed: Vec<_> = off
            .changed(&on)
            .into_iter()
            .map(|(namespace, key, _)| (namespace, key))
            .collect();
        assert_eq!(
            changed,
            [
                (LIVE_NAMESPACE, "gtk4-css"),
                (LIVE_NAMESPACE, "accent-color")
            ]
        );
    }

    #[test]
    fn only_a_chosen_accent_reaches_the_portal() {
        let automatic = Values::from_parts(&AppearanceConfig::default(), None, true);
        assert_eq!(automatic.accent, UNSET_ACCENT);
        assert_ne!(automatic.live_accent, UNSET_ACCENT);
        let chosen = Values::from_parts(
            &AppearanceConfig {
                accent_color: Some([0.9, 0.3, 0.1]),
                ..AppearanceConfig::default()
            },
            None,
            true,
        );
        assert_ne!(chosen.accent, UNSET_ACCENT);
    }

    #[test]
    fn a_new_tint_changes_only_the_live_stylesheet() {
        let neutral = Values::from_parts(&AppearanceConfig::default(), None, true);
        let tinted = Values::from_parts(
            &AppearanceConfig {
                window_color: WindowColorMode::Accent,
                ..AppearanceConfig::default()
            },
            None,
            true,
        );
        let changed: Vec<_> = tinted
            .changed(&neutral)
            .into_iter()
            .map(|(namespace, key, _)| (namespace, key))
            .collect();
        assert_eq!(changed, [(LIVE_NAMESPACE, "gtk4-css")]);
    }
}
