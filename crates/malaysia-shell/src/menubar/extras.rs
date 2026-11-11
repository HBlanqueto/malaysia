use bk::app::use_defaults;
use std::cell::Cell;
use std::rc::Rc;

use bk::menu::{CheckState, MenuIcon, MenuItem};
use bk::prelude::*;
use bk::style::Palette;
use ctappearance::menubar::MenubarConfig;
use ctdesign::{ACCENT_SWATCHES, MENUBAR_FADED_ALPHA, MENUBAR_MARGIN, Rgba, SYMBOL};
use freya_components::svg_viewer::SvgViewer;
use malaysia_services::bluez::{BlueZ, Bluetooth, BluetoothCommand, Device};
use malaysia_services::bus::{Live, Remote, use_service};
use malaysia_services::cups::Cups;
use malaysia_services::networkmanager::{
    AskToJoin, ExternalVpn, LinkState, Network, NetworkCommand, NetworkManager, Security, Vpn,
    WIFI_DOMAIN, Wifi, WifiConfig, WifiNetwork, signal_svg,
};
use malaysia_services::upower::{Battery, Charge, PowerProfiles, ProfileCommand, Profiles, UPower};

use crate::localize::tr;
use crate::menubar::menu::MenuTitleSpec;
use crate::menubar::status::StatusAnchors;
use crate::spawn::open_settings;

const CHARGING: Rgba = ACCENT_SWATCHES[6].1;
// macOS paints the battery red from 20 % down.
const LOW_BATTERY: f64 = 20.0;
const BATTERY_ICON: (f32, f32) = (24.0, 12.0);

pub struct Extras {
    network: Live<NetworkManager>,
    bluetooth: Live<BlueZ>,
    power: Live<UPower>,
    profiles: Live<PowerProfiles>,
    printer: Live<Cups>,
}

pub fn use_extras() -> Extras {
    let network = use_service::<NetworkManager>();
    use_ask_to_join(network.snapshot);
    Extras {
        network,
        bluetooth: use_service::<BlueZ>(),
        power: use_service::<UPower>(),
        profiles: use_service::<PowerProfiles>(),
        printer: use_service::<Cups>(),
    }
}

// macOS «Ask to join networks»: once per stretch without a known network in range.
fn use_ask_to_join(snapshot: State<Option<Network>>) {
    let config = use_defaults::<WifiConfig>(WIFI_DOMAIN);
    let mut asked = use_state(|| false);
    use_side_effect(move || {
        let alone = snapshot
            .read()
            .as_ref()
            .and_then(|network| network.wifi.as_ref())
            .filter(|wifi| wifi.only_unknown_networks_in_range())
            .map(|wifi| wifi.networks.len());
        let Some(count) = alone else {
            if *asked.peek() {
                asked.set(false);
            }
            return;
        };
        if *asked.peek() {
            return;
        }
        asked.set(true);
        match config.read().ask_to_join {
            AskToJoin::Off => {}
            AskToJoin::Notify => notify_networks(count),
            AskToJoin::Ask => open_settings("wifi"),
        }
    });
}

fn notify_networks(count: usize) {
    let summary = tr!("wifi-networks-available").to_string();
    let body = tr!("wifi-networks-available-body", count = count);
    let sent = std::thread::Builder::new()
        .name("wifi-notify".to_string())
        .spawn(move || {
            let notified = zbus::blocking::Connection::session().and_then(|bus| {
                let hints: std::collections::HashMap<&str, zbus::zvariant::Value> =
                    std::collections::HashMap::new();
                bus.call_method(
                    Some("org.freedesktop.Notifications"),
                    "/org/freedesktop/Notifications",
                    Some("org.freedesktop.Notifications"),
                    "Notify",
                    &(
                        tr!("wifi"),
                        0u32,
                        "network-wireless",
                        summary.as_str(),
                        body.as_str(),
                        Vec::<&str>::new(),
                        hints,
                        -1i32,
                    ),
                )
            });
            if let Err(error) = notified {
                tracing::debug!(%error, "no notification for Wi-Fi networks");
            }
        });
    if let Err(error) = sent {
        tracing::warn!(%error, "no thread for the Wi-Fi notification");
    }
}

