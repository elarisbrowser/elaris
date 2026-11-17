// SPDX-License-Identifier: GPL-3.0-only

//! Which of Ladybird's frames the engine shows. A ready-to-paint event can repeat a frame already
//! shown, and during a scale change Ladybird can still present frames laid out at the old one;
//! copying either would be wasted work, or show the page at the wrong size.

/// Frames laid out at a different display scale than the viewport's are stale.
const SCALE_TOLERANCE: f64 = 1e-3;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// The frame on show already.
    Duplicate,
    /// Laid out at a display scale the viewport no longer has.
    Stale,
    /// New, after `skipped` frames Ladybird presented that were replaced before the engine took
    /// them.
    Show { skipped: u64 },
}

#[derive(Default)]
pub(super) struct FrameGate {
    /// The display scale the view was last laid out at.
    scale: Option<f64>,
    /// The serial of the frame on show.
    serial: Option<u64>,
}

// Only the engine thread shows frames, which a build without the bridge lacks.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
impl FrameGate {
    pub(super) fn laid_out_at(&mut self, scale: f64) {
        self.scale = Some(scale);
    }

    /// Whether to show the frame with `serial`, laid out at `scale`.
    pub(super) fn check(&self, serial: u64, scale: f64) -> Verdict {
        if self.serial == Some(serial) {
            Verdict::Duplicate
        } else if self
            .scale
            .is_some_and(|wanted| (scale - wanted).abs() > SCALE_TOLERANCE)
        {
            Verdict::Stale
        } else {
            Verdict::Show {
                skipped: self.serial.map_or(0, |s| serial.saturating_sub(s + 1)),
            }
        }
    }

    /// Records the frame as on show.
    pub(super) fn shown(&mut self, serial: u64) {
        self.serial = Some(serial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_new_frames_and_counts_those_it_missed() {
        let mut gate = FrameGate::default();
        assert_eq!(gate.check(1, 2.0), Verdict::Show { skipped: 0 });
        gate.shown(1);
        assert_eq!(gate.check(1, 2.0), Verdict::Duplicate);
        assert_eq!(gate.check(4, 2.0), Verdict::Show { skipped: 2 });
    }

    #[test]
    fn drops_frames_at_an_old_scale() {
        let mut gate = FrameGate::default();
        gate.laid_out_at(2.0);
        assert_eq!(gate.check(1, 1.0), Verdict::Stale);
        assert_eq!(gate.check(1, 2.0 + 1e-6), Verdict::Show { skipped: 0 });
    }

    #[test]
    fn records_the_newest_frame() {
        let mut gate = FrameGate::default();
        gate.shown(1);
        gate.shown(2);
        assert_eq!(gate.check(2, 1.0), Verdict::Duplicate);
        assert_eq!(gate.check(3, 1.0), Verdict::Show { skipped: 0 });
    }
}
