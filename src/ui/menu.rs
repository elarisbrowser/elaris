// SPDX-License-Identifier: GPL-3.0-only

//! Menus, context menus and dropdowns. The panel is a rounded container with its rows inset
//! inside it, as the Settings category list is, and its highlights are concentric with its
//! corners. Opening fades the panel in and staggers its rows; an icon may play its animated glyph
//! while its row is hovered.
//!
//! The builder mirrors the subset of Rune Kit's menu the browser uses, so call sites only change
//! their import. Rune Kit draws its own surface and rows, which these must replace to animate.

use rune::tok::type_;
use rune::{
    Align, Color, El, Icon, Key, KeyEvent, PointerPress, Side, Stagger, Transition, Tx, col, el,
    icon, row, text,
};
use rune_kit::popover;

use super::animated_icon::{self, AnimatedIcon};
use super::theme::{self, Theme};
use crate::app::AppState;

/// Applies a stored handler to a row or panel. Keeps the state type out of [`Menu`] and
/// [`MenuItem`], which take closures over any state.
type Apply = Box<dyn FnOnce(El) -> El>;

pub(crate) struct Menu {
    key: String,
    trigger: Option<El>,
    entries: Vec<Entry>,
    open: bool,
    /// The window point a context menu opens at. Otherwise the menu opens beside its trigger.
    at: Option<(f32, f32)>,
    side: Side,
    align: Align,
    min_width: f32,
    /// The scrim behind an open menu, and the Escape handler for its panel.
    dismiss: Option<(El, Apply)>,
}

enum Entry {
    Item(Box<MenuItem>),
    Separator,
    Label(String),
}

pub(crate) struct MenuItem {
    label: String,
    icon: Option<&'static Icon>,
    animated: Option<&'static AnimatedIcon>,
    lead: Option<El>,
    checked: Option<bool>,
    disabled: bool,
    press: Option<Apply>,
}

/// A menu at a window point, such as where a secondary press landed. Clear the point in
/// `on_close` and in item handlers.
pub(crate) fn context_menu(
    key: impl Into<String>,
    trigger: El,
    point: Option<PointerPress>,
) -> Menu {
    let menu = Menu::new(key).trigger(trigger).open(point.is_some());
    match point {
        Some(point) => menu.at(point.x, point.y),
        None => menu,
    }
}

