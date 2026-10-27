//! Contador — demo app for BarrierKit's global menu export (ADR 0040 §10).

use std::cell::RefCell;
use std::rc::Rc;

use barrier_kit::app_menu::{AppMenu, Menu, MenuHandle};
use barrier_kit::controls::Button;
use barrier_kit::menu::{CheckState, MenuItem};
use barrier_kit::platform::{Decorations, WindowControls, WindowOptions, run_window};
use barrier_kit::titlebar::Titlebar;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Size};

const ID_NEW_WINDOW: &str = "new-window";
const ID_CLOSE_WINDOW: &str = "close-window";
const ID_COPY_VALUE: &str = "copy-value";
const ID_INCREMENT: &str = "increment";
const ID_DECREMENT: &str = "decrement";
const ID_RESET: &str = "reset";
const ID_STEP_1: &str = "step-1";
const ID_STEP_5: &str = "step-5";
const ID_STEP_10: &str = "step-10";
const ID_ALLOW_NEGATIVE: &str = "allow-negative";
const ID_ABOUT: &str = "about";

fn action(id: &'static str, label: &str, shortcut: Option<&str>, enabled: bool) -> MenuItem {
    MenuItem::Action {
        id: id.to_string(),
        label: label.to_string(),
        shortcut: shortcut.map(ToOwned::to_owned),
        enabled,
        check: CheckState::None,
        icon: None,
    }
}

fn radio(id: &'static str, label: &str, shortcut: Option<&str>, active: bool) -> MenuItem {
    MenuItem::Action {
        id: id.to_string(),
        label: label.to_string(),
        shortcut: shortcut.map(ToOwned::to_owned),
        enabled: true,
        check: CheckState::Radio(active),
        icon: None,
    }
}

fn checkbox(id: &'static str, label: &str, active: bool) -> MenuItem {
    MenuItem::Action {
        id: id.to_string(),
        label: label.to_string(),
        shortcut: None,
        enabled: true,
        check: CheckState::Checkbox(active),
        icon: None,
    }
}

fn separator() -> MenuItem {
    MenuItem::Separator
}

fn build_menu(count: i32, step: i32, allow_negative: bool) -> AppMenu {
    AppMenu::new(vec![
        Menu::new(
            "Archivo",
            vec![
                action(ID_NEW_WINDOW, "Nueva ventana", Some("Ctrl+N"), false),
                separator(),
                action(ID_CLOSE_WINDOW, "Cerrar ventana", Some("Ctrl+W"), true),
            ],
        ),
        Menu::new(
            "Edición",
            vec![action(ID_COPY_VALUE, "Copiar valor", Some("Ctrl+C"), false)],
        ),
        Menu::new(
            "Contador",
            vec![
                action(ID_INCREMENT, "Incrementar", Some("Ctrl+Up"), true),
                action(ID_DECREMENT, "Decrementar", Some("Ctrl+Down"), true),
                separator(),
                action(ID_RESET, "Reiniciar", Some("Ctrl+R"), count != 0),
                separator(),
                MenuItem::Submenu {
                    id: Some("step-submenu".to_string()),
                    label: "Paso".to_string(),
                    enabled: true,
                    icon: None,
                    items: vec![
                        radio(ID_STEP_1, "1", None, step == 1),
                        radio(ID_STEP_5, "5", Some("Ctrl+5"), step == 5),
                        radio(ID_STEP_10, "10", None, step == 10),
                    ],
                },
                separator(),
                checkbox(ID_ALLOW_NEGATIVE, "Permitir negativos", allow_negative),
            ],
        ),
        Menu::new(
            "Ayuda",
            vec![action(ID_ABOUT, "Acerca de Contador", None, true)],
        ),
    ])
}

fn sync_menu(handle: &MenuHandle, count: i32, step: i32, allow_negative: bool) {
    handle.update(ID_RESET, |item| {
        if let MenuItem::Action { enabled, .. } = item {
            *enabled = count != 0;
        }
    });
    handle.update(ID_STEP_1, |item| {
        if let MenuItem::Action { check, .. } = item {
            *check = CheckState::Radio(step == 1);
        }
    });
    handle.update(ID_STEP_5, |item| {
        if let MenuItem::Action { check, .. } = item {
            *check = CheckState::Radio(step == 5);
        }
    });
    handle.update(ID_STEP_10, |item| {
        if let MenuItem::Action { check, .. } = item {
            *check = CheckState::Radio(step == 10);
        }
    });
    handle.update(ID_ALLOW_NEGATIVE, |item| {
        if let MenuItem::Action { check, .. } = item {
            *check = CheckState::Checkbox(allow_negative);
        }
    });
}

