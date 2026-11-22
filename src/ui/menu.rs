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
use rune::{Spring, SpringPair, Vec2};
use rune_kit::popover;

use super::animated_icon::{self, AnimatedIcon};
use super::theme::{self, MENU_SCALE_FROM, Theme};
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
        self.dismiss = Some(dismissal(&self.key, close));
        self
    }

    /// Adds the menu to `layer` when open and returns the trigger in its place.
    pub(crate) fn build(mut self, state: &AppState, theme: &Theme, layer: &mut Vec<El>) -> El {
        let entries = std::mem::take(&mut self.entries);
        let key = self.key.clone();
        let min_width = self.min_width;
        self.anchored().attach(layer, |origin| {
            panel(entries, &key, min_width, state, theme, origin)
        })
    }

    fn anchored(&mut self) -> Anchored {
        Anchored {
            key: self.key.clone(),
            trigger: self.trigger.take(),
            open: self.open,
            at: self.at,
            side: self.side,
            align: self.align,
            dismiss: self.dismiss.take(),
        }
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
        let owner = format!("{key}-item{index}");
        let disabled = self.disabled;
        // With a sliding highlight drawn by the menu, rows leave their own fill transparent.
        let highlight = if state.context_menu_highlight.is_some() {
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
        if self.animated.is_some() {
            entry = entry.on_hover_change(animated_icon::hover_change(owner.clone()));
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
fn panel(
    entries: Vec<Entry>,
    key: &str,
    min_width: f32,
    state: &AppState,
    theme: &Theme,
    origin: Vec2,
) -> El {
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
    surface(
        key,
        "menu",
        min_width,
        reveal(reduced, origin),
        theme,
        popover::fit_scroll(col().key("rows").items_stretch().children(rows)),
    )
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

/// What a menu or popover shares with its trigger: where it sits and how it is dismissed.
struct Anchored {
    key: String,
    trigger: Option<El>,
    open: bool,
    at: Option<(f32, f32)>,
    side: Side,
    align: Align,
    dismiss: Option<(El, Apply)>,
}

impl Anchored {
    /// Returns the trigger in place, and adds the scrim and `content` to `layer` when open.
    fn attach(mut self, layer: &mut Vec<El>, content: impl FnOnce(Vec2) -> El) -> El {
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

        let mut panel = content(origin(self.side, self.align));
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

/// A scrim that closes the menu on a press outside it, and the Escape handler for its panel.
fn dismissal<S: 'static>(key: &str, close: impl Fn(&mut S) + Clone + 'static) -> (El, Apply) {
    let scrim = el()
        .key(format!("{key}-scrim"))
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
    (scrim, escape)
}

/// The rounded container every menu, popover and dropdown panel draws.
fn surface(
    key: &str,
    role: &'static str,
    min_width: f32,
    reveal: Transition,
    theme: &Theme,
    content: El,
) -> El {
    col()
        .key(format!("{key}-menu"))
        .role(role)
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
        .transition(reveal)
        .child(content)
}

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
                theme,
                content,
            )
        })
    }
}

