// SPDX-License-Identifier: GPL-3.0-only

//! One menu row: its icon slot, label, tick, hover and press.

use rune::tok::type_;
use rune::{Color, El, Icon, Tx, el, icon, row, text};

use super::Apply;
use super::highlight;
use super::surface::item_transition;
use crate::app::AppState;
use crate::ui::components::animated_icon::{self, AnimatedIcon};
use crate::ui::theme::{self, Theme};

pub(crate) struct MenuItem {
    label: String,
    icon: Option<&'static Icon>,
    animated: Option<&'static AnimatedIcon>,
    lead: Option<El>,
    checked: Option<bool>,
    disabled: bool,
    press: Option<Apply>,
}

impl MenuItem {
    pub(crate) fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            animated: None,
            lead: None,
            checked: None,
            disabled: false,
            press: None,
        }
    }

    /// A static icon before the label.
    pub(crate) fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// An icon that plays its motion while the row is hovered.
    pub(crate) fn animated(mut self, icon: &'static AnimatedIcon) -> Self {
        self.animated = Some(icon);
        self
    }

    /// Anything before the label, such as a search engine's mark.
    pub(crate) fn lead(mut self, lead: El) -> Self {
        self.lead = Some(lead);
        self
    }

    /// A checkmark row: shows a tick when `on`.
    pub(crate) fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn on_select<S: 'static>(mut self, select: impl Fn(&mut S) + 'static) -> Self {
        self.press = Some(Box::new(move |row: El| row.on_press(select)));
        self
    }

    pub(crate) fn on_select_tx<S: 'static>(
        mut self,
        select: impl Fn(&mut S, &mut Tx) + 'static,
    ) -> Self {
        self.press = Some(Box::new(move |row: El| row.on_press_tx(select)));
        self
    }

    pub(super) fn has_icon(&self) -> bool {
        self.icon.is_some() || self.animated.is_some() || self.lead.is_some()
    }

    pub(super) fn build(
        self,
        key: &str,
        index: usize,
        delay: f32,
        state: &AppState,
        theme: &Theme,
    ) -> El {
        let reduced = state.settings.reduce_motion;
        let owner = format!("{key}-item{index}");
        let disabled = self.disabled;
        // While the panel draws its sliding highlight, rows leave their own fill transparent.
        let highlight = if highlight::sliding_row(state, key).is_some() {
            Color::TRANSPARENT
        } else {
            theme.hover
        };
        let mut entry = row()
            .key(owner.clone())
            .role(if self.checked.is_some() {
                "menuitemcheckbox"
            } else {
                "menuitem"
            })
            .label(self.label.clone())
            .h(theme::MENU_ROW_HEIGHT)
            .px(theme::MENU_ROW_PADDING)
            .gap(theme::MENU_ICON_GAP)
            .center_y()
            .radius(theme::MENU_ITEM_RADIUS)
            .continuous_corners()
            .hover(highlight)
            .focus_highlight(highlight)
            .focusable(!disabled)
            .focus_selected(self.checked == Some(true))
            .disabled(disabled)
            .enter_delay(if reduced { 0.0 } else { delay })
            .transition(item_transition(reduced));
        if disabled {
            entry = entry.opacity(theme.disabled_opacity);
        }
        let icon_hover = self
            .animated
            .map(|_| animated_icon::hover_change(owner.clone()));
        let row = owner.clone();
        entry = entry.on_hover_change(move |state: &mut AppState, inside, tx| {
            highlight::hover_row(state, &row, inside, tx);
            if let Some(icon_hover) = &icon_hover {
                icon_hover(state, inside, tx);
            }
        });

        let ink = if disabled {
            theme.text_muted
        } else {
            theme.text
        };
        if self.has_icon() {
            let glyph = match (self.lead, self.animated, self.icon) {
                (Some(lead), _, _) => Some(lead),
                (None, Some(animated), _) => Some(animated_icon::glyph(
                    state,
                    animated,
                    &owner,
                    &format!("{key}-{index}-glyph"),
                    theme::MENU_ICON,
                    theme.icon,
                    reduced,
                )),
                (None, None, Some(icon_glyph)) => {
                    Some(icon(icon_glyph, theme::MENU_ICON).color(theme.icon))
                }
                (None, None, None) => None,
            };
            entry = entry.child(
                el().key(format!("{key}-{index}-slot"))
                    .size(theme::MENU_ICON, theme::MENU_ICON)
                    .center()
                    .a11y_hidden()
                    .children(glyph),
            );
        }
        entry = entry.child(
            text(self.label)
                .font(type_::BODY)
                .color(ink)
                .single_line()
                .a11y_hidden(),
        );
        if let Some(on) = self.checked {
            // A growing label measures as no width, so the panel could not widen to it; a spacer
            // pushes the tick to the trailing edge instead.
            entry = entry.child(el().grow(1.0));
            entry = entry.child(
                el().key(format!("{key}-{index}-tick"))
                    .size(theme::MENU_ICON, theme::MENU_ICON)
                    .center()
                    .a11y_hidden()
                    .child(
                        text("✓")
                            .font(type_::BODY)
                            .color(theme.text)
                            .opacity(if on { 1.0 } else { 0.0 }),
                    ),
            );
        }
        match self.press {
            Some(press) if !disabled => press(entry),
            _ => entry,
        }
    }
}
