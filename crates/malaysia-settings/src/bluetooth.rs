use bk::patterns::settings::Pane;
use bk::prelude::*;
use bk::progress::Spinner;
use bk::views::feedback::Sheet;
use bk::views::form::{Form, Row, Section};
use ctdesign::SPACE_2;
use freya_components::svg_viewer::SvgViewer;

use crate::localize::{intern, tr};
use crate::menubar::show_in_menu_bar;
use crate::wifi::TINT;
use ctappearance::menubar::MenubarConfig;
use malaysia_services::bluez::{BlueZ, BluetoothCommand, Device, PROMPTS, Prompt};
use malaysia_services::bus::{Live, WhileVisible};
use zbus::zvariant::OwnedObjectPath;

const PICTURE_SIZE: (f32, f32) = (40.0, 32.0);

// Pictures by the freedesktop icon name that BlueZ derives from the device class.
fn picture(icon: &str) -> (&'static str, &'static [u8]) {
    match icon {
        "input-keyboard" => ("keyboard", include_bytes!("../assets/devices/keyboard.svg")),
        "input-mouse" => ("mouse", include_bytes!("../assets/devices/mouse.svg")),
        "input-tablet" => ("tablet", include_bytes!("../assets/devices/tablet.svg")),
        "input-gaming" => ("gamepad", include_bytes!("../assets/devices/gamepad.svg")),
        "audio-headset" | "audio-headphones" => (
            "headphones",
            include_bytes!("../assets/devices/headphones.svg"),
        ),
        "audio-card" => ("speaker", include_bytes!("../assets/devices/speaker.svg")),
        "phone" => ("phone", include_bytes!("../assets/devices/phone.svg")),
        "computer" => ("computer", include_bytes!("../assets/devices/computer.svg")),
        _ => ("generic", include_bytes!("../assets/devices/generic.svg")),
    }
}

pub fn use_pairing_prompt() -> State<Option<Prompt>> {
    let mut prompt = use_state(|| None);
    use_hook(move || {
        let prompts = PROMPTS.1.clone();
        spawn(async move {
            while let Ok(next) = prompts.recv().await {
                prompt.set(match next {
                    Prompt::Done => None,
                    shown => Some(shown),
                });
            }
        });
    });
    prompt
}

// BlueZ answers Connect and Pair only when they finish; until the next snapshot or a failure the row waits.
pub fn use_busy_device(bluetooth: &Live<BlueZ>) -> State<Option<OwnedObjectPath>> {
    let mut busy = use_state(|| None);
    let (snapshot, failure) = (bluetooth.snapshot, bluetooth.failure);
    use_side_effect(move || {
        let _ = (snapshot.read(), failure.read());
        busy.set_if_modified(None);
    });
    busy
}

pub fn bluetooth_pane(
    bluetooth: &Live<BlueZ>,
    busy: State<Option<OwnedObjectPath>>,
    menubar: &MenubarConfig,
    error: State<Option<String>>,
) -> Option<Pane> {
    let adapter = bluetooth.snapshot.read().clone().flatten()?;
    let power = bluetooth.remote.clone();
    let visible = bluetooth.remote.clone();
    let switch = Switch::new(tr!("bluetooth"))
        .on(adapter.powered)
        .disabled(adapter.hardware_blocked)
        .on_toggle(move |on| power.send(BluetoothCommand::Power(on)));
    let mut header = Section::new().row(
        Row::new(tr!("bluetooth"))
            .symbol(Symbol::Bluetooth, TINT)
            .detail(tr!("bluetooth-about"))
            .control(WhileVisible::new(switch, move |shown| {
                visible.send(BluetoothCommand::Visible(shown))
            })),
    );
    let pane = |form: Form| {
        let form = form.section(Section::new().row(show_in_menu_bar(
            menubar,
            tr!("show-in-menu-bar"),
            |config| &mut config.show_bluetooth,
            error,
        )));
        Pane::new("bluetooth", tr!("bluetooth"), Symbol::Bluetooth, TINT, form)
    };
    if !adapter.powered {
        return Some(pane(Form::new().section(header)));
    }
    header = header.footer(intern(tr!(
        "bluetooth-visible-as",
        name = adapter.name.as_str()
    )));
    let (paired, nearby): (Vec<&Device>, Vec<&Device>) =
        adapter.devices.iter().partition(|device| device.paired);
    let mut form = Form::new().section(header);
    if !paired.is_empty() {
        form = form.section(paired.into_iter().fold(
            Section::titled(tr!("bluetooth-my-devices")),
            |section, device| section.row(paired_row(device, bluetooth, busy)),
        ));
    }
    let nearby = if nearby.is_empty() {
        Section::titled(tr!("bluetooth-nearby")).row(Row::new(tr!("bluetooth-searching")))
    } else {
        nearby.into_iter().fold(
            Section::titled(tr!("bluetooth-nearby")),
            |section, device| {
                let remote = bluetooth.remote.clone();
                let path = device.path.clone();
                let control = waiting(busy, &device.path).unwrap_or_else(|| {
                    Button::new(tr!("bluetooth-connect"))
                        .on_press(move |_| {
                            let mut busy = busy;
                            busy.set(Some(path.clone()));
                            remote.send(BluetoothCommand::Pair(path.clone()))
                        })
                        .into_element()
                });
                section.row(
                    Row::new(intern(device.name.clone()))
                        .icon(device_picture(&device.icon))
                        .control(control),
                )
            },
        )
    };
    Some(pane(form.section(nearby)))
}

