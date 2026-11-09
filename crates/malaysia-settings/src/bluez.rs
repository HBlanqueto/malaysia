use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};

use crate::bus::{Properties, Service, property, set};

const BLUEZ: &str = "org.bluez";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const BATTERY: &str = "org.bluez.Battery1";
const AGENT_MANAGER: &str = "org.bluez.AgentManager1";
const AGENT_PATH: &str = "/io/github/hblanqueto/malaysia/settings/agent";
const AGENT_CAPABILITY: &str = "KeyboardDisplay";
const RFKILL_DEVICE: &str = "/dev/rfkill";
const RFKILL_CLASS: &str = "/sys/class/rfkill";
// struct rfkill_event from linux/rfkill.h: idx, type, op, soft, hard.
const RFKILL_TYPE_BLUETOOTH: u8 = 2;
const RFKILL_OP_CHANGE_ALL: u8 = 3;
// The adapter leaves the rfkill block a moment after the kernel lifts it.
const UNBLOCK_SETTLE: Duration = Duration::from_millis(300);

pub static PROMPTS: LazyLock<(
    async_channel::Sender<Prompt>,
    async_channel::Receiver<Prompt>,
)> = LazyLock::new(async_channel::unbounded);
static PAIRING: Mutex<Option<OwnedObjectPath>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub path: OwnedObjectPath,
    pub name: String,
    pub icon: String,
    pub paired: bool,
    pub connected: bool,
    pub battery: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bluetooth {
    pub powered: bool,
    pub hardware_blocked: bool,
    pub name: String,
    pub devices: Vec<Device>,
}

pub enum BluetoothCommand {
    Power(bool),
    Visible(bool),
    Connect(OwnedObjectPath),
    Disconnect(OwnedObjectPath),
    Pair(OwnedObjectPath),
    Forget(OwnedObjectPath),
}

#[derive(Clone)]
pub enum Prompt {
    Confirm {
        device: String,
        passkey: String,
        reply: async_channel::Sender<bool>,
    },
    Display {
        device: String,
        passkey: String,
    },
    Done,
}

#[derive(Default)]
pub struct BlueZ {
    adapter: Option<OwnedObjectPath>,
    agent: bool,
    visible: bool,
}

type Objects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

impl Service for BlueZ {
    const NAME: &'static str = BLUEZ;
    type Snapshot = Option<Bluetooth>;
    type Command = BluetoothCommand;

    fn read(&mut self, bus: &Connection) -> zbus::Result<Option<Bluetooth>> {
        let objects: Objects = bus
            .call_method(
                Some(BLUEZ),
                "/",
                Some(OBJECT_MANAGER),
                "GetManagedObjects",
                &(),
            )
            .inspect_err(|_| self.agent = false)?
            .body()
            .deserialize()?;
        if !self.agent {
            self.agent = register_agent(bus)
                .inspect_err(|error| tracing::warn!(%error, "no Bluetooth pairing agent"))
                .is_ok();
        }
        let adapter = objects
            .iter()
            .filter_map(|(path, interfaces)| Some((path, interfaces.get(ADAPTER)?)))
            .min_by_key(|(path, adapter)| {
                (property(adapter, "Powered") != Some(true), path.as_str())
            });
        let Some((path, adapter)) = adapter else {
            self.adapter = None;
            return Ok(None);
        };
        self.adapter = Some(path.clone());
        let hardware_blocked = hardware_blocked();
        let mut devices: Vec<Device> = objects
            .iter()
            .filter(|(device, _)| device.as_str().starts_with(path.as_str()))
            .filter_map(|(device, interfaces)| {
                let found = interfaces.get(DEVICE)?;
                Some(Device {
                    path: device.clone(),
                    name: property(found, "Alias").unwrap_or_default(),
                    icon: property(found, "Icon").unwrap_or_default(),
                    paired: property(found, "Paired").unwrap_or(false),
                    connected: property(found, "Connected").unwrap_or(false),
                    battery: interfaces
                        .get(BATTERY)
                        .and_then(|battery| property(battery, "Percentage")),
                })
            })
            .filter(worth_listing)
            .collect();
        devices.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(Some(Bluetooth {
            powered: property(adapter, "Powered").unwrap_or(false),
            hardware_blocked,
            name: property(adapter, "Alias").unwrap_or_default(),
            devices,
        }))
    }

