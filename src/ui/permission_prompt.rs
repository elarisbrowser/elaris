// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::{space, type_};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant};

use crate::app::AppState;
use crate::engine::ViewCommand;
use crate::permissions::PermissionKind;
use crate::tab::{Tab, TabId};
use crate::ui::theme::{self, Theme};

/// Keeps page timing from steering browser UI: prompts ignore clicks until armed, and
/// entering fullscreen shows who took the screen and how to leave.
#[derive(Debug, Default)]
pub(crate) struct Guards {
    /// The visible prompt and the clock time it accepts answers.
    prompt: Option<(TabId, u64, f64)>,
    prompt_armed: bool,
    /// The fullscreen tab and the clock time its notice hides.
    fullscreen: Option<(TabId, f64)>,
}

impl Guards {
    fn armed(&self, tab: TabId, request_id: u64) -> bool {
        self.prompt_armed
            && self
                .prompt
                .is_some_and(|(id, request, _)| id == tab && request == request_id)
    }
}

fn prompt_tab(state: &AppState) -> Option<&Tab> {
    state
        .browser
        .peek
        .as_ref()
        .map(|peek| &peek.page)
        .or_else(|| state.browser.active_tab())
        .filter(|tab| shown_request(tab).is_some())
}

/// The request id of the prompt the tab shows: a permission question first, then an external link.
fn shown_request(tab: &Tab) -> Option<u64> {
    tab.permission_prompt
        .as_ref()
        .map(|prompt| prompt.request_id)
        .or_else(|| tab.external_url.as_ref().map(|request| request.request_id))
}

/// Advances the guards on Rune's frame clock; returns whether the interface must rebuild.
pub(crate) fn poll(state: &mut AppState, clock: f64) -> bool {
    let prompt = prompt_tab(state).and_then(|tab| Some((tab.id(), shown_request(tab)?)));
    let fullscreen = state.browser.fullscreen_tab().map(Tab::id);
    let guards = &mut state.page_guards;
    let mut changed = false;
    if prompt != guards.prompt.map(|(tab, request, _)| (tab, request)) {
        guards.prompt = prompt
            .map(|(tab, request)| (tab, request, clock + theme::PERMISSION_PROMPT_ARM_SECONDS));
        guards.prompt_armed = false;
        changed = true;
    } else if let Some((_, _, armed_at)) = guards.prompt
        && !guards.prompt_armed
        && clock >= armed_at
    {
        guards.prompt_armed = true;
        changed = true;
    }
    if fullscreen != guards.fullscreen.map(|(tab, _)| tab) {
        guards.fullscreen = fullscreen.map(|tab| (tab, clock + theme::FULLSCREEN_NOTICE_SECONDS));
        changed = true;
    } else if let Some((tab, until)) = guards.fullscreen
        && until.is_finite()
        && clock >= until
    {
        guards.fullscreen = Some((tab, f64::INFINITY));
        changed = true;
    }
    changed
}

pub(super) fn view(state: &AppState, theme: &Theme, origin: [f32; 2]) -> Option<El> {
    let tab = prompt_tab(state)?;
    let Some(prompt) = tab.permission_prompt.as_ref() else {
        return external_url(state, tab, theme, origin);
    };
    let kind = PermissionKind::from_key(&prompt.key)?;
    let id = tab.id();
    let request_id = prompt.request_id;
    let armed = state.page_guards.armed(id, request_id);
    let answer = move |state: &mut AppState, choice| {
        state
            .browser
            .send(id, ViewCommand::ReplyToPermission { request_id, choice });
    };
    let kit = theme.kit(theme.overlay_panel);
    let mut actions = row().w_full().justify_end().gap(space::S2);
    let title = if prompt.os_denied {
        actions = actions.child(
            Button::new("OK")
                .disabled(!armed)
                .on_press(move |state| answer(state, None))
                .build(&kit),
        );
        format!("System settings block {}", kind.label().to_lowercase())
    } else {
        actions = actions
            .child(
                Button::new("Block")
                    .disabled(!armed)
                    .on_press(move |state| answer(state, Some(false)))
                    .build(&kit),
            )
            .child(
                Button::new("Allow")
                    .variant(ButtonVariant::Submit)
                    .disabled(!armed)
                    .on_press(move |state| answer(state, Some(true)))
                    .build(&kit),
            );
        format!("Allow access to {}?", kind.label().to_lowercase())
    };
    let site = if prompt.requesting_origin == prompt.origin {
        prompt.origin.clone()
    } else {
        format!("{} embedded on {}", prompt.requesting_origin, prompt.origin)
    };
    Some(panel(
        "website-permission",
        "Website permission",
        title,
        site,
        actions,
        theme,
        origin,
    ))
}

