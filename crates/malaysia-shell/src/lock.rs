use std::path::PathBuf;
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

use async_channel::{Receiver, Sender};
use bk::app::use_defaults;
use bk::password::{PasswordField, State as FieldState};
use bk::platform::Timers;
use bk::prelude::*;
use bk::shell::{BlurRegion, SessionUnlock, run_lock, set_blur_region};
use chrono::Datelike;
use ctappearance::notifications::NotificationsConfig;
use ctdesign::{
    BODY, DURATION_FAST_MS, LOCK_AVATAR_FILL, LOCK_AVATAR_SIZE, LOCK_BOTTOM_MARGIN,
    LOCK_CLOCK_FONT_SIZE, LOCK_DATE_FONT_SIZE, LOCK_FIELD_ERROR, LOCK_FIELD_FILL,
    LOCK_FIELD_HEIGHT, LOCK_FIELD_INK, LOCK_FIELD_MAX_WIDTH, LOCK_FIELD_MIN_WIDTH, LOCK_INK,
    LOCK_KEYBOARD_WIDTH, LOCK_LOGIN_MS, LOCK_MASK_FONT_SIZE, LOCK_RISE_MS, LOCK_RISE_OFFSET,
    LOCK_SPACING, LOCK_TOP_PERCENT, MENUBAR_HEIGHT, MENUBAR_TITLE_PADDING_X, NOTIFICATION_GAP,
    NOTIFICATION_WIDTH, SYMBOL, TITLE_2,
};
use freya_animation::prelude::{AnimNum, Function, OnChange, use_animation_with_dependencies};
use freya_components::image_viewer::ImageViewer;
use freya_core::integration::AppComponent;
use malaysia_services::accounts::Accounts;
use malaysia_services::bus::use_service;
use torin::prelude::{Gaps, Position};

use crate::localize::tr;
use crate::menubar::lock_row::{power_icon, slot, status_row};
use crate::menubar::{bar::schedule_next_minute, system::user_display_name};
use crate::notifications::{Center, Notification};
use crate::{pam, power};
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
    clock: State<String>,
    leaving: State<bool>,
    power_menu: State<bool>,
}

