//! Startup and shutdown for the `neptune` compositor binary.
//!
//! The binary parses arguments, installs logging, then hands control to the
//! library's [`neptune::session`] event loop. It is a thin shell: argument
//! parsing and logging live here, the compositor lives in the library.

#![deny(unsafe_code)]

mod cli;
mod logging;

use std::process::ExitCode;

use anyhow::Result;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("neptune: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    match cli::parse(std::env::args().skip(1))? {
        cli::Command::Help => print!("{}", cli::USAGE),
        cli::Command::Version => println!("neptune {}", env!("CARGO_PKG_VERSION")),
        cli::Command::Run(options) => start(options)?,
    }
    Ok(())
}

fn start(options: cli::Cli) -> Result<()> {
    logging::init(options.log_level)?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "neptune starting");
    neptune::session::run()?;
    tracing::info!("neptune stopped");
    Ok(())
}
