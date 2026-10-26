//! The system keyboard layout, read once at startup.
//!
//! Priority:
//! 1. `XKB_DEFAULT_*` in the environment, an explicit override;
//! 2. `org.freedesktop.locale1` on the system bus (`localectl status`);
//! 3. xkbcommon's defaults.
//!
//! The query is blocking and runs before the event loop starts. A failure is
//! logged and never prevents the compositor from starting.

use std::env;

use smithay::input::keyboard::XkbConfig;
use tracing::{info, warn};

/// Where the keymap values came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `XKB_DEFAULT_*` environment variables.
    Environment,
    /// `org.freedesktop.locale1`.
    Locale1,
    /// xkbcommon's built-in defaults.
    Default,
}

impl Source {
    fn name(self) -> &'static str {
        match self {
            Self::Environment => "env",
            Self::Locale1 => "locale1",
            Self::Default => "default",
        }
    }
}

/// The five xkbcommon rule names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keymap {
    /// Rules file; empty means xkbcommon's default.
    pub rules: String,
    /// Keyboard model.
    pub model: String,
    /// Layout list.
    pub layout: String,
    /// Variant list.
    pub variant: String,
    /// xkb options.
    pub options: String,
}

impl Keymap {
    /// Builds an `XkbConfig` borrowing these values. Empty options stay `None`.
    pub fn xkb_config(&self) -> XkbConfig<'_> {
        XkbConfig {
            rules: &self.rules,
            model: &self.model,
            layout: &self.layout,
            variant: &self.variant,
            options: (!self.options.is_empty()).then(|| self.options.clone()),
        }
    }

    /// Reads the explicit `XKB_DEFAULT_*` override, if any variable is set.
    fn from_environment() -> Option<Self> {
        let names = [
            "XKB_DEFAULT_RULES",
            "XKB_DEFAULT_MODEL",
            "XKB_DEFAULT_LAYOUT",
            "XKB_DEFAULT_VARIANT",
            "XKB_DEFAULT_OPTIONS",
        ];
        if names.iter().all(|name| env::var_os(name).is_none()) {
            return None;
        }
        let value = |name: &str| env::var(name).unwrap_or_default();
        Some(Self {
            rules: value("XKB_DEFAULT_RULES"),
            model: value("XKB_DEFAULT_MODEL"),
            layout: value("XKB_DEFAULT_LAYOUT"),
            variant: value("XKB_DEFAULT_VARIANT"),
            options: value("XKB_DEFAULT_OPTIONS"),
        })
    }

    /// Queries `org.freedesktop.locale1` on the system bus.
    fn from_locale1() -> Result<Self, String> {
        use zbus::blocking::{Connection, Proxy};

        let connection = Connection::system().map_err(|error| error.to_string())?;
        let proxy = Proxy::new(
            &connection,
            "org.freedesktop.locale1",
            "/org/freedesktop/locale1",
            "org.freedesktop.locale1",
        )
        .map_err(|error| error.to_string())?;
        let property = |name: &str| -> Result<String, String> {
            proxy
                .get_property::<String>(name)
                .map_err(|error| error.to_string())
        };
        Ok(Self {
            rules: String::new(),
            model: property("X11Model")?,
            layout: property("X11Layout")?,
            variant: property("X11Variant")?,
            options: property("X11Options")?,
        })
    }
}

/// Chooses the keymap from the two sources, without touching D-Bus.
///
/// This is the testable half: the caller supplies the environment override
/// and the locale1 result.
fn choose(
    environment: Option<Keymap>,
    locale1: impl FnOnce() -> Result<Keymap, String>,
) -> (Keymap, Source) {
    if let Some(keymap) = environment {
        return (keymap, Source::Environment);
    }
    match locale1() {
        Ok(keymap) if !keymap.layout.is_empty() => (keymap, Source::Locale1),
        Ok(_) => (Keymap::default(), Source::Default),
        Err(error) => {
            warn!(%error, "could not read the system keymap from locale1");
            (Keymap::default(), Source::Default)
        }
    }
}

/// Loads the keymap once at startup and logs the chosen source and layout.
pub fn system() -> Keymap {
    let (keymap, source) = choose(Keymap::from_environment(), Keymap::from_locale1);
    info!(
        source = source.name(),
        layout = keymap.layout,
        "keymap selected"
    );
    keymap
}

#[cfg(test)]
mod tests {
    use super::{Keymap, Source, choose};

    fn keymap(layout: &str) -> Keymap {
        Keymap {
            layout: layout.to_string(),
            ..Keymap::default()
        }
    }

    #[test]
    fn the_environment_override_wins_over_locale1() {
        let (keymap, source) = choose(Some(keymap("latam")), || Ok(keymap("es")));
        assert_eq!(source, Source::Environment);
        assert_eq!(keymap.layout, "latam");
    }

    #[test]
    fn locale1_is_used_without_an_environment_override() {
        let (keymap, source) = choose(None, || Ok(keymap("es")));
        assert_eq!(source, Source::Locale1);
        assert_eq!(keymap.layout, "es");
    }

    #[test]
    fn an_empty_locale1_layout_falls_back_to_xkbcommon() {
        let (keymap, source) = choose(None, || Ok(keymap("")));
        assert_eq!(source, Source::Default);
        assert!(keymap.layout.is_empty());
    }

    #[test]
    fn a_failed_locale1_query_falls_back_to_xkbcommon() {
        let (keymap, source) = choose(None, || Err("no bus".to_string()));
        assert_eq!(source, Source::Default);
        assert!(keymap.layout.is_empty());
    }
}
