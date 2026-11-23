// SPDX-License-Identifier: GPL-3.0-only

//! The Task Manager: Ladybird's processes as its own Task Manager lists them, with the browser's
//! processes first, then each site, then the spare processes kept ready for the next site.

use elaris_core::processes::{self, Overview, Process, Site};
use rune::tok::{space, type_};
use rune::{Align, Color, El, Key, Transition, col, el, icon, row, text, text_input};
use rune_kit::{Button, ButtonVariant};

use crate::app::AppState;
use crate::tab::TabId;
use crate::ui::components::controls;
use crate::ui::{
    TASK_MANAGER_SCROLL, TASK_MANAGER_SEARCH, icons,
    theme::{self, Theme},
};

use super::layout::PageLayout;
use super::widgets;
use super::{PageAction, PageContext};

const SEARCH_WIDTH: f32 = 420.0;
/// Shown for a figure a sleeping tab doesn't have.
const NONE: &str = "—";

pub(super) const ACTIONS: &[PageAction] = &[PageAction {
    label: "End Process",
    enabled: |state| selected(state).is_some_and(Process::can_end),
    run: |state, _| state.end_selected_process(),
}];

fn selected(state: &AppState) -> Option<&Process> {
    let page = &state.task_manager;
    page.overview.as_ref()?.process(page.selected?)
}

pub(super) fn view(cx: &mut PageContext) -> PageLayout {
    let (state, theme, kit) = (cx.state, cx.theme, &cx.kit);
    let page = &state.task_manager;
    let end = Button::new("End Process")
        .variant(ButtonVariant::Danger)
        .disabled(!selected(state).is_some_and(Process::can_end))
        .on_press(AppState::end_selected_process)
        .build(kit)
        .key("task-manager-end");
    let toolbar = if cx.compact {
        col()
            .w_full()
            .gap(space::S2)
            .child(search(state, theme))
            .child(row().w_full().justify_end().child(end))
    } else {
        row()
            .w_full()
            .gap(theme::PAGE_GROUP_PADDING)
            .center_y()
            .justify_between()
            .child(search(state, theme))
            .child(end)
    };
    let mut content = col()
        .key("task-manager-content")
        .w_full()
        .gap(theme::PAGE_SECTION_GAP);
    if let Some(error) = &page.action_error {
        content = content.child(
            text(format!("The process could not be ended: {error}"))
                .font(type_::BODY)
                .color(theme.text)
                .wrap()
                .role("alert"),
        );
    }
    let Some(overview) = &page.overview else {
        let message = match &page.load_error {
            Some(error) => widgets::message(&icons::INFO, "Processes unavailable", error, theme),
            None => widgets::message(&icons::ACTIVITY, "Loading processes…", "", theme),
        };
        return PageLayout::new(
            TASK_MANAGER_SCROLL,
            &icons::ACTIVITY,
            content.child(message),
        )
        .toolbar(toolbar);
    };
    content = content.child(summary(overview, theme));
    let shown = overview.filtered(&page.input.text);
    let columns = Columns { pid: !cx.compact };
    let tab_rows = tab_rows(state, overview, &page.input.text, columns, kit, theme);
    if shown.is_empty() && tab_rows.is_empty() {
        content = content.child(
            widgets::message(
                &icons::SEARCH,
                "No matching tabs or processes",
                "Try another search.",
                theme,
            )
            .child(
                Button::new("Clear Search")
                    .variant(ButtonVariant::Ghost)
                    .on_press_tx(AppState::clear_task_manager_search)
                    .build(kit),
            ),
        );
    }
    let searching = !page.input.text.trim().is_empty();
    if !tab_rows.is_empty() {
        content = content.child(section(
            "task-manager-tabs",
            "Tabs",
            columns,
            theme,
            tab_rows,
        ));
    }
    if !shown.browser.is_empty() {
        let rows = shown
            .browser
            .iter()
            .map(|process| process_row(state, process, false, 0.0, columns, theme));
        content = content.child(section(
            "task-manager-browser",
            "Browser",
            columns,
            theme,
            rows,
        ));
    }
    if !shown.sites.is_empty() {
        let mut rows = Vec::new();
        for site in &shown.sites {
            let expanded = searching || page.expanded.contains(&site.key);
            rows.push(site_row(state, site, expanded, columns, theme));
            if expanded {
                rows.extend(site.processes.iter().map(|process| {
                    process_row(
                        state,
                        process,
                        true,
                        theme::TASK_MANAGER_INDENT,
                        columns,
                        theme,
                    )
                }));
            }
        }
        content = content.child(section("task-manager-sites", "Sites", columns, theme, rows));
    }
    if !shown.spare.is_empty() {
        let rows = shown
            .spare
            .iter()
            .map(|process| process_row(state, process, true, 0.0, columns, theme));
        content = content.child(section("task-manager-spare", "Spare", columns, theme, rows));
    }
    PageLayout::new(TASK_MANAGER_SCROLL, &icons::ACTIVITY, content).toolbar(toolbar)
}