pub fn extra_titles(
    extras: &Extras,
    config: &MenubarConfig,
    ink: Rgba,
    anchors: &StatusAnchors,
) -> Vec<MenuTitleSpec> {
    let network = extras.network.snapshot.read().clone();
    let anchor = |key: &str| {
        anchors
            .borrow_mut()
            .entry(key.to_string())
            .or_insert_with(|| Rc::new(Cell::new((0.0, 0.0))))
            .clone()
    };
    let mut titles = Vec::new();
    if config.show_vpn
        && let Some(network) = &network
        && !(network.vpns.is_empty() && network.external_vpns.is_empty())
    {
        titles.push(vpn_title(
            network,
            &extras.network.remote,
            ink,
            anchor("extra-vpn"),
        ));
    }
    if config.show_bluetooth
        && let Some(adapter) = extras.bluetooth.snapshot.read().clone().flatten()
    {
        titles.push(bluetooth_title(
            adapter,
            &extras.bluetooth.remote,
            ink,
            anchor("extra-bluetooth"),
        ));
    }
    if let Some(jobs) = queued_jobs(extras, config) {
        titles.push(printer_title(jobs, ink, anchor("extra-printer")));
    }
    if config.show_wifi
        && let Some(wifi) = network.and_then(|network| network.wifi)
    {
        titles.push(wifi_title(
            wifi,
            &extras.network.remote,
            ink,
            anchor("extra-wifi"),
        ));
    }
    if config.show_battery
        && let Some(battery) = extras.power.snapshot.read().flatten()
    {
        titles.push(battery_title(
            battery,
            extras.profiles.snapshot.read().flatten(),
            &extras.profiles.remote,
            config.show_battery_percentage,
            ink,
            anchor("extra-battery"),
        ));
    }
    titles
}

// ADR 0074: the lock screen's status row; same order as the bar, without menus.
pub fn lock_icons(extras: &Extras, config: &MenubarConfig, ink: Rgba) -> Vec<Element> {
    let mut icons = Vec::new();
    if config.show_bluetooth
        && extras
            .bluetooth
            .snapshot
            .read()
            .clone()
            .flatten()
            .is_some_and(|adapter| adapter.powered)
    {
        icons.push(symbol_icon(Symbol::Bluetooth, ink));
    }
    if queued_jobs(extras, config).is_some() {
        icons.push(symbol_icon(Symbol::Printer, ink));
    }
    if config.show_wifi
        && let Some(wifi) = extras
            .network
            .snapshot
            .read()
            .clone()
            .and_then(|network| network.wifi)
    {
        icons.push(wifi_icon(&wifi, ink));
    }
    if config.show_battery
        && let Some(battery) = extras.power.snapshot.read().flatten()
    {
        icons.push(battery_icon(&battery, config.show_battery_percentage, ink));
    }
    icons
}

fn queued_jobs(extras: &Extras, config: &MenubarConfig) -> Option<usize> {
    (*extras.printer.snapshot.read()).filter(|&jobs| config.show_printer && jobs > 0)
}

fn printer_title(jobs: usize, ink: Rgba, anchor: Rc<Cell<(f32, f32)>>) -> MenuTitleSpec {
    title(
        tr!("printer"),
        symbol_icon(Symbol::Printer, ink),
        vec![MenuItem::placeholder(tr!("printer-jobs", count = jobs))],
        |_| {},
        anchor,
    )
}

fn title(
    label: &str,
    icon: Element,
    items: Vec<MenuItem>,
    on_activate: impl Fn(&str) + 'static,
    anchor: Rc<Cell<(f32, f32)>>,
) -> MenuTitleSpec {
    MenuTitleSpec {
        icon: Some(icon),
        ..MenuTitleSpec::new(
            label.to_string(),
            Rc::new(move || items.clone()),
            Rc::new(move |id: String| on_activate(&id)),
            anchor,
        )
    }
}

