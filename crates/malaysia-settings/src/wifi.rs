use bk::buttons::MenuButton;
use bk::menu::{CheckState, MenuItem};
use bk::password::PasswordField;
use bk::patterns::settings::Pane;
use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::views::feedback::{Alert, Sheet};
use bk::views::form::{Form, Row, Section};
use ctappearance::palette;
use ctdesign::{ACCENT_SWATCHES, Rgba, SPACE_2, SPACE_3};
use freya_components::svg_viewer::SvgViewer;
use torin::prelude::Content;
use zbus::zvariant::OwnedObjectPath;
use zeroize::Zeroizing;

use crate::localize::{intern, tr};
use crate::menubar::show_in_menu_bar;
use malaysia_services::bus::{Live, Remote, WhileVisible};
use malaysia_services::networkmanager::{
    LinkState, NetworkCommand, NetworkManager, PrivateAddress, Protocol, Security, Wifi,
    WifiNetwork, WifiOptions, signal_svg,
};

pub const TINT: Rgba = ACCENT_SWATCHES[0].1;
const SIGNAL_SIZE: f32 = 16.0;
const DIALOG_ICON: f32 = 32.0;
const DIALOG_WIDTH: f32 = 460.0;
const PRIVATE_ADDRESSES: [PrivateAddress; 3] = [
    PrivateAddress::Off,
    PrivateAddress::Fixed,
    PrivateAddress::Rotating,
];

#[derive(Clone, PartialEq)]
pub enum WifiDialog {
    Password {
        name: String,
        access_point: OwnedObjectPath,
        protocol: Protocol,
    },
    Details {
        name: String,
        profile: OwnedObjectPath,
    },
    Forget {
        name: String,
        profile: OwnedObjectPath,
    },
}

pub fn wifi_pane(
    network: &Live<NetworkManager>,
    dialog: State<Option<WifiDialog>>,
    error: State<Option<String>>,
) -> Option<Pane> {
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
    let pane = |form: Form| {
        let form = form.section(Section::new().row(show_in_menu_bar(
            tr!("show-in-menu-bar"),
            |config| &mut config.show_wifi,
            error,
        )));
        Pane::new("wifi", tr!("wifi"), Symbol::Wifi, TINT, form)
    };
    if !wifi.enabled {
        return Some(pane(Form::new().section(header)));
    }
    if let Some(current) = wifi.networks.iter().find(|found| found.current) {
        header = header.row(current_row(current, &wifi, dialog));
    }
    let mut form = Form::new().section(header);
    let (known, other): (Vec<&WifiNetwork>, Vec<&WifiNetwork>) = wifi
        .networks
        .iter()
        .partition(|found| found.profile.is_some());
    if !known.is_empty() {
        form = form.section(
            known
                .into_iter()
                .fold(Section::titled(tr!("wifi-known")), |section, found| {
                    section.row(known_row(found, &network.remote, dialog))
                }),
        );
    }
    if !other.is_empty() {
        form = form.section(
            other
                .into_iter()
                .fold(Section::titled(tr!("wifi-other")), |section, found| {
                    section.row(other_row(found, network, dialog))
                }),
        );
    }
    Some(pane(form))
}

fn current_row(current: &WifiNetwork, wifi: &Wifi, mut dialog: State<Option<WifiDialog>>) -> Row {
    let status = match wifi.state {
        LinkState::Connected => tr!("status-connected"),
        _ => tr!("status-connecting"),
    };
    let details = current.profile.clone().map(|profile| {
        let name = current.name.clone();
        Button::new(tr!("wifi-details")).on_press(move |_| {
            dialog.set(Some(WifiDialog::Details {
                name: name.clone(),
                profile: profile.clone(),
            }))
        })
    });
    Row::new(intern(current.name.clone()))
        .detail(status)
        .control(signal(current).maybe_child(details))
}

