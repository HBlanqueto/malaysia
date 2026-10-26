//! Startup and shutdown for the `malaysia-compositor` binary.
//!
//! The binary parses arguments, installs logging, then hands control to the
//! library's [`malaysia_compositor::session`] event loop. It is a thin shell: argument
//! parsing and logging live here, the compositor lives in the library.

#![deny(unsafe_code)]

mod cli;
mod logging;

use std::process::ExitCode;

use anyhow::Result;
use nix::sys::signal::{SigSet, SigmaskHow, Signal, pthread_sigmask};

/// Blocks `SIGUSR1`/`SIGTERM`/`SIGINT` before any thread is spawned.
///
/// `calloop::signals` blocks them on the calling thread only; a signal is
/// delivered to any thread that has it unblocked, so renderer threads created
/// before that point could take `SIGTERM` and kill the process without the
/// graceful shutdown (and lose the `SIGUSR1` capture). Blocking them here, as
/// the first thing `main` does, makes every later thread inherit the mask.
fn block_shutdown_signals() {
    let mut set = SigSet::empty();
    for signal in [Signal::SIGUSR1, Signal::SIGTERM, Signal::SIGINT] {
        set.add(signal);
    }
    if let Err(error) = pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&set), None) {
        eprintln!("malaysia-compositor: could not block the shutdown signals: {error}");
    }
}

fn main() -> ExitCode {
    block_shutdown_signals();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("malaysia-compositor: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    match cli::parse(std::env::args().skip(1))? {
        cli::Command::Help => print!("{}", cli::USAGE),
        cli::Command::Version => println!("malaysia-compositor {}", env!("CARGO_PKG_VERSION")),
        cli::Command::Run(options) => start(options)?,
    }
    Ok(())
}

fn start(options: cli::Cli) -> Result<()> {
    logging::init(options.log_level)?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        backend = ?options.backend,
        "malaysia-compositor starting"
    );
    match options.backend {
        cli::Backend::Winit => malaysia_compositor::session::run()?,
        cli::Backend::Drm => {
            malaysia_compositor::udev::run(malaysia_compositor::udev::DrmOptions {
                device: options.drm_device.map(std::path::PathBuf::from),
                no_input: options.no_input,
            })?
        }
    }
    tracing::info!("malaysia-compositor stopped");
    Ok(())
}
