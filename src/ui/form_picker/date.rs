// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::{space, type_};
use rune::{El, col, el, row, text};
use rune_kit::{Button, ButtonVariant};

use super::{ActivePicker, AppState, Theme};

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn month_of(value: &str) -> Option<(u32, u32)> {
    let (year, rest) = value.split_once('-')?;
    let (month, _) = rest.split_once('-')?;
    let (year, month) = (year.parse().ok()?, month.parse().ok()?);
    (year > 0 && (1..=12).contains(&month)).then_some((year, month))
}

fn current_month() -> (u32, u32) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let local = rune::time::LocalTime::at(elapsed.as_millis() as i64);
    (local.year as u32, local.month)
}

fn adjacent_month(year: u32, month: u32, next: bool) -> (u32, u32) {
    match (month, next) {
        (12, true) if year < u32::MAX => (year + 1, 1),
        (1, false) if year > 1 => (year - 1, 12),
        (12, true) | (1, false) => (year, month),
        (_, true) => (year, month + 1),
        (_, false) => (year, month - 1),
    }
}

pub(super) fn calendar(active: &ActivePicker, minimum: &str, maximum: &str, theme: &Theme) -> El {
    let (year, month) = active
        .calendar_month
        .or_else(|| month_of(&active.input.text))
        .or_else(|| month_of(minimum))
        .or_else(|| month_of(maximum))
        .unwrap_or_else(current_month);
    let kit = theme.kit(theme.overlay_panel);
    let mut header = row().w_full().center_y().gap(space::S2).child(
        text(format!("{year:04}-{month:02}"))
            .grow(1.0)
            .font(type_::BODY)
            .color(theme.text),
    );
    for (label, next) in [("Previous month", false), ("Next month", true)] {
        let target = adjacent_month(year, month, next);
        header = header.child(
            Button::new(label)
                .disabled(target == (year, month))
                .on_press(move |state: &mut AppState| {
                    if let Some(active) = state.form_picker.as_mut() {
                        active.calendar_month = Some(target);
                    }
                })
                .build(&kit),
        );
    }
    // Gregorian weekday of the first day, with Monday in the first column.
    let prior_year = u64::from(year - 1);
    let prior_months: u64 = (1..month)
        .map(|month| u64::from(days_in_month(year, month)))
        .sum();
    let first =
        ((365 * prior_year + prior_year / 4 - prior_year / 100 + prior_year / 400 + prior_months)
            % 7) as u32;
    let mut grid = col().w_full().gap(space::S1).child(header);
    let mut weekdays = row().w_full().gap(space::S1);
    for day in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
        weekdays = weekdays.child(
            el().w(0.0)
                .grow(1.0)
                .center()
                .child(text(day).font(type_::LABEL).color(theme.text_muted)),
        );
    }
    grid = grid.child(weekdays);
    for week in 0..(first + days_in_month(year, month)).div_ceil(7) {
        let mut line = row().w_full().gap(space::S1);
        for weekday in 0..7 {
            let slot = week * 7 + weekday;
            let cell = el().w(0.0).grow(1.0).center();
            if slot < first || slot >= first + days_in_month(year, month) {
                line = line.child(cell);
                continue;
            }
            let day = slot - first + 1;
            let value = format!("{year:04}-{month:02}-{day:02}");
            let selected = active.input.text == value;
            let enabled = within_bounds("date", &value, minimum, maximum);
            line = line.child(
                cell.child(
                    Button::new(day.to_string())
                        .variant(if selected {
                            ButtonVariant::Submit
                        } else {
                            ButtonVariant::Ghost
                        })
                        .disabled(!enabled)
                        .on_press(move |state: &mut AppState| {
                            if let Some(active) = state.form_picker.as_mut() {
                                active.input.replace_text(value.clone());
                            }
                        })
                        .build(&kit)
                        .label(format!("{year:04}-{month:02}-{day:02}")),
                ),
            );
        }
        grid = grid.child(line);
    }
    grid
}

pub(super) fn within_bounds(kind: &str, value: &str, minimum: &str, maximum: &str) -> bool {
    fn key<'a>(kind: &str, value: &'a str) -> (u32, &'a str) {
        if kind == "time" {
            return (0, value);
        }
        value
            .split_once('-')
            .map_or((0, value), |(year, remainder)| {
                (year.parse().unwrap_or_default(), remainder)
            })
    }
    let value = key(kind, value);
    let has_minimum = valid_value(kind, minimum);
    let has_maximum = valid_value(kind, maximum);
    let above_minimum = !has_minimum || value >= key(kind, minimum);
    let below_maximum = !has_maximum || value <= key(kind, maximum);
    if kind == "time" && has_minimum && has_maximum && minimum > maximum {
        above_minimum || below_maximum
    } else {
        above_minimum && below_maximum
    }
}

pub(super) fn valid_value(kind: &str, value: &str) -> bool {
    fn number(value: &str, digits: usize, min: u32, max: u32) -> Option<u32> {
        if value.len() != digits || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        value
            .parse()
            .ok()
            .filter(|parsed| (min..=max).contains(parsed))
    }
    fn date(value: &str, require_day: bool) -> bool {
        let mut parts = value.split('-');
        let Some(year) = parts
            .next()
            .and_then(|part| number(part, part.len().max(4), 1, u32::MAX))
        else {
            return false;
        };
        let Some(month) = parts.next().and_then(|part| number(part, 2, 1, 12)) else {
            return false;
        };
        let days = days_in_month(year, month);
        (!require_day
            || parts
                .next()
                .and_then(|part| number(part, 2, 1, days))
                .is_some())
            && parts.next().is_none()
    }
    fn time(value: &str) -> bool {
        let mut parts = value.split(':');
        if parts
            .next()
            .and_then(|part| number(part, 2, 0, 23))
            .is_none()
            || parts
                .next()
                .and_then(|part| number(part, 2, 0, 59))
                .is_none()
        {
            return false;
        }
        if let Some(seconds) = parts.next() {
            let (whole, fraction) = seconds
                .split_once('.')
                .map_or((seconds, None), |(whole, fraction)| (whole, Some(fraction)));
            if number(whole, 2, 0, 59).is_none()
                || fraction.is_some_and(|fraction| {
                    !(1..=3).contains(&fraction.len())
                        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
                })
            {
                return false;
            }
        }
        parts.next().is_none()
    }
    match kind {
        "date" => date(value, true),
        "month" => date(value, false),
        "time" => time(value),
        "week" => value.split_once("-W").is_some_and(|(year, week)| {
            number(year, year.len().max(4), 1, u32::MAX).is_some()
                && number(week, 2, 1, 53).is_some()
        }),
        "datetime-local" => value
            .split_once('T')
            .is_some_and(|(day, clock)| date(day, true) && time(clock)),
        _ => false,
    }
}