struct Contador {
    action_rx: Rc<RefCell<Option<std::sync::mpsc::Receiver<String>>>>,
}

impl App for Contador {
    fn render(&self) -> impl IntoElement {
        let mut count = use_state(|| 0i32);
        let mut step = use_state(|| 1i32);
        let mut allow_negative = use_state(|| false);
        let mut menu_handle: State<Option<MenuHandle>> = use_state(|| None);

        {
            use_hook(move || {
                menu_handle.set(WindowControls::get().menu_handle());
            });
        }

        // Drain actions queued by the global menu or in-app shortcuts. The
        // receiver is read on every render because the compositor dispatches a
        // key event after handling a shortcut, which wakes the component.
        let mut pending = use_state(Vec::<String>::new);
        let rx = use_hook(|| {
            Rc::new(RefCell::new(
                self.action_rx
                    .borrow_mut()
                    .take()
                    .expect("menu action receiver already taken"),
            ))
        });
        while let Ok(id) = rx.borrow_mut().try_recv() {
            pending.write().push(id);
        }
        let ids: Vec<String> = {
            let guard = pending.read();
            guard.clone()
        };
        pending.write().clear();
        for id in ids {
            match id.as_str() {
                ID_INCREMENT => {
                    let s = *step.read();
                    let current = *count.read();
                    count.set(current.saturating_add(s));
                }
                ID_DECREMENT => {
                    let s = *step.read();
                    let current = *count.read();
                    let next = current.saturating_sub(s);
                    if next < 0 && !*allow_negative.read() {
                        count.set(0);
                    } else {
                        count.set(next);
                    }
                }
                ID_RESET => count.set(0),
                ID_STEP_1 => step.set(1),
                ID_STEP_5 => step.set(5),
                ID_STEP_10 => step.set(10),
                ID_ALLOW_NEGATIVE => {
                    let current = *allow_negative.read();
                    allow_negative.set(!current);
                }
                ID_CLOSE_WINDOW => std::process::exit(0),
                _ => {}
            }
            tracing::info!(action = id, count = *count.read(), "contador: action");
        }

        if let Some(handle) = menu_handle.read().as_ref() {
            sync_menu(handle, *count.read(), *step.read(), *allow_negative.read());
        }

        let palette = barrier_kit::theme::resolved();
        let colors = vow_appearance::palette(palette.scheme);
        let [tr, tg, tb, ta] = colors.text.focused.to_rgba8();
        let text = Color::from_argb(ta, tr, tg, tb);

        let display = format!("{}", *count.read());

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Vertical)
            .child(Titlebar::new().on_close(|()| std::process::exit(0)))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .center()
                    .direction(Direction::Vertical)
                    .spacing(24.0)
                    .main_align(Alignment::Center)
                    .child(label().text(display).font_size(96.0).color(text))
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .spacing(16.0)
                            .child(
                                Button::new()
                                    .on_press({
                                        let mut count = count;
                                        let step = step;
                                        let allow_negative = allow_negative;
                                        move |_| {
                                            let s = *step.read();
                                            let current = *count.read();
                                            let next = current.saturating_sub(s);
                                            count.set(if next < 0 && !*allow_negative.read() {
                                                0
                                            } else {
                                                next
                                            });
                                        }
                                    })
                                    .child(label().text("−").font_size(24.0).color(text)),
                            )
                            .child(
                                Button::new()
                                    .on_press({
                                        let mut count = count;
                                        let step = step;
                                        move |_| {
                                            let s = *step.read();
                                            let current = *count.read();
                                            count.set(current.saturating_add(s));
                                        }
                                    })
                                    .child(label().text("+").font_size(24.0).color(text)),
                            ),
                    ),
            )
            .into_element()
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    let (action_tx, action_rx) = std::sync::mpsc::channel::<String>();
    let app = AppComponent::new(Contador {
        action_rx: Rc::new(RefCell::new(Some(action_rx))),
    });
    let menu = build_menu(0, 1, false);
    run_window(
        app,
        WindowOptions::new("Contador", "Contador")
            .app_id("org.malaysia.Contador")
            .decorations(Decorations::Client)
            .size((480, 320))
            .menu(menu)
            .on_menu_action(move |id| {
                tracing::info!(action = id, "contador: menu action");
                let _ = action_tx.send(id.to_string());
            }),
    )
    .expect("contador");
}
