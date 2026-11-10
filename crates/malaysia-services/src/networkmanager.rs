use serde::{Deserialize, Serialize};
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
const TUN_DEVICE: &str = "org.freedesktop.NetworkManager.Device.Tun";
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
const TUN: u32 = 16;
const WIREGUARD: u32 = 29;
// NMSettingsConnectionFlags and NMActivationStateFlags: a tunnel another program set up (NetworkManager 1.26).
const PROFILE_EXTERNAL: u32 = 0x8;
const ACTIVE_EXTERNAL: u32 = 0x80;
const IFF_UP: u32 = 0x1;
const VPN_PLUGIN_DIRS: [&str; 2] = ["/etc/NetworkManager/VPN", "/usr/lib/NetworkManager/VPN"];
const OPENVPN: &str = "org.freedesktop.NetworkManager.openvpn";
// Interface name fragment, app name and desktop entry of tunnels that apps with their own daemon set up.
const VPN_APPS: [(&str, &str, Option<&str>); 4] = [
    ("tailscale", "Tailscale", None),
    ("mullvad", "Mullvad VPN", Some("mullvad-vpn")),
    ("nordlynx", "NordVPN", None),
    ("cscotun", "Cisco Secure Client", None),
];
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

pub const WIFI_DOMAIN: &str = "wifi";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AskToJoin {
    Off,
    #[default]
    Notify,
    Ask,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WifiConfig {
    pub ask_to_join: AskToJoin,
}

impl Wifi {
    pub fn only_unknown_networks_in_range(&self) -> bool {
        self.enabled
            && self.state == LinkState::Disconnected
            && !self.networks.is_empty()
            && self
                .networks
                .iter()
                .all(|network| network.profile.is_none())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Wpa2,
    Wpa3,
    Wpa2Wpa3,
}

impl Protocol {
    // A WPA2/WPA3 transition network accepts WPA2-PSK, the one key every driver supports.
    fn key_management(self) -> &'static str {
        match self {
            Self::Wpa2 | Self::Wpa2Wpa3 => "wpa-psk",
            Self::Wpa3 => "sae",
        }
    }

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
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalVpn {
    pub name: String,
    pub desktop_entry: Option<&'static str>,
    pub interface: String,
    pub wireguard: bool,
    pub address: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    pub wifi: Option<Wifi>,
    pub links: Vec<Link>,
    pub vpns: Vec<Vpn>,
    pub external_vpns: Vec<ExternalVpn>,
    pub openvpn: bool,
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
    JoinHidden {
        name: String,
        protocol: Option<Protocol>,
        password: Option<Zeroizing<String>>,
    },
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
        let plugins = vpn_plugins();
        let mut vpns: Vec<Vpn> = self
            .profiles
            .iter()
            .filter(|(path, profile)| {
                VPN_PROFILES.contains(&profile.kind.as_str()) && !external_profile(&objects, path)
            })
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
                    available: profile
                        .vpn_service
                        .as_ref()
                        .is_none_or(|service| plugins.contains(service)),
                }
            })
            .collect();
        vpns.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Network {
            wifi,
            links,
            vpns,
            external_vpns: external_vpns(&objects),
            openvpn: plugins.iter().any(|service| service == OPENVPN),
        })
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
            NetworkCommand::JoinHidden {
                name,
                protocol,
                password,
            } => self.join_hidden(bus, &name, protocol, password),
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
            address: ip4.filter(|_| connected).and_then(first_address),
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
        add_and_activate(bus, settings, device, &access_point)
    }

    fn join_hidden(
        &self,
        bus: &Connection,
        name: &str,
        protocol: Option<Protocol>,
        password: Option<Zeroizing<String>>,
    ) -> zbus::Result<()> {
        let Some(device) = &self.wifi_device else {
            return Ok(());
        };
        let mut settings: HashMap<&str, HashMap<&str, Value>> = HashMap::from([(
            WIFI_PROFILE,
            HashMap::from([
                ("ssid", Value::from(name.as_bytes().to_vec())),
                ("hidden", Value::from(true)),
            ]),
        )]);
        if let Some(protocol) = protocol {
            let mut security =
                HashMap::from([("key-mgmt", Value::from(protocol.key_management()))]);
            if let Some(password) = &password {
                security.insert("psk", Value::from(password.as_str()));
            }
            settings.insert(WIFI_SECURITY, security);
        }
        let any = OwnedObjectPath::try_from("/").map_err(zbus::Error::from)?;
        add_and_activate(bus, settings, device, &any)
    }
}

fn add_and_activate(
    bus: &Connection,
    settings: HashMap<&str, HashMap<&str, Value>>,
    device: &OwnedObjectPath,
    access_point: &OwnedObjectPath,
) -> zbus::Result<()> {
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

fn vpn_plugins() -> Vec<String> {
    VPN_PLUGIN_DIRS
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.ok()?.path()).ok())
        .filter_map(|text| {
            text.lines()
                .find_map(|line| line.trim().strip_prefix("service=").map(str::to_string))
        })
        .collect()
}

fn external_profile(objects: &Objects, path: &OwnedObjectPath) -> bool {
    interface(objects, path.as_str(), PROFILE)
        .and_then(|profile| property::<u32>(profile, "Flags"))
        .is_some_and(|flags| flags & PROFILE_EXTERNAL != 0)
}