fn settings_item(pane: &str, label: &str) -> MenuItem {
    MenuItem::action(format!("settings.{pane}"), label)
}

fn open_settings_for(id: &str) -> bool {
    match id.strip_prefix("settings.") {
        Some(pane) => {
            open_settings(pane);
            true
        }
        None => false,
    }
}

fn indexed(id: &str, prefix: &str) -> Option<usize> {
    id.strip_prefix(prefix)?.parse().ok()
}

fn symbol_icon(symbol: Symbol, ink: Rgba) -> Element {
    Image::system(symbol).size(SYMBOL.grid).color(ink).into()
}

fn faded(ink: Rgba) -> Rgba {
    Rgba {
        a: ink.a * MENUBAR_FADED_ALPHA,
        ..ink
    }
}

fn svg_icon(svg: String, (width, height): (f32, f32)) -> Element {
    SvgViewer::new((svg.clone(), Bytes::from(svg)))
        .width(Size::px(width))
        .height(Size::px(height))
        .show_loader(false)
        .into_element()
}

fn wifi_title(
    wifi: Wifi,
    remote: &Remote<NetworkCommand>,
    ink: Rgba,
    anchor: Rc<Cell<(f32, f32)>>,
) -> MenuTitleSpec {
    let palette = Palette::current();
    let icon = wifi_icon(&wifi, ink);
    let toggle = if wifi.enabled {
        tr!("wifi-turn-off")
    } else {
        tr!("wifi-turn-on")
    };
    let mut items = vec![MenuItem::action("wifi.toggle", toggle).with_enabled(wifi.switchable)];
    if wifi.enabled {
        let (known, other): (Vec<_>, Vec<_>) = wifi
            .networks
            .iter()
            .enumerate()
            .partition(|(_, network)| network.profile.is_some() || network.current);
        let entry = |(index, network): (usize, &WifiNetwork)| {
            MenuItem::action(format!("wifi.network.{index}"), network.name.clone())
                .with_icon(MenuIcon::Svg(
                    signal_svg(network.bars, palette.label, palette.tertiary_label).into(),
                ))
                .with_check(CheckState::Checkbox(network.current))
        };
        if !known.is_empty() {
            items.push(MenuItem::Separator);
            items.push(MenuItem::header(tr!("wifi-known")));
            items.extend(known.into_iter().map(entry));
        }
        if !other.is_empty() {
            items.push(MenuItem::Separator);
            items.push(MenuItem::header(tr!("wifi-other")));
            items.extend(other.into_iter().map(entry));
        }
    }
    items.push(MenuItem::Separator);
    items.push(settings_item("wifi", tr!("wifi-settings")));
    let remote = remote.clone();
    let scan = remote.clone();
    let enabled = wifi.enabled;
    let networks = wifi.networks;
    let mut spec = title(
        tr!("wifi"),
        icon,
        items,
        move |id| {
            if open_settings_for(id) {
                return;
            }
            if id == "wifi.toggle" {
                remote.send(NetworkCommand::Wifi(!enabled));
            } else if let Some(network) =
                indexed(id, "wifi.network.").and_then(|index| networks.get(index))
            {
                join(network, &remote);
            }
        },
        anchor,
    );
    spec.on_open = Some(Rc::new(move || scan.send(NetworkCommand::Scan)));
    spec
}

fn wifi_icon(wifi: &Wifi, ink: Rgba) -> Element {
    let bars = wifi
        .networks
        .iter()
        .find(|network| network.current && wifi.state == LinkState::Connected)
        .map_or(0, |network| network.bars);
    svg_icon(
        signal_svg(bars, ink, faded(ink)),
        (SYMBOL.grid, SYMBOL.grid),
    )
}

