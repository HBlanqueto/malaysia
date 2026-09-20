//! Startup and shutdown for the `neptune` compositor binary.
//!
//! The binary does four things: parse arguments, install logging, log one
//! startup line, and wait for a termination signal before exiting. There is no
//! event loop, protocol handling or rendering yet; `wait_for_shutdown` is the
//! whole process lifetime in Block 0.

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
    wait_for_shutdown()?;
    tracing::info!("neptune stopped");
    Ok(())
}

/// Blocks until a termination signal arrives.
///
/// The signal iterator blocks without polling, so the process sleeps until the
/// kernel delivers `SIGTERM` or `SIGINT`.
fn wait_for_shutdown() -> Result<()> {
    use signal_hook::consts::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    let mut signals = Signals::new([SIGTERM, SIGINT])?;
    if let Some(signal) = signals.forever().next() {
        tracing::info!(signal, "shutdown signal received");
    }
    Ok(())
}
