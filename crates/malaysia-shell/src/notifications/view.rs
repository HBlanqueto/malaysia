use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use bk::menu::MenuItem;
use bk::platform::Timers;
use bk::prelude::*;
use bk::shell::{
    Activation, Anchors, CardFrames, KeyboardInteractivity, LayerGeometry, Toplevels,
    set_card_frames, set_layer_geometry,
};
use bk::style::Palette;
use bk::theme;
use ctappearance::notifications::{AlertStyle, Focus, NotificationsConfig, ShowPreviews};
use ctdesign::{
    BODY, CAPTION, HAIRLINE, HEADLINE, NOTIFICATION_BANNER_MS, NOTIFICATION_BODY_LINES,
    NOTIFICATION_CLOSE_SIZE, NOTIFICATION_GAP, NOTIFICATION_ICON_SIZE, NOTIFICATION_MARGIN,
    NOTIFICATION_PADDING, NOTIFICATION_RADIUS, NOTIFICATION_STACK_INSET, NOTIFICATION_STACK_PEEK,
    NOTIFICATION_TEXT_GAP, NOTIFICATION_WIDTH,
};
use torin::prelude::{Area, Content, Gaps, Position};

use super::server::{self, Link, Reply};
use super::store::{Notification, Origin, focus_name, parse_focus};
use super::{focus_symbol, focus_title};
use crate::launcher::{app_icon, load_icon, spawn_entry};
use crate::localize::tr;

const ACTIVATION_WAIT: Duration = Duration::from_millis(250);
const HOVER_GRACE: Duration = Duration::from_millis(2500);
const SECONDS_PER_HOUR: i64 = 3600;
const SECONDS_PER_DAY: i64 = 86_400;

fn surface_width() -> u32 {
    (NOTIFICATION_WIDTH + 2 * NOTIFICATION_MARGIN) as u32
}

fn close_room() -> f32 {
    (NOTIFICATION_CLOSE_SIZE / 2.0).ceil()
}

#[derive(Clone, PartialEq)]
struct Snapshot {
    notifications: Vec<Notification>,
    banners: Vec<(u32, AlertStyle)>,
    panel_open: bool,
    focus: Option<Focus>,
    batteries: Vec<super::batteries::Battery>,
}

impl Snapshot {
    fn read(link: &Link) -> Self {
        let hub = link.hub();
        Self {
            notifications: hub.center.notifications.clone(),
            banners: hub.banners.clone(),
            panel_open: hub.panel_open,
            focus: hub.center.focus,
            batteries: hub.batteries.clone(),
        }
    }

    fn banner(&self, id: u32) -> Option<&Notification> {
        self.notifications.iter().find(|n| n.id == id)
    }
}

pub(crate) struct NotificationsApp {
    pub(crate) link: Link,
}

