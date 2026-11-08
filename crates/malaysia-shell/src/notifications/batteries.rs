use std::time::Duration;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

use super::server::Link;

const UPOWER: &str = "org.freedesktop.UPower";
const DEVICE: &str = "org.freedesktop.UPower.Device";
const POLL: Duration = Duration::from_secs(60);
// UPower's UpDeviceKind: 1 is line power, 2 the computer's own battery.
const LINE_POWER: u32 = 1;
pub(crate) const COMPUTER: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Battery {
    pub(crate) kind: u32,
    pub(crate) model: String,
    pub(crate) percent: f64,
}

fn read(connection: &Connection) -> zbus::Result<Vec<Battery>> {
    let upower = Proxy::new(connection, UPOWER, "/org/freedesktop/UPower", UPOWER)?;
    let devices: Vec<OwnedObjectPath> = upower.call("EnumerateDevices", &())?;
    let mut batteries = Vec::new();
    for path in devices {
        let device = Proxy::new(connection, UPOWER, path, DEVICE)?;
        let kind: u32 = device.get_property("Type")?;
        let present: bool = device.get_property("IsPresent")?;
        let percent: f64 = device.get_property("Percentage")?;
        if kind == LINE_POWER || !present || percent <= 0.0 {
            continue;
        }
        batteries.push(Battery {
            kind,
            model: device.get_property("Model").unwrap_or_default(),
            percent,
        });
    }
    batteries.sort_by_key(|battery| battery.kind != COMPUTER);
    Ok(batteries)
}

pub(crate) fn start(link: Link) {
    std::thread::Builder::new()
        .name("malaysia-batteries".to_string())
        .spawn(move || {
            let Ok(connection) = Connection::system() else {
                return;
            };
            loop {
                match read(&connection) {
                    Ok(batteries) => link.set_batteries(batteries),
                    Err(error) => tracing::debug!(%error, "notifications: no UPower"),
                }
                std::thread::sleep(POLL);
            }
        })
        .expect("malaysia-batteries thread");
}
