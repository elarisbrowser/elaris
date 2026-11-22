// SPDX-License-Identifier: GPL-3.0-only

//! Menus, context menus and dropdowns. The panel is a rounded container with its rows inset
//! inside it, as the Settings category list is, and its highlights are concentric with its
//! corners. Opening fades the panel in and staggers its rows; an icon may play its animated glyph
//! while its row is hovered.
//!
//! The builder mirrors the subset of Rune Kit's menu the browser uses, so call sites only change
//! their import. Rune Kit draws its own surface and rows, which these must replace to animate.
//!
//! | Module | Holds |
//! |---|---|
//! | `item` | One row: icon slot, label, tick, hover and press |
//! | `surface` | The panel, its entrance, anchoring beside a trigger and dismissal |
//! | `highlight` | The sliding highlight inside the panel and what moves it |
//! | `popover`, `dropdown` | The two other surfaces built on the same panel |

mod dropdown;
mod highlight;
mod item;
mod popover;
mod surface;

use rune::tok::type_;
use rune::{Align, El, PointerPress, Side, Stagger, Vec2, col, el, row, text};
use rune_kit::popover as kit_popover;

use super::theme::{self, Theme};
use crate::app::AppState;

pub(crate) use dropdown::Dropdown;
pub(crate) use highlight::poll as poll_highlight;
pub(crate) use item::MenuItem;
pub(crate) use popover::Popover;
use surface::{Anchored, dismissal, reveal, surface};

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
    /// The band above the rows, and what leads the rows inside the body card below it.
    header: Option<(El, El)>,
}

enum Entry {
    Item(Box<MenuItem>),
    Separator,
    Label(String),
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
            header: None,
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

    /// Puts `band` above the rows on the panel itself, and the rows in a raised card led by
    /// `lead`, as the page menu's title and navigation buttons are.
    pub(crate) fn header(mut self, band: El, lead: El) -> Self {
        self.header = Some((band, lead));
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
        let header = self.header.take();
        self.anchored().attach(layer, |origin| {
            highlight::track(panel(
                entries, header, &key, min_width, state, theme, origin,
            ))
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

/// The rows' panel: a rounded container whose content sits inset inside it.
fn panel(
    entries: Vec<Entry>,
    header: Option<(El, El)>,
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
    let highlighted = highlight::sliding_row(state, key);
    let mut rows = Vec::with_capacity(entries.len());
    let mut item_index = 0;
    // Rows have fixed heights, so the highlight's place comes from the entries themselves rather
    // than from last frame's layout.
    let mut top = 0.0;
    let mut highlight_top = None;
    for (index, entry) in entries.into_iter().enumerate() {
        let height = match entry {
            Entry::Separator => 1.0 + 2.0 * theme::MENU_SEPARATOR_GAP,
            Entry::Item(_) | Entry::Label(_) => theme::MENU_ROW_HEIGHT,
        };
        if highlighted.as_deref() == Some(format!("{key}-item{index}").as_str()) {
            highlight_top = Some(top);
        }
        top += height;
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
    let rows = kit_popover::fit_scroll(
        col()
            .key("rows")
            .items_stretch()
            .children(highlight_top.map(|top| highlight::view(key, top, reduced, theme)))
            .children(rows),
    );
    let (fill, content) = match header {
        None => (theme.overlay_panel, rows),
        Some((band, lead)) => (
            theme.overlay_header,
            col().items_stretch().shrink().min_h(0.0).child(band).child(
                col()
                    .key("body")
                    .items_stretch()
                    .shrink()
                    .min_h(0.0)
                    .padding(theme::MENU_INSET)
                    .radius(theme::MENU_ITEM_RADIUS)
                    .continuous_corners()
                    .fill(theme.overlay_panel)
                    .child(lead)
                    .child(rows),
            ),
        ),
    };
    surface(
        key,
        "menu",
        min_width,
        reveal(reduced, origin),
        fill,
        theme,
        content,
    )
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
