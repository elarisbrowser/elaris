// SPDX-License-Identifier: GPL-3.0-only

//! The shared omnibox editor and results, hosted by the active tab layout.

use crate::ui::menu::{Menu, MenuItem};
use rune::tok::type_;
use rune::{Cx, El, ScrollSpec, Transition, Tx, col, el, icon, row, text, text_input};

use crate::address::SearchEngine;
use crate::app::{self, AppState};
use crate::omnibox::{Action, Glyph, Section, Suggestion};
use crate::ui::theme::{self, Theme};

pub(crate) fn view(
    state: &AppState,
    theme: &Theme,
    height: f32,
    reduced_motion: bool,
    overlays: &mut Vec<El>,
) -> Option<El> {
    if !state.omnibox.open {
        return None;
    }

    let suggestions = state.suggestions();
    let input = row()
        .key("omnibox-input-row")
        .w_full()
        .h(theme::OMNIBOX_INPUT_HEIGHT)
        .px(theme::FIELD_PADDING + 4.0)
        .gap(theme::ROW_GAP + 4.0)
        .center_y()
        .child(search_engine_selector(state, theme, overlays))
        .child(editor(state, &suggestions, theme));

    let available = (height - 2.0 * theme::FIELD_PADDING).min(theme::OMNIBOX_MAX_HEIGHT);
    let results_limit = available - theme::OMNIBOX_INPUT_HEIGHT - theme::OMNIBOX_FOOTER_HEIGHT;
    let results = results(state, &suggestions, theme, results_limit);
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
        .transition(Transition::none())
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

fn editor(state: &AppState, suggestions: &[Suggestion], theme: &Theme) -> El {
    let style = if state.horizontal_tabs() {
        type_::BODY
    } else {
        theme::OMNIBOX_INPUT_TEXT
    };
    let mut field = text_input(&state.omnibox.input, style);
    if !suggestions.is_empty() {
        field = field.active_descendant(format!("omnibox-result-{}", state.omnibox.selected));
    }
    if !state.horizontal_tabs() {
        field = field.focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus);
    }
    field
        .key("omnibox-input")
        .single_line()
        .grow(1.0)
        .min_w(0.0)
        .color(theme.text)
        .caret(theme.text, theme.selection)
        .placeholder("Search or type a URL", theme.text_muted)
        .label("Search or type a URL")
        .autofocus()
        .on_edit(|state: &mut AppState| &mut state.omnibox.input)
        .on_input_change(|state: &mut AppState, tx| {
            tx.instant();
            state.omnibox.selected = 0;
            state.omnibox.selection_moved = false;
            state.request_history_search();
        })
        .on_input_key(app::omnibox_input_key)
        .on_submit(app::submit_omnibox)
}

fn results(state: &AppState, suggestions: &[Suggestion], theme: &Theme, results_limit: f32) -> El {
    let results = col()
        .key("omnibox-results")
        .w_full()
        .max_h(results_limit.max(0.0))
        .px(if state.horizontal_tabs() {
            0.0
        } else {
            theme::ROW_PADDING
        })
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .role("listbox")
        .label("Suggestions");
    let height = if state.horizontal_tabs() {
        theme::ROW_HEIGHT
    } else {
        theme::OMNIBOX_ROW_HEIGHT
    };
    let mut rows = Vec::new();
    let mut y = 0.0;
    let mut selected_y = None;
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
                rows.push(
                    row()
                        .h(theme::OMNIBOX_LABEL_HEIGHT)
                        .pl(theme::ROW_PADDING)
                        .center_y()
                        .child(text(label).font(type_::LABEL).color(theme.text_muted)),
                );
                y += theme::OMNIBOX_LABEL_HEIGHT + theme::TAB_GAP;
            }
        }
        previous = Some(suggestion.section);
        if index == state.omnibox.selected {
            selected_y = Some(y);
        }
        rows.push(result_row(state, suggestion, index, height, theme));
        y += height + theme::TAB_GAP;
    }
    let mut content = col()
        .key("omnibox-result-rows")
        .w_full()
        .gap(theme::TAB_GAP);
    if let Some(y) = selected_y {
        content = content.child(super::hover_highlight::selection(
            "omnibox-result-highlight",
            y,
            height,
            theme::ROW_RADIUS,
            theme,
            state.sidebar.hover.reduced.get(),
        ));
    }
    results.child(content.children(rows))
}

