// SPDX-License-Identifier: GPL-3.0-only

//! The native latest-visited list and its history-only controls.

use elaris::history::{Page, PageEntry};
use rune::tok::{space, type_};
use rune::{El, GrowthSpec, Key, Transition, col, el, icon, row, text, text_input};
use rune_kit::{Button, ButtonVariant, Checkbox, Dialog, Kit};

use crate::address::compact_address;
use crate::app::AppState;
use crate::storage::HistoryMutation;
use crate::ui::{
    HISTORY_SCROLL, HISTORY_SEARCH, controls, icons,
    theme::{self, Theme},
};

use super::layout::{self, PageLayout, SECTION_GAP};
const SEARCH_WIDTH: f32 = 520.0;
const ROW_HEIGHT: f32 = 48.0;
const TIME_WIDTH: f32 = 72.0;

pub(super) fn view(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
    compact: bool,
) -> PageLayout {
    let history = &state.history_page;
    let kit = theme.kit(theme.settings_surface);
    if history.confirming {
        confirmation(theme, overlays);
    }
    let has_entries = history
        .page
        .as_ref()
        .is_some_and(|page| !page.entries.is_empty());
    let actions = row()
        .gap(space::S2)
        .center_y()
        .justify_end()
        .children(actions(state, theme, &kit, has_entries));
    let toolbar = if compact {
        col()
            .w_full()
            .gap(space::S2)
            .child(search(state, theme))
            .child(actions.w_full())
    } else {
        row()
            .w_full()
            .gap(theme::PAGE_GROUP_PADDING)
            .center_y()
            .justify_between()
            .child(search(state, theme))
            .child(actions)
    }
    .key("history-header");
    let mut results = col()
        .key("history-results")
        .w_full()
        .gap(theme::PAGE_GROUP_PADDING);
    if let Some(error) = &history.action_error {
        results = results.child(
            text(error)
                .font(type_::BODY)
                .color(theme.text)
                .wrap()
                .role("alert"),
        );
    }
    if let Some(error) = &history.load_error {
        results = results.child(
            empty("History unavailable", error, theme).child(
                Button::new("Try Again")
                    .variant(ButtonVariant::Ghost)
                    .on_press(|state: &mut AppState| state.request_history_page())
                    .build(&kit),
            ),
        );
    } else if history.page.is_none() || (history.loading && !has_entries) {
        results = results.child(
            text("Loading history…")
                .font(type_::BODY)
                .color(theme.text_muted)
                .role("status")
                .py(SECTION_GAP),
        );
    } else if !has_entries {
        results = results.child(if !history.input.text.trim().is_empty() {
            empty("No matching history", "Try another search.", theme).child(
                Button::new("Clear Search")
                    .variant(ButtonVariant::Ghost)
                    .on_press_tx(AppState::clear_history_search)
                    .build(&kit),
            )
        } else if !history.previous.is_empty() {
            empty(
                "No older entries",
                "Go back to see more recent history.",
                theme,
            )
        } else {
            empty(
                "No browsing history yet",
                "Pages you visit will appear here.",
                theme,
            )
        });
    }
    results = results.child(entries(state, theme, &kit, reduced_motion));
    PageLayout {
        scroll_key: HISTORY_SCROLL,
        icon: &icons::HISTORY,
        toolbar: Some(toolbar),
        content: results.children(pagination(state, &kit)),
    }
}

/// The header's trailing controls: selection actions while any row is ticked, otherwise Clear.
fn actions(state: &AppState, theme: &Theme, kit: &Kit, has_entries: bool) -> Vec<El> {
    let history = &state.history_page;
    if history.selected.is_empty() {
        return vec![
            Button::new("Clear History")
                .variant(ButtonVariant::GhostMuted)
                .disabled(
                    history.writing
                        || (!has_entries
                            && history.input.text.trim().is_empty()
                            && history.previous.is_empty()),
                )
                .on_press(|state: &mut AppState| state.history_page.confirming = true)
                .build(kit)
                .key("history-clear"),
        ];
    }
    vec![
        text(format!("{} selected", history.selected.len()))
            .font(type_::BODY)
            .color(theme.text_muted)
            .single_line()
            .role("status"),
        Button::new("Cancel")
            .variant(ButtonVariant::Neutral)
            .on_press(|state: &mut AppState| state.history_page.selected.clear())
            .build(kit)
            .key("history-selection-cancel"),
        Button::new("Delete")
            .variant(ButtonVariant::Danger)
            .disabled(history.writing)
            .on_press(AppState::delete_selected_history)
            .build(kit)
            .key("history-selection-delete"),
    ]
}

