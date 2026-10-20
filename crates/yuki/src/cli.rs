//! Command-line argument parsing for the `yuki` binary.
//!
//! Turns the raw argument list into a [`Command`]: either a request to start
//! with a set of [`Cli`] options, or a request for help or the version. Block
//! 0 accepts a single option, `--log-level`, so argument parsing is hand
//! written against `std::env::args` rather than pulling in an argument
//! parsing crate.

use crate::logging::LogLevel;

/// Text printed by `yuki --help`.
pub const USAGE: &str = "\
yuki - a Wayland compositor

Usage:
    yuki [OPTIONS]

Options:
    --winit               Nested backend in a winit window (default)
    --drm                 Direct session on the primary DRM device
    --log-level <LEVEL>   Log verbosity: error, warn, info, debug or trace
                          [default: info]
    -h, --help            Print this help and exit
    -V, --version         Print the version and exit
";

/// What the command line asked the binary to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Start the compositor with these options.
    Run(Cli),
    /// Print usage and exit.
    Help,
    /// Print the version and exit.
    Version,
}

/// Which presentation backend to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    /// Nested in a winit window. The development default: it needs no TTY,
    /// no seat and no DRM master, so `dev.sh` and the probes keep working.
    #[default]
    Winit,
    /// Direct session on the primary DRM device, with libseat and libinput.
    Drm,
}

/// Options accepted when starting the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cli {
    /// Maximum log level to emit.
    pub log_level: LogLevel,
    /// Which presentation backend to start.
    pub backend: Backend,
}

/// Errors raised while parsing the command line.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    /// An option was not recognised.
    #[error("unknown option `{0}`")]
    UnknownOption(String),
    /// A bare argument was given, and none are accepted.
    #[error("unexpected argument `{0}`")]
    UnexpectedArgument(String),
    /// An option that requires a value was given without one.
    #[error("option `{option}` requires a value")]
    MissingValue {
        /// The option that was missing its value.
        option: &'static str,
    },
    /// The log level was not one of the accepted names.
    #[error("invalid log level `{value}`; expected one of error, warn, info, debug, trace")]
    InvalidLogLevel {
        /// The rejected value.
        value: String,
    },
}

/// Parses arguments, excluding the program name.
pub fn parse<I>(args: I) -> Result<Command, ParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut options = Cli::default();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--winit" => options.backend = Backend::Winit,
            "--drm" => options.backend = Backend::Drm,
            "--log-level" => {
                let option = "--log-level";
                let value = args.next().ok_or(ParseError::MissingValue { option })?;
                options.log_level = parse_level(value)?;
            }
            _ => match arg.strip_prefix("--log-level=") {
                Some(value) => options.log_level = parse_level(value.to_owned())?,
                None => return Err(unrecognised(&arg)),
            },
        }
    }

    Ok(Command::Run(options))
}

/// Classifies an unhandled argument as an option or a bare argument.
fn unrecognised(arg: &str) -> ParseError {
    if arg.starts_with('-') {
        ParseError::UnknownOption(arg.to_owned())
    } else {
        ParseError::UnexpectedArgument(arg.to_owned())
    }
}

/// Parses a log level name or reports the value that was rejected.
fn parse_level(value: String) -> Result<LogLevel, ParseError> {
    LogLevel::from_name(&value).ok_or(ParseError::InvalidLogLevel { value })
}

#[cfg(test)]
mod tests {
    use super::{Backend, Cli, Command, ParseError, parse};
    use crate::logging::LogLevel;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn no_arguments_runs_with_defaults() {
        assert_eq!(
            parse(args(&[])),
            Ok(Command::Run(Cli {
                log_level: LogLevel::Info,
                backend: Backend::Winit,
            }))
        );
    }

    #[test]
    fn log_level_accepts_a_separate_value() {
        assert_eq!(
            parse(args(&["--log-level", "debug"])),
            Ok(Command::Run(Cli {
                log_level: LogLevel::Debug,
                backend: Backend::Winit,
            }))
        );
    }

    #[test]
    fn log_level_accepts_an_inline_value() {
        assert_eq!(
            parse(args(&["--log-level=warn"])),
            Ok(Command::Run(Cli {
                log_level: LogLevel::Warn,
                backend: Backend::Winit,
            }))
        );
    }

    #[test]
    fn explicit_backends_are_parsed_and_winit_is_the_default() {
        assert_eq!(
            parse(args(&["--winit"])),
            Ok(Command::Run(Cli {
                log_level: LogLevel::Info,
                backend: Backend::Winit,
            }))
        );
        assert_eq!(
            parse(args(&["--drm"])),
            Ok(Command::Run(Cli {
                log_level: LogLevel::Info,
                backend: Backend::Drm,
            }))
        );
    }

    #[test]
    fn help_and_version_take_priority_when_first() {
        assert_eq!(parse(args(&["--help"])), Ok(Command::Help));
        assert_eq!(parse(args(&["-h"])), Ok(Command::Help));
        assert_eq!(parse(args(&["--version"])), Ok(Command::Version));
        assert_eq!(parse(args(&["-V"])), Ok(Command::Version));
    }

    #[test]
    fn a_missing_value_is_reported() {
        assert_eq!(
            parse(args(&["--log-level"])),
            Err(ParseError::MissingValue {
                option: "--log-level"
            })
        );
    }

    #[test]
    fn an_invalid_level_is_reported() {
        assert_eq!(
            parse(args(&["--log-level=verbose"])),
            Err(ParseError::InvalidLogLevel {
                value: "verbose".to_owned()
            })
        );
    }

    #[test]
    fn an_unknown_option_is_reported() {
        assert_eq!(
            parse(args(&["--colour"])),
            Err(ParseError::UnknownOption("--colour".to_owned()))
        );
    }

    #[test]
    fn an_unexpected_argument_is_reported() {
        assert_eq!(
            parse(args(&["start"])),
            Err(ParseError::UnexpectedArgument("start".to_owned()))
        );
    }
}