fn known_row(
    found: &WifiNetwork,
    remote: &Remote<NetworkCommand>,
    mut dialog: State<Option<WifiDialog>>,
) -> Row {
    let Some(profile) = found.profile.clone() else {
        return Row::new(intern(found.name.clone()));
    };
    let options = found.options;
    let items = vec![
        MenuItem::action("autoconnect", tr!("wifi-autoconnect-menu")).with_check(
            CheckState::Checkbox(options.is_some_and(|options| options.autoconnect)),
        ),
        MenuItem::Separator,
        MenuItem::action("copy-password", tr!("wifi-copy-password"))
            .with_enabled(found.security == Security::Personal),
        MenuItem::action("configure", tr!("wifi-configure")),
        MenuItem::Separator,
        MenuItem::action("forget", tr!("wifi-forget-menu")),
    ];
    let name = found.name.clone();
    let actions = remote.clone();
    let menu_profile = profile.clone();
    let more = MenuButton::system_image(tr!("wifi-more"), Symbol::EllipsisCircle, items)
        .on_activate(move |id: String| {
            let profile = menu_profile.clone();
            let name = name.clone();
            match id.as_str() {
                "autoconnect" => {
                    if let Some(options) = options {
                        actions.send(NetworkCommand::Configure(
                            profile,
                            WifiOptions {
                                autoconnect: !options.autoconnect,
                                ..options
                            },
                        ));
                    }
                }
                "copy-password" => copy_password(&actions, profile),
                "configure" => dialog.set(Some(WifiDialog::Details { name, profile })),
                "forget" => dialog.set(Some(WifiDialog::Forget { name, profile })),
                _ => {}
            }
        });
    let check = Image::system(Symbol::Checkmark)
        .size(SIGNAL_SIZE)
        .color(if found.current {
            palette(bk::theme::resolved().scheme).ink_strong
        } else {
            Rgba::TRANSPARENT
        });
    let row = Row::new(intern(found.name.clone()))
        .icon(check)
        .control(signal(found).child(more));
    if found.current {
        return row;
    }
    let join = remote.clone();
    row.hover_control(
        Button::new(tr!("wifi-connect"))
            .on_press(move |_| join.send(NetworkCommand::Activate(profile.clone()))),
    )
}

fn other_row(
    found: &WifiNetwork,
    network: &Live<NetworkManager>,
    dialog: State<Option<WifiDialog>>,
) -> Row {
    let connect = {
        let remote = network.remote.clone();
        let failure = network.failure;
        let found = found.clone();
        move || connect(&found, &remote, dialog, failure)
    };
    let from_menu = connect.clone();
    let more = MenuButton::system_image(
        tr!("wifi-more"),
        Symbol::EllipsisCircle,
        vec![MenuItem::action("connect", tr!("wifi-connect"))],
    )
    .on_activate(move |_: String| from_menu());
    Row::new(intern(found.name.clone()))
        .icon(spacer())
        .hover_control(Button::new(tr!("wifi-connect")).on_press(move |_| connect()))
        .control(signal(found).child(more))
}

fn connect(
    found: &WifiNetwork,
    remote: &Remote<NetworkCommand>,
    mut dialog: State<Option<WifiDialog>>,
    mut failure: State<Option<String>>,
) {
    match found.security {
        Security::Open | Security::Encrypted => remote.send(NetworkCommand::Join {
            access_point: found.access_point.clone(),
            password: None,
        }),
        Security::Personal => dialog.set(Some(WifiDialog::Password {
            name: found.name.clone(),
            access_point: found.access_point.clone(),
            protocol: found.protocol,
        })),
        Security::Enterprise => failure.set(Some(tr!("wifi-enterprise").to_string())),
    }
}

fn copy_password(remote: &Remote<NetworkCommand>, profile: OwnedObjectPath) {
    let (reply, answer) = async_channel::bounded(1);
    remote.send(NetworkCommand::Password(profile, reply));
    spawn(async move {
        if let Ok(password) = answer.recv().await
            && let Err(error) = freya_clipboard::prelude::Clipboard::set(password.to_string())
        {
            tracing::warn!(?error, "could not copy the Wi-Fi password");
        }
    });
}

