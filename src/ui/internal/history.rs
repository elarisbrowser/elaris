// SPDX-License-Identifier: GPL-3.0-only

//! The native latest-visited list and its history-only controls.

use elaris::history::{Page, PageEntry};
use rune::tok::{space, type_};
use rune::{El, GrowthSpec, Key, ScrollSpec, Transition, col, el, icon, row, text, text_input};
use rune_kit::{Button, ButtonVariant, Dialog, Kit};

use crate::address::compact_address;
use crate::app::AppState;
use crate::storage::HistoryMutation;
use crate::ui::{
    HISTORY_SCROLL, HISTORY_SEARCH, controls, icons,
    theme::{self, Theme},
};

const CONTENT_WIDTH: f32 = 860.0;
const ROW_HEIGHT: f32 = 56.0;
const SECTION_GAP: f32 = 16.0;

pub(super) fn view(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
) -> El {
    let history = &state.history_page;
    let kit = theme.kit(theme.settings_surface);
    if history.confirming {
        confirmation(theme, overlays);
    }
    let has_entries = history
        .page
        .as_ref()
        .is_some_and(|page| !page.entries.is_empty());
    let clear = Button::new("Clear History")
        .variant(ButtonVariant::GhostMuted)
        .disabled(
            history.writing
                || (!has_entries
                    && history.input.text.trim().is_empty()
                    && history.previous.is_empty()),
        )
        .on_press(|state: &mut AppState| state.history_page.confirming = true)
        .build(&kit)
        .key("history-clear");
    let header = col()
        .w_full()
        .gap(SECTION_GAP)
        .child(
            col()
                .gap(space::S1)
                .child(
                    text("History")
                        .font(theme::PAGE_TITLE)
                        .color(theme.text)
                        .role("heading"),
                )
                .child(
                    text("Your most recent visit to each address.")
                        .font(type_::BODY)
                        .color(theme.text_muted),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(space::S2)
                .center_y()
                .child(search(state, theme))
                .child(clear),
        );
    let mut results = col().key("history-results").w_full().gap(space::S2);
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
    results = results.child(entries(state, theme, reduced_motion));
    let content = col()
        .key("history-content")
        .w_full()
        .h_full()
        .max_w(CONTENT_WIDTH)
        .px(space::S6)
        .pt(48.0)
        .pb(space::S3)
        .gap(SECTION_GAP)
        .child(header)
        .child(
            col()
                .key(HISTORY_SCROLL)
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(results),
        )
        .children(pagination(state, &kit));
    row()
        .key("history-page")
        .w_full()
        .h_full()
        .justify_center()
        .child(content)
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
        .grow(1.0)
        .min_w(0.0)
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

fn entries(state: &AppState, theme: &Theme, reduced_motion: bool) -> El {
    let list = col().key("history-entries").w_full();
    let mut list = if reduced_motion {
        list.child_exit(Transition::none()).instant_changes()
    } else {
        list.growth(GrowthSpec::default())
    };
    if let Some(page) = &state.history_page.page {
        let mut previous_date = None;
        for entry in &page.entries {
            if previous_date != Some(entry.date.as_str()) {
                list = list.child(
                    text(date_label(&entry.date, page))
                        .key(format!("history-date-{}", entry.date))
                        .font(type_::LABEL)
                        .color(theme.text_muted)
                        .role("heading")
                        .px(space::S2)
                        .pt(if previous_date.is_none() {
                            space::S2
                        } else {
                            SECTION_GAP
                        })
                        .pb(space::S1)
                        .arrives_whole(),
                );
                previous_date = Some(&entry.date);
            }
            list = list.child(entry_row(state, entry, theme));
        }
    }
    list
}

fn date_label(date: &str, page: &Page) -> String {
    if date == page.today {
        return "Today".into();
    }
    if date == page.yesterday {
        return "Yesterday".into();
    }
    let mut parts = date.split('-');
    let months = [
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
    if let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next())
        && let Ok(month) = month.parse::<usize>()
        && let Some(month) = month.checked_sub(1).and_then(|index| months.get(index))
    {
        return format!("{} {month} {year}", day.trim_start_matches('0'));
    }
    "Unknown date".into()
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

fn entry_row(state: &AppState, row_entry: &PageEntry, theme: &Theme) -> El {
    let entry = &row_entry.entry;
    let title = if entry.title.trim().is_empty() {
        &entry.url
    } else {
        &entry.title
    };
    let address = compact_address(&entry.url);
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
    let (open_url, middle_url, delete_url) =
        (entry.url.clone(), entry.url.clone(), entry.url.clone());
    let key_url = open_url.clone();
    let open = row()
        .key(entry_key(&entry.url))
        .h_full()
        .grow(1.0)
        .min_w(0.0)
        .gap(space::S3)
        .center_y()
        .pl(space::S2)
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
            el().size(30.0, 30.0)
                .radius(theme::FIELD_RADIUS)
                .fill(theme.settings_surface)
                .center()
                .child(favicon)
                .a11y_hidden(),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::S1)
                .child(
                    text(title)
                        .font(type_::BODY)
                        .color(theme.text)
                        .single_line()
                        .truncate(),
                )
                .child(
                    text(address)
                        .font(type_::LABEL)
                        .color(theme.text_muted)
                        .single_line()
                        .truncate(),
                ),
        )
        .child(
            text(&row_entry.time)
                .font(type_::LABEL)
                .color(theme.text_muted)
                .single_line(),
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
        state.mutate_history(HistoryMutation::Remove(delete_url.clone()))
    });
    row()
        .key(format!("history-entry:{}", entry.url))
        .w_full()
        .h(ROW_HEIGHT)
        .center_y()
        .pr(space::S2)
        .gap(space::S2)
        .radius(theme::ROW_RADIUS)
        .hover_group()
        .hover(theme.hover)
        .arrives_whole()
        .transition(Transition::opacity())
        .child(open)
        .child(remove)
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
    col()
        .key("history-empty")
        .w_full()
        .items_center()
        .gap(space::S2)
        .py(space::S6)
        .child(icon(&icons::HISTORY, 20.0).color(theme.icon).a11y_hidden())
        .child(
            text(title)
                .font(type_::BODY)
                .color(theme.text)
                .role("heading"),
        )
        .child(
            text(detail)
                .font(type_::BODY)
                .color(theme.text_muted)
                .wrap(),
        )
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
