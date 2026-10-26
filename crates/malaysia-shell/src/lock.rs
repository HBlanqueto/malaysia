//! The lock component: the session-lock screen (`ext-session-lock-v1`).
//!
//! One BarrierKit/Freya surface per output (ADR 0034). PAM runs on its own
//! thread exactly as in Bloque 10; the app polls its event buffer from a Freya
//! task. If the surface cannot be rendered the process exits non-zero, so the
//! compositor keeps the session locked (the never-unlock guarantee lives on
//! the compositor side, `malaysia-compositor/src/lock.rs`).

use std::{
    cell::RefCell,
    io::Read,
    os::unix::net::UnixStream,
    rc::Rc,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use barrier_kit::password::{PasswordField, State as FieldState};
use barrier_kit::platform::{SessionUnlock, run_lock};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Size};
use vow_design::{
    LOCK_FIELD_HEIGHT, LOCK_FIELD_WIDTH, LOCK_FONT_SIZE, LOCK_LABEL_FONT_SIZE, LOCK_LABEL_OFFSET,
    LOCK_MASK_FONT_SIZE, LOCK_TITLE_SCALE,
};

use crate::pam::{self, EventBuffer};
use zeroize::Zeroizing;

#[derive(Clone, PartialEq)]
struct Model {
    label: String,
    masked: String,
    status: Option<String>,
    theme_revision: u64,
}

/// What the compositor should do after one PAM event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Continue,
    Unlock,
}

fn apply_pam_event(model: &mut Model, field: &mut PasswordField, event: &pam::Event) -> Action {
    match event {
        pam::Event::Said(pam::Message::Prompt { text, .. }) => {
            model.label = pam::prompt_label(text);
            Action::Continue
        }
        pam::Event::Said(pam::Message::Info(text)) => {
            eprintln!("lock: PAM: {text}");
            Action::Continue
        }
        pam::Event::Said(pam::Message::Error(text)) => {
            field.set_error(text.clone());
            model.status = Some(text.clone());
            model.masked = field.masked();
            Action::Continue
        }
        pam::Event::Ended(pam::Outcome::Authenticated) => Action::Unlock,
        pam::Event::Ended(pam::Outcome::Denied(message)) => {
            eprintln!("lock: PAM refused: {message}");
            field.set_error(message.clone());
            model.status = Some(message.clone());
            model.masked = field.masked();
            Action::Continue
        }
    }
}

/// Applies one key to the field. Returns the secret to send when Enter takes
/// a non-empty value; keys are ignored while the field is verifying.
fn handle_key(
    model: &mut Model,
    field: &mut PasswordField,
    key: &Key,
    text: Option<&str>,
    modifiers_empty: bool,
) -> Option<Zeroizing<String>> {
    if matches!(field.state(), FieldState::Verifying) {
        return None;
    }
    let secret = match key {
        Key::Named(NamedKey::Enter) => field.take(),
        Key::Named(NamedKey::Backspace) => {
            field.backspace();
            None
        }
        Key::Character(_) if modifiers_empty => {
            if let Some(text) = text {
                for ch in text.chars() {
                    field.insert(ch);
                }
            }
            None
        }
        _ => None,
    };
    model.masked = field.masked();
    if secret.is_some() || !matches!(field.state(), FieldState::Error(_)) {
        model.status = None;
    }
    secret
}

struct LockApp {
    model: State<Model>,
    field: State<PasswordField>,
    events: EventBuffer,
    attempt: Rc<RefCell<Option<pam::Attempt>>>,
    user: String,
    reader: UnixStream,
    notify: Option<UnixStream>,
}

