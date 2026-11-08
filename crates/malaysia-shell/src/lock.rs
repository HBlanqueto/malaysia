use std::{cell::RefCell, rc::Rc};

use async_channel::{Receiver, Sender};
use bk::password::{PasswordField, State as FieldState};
use bk::prelude::*;
use bk::shell::{SessionUnlock, run_lock};
use ctdesign::{
    LOCK_FIELD_HEIGHT, LOCK_FIELD_WIDTH, LOCK_FONT_SIZE, LOCK_LABEL_FONT_SIZE, LOCK_LABEL_OFFSET,
    LOCK_MASK_FONT_SIZE, LOCK_TITLE_SCALE,
};
use freya_core::integration::AppComponent;

use crate::localize::tr;
use crate::pam;
use bk::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Continue,
    Unlock,
}

fn apply_pam_event(label: &mut String, field: &mut PasswordField, event: &pam::Event) -> Action {
    match event {
        pam::Event::Said(pam::Message::Prompt(text)) => {
            if *field.state() == FieldState::Verifying {
                field.reset();
            }
            *label = pam::prompt_label(text);
            Action::Continue
        }
        pam::Event::Said(pam::Message::Info(text)) => {
            eprintln!("lock: PAM: {text}");
            Action::Continue
        }
        pam::Event::Said(pam::Message::Error(text)) => {
            field.set_error(text.clone());
            Action::Continue
        }
        pam::Event::Ended(pam::Outcome::Authenticated) => Action::Unlock,
        pam::Event::Ended(pam::Outcome::Denied(message)) => {
            eprintln!("lock: PAM refused: {message}");
            field.set_error(message.clone());
            Action::Continue
        }
    }
}

struct LockApp {
    label: State<String>,
    field: State<PasswordField>,
    events: Sender<pam::Event>,
    received: Receiver<pam::Event>,
    attempt: Rc<RefCell<Option<pam::Attempt>>>,
    user: String,
}

impl App for LockApp {
    fn render(&self) -> impl IntoElement {
        let label_state = self.label;
        let mut field = self.field;
        let received = self.received.clone();
        let attempt = self.attempt.clone();
        let attempt_for_events = attempt.clone();
        let events = self.events.clone();
        let user = self.user.clone();
        let unlock = SessionUnlock::get();

        use_future(move || {
            let received = received.clone();
            let attempt = attempt_for_events.clone();
            let unlock = unlock.clone();
            let mut label = label_state;
            let mut field = field;
            async move {
                while let Ok(event) = received.recv().await {
                    let action = apply_pam_event(&mut label.write(), &mut field.write(), &event);
                    if matches!(event, pam::Event::Ended(pam::Outcome::Denied(_))) {
                        *attempt.borrow_mut() = None;
                    }
                    if action == Action::Unlock {
                        unlock.unlock();
                    }
                }
            }
        });

        let view = self.field.read();
        let error = match view.state() {
            FieldState::Error(message) => Some(message.clone()),
            _ => None,
        };
        let masked = view.masked();
        let palette = bk::theme::lock_palette(&bk::theme::resolved());
        let background = theme::color(palette.bg_surface);
        let ink = theme::color(palette.ink);
        let muted = theme::color(palette.ink_muted);
        let accent = theme::color(palette.accent);
        let danger = theme::color(palette.danger);

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(background)
            .direction(Direction::Vertical)
            .main_align(Alignment::Center)
            .cross_align(Alignment::Center)
            .spacing(LOCK_LABEL_OFFSET as f32)
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let secret = field.write().handle_key(&event.key, event.modifiers);
                if let Some(secret) = secret {
                    start_attempt(&attempt, &events, &user);
                    if let Some(attempt) = attempt.borrow().as_ref() {
                        attempt.answer(secret);
                    }
                }
            })
            .child(
                label()
                    .text(tr!("lock-title"))
                    .font_size(LOCK_FONT_SIZE * LOCK_TITLE_SCALE)
                    .color(ink),
            )
            .child(
                label()
                    .text(self.label.read().clone())
                    .font_size(LOCK_LABEL_FONT_SIZE)
                    .color(muted),
            )
            .child(
                rect()
                    .width(Size::px(LOCK_FIELD_WIDTH as f32))
                    .height(Size::px(LOCK_FIELD_HEIGHT as f32))
                    .center()
                    .background(if error.is_some() { danger } else { accent })
                    .corner_radius((LOCK_FIELD_HEIGHT / 4) as f32)
                    .child(
                        label()
                            .text(if masked.is_empty() {
                                tr!("lock-password").to_string()
                            } else {
                                masked
                            })
                            .font_size(LOCK_MASK_FONT_SIZE)
                            .color(background),
                    ),
            )
            .child(match error {
                Some(text) => label().text(text).color(danger),
                None => label().text(" ").color(muted),
            })
    }
}