impl Menu {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            trigger: None,
            entries: Vec::new(),
            open: false,
            at: None,
            side: Side::Bottom,
            align: Align::Start,
            min_width: theme::MENU_MIN_WIDTH,
            dismiss: None,
        }
    }

    pub(crate) fn trigger(mut self, trigger: El) -> Self {
        self.trigger = Some(trigger);
        self
    }

    pub(crate) fn item(mut self, item: MenuItem) -> Self {
        self.entries.push(Entry::Item(Box::new(item)));
        self
    }

    pub(crate) fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }

    pub(crate) fn group_label(mut self, label: impl Into<String>) -> Self {
        self.entries.push(Entry::Label(label.into()));
        self
    }

    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Which side of the trigger the menu opens on, and how it aligns along that side. It flips
    /// when it doesn't fit.
    pub(crate) fn placement(mut self, side: Side, align: Align) -> Self {
        self.side = side;
        self.align = align;
        self
    }

    pub(crate) fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub(crate) fn at(mut self, x: f32, y: f32) -> Self {
        self.at = Some((x, y));
        self
    }

    /// Pressing outside the menu, or pressing Escape while it has focus, calls `close`.
    pub(crate) fn on_close<S: 'static>(mut self, close: impl Fn(&mut S) + Clone + 'static) -> Self {
        let scrim = el()
            .key(format!("{}-scrim", self.key))
            .role("none")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .on_press(close.clone());
        let escape: Apply = Box::new(move |panel: El| {
            panel.on_key(move |state: &mut S, event: &KeyEvent, _: &mut Tx| {
                if event.key == Key::Escape {
                    close(state);
                    true
                } else {
                    false
                }
            })
        });
        self.dismiss = Some((scrim, escape));
        self
    }

    /// Adds the menu to `layer` when open and returns the trigger in its place.
    pub(crate) fn build(mut self, state: &AppState, theme: &Theme, layer: &mut Vec<El>) -> El {
        let anchor_key = format!("{}-anchor", self.key);
        let pointer = self.at.is_some();
        let trigger = self
            .trigger
            .take()
            .map_or_else(el, |trigger| trigger.expanded(self.open && !pointer));
        let anchor = el().key(anchor_key.clone()).child(trigger);
        if !self.open {
            return anchor;
        }

        let (anchor_at, gap) = match self.at {
            Some((x, y)) => {
                let point = format!("{}-point", self.key);
                layer.push(el().key(point.clone()).absolute(x, y).size(0.0, 0.0));
                (point, 0.0)
            }
            None => (anchor_key.clone(), theme::MENU_GAP),
        };
        let (scrim, escape) = match self.dismiss.take() {
            Some((scrim, escape)) => (Some(scrim), Some(escape)),
            None => (None, None),
        };
        layer.extend(scrim);

        let mut panel = panel(
            std::mem::take(&mut self.entries),
            &self.key,
            self.min_width,
            state,
            theme,
        );
        if let Some(escape) = escape {
            panel = escape(panel);
        }
        layer.push(
            panel
                .anchored_fit(anchor_at, self.side, self.align, gap)
                .focus_return(anchor_key),
        );
        anchor
    }
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

    fn has_icon(&self) -> bool {
        self.icon.is_some() || self.animated.is_some() || self.lead.is_some()
    }

    fn build(
        self,
        key: &str,
        index: usize,
        delay: f32,
        slot: bool,
        state: &AppState,
        theme: &Theme,
    ) -> El {
        let reduced = state.settings.reduce_motion;
        let disabled = self.disabled;
        // With a sliding highlight drawn by the menu, rows leave their own fill transparent.
        let highlight = if state.context_menu_highlight.is_some() {
            Color::TRANSPARENT
        } else {
            theme.hover
        };
        let mut entry = row()
            .key(format!("{key}-item{index}"))
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
        if let Some(animated) = self.animated {
            entry = entry.on_hover_change(animated_icon::hover_change(animated));
        }

        let ink = if disabled {
            theme.text_muted
        } else {
            theme.text
        };
        if slot {
            let glyph = match (self.lead, self.animated, self.icon) {
                (Some(lead), _, _) => Some(lead),
                (None, Some(animated), _) => Some(animated_icon::glyph(
                    state,
                    animated,
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
                .grow(1.0)
                .min_w(0.0)
                .single_line()
                .truncate()
                .a11y_hidden(),
        );
        if let Some(on) = self.checked {
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

/// The rows' panel: a rounded container whose content sits inset inside it.
fn panel(entries: Vec<Entry>, key: &str, min_width: f32, state: &AppState, theme: &Theme) -> El {
    let reduced = state.settings.reduce_motion;
    let items = entries
        .iter()
        .filter(|entry| matches!(entry, Entry::Item(_)))
        .count();
    let slot = entries
        .iter()
        .any(|entry| matches!(entry, Entry::Item(item) if item.has_icon()));
    let stagger = Stagger::new(theme::MENU_ROW_STAGGER);
    let mut rows = Vec::with_capacity(entries.len());
    let mut item_index = 0;
    for (index, entry) in entries.into_iter().enumerate() {
        rows.push(match entry {
            Entry::Item(item) => {
                let delay = stagger.delay(item_index, items);
                item_index += 1;
                item.build(key, index, delay, slot, state, theme)
            }
            Entry::Separator => el()
                .key(format!("{key}-sep{index}"))
                .h(1.0)
                .mt(theme::MENU_SEPARATOR_GAP)
                .mb(theme::MENU_SEPARATOR_GAP)
                .ml(theme::MENU_ROW_PADDING)
                .mr(theme::MENU_ROW_PADDING)
                .fill(theme.separator)
                .a11y_hidden(),
            Entry::Label(label) => row()
                .key(format!("{key}-label{index}"))
                .h(theme::MENU_ROW_HEIGHT)
                .px(theme::MENU_ROW_PADDING)
                .center_y()
                .child(
                    text(label)
                        .font(type_::BODY)
                        .color(theme.text_muted)
                        .a11y_hidden(),
                ),
        });
    }
    col()
        .key(format!("{key}-menu"))
        .role("menu")
        .focus_group()
        .min_w(min_width)
        .items_stretch()
        .padding(theme::MENU_INSET)
        .radius(theme::MENU_RADIUS)
        .continuous_corners()
        .fill(theme.overlay_panel)
        .border(1.0, theme.overlay_border)
        .shadow(
            theme::MENU_SHADOW_Y,
            theme::MENU_SHADOW_BLUR,
            theme.overlay_shadow,
        )
        .transition(theme::overlay_reveal(reduced))
        .child(popover::fit_scroll(
            col().key("rows").items_stretch().children(rows),
        ))
}

/// Rows rise and fade in after the panel, one after another; they keep their place under reduced
/// motion, where the stagger is dropped.
fn item_transition(reduced: bool) -> Transition {
    if reduced {
        Transition::none()
    } else {
        Transition::offset(0.0, theme::MENU_ROW_OFFSET).combined(Transition::opacity())
    }
}