fn join(network: &WifiNetwork, remote: &Remote<NetworkCommand>) {
    match (&network.profile, network.security) {
        (Some(profile), _) => remote.send(NetworkCommand::Activate(profile.clone())),
        (None, Security::Open | Security::Encrypted) => remote.send(NetworkCommand::Join {
            access_point: network.access_point.clone(),
            password: None,
        }),
        (None, Security::Personal | Security::Enterprise) => open_settings("wifi"),
    }
}

fn bluetooth_title(
    adapter: Bluetooth,
    remote: &Remote<BluetoothCommand>,
    ink: Rgba,
    anchor: Rc<Cell<(f32, f32)>>,
) -> MenuTitleSpec {
    let toggle = if adapter.powered {
        tr!("bluetooth-turn-off")
    } else {
        tr!("bluetooth-turn-on")
    };
    let mut items =
        vec![MenuItem::action("bluetooth.toggle", toggle).with_enabled(!adapter.hardware_blocked)];
    let devices: Vec<Device> = adapter
        .devices
        .into_iter()
        .filter(|device| device.paired)
        .collect();
    if adapter.powered && !devices.is_empty() {
        items.push(MenuItem::Separator);
        items.push(MenuItem::header(tr!("bluetooth-devices")));
        items.extend(devices.iter().enumerate().map(|(index, device)| {
            let item = MenuItem::action(format!("bluetooth.device.{index}"), device.name.clone())
                .with_check(CheckState::Checkbox(device.connected));
            match device.battery {
                Some(percent) if device.connected => item.with_badge(format!("{percent} %")),
                _ => item,
            }
        }));
    }
    items.push(MenuItem::Separator);
    items.push(settings_item("bluetooth", tr!("bluetooth-settings")));
    let remote = remote.clone();
    let powered = adapter.powered;
    title(
        tr!("bluetooth"),
        symbol_icon(Symbol::Bluetooth, ink),
        items,
        move |id| {
            if open_settings_for(id) {
                return;
            }
            if id == "bluetooth.toggle" {
                remote.send(BluetoothCommand::Power(!powered));
            } else if let Some(device) =
                indexed(id, "bluetooth.device.").and_then(|index| devices.get(index))
            {
                remote.send(if device.connected {
                    BluetoothCommand::Disconnect(device.path.clone())
                } else {
                    BluetoothCommand::Connect(device.path.clone())
                });
            }
        },
        anchor,
    )
}

fn vpn_title(
    network: &Network,
    remote: &Remote<NetworkCommand>,
    ink: Rgba,
    anchor: Rc<Cell<(f32, f32)>>,
) -> MenuTitleSpec {
    let vpns: Vec<Vpn> = network.vpns.clone();
    let mut items: Vec<MenuItem> = vpns
        .iter()
        .enumerate()
        .map(|(index, vpn)| {
            MenuItem::action(format!("vpn.{index}"), vpn.name.clone())
                .with_check(CheckState::Checkbox(vpn.state != LinkState::Disconnected))
                .with_enabled(vpn.available)
        })
        .collect();
    let external: Vec<ExternalVpn> = network.external_vpns.clone();
    if !external.is_empty() {
        if !items.is_empty() {
            items.push(MenuItem::Separator);
        }
        items.push(MenuItem::header(tr!("vpn-external")));
        items.extend(external.iter().enumerate().map(|(index, vpn)| {
            MenuItem::action(format!("vpn-app.{index}"), vpn.name.clone())
                .with_enabled(vpn.desktop_entry.is_some())
        }));
    }
    items.push(MenuItem::Separator);
    items.push(settings_item("vpn", tr!("vpn-settings")));
    let active = !external.is_empty() || vpns.iter().any(|vpn| vpn.state == LinkState::Connected);
    let remote = remote.clone();
    title(
        tr!("vpn"),
        symbol_icon(Symbol::Lock, if active { ink } else { faded(ink) }),
        items,
        move |id| {
            if open_settings_for(id) {
                return;
            }
            if let Some(entry) =
                indexed(id, "vpn-app.").and_then(|index| external.get(index)?.desktop_entry)
            {
                if let Err(error) = crate::launcher::open_app(Some(entry)) {
                    tracing::warn!(%error, "could not open the VPN app");
                }
                return;
            }
            let Some(vpn) = indexed(id, "vpn.").and_then(|index| vpns.get(index)) else {
                return;
            };
            match &vpn.active {
                Some(active) => remote.send(NetworkCommand::Deactivate(active.clone())),
                None => remote.send(NetworkCommand::Activate(vpn.profile.clone())),
            }
        },
        anchor,
    )
}

