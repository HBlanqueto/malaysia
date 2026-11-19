use std::sync::mpsc::{Sender, channel};

use crate::spawn::command;

const SCHEMA: &str = "org.gnome.desktop.wm.preferences";
const KEY: &str = "button-layout";
const NO_BUTTONS: &str = ":";

pub(crate) struct WindowButtons {
    hidden: bool,
    writes: Sender<bool>,
}

impl WindowButtons {
    pub(crate) fn new() -> Self {
        let (writes, requests) = channel();
        std::thread::spawn(move || {
            if cfg!(test) {
                return;
            }
            if layout().as_deref() == Some(NO_BUTTONS) {
                write(false);
            }
            requests.into_iter().for_each(write);
        });
        Self {
            hidden: false,
            writes,
        }
    }

    pub(crate) fn set_hidden(&mut self, hidden: bool) {
        if self.hidden != hidden {
            self.hidden = hidden;
            let _ = self.writes.send(hidden);
        }
    }
}

fn layout() -> Option<String> {
    let output = command("gsettings")
        .args(["get", SCHEMA, KEY])
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .trim_matches('\'')
            .to_string()
    })
}

fn write(hidden: bool) {
    let mut gsettings = command("gsettings");
    if hidden {
        gsettings.args(["set", SCHEMA, KEY, NO_BUTTONS]);
    } else {
        gsettings.args(["reset", SCHEMA, KEY]);
    }
    if let Err(error) = gsettings.status() {
        tracing::warn!(%error, hidden, "could not change the window button layout");
    }
}