/// This window's web pages, with what each uses and whether Memory Saver lets it sleep.
fn tab_rows(
    state: &AppState,
    overview: &Overview,
    query: &str,
    columns: Columns,
    kit: &rune_kit::Kit,
    theme: &Theme,
) -> Vec<El> {
    let query = query.trim().to_lowercase();
    let usage = overview.tab_usage();
    let shown = state.browser.visible_tabs();
    state
        .browser
        .tabs()
        .iter()
        .filter(|tab| !tab.is_internal())
        .filter(|tab| {
            query.is_empty()
                || tab.label().to_lowercase().contains(&query)
                || tab.url.to_lowercase().contains(&query)
        })
        .map(|tab| {
            let id = tab.id();
            let status = if tab.asleep {
                "Sleeping".to_owned()
            } else {
                let reasons = state.awake_reasons(tab);
                if !reasons.is_empty() {
                    let reasons: Vec<_> = reasons.iter().map(|reason| reason.label()).collect();
                    format!("Stays awake: {}", reasons.join(", "))
                } else if state.settings.memory_saver == elaris_memory_saver::MemorySaver::Off {
                    String::new()
                } else {
                    "Can sleep".to_owned()
                }
            };
            let leading = if tab.asleep {
                glyph(&icons::BED, theme)
            } else {
                favicon(state, id.raw()).unwrap_or_else(|| glyph(&icons::PAGE, theme))
            };
            let sleep = (!tab.asleep && !shown.contains(&Some(id))).then(|| {
                Button::new("Sleep")
                    .variant(ButtonVariant::Ghost)
                    .on_press(move |state: &mut AppState| state.sleep_now(id))
                    .build(kit)
            });
            let name = row()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::S2)
                .center_y()
                .child(leading)
                .child(name_and_note(tab.label(), &status, false, theme))
                .children(sleep);
            let used = (!tab.asleep)
                .then(|| usage.get(&id.raw()))
                .flatten()
                .map(|usage| (usage.cpu, usage.memory));
            metrics_row(
                format!("task-manager-tab:{}", id.raw()),
                name,
                used,
                None,
                columns,
                theme,
            )
            .fill(Color::TRANSPARENT)
            .opacity(if tab.asleep {
                theme::ASLEEP_OPACITY
            } else {
                1.0
            })
            .label(format!("{}, {status}", tab.label()))
        })
        .collect()
}

/// Which of the optional columns fit.
#[derive(Clone, Copy)]
struct Columns {
    pid: bool,
}

/// Totals for every process, as the Task Manager's footer gives them.
fn summary(overview: &Overview, theme: &Theme) -> El {
    let figure = |label: &str, value: String| {
        row()
            .gap(space::S1)
            .child(
                text(label.to_owned())
                    .font(type_::BODY)
                    .color(theme.text_muted)
                    .single_line(),
            )
            .child(
                text(value)
                    .font(type_::BODY)
                    .color(theme.text)
                    .single_line(),
            )
    };
    row()
        .key("task-manager-summary")
        .w_full()
        .gap(theme::PAGE_SECTION_GAP)
        .center_y()
        .px(theme::PAGE_GROUP_PADDING)
        .role("status")
        .help("Total CPU and memory for all engine processes. Updates every second.")
        .child(figure("CPU", processes::format_cpu(overview.cpu)))
        .child(figure("Memory", processes::format_memory(overview.memory)))
        .child(
            text(format!(
                "{} · {}",
                processes::plural(overview.sites.len(), "site"),
                processes::plural(overview.count, "process")
            ))
            .font(type_::BODY)
            .color(theme.text_muted)
            .single_line(),
        )
}

