// SPDX-License-Identifier: GPL-3.0-only

//! An inactive window rests after a while and stays there until the user returns to it.

use std::time::{Duration, Instant};

use rune::Tx;

use super::AppState;

const INACTIVE_IDLE_AFTER: Duration = Duration::from_secs(5 * 60);
const RESUME_DELAY: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct WindowIdle {
    active: Option<bool>,
    phase: Phase,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Awake,
    Waiting {
        until: Instant,
    },
    Idling,
    Waking {
        until: Instant,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Transition {
    #[default]
    None,
    EnteredIdle,
    Resumed,
}

impl WindowIdle {
    fn observe_activity(&mut self, active: bool, now: Instant) -> bool {
        if self.active == Some(active) {
            return false;
        }
        self.active = Some(active);

        match (active, &self.phase) {
            (false, Phase::Awake) => {
                self.phase = Phase::Waiting {
                    until: now + INACTIVE_IDLE_AFTER,
                };
                true
            }
            (true, Phase::Waiting { .. }) => {
                self.phase = Phase::Awake;
                true
            }
            (true, Phase::Idling) => self.begin_wake(now),
            (false, Phase::Waking { .. }) => {
                self.phase = Phase::Idling;
                true
            }
            _ => false,
        }
    }

    fn advance(&mut self, now: Instant) -> Transition {
        match self.phase {
            Phase::Waiting { until } if now >= until => {
                self.phase = Phase::Idling;
                Transition::EnteredIdle
            }
            Phase::Waking { until } if now >= until => {
                self.phase = Phase::Awake;
                Transition::Resumed
            }
            _ => Transition::None,
        }
    }

    fn request_wake(&mut self, now: Instant) -> bool {
        if !matches!(self.phase, Phase::Idling) {
            return false;
        }
        self.begin_wake(now)
    }

    fn begin_wake(&mut self, now: Instant) -> bool {
        self.phase = Phase::Waking {
            until: now + RESUME_DELAY,
        };
        true
    }

    pub(super) fn holds_browser(&self) -> bool {
        matches!(self.phase, Phase::Idling | Phase::Waking { .. })
            || (matches!(self.phase, Phase::Waiting { .. }) && self.active == Some(false))
    }

    pub(super) fn showing(&self) -> bool {
        matches!(self.phase, Phase::Idling | Phase::Waking { .. })
    }

    pub(super) fn waking(&self) -> bool {
        matches!(self.phase, Phase::Waking { .. })
    }
}

impl AppState {
    pub(crate) fn idling(&self) -> bool {
        self.idle.showing()
    }

    pub(crate) fn idling_waking(&self) -> bool {
        self.idle.waking()
    }

    /// Pauses pages as soon as the window is off-screen or unfocused, then enters the idle view
    /// after a grace period. Returning to an idling window starts a delayed resume.
    pub(crate) fn poll_idle(&mut self, tx: &mut Tx) -> bool {
        if self.is_mini() {
            return false;
        }

        let now = Instant::now();
        let window_focused = self.browser.window_focused();
        #[cfg(target_os = "macos")]
        let application_active =
            !window_focused && crate::platform::macos::window::application_active();
        #[cfg(not(target_os = "macos"))]
        let application_active = false;
        let active = browser_window_active(tx.window_visible(), window_focused, application_active);
        let activity_changed = self.idle.observe_activity(active, now);
        if activity_changed && !active {
            tx.wake_after(INACTIVE_IDLE_AFTER);
            self.browser.suspend_window();
        } else if activity_changed && active && self.idle.waking() {
            tx.wake_after(RESUME_DELAY);
        } else if activity_changed && active && !self.idle.showing() {
            self.browser.resume_window();
        }

        let transition = self.idle.advance(now);
        if transition == Transition::EnteredIdle {
            return true;
        }
        if transition == Transition::Resumed {
            self.browser.resume_window();
            tx.focus_when_shown(crate::ui::viewport::page_focus_key(&self.browser), false);
        }
        if self.idle.holds_browser() {
            return activity_changed;
        }
        transition == Transition::Resumed || activity_changed
    }

    /// The idle screen takes the click, then gives the browser a moment to resume.
    pub(crate) fn request_idle_wake(&mut self, tx: &mut Tx) {
        if self.idle.request_wake(Instant::now()) {
            tx.wake_after(RESUME_DELAY);
        }
    }
}

fn browser_window_active(visible: bool, focused: bool, application_active: bool) -> bool {
    visible && (focused || (cfg!(target_os = "macos") && application_active))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inactive_window_pauses_then_enters_idle_after_the_grace_period() {
        let now = Instant::now();
        let mut idle = WindowIdle::default();

        assert!(idle.observe_activity(false, now));
        assert!(idle.holds_browser());
        assert_eq!(
            idle.advance(now + INACTIVE_IDLE_AFTER - Duration::from_millis(1)),
            Transition::None
        );
        assert_eq!(
            idle.advance(now + INACTIVE_IDLE_AFTER),
            Transition::EnteredIdle
        );
        assert!(idle.holds_browser());
    }

    #[test]
    fn returning_before_idle_cancels_the_transition() {
        let now = Instant::now();
        let mut idle = WindowIdle::default();

        idle.observe_activity(false, now);
        assert!(idle.observe_activity(true, now + Duration::from_secs(30)));
        assert_eq!(idle.advance(now + INACTIVE_IDLE_AFTER), Transition::None);
        assert!(!idle.holds_browser());
    }

    #[test]
    fn returning_to_an_idling_window_starts_a_delayed_wake() {
        let now = Instant::now();
        let mut idle = WindowIdle::default();

        idle.observe_activity(false, now);
        idle.advance(now + INACTIVE_IDLE_AFTER);
        idle.observe_activity(true, now + INACTIVE_IDLE_AFTER);
        assert!(idle.holds_browser());
        assert!(idle.waking());
        assert_eq!(
            idle.advance(now + INACTIVE_IDLE_AFTER + RESUME_DELAY - Duration::from_millis(1)),
            Transition::None
        );
        assert_eq!(
            idle.advance(now + INACTIVE_IDLE_AFTER + RESUME_DELAY),
            Transition::Resumed
        );
        assert!(!idle.holds_browser());
    }

    #[test]
    fn clicking_shows_a_waking_state_before_resuming() {
        let now = Instant::now();
        let mut idle = WindowIdle::default();

        idle.observe_activity(false, now);
        idle.advance(now + INACTIVE_IDLE_AFTER);
        assert!(idle.request_wake(now + INACTIVE_IDLE_AFTER));
        assert!(idle.holds_browser());
        assert!(idle.waking());
        assert_eq!(
            idle.advance(now + INACTIVE_IDLE_AFTER + RESUME_DELAY),
            Transition::Resumed
        );
        assert!(!idle.holds_browser());
    }

    #[test]
    fn native_ui_keeps_pages_visible_while_the_mac_app_is_active() {
        assert_eq!(
            browser_window_active(true, false, true),
            cfg!(target_os = "macos")
        );
        assert!(!browser_window_active(false, false, true));
        assert!(!browser_window_active(true, false, false));
    }
}