impl App for NotificationsApp {
    fn render(&self) -> impl IntoElement {
        let link = self.link.clone();
        let timers = Timers::get();
        let snapshot = use_state(|| Snapshot::read(&link));
        let banner_height = use_state(|| 0.0f32);
        let hovered = use_state(|| None::<u32>);
        let replying = use_state(|| None::<u32>);
        let draft = use_state(String::new);
        let expanded = use_state(HashSet::<String>::new);
        let editing = use_state(|| false);
        let timed = use_hook(|| Rc::new(RefCell::new(HashSet::<u32>::new())));
        let applied = use_hook(|| Rc::new(Cell::new(None::<LayerGeometry>)));
        use_hook({
            let link = link.clone();
            let timers = timers.clone();
            move || {
                link.set_wake(timers.wakeup({
                    let link = link.clone();
                    move || {
                        let mut snapshot = snapshot;
                        snapshot.set_if_modified(Snapshot::read(&link));
                    }
                }))
            }
        });
        let config = bk::app::use_defaults::<NotificationsConfig>("notifications");
        let config = config.read().clone();
        let now = chrono::Utc::now().timestamp();
        let shown = snapshot.read().clone();

        link.hub().front_app = Toplevels::get()
            .list
            .read()
            .iter()
            .find(|toplevel| toplevel.active)
            .map(|toplevel| toplevel.app_id.clone());

        for (id, style) in &shown.banners {
            if *style == AlertStyle::Temporary && timed.borrow_mut().insert(*id) {
                schedule_expiry(
                    &link,
                    &timers,
                    *id,
                    [hovered, replying],
                    snapshot,
                    Duration::from_millis(NOTIFICATION_BANNER_MS),
                );
            }
        }

        let geometry = if shown.panel_open {
            LayerGeometry {
                anchors: Anchors {
                    top: true,
                    right: true,
                    bottom: true,
                    left: false,
                },
                size: (surface_width() + close_room() as u32, 0),
                keyboard: KeyboardInteractivity::OnDemand,
            }
        } else if shown.banners.is_empty() || *banner_height.read() < 1.0 {
            LayerGeometry {
                anchors: Anchors {
                    top: true,
                    right: true,
                    bottom: false,
                    left: false,
                },
                size: (1, 1),
                keyboard: KeyboardInteractivity::None,
            }
        } else {
            LayerGeometry {
                anchors: Anchors {
                    top: true,
                    right: true,
                    bottom: false,
                    left: false,
                },
                size: (surface_width(), banner_height.read().ceil() as u32),
                keyboard: KeyboardInteractivity::None,
            }
        };
        let geometry = if replying.read().is_some() {
            LayerGeometry {
                keyboard: KeyboardInteractivity::Exclusive,
                ..geometry
            }
        } else {
            geometry
        };
        if applied.get() != Some(geometry) {
            applied.set(Some(geometry));
            set_layer_geometry(geometry);
        }

        let frames = use_hook(|| Rc::new(RefCell::new(HashMap::<u32, Area>::new())));
        let relayout = use_state(|| 0u64);
        let surface = use_state(Area::default);
        let sent = use_hook(|| Rc::new(RefCell::new(CardFrames::default())));
        let _ = *relayout.read();
        let context = Context {
            link: link.clone(),
            snapshot,
            hovered,
            replying,
            draft,
            previews: config.show_previews != ShowPreviews::Never,
            now,
            frames: frames.clone(),
            rendered: Rc::new(RefCell::new(Vec::new())),
            relayout,
        };
        let content = if shown.panel_open {
            panel(&context, &shown, &config, expanded, editing).into_element()
        } else {
            banners(&context, &shown, banner_height).into_element()
        };
        let cards = card_frames(&context, *surface.read());
        if *sent.borrow() != cards {
            *sent.borrow_mut() = cards.clone();
            set_card_frames(cards);
        }
        let mut measured = surface;
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .on_sized(move |event: Event<SizedEventData>| measured.set_if_modified(event.area))
            .padding(Gaps::new(
                NOTIFICATION_GAP,
                NOTIFICATION_MARGIN as f32,
                NOTIFICATION_GAP,
                NOTIFICATION_MARGIN as f32,
            ))
            .on_global_key_down({
                let link = link.clone();
                move |event: Event<KeyboardEventData>| {
                    if event.key != Key::Named(NamedKey::Escape) {
                        return;
                    }
                    if replying.peek().is_some() {
                        let mut replying = replying;
                        replying.set(None);
                    } else {
                        close_panel(&link, snapshot);
                    }
                }
            })
            .child(content)
    }
}

#[derive(Clone)]
struct Context {
    link: Link,
    snapshot: State<Snapshot>,
    hovered: State<Option<u32>>,
    replying: State<Option<u32>>,
    draft: State<String>,
    previews: bool,
    now: i64,
    frames: Rc<RefCell<HashMap<u32, Area>>>,
    rendered: Rc<RefCell<Vec<u32>>>,
    relayout: State<u64>,
}

// ADR 0068: the compositor gives each card the window frame; only cards fully on the surface count.
fn card_frames(context: &Context, surface: Area) -> CardFrames {
    let frames = context.frames.borrow();
    let cards = context
        .rendered
        .borrow()
        .iter()
        .filter_map(|id| frames.get(id))
        .filter(|area| area.min_y() >= surface.min_y() && area.max_y() <= surface.max_y())
        .map(|area| {
            (
                (area.min_x() - surface.min_x()).round() as i32,
                (area.min_y() - surface.min_y()).round() as i32,
                area.width().round() as i32,
                area.height().round() as i32,
            )
        })
        .collect();
    CardFrames {
        radius: NOTIFICATION_RADIUS as u32,
        cards,
    }
}

