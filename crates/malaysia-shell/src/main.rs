//! The Malaysia shell: one binary, many processes (ADR 0023).
//!
//! Each component runs as its own process and its own Wayland client, launched
//! as `malaysia-shell <component>`: `menubar`, `dock`, `lock` and `launcher`.
//! A component must be able to crash and restart on its own without taking any
//! other component (or the compositor) down.

mod desktop;
mod dock;
mod launcher;
mod lock;
mod menubar;
mod pam;

use std::io::IsTerminal;
use std::process::ExitCode;

const USAGE: &str = "\
malaysia-shell <component>

components:
  menubar     the bar at the top (active app + clock), Top layer
  dock        the app dock at the bottom (running apps), Top layer
  lock        the session lock screen (ext-session-lock-v1)
  launcher    the application launcher on the Overlay layer
";

fn main() -> ExitCode {
    init_logging();
    let component = std::env::args().nth(1);
    let result = match component.as_deref() {
        Some("menubar") => menubar::run(),
        Some("dock") => dock::run(),
        Some("lock") => lock::run(),
        Some("launcher") => launcher::run(),
        Some(other) => {
            eprintln!("malaysia-shell: unknown component '{other}'");
            eprintln!("{USAGE}");
            Ok(())
        }
        None => {
            eprintln!("{USAGE}");
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Installs the shell's tracing subscriber: `RUST_LOG` when set, `info`
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
