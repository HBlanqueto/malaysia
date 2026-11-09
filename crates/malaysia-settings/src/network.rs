use bk::patterns::settings::Pane;
use bk::prelude::*;
use bk::views::form::{Form, Row, Section};

use crate::bus::Live;
use crate::localize::{intern, tr};
use crate::networkmanager::{LinkKind, LinkState, NetworkCommand, NetworkManager, Vpn};
use crate::wifi::TINT;

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

pub fn vpn_pane(network: &Live<NetworkManager>) -> Option<Pane> {
    let snapshot = network.snapshot.read().clone()?;
    let list = if snapshot.vpns.is_empty() {
        Section::new().row(Row::new(tr!("vpn-none")))
    } else {
        snapshot.vpns.iter().fold(Section::new(), |section, vpn| {
            section.row(vpn_row(vpn, network))
        })
    };
    let remote = network.remote.clone();
    let import = Section::new().row(
        Row::navigation(tr!("vpn-import"), move |_| {
            let remote = remote.clone();
            spawn(async move {
                if let Some(file) = bk::portal::open_files(tr!("vpn-import"), false, false)
                    .await
                    .and_then(|files| files.into_iter().next())
                {
                    remote.send(NetworkCommand::Import(file));
                }
            });
        })
        .detail(tr!("vpn-import-help")),
    );
    Some(Pane::new(
        "vpn",
        tr!("vpn"),
        Symbol::Lock,
        TINT,
        Form::new().section(list).section(import),
    ))
}

fn vpn_row(vpn: &Vpn, network: &Live<NetworkManager>) -> Row {
    let title = intern(vpn.name.clone());
    let detail = intern(format!("{} · {}", vpn.kind, status(vpn.state)));
    let remote = network.remote.clone();
    let profile = vpn.profile.clone();
    let active = vpn.active.clone();
    Row::new(title)
        .symbol(Symbol::Lock, TINT)
        .detail(detail)
        .control(
            Switch::new(title)
                .on(vpn.state != LinkState::Disconnected)
                .on_toggle(move |on| match (&active, on) {
                    (_, true) => remote.send(NetworkCommand::Activate(profile.clone())),
                    (Some(active), false) => {
                        remote.send(NetworkCommand::Deactivate(active.clone()))
                    }
                    (None, false) => {}
                }),
        )
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