impl Context {
    fn refresh(&self) {
        let mut snapshot = self.snapshot;
        snapshot.set_if_modified(Snapshot::read(&self.link));
    }
}

fn schedule_expiry(
    link: &Link,
    timers: &Timers,
    id: u32,
    held: [State<Option<u32>>; 2],
    snapshot: State<Snapshot>,
    delay: Duration,
) {
    let link = link.clone();
    let again = timers.clone();
    timers.after(delay, move || {
        if held.iter().any(|state| *state.peek() == Some(id)) {
            schedule_expiry(&link, &again, id, held, snapshot, HOVER_GRACE);
            return;
        }
        server::expire_banner(&link, id);
        let mut snapshot = snapshot;
        snapshot.set_if_modified(Snapshot::read(&link));
    });
}

fn close_panel(link: &Link, mut snapshot: State<Snapshot>) {
    link.hub().panel_open = false;
    snapshot.set_if_modified(Snapshot::read(link));
}

fn banners(context: &Context, shown: &Snapshot, mut height: State<f32>) -> VStack {
    VStack::new()
        .width(Size::fill())
        .spacing(NOTIFICATION_GAP)
        .on_sized(move |event: Event<SizedEventData>| {
            height.set_if_modified(event.area.height() + 2.0 * NOTIFICATION_GAP)
        })
        .children(
            shown
                .banners
                .iter()
                .filter_map(|(id, _)| shown.banner(*id))
                .map(|notification| card(context, notification, None)),
        )
}

fn panel(
    context: &Context,
    shown: &Snapshot,
    config: &NotificationsConfig,
    expanded: State<HashSet<String>>,
    editing: State<bool>,
) -> ScrollView {
    let center = super::store::Center {
        notifications: shown.notifications.clone(),
        ..Default::default()
    };
    let groups = center.groups(config);
    let palette = Palette::current();
    let mut column = VStack::new()
        .width(Size::fill())
        .spacing(NOTIFICATION_GAP)
        .child(focus_toggle(context, shown.focus));
    if groups.is_empty() {
        column = column.child(
            rect()
                .width(Size::fill())
                .padding(NOTIFICATION_PADDING)
                .corner_radius(NOTIFICATION_RADIUS)
                .background(theme::color(palette.raised_background))
                .center()
                .child(
                    Text::new(tr!("notifications-empty"))
                        .font(BODY)
                        .color(theme::color(palette.secondary_label)),
                ),
        );
    }
    for group in groups {
        column = column.child(stack(context, group, expanded));
    }
    column = column.child(super::widgets::section(config, &shown.batteries, editing));
    ScrollView::new()
        .width(Size::fill())
        .height(Size::fill())
        .child(
            rect()
                .width(Size::fill())
                .padding(Gaps::new(0.0, 0.0, 0.0, close_room()))
                .child(column),
        )
}

fn focus_toggle(context: &Context, focus: Option<Focus>) -> Element {
    let palette = Palette::current();
    let (background, foreground) = match focus {
        Some(_) => (palette.accent, palette.on_accent),
        None => (palette.raised_background, palette.label),
    };
    let shown = focus.unwrap_or(Focus::DoNotDisturb);
    let press = {
        let context = context.clone();
        move |_| set_focus(&context, focus.is_none().then_some(Focus::DoNotDisturb))
    };
    let menu = Focus::ALL
        .into_iter()
        .map(|mode| MenuItem::action(focus_name(Some(mode)), focus_title(mode)))
        .chain([
            MenuItem::Separator,
            MenuItem::action("off", tr!("notifications-focus-off")),
        ])
        .collect::<Vec<_>>();
    let chosen = context.clone();
    rect()
        .width(Size::fill())
        .padding(Gaps::new(
            6.0,
            NOTIFICATION_PADDING,
            6.0,
            NOTIFICATION_PADDING,
        ))
        .corner_radius(NOTIFICATION_RADIUS)
        .background(theme::color(background))
        .center()
        .on_press(press)
        .child(
            HStack::new()
                .cross_align(Alignment::Center)
                .spacing(NOTIFICATION_GAP)
                .child(
                    Image::system(focus_symbol(shown))
                        .size(HEADLINE.size as f32)
                        .color(foreground),
                )
                .child(
                    Text::new(focus_title(shown))
                        .font(HEADLINE)
                        .color(theme::color(foreground)),
                ),
        )
        .context_menu(menu, move |command: String| {
            set_focus(&chosen, parse_focus(&command))
        })
        .into_element()
}

