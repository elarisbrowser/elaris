// SPDX-License-Identifier: GPL-3.0-only

//! The browser's native settings page: side navigation over one module per category.
//!
//! To add a category, write a module whose `sections` returns its sections and list it in
//! [`CATEGORIES`].

mod about;
mod appearance;
mod browsing;
mod keyboard;
mod memory;
mod privacy;
mod system;

use rune::El;

use crate::ui::icons;
use crate::ui::theme::Theme;

use super::layout::PageLayout;
use super::widgets::{self, NavItem};
use super::{PageAction, PageContext};

const SCROLL: &str = "settings-scroll";

pub(super) const ACTIONS: &[PageAction] = &[
    PageAction {
        label: "Edit config.toml",
        enabled: |_| true,
        run: |state, _| state.open_config_file(),
    },
    PageAction {
        label: "Reload config.toml",
        enabled: |_| true,
        run: |state, _| state.reload_configuration(),
    },
];

struct Category {
    nav: NavItem,
    sections: fn(&mut PageContext) -> Vec<El>,
}

static CATEGORIES: [Category; 6] = [
    Category {
        nav: NavItem {
            key: "settings-look",
            title: "Look & layout",
            short: "Look",
            icon: &icons::SIDEBAR,
        },
        sections: appearance::sections,
    },
    Category {
        nav: NavItem {
            key: "settings-browsing",
            title: "Browsing",
            short: "Browsing",
            icon: &icons::PAGE,
        },
        sections: browsing::sections,
    },
    Category {
        nav: NavItem {
            key: "settings-keyboard",
            title: "Keyboard shortcuts",
            short: "Keyboard",
            icon: &icons::KEYBOARD,
        },
        sections: keyboard::sections,
    },
    Category {
        nav: NavItem {
            key: "settings-privacy",
            title: "Privacy & security",
            short: "Privacy",
            icon: &icons::LOCK,
        },
        sections: privacy::sections,
    },
    Category {
        nav: NavItem {
            key: "settings-system",
            title: "Browser setup",
            short: "Setup",
            icon: &icons::SITE_CONTROLS,
        },
        sections: system::sections,
    },
    Category {
        nav: NavItem {
            key: "settings-about",
            title: "About Elaris",
            short: "About",
            icon: &icons::INFO,
        },
        sections: about::sections,
    },
];

pub(super) fn view(cx: &mut PageContext) -> PageLayout {
    let current = cx
        .state
        .page_category
        .and_then(|key| CATEGORIES.iter().find(|category| category.nav.key == key))
        .unwrap_or(&CATEGORIES[0]);
    let nav = widgets::nav(
        "Settings categories",
        SCROLL,
        &CATEGORIES.each_ref().map(|category| &category.nav),
        current.nav.key,
        cx,
    );
    let sections = (current.sections)(cx);
    PageLayout::new(
        SCROLL,
        &icons::SITE_CONTROLS,
        widgets::category(current.nav.key, current.nav.title, sections, cx.theme),
    )
    .nav(nav)
}

/// A settings section, keyed by its title so tests and focus can find it.
fn section(title: &str, theme: &Theme, rows: impl IntoIterator<Item = El>) -> El {
    widgets::section(
        format!("settings-section-{title}"),
        title,
        theme,
        widgets::rows(theme, rows),
    )
}