// ADR 0074: the login appears once the clock has mostly risen.
const LOGIN_DELAY_MS: u64 = LOCK_RISE_MS * 3 / 5;
const ENTRY_MS: u64 = LOGIN_DELAY_MS + LOCK_LOGIN_MS;

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
        let timers = Timers::get();
        let leaving = self.leaving;

        let clock = self.clock;
        let minute_timers = timers.clone();
        use_hook(move || schedule_next_minute(minute_timers, clock));

        use_future(move || {
            let received = received.clone();
            let attempt = attempt_for_events.clone();
            let unlock = unlock.clone();
            let timers = timers.clone();
            let mut label = label_state;
            let mut field = field;
            let mut leaving = leaving;
            async move {
                while let Ok(event) = received.recv().await {
                    let action = apply_pam_event(&mut label.write(), &mut field.write(), &event);
                    if matches!(event, pam::Event::Ended(pam::Outcome::Denied(_))) {
                        *attempt.borrow_mut() = None;
                    }
                    if action == Action::Unlock {
                        leaving.set(true);
                        let unlock = unlock.clone();
                        timers.after(Duration::from_millis(DURATION_FAST_MS), move || {
                            unlock.unlock();
                        });
                    }
                }
            }
        });

        let resolved = theme::resolved();
        let shown = SessionUnlock::get().locked();
        let entry = use_animation_with_dependencies(&shown, |conf, shown| {
            conf.on_change(OnChange::Rerun);
            let total = if *shown { ENTRY_MS } else { 0 };
            AnimNum::new(0.0, total as f32)
                .time(total)
                .function(Function::Linear)
        });
        let leave = use_animation_with_dependencies(&*self.leaving.read(), |conf, leaving| {
            conf.on_change(OnChange::Rerun);
            let total = if *leaving { DURATION_FAST_MS } else { 0 };
            AnimNum::new(0.0, total as f32)
                .time(total)
                .function(Function::Linear)
        });
        let elapsed = if shown { entry.get().value() } else { 0.0 };
        let stay = 1.0 - progress(leave.get().value(), 0, DURATION_FAST_MS);
        let rise = ease_out(progress(elapsed, 0, LOCK_RISE_MS));
        let login = ease_out(progress(elapsed, LOGIN_DELAY_MS, LOCK_LOGIN_MS)) * stay;
        let power = ease_out(progress(elapsed, 0, LOCK_LOGIN_MS));
        let rise_offset = if resolved.reduced_motion {
            0.0
        } else {
            (1.0 - rise) * LOCK_RISE_OFFSET
        };

        let account = use_service::<Accounts>().snapshot.read().clone();
        let mut hint_shown = use_state(|| false);
        let keyboard_layout = SessionUnlock::get().keyboard_layout();
        let name = use_hook(|| user_display_name().unwrap_or_else(|| self.user.clone()));
        let notices = lock_notices();
        let mut power_menu = self.power_menu;

        let on_key = move |event: Event<KeyboardEventData>| {
            let secret = field.write().handle_key(&event.key, event.modifiers);
            if let Some(secret) = secret {
                start_attempt(&attempt, &events, &user);
                if let Some(attempt) = attempt.borrow().as_ref() {
                    attempt.answer(secret);
                }
            }
        };

        let power_button = rect()
            .opacity(power)
            .on_press(move |_| {
                let open = *power_menu.read();
                power_menu.set(!open);
            })
            .a11y_alt(tr!("lock-power-menu"))
            .child(power_icon(theme::color(LOCK_INK)))
            .into_element();
        let row = rect()
            .position(Position::new_absolute())
            .width(Size::fill())
            .height(Size::px(MENUBAR_HEIGHT as f32))
            .child(status_row(
                HStack::new()
                    .height(Size::fill())
                    .spacing(0.0)
                    .child(keyboard_indicator(keyboard_layout).opacity(power))
                    .child(slot(power_button))
                    .into_element(),
            ));

        let header =
            VStack::new()
                .position(Position::new_absolute())
                .width(Size::fill())
                .height(Size::fill())
                .cross_align(Alignment::Center)
                .offset_y(rise_offset)
                .opacity(rise * stay)
                .child(rect().height(Size::percent(LOCK_TOP_PERCENT)))
                .child(
                    Text::new(short_date(chrono::Local::now().date_naive()))
                        .font(TITLE_2)
                        .font_size(LOCK_DATE_FONT_SIZE)
                        .color(theme::color(LOCK_INK)),
                )
                .child(
                    Text::new(self.clock.read().clone())
                        .font(ctdesign::LARGE_TITLE)
                        .font_size(LOCK_CLOCK_FONT_SIZE)
                        .color(theme::color(LOCK_INK)),
                )
                .maybe_child((!notices.is_empty()).then(|| {
                    VStack::new()
                        .width(Size::px(NOTIFICATION_WIDTH as f32))
                        .margin(Gaps::new(LOCK_SPACING, 0.0, 0.0, 0.0))
                        .spacing(NOTIFICATION_GAP)
                        .opacity(login)
                        .children(notices.iter().take(LOCK_NOTICES).map(
                            |(notification, hidden)| {
                                crate::notifications::lock_card(notification, *hidden)
                            },
                        ))
                }));

        let view = self.field.read();
        let error = match view.state() {
            FieldState::Error(message) => Some(message.clone()),
            _ => None,
        };
        let hint = account
            .as_ref()
            .and_then(|account| account.password_hint.clone());
        let masked = view.masked();
        let typing = !masked.is_empty();
        let fill = if error.is_some() {
            LOCK_FIELD_ERROR
        } else {
            LOCK_FIELD_FILL
        };
        let field_view = rect()
            .width(Size::px(if typing {
                LOCK_FIELD_MAX_WIDTH
            } else {
                LOCK_FIELD_MIN_WIDTH
            }))
            .height(Size::px(LOCK_FIELD_HEIGHT))
            .corner_radius(LOCK_FIELD_HEIGHT / 2.0)
            .background(theme::color(fill))
            .main_align(Alignment::Center)
            .padding(Gaps::new(
                0.0,
                LOCK_FIELD_HEIGHT / 2.0,
                0.0,
                LOCK_FIELD_HEIGHT / 2.0,
            ))
            .on_sized(|event: Event<SizedEventData>| {
                let area = event.data().area;
                set_blur_region(BlurRegion {
                    rects: vec![(
                        area.origin.x.round() as i32,
                        area.origin.y.round() as i32,
                        area.size.width.round() as i32,
                        area.size.height.round() as i32,
                    )],
                });
            })
            .child(if typing {
                Text::new(masked)
                    .font_size(LOCK_MASK_FONT_SIZE)
                    .color(theme::color(LOCK_INK))
            } else {
                Text::new(self.label.read().clone())
                    .font(BODY)
                    .color(theme::color(LOCK_FIELD_INK))
            });
        let login_view = VStack::new()
            .position(Position::new_absolute().bottom(LOCK_BOTTOM_MARGIN))
            .width(Size::fill())
            .cross_align(Alignment::Center)
            .spacing(LOCK_SPACING)
            .opacity(login)
            .child(avatar(
                account.as_ref().and_then(|account| account.picture.clone()),
                &name,
            ))
            .child(
                Text::new(name.clone())
                    .font(TITLE_2)
                    .color(theme::color(LOCK_INK)),
            )
            .child(match &hint {
                Some(_) => HStack::new()
                    .cross_align(Alignment::Center)
                    .spacing(LOCK_SPACING)
                    .child(rect().width(Size::px(LOCK_FIELD_HEIGHT)))
                    .child(field_view)
                    .child(hint_button(hint_shown))
                    .into_element(),
                None => field_view.into_element(),
            })
            .child(
                Text::new(
                    error
                        .or_else(|| hint.filter(|_| *hint_shown.read()))
                        .unwrap_or_else(|| " ".to_string()),
                )
                .font(BODY)
                .color(theme::color(LOCK_INK)),
            );

        let mut root = rect()
            .width(Size::fill())
            .height(Size::fill())
            .on_global_key_down(on_key)
            .child(header)
            .child(login_view)
            .child(row);
        if *self.power_menu.read() {
            root = root.child(power_options(self.power_menu));
        }
        for family in ctdesign::SANS_FAMILIES {
            root = root.font_family(family);
        }
        root
    }
}

