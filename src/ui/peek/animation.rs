// SPDX-License-Identifier: GPL-3.0-only

//! Opening, dismissing, and promoting Peek on Rune's frame clock.

use std::{cell::Cell, time::Duration};

use rune::{Spring, Tx};

use crate::app::AppState;
use crate::browser::{
    peek::{self, Destination},
    split,
};
use crate::tab::TabId;

use super::{page_status, theme};
use crate::ui::viewport;

const INITIAL_SCALE: f32 = 0.035;
const OPEN_MOTION: Spring = Spring::new(0.40, 0.18);
const OPEN_SETTLE: f32 = 0.46;
const DISMISS_TIME: f64 = 0.20;
const PROMOTE_TIME: f64 = 0.24;

#[derive(Default)]
pub(crate) struct Presentation {
    reduced_motion: Cell<bool>,
    opening: Option<Opening>,
    pub(super) animation: Option<PeekAnimation>,
}

struct Opening {
    started_at: f64,
    elapsed: f32,
}

impl Presentation {
    pub(super) fn note_reduced_motion(&self, reduced: bool) {
        self.reduced_motion.set(reduced);
    }

    pub(crate) fn open(&mut self, clock: f64) {
        self.opening = Some(Opening {
            started_at: clock,
            elapsed: 0.0,
        });
        self.animation = None;
    }

    pub(super) fn opening_time(&self) -> f32 {
        self.opening
            .as_ref()
            .map_or(OPEN_SETTLE, |opening| opening.elapsed)
    }
}

pub(super) struct PeekAnimation {
    pub(super) id: TabId,
    from: [f32; 4],
    started_at: Option<f64>,
    pub(super) progress: f32,
    pub(super) kind: AnimationKind,
}

pub(super) enum AnimationKind {
    Dismiss {
        frame: rune::Pixels,
        status: Option<String>,
        target: [f32; 4],
    },
    Promote(Destination),
}

impl PeekAnimation {
    fn new(id: TabId, from: [f32; 4], kind: AnimationKind) -> Self {
        Self {
            id,
            from,
            started_at: None,
            progress: 0.0,
            kind,
        }
    }

    pub(super) fn rect(&self, state: &AppState) -> [f32; 4] {
        let [_, _, width, height] = state.split_rect;
        let (target, t) = match self.kind {
            AnimationKind::Dismiss { target, .. } => (target, self.progress.powi(2)),
            AnimationKind::Promote(destination) => {
                let viewport = [0.0, theme::VIEWPORT_INSET, width, height];
                let bounds = match destination {
                    Destination::Tab => viewport,
                    Destination::Split => {
                        if state.browser.visible_tabs().len() == 2 {
                            let [left, right] = split::widths(
                                width,
                                state.browser.split.map_or(0.5, |split| split.ratio),
                            );
                            [left + split::DIVIDER, viewport[1], right, height]
                        } else {
                            split::grid_cell_rect(
                                viewport,
                                state.browser.visible_tabs().len(),
                                1,
                                state.browser.split.map_or(0.5, |split| split.ratio),
                            )
                        }
                    }
                };
                (bounds, 1.0 - (1.0 - self.progress).powi(3))
            }
        };
        std::array::from_fn(|i| self.from[i] + (target[i] - self.from[i]) * t)
    }
}

pub(super) struct CardPose {
    pub(super) rect: [f32; 4],
    pub(super) scale: f32,
    pub(super) offset: [f32; 2],
}

impl CardPose {
    pub(super) fn drawn_rect(&self) -> [f32; 4] {
        let [x, y, w, h] = self.rect;
        [
            x + self.offset[0] + w * (1.0 - self.scale) / 2.0,
            y + self.offset[1] + h * (1.0 - self.scale) / 2.0,
            w * self.scale,
            h * self.scale,
        ]
    }
}

