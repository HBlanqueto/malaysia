//! Logging initialisation for the `yuki` binary.
//!
//! Owns the mapping from a user-facing [`LogLevel`] to a `tracing` filter and
//! the one-time installation of the global subscriber. Nothing in this module
//! knows about arguments; it receives an already-parsed level.

use std::io::IsTerminal;

use tracing::level_filters::LevelFilter;

/// Maximum level of detail the compositor emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    /// Only errors.
    Error,
    /// Warnings and errors.
    Warn,
    /// Normal operational messages.
    #[default]
    Info,
    /// Developer-facing detail.
    Debug,
    /// Everything, including per-frame tracing.
    Trace,
}

impl LogLevel {
    /// Parses a lower-case level name, returning `None` if it is not one.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "error" => Some(Self::Error),
            "warn" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    /// Returns the equivalent `tracing` level filter.
    pub fn to_filter(self) -> LevelFilter {
        match self {
            Self::Error => LevelFilter::ERROR,
            Self::Warn => LevelFilter::WARN,
            Self::Info => LevelFilter::INFO,
            Self::Debug => LevelFilter::DEBUG,
            Self::Trace => LevelFilter::TRACE,
        }
    }
}

/// Errors raised while installing the global subscriber.
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    /// A global subscriber was already installed in this process.
    #[error("a global tracing subscriber is already installed")]
    AlreadyInstalled(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Installs the global `tracing` subscriber at `level`.
///
/// May be called only once per process; a second call fails with
/// [`InitError::AlreadyInstalled`].
pub fn init(level: LogLevel) -> Result<(), InitError> {
    tracing_subscriber::fmt()
        .with_max_level(level.to_filter())
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .try_init()
        .map_err(InitError::AlreadyInstalled)
}

#[cfg(test)]
mod tests {
    use super::LogLevel;
    use tracing::level_filters::LevelFilter;

    #[test]
    fn every_level_name_round_trips() {
        for name in ["error", "warn", "info", "debug", "trace"] {
            assert!(LogLevel::from_name(name).is_some(), "missing {name}");
        }
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert_eq!(LogLevel::from_name("verbose"), None);
        assert_eq!(LogLevel::from_name("INFO"), None);
    }

    #[test]
    fn info_is_the_default() {
        assert_eq!(LogLevel::default(), LogLevel::Info);
    }

    #[test]
    fn filters_match_their_levels() {
        assert_eq!(LogLevel::Error.to_filter(), LevelFilter::ERROR);
        assert_eq!(LogLevel::Trace.to_filter(), LevelFilter::TRACE);
    }
}