// Tunnels set up outside NetworkManager: an external activation, or an unmanaged interface that is up.
fn external_vpns(objects: &Objects) -> Vec<ExternalVpn> {
    let mut found: Vec<ExternalVpn> = objects
        .values()
        .filter_map(|interfaces| {
            let device = interfaces.get(DEVICE)?;
            let kind: u32 = property(device, "DeviceType")?;
            let layer3_tunnel = interfaces
                .get(TUN_DEVICE)
                .and_then(|tun| property::<String>(tun, "Mode"))
                .is_some_and(|mode| mode == "tun");
            // TAP devices are virtual machine ports, not VPNs.
            if !(kind == WIREGUARD || kind == TUN && layer3_tunnel) {
                return None;
            }
            let name: String = property(device, "Interface")?;
            let external = if property(device, "Managed") == Some(true) {
                property::<OwnedObjectPath>(device, "ActiveConnection")
                    .and_then(|path| interface(objects, path.as_str(), ACTIVE))
                    .and_then(|active| property::<u32>(active, "StateFlags"))
                    .is_some_and(|flags| flags & ACTIVE_EXTERNAL != 0)
            } else {
                interface_up(&name)
            };
            if !external {
                return None;
            }
            let app = VPN_APPS
                .iter()
                .find(|(fragment, ..)| name.contains(fragment));
            Some(ExternalVpn {
                name: app.map_or_else(|| name.clone(), |(_, app, _)| app.to_string()),
                desktop_entry: app.and_then(|(.., entry)| *entry),
                address: property::<OwnedObjectPath>(device, "Ip4Config")
                    .and_then(|path| interface(objects, path.as_str(), IP4_CONFIG))
                    .and_then(first_address),
                wireguard: kind == WIREGUARD,
                interface: name,
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn interface_up(name: &str) -> bool {
    std::fs::read_to_string(format!("/sys/class/net/{name}/flags"))
        .ok()
        .and_then(|flags| u32::from_str_radix(flags.trim().trim_start_matches("0x"), 16).ok())
        .is_some_and(|flags| flags & IFF_UP != 0)
}

fn first_address(ip4: &Properties) -> Option<String> {
    property(
        property::<Vec<Properties>>(ip4, "AddressData")?.first()?,
        "address",
    )
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

    fn object(interfaces: &[(&str, Vec<(&str, Value<'static>)>)]) -> HashMap<String, Properties> {
        interfaces
            .iter()
            .map(|(name, properties)| {
                let properties = properties
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.try_to_owned().unwrap()))
                    .collect();
                (name.to_string(), properties)
            })
            .collect()
    }

    fn path(text: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(text).unwrap()
    }

    fn tunnel(name: &str, kind: u32, active: &str, mode: &str) -> HashMap<String, Properties> {
        object(&[
            (
                DEVICE,
                vec![
                    ("DeviceType", Value::from(kind)),
                    ("Interface", Value::from(name.to_string())),
                    ("Managed", Value::from(true)),
                    ("ActiveConnection", Value::from(path(active))),
                    ("Ip4Config", Value::from(path("/ip4/1"))),
                ],
            ),
            (TUN_DEVICE, vec![("Mode", Value::from(mode.to_string()))]),
        ])
    }

    #[test]
    fn only_tunnels_another_program_set_up_are_external_vpns() {
        let address: HashMap<String, OwnedValue> = [(
            "address".to_string(),
            Value::from("100.101.4.7".to_string())
                .try_to_owned()
                .unwrap(),
        )]
        .into();
        let objects: Objects = [
            (
                path("/dev/1"),
                tunnel("tailscale0", TUN, "/active/1", "tun"),
            ),
            (path("/dev/2"), tunnel("wg0", WIREGUARD, "/active/2", "")),
            (path("/dev/3"), tunnel("vnet0", TUN, "/active/3", "tap")),
            (path("/dev/4"), tunnel("tun9", TUN, "/active/4", "tun")),
            (
                path("/active/1"),
                object(&[(ACTIVE, vec![("StateFlags", Value::from(ACTIVE_EXTERNAL))])]),
            ),
            (
                path("/active/2"),
                object(&[(ACTIVE, vec![("StateFlags", Value::from(0u32))])]),
            ),
            (
                path("/active/3"),
                object(&[(ACTIVE, vec![("StateFlags", Value::from(ACTIVE_EXTERNAL))])]),
            ),
            (
                path("/active/4"),
                object(&[(ACTIVE, vec![("StateFlags", Value::from(ACTIVE_EXTERNAL))])]),
            ),
            (
                path("/ip4/1"),
                object(&[(
                    IP4_CONFIG,
                    vec![("AddressData", Value::from(vec![address]))],
                )]),
            ),
        ]
        .into();
        let found = external_vpns(&objects);
        let names: Vec<(&str, &str, Option<&str>)> = found
            .iter()
            .map(|vpn| {
                (
                    vpn.name.as_str(),
                    vpn.interface.as_str(),
                    vpn.address.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            names,
            [
                ("Tailscale", "tailscale0", Some("100.101.4.7")),
                ("tun9", "tun9", Some("100.101.4.7")),
            ]
        );
    }

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