pub(super) fn address_bar(
    state: &AppState,
    cx: &Cx<AppState>,
    theme: &Theme,
    reduced: bool,
    overlays: &mut Vec<El>,
) -> El {
    let address = state
        .browser
        .active_tab()
        .map(|tab| tab.url.as_str())
        .filter(|url| !url.is_empty() && *url != "about:blank");
    let editing =
        state.omnibox.open && state.browser.peek.is_none() && state.tab_switcher.is_none();
    let mut bar = row()
        .key("horizontal-address-bar")
        .w(0.0)
        .min_w(0.0)
        .grow(1.0)
        .h(theme::HORIZONTAL_FIELD_HEIGHT)
        .px(theme::ROW_PADDING)
        .gap(theme::ROW_GAP)
        .center_y()
        .radius(theme::HORIZONTAL_FIELD_RADIUS)
        .continuous_corners()
        .fill(theme.address_field)
        .token_motion(if reduced {
            rune::Spring::NONE
        } else {
            theme::OMNIBOX_MOTION
        });
    if !editing && state.browser.active_tab().is_some() {
        bar = bar.child(super::sidebar::site_controls(theme, theme::CLOSE_BUTTON).press_scale(1.0));
    }
    if editing {
        let suggestions = state.suggestions();
        let mut engine_menu = Vec::new();
        bar = bar
            .child(search_engine_selector(state, theme, &mut engine_menu))
            .child(editor(state, &suggestions, theme));
        if let Some([x, y, width, height]) = cx.window_layout("horizontal-address-bar") {
            let top = y + height + theme::NAV_GAP;
            overlays.push(
                el().key("address-results-dismiss")
                    .absolute(0.0, theme::HORIZONTAL_CHROME_HEIGHT)
                    .w_full()
                    .h_full()
                    .a11y_hidden()
                    .on_press_tx(|state: &mut AppState, tx| {
                        state.omnibox.close();
                        tx.focus(super::viewport::focus_key(&state.browser), false);
                    }),
            );
            if !suggestions.is_empty() || state.address_error.is_some() {
                let available = cx
                    .layout_size("window")
                    .map_or(theme::OMNIBOX_MAX_HEIGHT, |size| size[1] - top)
                    .min(theme::OMNIBOX_MAX_HEIGHT);
                let panel = col()
                    .key("address-suggestions")
                    .absolute(x, top)
                    .w(width)
                    .padding(theme::ROW_PADDING)
                    .radius(theme::FIELD_RADIUS)
                    .continuous_corners()
                    .fill(theme.overlay_panel)
                    .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
                    .shadow(
                        theme::OMNIBOX_SHADOW_Y,
                        theme::OMNIBOX_SHADOW_BLUR,
                        theme.overlay_shadow,
                    )
                    .preserve_focus_on_press(true)
                    .transition(if reduced {
                        Transition::none()
                    } else {
                        Transition::offset(0.0, -theme::TAB_GAP).combined(Transition::opacity())
                    })
                    .child(results(
                        state,
                        &suggestions,
                        theme,
                        available - 2.0 * theme::ROW_PADDING,
                    ));
                overlays.push(if let Some(error) = &state.address_error {
                    panel.child(
                        text(error)
                            .font(type_::LABEL)
                            .color(theme.text_muted)
                            .px(theme::ROW_PADDING),
                    )
                } else {
                    panel
                });
            }
        }
        // The picker must receive presses before the suggestions and their dismissal surface.
        overlays.extend(engine_menu);
    } else {
        let label = address
            .map(crate::address::compact_address)
            .unwrap_or("Search or type a URL");
        let zoom = super::sidebar::header::zoom_indicator(state, theme, overlays);
        let activity = state
            .browser
            .active_tab()
            .map(|tab| super::permission_prompt::address_indicators(tab, theme))
            .unwrap_or_default();
        bar = bar.child(
            row()
                .key("omnibox-launcher")
                .w(0.0)
                .min_w(0.0)
                .grow(1.0)
                .h_full()
                .center_y()
                .role("button")
                .label("Search or type a URL")
                .help(state.status().or(address).unwrap_or("Search or type a URL"))
                .focusable(true)
                .shortcut_hint("cmd+l")
                .on_press_tx(app::open_from_launcher)
                .child(
                    text(label)
                        .font(type_::BODY)
                        .color(if address.is_some() {
                            theme.text
                        } else {
                            theme.text_muted
                        })
                        .single_line()
                        .truncate()
                        .grow(1.0),
                )
                .children(activity)
                .children(zoom),
        );
    }
    bar
}