fn set_focus(context: &Context, focus: Option<Focus>) {
    {
        let mut hub = context.link.hub();
        hub.center.focus = focus;
        hub.center.save();
    }
    context.link.reply(Reply::Focus);
    context.refresh();
}

fn stack(context: &Context, group: Vec<Notification>, expanded: State<HashSet<String>>) -> Element {
    let app_id = group[0].app_id.clone();
    let open = group.len() > 1 && expanded.read().contains(&app_id);
    let palette = Palette::current();
    if open {
        let header = HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .spacing(NOTIFICATION_GAP)
            .child(
                rect().width(Size::flex(1.0)).child(
                    Text::new(group[0].app_name.clone())
                        .font(HEADLINE)
                        .color(theme::color(palette.label)),
                ),
            )
            .child(Button::new(tr!("notifications-show-less")).on_press({
                let app_id = app_id.clone();
                let mut expanded = expanded;
                move |_| {
                    expanded.write().remove(&app_id);
                }
            }))
            .child(Button::new(tr!("notifications-clear-all")).on_press({
                let ids: Vec<u32> = group.iter().map(|n| n.id).collect();
                let context = context.clone();
                move |_| {
                    for id in &ids {
                        server::close(&context.link, *id, false);
                    }
                    context.refresh();
                }
            }));
        return VStack::new()
            .width(Size::fill())
            .spacing(NOTIFICATION_GAP)
            .child(header)
            .children(
                group
                    .iter()
                    .map(|notification| card(context, notification, None)),
            )
            .into_element();
    }
    let hidden = group.len() - 1;
    let behind = (1..=hidden.min(2)).map(|layer| {
        let inset = NOTIFICATION_STACK_INSET * layer as f32;
        rect()
            .width(Size::fill())
            .height(Size::px(NOTIFICATION_STACK_PEEK))
            .layer(-(layer as i16))
            .child(
                edged(
                    rect()
                        .width(Size::fill())
                        .height(Size::fill())
                        .corner_radius(NOTIFICATION_RADIUS)
                        .background(theme::color(palette.sunken_background)),
                )
                .position(
                    Position::new_absolute()
                        .top(-2.0 * NOTIFICATION_RADIUS)
                        .left(inset),
                )
                .width(Size::px(NOTIFICATION_WIDTH as f32 - 2.0 * inset))
                .height(Size::px(
                    2.0 * NOTIFICATION_RADIUS + NOTIFICATION_STACK_PEEK,
                )),
            )
            .into_element()
    });
    VStack::new()
        .width(Size::fill())
        .spacing(0.0)
        .child(card(
            context,
            &group[0],
            (hidden > 0).then(|| (hidden, expand(app_id, expanded))),
        ))
        .children(behind)
        .into_element()
}

fn expand(app_id: String, expanded: State<HashSet<String>>) -> Rc<dyn Fn()> {
    Rc::new(move || {
        let mut expanded = expanded;
        expanded.write().insert(app_id.clone());
    })
}