fn search(state: &AppState, theme: &Theme) -> El {
    let field = text_input(&state.history_page.input, type_::BODY)
        .key(HISTORY_SEARCH)
        .single_line()
        .grow(1.0)
        .min_w(0.0)
        .color(theme.text)
        .caret(theme.text, theme.selection)
        .no_focus_ring()
        .placeholder("Search history", theme.text_muted)
        .label("Search history")
        .on_edit(|state: &mut AppState| &mut state.history_page.input)
        .on_input_change(AppState::history_search_changed)
        .on_input_key(|state: &mut AppState, event, tx| {
            if event.key == Key::Escape {
                tx.focus(crate::ui::VIEWPORT, false);
                true
            } else if matches!(event.key, Key::Enter | Key::Down)
                && let Some(entry) = state
                    .history_page
                    .page
                    .as_ref()
                    .and_then(|page| page.entries.first())
            {
                focus_entry(tx, &entry.entry.url);
                true
            } else {
                false
            }
        });
    let clear = (!state.history_page.input.text.is_empty()).then(|| {
        controls::icon_button(
            "history-search-clear",
            "Clear search",
            &icons::CLOSE,
            theme::CLOSE_BUTTON,
            theme,
        )
        .focusable(true)
        .on_press_tx(AppState::clear_history_search)
    });
    row()
        .w_full()
        .max_w(SEARCH_WIDTH)
        .shrink()
        .h(theme::FIELD_HEIGHT)
        .center_y()
        .px(theme::FIELD_PADDING)
        .gap(space::S2)
        .radius(theme::FIELD_RADIUS)
        .fill(theme.address_field)
        .border(1.0, theme.control_border)
        .focus_within_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .child(
            icon(&icons::SEARCH, theme::ICON)
                .color(theme.icon)
                .a11y_hidden(),
        )
        .child(field)
        .children(clear)
}