/// A titled group whose heading carries the column labels.
fn section(
    key: &'static str,
    title: &str,
    columns: Columns,
    theme: &Theme,
    rows: impl IntoIterator<Item = El>,
) -> El {
    let mut group = widgets::group(theme)
        .child_exit(Transition::none())
        .instant_changes();
    for (index, content) in rows.into_iter().enumerate() {
        if index > 0 {
            group = group.child(widgets::separator(theme));
        }
        group = group.child(content);
    }
    let label = |label: &str, width: f32| {
        text(label.to_owned())
            .font(theme::PAGE_SECTION_TITLE)
            .color(theme.text_muted)
            .align(Align::End)
            .w(width)
            .single_line()
    };
    let heading = row()
        .w_full()
        .center_y()
        .px(theme::PAGE_GROUP_PADDING)
        .child(
            text(title.to_owned())
                .font(theme::PAGE_SECTION_TITLE)
                .color(theme.text_muted)
                .role("heading")
                .grow(1.0),
        )
        .child(label("CPU", theme::TASK_MANAGER_METRIC_WIDTH))
        .child(label("Memory", theme::TASK_MANAGER_METRIC_WIDTH))
        .children(
            columns
                .pid
                .then(|| label("PID", theme::TASK_MANAGER_PID_WIDTH)),
        );
    col()
        .key(key)
        .w_full()
        .gap(theme::PAGE_SECTION_TITLE_GAP)
        .child(heading)
        .child(group)
}

fn site_row(state: &AppState, site: &Site, expanded: bool, columns: Columns, theme: &Theme) -> El {
    let key = site.key.clone();
    let toggle = key.clone();
    let chevron = if expanded {
        &icons::CHEVRON_DOWN
    } else {
        &icons::CHEVRON_RIGHT
    };
    let favicon = site
        .processes
        .iter()
        .flat_map(|process| &process.tabs)
        .find_map(|tab| favicon(state, tab.view));
    let name = row()
        .grow(1.0)
        .min_w(0.0)
        .gap(space::S2)
        .center_y()
        .child(icon(chevron, theme::ICON).color(theme.icon).a11y_hidden())
        .child(favicon.unwrap_or_else(|| glyph(&icons::PAGE, theme)))
        .child(name_and_note(&site.name, &site.note(), true, theme));
    metrics_row(
        format!("task-manager-site:{key}"),
        name,
        Some((site.cpu, site.memory)),
        None,
        columns,
        theme,
    )
    .fill(Color::TRANSPARENT)
    .label(format!(
        "{}, {}, CPU {}, memory {}",
        site.name,
        if expanded { "expanded" } else { "collapsed" },
        processes::format_cpu(site.cpu),
        processes::format_memory(site.memory),
    ))
    .role("button")
    .focusable(true)
    .on_key(move |state: &mut AppState, event, _| {
        if matches!(event.key, Key::Enter) || matches!(&event.key, Key::Text(t) if t == " ") {
            state.toggle_site(&key);
            true
        } else {
            false
        }
    })
    .on_press(move |state: &mut AppState| state.toggle_site(&toggle))
}

