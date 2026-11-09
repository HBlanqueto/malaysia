# 0069 — Network, Bluetooth and energy settings

Status: accepted (2026-11-09); phase 1 and the menu bar part of phase 2 done.

## Goal and scope

This ADR fixes how System Settings shows and changes Wi-Fi, Bluetooth,
Network, VPN and Energy, modelled on macOS System Settings. It starts at the
D-Bus services and ends at the panes and their sheets. The menu bar
indicators, the secret agents in the shell and the idle timers are later
phases. Readers: whoever works on `malaysia-settings`, the menubar or the
compositor idle code.

## Context

1. macOS puts each topic in its own sidebar pane, lists networks and devices
   in grouped sections, and opens a sheet for passwords and pairing codes
   ([Wi-Fi](https://support.apple.com/guide/mac-help/mh11935/mac),
   [Bluetooth](https://support.apple.com/guide/mac-help/blth1008/mac),
   [Battery](https://support.apple.com/guide/mac-help/mchlfc3b7879/mac)).
2. Linux has one daemon per topic, all on the system bus:
   [NetworkManager](https://networkmanager.dev/docs/api/latest/),
   [BlueZ](https://github.com/bluez/bluez/tree/master/doc),
   [UPower](https://upower.freedesktop.org/docs/) and
   [power-profiles-daemon](https://gitlab.freedesktop.org/upower/power-profiles-daemon).
   NixOS enables NetworkManager and power-profiles-daemon in the dotfiles;
   members of the `networkmanager` group pass its polkit checks.
3. GNOME Settings and COSMIC read the same daemons. COSMIC registers its
   agents while its pages are open; GNOME keeps them in the shell. niri ships
   no settings and relies on external applets.
4. The Rust bindings do not fit: `nmrs` and `bluer` need tokio, COSMIC's
   bindings moved to zbus 6 and `upower_dbus` is on zbus 3.

## Decision

1. **One worker per daemon.** `bus::use_service` opens a private system-bus
   connection in a thread, reads a full snapshot and sends it to the UI. It
   subscribes to the daemon's signals and to `NameOwnerChanged`; any signal
   reads the snapshot again after 150 ms of quiet. Commands from the UI run in
   the same thread, so no D-Bus call blocks the window. Failures reach an
   alert.
2. **Own zbus 5 calls, no binding crate.** Each service module reads with
   `GetManagedObjects` (NetworkManager at `/org/freedesktop`, BlueZ at `/`)
   or `Properties.GetAll` (UPower, power-profiles-daemon).
3. **Wi-Fi** (`networkmanager.rs`, `wifi.rs`):
   1. The switch writes `WirelessEnabled`; a hardware kill switch disables it.
   2. Scanning runs only while the pane is on screen (`WhileVisible`), every
      15 s, as GNOME Shell does; NetworkManager queues extra requests.
   3. Access points group by SSID bytes and security, keeping the strongest.
      Empty SSIDs are hidden. The current network stays first.
   4. A known network activates its profile. An open or OWE network joins
      directly. A WPA/WPA3 network asks for the password in a sheet and joins
      with `AddAndActivateConnection2`; NetworkManager fills in `key-mgmt`
      from the access point. An 802.1X network points to the connection
      editor.
   5. A refused password (`NO_SECRETS`) shows under the switch.
4. **Network** lists the Wi-Fi service and the Ethernet, modem, bond, VLAN
   and bridge devices with their state.
5. **VPN** lists `vpn` and `wireguard` profiles with a connect switch.
   Import runs `nmcli connection import` for `.conf` (WireGuard) and `.ovpn`
   (OpenVPN): import has no D-Bus call and each plugin parses its own format.
6. **Bluetooth** (`bluez.rs`, `bluetooth.rs`):
   1. Off soft-blocks Bluetooth through `/dev/rfkill`, so systemd-rfkill keeps
      it off after a reboot; on unblocks it and sets `Powered`. Without
      access to `/dev/rfkill`, `Powered` alone is used.
   2. While the pane is on screen the adapter is discoverable and discovers.
   3. My Devices are paired devices with connect, disconnect and forget, and
      the `Battery1` level. Nearby Devices hides devices whose name is only
      their address.
   4. Settings registers an `org.bluez.Agent1` (`KeyboardDisplay`) as the
      default agent. It shows the code to confirm or to type in a sheet and
      accepts just-works pairing only for the device being paired.
   5. Each device shows a picture by the freedesktop icon name that BlueZ
      derives from its class (`assets/devices/`); macOS uses exact models
      only for Apple products.
7. **Energy** (`energy.rs`): the pane is «Battery» with a battery and
   «Energy» without one. Low Power Mode writes `ActiveProfile`
   (`power-saver` or `balanced`) and hides when power-profiles-daemon does not
   run. Battery health shows `Capacity` (under 75 % is «service
   recommended», per UPower) and `ChargeCycles`. The charge limit calls
   `EnableChargeThreshold` and appears only when `ChargeThresholdSupported`.

## Not possible on Linux

| macOS option | Reason |
|---|---|
| Personal Hotspots and Ask to join hotspots | Instant Hotspot is Apple's private protocol over Bluetooth LE and iCloud |
| Limit IP address tracking | It depends on iCloud Private Relay |
| Start up after a power failure | A firmware setting with no standard interface on Linux |
| Power Nap | It needs Apple hardware that wakes in the background |
| Optimized charging that learns a routine | No stable API; UPower's charge strategy is unreleased |
| Network locations | NetworkManager has none; profile priorities cover the case |
| Firewall per app and stealth mode | nftables filters by port, not by app |
| Apps using significant energy | No standard per-app energy measure |

## Consequences

1. **Phase 2:** the menubar shows VPN, Bluetooth, Wi-Fi and Battery menus
   ([`menubar.md`](../architecture/menubar.md#system-status-menus)), read
   through the shared `malaysia-services` crate; each pane of Settings has
   «Show in menu bar» (`menubar.domain`). Still to do: the menubar hosts the
   NetworkManager secret agent and the BlueZ agent, with their dialogs, so
   prompts work with Settings closed; wrong-password retries and VPN one-time
   passwords need it. Until then only Settings registers the BlueZ agent
   (`BluetoothCommand::HostAgent`), and a network that needs a password opens
   Settings.
2. **Phase 3:** idle timers in the compositor (`ext-idle-notify-v1`) for
   display off and sleep, Low Power Mode only on battery and on low battery,
   network details (TCP/IP, DNS, private address, metered), Wake-on-LAN,
   hotspot and usage graphs from UPower `GetHistory` (seven days kept).
3. The firewall is declared by NixOS (`networking.firewall`) and stays out of
   Settings unless firewalld becomes the backend.
4. NixOS needs `networking.networkmanager.plugins` for OpenVPN and other VPN
   types, and `networking.networkmanager.settings.connectivity.uri` for
   captive portal detection.

## Rejected alternatives

- `nmrs` and `bluer`: they pull tokio next to zbus and async-io.
- Polling UPower every minute, as the notification widget does today: signals
  arrive at once and cost nothing while nothing changes.
- An agent only in the shell for phase 1: pairing a keyboard needs a code
  today, and the shell agent is phase 2.