pub(super) fn opening_pose(
    [_, _, width, height]: [f32; 4],
    origin: [f32; 2],
    elapsed: f32,
    reduced: bool,
) -> CardPose {
    let panel = peek::geometry([width, height]);
    let y = theme::VIEWPORT_INSET + panel.y;
    let [dx, dy] = [
        origin[0] - (panel.x + panel.width / 2.0),
        origin[1] - (panel.y + panel.height / 2.0),
    ];
    let progress = if reduced || elapsed >= OPEN_SETTLE {
        1.0
    } else {
        OPEN_MOTION.sample(0.0, 0.0, 1.0, elapsed).0
    };
    let max_scale = (height + 2.0 * theme::VIEWPORT_INSET) / panel.height;
    let scale = (INITIAL_SCALE + (1.0 - INITIAL_SCALE) * progress).min(max_scale);
    let travel = 1.0 - progress.min(1.0);
    CardPose {
        rect: [panel.x, y, panel.width, panel.height],
        scale,
        offset: [dx * travel, dy * travel],
    }
}

/// Advance Peek on Rune's frame clock without resizing the Ladybird view during motion.
pub(crate) fn advance(state: &mut AppState, clock: f64, tx: &mut Tx) -> bool {
    let reduced = state.peek_presentation.reduced_motion.get();
    let presentation = &mut state.peek_presentation;
    if let Some(animation) = &mut presentation.animation {
        let start = *animation.started_at.get_or_insert(clock);
        let duration = match animation.kind {
            AnimationKind::Dismiss { .. } => DISMISS_TIME,
            AnimationKind::Promote(_) => PROMOTE_TIME,
        };
        animation.progress = ((clock - start) / duration).clamp(0.0, 1.0) as f32;
        if animation.progress < 1.0 && !reduced {
            tx.wake_after(Duration::ZERO);
        } else {
            let destination = match animation.kind {
                AnimationKind::Promote(destination)
                    if state
                        .browser
                        .peek
                        .as_ref()
                        .is_some_and(|peek| peek.page.id() == animation.id) =>
                {
                    Some(destination)
                }
                _ => None,
            };
            presentation.animation = None;
            if let Some(destination) = destination {
                state.promote_peek(destination, tx);
                // The destination viewport takes over at the card's final bounds.
                tx.instant();
            }
        }
        return true;
    }
    let Some(opening) = &mut presentation.opening else {
        return false;
    };
    opening.elapsed = (clock - opening.started_at).max(0.0) as f32;
    if reduced || opening.elapsed >= OPEN_SETTLE {
        presentation.opening = None;
    } else {
        tx.wake_after(Duration::ZERO);
    }
    true
}

pub(super) fn promote(state: &mut AppState, tx: &mut Tx, destination: Destination) {
    if state.peek_presentation.reduced_motion.get() {
        state.peek_presentation.opening = None;
        state.promote_peek(destination, tx);
    } else if let Some(peek) = &state.browser.peek {
        let presentation = &mut state.peek_presentation;
        let pose = opening_pose(
            state.split_rect,
            peek.origin,
            presentation.opening_time(),
            false,
        );
        presentation.opening = None;
        presentation.animation = Some(PeekAnimation::new(
            peek.page.id(),
            pose.drawn_rect(),
            AnimationKind::Promote(destination),
        ));
        tx.wake_after(Duration::ZERO);
        tx.animated();
    }
}

pub(crate) fn dismiss(state: &mut AppState, tx: &mut Tx) -> bool {
    let Some(peek) = &state.browser.peek else {
        return false;
    };
    let animation = (!state.peek_presentation.reduced_motion.get()).then(|| {
        let pose = opening_pose(
            state.split_rect,
            peek.origin,
            state.peek_presentation.opening_time(),
            false,
        );
        let from = state
            .peek_presentation
            .animation
            .as_ref()
            .map_or_else(|| pose.drawn_rect(), |animation| animation.rect(state));
        let [x, y] = peek.origin;
        let [_, _, width, height] = pose.rect;
        let [w, h] = [width * INITIAL_SCALE, height * INITIAL_SCALE];
        let target = [x - w / 2.0, y + theme::VIEWPORT_INSET - h / 2.0, w, h];
        PeekAnimation::new(
            peek.page.id(),
            from,
            AnimationKind::Dismiss {
                frame: peek.page.surface.pixels.clone(),
                status: page_status(&peek.page).map(str::to_owned),
                target,
            },
        )
    });
    state.browser.dismiss_peek();
    state.peek_presentation.opening = None;
    state.peek_presentation.animation = animation;
    tx.focus(viewport::focus_key(&state.browser), false);
    if state.peek_presentation.animation.is_some() {
        tx.wake_after(Duration::ZERO);
    }
    tx.animated();
    true
}