impl App for LockApp {
    fn render(&self) -> impl IntoElement {
        let mut model = self.model;
        let mut field = self.field;
        let events = self.events.clone();
        let attempt = self.attempt.clone();
        let attempt_for_future = attempt.clone();
        let events_for_keys = self.events.clone();
        let user = self.user.clone();
        let notify = self.notify.as_ref().and_then(|s| s.try_clone().ok());
        let reader = Rc::new(RefCell::new(self.reader.try_clone().ok()));
        let unlock = SessionUnlock::get();

        use_future(move || {
            let events = events.clone();
            let attempt = attempt_for_future.clone();
            let mut model = model;
            let unlock = unlock.clone();
            let reader = reader.borrow_mut().take();
            async move {
                let Some(reader) = reader.and_then(|stream| async_io::Async::new(stream).ok())
                else {
                    return;
                };
                let mut wake = [0u8; 64];
                loop {
                    if reader.readable().await.is_err() {
                        break;
                    }
                    while reader.get_ref().read(&mut wake).is_ok() {}
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
                        let mut next = model.peek().clone();
                        let action = {
                            let mut field = field.write();
                            apply_pam_event(&mut next, &mut field, &event)
                        };
                        if matches!(event, pam::Event::Ended(pam::Outcome::Denied(_))) {
                            *attempt.borrow_mut() = None;
                        }
                        model.set(next);
                        if action == Action::Unlock {
                            unlock.unlock();
                        }
                    }
                }
            }
        });

