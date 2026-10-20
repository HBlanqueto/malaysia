//! Command-line argument parsing for the `yuki` binary.
//!
//! Turns the raw argument list into a [`Command`]: either a request to start
//! with a set of [`Cli`] options, or a request for help or the version. The
//! option set is still small, so argument parsing is hand written against
//! `std::env::args` rather than pulling in an argument parsing crate.

use crate::logging::LogLevel;

/// Text printed by `yuki --help`.
pub const USAGE: &str = "\
yuki - a Wayland compositor

Usage:
    yuki [OPTIONS]

Options:
    --winit               Nested backend in a winit window (default)
    --drm                 Direct session on the primary DRM device
    --drm-device <PATH>   DRM/KMS node for --drm (dev only; e.g. vkms)
    --no-input            Do not open libinput under --drm (dev only)
    --log-level <LEVEL>   Log verbosity: error, warn, info, debug or trace
                          [default: info]
    -h, --help            Print this help and exit
    -V, --version         Print the version and exit
";

/// What the command line asked the binary to do.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cli {
    /// Maximum log level to emit.
    pub log_level: LogLevel,
    /// Which presentation backend to start.
    pub backend: Backend,
    /// A specific DRM/KMS node for `--drm`, instead of the primary GPU.
    ///
    /// Development only: it lets a test target vkms without touching the
    /// panel's card.
    pub drm_device: Option<String>,
    /// Do not open libinput under `--drm`.
    ///
    /// Development only: it keeps a test from touching the real input
    /// devices.
    pub no_input: bool,
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
    /// A development option was used without the backend it belongs to.
    #[error("option `{option}` is only valid with `--drm`")]
    OptionRequiresDrm {
        /// The option that needs `--drm`.
        option: &'static str,
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
            "--no-input" => options.no_input = true,
            "--drm-device" => {
                let option = "--drm-device";
                let value = args.next().ok_or(ParseError::MissingValue { option })?;
                options.drm_device = Some(value);
            }
            "--log-level" => {
                let option = "--log-level";
                let value = args.next().ok_or(ParseError::MissingValue { option })?;
                options.log_level = parse_level(value)?;
            }
            _ => match arg.strip_prefix("--log-level=") {
                Some(value) => options.log_level = parse_level(value.to_owned())?,
                None => match arg.strip_prefix("--drm-device=") {
                    Some(value) => options.drm_device = Some(value.to_owned()),
                    None => return Err(unrecognised(&arg)),
                },
            },
        }
    }

    if options.backend != Backend::Drm {
        if options.drm_device.is_some() {
            return Err(ParseError::OptionRequiresDrm {
                option: "--drm-device",
            });
        }
        if options.no_input {
            return Err(ParseError::OptionRequiresDrm {
                option: "--no-input",
            });
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
                ..Cli::default()
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
                ..Cli::default()
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
                ..Cli::default()
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
                ..Cli::default()
            }))
        );
        assert_eq!(
            parse(args(&["--drm"])),
            Ok(Command::Run(Cli {
                log_level: LogLevel::Info,
                backend: Backend::Drm,
                ..Cli::default()
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

    #[test]
    fn drm_devices_accept_separate_and_inline_values() {
        assert_eq!(
            parse(args(&[
                "--drm",
                "--drm-device",
                "/dev/dri/card2",
                "--no-input"
            ])),
            Ok(Command::Run(Cli {
                backend: Backend::Drm,
                drm_device: Some("/dev/dri/card2".to_owned()),
                no_input: true,
                ..Cli::default()
            }))
        );
        assert_eq!(
            parse(args(&["--drm", "--drm-device=/dev/dri/card2"])),
            Ok(Command::Run(Cli {
                backend: Backend::Drm,
                drm_device: Some("/dev/dri/card2".to_owned()),
                ..Cli::default()
            }))
        );
    }

    #[test]
    fn development_options_require_drm() {
        assert_eq!(
            parse(args(&["--drm-device", "/dev/dri/card2"])),
            Err(ParseError::OptionRequiresDrm {
                option: "--drm-device"
            })
        );
        assert_eq!(
            parse(args(&["--no-input"])),
            Err(ParseError::OptionRequiresDrm {
                option: "--no-input"
            })
        );
    }
}
