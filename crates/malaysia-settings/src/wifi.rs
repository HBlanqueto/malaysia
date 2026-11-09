use bk::password::PasswordField;
use bk::patterns::settings::Pane;
use bk::prelude::*;
use bk::views::feedback::Sheet;
use bk::views::form::{Form, Row, Section};
use ctappearance::palette;
use ctdesign::{ACCENT_SWATCHES, Rgba, SPACE_2};
use freya_components::svg_viewer::SvgViewer;
use zbus::zvariant::OwnedObjectPath;

use crate::bus::{Live, Remote, WhileVisible};
use crate::localize::{intern, tr};
use crate::networkmanager::{LinkState, NetworkCommand, NetworkManager, Security, WifiNetwork};

pub const TINT: Rgba = ACCENT_SWATCHES[0].1;
const SIGNAL_SIZE: f32 = 16.0;
const ARCS: [&str; 3] = [
    "M6.5 11.4a2.2 2.2 0 0 1 3 0",
    "M4.3 9a5.5 5.5 0 0 1 7.4 0",
    "M2 6.5a9 9 0 0 1 12 0",
];

#[derive(Clone, PartialEq)]
pub struct Joining {
    name: String,
    access_point: OwnedObjectPath,
}

pub fn wifi_pane(network: &Live<NetworkManager>, joining: State<Option<Joining>>) -> Option<Pane> {
    let wifi = network.snapshot.read().clone()?.wifi?;
    let remote = network.remote.clone();
    let scanning = network.remote.clone();
    let switch = Switch::new(tr!("wifi"))
        .on(wifi.enabled)
        .disabled(!wifi.switchable)
        .on_toggle(move |on| remote.send(NetworkCommand::Wifi(on)));
    let about = if wifi.wrong_password {
        tr!("wifi-wrong-password")
    } else {
        tr!("wifi-about")
    };
    let mut header = Section::new().row(
        Row::new(tr!("wifi"))
            .symbol(Symbol::Wifi, TINT)
            .detail(about)
            .control(WhileVisible::new(switch, move |shown| {
                scanning.send(NetworkCommand::Scanning(shown))
            })),
    );
    let pane = |form: Form| Pane::new("wifi", tr!("wifi"), Symbol::Wifi, TINT, form);
    if !wifi.enabled {
        return Some(pane(Form::new().section(header)));
    }
    if let Some(current) = wifi.networks.iter().find(|found| found.current) {
        let status = match wifi.state {
            LinkState::Connected => tr!("status-connected"),
            _ => tr!("status-connecting"),
        };
        header = header.row(
            Row::new(intern(current.name.clone()))
                .detail(status)
                .control(signal(current)),
        );
    }
    let mut form = Form::new().section(header);
    let known: Vec<&WifiNetwork> = wifi
        .networks
        .iter()
        .filter(|found| found.profile.is_some() && !found.current)
        .collect();
    if !known.is_empty() {
        form = form.section(
            known
                .into_iter()
                .fold(Section::titled(tr!("wifi-known")), |section, found| {
                    section.row(known_row(found, &network.remote))
                }),
        );
    }
    let others = wifi
        .networks
        .iter()
        .filter(|found| found.profile.is_none() && !found.current)
        .fold(Section::titled(tr!("wifi-other")), |section, found| {
            section.row(other_row(found, &network.remote, joining, network.failure))
        });
    Some(pane(form.section(others)))
}

fn known_row(found: &WifiNetwork, remote: &Remote<NetworkCommand>) -> Row {
    let profile = found.profile.clone();
    let join = remote.clone();
    let forget = remote.clone();
    let forget_profile = profile.clone();
    Row::navigation(intern(found.name.clone()), move |_| {
        if let Some(profile) = &profile {
            join.send(NetworkCommand::Activate(profile.clone()));
        }
    })
    .control(HStack::new().spacing(SPACE_2).child(signal(found)).child(
        Button::new(tr!("wifi-forget")).on_press(move |_| {
            if let Some(profile) = &forget_profile {
                forget.send(NetworkCommand::Forget(profile.clone()));
            }
        }),
    ))
}

fn other_row(
    found: &WifiNetwork,
    remote: &Remote<NetworkCommand>,
    mut joining: State<Option<Joining>>,
    mut failure: State<Option<String>>,
) -> Row {
    let remote = remote.clone();
    let target = Joining {
        name: found.name.clone(),
        access_point: found.access_point.clone(),
    };
    let security = found.security;
    Row::navigation(intern(found.name.clone()), move |_| match security {
        Security::Open | Security::Encrypted => remote.send(NetworkCommand::Join {
            access_point: target.access_point.clone(),
            password: None,
        }),
        Security::Personal => joining.set(Some(target.clone())),
        Security::Enterprise => failure.set(Some(tr!("wifi-enterprise").to_string())),
    })
    .control(signal(found))
}

fn signal(found: &WifiNetwork) -> HStack {
    let colors = palette(bk::theme::resolved().scheme);
    let stack = HStack::new().spacing(SPACE_2);
    let stack = if found.security.locked() {
        stack.child(
            Image::system(Symbol::Lock)
                .size(SIGNAL_SIZE)
                .color(colors.ink_muted),
        )
    } else {
        stack
    };
    let svg = signal_svg(found.bars, colors.ink_strong, colors.ink_faint);
    stack.child(
        SvgViewer::new((intern(format!("wifi-signal-{svg}")), Bytes::from(svg)))
            .width(Size::px(SIGNAL_SIZE))
            .height(Size::px(SIGNAL_SIZE))
            .show_loader(false),
    )
}

fn signal_svg(bars: u8, on: Rgba, off: Rgba) -> String {
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

pub fn password_sheet(
    mut joining: State<Option<Joining>>,
    password: State<PasswordField>,
    remote: &Remote<NetworkCommand>,
) -> Option<Sheet> {
    let target = joining.read().clone()?;
    let submit = {
        let remote = remote.clone();
        let target = target.clone();
        move |typed: zeroize::Zeroizing<String>| {
            remote.send(NetworkCommand::Join {
                access_point: target.access_point.clone(),
                password: Some(typed),
            });
            joining.set(None);
        }
    };
    let mut join = submit.clone();
    let mut field = password;
    Some(Sheet::new(
        VStack::new()
            .spacing(SPACE_2)
            .child(
                Text::new(tr!("wifi-password-title", network = target.name.as_str()))
                    .font(HEADLINE),
            )
            .child(Text::new(tr!("wifi-password-help")))
            .child(
                SecureField::new(password, tr!("wifi-password"))
                    .auto_focus(true)
                    .on_submit(submit),
            )
            .child(
                HStack::new()
                    .spacing(SPACE_2)
                    .child(Button::new(tr!("cancel")).on_press(move |_| joining.set(None)))
                    .child(Button::new(tr!("wifi-join")).on_press(move |_| {
                        if let Some(typed) = field.write().take() {
                            join(typed);
                        }
                    })),
            ),
    ))
}
