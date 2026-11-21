// SPDX-License-Identifier: GPL-3.0-only

//! Two-finger horizontal trackpad swipes over a page that go back or forward in its history.
//!
//! A touchpad scroll goes to the page first. Ladybird reports for each wheel event whether the page
//! cancelled it. Once the page has been sent part of a gesture and cancelled it, the gesture stays a
//! scroll. Otherwise a sideways gesture becomes a swipe the page no longer sees, once history allows
//! that way, whether or not the page would have scrolled sideways. Releasing a swipe past the commit
//! distance navigates.

use std::collections::VecDeque;

use rune::{GesturePhase, WheelEvent};

use crate::tab::TabId;

/// Travel in points before an unused scroll may become a swipe.
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
    sent: Sent,
}

/// Wheel events sent to one tab and how many Ladybird has finished, which it does in order, so a
/// result can be matched to the gesture it belongs to.
#[derive(Default)]
struct Sent {
    tab: Option<TabId>,
    sent: u64,
    finished: u64,
}

struct Gesture {
    tab: TabId,
    /// The count of events sent to the tab before this gesture's first.
    first: u64,
    /// Travel in the fingers' direction, positive rightward.
    dx: f32,
    dy: f32,
    /// The travel up to each event sent to the page and not yet finished, oldest first.
    unanswered: VecDeque<[f32; 2]>,
    lock: Lock,
    /// The fingers lifted; the gesture is only kept until the page answers its last event.
    ended: bool,
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
    /// `natural` when content follows the fingers, as macOS's natural scrolling has it; swipes
    /// always follow the fingers.
    pub(crate) fn wheel(&mut self, tab: TabId, wheel: &WheelEvent, natural: bool) -> Step {
        if !wheel.precise {
            self.cancel();
            return self.send_to_page(tab);
        }
        if wheel.momentum {
            return if self.coasting {
                Step::Swallowed { armed_now: false }
            } else {
                self.send_to_page(tab)
            };
        }
        self.coasting = false;
        if wheel.phase == GesturePhase::Began {
            self.track(tab);
            self.gesture = Some(Gesture {
                tab,
                first: self.sent.sent,
                dx: 0.0,
                dy: 0.0,
                unanswered: VecDeque::new(),
                lock: Lock::Undecided,
                ended: false,
            });
        }
        let Some(gesture) = self.gesture.as_mut().filter(|gesture| gesture.tab == tab) else {
            return self.send_to_page(tab);
        };
        let ended = wheel.phase == GesturePhase::Ended;
        let Lock::Swipe(direction) = gesture.lock else {
            gesture.dx += if natural { wheel.dx } else { -wheel.dx };
            gesture.dy += wheel.dy;
            gesture.unanswered.push_back([gesture.dx, gesture.dy]);
            gesture.ended = ended;
            return self.send_to_page(tab);
        };
        let before = gesture.progress(direction) >= 1.0;
        gesture.dx += if natural { wheel.dx } else { -wheel.dx };
        let armed = gesture.progress(direction) >= 1.0;
        if ended {
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

    /// Ladybird finished a wheel event for `tab`. `cancelled` says the page cancelled it; `can_go`
    /// says whether history has a page that way. Returns whether a swipe began.
    pub(crate) fn page_result(
        &mut self,
        tab: TabId,
        cancelled: bool,
        can_go: impl Fn(Direction) -> bool,
    ) -> bool {
        let swiped = self.judge(tab, cancelled, can_go);
        // An ended gesture can no longer change once it is a scroll or has no answers left.
        if self.gesture.as_ref().is_some_and(|gesture| {
            gesture.ended && (gesture.lock != Lock::Undecided || gesture.unanswered.is_empty())
        }) {
            self.gesture = None;
        }
        swiped
    }

    fn judge(&mut self, tab: TabId, cancelled: bool, can_go: impl Fn(Direction) -> bool) -> bool {
        if self.sent.tab != Some(tab) {
            return false;
        }
        let index = self.sent.finished;
        self.sent.finished += 1;
        let Some(gesture) = self
            .gesture
            .as_mut()
            .filter(|gesture| gesture.tab == tab && gesture.lock == Lock::Undecided)
            .filter(|gesture| index >= gesture.first)
        else {
            return false;
        };
        // Only travel the page has answered for counts: a later event may still scroll something.
        let Some([dx, dy]) = gesture.unanswered.pop_front() else {
            return false;
        };
        if cancelled {
            gesture.lock = Lock::Scroll;
            return false;
        }
        if dx.hypot(dy) < LOCK_DISTANCE {
            return false;
        }
        let direction = if dx > 0.0 {
            Direction::Back
        } else {
            Direction::Forward
        };
        // A swipe decided after the fingers lifted would have nothing left to release, so it stays a scroll.
        if gesture.ended || dx.abs() < dy.abs() * LOCK_RATIO || !can_go(direction) {
            gesture.lock = Lock::Scroll;
            return false;
        }
        // The swipe starts from the travel since, so its indicator grows from nothing.
        gesture.dx -= dx;
        gesture.lock = Lock::Swipe(direction);
        true
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

    fn track(&mut self, tab: TabId) {
        if self.sent.tab != Some(tab) {
            self.sent = Sent {
                tab: Some(tab),
                ..Sent::default()
            };
        }
    }

    fn send_to_page(&mut self, tab: TabId) -> Step {
        self.track(tab);
        self.sent.sent += 1;
        Step::Page
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

    fn send(swipes: &mut Swipes, dx: f32, dy: f32, phase: GesturePhase) -> Step {
        swipes.wheel(tab(), &event(dx, dy, phase), true)
    }

    /// Starts a gesture whose first events the page reports with `used`.
    fn start(swipes: &mut Swipes, dx: f32, used: bool) -> bool {
        send(swipes, 0.0, 0.0, GesturePhase::Began);
        send(swipes, dx, 0.0, GesturePhase::Moved);
        let began = swipes.page_result(tab(), used, |_| true);
        swipes.page_result(tab(), used, |_| true) || began
    }

    #[test]
    fn unused_sideways_scroll_becomes_a_swipe_that_goes_back() {
        let mut swipes = Swipes::default();
        assert!(start(&mut swipes, 20.0, false));
        let armed = (0..12)
            .map(|_| send(&mut swipes, 12.0, 0.0, GesturePhase::Moved))
            .filter(|step| *step == Step::Swallowed { armed_now: true })
            .count();
        assert_eq!(armed, 1);
        assert!(swipes.view().is_some_and(|view| view.armed));
        assert_eq!(
            send(&mut swipes, 0.0, 0.0, GesturePhase::Ended),
            Step::Navigate(tab(), Direction::Back)
        );
        let mut momentum = event(8.0, 0.0, GesturePhase::Moved);
        momentum.momentum = true;
        assert_eq!(
            swipes.wheel(tab(), &momentum, true),
            Step::Swallowed { armed_now: false }
        );
    }

    #[test]
    fn a_scroll_the_page_used_stays_with_the_page() {
        let mut swipes = Swipes::default();
        assert!(!start(&mut swipes, 20.0, true));
        swipes.page_result(tab(), false, |_| true);
        assert_eq!(
            send(&mut swipes, 200.0, 0.0, GesturePhase::Moved),
            Step::Page
        );
        assert!(swipes.view().is_none());
    }

    #[test]
    fn results_of_an_earlier_gesture_do_not_start_a_swipe() {
        let mut swipes = Swipes::default();
        send(&mut swipes, 0.0, 0.0, GesturePhase::Began);
        send(&mut swipes, 30.0, 0.0, GesturePhase::Moved);
        send(&mut swipes, 0.0, 0.0, GesturePhase::Ended);
        send(&mut swipes, 0.0, 0.0, GesturePhase::Began);
        send(&mut swipes, 30.0, 0.0, GesturePhase::Moved);
        for _ in 0..4 {
            assert!(!swipes.page_result(tab(), false, |_| true));
        }
        assert!(swipes.page_result(tab(), false, |_| true));
    }

    #[test]
    fn a_gesture_that_lifts_before_its_answers_never_swipes() {
        let mut swipes = Swipes::default();
        send(&mut swipes, 0.0, 0.0, GesturePhase::Began);
        send(&mut swipes, 30.0, 0.0, GesturePhase::Moved);
        send(&mut swipes, 0.0, 0.0, GesturePhase::Ended);
        assert!(!swipes.page_result(tab(), false, |_| true));
        assert!(!swipes.page_result(tab(), false, |_| true));
        assert!(swipes.view().is_none());
        assert!(!swipes.page_result(tab(), false, |_| true));
    }

    #[test]
    fn vertical_scroll_and_missing_history_never_swipe() {
        let mut swipes = Swipes::default();
        send(&mut swipes, 0.0, 0.0, GesturePhase::Began);
        send(&mut swipes, 4.0, 20.0, GesturePhase::Moved);
        assert!(!swipes.page_result(tab(), false, |_| true));
        assert!(!swipes.page_result(tab(), false, |_| true));

        send(&mut swipes, 0.0, 0.0, GesturePhase::Began);
        send(&mut swipes, 40.0, 0.0, GesturePhase::Moved);
        assert!(!swipes.page_result(tab(), false, |_| false));
        assert!(!swipes.page_result(tab(), false, |_| false));
        assert!(swipes.view().is_none());
    }

    #[test]
    fn swipes_follow_the_fingers_without_natural_scrolling() {
        let mut swipes = Swipes::default();
        swipes.wheel(tab(), &event(0.0, 0.0, GesturePhase::Began), false);
        swipes.wheel(tab(), &event(-30.0, 0.0, GesturePhase::Moved), false);
        assert!(!swipes.page_result(tab(), false, |_| true));
        assert!(swipes.page_result(tab(), false, |_| true));
        assert_eq!(
            swipes.view().map(|view| view.direction),
            Some(Direction::Back)
        );
    }
}
