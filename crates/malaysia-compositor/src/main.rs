#![deny(unsafe_code)]

mod cli;
mod logging;

use std::process::ExitCode;

use anyhow::Result;
use nix::sys::signal::{SigSet, SigmaskHow, Signal, pthread_sigmask};

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