fn card(
    context: &Context,
    notification: &Notification,
    stacked: Option<(usize, Rc<dyn Fn()>)>,
) -> Element {
    let palette = Palette::current();
    let id = notification.id;
    let hovered = *context.hovered.read() == Some(id);
    let title = if notification.summary.is_empty() || !context.previews {
        notification.app_name.clone()
    } else {
        notification.summary.clone()
    };
    let body = if context.previews {
        notification.body.clone()
    } else {
        tr!("notifications-hidden-preview").to_string()
    };
    let icon = app_icon(
        load_icon(Some(notification.icon.as_str())).as_ref(),
        &notification.app_name,
        NOTIFICATION_ICON_SIZE,
    );
    let title_row = HStack::new()
        .width(Size::fill())
        .content(Content::flex())
        .cross_align(Alignment::Center)
        .spacing(NOTIFICATION_GAP)
        .child(
            rect().width(Size::flex(1.0)).child(
                Text::new(title)
                    .font(HEADLINE)
                    .line_limit(1)
                    .color(theme::color(palette.label)),
            ),
        )
        .child(
            Text::new(age(notification.posted, context.now))
                .font(CAPTION)
                .color(theme::color(palette.secondary_label)),
        );
    let mut text = VStack::new()
        .width(Size::flex(1.0))
        .cross_align(Alignment::Start)
        .spacing(2.0)
        .child(title_row)
        .maybe_child((!body.is_empty()).then(|| {
            Text::new(body)
                .font(BODY)
                .line_limit(NOTIFICATION_BODY_LINES)
                .color(theme::color(palette.label))
        }));
    if let Some((hidden, _)) = &stacked {
        text = text.child(
            Text::new(tr!("notifications-more", count = hidden))
                .font(CAPTION)
                .color(theme::color(palette.secondary_label)),
        );
    }
    let buttons: Vec<_> = notification
        .actions
        .iter()
        .filter(|action| action.key != server::DEFAULT_ACTION)
        .collect();
    let replying = *context.replying.read() == Some(id);
    if replying {
        text = text.child(reply_field(context, notification));
    } else if hovered && context.previews && (!buttons.is_empty() || notification.reply.is_some()) {
        let reply = notification.reply.as_ref().map(|reply| {
            let mut replying = context.replying;
            let mut draft = context.draft;
            Button::new(reply.label.clone())
                .on_press(move |_| {
                    draft.set(String::new());
                    replying.set(Some(id));
                })
                .into_element()
        });
        text = text.child(
            HStack::new()
                .spacing(NOTIFICATION_GAP)
                .maybe_child(reply)
                .children(buttons.into_iter().map(|action| {
                    let context = context.clone();
                    let notification = notification.clone();
                    let key = action.key.clone();
                    Button::new(action.label.clone())
                        .on_press(move |_| open(&context, &notification, Some(key.clone())))
                        .into_element()
                })),
        );
    }
    let mut hovered_state = context.hovered;
    let mut left = context.hovered;
    let press = {
        let context = context.clone();
        let notification = notification.clone();
        let stacked = stacked.as_ref().map(|(_, expand)| expand.clone());
        move |_| match &stacked {
            Some(expand) => expand(),
            None => open(&context, &notification, None),
        }
    };
    let menu = vec![
        MenuItem::action("mute-hour", tr!("notifications-mute-hour")),
        MenuItem::action("mute-today", tr!("notifications-mute-today")),
        MenuItem::action("turn-off", tr!("notifications-turn-off")),
        MenuItem::Separator,
        MenuItem::action("settings", tr!("notifications-settings")),
    ];
    let app_id = notification.app_id.clone();
    let menu_context = context.clone();
    let body = rect()
        .width(Size::fill())
        .padding(NOTIFICATION_PADDING)
        .corner_radius(NOTIFICATION_RADIUS)
        .background(theme::color(palette.raised_background))
        .on_pointer_enter(move |_| hovered_state.set(Some(id)))
        .on_pointer_leave(move |_| {
            if *left.peek() == Some(id) {
                left.set(None);
            }
        })
        .on_press(press)
        .child(
            HStack::new()
                .width(Size::fill())
                .content(Content::flex())
                .cross_align(Alignment::Start)
                .spacing(NOTIFICATION_TEXT_GAP)
                .child(icon)
                .child(text),
        )
        .context_menu(menu, move |command: String| {
            mute(&menu_context, &app_id, &command)
        });
    rect()
        .width(Size::fill())
        .child(framed(context, id, body))
        .maybe_child(hovered.then(|| close_button(context, id)))
        .into_element()
}

fn reply_field(context: &Context, notification: &Notification) -> HStack {
    let send = {
        let context = context.clone();
        let notification = notification.clone();
        move |text: String| answer(&context, &notification, text)
    };
    let press = {
        let send = send.clone();
        let draft = context.draft;
        move |_| send(draft.peek().clone())
    };
    HStack::new()
        .width(Size::fill())
        .content(Content::flex())
        .cross_align(Alignment::Center)
        .spacing(NOTIFICATION_GAP)
        .child(
            TextField::new(context.draft, tr!("notifications-reply-placeholder"))
                .width(Size::flex(1.0))
                .auto_focus(true)
                .on_submit(send),
        )
        .child(Button::new(tr!("notifications-reply-send")).on_press(press))
}

