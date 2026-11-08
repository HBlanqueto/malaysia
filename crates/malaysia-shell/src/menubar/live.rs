use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bk::platform::{
    PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupSpec, Popups, Timers,
};
use bk::prelude::*;
use bk::progress::ProgressView;
use bk::style::Palette;
use bk::theme;
use ctdesign::{
    BODY, CAPTION, HEADLINE, LIVE_ACTIVITY_GAP, LIVE_ACTIVITY_HEIGHT, LIVE_ACTIVITY_RADIUS,
    LIVE_ACTIVITY_WIDTH, MENUBAR_FONT_SIZE, MENUBAR_HEIGHT, MENUBAR_MARGIN, NOTIFICATION_GAP,
    NOTIFICATION_PADDING,
};
use freya_core::integration::AppComponent;
use torin::prelude::{Area, Gaps};

use crate::localize::tr;
use crate::notifications::{
    Activity, ActivityKind, cancel_activity, pause_activity, watch_activities,
};

const TICK: Duration = Duration::from_secs(1);

pub(crate) fn use_activities() -> State<Vec<Activity>> {
    let state = use_state(Vec::new);
    use_hook(move || {
        let latest = Arc::new(Mutex::new(Vec::new()));
        let shared = latest.clone();
        let wake = Timers::get().wakeup(move || {
            let mut state = state;
            state.set_if_modified(
                shared
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone(),
            );
        });
        watch_activities(move |activities| {
            *latest
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = activities;
            wake.wake();
        });
    });
    let tick = use_state(|| 0u64);
    let calling = state
        .read()
        .iter()
        .any(|activity| activity.kind() == ActivityKind::Call && !activity.ended);
    use_hook(move || keep_ticking(Timers::get(), tick));
    if calling {
        let _ = *tick.read();
    }
    state
}

fn keep_ticking(timers: Timers, tick: State<u64>) {
    let again = timers.clone();
    timers.after(TICK, move || {
        let mut tick = tick;
        *tick.write() += 1;
        keep_ticking(again.clone(), tick);
    });
}

pub(crate) fn live_area(activities: State<Vec<Activity>>, color: Color) -> HStack {
    let shown = activities.read().clone();
    HStack::new()
        .height(Size::fill())
        .cross_align(Alignment::Center)
        .spacing(MENUBAR_MARGIN as f32 / 2.0)
        .children(
            shown
                .into_iter()
                .map(|activity| pill(activity, activities, color)),
        )
}

fn symbol(activity: &Activity) -> Symbol {
    if activity.ended {
        return Symbol::Checkmark;
    }
    match activity.kind() {
        ActivityKind::Transfer => Symbol::SquareOnSquare,
        ActivityKind::Call => Symbol::Phone,
        ActivityKind::Print => Symbol::Printer,
    }
}

fn compact(activity: &Activity) -> String {
    if activity.ended {
        return tr!("live-done").to_string();
    }
    match activity.kind() {
        ActivityKind::Call => elapsed(activity.started),
        ActivityKind::Print if activity.total > 0 => {
            format!("{}/{}", activity.processed, activity.total)
        }
        ActivityKind::Print => tr!("live-printing").to_string(),
        ActivityKind::Transfer => activity.fraction().map_or_else(
            || tr!("live-copying").to_string(),
            |fraction| format!("{:.0} %", fraction * 100.0),
        ),
    }
}