fn process_row(
    state: &AppState,
    process: &Process,
    grouped: bool,
    indent: f32,
    columns: Columns,
    theme: &Theme,
) -> El {
    let id = process.id;
    let selected = state.task_manager.selected == Some(id);
    let muted = process.spare;
    let tabs = process
        .tabs
        .iter()
        .map(|tab| {
            if tab.title.is_empty() {
                "Untitled"
            } else {
                &tab.title
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let note = [Some(tabs).filter(|tabs| !tabs.is_empty()), process.note()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let leading = process
        .tabs
        .iter()
        .find_map(|tab| favicon(state, tab.view))
        .unwrap_or_else(|| glyph(process_icon(process), theme));
    let name = process.name(grouped);
    let label = format!(
        "{name}, CPU {}, memory {}, PID {}",
        processes::format_cpu(process.cpu),
        processes::format_memory(process.memory),
        process.pid
    );
    let content = row()
        .grow(1.0)
        .min_w(0.0)
        .gap(space::S2)
        .center_y()
        .pl(indent)
        .child(leading)
        .child(name_and_note(&name, &note, process.is_browser(), theme));
    metrics_row(
        format!("task-manager-process:{id}"),
        content,
        Some((process.cpu, process.memory)),
        Some(process.pid),
        columns,
        theme,
    )
    .opacity(if muted { 0.6 } else { 1.0 })
    .fill(if selected {
        theme.selection
    } else {
        Color::TRANSPARENT
    })
    .label(label)
    .help(if process.can_end() {
        "Select, then End Process to stop it"
    } else {
        "The browser's own process"
    })
    .role("button")
    .focusable(true)
    .on_key(move |state: &mut AppState, event, _| match &event.key {
        Key::Enter => {
            state.select_process(id);
            true
        }
        Key::Text(t) if t == " " => {
            state.select_process(id);
            true
        }
        Key::Backspace | Key::Delete if state.task_manager.selected == Some(id) => {
            state.end_selected_process();
            true
        }
        _ => false,
    })
    .on_press(move |state: &mut AppState| state.select_process(id))
}

/// A row with the name leading and the figures in fixed columns after it.
fn metrics_row(
    key: String,
    name: El,
    usage: Option<(f64, u64)>,
    pid: Option<i64>,
    columns: Columns,
    theme: &Theme,
) -> El {
    let figure = |value: String, width: f32| {
        text(value)
            .font(type_::BODY)
            .color(theme.text_muted)
            .align(Align::End)
            .w(width)
            .single_line()
    };
    row()
        .key(key)
        .w_full()
        .h(theme::TASK_MANAGER_ROW_HEIGHT)
        .center_y()
        .px(theme::PAGE_GROUP_PADDING)
        .hover(theme.hover)
        .focus_ring(theme::FOCUS_WIDTH, -1.0, theme.focus)
        .child(name)
        .child(figure(
            usage.map_or_else(|| NONE.to_owned(), |(cpu, _)| processes::format_cpu(cpu)),
            theme::TASK_MANAGER_METRIC_WIDTH,
        ))
        .child(figure(
            usage.map_or_else(
                || NONE.to_owned(),
                |(_, memory)| processes::format_memory(memory),
            ),
            theme::TASK_MANAGER_METRIC_WIDTH,
        ))
        .children(columns.pid.then(|| {
            figure(
                pid.map(|pid| pid.to_string()).unwrap_or_default(),
                theme::TASK_MANAGER_PID_WIDTH,
            )
        }))
}

fn name_and_note(name: &str, note: &str, strong: bool, theme: &Theme) -> El {
    row()
        .grow(1.0)
        .min_w(0.0)
        .gap(space::S2)
        .center_y()
        .child(
            text(name.to_owned())
                .font(if strong {
                    theme::PAGE_ROW_TITLE
                } else {
                    type_::BODY
                })
                .color(theme.text)
                .single_line()
                .truncate()
                .shrink(),
        )
        .children((!note.is_empty()).then(|| {
            text(note.to_owned())
                .font(type_::BODY)
                .color(theme.text_muted)
                .single_line()
                .truncate()
                .shrink()
        }))
}

fn process_icon(process: &Process) -> &'static rune::Icon {
    match process.kind.as_str() {
        "Browser" | "UI" | "Hub" => &icons::SYSTEM,
        "WebContent" => &icons::PAGE,
        _ => &icons::ACTIVITY,
    }
}

fn glyph(glyph: &'static rune::Icon, theme: &Theme) -> El {
    icon(glyph, theme::ICON).color(theme.icon).a11y_hidden()
}

/// The favicon of the tab with this ID, if it is open in this window.
fn favicon(state: &AppState, view: u64) -> Option<El> {
    let tab = state
        .browser
        .tabs()
        .iter()
        .find(|tab| tab.id() == TabId::from_raw(view))?;
    let pixels = tab.favicon.as_ref()?;
    Some(
        el().size(theme::ICON, theme::ICON)
            .pixels(pixels)
            .a11y_hidden(),
    )
}

fn search(state: &AppState, theme: &Theme) -> El {
    let page = &state.task_manager;
    let field = text_input(&page.input, type_::BODY)
        .key(TASK_MANAGER_SEARCH)
        .single_line()
        .grow(1.0)
        .min_w(0.0)
        .color(theme.text)
        .caret(theme.text, theme.selection)
        .no_focus_ring()
        .placeholder("Search processes", theme.text_muted)
        .label("Search processes")
        .on_edit(|state: &mut AppState| &mut state.task_manager.input)
        .on_input_change(AppState::task_manager_search_changed)
        .on_input_key(|_: &mut AppState, event, tx| {
            if event.key == Key::Escape {
                tx.focus(crate::ui::VIEWPORT, false);
                true
            } else {
                false
            }
        });
    let clear = (!page.input.text.is_empty()).then(|| {
        controls::icon_button(
            "task-manager-search-clear",
            "Clear search",
            &icons::CLOSE,
            theme::CLOSE_BUTTON,
            theme,
        )
        .focusable(true)
        .on_press_tx(AppState::clear_task_manager_search)
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
