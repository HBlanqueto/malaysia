use bk::patterns::settings::Pane;
use bk::prelude::*;
use bk::views::form::{Form, Row, Section};
use ctdesign::ACCENT_SWATCHES;
use malaysia_services::bus::Live;
use malaysia_services::upower::{
    Battery, BatteryCommand, Charge, HEALTHY_CAPACITY, PowerProfiles, ProfileCommand, UPower,
};

use crate::localize::{intern, tr};
use crate::menubar::show_in_menu_bar;
use ctappearance::menubar::MenubarConfig;

const TINT: ctdesign::Rgba = ACCENT_SWATCHES[6].1;

pub fn energy_pane(
    power: &Live<UPower>,
    profiles: &Live<PowerProfiles>,
    menubar: &MenubarConfig,
    error: State<Option<String>>,
) -> Pane {
    let battery = power.snapshot.read().flatten();
    let mut form = Form::new();
    if let Some(battery) = battery {
        form = form.section(Section::new().row(Row::value(tr!("battery-level"), level(&battery))));
    }
    if let Some(current) = profiles.snapshot.read().flatten() {
        let remote = profiles.remote.clone();
        form = form.section(
            Section::new().row(
                Row::new(tr!("low-power-mode")).control(
                    Switch::new(tr!("low-power-mode"))
                        .on(current.low_power)
                        .on_toggle(move |on| remote.send(ProfileCommand::LowPower(on))),
                ),
            ),
        );
    }
    if let Some(battery) = battery {
        form = form.section(health(&battery, power)).section(
            Section::new()
                .row(show_in_menu_bar(
                    menubar,
                    tr!("show-in-menu-bar"),
                    |config| &mut config.show_battery,
                    error,
                ))
                .row(show_in_menu_bar(
                    menubar,
                    tr!("show-battery-percentage"),
                    |config| &mut config.show_battery_percentage,
                    error,
                )),
        );
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
            Row::new(title).control(
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
    let state = match battery.charge() {
        Charge::Full => Some(tr!("battery-charged").to_string()),
        Charge::Charging(Some(time)) => Some(tr!("battery-until-full", time = time)),
        Charge::Charging(None) => Some(tr!("battery-charging").to_string()),
        Charge::Discharging(Some(time)) => Some(tr!("battery-until-empty", time = time)),
        Charge::Discharging(None) => None,
    };
    match state {
        Some(state) => format!("{percent} · {state}"),
        None => percent,
    }
}