fn spacer() -> Rect {
    rect()
        .width(Size::px(SIGNAL_SIZE))
        .height(Size::px(SIGNAL_SIZE))
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

pub fn wifi_dialog(
    mut dialog: State<Option<WifiDialog>>,
    network: &Live<NetworkManager>,
) -> Option<Element> {
    let shown = dialog.read().clone()?;
    let remote = network.remote.clone();
    Some(match shown {
        WifiDialog::Password {
            name,
            access_point,
            protocol,
        } => PasswordSheet {
            name,
            access_point,
            protocol,
            dialog,
            remote: RemoteHandle(remote),
        }
        .into_element(),
        WifiDialog::Details { name, profile } => {
            let wifi = network
                .snapshot
                .read()
                .clone()
                .and_then(|network| network.wifi);
            let found = wifi.as_ref().and_then(|wifi| {
                wifi.networks
                    .iter()
                    .find(|network| network.profile.as_ref() == Some(&profile))
                    .cloned()
            });
            let (Some(wifi), Some(found)) = (wifi, found) else {
                return None;
            };
            DetailsSheet {
                name,
                profile,
                found,
                wifi,
                dialog,
                remote: RemoteHandle(remote),
            }
            .into_element()
        }
        WifiDialog::Forget { name, profile } => Alert::new(
            intern(tr!("wifi-forget-title", network = name.as_str())),
            tr!("wifi-forget-message"),
        )
        .confirm(tr!("wifi-forget-confirm"), move |_| {
            remote.send(NetworkCommand::Forget(profile.clone()));
            dialog.set(None);
        })
        .cancel(tr!("cancel"), move |_| dialog.set(None))
        .into_element(),
    })
}

struct RemoteHandle(Remote<NetworkCommand>);

impl PartialEq for RemoteHandle {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(PartialEq)]
struct PasswordSheet {
    name: String,
    access_point: OwnedObjectPath,
    protocol: Protocol,
    dialog: State<Option<WifiDialog>>,
    remote: RemoteHandle,
}

impl Component for PasswordSheet {
    fn render(&self) -> impl IntoElement {
        let password = use_state(PasswordField::default);
        let mut revealed = use_state(|| false);
        let mut dialog = self.dialog;
        let join = {
            let remote = self.remote.0.clone();
            let access_point = self.access_point.clone();
            move |typed: Zeroizing<String>| {
                remote.send(NetworkCommand::Join {
                    access_point: access_point.clone(),
                    password: Some(typed),
                });
                dialog.set(None);
            }
        };
        let mut submit = join.clone();
        let mut field = password;
        let protocol = match self.protocol {
            Protocol::Wpa2 => "WPA2",
            Protocol::Wpa3 => "WPA3",
            Protocol::Wpa2Wpa3 => "WPA2/WPA3",
        };
        let title = tr!(
            "wifi-password-title",
            network = self.name.as_str(),
            protocol = protocol
        );
        let fields = Form::new().section(
            Section::new()
                .row(
                    Row::new(tr!("wifi-password")).control(
                        SecureField::new(password, tr!("wifi-password"))
                            .revealed(*revealed.read())
                            .auto_focus(true)
                            .width(Size::px(DIALOG_WIDTH / 2.0))
                            .on_submit(join),
                    ),
                )
                .row(
                    Row::new(tr!("wifi-show-password")).control(
                        Switch::new(tr!("wifi-show-password"))
                            .on(*revealed.read())
                            .on_toggle(move |on| revealed.set(on)),
                    ),
                ),
        );
        Sheet::new(
            VStack::new()
                .width(Size::px(DIALOG_WIDTH))
                .spacing(SPACE_3)
                .cross_align(Alignment::Start)
                .child(
                    HStack::new()
                        .spacing(SPACE_3)
                        .cross_align(Alignment::Start)
                        .child(Image::system(Symbol::Wifi).size(DIALOG_ICON).color(TINT))
                        .child(Text::new(title).font(HEADLINE).width(Size::fill())),
                )
                .child(fields)
                .child(buttons(
                    None::<fn()>,
                    move || dialog.set(None),
                    move || {
                        if let Some(typed) = field.write().take() {
                            submit(typed);
                        }
                    },
                )),
        )
    }
}

#[derive(PartialEq)]
struct DetailsSheet {
    name: String,
    profile: OwnedObjectPath,
    found: WifiNetwork,
    wifi: Wifi,
    dialog: State<Option<WifiDialog>>,
    remote: RemoteHandle,
}

impl Component for DetailsSheet {
    fn render(&self) -> impl IntoElement {
        let saved = self.found.options.unwrap_or(WifiOptions {
            autoconnect: true,
            low_data: false,
            private_address: PrivateAddress::Off,
        });
        let mut draft = use_state(|| saved);
        let mut dialog = self.dialog;
        let options = *draft.read();
        let mut identity = Section::new().row(
            Row::new(tr!("wifi-private-address"))
                .detail(tr!("wifi-private-address-help"))
                .control(
                    PopUpButton::new(tr!("wifi-private-address"))
                        .item(tr!("private-address-off"))
                        .item(tr!("private-address-fixed"))
                        .item(tr!("private-address-rotating"))
                        .selected(
                            PRIVATE_ADDRESSES
                                .iter()
                                .position(|address| *address == options.private_address)
                                .unwrap_or(0),
                        )
                        .on_change(move |index: usize| {
                            draft.write().private_address = PRIVATE_ADDRESSES[index];
                        }),
                ),
        );
        if self.found.current && !self.wifi.hardware_address.is_empty() {
            identity = identity.row(Row::value(
                tr!("wifi-address"),
                self.wifi.hardware_address.clone(),
            ));
        }
        let mut form = Form::new()
            .section(
                Section::new().row(
                    Row::new(tr!("wifi-autoconnect")).control(
                        Switch::new(tr!("wifi-autoconnect"))
                            .on(options.autoconnect)
                            .on_toggle(move |on| draft.write().autoconnect = on),
                    ),
                ),
            )
            .section(identity)
            .section(
                Section::new().row(
                    Row::new(tr!("wifi-low-data"))
                        .detail(tr!("wifi-low-data-help"))
                        .control(
                            Switch::new(tr!("wifi-low-data"))
                                .on(options.low_data)
                                .on_toggle(move |on| draft.write().low_data = on),
                        ),
                ),
            );
        if self.found.current {
            let mut addresses = Section::new();
            if let Some(address) = &self.wifi.address {
                addresses = addresses.row(Row::value(tr!("wifi-ip-address"), address.clone()));
            }
            if let Some(router) = &self.wifi.router {
                addresses = addresses.row(Row::value(tr!("wifi-router"), router.clone()));
            }
            form = form.section(addresses);
        }
        let save = {
            let remote = self.remote.0.clone();
            let profile = self.profile.clone();
            move || {
                if *draft.peek() != saved {
                    remote.send(NetworkCommand::Configure(profile.clone(), *draft.peek()));
                }
                dialog.set(None);
            }
        };
        let forget = {
            let name = self.name.clone();
            let profile = self.profile.clone();
            move || {
                dialog.set(Some(WifiDialog::Forget {
                    name: name.clone(),
                    profile: profile.clone(),
                }))
            }
        };
        Sheet::new(
            VStack::new()
                .width(Size::px(DIALOG_WIDTH))
                .spacing(SPACE_3)
                .child(form)
                .child(buttons(Some(forget), move || dialog.set(None), save)),
        )
    }
}

fn buttons(
    forget: Option<impl FnMut() + 'static>,
    mut cancel: impl FnMut() + 'static,
    mut confirm: impl FnMut() + 'static,
) -> HStack {
    let leading =
        rect()
            .width(Size::flex(1.0))
            .maybe_child(forget.map(|mut forget| {
                Button::new(tr!("wifi-forget-menu")).on_press(move |_| forget())
            }));
    HStack::new()
        .width(Size::fill())
        .content(Content::flex())
        .spacing(SPACE_2)
        .child(leading)
        .child(Button::new(tr!("cancel")).on_press(move |_| cancel()))
        .child(
            Button::new(tr!("ok"))
                .style(ButtonStyle::bordered_prominent)
                .on_press(move |_| confirm()),
        )
}