pub fn run() -> anyhow::Result<()> {
    let (events, received) = async_channel::unbounded();
    let attempt = Rc::new(RefCell::new(None));
    let user = pam::current_user().unwrap_or_default();
    start_attempt(&attempt, &events, &user);
    let app = AppComponent::new(LockApp {
        label: State::create_global(tr!("lock-password").to_string()),
        field: State::create_global(PasswordField::default()),
        events,
        received,
        attempt,
        user,
    });
    run_lock(app).map_err(|error| anyhow::anyhow!("{error}"))
}

fn start_attempt(attempt: &RefCell<Option<pam::Attempt>>, events: &Sender<pam::Event>, user: &str) {
    if attempt.borrow().is_some() {
        return;
    }
    if user.is_empty() {
        eprintln!("lock: cannot resolve the current user for PAM");
        let _ = events.try_send(pam::Event::Ended(pam::Outcome::Denied(
            tr!("lock-unknown-user").to_string(),
        )));
        return;
    }
    eprintln!("lock: starting PAM for {user}");
    *attempt.borrow_mut() = Some(pam::Attempt::start(events.clone(), user.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(field: &mut PasswordField, text: &str) {
        text.chars().for_each(|ch| field.insert(ch));
    }

    #[test]
    fn a_prompt_keeps_the_field_open() {
        let mut label = String::new();
        let mut field = PasswordField::default();
        apply_pam_event(
            &mut label,
            &mut field,
            &pam::Event::Said(pam::Message::Prompt("Password:".to_string())),
        );
        assert_eq!(label, "Password");
        assert!(matches!(field.state(), FieldState::Idle));
        typed(&mut field, "a");
        assert_eq!(field.masked(), "•");
    }

    #[test]
    fn a_second_prompt_after_submit_accepts_keys_again() {
        let mut label = String::new();
        let mut field = PasswordField::default();
        typed(&mut field, "a");
        field.take().expect("submit a non-empty field");
        apply_pam_event(
            &mut label,
            &mut field,
            &pam::Event::Said(pam::Message::Prompt("Verification code:".to_string())),
        );
        assert!(matches!(field.state(), FieldState::Idle));
        typed(&mut field, "1");
        assert_eq!(field.masked(), "•");
    }

    #[test]
    fn a_refusal_clears_the_field_and_accepts_keys_again() {
        let mut label = String::new();
        let mut field = PasswordField::default();
        typed(&mut field, "a");
        let _ = field.take();
        apply_pam_event(
            &mut label,
            &mut field,
            &pam::Event::Ended(pam::Outcome::Denied("Authentication failure".to_string())),
        );
        assert_eq!(field.masked(), "");
        assert_eq!(
            field.state(),
            &FieldState::Error("Authentication failure".to_string())
        );

        typed(&mut field, "b");
        assert!(matches!(field.state(), FieldState::Idle));
        assert_eq!(field.masked(), "•");
    }

    #[test]
    fn a_second_attempt_submits_the_new_secret() {
        let mut label = String::new();
        let mut field = PasswordField::default();
        typed(&mut field, "old");
        let _ = field.take();
        apply_pam_event(
            &mut label,
            &mut field,
            &pam::Event::Ended(pam::Outcome::Denied("no".to_string())),
        );

        typed(&mut field, "new");
        let secret = field.take().expect("submit the second secret");
        assert_eq!(secret.as_str(), "new");
    }

    #[test]
    fn an_error_also_reopens_the_field() {
        let mut label = String::new();
        let mut field = PasswordField::default();
        typed(&mut field, "a");
        let _ = field.take();
        apply_pam_event(
            &mut label,
            &mut field,
            &pam::Event::Said(pam::Message::Error("try again".to_string())),
        );
        typed(&mut field, "b");
        assert_eq!(field.masked(), "•");
    }

    #[test]
    fn authenticated_asks_for_the_unlock() {
        let mut label = String::new();
        let mut field = PasswordField::default();
        assert_eq!(
            apply_pam_event(
                &mut label,
                &mut field,
                &pam::Event::Ended(pam::Outcome::Authenticated),
            ),
            Action::Unlock
        );
    }
}