fn external_url(state: &AppState, tab: &Tab, theme: &Theme, origin: [f32; 2]) -> Option<El> {
    let request = tab.external_url.as_ref()?;
    let id = tab.id();
    let request_id = request.request_id;
    let armed = state.page_guards.armed(id, request_id);
    let answer = move |state: &mut AppState, open| {
        state
            .browser
            .send(id, ViewCommand::ReplyToExternalUrl { request_id, open });
    };
    let kit = theme.kit(theme.overlay_panel);
    let mut actions = row().w_full().justify_end().gap(space::S2);
    let (title, detail) = if request.unavailable {
        actions = actions.child(
            Button::new("OK")
                .on_press(move |state| answer(state, false))
                .build(&kit),
        );
        ("Can't open this link".to_owned(), request.url.clone())
    } else {
        actions = actions
            .child(
                Button::new("Cancel")
                    .disabled(!armed)
                    .on_press(move |state| answer(state, false))
                    .build(&kit),
            )
            .child(
                Button::new("Open")
                    .variant(ButtonVariant::Submit)
                    .disabled(!armed)
                    .on_press(move |state| answer(state, true))
                    .build(&kit),
            );
        let site = if request.origin.is_empty() || request.origin == "null" {
            "This page".to_owned()
        } else {
            request.origin.clone()
        };
        (
            format!("Open {}?", request.application),
            format!("{site} wants to open {}", request.url),
        )
    };
    Some(panel(
        "external-link",
        "Open in another application",
        title,
        detail,
        actions,
        theme,
        origin,
    ))
}

fn panel(
    key: &'static str,
    label: &'static str,
    title: String,
    detail: String,
    actions: El,
    theme: &Theme,
    origin: [f32; 2],
) -> El {
    col()
        .key(key)
        .absolute(origin[0] + theme::FIND_INSET, origin[1] + theme::FIND_INSET)
        .max_w(theme::FIND_WIDTH)
        .px(space::S3)
        .py(space::S3)
        .gap(space::S2)
        .radius(theme::FIND_RADIUS)
        .fill(theme.overlay_panel)
        .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
        .role("group")
        .label(label)
        .child(text(title).font(type_::BODY).color(theme.text).wrap())
        .child(
            text(detail)
                .font(type_::LABEL)
                .color(theme.text_muted)
                .wrap(),
        )
        .child(actions)
}

pub(super) fn indicators(tab: &crate::tab::Tab, theme: &Theme) -> Option<El> {
    let mut indicators = row().gap(space::S2);
    let mut visible = false;
    for (kind, symbol, label) in [
        (
            PermissionKind::Microphone,
            &super::icons::MICROPHONE,
            "Microphone in use",
        ),
        (
            PermissionKind::Location,
            &super::icons::LOCATION,
            "Location in use",
        ),
    ] {
        if tab.permissions_in_use[kind.index()] {
            visible = true;
            indicators = indicators.child(
                row()
                    .center_y()
                    .gap(space::S1)
                    .label(label)
                    .child(rune::icon(symbol, theme::ICON).color(theme.text))
                    .child(text(label).font(type_::LABEL).color(theme.text)),
            );
        }
    }
    visible.then(|| {
        indicators
            .key("site-activity")
            .absolute(theme::FIND_INSET, theme::FIND_INSET)
            .px(space::S2)
            .py(space::S1)
            .radius(theme::FIND_RADIUS)
            .fill(theme.overlay_panel)
            .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
    })
}

/// Names the site that took the screen while its fullscreen notice is up.
pub(super) fn fullscreen_notice(state: &AppState, theme: &Theme) -> Option<El> {
    let tab = state.browser.fullscreen_tab()?;
    state
        .page_guards
        .fullscreen
        .filter(|&(id, until)| id == tab.id() && until.is_finite())?;
    let site = crate::permissions::PermissionStore::origin(&tab.committed_url)
        .unwrap_or_else(|| tab.committed_url.clone());
    Some(
        row()
            .key("fullscreen-notice")
            .absolute(theme::FIND_INSET, theme::FIND_INSET)
            .max_w(theme::FIND_WIDTH)
            .px(space::S3)
            .py(space::S2)
            .radius(theme::FIND_RADIUS)
            .fill(theme.overlay_panel)
            .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
            .role("status")
            .child(
                text(format!("{site} is now full screen. Press Esc to exit."))
                    .font(type_::LABEL)
                    .color(theme.text)
                    .wrap(),
            ),
    )
}
