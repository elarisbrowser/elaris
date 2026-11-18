// SPDX-License-Identifier: GPL-3.0-only

//! Rune's native, window-centred presentation for the omnibox.

use rune::tok::type_;
use rune::{El, ScrollSpec, Transition, col, el, icon, row, text, text_input};

use crate::app::{self, AppState};
use crate::omnibox::{Action, Glyph, Section, Suggestion};
use crate::ui::theme::{self, Theme};

pub(crate) fn view(
    state: &AppState,
    theme: &Theme,
    height: f32,
    reduced_motion: bool,
) -> Option<El> {
    if !state.omnibox.open {
        return None;
    }

    let suggestions = state.suggestions();
    let mut field = text_input(&state.omnibox.input, theme::OMNIBOX_INPUT_TEXT);
    if !suggestions.is_empty() {
        field = field.active_descendant(format!("omnibox-result-{}", state.omnibox.selected));
    }
    let input = row()
        .key("omnibox-input-row")
        .w_full()
        .h(theme::OMNIBOX_INPUT_HEIGHT)
        .px(theme::FIELD_PADDING + 4.0)
        .gap(theme::ROW_GAP + 4.0)
        .center_y()
        .child(icon(&super::icons::SEARCH, theme::OMNIBOX_SEARCH_ICON).color(theme.icon))
        .child(
            field
                .key("omnibox-input")
                .single_line()
                .grow(1.0)
                .min_w(0.0)
                .color(theme.text)
                .placeholder("Search or type a URL", theme.text_muted)
                .label("Search or type a URL")
                .autofocus()
                .on_edit(|state: &mut AppState| &mut state.omnibox.input)
                .on_input_change(move |state: &mut AppState, tx| {
                    if reduced_motion {
                        tx.instant();
                    } else {
                        tx.animated();
                    }
                    state.omnibox.selected = 0;
                    state.omnibox.selection_moved = false;
                })
                .on_input_key(app::omnibox_input_key)
                .on_submit(app::submit_omnibox),
        );

    let available = (height - 2.0 * theme::FIELD_PADDING).min(theme::OMNIBOX_MAX_HEIGHT);
    let results_limit = available - theme::OMNIBOX_INPUT_HEIGHT - theme::OMNIBOX_FOOTER_HEIGHT;
    let mut results = col()
        .key("omnibox-results")
        .instant_changes()
        .w_full()
        .max_h(results_limit.max(0.0))
        .px(theme::ROW_PADDING)
        .gap(theme::TAB_GAP)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .role("listbox")
        .label("Suggestions");
    let mut previous = None;
    for (index, suggestion) in suggestions.iter().enumerate() {
        if previous != Some(suggestion.section) && suggestions.len() > 1 {
            let label = match suggestion.section {
                Section::OpenTabs => Some("Open tabs"),
                Section::Recent => Some("Recent"),
                Section::QuickActions => Some("Actions"),
                Section::Navigate => None,
            };
            if let Some(label) = label {
                results = results.child(
                    row()
                        .h(theme::OMNIBOX_LABEL_HEIGHT)
                        .pl(theme::ROW_PADDING)
                        .center_y()
                        .child(text(label).font(type_::LABEL).color(theme.text_muted)),
                );
            }
        }
        previous = Some(suggestion.section);
        results = results.child(result_row(state, suggestion, index, theme));
    }
    let hints = match suggestions.len() {
        0 => state.address_error.as_deref().unwrap_or("Esc  Close"),
        1 if !state.omnibox.input.text.trim().is_empty() || state.omnibox.selection_moved => {
            "↵  Open     Esc  Close"
        }
        _ => "↑↓  Navigate     ↵  Open     Esc  Close",
    };
    let footer = row()
        .key("omnibox-hints")
        .w_full()
        .h(theme::OMNIBOX_FOOTER_HEIGHT)
        .px(theme::FIELD_PADDING + 4.0)
        .center_y()
        .justify_end()
        .child(text(hints).font(type_::LABEL).color(theme.text_muted));

    let panel = col()
        .key("omnibox-panel")
        .w_full()
        .max_w(theme::OMNIBOX_WIDTH)
        .radius(theme::OMNIBOX_RADIUS)
        .continuous_corners()
        .fill(theme.overlay_panel)
        .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
        .shadow(
            theme::OMNIBOX_SHADOW_Y,
            theme::OMNIBOX_SHADOW_BLUR,
            theme.overlay_shadow,
        )
        .clip_rounded(theme::OMNIBOX_RADIUS)
        .preserve_focus_on_press(true)
        .modal_tx(true, |state: &mut AppState, tx| {
            state.omnibox.close();
            tx.focus(crate::ui::VIEWPORT, false);
        })
        .role("dialog")
        .label("Search and navigation")
        .token_motion(if reduced_motion {
            rune::Spring::NONE
        } else {
            theme::OMNIBOX_MOTION
        })
        .transition(Transition::opacity())
        .child(input)
        .child(results)
        .child(footer);

    Some(
        col()
            .key("omnibox-scrim")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .px(theme::FIELD_PADDING + 4.0)
            .py(theme::FIELD_PADDING)
            .center()
            .fill(theme.overlay_scrim)
            .child(panel),
    )
}