/// A dropdown: a field showing the current choice, which opens a menu of every choice with the
/// current one ticked.
pub(crate) struct Dropdown<T> {
    key: String,
    label: String,
    width: f32,
    current: Option<T>,
    options: Vec<(T, &'static str)>,
    open: bool,
    toggle: Option<fn(&mut AppState)>,
    close: Option<fn(&mut AppState)>,
    choose: Option<fn(&mut AppState, T)>,
}

impl<T: Copy + PartialEq + 'static> Dropdown<T> {
    pub(crate) fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            width: theme::PAGE_CONTROL_WIDTH,
            current: None,
            options: Vec::new(),
            open: false,
            toggle: None,
            close: None,
            choose: None,
        }
    }

    pub(crate) fn options(mut self, options: impl IntoIterator<Item = (T, &'static str)>) -> Self {
        self.options = options.into_iter().collect();
        self
    }

    pub(crate) fn current(mut self, current: T) -> Self {
        self.current = Some(current);
        self
    }

    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub(crate) fn on_toggle(mut self, toggle: fn(&mut AppState)) -> Self {
        self.toggle = Some(toggle);
        self
    }

    pub(crate) fn on_close(mut self, close: fn(&mut AppState)) -> Self {
        self.close = Some(close);
        self
    }

    pub(crate) fn on_choose(mut self, choose: fn(&mut AppState, T)) -> Self {
        self.choose = Some(choose);
        self
    }

    pub(crate) fn build(self, state: &AppState, theme: &Theme, layer: &mut Vec<El>) -> El {
        let shown = self
            .options
            .iter()
            .find(|(value, _)| Some(*value) == self.current)
            .map_or("", |(_, label)| label);
        let field = row()
            .key(format!("{}-field", self.key))
            .w(self.width)
            .h(theme::FIELD_HEIGHT)
            .px(theme::FIELD_PADDING)
            .gap(theme::MENU_ICON_GAP)
            .center_y()
            .radius(theme::FIELD_RADIUS)
            .continuous_corners()
            .border(1.0, theme.control_border)
            .hover(theme.hover)
            .press(theme.press)
            .focusable(true)
            .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
            .role("combobox")
            .label(self.label.clone())
            .child(
                text(shown)
                    .font(type_::BODY)
                    .color(theme.text)
                    .grow(1.0)
                    .min_w(0.0)
                    .single_line()
                    .truncate()
                    .a11y_hidden(),
            )
            .child(
                icon(&super::icons::CHEVRON_DOWN, theme::ICON)
                    .color(theme.icon)
                    .a11y_hidden(),
            );
        let field = match self.toggle {
            Some(toggle) => field.on_press(toggle),
            None => field,
        };

        let mut menu = Menu::new(self.key.clone())
            .trigger(field)
            .open(self.open)
            .min_width(self.width);
        if let Some(close) = self.close {
            menu = menu.on_close(close);
        }
        for (value, label) in self.options {
            let mut item = MenuItem::new(label).checked(Some(value) == self.current);
            if let Some(choose) = self.choose {
                item = item.on_select(move |state: &mut AppState| choose(state, value));
            }
            menu = menu.item(item);
        }
        menu.build(state, theme, layer)
    }
}

/// Escape closes the topmost open menu, popover or dropdown. A menu that opened on a press rarely
/// holds keyboard focus, so the shortcut hook asks here before the page sees the key.
pub(crate) fn escape(state: &mut AppState) -> bool {
    if state.context_menu().is_some() {
        state.dismiss_context_menu();
    } else if state.page_menu.open() {
        state.page_menu = Default::default();
    } else if state.navigation_menu.is_some() {
        state.navigation_menu = None;
    } else if state.zoom_menu.is_some() {
        state.zoom_menu = None;
    } else if state.sidebar.overflow_menu_open {
        state.sidebar.overflow_menu_open = false;
    } else if state.omnibox.search_engine_menu_open {
        state.omnibox.search_engine_menu_open = false;
    } else if state.downloads_open {
        state.downloads_open = false;
    } else if state.startup_picker_open {
        state.startup_picker_open = false;
    } else if state.settings_picker_open {
        state.settings_picker_open = false;
    } else {
        return super::form_picker::cancel(state);
    }
    true
}

/// Where a panel grows from: the edge that touches its trigger, and the side of that edge it
/// aligns to. Matches Rune's overlay origin so a menu opens from its trigger.
fn origin(side: Side, align: Align) -> Vec2 {
    let along = match align {
        Align::Start => 0.0,
        Align::Center => 0.5,
        Align::End => 1.0,
    };
    match side {
        Side::Top => Vec2::new(along, 1.0),
        Side::Bottom => Vec2::new(along, 0.0),
        Side::Left => Vec2::new(1.0, along),
        Side::Right => Vec2::new(0.0, along),
    }
}

/// A panel scales in from 0.97 at its trigger edge while it fades in, and fades out in place.
/// The rows follow it, so the two move the same way.
fn reveal(reduced: bool, origin: Vec2) -> Transition {
    if reduced {
        return Transition::none();
    }
    Transition::asymmetric(
        Transition::scale_anchor(MENU_SCALE_FROM, origin),
        Transition::opacity(),
    )
    .springs(SpringPair::new(
        Spring::new(0.2, 0.0),
        Spring::new(0.15, 0.0),
    ))
}