fn elapsed(started: i64) -> String {
    let seconds = (chrono::Utc::now().timestamp() - started).max(0);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn pill(activity: Activity, activities: State<Vec<Activity>>, color: Color) -> Element {
    let area = Rc::new(Cell::new(Area::default()));
    let measured = area.clone();
    let id = activity.id.clone();
    HStack::new()
        .height(Size::px(MENUBAR_HEIGHT as f32 * 0.8))
        .padding(Gaps::new(0.0, NOTIFICATION_GAP, 0.0, NOTIFICATION_GAP))
        .corner_radius(MENUBAR_HEIGHT as f32 * 0.4)
        .cross_align(Alignment::Center)
        .spacing(MENUBAR_MARGIN as f32 / 2.0)
        .on_sized(move |event: Event<SizedEventData>| measured.set(event.area))
        .on_press(move |_| expand(id.clone(), activities, area.get()))
        .child(
            Image::system(symbol(&activity))
                .size(MENUBAR_FONT_SIZE)
                .color(Palette::current().label),
        )
        .child(Text::new(compact(&activity)).color(color))
        .into_element()
}

fn expand(id: String, activities: State<Vec<Activity>>, area: Area) {
    let width = LIVE_ACTIVITY_WIDTH;
    let spec = PopupSpec {
        anchor_rect: (
            area.min_x() as i32,
            0,
            area.width().max(1.0) as u32,
            MENUBAR_HEIGHT as u32,
        ),
        anchor: PopupAnchor::Bottom,
        gravity: PopupGravity::Bottom,
        offset: (0, LIVE_ACTIVITY_GAP),
        size: (width, LIVE_ACTIVITY_HEIGHT),
        constraint_adjustment: PopupConstraintAdjustment {
            flip_x: false,
            flip_y: false,
            slide_x: true,
            slide_y: false,
            resize_y: false,
        },
        grab: true,
        on_dismissed: None,
    };
    Popups::get().open(spec, AppComponent::new(Expanded { id, activities }));
}

struct Expanded {
    id: String,
    activities: State<Vec<Activity>>,
}

impl App for Expanded {
    fn render(&self) -> impl IntoElement {
        let palette = Palette::current();
        let activity = self
            .activities
            .read()
            .iter()
            .find(|activity| activity.id == self.id)
            .cloned();
        let Some(activity) = activity else {
            return rect().into_element();
        };
        let id = activity.id.clone();
        let paused = activity.paused;
        let progress = activity.fraction().map(|fraction| fraction as f32);
        let mut buttons = HStack::new().spacing(NOTIFICATION_GAP);
        if activity.pausable {
            let id = id.clone();
            let label = if paused {
                tr!("live-resume")
            } else {
                tr!("live-pause")
            };
            buttons = buttons
                .child(Button::new(label).on_press(move |_| pause_activity(id.clone(), !paused)));
        }
        if activity.cancellable {
            let label = if activity.kind() == ActivityKind::Call {
                tr!("live-hang-up")
            } else {
                tr!("live-cancel")
            };
            buttons =
                buttons.child(Button::new(label).on_press(move |_| cancel_activity(id.clone())));
        }
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(NOTIFICATION_PADDING)
            .corner_radius(LIVE_ACTIVITY_RADIUS)
            .background(theme::color(palette.raised_background))
            .child(
                VStack::new()
                    .width(Size::fill())
                    .height(Size::fill())
                    .cross_align(Alignment::Start)
                    .spacing(NOTIFICATION_GAP)
                    .child(
                        HStack::new()
                            .cross_align(Alignment::Center)
                            .spacing(NOTIFICATION_GAP)
                            .child(
                                Image::system(symbol(&activity)).size(HEADLINE.size as f32 * 1.5),
                            )
                            .child(
                                VStack::new()
                                    .cross_align(Alignment::Start)
                                    .child(
                                        Text::new(activity.title.clone())
                                            .font(HEADLINE)
                                            .line_limit(1),
                                    )
                                    .child(
                                        Text::new(summary(&activity))
                                            .font(CAPTION)
                                            .line_limit(1)
                                            .color(theme::color(palette.secondary_label)),
                                    ),
                            ),
                    )
                    .child(match progress {
                        Some(value) => {
                            ProgressView::determinate(tr!("live-progress"), value).into_element()
                        }
                        None if activity.ended => {
                            Text::new(tr!("live-done")).font(BODY).into_element()
                        }
                        None => ProgressView::indeterminate(tr!("live-progress")).into_element(),
                    })
                    .child(buttons),
            )
            .into_element()
    }
}

fn summary(activity: &Activity) -> String {
    let amount = match activity.unit.as_str() {
        "bytes" if activity.total > 0 => tr!(
            "live-of",
            done = bytes(activity.processed),
            total = bytes(activity.total)
        )
        .to_string(),
        "files" | "pages" if activity.total > 0 => tr!(
            "live-of",
            done = activity.processed.to_string(),
            total = activity.total.to_string()
        )
        .to_string(),
        _ => String::new(),
    };
    let parts: Vec<String> = [activity.detail.clone(), amount]
        .into_iter()
        .chain((activity.kind() == ActivityKind::Call).then(|| elapsed(activity.started)))
        .filter(|part| !part.is_empty())
        .collect();
    parts.join(" · ")
}

fn bytes(amount: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = amount as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    let number = if unit == 0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    format!(
        "{} {}",
        number.replace('.', tr!("live-decimal")),
        UNITS[unit]
    )
}