fn answer(context: &Context, notification: &Notification, text: String) {
    if text.trim().is_empty() {
        return;
    }
    {
        let mut hub = context.link.hub();
        hub.banners.retain(|(banner, _)| *banner != notification.id);
        hub.center.remove(notification.id);
        hub.center.save();
    }
    let mut replying = context.replying;
    replying.set(None);
    context
        .link
        .reply(Reply::Answered(notification.clone(), text));
    context.refresh();
}

pub(crate) fn lock_card(notification: &Notification, hidden: bool) -> Element {
    let palette = Palette::current();
    let (title, body) = if hidden {
        (
            notification.app_name.clone(),
            tr!("notifications-hidden-preview").to_string(),
        )
    } else {
        (notification.summary.clone(), notification.body.clone())
    };
    let icon = app_icon(
        load_icon(Some(notification.icon.as_str())).as_ref(),
        &notification.app_name,
        NOTIFICATION_ICON_SIZE,
    );
    let card = rect()
        .width(Size::fill())
        .padding(NOTIFICATION_PADDING)
        .corner_radius(NOTIFICATION_RADIUS)
        .background(theme::color(palette.raised_background))
        .child(
            HStack::new()
                .width(Size::fill())
                .content(Content::flex())
                .cross_align(Alignment::Start)
                .spacing(NOTIFICATION_TEXT_GAP)
                .child(icon)
                .child(
                    VStack::new()
                        .width(Size::flex(1.0))
                        .cross_align(Alignment::Start)
                        .spacing(2.0)
                        .child(
                            Text::new(title)
                                .font(HEADLINE)
                                .line_limit(1)
                                .color(theme::color(palette.label)),
                        )
                        .child(
                            Text::new(body)
                                .font(BODY)
                                .line_limit(NOTIFICATION_BODY_LINES)
                                .color(theme::color(palette.label)),
                        ),
                ),
        );
    edged(card)
        .width(Size::px(NOTIFICATION_WIDTH as f32))
        .into_element()
}

fn framed(context: &Context, id: u32, body: Rect) -> Rect {
    context.rendered.borrow_mut().push(id);
    let frames = context.frames.clone();
    let mut relayout = context.relayout;
    body.corner_radius(bk::style::continuous_corner(NOTIFICATION_RADIUS))
        .on_sized(move |event: Event<SizedEventData>| {
            if frames.borrow_mut().insert(id, event.area) != Some(event.area) {
                *relayout.write() += 1;
            }
        })
}

// Stack layers and lock cards draw the window border colours: hairline inside, dark line outside (ADR 0068).
fn edged(body: Rect) -> Rect {
    let palette = ctappearance::palette(theme::resolved().scheme);
    let body = match palette.inner_line {
        Some(inner) => body.border(
            Border::new()
                .fill(theme::color(inner))
                .width(HAIRLINE)
                .alignment(BorderAlignment::Inner),
        ),
        None => body,
    };
    rect()
        .corner_radius(NOTIFICATION_RADIUS)
        .border(
            Border::new()
                .fill(theme::color(palette.outer_border))
                .width(HAIRLINE / 2.0)
                .alignment(BorderAlignment::Outer),
        )
        .child(body)
}

fn close_button(context: &Context, id: u32) -> Rect {
    let palette = Palette::current();
    let context = context.clone();
    let offset = -NOTIFICATION_CLOSE_SIZE / 3.0;
    rect()
        .position(Position::new_absolute().top(offset).left(offset))
        .width(Size::px(NOTIFICATION_CLOSE_SIZE))
        .height(Size::px(NOTIFICATION_CLOSE_SIZE))
        .corner_radius(NOTIFICATION_CLOSE_SIZE / 2.0)
        .background(theme::color(palette.raised_background))
        .border(
            Border::new()
                .fill(theme::color(palette.separator))
                .width(1.0)
                .alignment(BorderAlignment::Inner),
        )
        .center()
        .on_press(move |_| {
            server::close(&context.link, id, false);
            context.refresh();
        })
        .child(
            Image::system(Symbol::XMark)
                .size(NOTIFICATION_CLOSE_SIZE / 2.0)
                .color(palette.secondary_label),
        )
}