fn entries(state: &AppState, theme: &Theme, kit: &Kit, reduced_motion: bool) -> El {
    let quiet = reduced_motion || !state.history_page.input.text.trim().is_empty();
    let motion = |list: El| {
        if quiet {
            list.child_exit(Transition::none()).instant_changes()
        } else {
            list.growth(GrowthSpec::default())
        }
    };
    let mut list = motion(col().key("history-entries").w_full().gap(SECTION_GAP));
    let Some(page) = &state.history_page.page else {
        return list;
    };
    for day in page.entries.chunk_by(|a, b| a.date == b.date) {
        let date = &day[0].date;
        let mut rows = motion(layout::group(theme));
        for (index, entry) in day.iter().enumerate() {
            rows = rows.child(entry_row(state, entry, index == 0, theme, kit, quiet));
        }
        list = list.child({
            let section = layout::section(
                format!("history-date-{date}"),
                &date_label(date, page),
                theme,
                rows,
            );
            if quiet {
                section
            } else {
                section.arrives_whole()
            }
        });
    }
    list
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

/// A `YYYY-MM-DD` day as "Saturday, November 21, 2026", led by Today or Yesterday when it is one.
fn date_label(date: &str, page: &Page) -> String {
    let full = full_date(date).unwrap_or_else(|| "Unknown date".into());
    if date == page.today {
        format!("Today – {full}")
    } else if date == page.yesterday {
        format!("Yesterday – {full}")
    } else {
        full
    }
}

fn full_date(date: &str) -> Option<String> {
    let mut parts = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    let name = MONTHS.get(usize::try_from(month.checked_sub(1)?).ok()?)?;
    let weekday = WEEKDAYS[days_from_civil(year, month, day).rem_euclid(7) as usize];
    Some(format!("{weekday}, {name} {day}, {year}"))
}

/// Days since 1970-01-01 (a Thursday) for a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// A 24-hour `HH:MM` time as "1:03 PM".
fn clock_time(time: &str) -> String {
    let Some((hour, minute)) = time
        .split_once(':')
        .and_then(|(hour, minute)| Some((hour.parse::<u8>().ok()?, minute)))
    else {
        return time.into();
    };
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let hour = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    format!("{hour}:{minute} {suffix}")
}

fn entry_key(url: &str) -> String {
    format!(
        "history-open:{}",
        url.replace('%', "%25").replace('/', "%2F")
    )
}

fn focus_entry(tx: &mut rune::Tx, url: &str) {
    let key = entry_key(url);
    tx.scroll_into_view(&key);
    tx.focus(key, true);
}

fn navigate_entries(state: &mut AppState, url: &str, key: &Key, tx: &mut rune::Tx) -> bool {
    let Some(page) = &state.history_page.page else {
        return false;
    };
    let Some(index) = page.entries.iter().position(|entry| entry.entry.url == url) else {
        return false;
    };
    let next = match key {
        Key::Down => page.entries.get(index + 1),
        Key::Up if index == 0 => {
            tx.focus(HISTORY_SEARCH, true);
            return true;
        }
        Key::Up => page.entries.get(index - 1),
        _ => return false,
    };
    if let Some(entry) = next {
        focus_entry(tx, &entry.entry.url);
    }
    true
}

fn entry_row(
    state: &AppState,
    row_entry: &PageEntry,
    first: bool,
    theme: &Theme,
    kit: &Kit,
    quiet: bool,
) -> El {
    let entry = &row_entry.entry;
    let title = if entry.title.trim().is_empty() {
        &entry.url
    } else {
        &entry.title
    };
    let address = compact_address(&entry.url);
    let selected = state.history_page.selected.contains(&entry.url);
    let favicon = state
        .browser
        .tabs()
        .iter()
        .find(|tab| tab.committed_url == entry.url && tab.url == entry.url && tab.favicon.is_some())
        .and_then(|tab| tab.favicon.as_ref())
        .map_or_else(
            || icon(&icons::PAGE, theme::ICON).color(theme.icon),
            |pixels| el().size(theme::ICON, theme::ICON).pixels(pixels),
        );
    let (open_url, middle_url, delete_url, toggle_url) = (
        entry.url.clone(),
        entry.url.clone(),
        entry.url.clone(),
        entry.url.clone(),
    );
    let key_url = open_url.clone();
    let check = Checkbox::new(selected)
        .label(format!("Select {title}"))
        .hide_label()
        .disabled(state.history_page.writing)
        .on_toggle(move |state: &mut AppState| state.toggle_history_selection(&toggle_url))
        .build(kit)
        .key(format!("history-select:{}", entry.url));
    let open = row()
        .key(entry_key(&entry.url))
        .h_full()
        .grow(1.0)
        .min_w(0.0)
        .gap(space::S3)
        .center_y()
        .radius(theme::ROW_RADIUS)
        .focus_ring(theme::FOCUS_WIDTH, 0.0, theme.focus)
        .label(format!("{} — {}", title, entry.url))
        .help(&entry.url)
        .role("link")
        .focusable(true)
        .on_key(move |state: &mut AppState, event, tx| {
            if event.key == Key::Enter {
                if !tx.key_repeat() {
                    state.open_history_entry(&key_url, false, tx);
                }
                true
            } else if matches!(&event.key, Key::Text(text) if text == " ")
                && event.mods == rune::Mods::default()
            {
                state.toggle_history_selection(&key_url);
                true
            } else if event.mods == rune::Mods::default() {
                navigate_entries(state, &key_url, &event.key, tx)
            } else {
                false
            }
        })
        .on_press_tx(move |state: &mut AppState, tx| state.open_history_entry(&open_url, false, tx))
        .on_middle_press(move |state: &mut AppState, tx| {
            state.open_history_entry(&middle_url, true, tx)
        })
        .child(
            text(clock_time(&row_entry.time))
                .font(type_::BODY)
                .color(theme.text_muted)
                .single_line()
                .w(TIME_WIDTH),
        )
        .child(el().child(favicon).a11y_hidden())
        .child(
            row()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::S3)
                .center_y()
                .child(
                    text(title)
                        .font(type_::BODY)
                        .color(theme.text)
                        .single_line()
                        .truncate()
                        .shrink(),
                )
                .child(
                    text(address)
                        .font(type_::BODY)
                        .color(theme.text_muted)
                        .single_line()
                        .truncate()
                        .shrink(),
                ),
        );
    let label = format!("Remove all visits to {}", entry.url);
    let remove = controls::icon_button(
        &format!("history-remove:{}", entry.url),
        &label,
        &icons::CLOSE,
        theme::ICON_BUTTON,
        theme,
    )
    .help("Remove all visits to this address")
    .group_opacity(0.0, 1.0)
    .focusable(!state.history_page.writing)
    .disabled(state.history_page.writing)
    .on_press(move |state: &mut AppState| {
        state.mutate_history(HistoryMutation::Remove(vec![delete_url.clone()]))
    });
    let content = row()
        .w_full()
        .h(ROW_HEIGHT)
        .center_y()
        .px(theme::PAGE_GROUP_PADDING)
        .gap(theme::PAGE_GROUP_PADDING)
        .hover_group()
        .hover(theme.hover)
        .child(check)
        .child(open)
        .child(remove);
    let row = col()
        .key(format!("history-entry:{}", entry.url))
        .w_full()
        .fill(if selected {
            theme.selection
        } else {
            rune::Color::TRANSPARENT
        })
        .transition(if quiet {
            Transition::none()
        } else {
            Transition::opacity()
        });
    let row = if quiet {
        row.instant_changes()
    } else {
        row.arrives_whole()
    };
    row.children((!first).then(|| layout::separator(theme)))
        .child(content)
}

