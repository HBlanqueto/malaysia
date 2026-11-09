use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use ctdesign::Rgba;
use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zeroize::Zeroizing;

use crate::bus::{Properties, Service, property, set};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const OBJECTS_PATH: &str = "/org/freedesktop";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const PROFILE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const IP4_CONFIG: &str = "org.freedesktop.NetworkManager.IP4Config";
const WIFI_SECURITY: &str = "802-11-wireless-security";
// Update2 flag NM_SETTINGS_UPDATE2_FLAG_TO_DISK.
const UPDATE_TO_DISK: u32 = 0x1;
// NMMetered: an explicit «yes» is Low Data Mode; 0 lets NetworkManager guess.
const METERED_YES: i32 = 1;
const METERED_UNKNOWN: i32 = 0;
const WIFI_PROFILE: &str = "802-11-wireless";
const VPN_PROFILES: [&str; 2] = ["vpn", "wireguard"];
// NMDeviceType values listed in the Network pane; Wi-Fi has its own pane and VPNs theirs.
const ETHERNET: u32 = 1;
const WIFI: u32 = 2;
const MODEM: u32 = 8;
const BOND: u32 = 10;
const VLAN: u32 = 11;
const BRIDGE: u32 = 13;
// NMDeviceState and NMActiveConnectionState.
const DEVICE_PREPARE: u32 = 40;
const DEVICE_ACTIVATED: u32 = 100;
const DEVICE_FAILED: u32 = 120;
const DEVICE_DISCONNECTED: u32 = 30;
const ACTIVE_ACTIVATING: u32 = 1;
const ACTIVE_ACTIVATED: u32 = 2;
// NMDeviceStateReason: no secrets means the stored or typed password was refused.
const REASON_NO_SECRETS: u32 = 7;
// NM80211ApFlags and NM80211ApSecurityFlags.
const AP_PRIVACY: u32 = 0x1;
const KEY_PSK: u32 = 0x100;
const KEY_802_1X: u32 = 0x200;
const KEY_SAE: u32 = 0x400;
const KEY_OWE: u32 = 0x800 | 0x1000;
// GNOME Shell's strength buckets; below 40 % is weak, from 80 % excellent.
const WEAK: u8 = 40;
const EXCELLENT: u8 = 80;

type Objects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;
type ProfileSettings = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Security {
    Open,
    Encrypted,
    Personal,
    Enterprise,
}

impl Security {
    fn from_flags(flags: u32, wpa: u32, rsn: u32) -> Self {
        let keys = wpa | rsn;
        if keys & KEY_802_1X != 0 {
            Self::Enterprise
        } else if keys & (KEY_PSK | KEY_SAE) != 0 || flags & AP_PRIVACY != 0 && keys == 0 {
            Self::Personal
        } else if keys & KEY_OWE != 0 {
            Self::Encrypted
        } else {
            Self::Open
        }
    }

    pub fn locked(self) -> bool {
        matches!(self, Self::Personal | Self::Enterprise)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    Connected,
    Connecting,
    Disconnected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateAddress {
    Off,
    Fixed,
    Rotating,
}

impl PrivateAddress {
    // `assigned-mac-address` values from nm-settings-dbus(5).
    fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("stable") => Self::Fixed,
            Some("random") => Self::Rotating,
            _ => Self::Off,
        }
    }

