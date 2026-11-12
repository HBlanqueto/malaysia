use std::collections::BTreeMap;

use bk::app::{load_restoration, use_defaults};
use bk::patterns::settings::Pane;
use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::views::form::{Form, Row, Section};
use ctappearance::notifications::{
    AlertStyle, AppNotifications, Focus, Grouping, NotificationsConfig, ShowPreviews,
};
use ctdesign::{ACCENT_SWATCHES, Rgba};
use serde::Deserialize;

use crate::app::position;
use crate::localize::{intern, tr, tr_list};
use crate::state::edit_defaults;

const DOMAIN: &str = "notifications";
const TINT: Rgba = ACCENT_SWATCHES[3].1;
const FOCUS_TINT: Rgba = ACCENT_SWATCHES[1].1;
const PREVIEWS: [ShowPreviews; 3] = [
    ShowPreviews::Always,
    ShowPreviews::WhenUnlocked,
    ShowPreviews::Never,
];
const GROUPINGS: [Grouping; 2] = [Grouping::Automatic, Grouping::Off];
const STYLES: [AlertStyle; 2] = [AlertStyle::Temporary, AlertStyle::Persistent];

#[derive(Default, Deserialize)]
#[serde(default)]
struct History {
    notifications: Vec<Seen>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Seen {
    app_id: String,
    app_name: String,
}

fn edit<T: 'static>(
    error: State<Option<String>>,
    apply: impl Fn(&mut NotificationsConfig, T) + 'static,
) -> Callback<T, ()> {
    edit_defaults(DOMAIN, error, apply)
}

fn known_apps(config: &NotificationsConfig) -> BTreeMap<String, String> {
    let history: History = load_restoration(DOMAIN);
    let mut apps: BTreeMap<String, String> = config
        .apps
        .keys()
        .map(|id| (id.clone(), id.clone()))
        .collect();
    for seen in history.notifications {
        apps.insert(seen.app_id, seen.app_name);
    }
    apps
}

pub fn panes(error: State<Option<String>>) -> [Pane; 2] {
    let config = use_defaults::<NotificationsConfig>(DOMAIN).read().clone();
    let apps = known_apps(&config);
    [
        notifications_pane(&config, &apps, error),
        focus_pane(&config, &apps, error),
    ]
}

fn notifications_pane(
    config: &NotificationsConfig,
    apps: &BTreeMap<String, String>,
    error: State<Option<String>>,
) -> Pane {
    let general = Section::titled(tr!("notification-center"))
        .row(
            Row::new(tr!("notifications-when-locked")).control(
                Switch::new(tr!("notifications-when-locked"))
                    .on(config.when_locked)
                    .on_toggle(edit(error, |config, on: bool| config.when_locked = on)),
            ),
        )
        .row(
            Row::new(tr!("notifications-show-previews")).control(
                tr_list!(
                    "previews-always",
                    "previews-when-unlocked",
                    "previews-never"
                )
                .iter()
                .fold(
                    PopUpButton::new(tr!("notifications-show-previews")),
                    |popup, name| popup.item(name),
                )
                .selected(position(&PREVIEWS, config.show_previews))
                .on_change(edit(error, |config, index: usize| {
                    config.show_previews = PREVIEWS[index]
                })),
            ),
        );
    let list = if apps.is_empty() {
        Section::titled(tr!("notifications-apps")).row(Row::new(tr!("notifications-no-apps")))
    } else {
        apps.iter().fold(
            Section::titled(tr!("notifications-apps")),
            |section, (id, name)| {
                section.row(
                    Row::page(intern(name.clone()), intern(format!("notifications-{id}")))
                        .detail(summary(&config.app(id))),
                )
            },
        )
    };
    let pane = Pane::new(
        "notifications",
        tr!("notifications"),
        Symbol::Bell,
        TINT,
        Form::new().section(general).section(list),
    );
    apps.iter().fold(pane, |pane, (id, name)| {
        let app = config.app(id);
        pane.page(Pane::new(
            intern(format!("notifications-{id}")),
            intern(name.clone()),
            Symbol::Bell,
            TINT,
            app_form(intern(id.clone()), &app, error),
        ))
    })
}

fn summary(app: &AppNotifications) -> &'static str {
    if !app.allow {
        return tr!("notifications-summary-off");
    }
    let mut parts = Vec::new();
    if app.desktop {
        parts.push(match app.style {
            AlertStyle::Temporary => tr!("notifications-summary-temporary"),
            AlertStyle::Persistent => tr!("notifications-summary-persistent"),
        });
    }
    if app.sound {
        parts.push(tr!("notifications-summary-sound"));
    }
    intern(parts.join(", "))
}

