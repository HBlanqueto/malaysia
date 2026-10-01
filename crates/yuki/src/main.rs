//! Startup and shutdown for the `yuki` compositor binary.
//!
//! The binary parses arguments, installs logging, then hands control to the
//! library's [`yuki::session`] event loop. It is a thin shell: argument
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
            eprintln!("yuki: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    match cli::parse(std::env::args().skip(1))? {
        cli::Command::Help => print!("{}", cli::USAGE),
        cli::Command::Version => println!("yuki {}", env!("CARGO_PKG_VERSION")),
        cli::Command::Run(options) => start(options)?,
    }
    Ok(())
}

fn start(options: cli::Cli) -> Result<()> {
    logging::init(options.log_level)?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "yuki starting");
    yuki::session::run()?;
    tracing::info!("yuki stopped");
    Ok(())
}
