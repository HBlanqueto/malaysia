use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::bus::{Properties, Service, get_all, property, set};

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const DEVICE: &str = "org.freedesktop.UPower.Device";
const PROFILES: &str = "org.freedesktop.UPower.PowerProfiles";
const PROFILES_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const POWER_SAVER: &str = "power-saver";
const BALANCED: &str = "balanced";
// UPower's UpDeviceKind and UpDeviceState.
const KIND_BATTERY: u32 = 2;
const CHARGING: u32 = 1;
const FULLY_CHARGED: u32 = 4;
// UPower documents a capacity under 75 % as worth replacing.
pub const HEALTHY_CAPACITY: f64 = 75.0;

#[derive(Clone, Copy, PartialEq)]
pub struct Battery {
    pub percent: f64,
    pub state: u32,
    pub until_full: i64,
    pub until_empty: i64,
    pub capacity: f64,
    pub cycles: i32,
    pub limit: Option<ChargeLimit>,
}

#[derive(Clone, Copy, PartialEq)]
pub struct ChargeLimit {
    pub enabled: bool,
    pub end: u32,
}

#[derive(Default)]
pub struct UPower {
    battery: Option<OwnedObjectPath>,
}

pub enum BatteryCommand {
    LimitCharge(bool),
}

impl Service for UPower {
    const NAME: &'static str = UPOWER;
    type Snapshot = Option<Battery>;
    type Command = BatteryCommand;

    fn read(&mut self, bus: &Connection) -> zbus::Result<Option<Battery>> {
        let devices: Vec<OwnedObjectPath> = bus
            .call_method(
                Some(UPOWER),
                UPOWER_PATH,
                Some(UPOWER),
                "EnumerateDevices",
                &(),
            )?
            .body()
            .deserialize()?;
        self.battery = None;
        for path in devices {
            let device = get_all(bus, UPOWER, path.as_ref(), DEVICE)?;
            if property(&device, "Type") == Some(KIND_BATTERY)
                && property(&device, "PowerSupply") == Some(true)
            {
                self.battery = Some(path);
                return Ok(Some(battery(&device)));
            }
        }
        Ok(None)
    }

    fn run(&mut self, bus: &Connection, command: BatteryCommand) -> zbus::Result<()> {
        let BatteryCommand::LimitCharge(enabled) = command;
        let Some(path) = &self.battery else {
            return Ok(());
        };
        bus.call_method(
            Some(UPOWER),
            path,
            Some(DEVICE),
            "EnableChargeThreshold",
            &enabled,
        )?;
        Ok(())
    }
}

fn battery(device: &Properties) -> Battery {
    let limit = (property(device, "ChargeThresholdSupported") == Some(true)).then(|| ChargeLimit {
        enabled: property(device, "ChargeThresholdEnabled").unwrap_or(false),
        end: property(device, "ChargeEndThreshold").unwrap_or(0),
    });
    Battery {
        percent: property(device, "Percentage").unwrap_or(0.0),
        state: property(device, "State").unwrap_or(0),
        until_full: property(device, "TimeToFull").unwrap_or(0),
        until_empty: property(device, "TimeToEmpty").unwrap_or(0),
        capacity: property(device, "Capacity").unwrap_or(0.0),
        cycles: property(device, "ChargeCycles").unwrap_or(-1),
        limit,
    }
}

#[derive(Default)]
pub struct PowerProfiles;

#[derive(Clone, Copy, PartialEq)]
pub struct Profiles {
    pub low_power: bool,
}

pub enum ProfileCommand {
    LowPower(bool),
}

impl Service for PowerProfiles {
    const NAME: &'static str = PROFILES;
    type Snapshot = Option<Profiles>;
    type Command = ProfileCommand;

    fn read(&mut self, bus: &Connection) -> zbus::Result<Option<Profiles>> {
        let daemon = get_all(bus, PROFILES, PROFILES_PATH, PROFILES)?;
        let profiles: Vec<std::collections::HashMap<String, OwnedValue>> =
            property(&daemon, "Profiles").unwrap_or_default();
        let saver_available = profiles
            .iter()
            .any(|profile| property::<String>(profile, "Profile").as_deref() == Some(POWER_SAVER));
        Ok(saver_available.then(|| Profiles {
            low_power: property::<String>(&daemon, "ActiveProfile").as_deref() == Some(POWER_SAVER),
        }))
    }

    fn run(&mut self, bus: &Connection, command: ProfileCommand) -> zbus::Result<()> {
        let ProfileCommand::LowPower(on) = command;
        let profile = if on { POWER_SAVER } else { BALANCED };
        set(
            bus,
            PROFILES,
            PROFILES_PATH,
            PROFILES,
            "ActiveProfile",
            Value::from(profile),
        )
    }
}

pub enum Charge {
    Full,
    Charging(Option<String>),
    Discharging(Option<String>),
}

impl Battery {
    pub fn charge(&self) -> Charge {
        let time = |seconds: i64| (seconds > 0).then(|| duration(seconds));
        match self.state {
            FULLY_CHARGED => Charge::Full,
            CHARGING => Charge::Charging(time(self.until_full)),
            _ => Charge::Discharging(time(self.until_empty)),
        }
    }
}

fn duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, minutes) => format!("{hours} h {minutes:02} min"),
    }
}