        let view = self.model.read();
        let palette = barrier_kit::theme::lock_palette(&barrier_kit::theme::resolved());
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
                let mut next = model.peek().clone();
                let secret = {
                    let mut field = field.write();
                    handle_key(
                        &mut next,
                        &mut field,
                        &event.key,
                        match &event.key {
                            Key::Character(text) => Some(text.as_str()),
                            _ => None,
                        },
                        event.modifiers.is_empty(),
                    )
                };
                model.set(next);
                if let Some(secret) = secret {
                    submit(
                        secret,
                        attempt.clone(),
                        events_for_keys.clone(),
                        user.clone(),
                        notify.as_ref().and_then(|s| s.try_clone().ok()),
                    );
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
    let (reader, notify) = UnixStream::pair()?;
    reader.set_nonblocking(true)?;
    notify.set_nonblocking(true)?;
    let user = start_auth(events.clone(), attempt.clone(), Some(notify.try_clone()?));

    let model = State::create_global(Model {
        label: "Contraseña".to_string(),
        masked: String::new(),
        status: None,
        theme_revision: theme_revision(),
    });
    let field = State::create_global(PasswordField::default());
    let app = AppComponent::new(LockApp {
        model,
        field,
        events,
        attempt,
        user,
        reader,
        notify: Some(notify),
    });
    run_lock(app).map_err(|error| anyhow::anyhow!("{error}"))
}

fn start_auth(
    events: EventBuffer,
    attempt: Rc<RefCell<Option<pam::Attempt>>>,
    notify: Option<UnixStream>,
) -> String {
    let Some(user) = pam::current_user() else {
        eprintln!("lock: cannot resolve the current user for PAM");
        return String::new();
    };
    eprintln!("lock: starting PAM for {user}");
    *attempt.borrow_mut() = Some(pam::Attempt::start(
        events,
        notify,
        pam::Service::LOCK,
        user.clone(),
    ));
    user
}

fn submit(
    secret: Zeroizing<String>,
    attempt: Rc<RefCell<Option<pam::Attempt>>>,
    events: EventBuffer,
    user: String,
    notify: Option<UnixStream>,
) {
    if attempt.borrow().is_none() && !user.is_empty() {
        *attempt.borrow_mut() = Some(pam::Attempt::start(
            events,
            notify,
            pam::Service::LOCK,
            user,
        ));
    }
    if let Some(attempt) = attempt.borrow().as_ref() {
        attempt.answer(secret);
    }
}

fn rgba(c: vow_design::Rgba) -> Color {
    let [r, g, b, a] = c.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn theme_revision() -> u64 {
    let mut hash = 0u64;
    for path in [
        vow_appearance::appearance_path(),
        vow_appearance::wallpaper_state_path(),
    ] {
        if let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified())
            && let Ok(value) = modified.duration_since(SystemTime::UNIX_EPOCH)
        {
            hash = hash.wrapping_mul(31).wrapping_add(value.as_nanos() as u64);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        Model {
            label: "Contraseña".to_string(),
            masked: String::new(),
            status: None,
            theme_revision: 0,
        }
    }

    fn typed(model: &mut Model, field: &mut PasswordField, text: &str) {
        for ch in text.chars() {
            assert!(
                handle_key(
                    model,
                    field,
                    &Key::Character(ch.to_string()),
                    Some(&ch.to_string()),
                    true
                )
                .is_none()
            );
        }
    }

    #[test]
    fn a_prompt_keeps_the_field_open() {
        let mut model = model();
        let mut field = PasswordField::default();
        apply_pam_event(
            &mut model,
            &mut field,
            &pam::Event::Said(pam::Message::Prompt {
                text: "Password:".to_string(),
                secret: true,
            }),
        );
        assert_eq!(model.label, "Password");
        assert!(matches!(field.state(), FieldState::Idle));
        typed(&mut model, &mut field, "a");
        assert_eq!(field.masked(), "•");
    }

    #[test]
    fn enter_blocks_input_until_the_attempt_ends() {
        let mut model = model();
        let mut field = PasswordField::default();
        typed(&mut model, &mut field, "a");
        let secret = handle_key(
            &mut model,
            &mut field,
            &Key::Named(NamedKey::Enter),
            None,
            true,
        )
        .expect("submit a non-empty field");
        assert_eq!(secret.as_str(), "a");
        assert!(matches!(field.state(), FieldState::Verifying));
        assert!(
            handle_key(
                &mut model,
                &mut field,
                &Key::Character("b".to_string()),
                Some("b"),
                true,
            )
            .is_none()
        );
        assert_eq!(field.masked(), "");
    }

    #[test]
    fn a_refusal_clears_the_field_and_accepts_keys_again() {
        let mut model = model();
        let mut field = PasswordField::default();
        typed(&mut model, &mut field, "a");
        let _ = handle_key(
            &mut model,
            &mut field,
            &Key::Named(NamedKey::Enter),
            None,
            true,
        );
        apply_pam_event(
            &mut model,
            &mut field,
            &pam::Event::Ended(pam::Outcome::Denied("Authentication failure".to_string())),
        );
        assert!(matches!(field.state(), FieldState::Error(_)));
        assert_eq!(field.masked(), "");
        assert_eq!(model.status.as_deref(), Some("Authentication failure"));

        typed(&mut model, &mut field, "b");
        assert!(matches!(field.state(), FieldState::Idle));
        assert_eq!(field.masked(), "•");
        assert_eq!(model.status, None);
    }

    #[test]
    fn a_second_attempt_submits_the_new_secret() {
        let mut model = model();
        let mut field = PasswordField::default();
        typed(&mut model, &mut field, "old");
        let _ = handle_key(
            &mut model,
            &mut field,
            &Key::Named(NamedKey::Enter),
            None,
            true,
        );
        apply_pam_event(
            &mut model,
            &mut field,
            &pam::Event::Ended(pam::Outcome::Denied("no".to_string())),
        );

        typed(&mut model, &mut field, "new");
        let secret = handle_key(
            &mut model,
            &mut field,
            &Key::Named(NamedKey::Enter),
            None,
            true,
        )
        .expect("submit the second secret");
        assert_eq!(secret.as_str(), "new");
    }

    #[test]
    fn an_error_also_reopens_the_field() {
        let mut model = model();
        let mut field = PasswordField::default();
        typed(&mut model, &mut field, "a");
        let _ = handle_key(
            &mut model,
            &mut field,
            &Key::Named(NamedKey::Enter),
            None,
            true,
        );
        apply_pam_event(
            &mut model,
            &mut field,
            &pam::Event::Said(pam::Message::Error("try again".to_string())),
        );
        typed(&mut model, &mut field, "b");
        assert_eq!(field.masked(), "•");
    }

    #[test]
    fn authenticated_asks_for_the_unlock() {
        let mut model = model();
        let mut field = PasswordField::default();
        assert_eq!(
            apply_pam_event(
                &mut model,
                &mut field,
                &pam::Event::Ended(pam::Outcome::Authenticated),
            ),
            Action::Unlock
        );
    }
}
