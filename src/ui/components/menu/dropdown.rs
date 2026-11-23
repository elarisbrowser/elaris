// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::type_;
use rune::{El, icon, row, text};

use super::{Menu, MenuItem};
use crate::app::AppState;
use crate::ui::theme::{self, Theme};

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
                icon(&crate::ui::icons::CHEVRON_DOWN, theme::ICON)
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
