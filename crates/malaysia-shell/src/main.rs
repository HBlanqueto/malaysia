mod app_scope;
mod desktop;
mod dialogs;
mod dock;
mod launcher;
mod localize;
mod lock;
mod menubar;
mod notifications;
mod pam;
mod power;
mod spawn;

use std::io::IsTerminal;
use std::process::ExitCode;

const USAGE: &str = "\
malaysia-shell <component>

components:
  menubar     the bar at the top (active app + clock), Top layer
  dock        the app dock at the bottom (running apps), Top layer
  lock        the session lock screen (ext-session-lock-v1)
  launcher    the application launcher on the Overlay layer
  notifications
              the notification server, banners and Notification Center
  power <restart|shutdown|logout>
              confirm a power action (MALAYSIA_POWER_DRY_RUN=1 only logs it)
  force-quit  the «Forzar salida» list of running apps
  about       «Acerca de este PC»
  about-app <app_id>
              «Acerca de» for an app that has none of its own
";

fn main() -> ExitCode {
    init_logging();
    let component = std::env::args().nth(1);
    let result = match component.as_deref() {
        Some("menubar") => menubar::run(),
        Some("dock") => dock::run(),
        Some("lock") => lock::run(),
        Some("launcher") => launcher::run(),
        Some("notifications") => notifications::run(),
        Some("power") => dialogs::run_confirm(std::env::args().nth(2).as_deref()),
        Some("force-quit") => dialogs::run_force_quit(),
        Some("about") => dialogs::run_about(),
        Some("about-app") => dialogs::run_about_app(std::env::args().nth(2).as_deref()),
        Some(other) => {
            eprintln!("malaysia-shell: unknown component '{other}'");
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
        None => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
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