fn app_form(id: &'static str, app: &AppNotifications, error: State<Option<String>>) -> Form {
    let switch = |title: &'static str, on: bool, apply: fn(&mut AppNotifications, bool)| {
        Row::new(title).control(
            Switch::new(title)
                .on(on)
                .on_toggle(edit(error, move |config, on: bool| {
                    apply(config.app_mut(id), on)
                })),
        )
    };
    let place = |title: &'static str, on: bool, apply: fn(&mut AppNotifications, bool)| {
        Row::new(title).control(
            Checkbox::new(title)
                .checked(on)
                .on_toggle(edit(error, move |config, on: bool| {
                    apply(config.app_mut(id), on)
                })),
        )
    };
    Form::new()
        .section(
            Section::new().row(switch(tr!("notifications-allow"), app.allow, |app, on| {
                app.allow = on
            })),
        )
        .section(
            Section::titled(tr!("notifications-alert-style"))
                .row(place(
                    tr!("notifications-desktop"),
                    app.desktop,
                    |app, on| app.desktop = on,
                ))
                .row(place(tr!("notification-center"), app.center, |app, on| {
                    app.center = on
                }))
                .row(place(
                    tr!("notifications-lock-screen"),
                    app.lock_screen,
                    |app, on| app.lock_screen = on,
                ))
                .row(
                    Row::new(tr!("notifications-style")).control(
                        SegmentedControl::new(tr!("notifications-style"))
                            .segment(tr!("notifications-temporary"))
                            .segment(tr!("notifications-persistent"))
                            .selected(position(&STYLES, app.style))
                            .on_change(edit(error, move |config, index: usize| {
                                config.app_mut(id).style = STYLES[index];
                            })),
                    ),
                ),
        )
        .section(
            Section::new()
                .row(switch(
                    tr!("notifications-critical"),
                    app.critical,
                    |app, on| app.critical = on,
                ))
                .row(switch(
                    tr!("notifications-time-sensitive"),
                    app.time_sensitive,
                    |app, on| app.time_sensitive = on,
                ))
                .row(switch(tr!("notifications-sound"), app.sound, |app, on| {
                    app.sound = on
                }))
                .row(switch(tr!("notifications-badge"), app.badge, |app, on| {
                    app.badge = on
                }))
                .row(
                    Row::new(tr!("notifications-grouping")).control(
                        tr_list!("grouping-automatic", "grouping-off")
                            .iter()
                            .fold(
                                PopUpButton::new(tr!("notifications-grouping")),
                                |popup, name| popup.item(name),
                            )
                            .selected(position(&GROUPINGS, app.grouping))
                            .on_change(edit(error, move |config, index: usize| {
                                config.app_mut(id).grouping = GROUPINGS[index];
                            })),
                    ),
                ),
        )
}

fn focus_parts(focus: Focus) -> (&'static str, &'static str, Symbol) {
    match focus {
        Focus::DoNotDisturb => (
            "focus-do-not-disturb",
            tr!("focus-do-not-disturb"),
            Symbol::Moon,
        ),
        Focus::Sleep => ("focus-sleep", tr!("focus-sleep"), Symbol::Bed),
        Focus::Personal => ("focus-personal", tr!("focus-personal"), Symbol::Person),
        Focus::Work => ("focus-work", tr!("focus-work"), Symbol::Briefcase),
    }
}

fn focus_pane(
    config: &NotificationsConfig,
    apps: &BTreeMap<String, String>,
    error: State<Option<String>>,
) -> Pane {
    let list = Focus::ALL
        .into_iter()
        .fold(Section::new(), |section, focus| {
            let (id, title, symbol) = focus_parts(focus);
            section.row(Row::page(title, id).symbol(symbol, FOCUS_TINT))
        });
    let pane = Pane::new(
        "focus",
        tr!("focus"),
        Symbol::Moon,
        FOCUS_TINT,
        Form::new().section(list),
    );
    Focus::ALL.into_iter().fold(pane, |pane, focus| {
        let (id, title, symbol) = focus_parts(focus);
        pane.page(Pane::new(
            id,
            title,
            symbol,
            FOCUS_TINT,
            focus_form(focus, config, apps, error),
        ))
    })
}

fn focus_form(
    focus: Focus,
    config: &NotificationsConfig,
    apps: &BTreeMap<String, String>,
    error: State<Option<String>>,
) -> Form {
    let settings = config.focus(focus);
    let allowed = if apps.is_empty() {
        Section::titled(tr!("focus-allowed-apps")).row(Row::new(tr!("notifications-no-apps")))
    } else {
        apps.iter().fold(
            Section::titled(tr!("focus-allowed-apps")),
            |section, (id, name)| {
                let id = intern(id.clone());
                let name = intern(name.clone());
                section.row(
                    Row::new(name).control(
                        Checkbox::new(name)
                            .checked(settings.allowed_apps.iter().any(|allowed| allowed == id))
                            .on_toggle(edit(error, move |config, on: bool| {
                                let allowed = &mut config.focus_mut(focus).allowed_apps;
                                allowed.retain(|app| app != id);
                                if on {
                                    allowed.push(id.to_string());
                                }
                            })),
                    ),
                )
            },
        )
    };
    Form::new().section(allowed).section(
        Section::new().row(
            Row::new(tr!("focus-time-sensitive")).control(
                Switch::new(tr!("focus-time-sensitive"))
                    .on(settings.time_sensitive)
                    .on_toggle(edit(error, move |config, on: bool| {
                        config.focus_mut(focus).time_sensitive = on;
                    })),
            ),
        ),
    )
}
