//! Shortcut count and restore controls for the new tab page.

use gpui::App;
use photon_core::{SHORTCUT_COUNTS, SpaceId};

use super::super::super::ClickHandler;
use super::super::super::controls::Choice;
use super::super::super::settings::Settings;
use super::super::change;

/// The new tab page's current layout options.
pub(in super::super) struct LayoutOptions {
    pub(in super::super) show_shortcuts: bool,
    pub(in super::super) has_removed: bool,
}

impl LayoutOptions {
    pub(in super::super) fn read(space: SpaceId, cx: &App) -> Self {
        let settings = Settings::get(cx);
        Self {
            show_shortcuts: settings.is_feature_enabled(photon_core::Feature::NewTabShortcuts),
            has_removed: settings
                .spaces
                .get(space)
                .is_some_and(|space| space.new_tab.has_removed()),
        }
    }

    pub(in super::super) fn restore_removed(&self, space: SpaceId) -> ClickHandler {
        Box::new(move |_, _, cx| {
            if !Settings::get(cx)
                .spaces
                .get(space)
                .is_some_and(|space| space.new_tab.has_removed())
            {
                return;
            }
            Settings::update(cx, |settings| {
                if let Some(space) = settings.spaces.get_mut(space) {
                    space.new_tab.restore_removed();
                }
            });
            super::super::super::show_alert(cx, "Removed shortcuts restored");
        })
    }

    /// The numbers of shortcuts to choose from, the current one chosen.
    pub(in super::super) fn shortcut_counts(cx: &App) -> Vec<Choice> {
        let limit = Settings::get(cx).new_tab.shortcut_limit();
        SHORTCUT_COUNTS
            .into_iter()
            .map(|count| Choice {
                label: count.to_string().into(),
                chosen: count == limit,
                on_choose: change(move |settings| settings.new_tab.shortcut_count = count),
            })
            .collect()
    }
}
