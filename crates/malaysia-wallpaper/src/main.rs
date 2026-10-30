//! `malaysia-wallpaper` — the MalaysiaDreams desktop wallpaper.
//!
//! A standalone Wayland client that draws the desktop picture with wgpu on
//! Vulkan, outside of BarrierKit. See ADR 0049.

mod image;
mod render;
mod wayland;

use std::io::IsTerminal;
use std::process::ExitCode;

fn main() -> ExitCode {
    init_logging();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let (mut app, event_queue) = wayland::App::new()?;
    app.run(event_queue)
}

/// Installs the wallpaper's tracing subscriber: `RUST_LOG` when set, `info`
/// otherwise, always on stderr.
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .try_init();
}