fn avatar(picture: Option<String>, name: &str) -> Element {
    let size = LOCK_AVATAR_SIZE;
    let frame = rect()
        .width(Size::px(size))
        .height(Size::px(size))
        .corner_radius(size / 2.0)
        .overflow(Overflow::Clip);
    match picture.map(PathBuf::from).filter(|path| path.exists()) {
        Some(path) => frame
            .child(
                ImageViewer::new(path)
                    .width(Size::fill())
                    .height(Size::fill())
                    .aspect_ratio(AspectRatio::Max)
                    .image_cover(ImageCover::Center),
            )
            .into_element(),
        None => frame
            .background(theme::color(LOCK_AVATAR_FILL))
            .center()
            .child(
                Text::new(initial(name))
                    .font(TITLE_2)
                    .font_size(size / 2.0)
                    .color(theme::color(LOCK_INK)),
            )
            .into_element(),
    }
}

fn initial(name: &str) -> String {
    name.chars()
        .next()
        .map(|first| first.to_uppercase().collect())
        .unwrap_or_default()
}

fn power_options(mut open: State<bool>) -> Element {
    let resolved = theme::resolved();
    let palette = theme::menu_palette(&resolved, theme::SurfaceKind::LayerShell);
    let item = |label: String, action: fn()| {
        rect()
            .width(Size::fill())
            .height(Size::px(palette.item_height))
            .padding(Gaps::new(
                0.0,
                palette.item_padding_x,
                0.0,
                palette.item_padding_x,
            ))
            .corner_radius(palette.item_radius)
            .main_align(Alignment::Center)
            .on_press(move |_| {
                open.set(false);
                action();
            })
            .child(
                Text::new(label)
                    .font_size(palette.font_size)
                    .color(theme::color(palette.fg)),
            )
    };
    bk::style::panel_edges(
        rect()
            .width(Size::px(palette.min_width))
            .padding(palette.padding)
            .child(item(tr!("system-sleep").to_string(), power::suspend))
            .child(item(tr!("power-restart-button").to_string(), || {
                power::run(power::PowerAction::Restart)
            }))
            .child(item(tr!("power-shut-down-button").to_string(), || {
                power::run(power::PowerAction::ShutDown)
            })),
        palette.bar_title_open_radius.max(palette.item_radius * 2.0),
        palette.bg,
    )
    .position(
        Position::new_absolute()
            .top(MENUBAR_HEIGHT as f32)
            .right(ctdesign::MENUBAR_MARGIN_RIGHT as f32),
    )
    .into_element()
}

