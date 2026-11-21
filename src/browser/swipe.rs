// SPDX-License-Identifier: GPL-3.0-only

//! Two-finger horizontal trackpad swipes over a page that go back or forward in its history.
//!
//! A touchpad scroll starts undecided. Once it has travelled far enough it locks either to the
//! page as an ordinary scroll, or, when it is clearly sideways and history allows that way, to a
//! swipe the page never sees. Releasing a swipe past the commit distance navigates.

use rune::{GesturePhase, WheelEvent};

use crate::tab::TabId;

/// Travel in points before a scroll decides whether it is a swipe.
const LOCK_DISTANCE: f32 = 10.0;
/// How much more sideways than vertical a scroll must be to become a swipe.
const LOCK_RATIO: f32 = 2.0;
/// Travel in points that arms a swipe to navigate on release.
const COMMIT_DISTANCE: f32 = 140.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Back,
    Forward,
}

/// What the view draws for a swipe in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SwipeView {
    pub(crate) tab: TabId,
    pub(crate) direction: Direction,
    /// From 0 at the lock to 1 at the commit distance.
    pub(crate) progress: f32,
    /// Releasing now navigates.
    pub(crate) armed: bool,
}

/// What to do with one wheel event.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    /// An ordinary scroll for the page.
    Page,
    /// The swipe took it; `armed_now` when it just crossed the commit distance.
    Swallowed {
        armed_now: bool,
    },
    Navigate(TabId, Direction),
}

#[derive(Default)]
pub(crate) struct Swipes {
    gesture: Option<Gesture>,
    /// The momentum after a released swipe, which must not scroll the page.
    coasting: bool,
}

struct Gesture {
    tab: TabId,
    dx: f32,
    dy: f32,
    lock: Lock,
}

#[derive(Clone, Copy, PartialEq)]
enum Lock {
    Undecided,
    Scroll,
    Swipe(Direction),
}

impl Gesture {
    fn progress(&self, direction: Direction) -> f32 {
        let travel = match direction {
            Direction::Back => self.dx,
            Direction::Forward => -self.dx,
        };
        (travel / COMMIT_DISTANCE).clamp(0.0, 1.0)
    }
}

impl Swipes {
    /// `can_go` says whether the tab's history has a page in that direction.
    pub(crate) fn wheel(
        &mut self,
        tab: TabId,
        wheel: &WheelEvent,
        can_go: impl Fn(Direction) -> bool,
    ) -> Step {
        if !wheel.precise {
            self.cancel();
            return Step::Page;
        }
        if wheel.momentum {
            return if self.coasting {
                Step::Swallowed { armed_now: false }
            } else {
                Step::Page
            };
        }
        self.coasting = false;
        if wheel.phase == GesturePhase::Began {
            self.gesture = Some(Gesture {
                tab,
                dx: 0.0,
                dy: 0.0,
                lock: Lock::Undecided,
            });
        }
        let Some(gesture) = self.gesture.as_mut().filter(|gesture| gesture.tab == tab) else {
            return Step::Page;
        };
        let before = match gesture.lock {
            Lock::Swipe(direction) => gesture.progress(direction) >= 1.0,
            _ => false,
        };
        gesture.dx += wheel.dx;
        gesture.dy += wheel.dy;
        if gesture.lock == Lock::Undecided && gesture.dx.hypot(gesture.dy) >= LOCK_DISTANCE {
            let direction = if gesture.dx > 0.0 {
                Direction::Back
            } else {
                Direction::Forward
            };
            gesture.lock = if gesture.dx.abs() >= gesture.dy.abs() * LOCK_RATIO && can_go(direction)
            {
                // The swipe starts from here, so its indicator grows from nothing.
                gesture.dx = 0.0;
                Lock::Swipe(direction)
            } else {
                Lock::Scroll
            };
        }
        let Lock::Swipe(direction) = gesture.lock else {
            if wheel.phase == GesturePhase::Ended {
                self.gesture = None;
            }
            return Step::Page;
        };
        let armed = gesture.progress(direction) >= 1.0;
        if wheel.phase == GesturePhase::Ended {
            self.gesture = None;
            self.coasting = true;
            if armed {
                return Step::Navigate(tab, direction);
            }
        }
        Step::Swallowed {
            armed_now: armed && !before,
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.gesture = None;
        self.coasting = false;
    }

    pub(crate) fn view(&self) -> Option<SwipeView> {
        let gesture = self.gesture.as_ref()?;
        let Lock::Swipe(direction) = gesture.lock else {
            return None;
        };
        let progress = gesture.progress(direction);
        Some(SwipeView {
            tab: gesture.tab,
            direction,
            progress,
            armed: progress >= 1.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab() -> TabId {
        TabId::from_raw(1)
    }

    fn event(dx: f32, dy: f32, phase: GesturePhase) -> WheelEvent {
        let mut wheel = WheelEvent::new(dx, dy);
        wheel.precise = true;
        wheel.phase = phase;
        wheel
    }

    fn feed(swipes: &mut Swipes, events: &[(f32, f32, GesturePhase)]) -> Vec<Step> {
        events
            .iter()
            .map(|&(dx, dy, phase)| swipes.wheel(tab(), &event(dx, dy, phase), |_| true))
            .collect()
    }

    #[test]
    fn sideways_swipe_past_commit_goes_back_with_one_haptic() {
        let mut swipes = Swipes::default();
        let mut steps = feed(&mut swipes, &[(0.0, 0.0, GesturePhase::Began)]);
        steps.extend(feed(&mut swipes, &[(12.0, 0.0, GesturePhase::Moved); 14]));
        assert_eq!(steps[0], Step::Page);
        let armed = steps
            .iter()
            .filter(|step| **step == Step::Swallowed { armed_now: true })
            .count();
        assert_eq!(armed, 1);
        assert!(swipes.view().is_some_and(|view| view.armed));
        assert_eq!(
            feed(&mut swipes, &[(0.0, 0.0, GesturePhase::Ended)]),
            [Step::Navigate(tab(), Direction::Back)]
        );
        let mut momentum = event(8.0, 0.0, GesturePhase::Moved);
        momentum.momentum = true;
        assert_eq!(
            swipes.wheel(tab(), &momentum, |_| true),
            Step::Swallowed { armed_now: false }
        );
    }

    #[test]
    fn short_swipe_cancels_and_leftward_goes_forward() {
        let mut swipes = Swipes::default();
        feed(
            &mut swipes,
            &[
                (0.0, 0.0, GesturePhase::Began),
                (-30.0, 0.0, GesturePhase::Moved),
            ],
        );
        assert_eq!(
            swipes.view().map(|view| view.direction),
            Some(Direction::Forward)
        );
        assert_eq!(
            feed(&mut swipes, &[(0.0, 0.0, GesturePhase::Ended)]),
            [Step::Swallowed { armed_now: false }]
        );
        assert!(swipes.view().is_none());
    }

    #[test]
    fn vertical_scroll_and_missing_history_stay_with_the_page() {
        let mut swipes = Swipes::default();
        let steps = feed(
            &mut swipes,
            &[
                (0.0, 0.0, GesturePhase::Began),
                (4.0, 20.0, GesturePhase::Moved),
                (200.0, 0.0, GesturePhase::Moved),
            ],
        );
        assert!(steps.iter().all(|step| *step == Step::Page));

        let begin = event(0.0, 0.0, GesturePhase::Began);
        swipes.wheel(tab(), &begin, |_| false);
        let side = event(40.0, 0.0, GesturePhase::Moved);
        assert_eq!(swipes.wheel(tab(), &side, |_| false), Step::Page);
        assert!(swipes.view().is_none());
    }
}