fn battery_title(
    battery: Battery,
    profiles: Option<Profiles>,
    remote: &Remote<ProfileCommand>,
    show_percentage: bool,
    ink: Rgba,
    anchor: Rc<Cell<(f32, f32)>>,
) -> MenuTitleSpec {
    let charge = battery.charge();
    let percent = format!("{:.0} %", battery.percent);
    let icon = battery_icon(&battery, show_percentage, ink);
    let (source, state) = match charge {
        Charge::Full => (
            tr!("power-source-adapter"),
            tr!("battery-charged").to_string(),
        ),
        Charge::Charging(Some(time)) => (
            tr!("power-source-adapter"),
            tr!("battery-until-full", time = time),
        ),
        Charge::Charging(None) => (
            tr!("power-source-adapter"),
            tr!("battery-charging").to_string(),
        ),
        Charge::Discharging(Some(time)) => (
            tr!("power-source-battery"),
            tr!("battery-until-empty", time = time),
        ),
        Charge::Discharging(None) => (tr!("power-source-battery"), String::new()),
    };
    let mut items = vec![
        MenuItem::header(format!("{} · {percent}", tr!("battery"))),
        MenuItem::placeholder(source),
    ];
    if !state.is_empty() {
        items.push(MenuItem::placeholder(state));
    }
    if let Some(profiles) = profiles {
        items.push(MenuItem::Separator);
        items.push(
            MenuItem::action("power.low", tr!("low-power-mode"))
                .with_check(CheckState::Checkbox(profiles.low_power)),
        );
    }
    items.push(MenuItem::Separator);
    items.push(settings_item("battery", tr!("battery-settings")));
    let remote = remote.clone();
    let low_power = profiles.is_some_and(|profiles| profiles.low_power);
    title(
        tr!("battery"),
        icon,
        items,
        move |id| {
            if open_settings_for(id) {
                return;
            }
            if id == "power.low" {
                remote.send(ProfileCommand::LowPower(!low_power));
            }
        },
        anchor,
    )
}

fn battery_icon(battery: &Battery, show_percentage: bool, ink: Rgba) -> Element {
    let fill = match battery.charge() {
        Charge::Charging(_) | Charge::Full => CHARGING,
        Charge::Discharging(_) if battery.percent <= LOW_BATTERY => Palette::current().destructive,
        Charge::Discharging(_) => ink,
    };
    HStack::new()
        .spacing(MENUBAR_MARGIN as f32 / 2.0)
        .maybe_child(
            show_percentage
                .then(|| Text::new(format!("{:.0} %", battery.percent)).color(color(ink))),
        )
        .child(svg_icon(
            battery_svg(battery.percent, ink, fill),
            BATTERY_ICON,
        ))
        .into_element()
}

fn battery_svg(percent: f64, outline: Rgba, fill: Rgba) -> String {
    let [r, g, b, _] = outline.to_rgba8();
    let [fr, fg, fb, _] = fill.to_rgba8();
    let level = (percent.clamp(0.0, 100.0) / 100.0 * 17.0).max(1.5);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><rect x="0.5" y="0.5" width="20" height="11" rx="3" fill="none" stroke="rgb({r},{g},{b})" stroke-opacity="0.55"/><rect x="2" y="2" width="{level:.1}" height="8" rx="1.6" fill="rgb({fr},{fg},{fb})"/><path d="M22.5 4.5v3" stroke="rgb({r},{g},{b})" stroke-opacity="0.55" stroke-linecap="round"/></svg>"#
    )
}
