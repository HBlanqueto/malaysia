use bk::prelude::*;
use bk::style::Palette;
use bk::theme;
use chrono::{Datelike, Local, NaiveDate};
use ctappearance::notifications::{NotificationsConfig, Widget};
use ctdesign::{
    BODY, CAPTION, HEADLINE, NOTIFICATION_GAP, NOTIFICATION_PADDING, NOTIFICATION_WIDGET_RADIUS,
    NOTIFICATION_WIDGET_SIZE,
};
use torin::prelude::Content;

use super::batteries::{Battery, COMPUTER};
use crate::localize::tr;

const WEEK: usize = 7;
const MOST_BATTERIES: usize = 4;

pub(crate) fn section(
    config: &NotificationsConfig,
    batteries: &[Battery],
    mut editing: State<bool>,
) -> VStack {
    let edit = *editing.read();
    let missing: Vec<Widget> = Widget::ALL
        .into_iter()
        .filter(|widget| !config.widgets.contains(widget))
        .collect();
    let row = config.widgets.iter().fold(
        HStack::new().width(Size::fill()).spacing(NOTIFICATION_GAP),
        |row, widget| row.child(tile(*widget, batteries, edit)),
    );
    let mut column = VStack::new()
        .width(Size::fill())
        .spacing(NOTIFICATION_GAP)
        .child(row);
    if edit {
        column = column.children(missing.into_iter().map(|widget| {
            Button::new(tr!("widgets-add", name = title(widget)))
                .on_press(move |_| change(|widgets| widgets.push(widget)))
                .into_element()
        }));
    }
    column.child(
        Button::new(if edit {
            tr!("widgets-done")
        } else {
            tr!("widgets-edit")
        })
        .on_press(move |_| editing.set(!edit)),
    )
}

fn title(widget: Widget) -> &'static str {
    match widget {
        Widget::Calendar => tr!("widget-calendar"),
        Widget::Batteries => tr!("widget-batteries"),
    }
}

fn change(apply: impl FnOnce(&mut Vec<Widget>)) {
    let mut config: NotificationsConfig = bk::app::load_defaults("notifications");
    apply(&mut config.widgets);
    if let Err(error) = bk::app::save_defaults("notifications", &config) {
        tracing::warn!(%error, "notifications: could not save the widgets");
    }
}

fn tile(widget: Widget, batteries: &[Battery], edit: bool) -> Element {
    let palette = Palette::current();
    let content = match widget {
        Widget::Calendar => calendar(Local::now().date_naive()).into_element(),
        Widget::Batteries => battery_list(batteries).into_element(),
    };
    let body = rect()
        .width(Size::px(NOTIFICATION_WIDGET_SIZE))
        .height(Size::px(NOTIFICATION_WIDGET_SIZE))
        .padding(NOTIFICATION_PADDING)
        .child(content)
        .maybe_child(edit.then(|| {
            Button::new(tr!("widgets-remove"))
                .on_press(move |_| change(|widgets| widgets.retain(|kept| *kept != widget)))
                .into_element()
        }));
    bk::style::panel_edges(body, NOTIFICATION_WIDGET_RADIUS, palette.raised_background)
        .into_element()
}

pub(crate) fn month_cells(today: NaiveDate) -> Vec<Option<u32>> {
    let first = today.with_day(1).unwrap_or(today);
    let lead = first.weekday().num_days_from_monday() as usize;
    let days = (28..=31)
        .rev()
        .find(|day| today.with_day(*day).is_some())
        .unwrap_or(28);
    let mut cells = vec![None; lead];
    cells.extend((1..=days).map(Some));
    cells
}

fn calendar(today: NaiveDate) -> VStack {
    let palette = Palette::current();
    let cell = (NOTIFICATION_WIDGET_SIZE - 2.0 * NOTIFICATION_PADDING) / WEEK as f32;
    let cells = month_cells(today);
    let weeks = cells.chunks(WEEK).map(|week| {
        HStack::new()
            .spacing(0.0)
            .children(week.iter().map(|day| {
                let is_today = *day == Some(today.day());
                rect()
                    .width(Size::px(cell))
                    .height(Size::px(cell * 0.8))
                    .corner_radius(cell / 2.0)
                    .background(if is_today {
                        theme::color(palette.accent)
                    } else {
                        Color::TRANSPARENT
                    })
                    .center()
                    .child(
                        Text::new(day.map(|day| day.to_string()).unwrap_or_default())
                            .font(CAPTION)
                            .color(theme::color(if is_today {
                                palette.on_accent
                            } else {
                                palette.label
                            })),
                    )
                    .into_element()
            }))
            .into_element()
    });
    VStack::new()
        .cross_align(Alignment::Start)
        .spacing(0.0)
        .child(
            Text::new(month_name(today.month()).to_uppercase())
                .font(HEADLINE)
                .color(theme::color(palette.accent)),
        )
        .children(weeks)
}

pub(crate) fn month_name(month: u32) -> &'static str {
    match month {
        1 => tr!("month-1"),
        2 => tr!("month-2"),
        3 => tr!("month-3"),
        4 => tr!("month-4"),
        5 => tr!("month-5"),
        6 => tr!("month-6"),
        7 => tr!("month-7"),
        8 => tr!("month-8"),
        9 => tr!("month-9"),
        10 => tr!("month-10"),
        11 => tr!("month-11"),
        _ => tr!("month-12"),
    }
}

fn battery_list(batteries: &[Battery]) -> VStack {
    let palette = Palette::current();
    let mut list = VStack::new()
        .width(Size::fill())
        .cross_align(Alignment::Start)
        .spacing(NOTIFICATION_GAP)
        .child(Text::new(tr!("widget-batteries")).font(HEADLINE));
    if batteries.is_empty() {
        return list.child(
            Text::new(tr!("widget-no-batteries"))
                .font(CAPTION)
                .color(theme::color(palette.secondary_label)),
        );
    }
    for battery in batteries.iter().take(MOST_BATTERIES) {
        let name = if battery.kind == COMPUTER || battery.model.is_empty() {
            tr!("widget-this-computer").to_string()
        } else {
            battery.model.clone()
        };
        list = list.child(
            HStack::new()
                .width(Size::fill())
                .content(Content::flex())
                .spacing(NOTIFICATION_GAP)
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .child(Text::new(name).font(BODY).line_limit(1)),
                )
                .child(
                    Text::new(format!("{:.0} %", battery.percent))
                        .font(BODY)
                        .color(theme::color(palette.secondary_label)),
                ),
        );
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_month_starts_under_its_weekday_and_ends_on_its_last_day() {
        let november = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let cells = month_cells(november);
        assert_eq!(cells.iter().take_while(|cell| cell.is_none()).count(), 6);
        assert_eq!(cells.last(), Some(&Some(30)));
        let february = NaiveDate::from_ymd_opt(2028, 2, 10).unwrap();
        assert_eq!(month_cells(february).last(), Some(&Some(29)));
    }
}