fn age(posted: i64, now: i64) -> String {
    let seconds = (now - posted).max(0);
    match seconds {
        0..60 => tr!("notifications-now").to_string(),
        60..SECONDS_PER_HOUR => {
            tr!("notifications-minutes", count = ((seconds / 60) as usize)).to_string()
        }
        SECONDS_PER_HOUR..SECONDS_PER_DAY => tr!(
            "notifications-hours",
            count = ((seconds / SECONDS_PER_HOUR) as usize)
        )
        .to_string(),
        _ => chrono::DateTime::from_timestamp(posted, 0)
            .map(|time| {
                time.with_timezone(&chrono::Local)
                    .format("%d/%m")
                    .to_string()
            })
            .unwrap_or_default(),
    }
}

fn mute(context: &Context, app_id: &str, command: &str) {
    if command == "settings" {
        crate::spawn::spawn_settings();
        return;
    }
    let now = chrono::Local::now();
    let mut config: NotificationsConfig = bk::app::load_defaults("notifications");
    let app = config.app_mut(app_id);
    match command {
        "mute-hour" => app.muted_until = Some(now.timestamp() + SECONDS_PER_HOUR),
        "mute-today" => {
            app.muted_until = now
                .date_naive()
                .succ_opt()
                .and_then(|tomorrow| tomorrow.and_hms_opt(0, 0, 0))
                .and_then(|midnight| midnight.and_local_timezone(chrono::Local).single())
                .map(|midnight| midnight.timestamp())
        }
        _ => app.allow = false,
    }
    if let Err(error) = bk::app::save_defaults("notifications", &config) {
        tracing::warn!(%error, "notifications: could not save the mute");
    }
    context.refresh();
}

fn open(context: &Context, notification: &Notification, action: Option<String>) {
    let activation = Activation::get();
    let timers = Timers::get();
    let done = Rc::new(Cell::new(false));
    let go = {
        let context = context.clone();
        let notification = notification.clone();
        Rc::new(move |token: Option<String>| {
            if !done.replace(true) {
                invoke(&context, &notification, action.clone(), token);
            }
        })
    };
    let fallback = go.clone();
    timers.after(ACTIVATION_WAIT, move || fallback(None));
    activation.request(move |token| go(token));
}

fn invoke(
    context: &Context,
    notification: &Notification,
    action: Option<String>,
    token: Option<String>,
) {
    {
        let mut hub = context.link.hub();
        hub.banners.retain(|(banner, _)| *banner != notification.id);
        hub.panel_open = false;
        hub.center.remove(notification.id);
        hub.center.save();
    }
    let default = match &notification.origin {
        Origin::Gtk { .. } | Origin::Portal { .. } => notification
            .origin
            .default_action()
            .map(|action| action.key.clone()),
        Origin::Freedesktop => notification
            .actions
            .iter()
            .any(|action| action.key == server::DEFAULT_ACTION)
            .then(|| server::DEFAULT_ACTION.to_string()),
    };
    match action.or(default) {
        Some(key) => context
            .link
            .reply(Reply::Invoked(notification.clone(), key, token)),
        None if notification.origin.gtk_id().is_some() => {
            context
                .link
                .reply(Reply::Invoked(notification.clone(), String::new(), token))
        }
        None => {
            focus_app(&notification.app_id, token.as_deref());
            context.link.reply(Reply::Closed(
                notification.clone(),
                server::CLOSED_DISMISSED,
            ));
        }
    }
    context.refresh();
}

fn focus_app(app_id: &str, token: Option<&str>) {
    let toplevels = Toplevels::get();
    let window = toplevels
        .list
        .read()
        .iter()
        .find(|toplevel| toplevel.app_id == app_id)
        .map(|toplevel| toplevel.id);
    match window {
        Some(window) => toplevels.activate(window),
        None => {
            if let Some(entry) = crate::desktop::read_entries()
                .into_iter()
                .find(|entry| entry.id == app_id)
            {
                spawn_entry(&entry, token);
            }
        }
    }
}
