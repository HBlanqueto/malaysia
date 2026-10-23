//! The lock component: the session-lock screen (`ext-session-lock-v1`).
//!
//! One CelestialKit/Freya surface per output (ADR 0034). PAM runs on its own
//! thread exactly as in Bloque 10; the app polls its event buffer from a Freya
//! task. If the surface cannot be rendered the process exits non-zero, so the
//! compositor keeps the session locked (the never-unlock guarantee lives on
//! the compositor side, `yuki/src/lock.rs`).

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use celestial_kit::password::PasswordField;
use celestial_kit::platform::{SessionUnlock, run_lock};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Size};
use yuki_design::{
    LOCK_FIELD_HEIGHT, LOCK_FIELD_WIDTH, LOCK_FONT_SIZE, LOCK_LABEL_FONT_SIZE, LOCK_LABEL_OFFSET,
    LOCK_MASK_FONT_SIZE, LOCK_TITLE_SCALE,
};

use crate::pam::{self, EventBuffer};

#[derive(Clone, PartialEq)]
struct Model {
    label: String,
    masked: String,
    status: Option<String>,
    verifying: bool,
    theme_revision: u64,
}

struct LockApp {
    model: State<Model>,
    field: State<PasswordField>,
    events: EventBuffer,
    attempt: Rc<RefCell<Option<pam::Attempt>>>,
}

impl App for LockApp {
    fn render(&self) -> impl IntoElement {
        let mut model = self.model;
        let mut field = self.field;
        let events = self.events.clone();
        let attempt = self.attempt.clone();
        let attempt_for_future = attempt.clone();
        let unlock = SessionUnlock::get();

        use_future(move || {
            let events = events.clone();
            let attempt = attempt_for_future.clone();
            let mut model = model;
            let unlock = unlock.clone();
            async move {
                loop {
                    async_io::Timer::after(Duration::from_millis(100)).await;
                    let pending = {
                        let Ok(mut buffer) = events.lock() else {
                            continue;
                        };
                        std::mem::take(&mut *buffer)
                    };
                    if pending.is_empty() {
                        continue;
                    }
                    for event in pending {
                        match event {
                            pam::Event::Said(pam::Message::Prompt { text, .. }) => {
                                let mut next = model.peek().clone();
                                next.label = pam::prompt_label(&text);
                                next.verifying = true;
                                next.status = None;
                                model.set(next);
                            }
                            pam::Event::Said(pam::Message::Info(text)) => {
                                eprintln!("lock: PAM: {text}");
                            }
                            pam::Event::Said(pam::Message::Error(text)) => {
                                let mut next = model.peek().clone();
                                next.status = Some(text);
                                next.verifying = false;
                                model.set(next);
                            }
                            pam::Event::Ended(pam::Outcome::Authenticated) => {
                                unlock.unlock();
                            }
                            pam::Event::Ended(pam::Outcome::Denied(message)) => {
                                eprintln!("lock: PAM refused: {message}");
                                *attempt.borrow_mut() = None;
                                let mut next = model.peek().clone();
                                next.status = Some(message);
                                next.masked.clear();
                                next.verifying = false;
                                model.set(next);
                            }
                        }
                    }
                }
            }
        });

        let view = self.model.read();
        let palette = celestial_kit::theme::lock_palette(&celestial_kit::theme::resolved());
        let background = rgba(palette.bg_surface);
        let ink = rgba(palette.ink);
        let muted = rgba(palette.ink_muted);
        let accent = rgba(palette.accent);
        let danger = rgba(palette.danger);
        let status = view.status.clone();

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(background)
            .direction(Direction::Vertical)
            .main_align(Alignment::Center)
            .cross_align(Alignment::Center)
            .spacing(LOCK_LABEL_OFFSET as f32)
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if model.peek().verifying {
                    return;
                }
                match &event.key {
                    Key::Named(NamedKey::Enter) => {
                        submit(field, attempt.clone());
                    }
                    Key::Named(NamedKey::Backspace) => {
                        field.write().backspace();
                        let mut next = model.peek().clone();
                        next.masked = field.read().masked();
                        next.status = None;
                        model.set(next);
                    }
                    Key::Character(text) if event.modifiers.is_empty() => {
                        {
                            let mut field = field.write();
                            for ch in text.chars() {
                                field.insert(ch);
                            }
                        }
                        let mut next = model.peek().clone();
                        next.masked = field.read().masked();
                        next.status = None;
                        model.set(next);
                    }
                    _ => {}
                }
            })
            .child(
                label()
                    .text("Sesión bloqueada")
                    .font_size(LOCK_FONT_SIZE * LOCK_TITLE_SCALE)
                    .color(ink),
            )
            .child(
                label()
                    .text(view.label.clone())
                    .font_size(LOCK_LABEL_FONT_SIZE)
                    .color(muted),
            )
            .child(
                rect()
                    .width(Size::px(LOCK_FIELD_WIDTH as f32))
                    .height(Size::px(LOCK_FIELD_HEIGHT as f32))
                    .center()
                    .background(if view.status.is_some() {
                        danger
                    } else {
                        accent
                    })
                    .corner_radius((LOCK_FIELD_HEIGHT / 4) as f32)
                    .child(
                        label()
                            .text(if view.masked.is_empty() {
                                "Contraseña".to_string()
                            } else {
                                view.masked.clone()
                            })
                            .font_size(LOCK_MASK_FONT_SIZE)
                            .color(background),
                    ),
            )
            .child(match status {
                Some(text) => label().text(text).color(danger),
                None => label().text(" ").color(muted),
            })
    }
}

/// Connects to the compositor, locks the session and runs the lock screen.
pub fn run() -> anyhow::Result<()> {
    let events: EventBuffer = Arc::new(Mutex::new(Vec::new()));
    let attempt = Rc::new(RefCell::new(None));
    start_auth(events.clone(), attempt.clone());

    let model = State::create_global(Model {
        label: "Contraseña".to_string(),
        masked: String::new(),
        status: None,
        verifying: false,
        theme_revision: theme_revision(),
    });
    let field = State::create_global(PasswordField::default());
    let app = AppComponent::new(LockApp {
        model,
        field,
        events,
        attempt,
    });
    run_lock(app).map_err(|error| anyhow::anyhow!("{error}"))
}

fn start_auth(events: EventBuffer, attempt: Rc<RefCell<Option<pam::Attempt>>>) -> String {
    let Some(user) = pam::current_user() else {
        eprintln!("lock: cannot resolve the current user for PAM");
        return String::new();
    };
    eprintln!("lock: starting PAM for {user}");
    *attempt.borrow_mut() = Some(pam::Attempt::start(
        events,
        pam::Service::LOCK,
        user.clone(),
    ));
    user
}

fn submit(mut field: State<PasswordField>, attempt: Rc<RefCell<Option<pam::Attempt>>>) {
    let Some(secret) = field.write().take() else {
        return;
    };
    if let Some(attempt) = attempt.borrow().as_ref() {
        attempt.answer(secret);
    }
}

fn rgba(c: yuki_design::Rgba) -> Color {
    let [r, g, b, a] = c.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn theme_revision() -> u64 {
    let mut hash = 0u64;
    for path in [
        yuki_appearance::appearance_path(),
        yuki_appearance::wallpaper_state_path(),
    ] {
        if let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified())
            && let Ok(value) = modified.duration_since(SystemTime::UNIX_EPOCH)
        {
            hash = hash.wrapping_mul(31).wrapping_add(value.as_nanos() as u64);
        }
    }
    hash
}
