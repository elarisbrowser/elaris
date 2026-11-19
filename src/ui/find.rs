// SPDX-License-Identifier: GPL-3.0-only

//! The compact Find in Page bar anchored to the active webpage.

use rune::tok::type_;
use rune::{El, Key, KeyEvent, Spring, Transition, Tx, row, text, text_input};

use crate::app::AppState;
use crate::tab::{FindState, Tab};
use crate::ui::theme::{self, Theme};

pub(crate) fn view(
    state: &AppState,
    theme: &Theme,
    reduced_motion: bool,
    page_origin: [f32; 2],
) -> Option<El> {
    if !state.find_open {
        return None;
    }

    let tab = state
        .find_tab
        .and_then(|id| state.browser.tab(id))
        .filter(|tab| Some(tab.id()) == state.browser.active_tab().map(Tab::id));
    let unavailable = match tab {
        Some(tab) if tab.is_internal() => Some("Find is unavailable on Elaris pages"),
        Some(_) if !cfg!(feature = "ladybird-bridge") => {
            Some("Find requires the Ladybird web engine")
        }
        None => Some("No page to search"),
        _ => None,
    };
    let no_matches = tab.is_none_or(|tab| {
        tab.find.query.is_empty()
            || tab
                .find
                .result
                .is_some_and(|result| result.total_match_count == Some(0))
    });
    let counter = tab.map_or_else(String::new, |tab| result_label(&tab.find));

    let mut bar = row()
        .key("find-bar")
        .w_full()
        .max_w(theme::FIND_WIDTH)
        .h(theme::FIND_HEIGHT)
        .px(theme::FIND_PADDING)
        .gap(theme::FIND_GAP)
        .center_y()
        .radius(theme::FIND_RADIUS)
        .continuous_corners()
        .fill(theme.overlay_panel)
        .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
        .shadow(
            theme::FIND_SHADOW_Y,
            theme::FIND_SHADOW_BLUR,
            theme.overlay_shadow,
        )
        .preserve_focus_on_press(true)
        .role("group")
        .label("Find in page")
        .transition(if reduced_motion {
            Transition::none()
        } else {
            Transition::offset(0.0, -4.0).combined(Transition::opacity())
        })
        .token_motion(if reduced_motion {
            Spring::NONE
        } else {
            theme::FIND_MOTION
        });

    if let Some(message) = unavailable {
        bar = bar.child(
            text(message)
                .key("find-unavailable")
                .font(type_::LABEL)
                .color(theme.text_muted)
                .single_line()
                .truncate_fade()
                .grow(1.0)
                .min_w(0.0),
        );
    } else {
        let mut field = text_input(&state.find_input, theme::FIND_INPUT_TEXT)
            .key(crate::ui::FIND_INPUT)
            .single_line()
            .grow(1.0)
            .min_w(0.0)
            .color(theme.text)
            .caret(theme.text, theme.selection)
            .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
            .placeholder("Search this page", theme.text_muted)
            .label("Find in page")
            .autofocus()
            .on_edit(|state: &mut AppState| &mut state.find_input)
            .on_input_change(|state: &mut AppState, tx| {
                tx.instant();
                state.find_input_changed();
            })
            .on_input_key(find_input_key);
        if no_matches {
            field = field.caret(theme.text_muted, theme.selection);
        }
        bar = bar
            .child(field)
            .child(
                text(counter)
                    .key("find-result-count")
                    .font(type_::LABEL)
                    .color(theme.text_muted)
                    .single_line()
                    .truncate()
                    .label(counter_label(tab.map(|tab| &tab.find))),
            )
            .child(button(
                "find-previous-button",
                "Previous match",
                &crate::ui::icons::CHEVRON_UP,
                theme,
                no_matches,
                |state, tx| {
                    if let Some(tab) = state.find_tab {
                        let _ = state.browser.find_previous(tab);
                        tx.focus(crate::ui::FIND_INPUT, false);
                    }
                },
            ))
            .child(button(
                "find-next-button",
                "Next match",
                &crate::ui::icons::CHEVRON_DOWN,
                theme,
                no_matches,
                |state, tx| {
                    if let Some(tab) = state.find_tab {
                        let _ = state.browser.find_next(tab);
                        tx.focus(crate::ui::FIND_INPUT, false);
                    }
                },
            ));
    }

    bar = bar.child(button(
        "find-close-button",
        "Close find",
        &crate::ui::icons::CLOSE,
        theme,
        false,
        |state, tx| state.close_find(tx),
    ));

    Some(
        row()
            .key("find-position")
            .pin(
                Some(page_origin[0] + theme::FIND_INSET),
                Some(page_origin[1] + theme::FIND_INSET),
                Some(
                    theme::VIEWPORT_INSET
                        + theme::FIND_INSET
                        + state
                            .browser
                            .split
                            .filter(|_| state.browser.focused_side() == 0)
                            .map_or(0.0, |_| {
                                state.browser.content_width() - state.browser.viewport_width()
                            }),
                ),
                None,
            )
            .h(theme::FIND_HEIGHT)
            .justify_end()
            .center_y()
            .transition(if reduced_motion {
                Transition::none()
            } else {
                Transition::opacity()
            })
            .child(bar),
    )
}

