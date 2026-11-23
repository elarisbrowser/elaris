// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::type_;
use rune::{Align, El, Side, col, row, text};

use super::Apply;
use super::surface::{Anchored, dismissal, item_transition, reveal, surface};
use crate::app::AppState;
use crate::ui::theme::{self, Theme};

/// A popover: a panel beside its trigger with a title and a body. The title and body rise in
/// one after the other, as menu rows do.
pub(crate) struct Popover {
    key: String,
    trigger: Option<El>,
    title: Option<String>,
    body: Option<El>,
    open: bool,
    side: Side,
    align: Align,
    dismiss: Option<(El, Apply)>,
}

impl Popover {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            trigger: None,
            title: None,
            body: None,
            open: false,
            side: Side::Bottom,
            align: Align::Start,
            dismiss: None,
        }
    }

    pub(crate) fn trigger(mut self, trigger: El) -> Self {
        self.trigger = Some(trigger);
        self
    }

    pub(crate) fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub(crate) fn child(mut self, body: El) -> Self {
        self.body = Some(body);
        self
    }

    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub(crate) fn placement(mut self, side: Side, align: Align) -> Self {
        self.side = side;
        self.align = align;
        self
    }

    pub(crate) fn on_close<S: 'static>(mut self, close: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.dismiss = Some(dismissal(&self.key, close));
        self
    }

    pub(crate) fn build(mut self, state: &AppState, theme: &Theme, layer: &mut Vec<El>) -> El {
        let reduced = state.settings.reduce_motion;
        let stagger = theme::MENU_ROW_STAGGER;
        let title = self.title.take();
        let body = self.body.take();
        let key = self.key.clone();
        let anchored = Anchored {
            key: self.key.clone(),
            trigger: self.trigger.take(),
            open: self.open,
            at: None,
            side: self.side,
            align: self.align,
            dismiss: self.dismiss.take(),
        };
        anchored.attach(layer, |origin| {
            let mut content = col()
                .key("content")
                .items_stretch()
                .gap(theme::MENU_SEPARATOR_GAP);
            if let Some(title) = title {
                content = content.child(
                    row()
                        .key("title")
                        .h(theme::MENU_ROW_HEIGHT)
                        .px(theme::MENU_ROW_PADDING)
                        .center_y()
                        .transition(item_transition(reduced))
                        .child(
                            text(title)
                                .font(type_::BODY)
                                .color(theme.text)
                                .wrap()
                                .role("heading"),
                        ),
                );
            }
            if let Some(body) = body {
                content = content.child(
                    col()
                        .key("body")
                        .px(theme::MENU_ROW_PADDING)
                        .enter_delay(if reduced { 0.0 } else { stagger })
                        .transition(item_transition(reduced))
                        .child(body),
                );
            }
            surface(
                &key,
                "dialog",
                theme::MENU_MIN_WIDTH,
                reveal(reduced, origin),
                theme.overlay_panel,
                theme,
                content,
            )
        })
    }
}