fn waiting(busy: State<Option<OwnedObjectPath>>, device: &OwnedObjectPath) -> Option<Element> {
    (busy.read().as_ref() == Some(device))
        .then(|| Spinner::new(tr!("status-connecting")).into_element())
}

fn paired_row(
    device: &Device,
    bluetooth: &Live<BlueZ>,
    busy: State<Option<OwnedObjectPath>>,
) -> Row {
    let status = match (device.connected, device.battery) {
        (true, Some(battery)) => tr!("bluetooth-connected-battery", percent = battery),
        (true, None) => tr!("status-connected").to_string(),
        (false, _) => tr!("status-disconnected").to_string(),
    };
    let toggle = bluetooth.remote.clone();
    let forget = bluetooth.remote.clone();
    let path = device.path.clone();
    let forget_path = device.path.clone();
    let connected = device.connected;
    let action = if connected {
        tr!("bluetooth-disconnect")
    } else {
        tr!("bluetooth-connect")
    };
    let waiting = waiting(busy, &device.path);
    let status = if waiting.is_some() {
        tr!("status-connecting").to_string()
    } else {
        status
    };
    Row::new(intern(device.name.clone()))
        .icon(device_picture(&device.icon))
        .detail(intern(status))
        .control(waiting.unwrap_or_else(|| {
            HStack::new()
                .spacing(SPACE_2)
                .child(Button::new(action).on_press(move |_| {
                    let mut busy = busy;
                    busy.set(Some(path.clone()));
                    toggle.send(if connected {
                        BluetoothCommand::Disconnect(path.clone())
                    } else {
                        BluetoothCommand::Connect(path.clone())
                    })
                }))
                .child(
                    Button::new(tr!("bluetooth-forget")).on_press(move |_| {
                        forget.send(BluetoothCommand::Forget(forget_path.clone()))
                    }),
                )
                .into_element()
        }))
}

fn device_picture(icon: &str) -> SvgViewer {
    let (name, svg) = picture(icon);
    let (width, height) = PICTURE_SIZE;
    SvgViewer::new((name, Bytes::from_static(svg)))
        .width(Size::px(width))
        .height(Size::px(height))
        .show_loader(false)
}

pub fn pairing_sheet(mut prompt: State<Option<Prompt>>) -> Option<Sheet> {
    let shown = prompt.read().clone()?;
    let (device, passkey, reply) = match shown {
        Prompt::Confirm {
            device,
            passkey,
            reply,
        } => (device, passkey, Some(reply)),
        Prompt::Display { device, passkey } => (device, passkey, None),
        Prompt::Done => return None,
    };
    let confirming = reply.is_some();
    let help = if confirming {
        tr!("bluetooth-pair-confirm")
    } else {
        tr!("bluetooth-pair-type")
    };
    let answer = move |accepted: bool| {
        if let Some(reply) = &reply {
            let _ = reply.try_send(accepted);
        }
    };
    let cancel = answer.clone();
    let mut buttons = HStack::new()
        .spacing(SPACE_2)
        .child(Button::new(tr!("cancel")).on_press(move |_| {
            cancel(false);
            prompt.set(None);
        }));
    if confirming {
        buttons = buttons.child(Button::new(tr!("bluetooth-pair")).on_press(move |_| {
            answer(true);
            prompt.set(None);
        }));
    }
    Some(Sheet::new(
        VStack::new()
            .spacing(SPACE_2)
            .child(Text::new(tr!(
                "bluetooth-pair-title",
                device = device.as_str()
            )))
            .child(Text::new(help))
            .child(Text::new(passkey).font_size(TITLE.size as f32))
            .child(buttons),
    ))
}