    fn run(&mut self, bus: &Connection, command: BluetoothCommand) -> zbus::Result<()> {
        match command {
            BluetoothCommand::Power(on) => self.power(bus, on),
            BluetoothCommand::Visible(visible) => {
                self.visible = visible;
                self.discover(bus, visible);
                Ok(())
            }
            BluetoothCommand::Connect(device) => call(bus, &device, DEVICE, "Connect"),
            BluetoothCommand::Disconnect(device) => call(bus, &device, DEVICE, "Disconnect"),
            BluetoothCommand::Pair(device) => pair(bus, device),
            BluetoothCommand::Forget(device) => {
                let Some(adapter) = &self.adapter else {
                    return Ok(());
                };
                bus.call_method(
                    Some(BLUEZ),
                    adapter,
                    Some(ADAPTER),
                    "RemoveDevice",
                    &device.as_ref(),
                )?;
                Ok(())
            }
        }
    }
}

impl BlueZ {
    fn power(&mut self, bus: &Connection, on: bool) -> zbus::Result<()> {
        let Some(adapter) = self.adapter.clone() else {
            return Ok(());
        };
        // rfkill keeps the choice across reboots (systemd-rfkill); Powered alone does not.
        match block_bluetooth(!on) {
            Ok(()) if !on => return Ok(()),
            Ok(()) => thread::sleep(UNBLOCK_SETTLE),
            Err(error) => tracing::debug!(%error, "rfkill unavailable, using Powered only"),
        }
        set(bus, BLUEZ, adapter, ADAPTER, "Powered", Value::from(on))?;
        if on && self.visible {
            self.discover(bus, true);
        }
        Ok(())
    }

    fn discover(&self, bus: &Connection, visible: bool) {
        let Some(adapter) = &self.adapter else {
            return;
        };
        let method = if visible {
            "StartDiscovery"
        } else {
            "StopDiscovery"
        };
        let discovered = set(
            bus,
            BLUEZ,
            adapter,
            ADAPTER,
            "Discoverable",
            Value::from(visible),
        )
        .and_then(|()| call(bus, adapter, ADAPTER, method));
        if let Err(error) = discovered {
            tracing::debug!(%error, method, "Bluetooth discovery");
        }
    }
}

fn worth_listing(device: &Device) -> bool {
    device.paired || !device.icon.is_empty() && !is_address(&device.name)
}

// BlueZ falls back to the address, with dashes, when a device has no name.
fn is_address(name: &str) -> bool {
    let parts: Vec<&str> = name.split('-').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
}

fn call(
    bus: &Connection,
    path: &OwnedObjectPath,
    interface: &str,
    method: &str,
) -> zbus::Result<()> {
    bus.call_method(Some(BLUEZ), path, Some(interface), method, &())?;
    Ok(())
}

fn pair(bus: &Connection, device: OwnedObjectPath) -> zbus::Result<()> {
    *PAIRING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(device.clone());
    let paired = match call(bus, &device, DEVICE, "Pair") {
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.bluez.Error.AlreadyExists" =>
        {
            Ok(())
        }
        other => other,
    };
    *PAIRING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    let _ = PROMPTS.0.send_blocking(Prompt::Done);
    paired?;
    set(
        bus,
        BLUEZ,
        device.clone(),
        DEVICE,
        "Trusted",
        Value::from(true),
    )?;
    call(bus, &device, DEVICE, "Connect")
}

fn register_agent(bus: &Connection) -> zbus::Result<()> {
    bus.object_server().at(AGENT_PATH, Agent)?;
    let path = ObjectPath::from_static_str_unchecked(AGENT_PATH);
    bus.call_method(
        Some(BLUEZ),
        "/org/bluez",
        Some(AGENT_MANAGER),
        "RegisterAgent",
        &(&path, AGENT_CAPABILITY),
    )?;
    bus.call_method(
        Some(BLUEZ),
        "/org/bluez",
        Some(AGENT_MANAGER),
        "RequestDefaultAgent",
        &(&path,),
    )?;
    Ok(())
}

