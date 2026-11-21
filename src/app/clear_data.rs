// SPDX-License-Identifier: GPL-3.0-only

//! Clearing browsing history and engine-held site data, cache and download records.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use elaris::ladybird::BrowsingData;

use crate::storage::HistoryMutation;

/// How far back Clear Browsing Data reaches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ClearRange {
    LastHour,
    LastDay,
    LastWeek,
    #[default]
    AllTime,
}

impl ClearRange {
    pub(crate) const ALL: [Self; 4] =
        [Self::LastHour, Self::LastDay, Self::LastWeek, Self::AllTime];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::LastHour => "Last hour",
            Self::LastDay => "Last 24 hours",
            Self::LastWeek => "Last 7 days",
            Self::AllTime => "All time",
        }
    }

    /// The earliest time to clear from, or `None` for everything.
    fn since(self, now: SystemTime) -> Option<SystemTime> {
        const HOUR: u64 = 60 * 60;
        let seconds = match self {
            Self::LastHour => HOUR,
            Self::LastDay => 24 * HOUR,
            Self::LastWeek => 7 * 24 * HOUR,
            Self::AllTime => return None,
        };
        now.checked_sub(Duration::from_secs(seconds))
    }
}

/// The open Clear Browsing Data dialog and what it will remove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClearDataDialog {
    pub(crate) range: ClearRange,
    pub(crate) history: bool,
    pub(crate) engine: BrowsingData,
}

impl Default for ClearDataDialog {
    fn default() -> Self {
        Self {
            range: ClearRange::default(),
            history: true,
            engine: BrowsingData {
                cache: true,
                site_data: true,
                download_history: false,
            },
        }
    }
}

impl ClearDataDialog {
    pub(crate) fn has_selection(&self) -> bool {
        self.history || self.engine.cache || self.engine.site_data || self.engine.download_history
    }
}

impl super::AppState {
    pub(crate) fn open_clear_data(&mut self) {
        self.clear_data = Some(ClearDataDialog::default());
        self.clear_data_status = None;
    }

    pub(crate) fn update_clear_data(&mut self, update: impl FnOnce(&mut ClearDataDialog)) {
        if let Some(dialog) = &mut self.clear_data {
            update(dialog);
        }
    }

    /// Clears what the dialog selected and closes it.
    pub(crate) fn confirm_clear_data(&mut self) {
        let Some(dialog) = self.clear_data.take() else {
            return;
        };
        if !dialog.has_selection() {
            return;
        }
        let since = dialog.range.since(SystemTime::now());
        let mut failed = false;
        if dialog.history {
            let mutation = since.map_or(HistoryMutation::Clear, |since| {
                HistoryMutation::ClearSince(unix_micros(since))
            });
            failed = self
                .storage
                .as_ref()
                .is_some_and(|storage| storage.history().mutate(self.window_id, mutation).is_err());
        }
        self.browser.clear_browsing_data(since, dialog.engine);
        self.clear_data_status = Some(if failed {
            "Could not clear browsing history.".into()
        } else {
            "Browsing data cleared.".into()
        });
    }
}

fn unix_micros(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_reach_back_from_now_and_all_time_has_no_start() {
        let now = UNIX_EPOCH + Duration::from_secs(30 * 24 * 60 * 60);
        assert_eq!(
            ClearRange::LastHour.since(now),
            Some(now - Duration::from_secs(3600))
        );
        assert_eq!(
            ClearRange::LastWeek.since(now),
            Some(now - Duration::from_secs(7 * 24 * 3600))
        );
        assert_eq!(ClearRange::AllTime.since(now), None);
    }

    #[test]
    fn the_dialog_needs_something_selected() {
        let mut dialog = ClearDataDialog::default();
        assert!(dialog.has_selection());
        dialog.history = false;
        dialog.engine = BrowsingData::default();
        assert!(!dialog.has_selection());
    }
}
