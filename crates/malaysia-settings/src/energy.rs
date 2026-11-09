use bk::patterns::settings::Pane;
use bk::prelude::*;
use bk::views::form::{Form, Row, Section};
use ctdesign::ACCENT_SWATCHES;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::bus::{Live, Properties, Service, get_all, property, set};
use crate::localize::{intern, tr};

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
const HEALTHY_CAPACITY: f64 = 75.0;
const TINT: ctdesign::Rgba = ACCENT_SWATCHES[6].1;

#[derive(Clone, Copy, PartialEq)]
pub struct Battery {
    percent: f64,
    state: u32,
    until_full: i64,
    until_empty: i64,
    capacity: f64,
    cycles: i32,
    limit: Option<ChargeLimit>,
}

#[derive(Clone, Copy, PartialEq)]
struct ChargeLimit {
    enabled: bool,
    end: u32,
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
    low_power: bool,
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

pub fn energy_pane(power: &Live<UPower>, profiles: &Live<PowerProfiles>) -> Pane {
    let battery = power.snapshot.read().flatten();
    let mut form = Form::new();
    if let Some(battery) = battery {
        form = form.section(Section::new().row(Row::value(tr!("battery-level"), level(&battery))));
    }
    if let Some(current) = profiles.snapshot.read().flatten() {
        let remote = profiles.remote.clone();
        form = form.section(
            Section::new().row(
                Row::new(tr!("low-power-mode"))
                    .detail(tr!("low-power-mode-help"))
                    .control(
                        Switch::new(tr!("low-power-mode"))
                            .on(current.low_power)
                            .on_toggle(move |on| remote.send(ProfileCommand::LowPower(on))),
                    ),
            ),
        );
    }
    if let Some(battery) = battery {
        form = form.section(health(&battery, power));
    }
    let (id, title, symbol) = match battery {
        Some(_) => ("battery", tr!("battery"), Symbol::Battery),
        None => ("energy", tr!("energy"), Symbol::Bolt),
    };
    Pane::new(id, title, symbol, TINT, form)
}

fn health(battery: &Battery, power: &Live<UPower>) -> Section {
    let mut section = Section::titled(tr!("battery-health"));
    if battery.capacity > 0.0 {
        let condition = if battery.capacity < HEALTHY_CAPACITY {
            tr!("battery-service")
        } else {
            tr!("battery-normal")
        };
        section = section
            .row(Row::value(tr!("battery-condition"), condition))
            .row(Row::value(
                tr!("battery-capacity"),
                format!("{:.0} %", battery.capacity),
            ));
    }
    if battery.cycles > 0 {
        section = section.row(Row::value(
            tr!("battery-cycles"),
            battery.cycles.to_string(),
        ));
    }
    if let Some(limit) = battery.limit {
        let title = intern(tr!("charge-limit", percent = limit.end));
        let remote = power.remote.clone();
        section = section.row(
            Row::new(title).detail(tr!("charge-limit-help")).control(
                Switch::new(title)
                    .on(limit.enabled)
                    .on_toggle(move |on| remote.send(BatteryCommand::LimitCharge(on))),
            ),
        );
    }
    section
}

fn level(battery: &Battery) -> String {
    let percent = format!("{:.0} %", battery.percent);
    let state = match battery.state {
        FULLY_CHARGED => Some(tr!("battery-charged").to_string()),
        CHARGING if battery.until_full > 0 => Some(tr!(
            "battery-until-full",
            time = duration(battery.until_full)
        )),
        CHARGING => Some(tr!("battery-charging").to_string()),
        _ if battery.until_empty > 0 => Some(tr!(
            "battery-until-empty",
            time = duration(battery.until_empty)
        )),
        _ => None,
    };
    match state {
        Some(state) => format!("{percent} · {state}"),
        None => percent,
    }
}

fn duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, minutes) => format!("{hours} h {minutes:02} min"),
    }
}