fn short_date(today: chrono::NaiveDate) -> String {
    tr!(
        "lock-date",
        weekday = weekday_name(today.weekday()),
        day = today.day(),
        month = month_name(today.month())
    )
    .to_string()
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => tr!("month-short-1"),
        2 => tr!("month-short-2"),
        3 => tr!("month-short-3"),
        4 => tr!("month-short-4"),
        5 => tr!("month-short-5"),
        6 => tr!("month-short-6"),
        7 => tr!("month-short-7"),
        8 => tr!("month-short-8"),
        9 => tr!("month-short-9"),
        10 => tr!("month-short-10"),
        11 => tr!("month-short-11"),
        _ => tr!("month-short-12"),
    }
}

fn keyboard_indicator(layout: Option<String>) -> HStack {
    let row = HStack::new()
        .width(Size::px(LOCK_KEYBOARD_WIDTH))
        .height(Size::fill())
        .main_align(Alignment::End)
        .cross_align(Alignment::Center)
        .spacing(MENUBAR_TITLE_PADDING_X);
    let Some(layout) = layout.filter(|layout| !layout.is_empty()) else {
        return row;
    };
    let label = layout_label(&layout);
    row.a11y_alt(tr!("lock-keyboard-layout", layout = label.clone()))
        .child(
            Text::new(label)
                .font(BODY)
                .line_limit(1)
                .color(theme::color(LOCK_INK)),
        )
        .child(Element::from(
            Image::system(Symbol::Keyboard)
                .size(SYMBOL.grid)
                .color(LOCK_INK),
        ))
}

fn layout_label(layout: &str) -> String {
    let slug = layout
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let id = format!("keyboard-layout-{slug}");
    if crate::localize::LOADER.has(&id) {
        crate::localize::LOADER.get(&id)
    } else {
        layout.to_string()
    }
}

fn hint_button(mut shown: State<bool>) -> Element {
    rect()
        .width(Size::px(LOCK_FIELD_HEIGHT))
        .height(Size::px(LOCK_FIELD_HEIGHT))
        .corner_radius(LOCK_FIELD_HEIGHT / 2.0)
        .background(theme::color(LOCK_FIELD_FILL))
        .center()
        .a11y_alt(tr!("lock-password-hint"))
        .on_press(move |_| {
            let open = *shown.read();
            shown.set(!open);
        })
        .child(Element::from(
            Image::system(Symbol::QuestionMark)
                .size(SYMBOL.grid)
                .color(LOCK_INK),
        ))
        .into_element()
}

fn weekday_name(weekday: chrono::Weekday) -> &'static str {
    match weekday {
        chrono::Weekday::Mon => tr!("weekday-short-1"),
        chrono::Weekday::Tue => tr!("weekday-short-2"),
        chrono::Weekday::Wed => tr!("weekday-short-3"),
        chrono::Weekday::Thu => tr!("weekday-short-4"),
        chrono::Weekday::Fri => tr!("weekday-short-5"),
        chrono::Weekday::Sat => tr!("weekday-short-6"),
        chrono::Weekday::Sun => tr!("weekday-short-7"),
    }
}

fn progress(elapsed: f32, start: u64, length: u64) -> f32 {
    if length == 0 {
        return if elapsed >= start as f32 { 1.0 } else { 0.0 };
    }
    ((elapsed - start as f32) / length as f32).clamp(0.0, 1.0)
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

const LOCK_NOTICES: usize = 4;
const LOCK_NOTICES_POLL: Duration = Duration::from_secs(2);

fn lock_notices() -> Vec<(Notification, bool)> {
    let center = use_state(Center::load);
    use_hook(move || poll_center(Timers::get(), center));
    let config = use_defaults::<NotificationsConfig>("notifications");
    center.read().on_lock_screen(&config.read())
}

fn poll_center(timers: Timers, center: State<Center>) {
    let again = timers.clone();
    timers.after(LOCK_NOTICES_POLL, move || {
        let mut latest = center;
        latest.set_if_modified(Center::load());
        poll_center(again.clone(), center);
    });
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
        clock: State::create_global(chrono::Local::now().format("%H:%M").to_string()),
        leaving: State::create_global(false),
        power_menu: State::create_global(false),
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