fn result_row(state: &AppState, suggestion: &Suggestion, index: usize, theme: &Theme) -> El {
    let selected = state.omnibox.selected == index;
    let glyph = glyph_for(state, suggestion, theme);
    let choice = suggestion.clone();
    let key = format!("omnibox-result-{index}");
    let mut row = row()
        .key(key)
        .w_full()
        .h(theme::OMNIBOX_ROW_HEIGHT)
        .px(theme::ROW_PADDING)
        .gap(theme::ROW_GAP)
        .center_y()
        .radius(theme::ROW_RADIUS)
        .continuous_corners()
        .hover(theme.hover)
        .label(if suggestion.detail.is_empty() {
            suggestion.title.clone()
        } else {
            format!("{}, {}", suggestion.title, suggestion.detail)
        })
        .role("option")
        .on_hover_change(move |state: &mut AppState, inside, _| {
            if inside {
                state.omnibox.selected = index;
            }
        })
        .on_press_tx(move |state: &mut AppState, tx| {
            state.choose_suggestion(choice.clone(), tx);
        });
    if selected {
        row = row.fill(theme.overlay_selected);
    }
    row = row
        .child(
            el().size(theme::CLOSE_BUTTON, theme::CLOSE_BUTTON)
                .center()
                .radius(theme::ICON_BUTTON_RADIUS)
                .fill(theme.hover)
                .child(glyph),
        )
        .child(
            text(&suggestion.title)
                .font(type_::BODY)
                .color(theme.text)
                .single_line()
                .truncate(),
        );
    if suggestion.detail.is_empty() {
        return row;
    }
    row.child(
        text(&suggestion.detail)
            .font(type_::LABEL)
            .color(theme.text_muted)
            .single_line()
            .truncate(),
    )
}

fn glyph_for(state: &AppState, suggestion: &Suggestion, theme: &Theme) -> El {
    if let Action::SwitchTab(id) = suggestion.action
        && let Some(favicon) = state.browser.tab(id).and_then(|tab| tab.favicon.as_ref())
    {
        return el().size(theme::ICON, theme::ICON).center().pixels(favicon);
    }

    let glyph = match suggestion.glyph {
        Glyph::Search => &super::icons::SEARCH,
        Glyph::Page => &super::icons::PAGE,
        Glyph::History => &super::icons::HISTORY,
        Glyph::NewTab => &super::icons::PLUS,
        Glyph::Reload => &super::icons::RELOAD,
        Glyph::Close => &super::icons::CLOSE,
        Glyph::Sidebar => &super::icons::SIDEBAR,
    };
    icon(glyph, theme::ICON).color(theme.icon)
}
