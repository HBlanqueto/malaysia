//! The Yuki shell: one binary, many processes (ADR 0023).
//!
//! Each component runs as its own process and its own Wayland client, launched
//! as `yuki-shell <component>`: `wallpaper` today, `menubar`, `dock`, `lock`, ...
//! later. A component must be able to crash and restart on its own without
//! taking any other component (or the compositor) down.

mod desktop;
mod dock;
mod launcher;
mod lock;
mod menubar;
mod pam;
mod wallpaper;

use std::process::ExitCode;

const USAGE: &str = "\
yuki-shell <component>

components:
  wallpaper   the desktop picture on the Background layer
  menubar     the bar at the top (active app + clock), Top layer
  dock        the app dock at the bottom (running apps), Top layer
  lock        the session lock screen (ext-session-lock-v1)
  launcher    the application launcher on the Overlay layer
";

fn main() -> ExitCode {
    let component = std::env::args().nth(1);
    let result = match component.as_deref() {
        Some("wallpaper") => wallpaper::run(),
        Some("menubar") => menubar::run(),
        Some("dock") => dock::run(),
        Some("lock") => lock::run(),
        Some(other) => {
            eprintln!("yuki-shell: unknown component '{other}'");
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
            eprintln!("yuki-shell: {error:#}");
            ExitCode::FAILURE
        }
    }
}
