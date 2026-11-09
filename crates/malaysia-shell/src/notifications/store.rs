use ctappearance::notifications::{
    AlertStyle, AppNotifications, Focus, Grouping, NotificationsConfig, ShowPreviews,
};
use serde::{Deserialize, Serialize};

const STATE: &str = "notifications";
// GNOME keeps 10 per source (messageTray.js); a desk-sized history still has to stay small.
const KEPT_PER_APP: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Level {
    Passive,
    Active,
    TimeSensitive,
    Critical,
}

impl Level {
    pub(crate) fn from_urgency(urgency: Option<u8>) -> Self {
        match urgency {
            Some(0) => Self::Passive,
            Some(2) => Self::Critical,
            _ => Self::Active,
        }
    }

    pub(crate) fn from_priority(priority: &str) -> Self {
        match priority {
            "low" => Self::Passive,
            "high" => Self::TimeSensitive,
            "urgent" => Self::Critical,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Origin {
    Freedesktop,
    Gtk {
        id: String,
        default_action: Option<Action>,
    },
    Portal {
        id: String,
        default_action: Option<Action>,
    },
}

impl Origin {
    pub(crate) fn gtk_id(&self) -> Option<&str> {
        match self {
            Self::Gtk { id, .. } | Self::Portal { id, .. } => Some(id),
            Self::Freedesktop => None,
        }
    }

    pub(crate) fn default_action(&self) -> Option<&Action> {
        match self {
            Self::Gtk { default_action, .. } | Self::Portal { default_action, .. } => {
                default_action.as_ref()
            }
            Self::Freedesktop => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Action {
    pub(crate) key: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Notification {
    pub(crate) id: u32,
    pub(crate) app_id: String,
    pub(crate) app_name: String,
    pub(crate) icon: String,
    #[serde(default)]
    pub(crate) image: Option<String>,
    #[serde(default)]
    pub(crate) communication: bool,
    pub(crate) summary: String,
    pub(crate) body: String,
    pub(crate) actions: Vec<Action>,
    #[serde(default)]
    pub(crate) reply: Option<Action>,
    #[serde(default)]
    pub(crate) hide_on_lock: bool,
    #[serde(default)]
    pub(crate) hide_content_on_lock: bool,
    pub(crate) level: Level,
    pub(crate) posted: i64,
    pub(crate) origin: Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub(crate) kept: bool,
    pub(crate) banner: Option<AlertStyle>,
    pub(crate) sound: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusRule {
    pub(crate) allowed: bool,
    pub(crate) time_sensitive: bool,
}

impl FocusRule {
    pub(crate) fn of(
        focus: Option<Focus>,
        config: &NotificationsConfig,
        app_id: &str,
    ) -> Option<Self> {
        focus.map(|focus| {
            let settings = config.focus(focus);
            Self {
                allowed: settings
                    .allowed_apps
                    .iter()
                    .any(|allowed| allowed == app_id),
                time_sensitive: settings.time_sensitive,
            }
        })
    }
}

pub(crate) fn focus_name(focus: Option<Focus>) -> &'static str {
    match focus {
        None => "",
        Some(Focus::DoNotDisturb) => "do-not-disturb",
        Some(Focus::Sleep) => "sleep",
        Some(Focus::Personal) => "personal",
        Some(Focus::Work) => "work",
    }
}

pub(crate) fn parse_focus(name: &str) -> Option<Focus> {
    Focus::ALL
        .into_iter()
        .find(|focus| focus_name(Some(*focus)) == name)
}

pub(crate) struct Moment {
    pub(crate) focus: Option<FocusRule>,
    pub(crate) panel_open: bool,
    pub(crate) app_in_front: bool,
    pub(crate) transient: bool,
    pub(crate) now: i64,
}

// ADR 0068, decisions 5, 6, 8 and 10: the user's settings decide, not the app.
pub(crate) fn present(level: Level, app: &AppNotifications, moment: &Moment) -> Presentation {
    let silent = Presentation {
        kept: false,
        banner: None,
        sound: false,
    };
    if !app.allow {
        return silent;
    }
    let critical = level == Level::Critical && app.critical;
    let kept = (app.center || critical) && !moment.transient;
    let quiet = level == Level::Passive
        || (app.muted(moment.now) && !critical)
        || (!let_through(level, app, moment.focus) && !critical);
    let shown = app.desktop && !moment.panel_open && !moment.app_in_front && !quiet;
    let banner = (shown || critical && !moment.panel_open).then_some(if critical {
        AlertStyle::Persistent
    } else {
        app.style
    });
    Presentation {
        kept,
        banner,
        sound: banner.is_some() && (app.sound || critical),
    }
}

// ADR 0068: a Focus lets through its allowed apps and, if both agree, time-sensitive alerts.
fn let_through(level: Level, app: &AppNotifications, focus: Option<FocusRule>) -> bool {
    focus.is_none_or(|rule| {
        rule.allowed || (level == Level::TimeSensitive && rule.time_sensitive && app.time_sensitive)
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Center {
    pub(crate) focus: Option<Focus>,
    pub(crate) next_id: u32,
    pub(crate) notifications: Vec<Notification>,
}

impl Center {
    pub(crate) fn load() -> Self {
        bk::app::load_restoration(STATE)
    }

    pub(crate) fn save(&self) {
        super::image::prune(&self.notifications);
        if let Err(error) = bk::app::save_restoration(STATE, self) {
            tracing::warn!(%error, "notifications: could not save the history");
        }
    }

    pub(crate) fn allocate_id(&mut self, replaces: u32) -> u32 {
        if replaces != 0 {
            return replaces;
        }
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }

    pub(crate) fn keep(&mut self, notification: Notification) {
        self.notifications.retain(|kept| kept.id != notification.id);
        let app_id = notification.app_id.clone();
        self.notifications.insert(0, notification);
        let mut seen = 0;
        self.notifications.retain(|kept| {
            if kept.app_id != app_id {
                return true;
            }
            seen += 1;
            seen <= KEPT_PER_APP
        });
    }

    pub(crate) fn remove(&mut self, id: u32) -> Option<Notification> {
        let index = self.notifications.iter().position(|kept| kept.id == id)?;
        Some(self.notifications.remove(index))
    }

    pub(crate) fn gtk_id(&self, app_id: &str, gtk_id: &str) -> Option<u32> {
        self.notifications
            .iter()
            .find(|kept| kept.app_id == app_id && kept.origin.gtk_id() == Some(gtk_id))
            .map(|kept| kept.id)
    }

    pub(crate) fn on_lock_screen(&self, config: &NotificationsConfig) -> Vec<(Notification, bool)> {
        if !config.when_locked {
            return Vec::new();
        }
        self.notifications
            .iter()
            .filter(|notification| {
                let app = config.app(&notification.app_id);
                app.allow && app.lock_screen && !notification.hide_on_lock
            })
            .map(|notification| {
                let hidden = notification.hide_content_on_lock
                    || config.show_previews != ShowPreviews::Always;
                (notification.clone(), hidden)
            })
            .collect()
    }

    pub(crate) fn groups(&self, config: &NotificationsConfig) -> Vec<Vec<Notification>> {
        let mut groups: Vec<Vec<Notification>> = Vec::new();
        for notification in &self.notifications {
            let grouped = config.app(&notification.app_id).grouping != Grouping::Off;
            match groups
                .iter_mut()
                .find(|group| grouped && group[0].app_id == notification.app_id)
            {
                Some(group) => group.push(notification.clone()),
                None => groups.push(vec![notification.clone()]),
            }
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment() -> Moment {
        Moment {
            focus: None,
            panel_open: false,
            app_in_front: false,
            transient: false,
            now: 1_000,
        }
    }

    fn note(id: u32, app_id: &str) -> Notification {
        Notification {
            id,
            app_id: app_id.into(),
            app_name: app_id.into(),
            icon: String::new(),
            image: None,
            communication: false,
            summary: String::new(),
            body: String::new(),
            actions: Vec::new(),
            reply: None,
            hide_on_lock: false,
            hide_content_on_lock: false,
            level: Level::Active,
            posted: 0,
            origin: Origin::Freedesktop,
        }
    }

    #[test]
    fn an_ordinary_notification_shows_the_apps_banner_style_and_is_kept() {
        let app = AppNotifications {
            style: AlertStyle::Persistent,
            ..AppNotifications::default()
        };
        let shown = present(Level::Active, &app, &moment());
        assert_eq!(shown.banner, Some(AlertStyle::Persistent));
        assert!(shown.kept && shown.sound);
    }

    #[test]
    fn a_focus_silences_everything_but_allowed_apps_and_critical_alerts() {
        let dnd = FocusRule {
            allowed: false,
            time_sensitive: false,
        };
        let quiet = Moment {
            focus: Some(dnd),
            ..moment()
        };
        let app = AppNotifications::default();
        let silenced = present(Level::TimeSensitive, &app, &quiet);
        assert_eq!((silenced.banner, silenced.kept), (None, true));
        assert_eq!(present(Level::Critical, &app, &quiet).banner, None);
        let allowed = AppNotifications {
            critical: true,
            style: AlertStyle::Temporary,
            sound: false,
            ..app.clone()
        };
        let critical = present(Level::Critical, &allowed, &quiet);
        assert_eq!(critical.banner, Some(AlertStyle::Persistent));
        assert!(critical.sound);
        let work = Moment {
            focus: Some(FocusRule {
                allowed: true,
                time_sensitive: false,
            }),
            ..moment()
        };
        assert!(present(Level::Active, &app, &work).banner.is_some());
        let muted = AppNotifications {
            muted_until: Some(2_000),
            ..AppNotifications::default()
        };
        assert_eq!(present(Level::Active, &muted, &moment()).banner, None);
    }

    #[test]
    fn time_sensitive_alerts_break_through_when_the_focus_and_the_app_allow_it() {
        let rule = FocusRule {
            allowed: false,
            time_sensitive: true,
        };
        let moment = Moment {
            focus: Some(rule),
            ..moment()
        };
        let app = AppNotifications::default();
        assert!(
            present(Level::TimeSensitive, &app, &moment)
                .banner
                .is_some()
        );
        assert!(present(Level::Active, &app, &moment).banner.is_none());
        let refused = AppNotifications {
            time_sensitive: false,
            ..app
        };
        assert!(
            present(Level::TimeSensitive, &refused, &moment)
                .banner
                .is_none()
        );
    }

    #[test]
    fn focus_names_round_trip() {
        for focus in Focus::ALL {
            assert_eq!(parse_focus(focus_name(Some(focus))), Some(focus));
        }
        assert_eq!(parse_focus(""), None);
    }

    #[test]
    fn passive_transient_and_front_app_notifications_skip_the_banner_or_the_list() {
        let app = AppNotifications::default();
        assert_eq!(present(Level::Passive, &app, &moment()).banner, None);
        let transient = Moment {
            transient: true,
            ..moment()
        };
        let shown = present(Level::Active, &app, &transient);
        assert!(shown.banner.is_some() && !shown.kept);
        let front = Moment {
            app_in_front: true,
            ..moment()
        };
        let quiet = present(Level::Active, &app, &front);
        assert!(quiet.banner.is_none() && quiet.kept && !quiet.sound);
        let denied = AppNotifications {
            allow: false,
            ..AppNotifications::default()
        };
        assert!(!present(Level::Critical, &denied, &moment()).kept);
    }

    #[test]
    fn a_replacement_keeps_its_id_and_moves_to_the_top() {
        let mut center = Center::default();
        let first = center.allocate_id(0);
        let second = center.allocate_id(0);
        center.keep(note(first, "a"));
        center.keep(note(second, "b"));
        assert_eq!(center.allocate_id(first), first);
        center.keep(note(first, "a"));
        let ids: Vec<u32> = center.notifications.iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![first, second]);
    }

    #[test]
    fn each_app_keeps_only_its_newest_notifications() {
        let mut center = Center::default();
        for _ in 0..KEPT_PER_APP + 3 {
            let id = center.allocate_id(0);
            center.keep(note(id, "chatty"));
        }
        center.keep(note(center.next_id + 1, "quiet"));
        let chatty = center
            .notifications
            .iter()
            .filter(|n| n.app_id == "chatty")
            .count();
        assert_eq!(chatty, KEPT_PER_APP);
        assert_eq!(center.notifications.len(), KEPT_PER_APP + 1);
    }

    #[test]
    fn groups_follow_each_apps_grouping_setting() {
        let mut center = Center::default();
        for (id, app) in [(1, "mail"), (2, "chat"), (3, "mail"), (4, "chat")] {
            center.keep(note(id, app));
        }
        let mut config = NotificationsConfig::default();
        config.app_mut("chat").grouping = Grouping::Off;
        let sizes: Vec<(String, usize)> = center
            .groups(&config)
            .iter()
            .map(|group| (group[0].app_id.clone(), group.len()))
            .collect();
        assert_eq!(
            sizes,
            vec![("chat".into(), 1), ("mail".into(), 2), ("chat".into(), 1)]
        );
    }

    #[test]
    fn the_lock_screen_shows_allowed_apps_and_hides_content_unless_previews_are_always() {
        let mut center = Center::default();
        center.keep(note(1, "mail"));
        center.keep(note(2, "bank"));
        let mut secret = note(3, "chat");
        secret.hide_content_on_lock = true;
        center.keep(secret);
        let mut config = NotificationsConfig::default();
        config.app_mut("bank").lock_screen = false;
        let shown: Vec<(u32, bool)> = center
            .on_lock_screen(&config)
            .iter()
            .map(|(n, hidden)| (n.id, *hidden))
            .collect();
        assert_eq!(shown, vec![(3, true), (1, false)]);
        config.show_previews = ShowPreviews::WhenUnlocked;
        assert!(
            center
                .on_lock_screen(&config)
                .iter()
                .all(|(_, hidden)| *hidden)
        );
        config.when_locked = false;
        assert!(center.on_lock_screen(&config).is_empty());
    }

    #[test]
    fn urgency_and_gtk_priority_map_to_the_macos_levels() {
        assert_eq!(Level::from_urgency(Some(0)), Level::Passive);
        assert_eq!(Level::from_urgency(None), Level::Active);
        assert_eq!(Level::from_urgency(Some(2)), Level::Critical);
        assert_eq!(Level::from_priority("high"), Level::TimeSensitive);
        assert_eq!(Level::from_priority("urgent"), Level::Critical);
    }
}