fn block_bluetooth(blocked: bool) -> io::Result<()> {
    let event = [
        0,
        0,
        0,
        0,
        RFKILL_TYPE_BLUETOOTH,
        RFKILL_OP_CHANGE_ALL,
        u8::from(blocked),
        0,
    ];
    OpenOptions::new()
        .write(true)
        .open(RFKILL_DEVICE)?
        .write_all(&event)
}

fn hardware_blocked() -> bool {
    let Ok(entries) = fs::read_dir(RFKILL_CLASS) else {
        return false;
    };
    let read =
        |dir: &std::path::Path, name: &str| fs::read_to_string(dir.join(name)).unwrap_or_default();
    entries
        .flatten()
        .map(|entry| entry.path())
        .any(|dir| read(&dir, "type").trim() == "bluetooth" && read(&dir, "hard").trim() == "1")
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
}

struct Agent;

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {}

    async fn request_confirmation(
        &self,
        #[zbus(connection)] bus: &zbus::Connection,
        device: OwnedObjectPath,
        passkey: u32,
    ) -> Result<(), AgentError> {
        let (reply, answer) = async_channel::bounded(1);
        let prompt = Prompt::Confirm {
            device: device_name(bus, &device).await,
            passkey: format!("{passkey:06}"),
            reply,
        };
        let _ = PROMPTS.0.send(prompt).await;
        match answer.recv().await {
            Ok(true) => Ok(()),
            _ => Err(AgentError::Rejected(String::new())),
        }
    }

    async fn display_passkey(
        &self,
        #[zbus(connection)] bus: &zbus::Connection,
        device: OwnedObjectPath,
        passkey: u32,
        _entered: u16,
    ) {
        let prompt = Prompt::Display {
            device: device_name(bus, &device).await,
            passkey: format!("{passkey:06}"),
        };
        let _ = PROMPTS.0.send(prompt).await;
    }

    async fn display_pin_code(
        &self,
        #[zbus(connection)] bus: &zbus::Connection,
        device: OwnedObjectPath,
        pincode: String,
    ) {
        let prompt = Prompt::Display {
            device: device_name(bus, &device).await,
            passkey: pincode,
        };
        let _ = PROMPTS.0.send(prompt).await;
    }

    fn request_pin_code(&self, _device: OwnedObjectPath) -> Result<String, AgentError> {
        Err(AgentError::Rejected(String::new()))
    }

    fn request_passkey(&self, _device: OwnedObjectPath) -> Result<u32, AgentError> {
        Err(AgentError::Rejected(String::new()))
    }

    fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), AgentError> {
        let pairing = PAIRING
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pairing.as_ref() == Some(&device) {
            Ok(())
        } else {
            Err(AgentError::Rejected(String::new()))
        }
    }

    fn authorize_service(&self, _device: OwnedObjectPath, _uuid: String) -> Result<(), AgentError> {
        Err(AgentError::Rejected(String::new()))
    }

    async fn cancel(&self) {
        let _ = PROMPTS.0.send(Prompt::Done).await;
    }
}

async fn device_name(bus: &zbus::Connection, device: &OwnedObjectPath) -> String {
    let alias = async {
        let proxy = zbus::fdo::PropertiesProxy::builder(bus)
            .destination(BLUEZ)?
            .path(device.as_ref())?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;
        let value = proxy
            .get(
                zbus::names::InterfaceName::from_static_str_unchecked(DEVICE),
                "Alias",
            )
            .await?;
        Ok::<_, zbus::Error>(String::try_from(value)?)
    };
    alias.await.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::is_address;

    #[test]
    fn an_address_fallback_is_not_a_device_name() {
        assert!(is_address("A4-C1-38-0B-9F-2E"));
        assert!(!is_address("WH-CH720N"));
        assert!(!is_address("MX Master 3S"));
    }
}