fn pagination(state: &AppState, kit: &Kit) -> Option<El> {
    let history = &state.history_page;
    let older = history
        .page
        .as_ref()
        .is_some_and(|page| page.next.is_some());
    let newer = !history.previous.is_empty();
    (older || newer).then(|| {
        row()
            .key("history-pagination")
            .w_full()
            .gap(space::S2)
            .child(
                Button::new("Newer")
                    .variant(ButtonVariant::Ghost)
                    .disabled(!newer || history.loading || history.writing)
                    .on_press_tx(|state: &mut AppState, tx| state.move_history_page(false, tx))
                    .build(kit)
                    .key("history-newer"),
            )
            .child(
                Button::new("Older")
                    .variant(ButtonVariant::Ghost)
                    .disabled(!older || history.loading || history.writing)
                    .on_press_tx(|state: &mut AppState, tx| state.move_history_page(true, tx))
                    .build(kit)
                    .key("history-older"),
            )
    })
}

fn empty(title: &str, detail: &str, theme: &Theme) -> El {
    layout::message(title, detail, theme).key("history-empty")
}

fn confirmation(theme: &Theme, overlays: &mut Vec<El>) {
    let kit = theme.kit(theme.overlay_panel);
    let actions = row()
        .w_full()
        .gap(space::S2)
        .justify_end()
        .child(
            Button::new("Cancel")
                .variant(ButtonVariant::Neutral)
                .on_press(|state: &mut AppState| state.history_page.confirming = false)
                .build(&kit)
                .key("history-cancel")
                .autofocus(),
        )
        .child(
            Button::new("Clear History")
                .variant(ButtonVariant::Danger)
                .on_press(|state: &mut AppState| state.mutate_history(HistoryMutation::Clear))
                .build(&kit)
                .key("history-confirm-clear"),
        );
    Dialog::new("history-confirm", "Clear browsing history?")
        .open(true)
        .fit()
        .width(400.0)
        .click_outside(false)
        .on_close(|state: &mut AppState| state.history_page.confirming = false)
        .child(
            col()
                .w_full()
                .px(SECTION_GAP)
                .pb(SECTION_GAP)
                .gap(SECTION_GAP)
                .child(
                    text("This will remove your saved browsing history from this device.")
                        .font(type_::BODY)
                        .color(theme.text_muted)
                        .wrap(),
                )
                .child(actions),
        )
        .build(&kit, overlays);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_times_read_naturally() {
        assert_eq!(
            full_date("2026-11-21").as_deref(),
            Some("Saturday, November 21, 2026")
        );
        assert_eq!(
            full_date("2026-10-28").as_deref(),
            Some("Wednesday, October 28, 2026")
        );
        assert_eq!(
            full_date("2000-02-29").as_deref(),
            Some("Tuesday, February 29, 2000")
        );
        assert_eq!(full_date("garbage"), None);
        assert_eq!(clock_time("00:05"), "12:05 AM");
        assert_eq!(clock_time("13:03"), "1:03 PM");
        assert_eq!(clock_time("12:30"), "12:30 PM");
    }
}