fn button(
    key: &'static str,
    label: &'static str,
    glyph: &'static rune::Icon,
    theme: &Theme,
    disabled: bool,
    on_press: impl Fn(&mut AppState, &mut Tx) + 'static,
) -> El {
    super::controls::icon_button(key, label, glyph, theme::FIND_BUTTON, theme)
        .role("button")
        .focusable(!disabled)
        .disabled(disabled)
        .opacity(if disabled {
            theme.disabled_opacity
        } else {
            1.0
        })
        .preserve_focus_on_press(true)
        .on_press_tx(on_press)
}

fn result_label(find: &FindState) -> String {
    if find.query.is_empty() {
        return String::new();
    }
    match find.result {
        Some(result) => match result.total_match_count {
            Some(0) => "No matches".to_owned(),
            Some(total) => format!(
                "{}/{}",
                result.current_match_index.saturating_add(1).min(total),
                total
            ),
            None => "Searching…".to_owned(),
        },
        None => "Searching…".to_owned(),
    }
}

fn counter_label(find: Option<&FindState>) -> String {
    find.map_or_else(String::new, |find| {
        let result = result_label(find);
        if result.is_empty() {
            "Search results".to_owned()
        } else {
            result
        }
    })
}

fn find_input_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if crate::ui::tab_switcher::cycle_key(state, event, tx) {
        return true;
    }
    match &event.key {
        Key::Escape => {
            state.close_find(tx);
            true
        }
        Key::Enter => {
            if let Some(tab) = state.find_tab {
                if event.mods.shift {
                    let _ = state.browser.find_previous(tab);
                } else {
                    let _ = state.browser.find_next(tab);
                }
            }
            true
        }
        Key::Command(key) if key.eq_ignore_ascii_case(&'g') => {
            if let Some(tab) = state.find_tab {
                if event.mods.shift {
                    let _ = state.browser.find_previous(tab);
                } else {
                    let _ = state.browser.find_next(tab);
                }
            }
            true
        }
        Key::Command(key) if key.eq_ignore_ascii_case(&'f') => {
            state.find_input.anchor = 0;
            state.find_input.focus = state.find_input.text.len();
            tx.focus(crate::ui::FIND_INPUT, false);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::FindResult;

    #[test]
    fn find_counter_uses_only_reported_ladybird_results() {
        let mut find = FindState::default();
        assert_eq!(result_label(&find), "");

        find.query = "match".into();
        assert_eq!(result_label(&find), "Searching…");

        find.result = Some(FindResult {
            current_match_index: 0,
            total_match_count: Some(1),
        });
        assert_eq!(result_label(&find), "1/1");

        find.result = Some(FindResult {
            current_match_index: 2,
            total_match_count: Some(12),
        });
        assert_eq!(result_label(&find), "3/12");

        find.result = Some(FindResult {
            current_match_index: 0,
            total_match_count: Some(0),
        });
        assert_eq!(result_label(&find), "No matches");
    }
}
