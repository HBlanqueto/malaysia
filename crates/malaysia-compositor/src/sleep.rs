//! Locks the session before logind suspends the machine.

use std::sync::mpsc;
use std::time::Duration;

use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::channel::{self, Event};
use tracing::warn;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedFd;

use crate::idle::IdleHost;

// logind suspends anyway once InhibitDelayMaxSec (5 s by default) passes.
const LOCK_WAIT: Duration = Duration::from_secs(4);

pub fn watch<D: IdleHost + 'static>(handle: &LoopHandle<'static, D>) {
    let (to_loop, from_thread) = channel::channel::<mpsc::Sender<()>>();
    if let Err(error) = handle.insert_source(from_thread, |event, _, data: &mut D| {
        if let Event::Msg(locked) = event {
            data.idle_state().idle.lock_before_sleep(locked);
        }
    }) {
        warn!(%error, "watching logind sleep");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("malaysia-sleep".into())
        .spawn(move || {
            if let Err(error) = run(&to_loop) {
                warn!(%error, "watching logind sleep");
            }
        });
    if let Err(error) = spawned {
        warn!(%error, "starting the logind sleep watcher");
    }
}

fn run(to_loop: &channel::Sender<mpsc::Sender<()>>) -> zbus::Result<()> {
    let connection = Connection::system()?;
    let manager = Proxy::new(
        &connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )?;
    let signals = manager.receive_signal("PrepareForSleep")?;
    let inhibit = || {
        manager.call::<_, _, OwnedFd>(
            "Inhibit",
            &("sleep", "malaysia", "Lock the session before sleeping", "delay"),
        )
    };
    let mut inhibitor = Some(inhibit()?);
    for signal in signals {
        if signal.body().deserialize::<bool>()? {
            let (locked, locked_rx) = mpsc::channel();
            if to_loop.send(locked).is_err() {
                break;
            }
            if locked_rx.recv_timeout(LOCK_WAIT).is_err() {
                warn!("suspending before the session locked");
            }
            inhibitor = None;
        } else if inhibitor.is_none() {
            inhibitor = Some(inhibit()?);
        }
    }
    Ok(())
}