fn search_engine_selector(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) -> El {
    let engine = state.settings.search_engine;
    let label = format!("Search engine: {}", engine.label());
    let height = if state.horizontal_tabs() {
        theme::CLOSE_BUTTON
    } else {
        theme::OMNIBOX_SEARCH_BUTTON_HEIGHT
    };
    let trigger = row()
        .key("search-engine-button")
        .h(height)
        .px(theme::OMNIBOX_SEARCH_BUTTON_PADDING)
        .gap(theme::OMNIBOX_SEARCH_BUTTON_GAP)
        .center_y()
        .radius(height / 2.0)
        .fill(theme.address_field)
        .hover(theme.hover)
        .press(theme.press)
        .press_scale(if state.horizontal_tabs() {
            1.0
        } else {
            theme::PRESS_SCALE
        })
        .label(label.clone())
        .help(format!("{label}. Click to change."))
        .role("button")
        .focusable(true)
        .on_press(|state: &mut AppState| {
            state.omnibox.search_engine_menu_open = !state.omnibox.search_engine_menu_open;
        })
        .child(super::icons::search_engine(
            engine,
            theme::OMNIBOX_SEARCH_ICON,
        ))
        .child(icon(&super::icons::CHEVRON_DOWN, theme::OMNIBOX_SEARCH_CHEVRON).color(theme.icon));
    let mut menu = Menu::new("search-engine-menu")
        .trigger(trigger)
        .group_label("Search with");
    for option in SearchEngine::ALL {
        menu = menu.item(
            MenuItem::new(option.label())
                .lead(super::icons::search_engine(option, theme::ICON))
                .checked(option == engine)
                .on_select_tx(move |state: &mut AppState, tx: &mut Tx| {
                    state.set_search_engine(option);
                    state.omnibox.search_engine_menu_open = false;
                    tx.focus("omnibox-input", false);
                }),
        );
    }
    let menu = menu
        .open(state.omnibox.search_engine_menu_open)
        .on_close(|state: &mut AppState| state.omnibox.search_engine_menu_open = false);
    super::hover_highlight::build_highlight_menu(
        state,
        menu,
        Some("search-engine-menu-menu"),
        theme,
        overlays,
    )
}

fn result_row(
    state: &AppState,
    suggestion: &Suggestion,
    index: usize,
    height: f32,
    theme: &Theme,
) -> El {
    let glyph = glyph_for(state, suggestion, theme);
    let choice = suggestion.clone();
    let key = format!("omnibox-result-{index}");
    let mut row = row()
        .key(key)
        .w_full()
        .h(height)
        .px(theme::ROW_PADDING)
        .gap(theme::ROW_GAP)
        .center_y()
        .radius(theme::ROW_RADIUS)
        .continuous_corners()
        .label(if suggestion.detail.is_empty() {
            suggestion.title.clone()
        } else {
            format!("{}, {}", suggestion.title, suggestion.detail)
        })
        .role("option")
        .on_hover_change(move |state: &mut AppState, inside, tx| {
            if inside && state.omnibox.selected != index {
                state.omnibox.selected = index;
                tx.animated();
            } else {
                tx.live_only();
            }
        })
        .on_press_tx(move |state: &mut AppState, tx| {
            state.choose_suggestion(choice.clone(), tx);
        });
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
