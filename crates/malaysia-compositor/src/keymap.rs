//! Reads the system keyboard layout once at startup.

use std::env;

use smithay::input::keyboard::XkbConfig;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Environment,
    Locale1,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keymap {
    pub rules: String,
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: String,
}

impl Keymap {
    pub fn xkb_config(&self) -> XkbConfig<'_> {
        XkbConfig {
            rules: &self.rules,
            model: &self.model,
            layout: &self.layout,
            variant: &self.variant,
            options: (!self.options.is_empty()).then(|| self.options.clone()),
        }
    }

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
