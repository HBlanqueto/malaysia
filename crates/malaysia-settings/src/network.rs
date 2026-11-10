use bk::patterns::settings::Pane;
use bk::prelude::*;
use bk::views::form::{Form, Row, Section};

use crate::localize::{intern, tr};
use crate::menubar::show_in_menu_bar;
use crate::wifi::TINT;
use malaysia_services::bus::Live;
use malaysia_services::networkmanager::{
    ExternalVpn, LinkKind, LinkState, NetworkCommand, NetworkManager, Vpn,
};

pub fn network_pane(network: &Live<NetworkManager>) -> Option<Pane> {
    let snapshot = network.snapshot.read().clone()?;
    let mut form = Form::new();
    if let Some(wifi) = &snapshot.wifi {
        let status = if wifi.enabled {
            status(wifi.state)
        } else {
            tr!("status-off")
        };
        form = form.section(
            Section::new().row(
                Row::page(tr!("wifi"), "wifi")
                    .symbol(Symbol::Wifi, TINT)
                    .detail(status),
            ),
        );
    }
    if !snapshot.links.is_empty() {
        form = form.section(snapshot.links.iter().fold(
            Section::titled(tr!("network-services")),
            |section, link| {
                let title = intern(format!("{} ({})", kind_name(link.kind), link.interface));
                section.row(
                    Row::new(title)
                        .symbol(Symbol::Network, TINT)
                        .detail(status(link.state)),
                )
            },
        ));
    }
    Some(Pane::new(
        "network",
        tr!("network"),
        Symbol::Network,
        TINT,
        form,
    ))
}

pub fn vpn_pane(network: &Live<NetworkManager>, error: State<Option<String>>) -> Option<Pane> {
    let snapshot = network.snapshot.read().clone()?;
    let list = if snapshot.vpns.is_empty() {
        Section::titled(tr!("vpn-configurations")).row(Row::new(tr!("vpn-none")))
    } else {
        snapshot.vpns.iter().fold(
            Section::titled(tr!("vpn-configurations")),
            |section, vpn| section.row(vpn_row(vpn, network)),
        )
    };
    let remote = network.remote.clone();
    let openvpn = snapshot.openvpn;
    let mut failure = network.failure;
    let import = Section::new().row(
        Row::navigation(tr!("vpn-import"), move |_| {
            let remote = remote.clone();
            spawn(async move {
                let Some(file) = bk::portal::open_files(tr!("vpn-import"), false, false)
                    .await
                    .and_then(|files| files.into_iter().next())
                else {
                    return;
                };
                if openvpn
                    || file
                        .extension()
                        .is_some_and(|extension| extension == "conf")
                {
                    remote.send(NetworkCommand::Import(file));
                } else {
                    failure.set(Some(tr!("vpn-openvpn-missing").to_string()));
                }
            });
        })
        .detail(if openvpn {
            tr!("vpn-import-help")
        } else {
            tr!("vpn-import-help-wireguard")
        }),
    );
    let mut form = Form::new()
        .section(Section::new().row(show_in_menu_bar(
            tr!("show-in-menu-bar"),
            |config| &mut config.show_vpn,
            error,
        )))
        .section(list)
        .section(import);
    if !snapshot.external_vpns.is_empty() {
        form = form.section(
            snapshot
                .external_vpns
                .iter()
                .fold(Section::titled(tr!("vpn-external")), |section, vpn| {
                    section.row(external_row(vpn))
                })
                .footer(tr!("vpn-external-help")),
        );
    }
    Some(Pane::new("vpn", tr!("vpn"), Symbol::Lock, TINT, form))
}

fn vpn_row(vpn: &Vpn, network: &Live<NetworkManager>) -> Row {
    let title = intern(vpn.name.clone());
    let detail = if vpn.available {
        intern(format!("{} · {}", vpn.kind, status(vpn.state)))
    } else {
        intern(tr!("vpn-plugin-missing", kind = vpn.kind.clone()))
    };
    let remote = network.remote.clone();
    let profile = vpn.profile.clone();
    let active = vpn.active.clone();
    Row::new(title)
        .symbol(Symbol::Lock, TINT)
        .detail(detail)
        .control(
            Switch::new(title)
                .on(vpn.state != LinkState::Disconnected)
                .disabled(!vpn.available)
                .on_toggle(move |on| match (&active, on) {
                    (_, true) => remote.send(NetworkCommand::Activate(profile.clone())),
                    (Some(active), false) => {
                        remote.send(NetworkCommand::Deactivate(active.clone()))
                    }
                    (None, false) => {}
                }),
        )
}

fn external_row(vpn: &ExternalVpn) -> Row {
    let kind = if vpn.wireguard {
        "WireGuard"
    } else {
        tr!("vpn-tunnel")
    };
    let place = match &vpn.address {
        Some(address) => format!("{} · {address}", vpn.interface),
        None => vpn.interface.clone(),
    };
    let row = Row::new(intern(vpn.name.clone()))
        .symbol(Symbol::Lock, TINT)
        .detail(intern(format!("{kind} · {} · {place}", tr!("vpn-active"))));
    match vpn
        .desktop_entry
        .filter(|entry| crate::apps::installed(entry))
    {
        Some(entry) => row
            .control(Button::new(tr!("vpn-open")).on_press(move |_| crate::apps::open_app(entry))),
        None => row,
    }
}

fn status(state: LinkState) -> &'static str {
    match state {
        LinkState::Connected => tr!("status-connected"),
        LinkState::Connecting => tr!("status-connecting"),
        LinkState::Disconnected => tr!("status-disconnected"),
    }
}

fn kind_name(kind: LinkKind) -> &'static str {
    match kind {
        LinkKind::Ethernet => tr!("link-ethernet"),
        LinkKind::Modem => tr!("link-modem"),
        LinkKind::Bond => tr!("link-bond"),
        LinkKind::Vlan => tr!("link-vlan"),
        LinkKind::Bridge => tr!("link-bridge"),
    }
}