    fn setting(self) -> &'static str {
        match self {
            Self::Off => "permanent",
            Self::Fixed => "stable",
            Self::Rotating => "random",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WifiOptions {
    pub autoconnect: bool,
    pub low_data: bool,
    pub private_address: PrivateAddress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Wpa2,
    Wpa3,
    Wpa2Wpa3,
}

impl Protocol {
    fn from_keys(keys: u32) -> Self {
        match (keys & KEY_PSK != 0, keys & KEY_SAE != 0) {
            (true, true) => Self::Wpa2Wpa3,
            (false, true) => Self::Wpa3,
            _ => Self::Wpa2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WifiNetwork {
    pub name: String,
    pub bars: u8,
    pub security: Security,
    pub protocol: Protocol,
    pub options: Option<WifiOptions>,
    pub access_point: OwnedObjectPath,
    pub profile: Option<OwnedObjectPath>,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Wifi {
    pub enabled: bool,
    pub switchable: bool,
    pub state: LinkState,
    pub wrong_password: bool,
    pub hardware_address: String,
    pub address: Option<String>,
    pub router: Option<String>,
    pub networks: Vec<WifiNetwork>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Ethernet,
    Modem,
    Bond,
    Vlan,
    Bridge,
}

impl LinkKind {
    fn from_device_type(kind: u32) -> Option<Self> {
        match kind {
            ETHERNET => Some(Self::Ethernet),
            MODEM => Some(Self::Modem),
            BOND => Some(Self::Bond),
            VLAN => Some(Self::Vlan),
            BRIDGE => Some(Self::Bridge),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub kind: LinkKind,
    pub interface: String,
    pub state: LinkState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vpn {
    pub name: String,
    pub kind: String,
    pub profile: OwnedObjectPath,
    pub active: Option<OwnedObjectPath>,
    pub state: LinkState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    pub wifi: Option<Wifi>,
    pub links: Vec<Link>,
    pub vpns: Vec<Vpn>,
}

pub enum NetworkCommand {
    Wifi(bool),
    Scanning(bool),
    Scan,
    Join {
        access_point: OwnedObjectPath,
        password: Option<Zeroizing<String>>,
    },
    Activate(OwnedObjectPath),
    Deactivate(OwnedObjectPath),
    Forget(OwnedObjectPath),
    Import(PathBuf),
    Configure(OwnedObjectPath, WifiOptions),
    Password(OwnedObjectPath, async_channel::Sender<Zeroizing<String>>),
}

#[derive(Clone)]
struct Profile {
    name: String,
    kind: String,
    ssid: Option<Vec<u8>>,
    vpn_service: Option<String>,
    options: Option<WifiOptions>,
}

#[derive(Default)]
pub struct NetworkManager {
    profiles: HashMap<OwnedObjectPath, Profile>,
    wifi_device: Option<OwnedObjectPath>,
    scanning: bool,
}

struct AccessPoint {
    ssid: Vec<u8>,
    strength: u8,
    security: Security,
    protocol: Protocol,
    path: OwnedObjectPath,
}

impl Service for NetworkManager {
    const NAME: &'static str = NM;
    type Snapshot = Network;
    type Command = NetworkCommand;

    fn read(&mut self, bus: &Connection) -> zbus::Result<Network> {
        let objects: Objects = bus
            .call_method(
                Some(NM),
                OBJECTS_PATH,
                Some(OBJECT_MANAGER),
                "GetManagedObjects",
                &(),
            )?
            .body()
            .deserialize()?;
        self.refresh_profiles(bus, &objects);
        let active = active_connections(&objects);
        let devices = devices(&objects);
        self.wifi_device = devices
            .iter()
            .find(|(_, device)| property(device, "DeviceType") == Some(WIFI))
            .map(|(path, _)| (*path).clone());
        let wifi = self
            .wifi_device
            .as_ref()
            .map(|path| self.wifi(&objects, path));
        let links = devices
            .iter()
            .filter_map(|(_, device)| {
                let kind = LinkKind::from_device_type(property(device, "DeviceType")?)?;
                Some(Link {
                    kind,
                    interface: property(device, "Interface").unwrap_or_default(),
                    state: device_state(property(device, "State").unwrap_or(0)),
                })
            })
            .collect();
        let mut vpns: Vec<Vpn> = self
            .profiles
            .iter()
            .filter(|(_, profile)| VPN_PROFILES.contains(&profile.kind.as_str()))
            .map(|(path, profile)| {
                let (active, state) = match active.get(path) {
                    Some((active, state)) => (Some(active.clone()), active_state(*state)),
                    None => (None, LinkState::Disconnected),
                };
                Vpn {
                    name: profile.name.clone(),
                    kind: vpn_kind(profile),
                    profile: path.clone(),
                    active,
                    state,
                }
            })
            .collect();
        vpns.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Network { wifi, links, vpns })
    }

    fn run(&mut self, bus: &Connection, command: NetworkCommand) -> zbus::Result<()> {
        match command {
            NetworkCommand::Wifi(on) => {
                set(bus, NM, NM_PATH, NM, "WirelessEnabled", Value::from(on))
            }
            NetworkCommand::Scan => {
                self.scan(bus);
                Ok(())
            }
            NetworkCommand::Scanning(on) => {
                self.scanning = on;
                if on {
                    self.scan(bus);
                }
                Ok(())
            }
            NetworkCommand::Join {
                access_point,
                password,
            } => self.join(bus, access_point, password),
            NetworkCommand::Activate(profile) => {
                let device = match self.profiles.get(&profile) {
                    Some(found) if found.kind == WIFI_PROFILE => self.wifi_device.clone(),
                    _ => None,
                };
                let root = ObjectPath::from_static_str_unchecked("/");
                let device = device
                    .as_ref()
                    .map(|path| path.as_ref())
                    .unwrap_or(root.clone());
                bus.call_method(
                    Some(NM),
                    NM_PATH,
                    Some(NM),
                    "ActivateConnection",
                    &(profile.as_ref(), device, root),
                )?;
                Ok(())
            }
            NetworkCommand::Deactivate(active) => {
                bus.call_method(
                    Some(NM),
                    NM_PATH,
                    Some(NM),
                    "DeactivateConnection",
                    &active.as_ref(),
                )?;
                Ok(())
            }
            NetworkCommand::Import(file) => import(&file),
            NetworkCommand::Configure(profile, options) => configure(bus, &profile, options),
            NetworkCommand::Password(profile, reply) => {
                let _ = reply.send_blocking(password(bus, &profile)?);
                Ok(())
            }
            NetworkCommand::Forget(profile) => {
                bus.call_method(Some(NM), &profile, Some(PROFILE), "Delete", &())?;
                self.profiles.remove(&profile);
                Ok(())
            }
        }
    }

    fn tick(&mut self, bus: &Connection) {
        if self.scanning {
            self.scan(bus);
        }
    }
}

impl NetworkManager {
    // Profiles are read again on every change: Update2 from any client leaves no other trace.
    fn refresh_profiles(&mut self, bus: &Connection, objects: &Objects) {
        self.profiles.clear();
        for path in objects.keys() {
            if !has_interface(objects, path, PROFILE) {
                continue;
            }
            match profile(bus, path) {
                Ok(found) => {
                    self.profiles.insert(path.clone(), found);
                }
                Err(error) => tracing::debug!(%error, %path, "network profile unreadable"),
            }
        }
    }

    fn wifi(&self, objects: &Objects, device_path: &OwnedObjectPath) -> Wifi {
        let manager = interface(objects, NM_PATH, NM);
        let device = interface(objects, device_path.as_str(), DEVICE);
        let wireless = interface(objects, device_path.as_str(), WIRELESS);
        let state = device.and_then(|d| property(d, "State")).unwrap_or(0);
        let reason = device
            .and_then(|d| property::<(u32, u32)>(d, "StateReason"))
            .map(|(_, reason)| reason);
        let current: Option<OwnedObjectPath> = wireless
            .and_then(|w| property(w, "ActiveAccessPoint"))
            .filter(|path: &OwnedObjectPath| path.as_str() != "/");
        let access_points: Vec<AccessPoint> = wireless
            .and_then(|w| property::<Vec<OwnedObjectPath>>(w, "AccessPoints"))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| access_point(objects, path))
            .collect();
        let known: HashMap<Vec<u8>, (OwnedObjectPath, Option<WifiOptions>)> = self
            .profiles
            .iter()
            .filter_map(|(path, profile)| {
                Some((profile.ssid.clone()?, (path.clone(), profile.options)))
            })
            .collect();
        let ip4 = device
            .and_then(|d| property::<OwnedObjectPath>(d, "Ip4Config"))
            .and_then(|path| interface(objects, path.as_str(), IP4_CONFIG));
        let connected = state == DEVICE_ACTIVATED;
        Wifi {
            enabled: manager
                .and_then(|m| property(m, "WirelessEnabled"))
                .unwrap_or(false),
            switchable: manager
                .and_then(|m| property(m, "WirelessHardwareEnabled"))
                .unwrap_or(false),
            state: device_state(state),
            wrong_password: matches!(state, DEVICE_FAILED | DEVICE_DISCONNECTED)
                && reason == Some(REASON_NO_SECRETS),
            // NetworkManager before 1.24 publishes the address only on Device.Wireless.
            hardware_address: device
                .and_then(|d| property(d, "HwAddress"))
                .or_else(|| wireless.and_then(|w| property(w, "HwAddress")))
                .unwrap_or_default(),
            address: ip4
                .filter(|_| connected)
                .and_then(|ip4| property::<Vec<Properties>>(ip4, "AddressData"))
                .and_then(|addresses| property(addresses.first()?, "address")),
            router: ip4
                .filter(|_| connected)
                .and_then(|ip4| property::<String>(ip4, "Gateway"))
                .filter(|gateway| !gateway.is_empty()),
            networks: networks(access_points, &known, current.as_ref()),
        }
    }

    fn scan(&self, bus: &Connection) {
        let Some(device) = &self.wifi_device else {
            return;
        };
        let options: HashMap<&str, Value> = HashMap::new();
        if let Err(error) =
            bus.call_method(Some(NM), device, Some(WIRELESS), "RequestScan", &options)
        {
            tracing::debug!(%error, "Wi-Fi scan refused");
        }
    }

    fn join(
        &self,
        bus: &Connection,
        access_point: OwnedObjectPath,
        password: Option<Zeroizing<String>>,
    ) -> zbus::Result<()> {
        let Some(device) = &self.wifi_device else {
            return Ok(());
        };
        let mut settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();
        if let Some(password) = &password {
            settings.insert(
                "802-11-wireless-security",
                HashMap::from([("psk", Value::from(password.as_str()))]),
            );
        }
        let options: HashMap<&str, Value> = HashMap::new();
        bus.call_method(
            Some(NM),
            NM_PATH,
            Some(NM),
            "AddAndActivateConnection2",
            &(settings, device.as_ref(), access_point.as_ref(), options),
        )?;
        Ok(())
    }
}

// D-Bus has no import call; each VPN plugin parses its own format through nmcli.
fn import(file: &Path) -> zbus::Result<()> {
    let kind = match file.extension().and_then(|extension| extension.to_str()) {
        Some("conf") => "wireguard",
        _ => "openvpn",
    };
    let output = Command::new("nmcli")
        .args(["connection", "import", "type", kind, "file"])
        .arg(file)
        .output()
        .map_err(|error| zbus::Error::Failure(error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(zbus::Error::Failure(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

fn interface<'a>(objects: &'a Objects, path: &str, name: &str) -> Option<&'a Properties> {
    objects
        .iter()
        .find(|(object, _)| object.as_str() == path)
        .and_then(|(_, interfaces)| interfaces.get(name))
}

fn has_interface(objects: &Objects, path: &OwnedObjectPath, name: &str) -> bool {
    objects
        .get(path)
        .is_some_and(|interfaces| interfaces.contains_key(name))
}

fn devices(objects: &Objects) -> Vec<(&OwnedObjectPath, &Properties)> {
    let mut devices: Vec<_> = objects
        .iter()
        .filter_map(|(path, interfaces)| Some((path, interfaces.get(DEVICE)?)))
        .filter(|(_, device)| property(device, "Managed") == Some(true))
        .collect();
    devices.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    devices
}

fn active_connections(objects: &Objects) -> HashMap<OwnedObjectPath, (OwnedObjectPath, u32)> {
    objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let active = interfaces.get(ACTIVE)?;
            let profile = property(active, "Connection")?;
            Some((
                profile,
                (path.clone(), property(active, "State").unwrap_or(0)),
            ))
        })
        .collect()
}

fn access_point(objects: &Objects, path: OwnedObjectPath) -> Option<AccessPoint> {
    let ap = interface(objects, path.as_str(), ACCESS_POINT)?;
    Some(AccessPoint {
        ssid: property(ap, "Ssid")?,
        strength: property(ap, "Strength").unwrap_or(0),
        security: Security::from_flags(
            property(ap, "Flags").unwrap_or(0),
            property(ap, "WpaFlags").unwrap_or(0),
            property(ap, "RsnFlags").unwrap_or(0),
        ),
        protocol: Protocol::from_keys(
            property::<u32>(ap, "WpaFlags").unwrap_or(0)
                | property::<u32>(ap, "RsnFlags").unwrap_or(0),
        ),
        path,
    })
}

fn profile(bus: &Connection, path: &OwnedObjectPath) -> zbus::Result<Profile> {
    let settings: ProfileSettings = bus
        .call_method(Some(NM), path, Some(PROFILE), "GetSettings", &())?
        .body()
        .deserialize()?;
    let section = |name: &str| settings.get(name);
    let connection = section("connection");
    Ok(Profile {
        name: connection
            .and_then(|c| property(c, "id"))
            .unwrap_or_default(),
        kind: connection
            .and_then(|c| property(c, "type"))
            .unwrap_or_default(),
        ssid: section(WIFI_PROFILE).and_then(|w| property(w, "ssid")),
        vpn_service: section("vpn").and_then(|v| property(v, "service-type")),
        options: section(WIFI_PROFILE).map(|wifi| WifiOptions {
            autoconnect: connection
                .and_then(|c| property(c, "autoconnect"))
                .unwrap_or(true),
            low_data: connection.and_then(|c| property::<i32>(c, "metered")) == Some(METERED_YES),
            private_address: PrivateAddress::from_setting(
                property::<String>(wifi, "assigned-mac-address").as_deref(),
            ),
        }),
    })
}

fn settings_with_secrets(
    bus: &Connection,
    path: &OwnedObjectPath,
) -> zbus::Result<ProfileSettings> {
    let mut settings: ProfileSettings = bus
        .call_method(Some(NM), path, Some(PROFILE), "GetSettings", &())?
        .body()
        .deserialize()?;
    if settings.contains_key(WIFI_SECURITY) {
        let secrets: ProfileSettings = bus
            .call_method(Some(NM), path, Some(PROFILE), "GetSecrets", &WIFI_SECURITY)?
            .body()
            .deserialize()?;
        for (section, values) in secrets {
            settings.entry(section).or_default().extend(values);
        }
    }
    Ok(settings)
}

// Update2 replaces the whole profile, so it carries the stored secrets back.
fn configure(bus: &Connection, path: &OwnedObjectPath, options: WifiOptions) -> zbus::Result<()> {
    let mut settings = settings_with_secrets(bus, path)?;
    let owned = |value: Value| OwnedValue::try_from(value).map_err(zbus::Error::from);
    let connection = settings.entry("connection".to_string()).or_default();
    connection.insert(
        "autoconnect".to_string(),
        owned(Value::from(options.autoconnect))?,
    );
    let metered = if options.low_data {
        METERED_YES
    } else {
        METERED_UNKNOWN
    };
    connection.insert("metered".to_string(), owned(Value::from(metered))?);
    let wifi = settings.entry(WIFI_PROFILE.to_string()).or_default();
    let current =
        PrivateAddress::from_setting(property::<String>(wifi, "assigned-mac-address").as_deref());
    if current != options.private_address {
        wifi.remove("cloned-mac-address");
        wifi.insert(
            "assigned-mac-address".to_string(),
            owned(Value::from(options.private_address.setting()))?,
        );
    }
    let arguments: HashMap<&str, Value> = HashMap::new();
    bus.call_method(
        Some(NM),
        path,
        Some(PROFILE),
        "Update2",
        &(settings, UPDATE_TO_DISK, arguments),
    )?;
    Ok(())
}

fn password(bus: &Connection, path: &OwnedObjectPath) -> zbus::Result<Zeroizing<String>> {
    let secrets: ProfileSettings = bus
        .call_method(Some(NM), path, Some(PROFILE), "GetSecrets", &WIFI_SECURITY)?
        .body()
        .deserialize()?;
    Ok(Zeroizing::new(
        secrets
            .get(WIFI_SECURITY)
            .and_then(|security| property(security, "psk"))
            .unwrap_or_default(),
    ))
}

fn vpn_kind(profile: &Profile) -> String {
    let service = profile.vpn_service.as_deref().unwrap_or(&profile.kind);
    let plugin = service.rsplit('.').next().unwrap_or(service);
    match plugin {
        "wireguard" => "WireGuard",
        "openvpn" => "OpenVPN",
        "strongswan" => "IKEv2",
        "l2tp" => "L2TP",
        "openconnect" => "OpenConnect",
        "vpnc" => "Cisco IPsec",
        other => other,
    }
    .to_string()
}

fn device_state(state: u32) -> LinkState {
    match state {
        DEVICE_ACTIVATED => LinkState::Connected,
        DEVICE_PREPARE..DEVICE_ACTIVATED => LinkState::Connecting,
        _ => LinkState::Disconnected,
    }
}

fn active_state(state: u32) -> LinkState {
    match state {
        ACTIVE_ACTIVATED => LinkState::Connected,
        ACTIVE_ACTIVATING => LinkState::Connecting,
        _ => LinkState::Disconnected,
    }
}

fn bars(strength: u8) -> u8 {
    match strength {
        0..WEAK => 1,
        WEAK..EXCELLENT => 2,
        _ => 3,
    }
}

fn networks(
    access_points: Vec<AccessPoint>,
    known: &HashMap<Vec<u8>, (OwnedObjectPath, Option<WifiOptions>)>,
    current: Option<&OwnedObjectPath>,
) -> Vec<WifiNetwork> {
    let mut grouped: HashMap<(Vec<u8>, Security), (AccessPoint, bool)> = HashMap::new();
    for ap in access_points {
        if ap.ssid.is_empty() {
            continue;
        }
        let is_current = current == Some(&ap.path);
        let key = (ap.ssid.clone(), ap.security);
        match grouped.get_mut(&key) {
            Some((strongest, was_current)) => {
                *was_current |= is_current;
                if ap.strength > strongest.strength {
                    *strongest = ap;
                }
            }
            None => {
                grouped.insert(key, (ap, is_current));
            }
        }
    }
    let mut networks: Vec<WifiNetwork> = grouped
        .into_values()
        .map(|(ap, current)| WifiNetwork {
            name: String::from_utf8_lossy(&ap.ssid).into_owned(),
            bars: bars(ap.strength),
            security: ap.security,
            protocol: ap.protocol,
            profile: known.get(&ap.ssid).map(|(path, _)| path.clone()),
            options: known.get(&ap.ssid).and_then(|(_, options)| *options),
            access_point: ap.path,
            current,
        })
        .collect();
    networks.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then(b.bars.cmp(&a.bars))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    networks
}

const ARCS: [&str; 3] = [
    "M6.5 11.4a2.2 2.2 0 0 1 3 0",
    "M4.3 9a5.5 5.5 0 0 1 7.4 0",
    "M2 6.5a9 9 0 0 1 12 0",
];

pub fn signal_svg(bars: u8, on: Rgba, off: Rgba) -> String {
    let arcs: String = ARCS
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let [r, g, b, a] = if usize::from(bars) > index { on } else { off }.to_rgba8();
            let alpha = f32::from(a) / 255.0;
            format!(
                r#"<path d="{path}" fill="none" stroke="rgba({r},{g},{b},{alpha})" stroke-width="1.6" stroke-linecap="round"/>"#
            )
        })
        .collect();
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">{arcs}</svg>"#)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ap(ssid: &str, strength: u8, security: Security, path: &str) -> AccessPoint {
        AccessPoint {
            ssid: ssid.as_bytes().to_vec(),
            strength,
            security,
            protocol: Protocol::Wpa2,
            path: OwnedObjectPath::try_from(path).unwrap(),
        }
    }

    #[test]
    fn access_points_with_the_same_name_and_security_collapse_into_the_strongest() {
        let found = networks(
            vec![
                ap("Casa", 30, Security::Personal, "/ap/1"),
                ap("Casa", 90, Security::Personal, "/ap/2"),
                ap("Casa", 60, Security::Open, "/ap/3"),
            ],
            &HashMap::new(),
            None,
        );
        assert_eq!(found.len(), 2);
        let personal = found
            .iter()
            .find(|network| network.security == Security::Personal)
            .unwrap();
        assert_eq!(personal.access_point.as_str(), "/ap/2");
        assert_eq!(personal.bars, 3);
    }

    #[test]
    fn hidden_access_points_are_skipped() {
        let found = networks(
            vec![ap("", 90, Security::Personal, "/ap/1")],
            &HashMap::new(),
            None,
        );
        assert!(found.is_empty());
    }

    #[test]
    fn the_current_network_stays_first_even_if_another_signal_is_stronger() {
        let current = OwnedObjectPath::try_from("/ap/1").unwrap();
        let found = networks(
            vec![
                ap("Lejos", 20, Security::Personal, "/ap/1"),
                ap("Cerca", 95, Security::Personal, "/ap/2"),
            ],
            &HashMap::new(),
            Some(&current),
        );
        assert_eq!(found[0].name, "Lejos");
        assert!(found[0].current);
    }

    #[test]
    fn owe_networks_are_encrypted_without_a_password_and_wpa3_needs_one() {
        assert_eq!(Security::from_flags(0, 0, 0x800), Security::Encrypted);
        assert!(!Security::Encrypted.locked());
        assert_eq!(Security::from_flags(1, 0, KEY_SAE), Security::Personal);
        assert_eq!(Security::from_flags(1, 0, KEY_802_1X), Security::Enterprise);
        assert_eq!(Security::from_flags(1, 0, 0), Security::Personal);
    }
}
