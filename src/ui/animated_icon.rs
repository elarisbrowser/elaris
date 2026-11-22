// SPDX-License-Identifier: GPL-3.0-only

//! Icons that play a short animation while hovered. An `AnimatedIcon` pairs a static rest glyph
//! with a motion glyph whose SMIL `<animate>` elements Rune plays on mount. While the pointer is
//! over the control, the motion glyph replaces the rest glyph under its own key, so each hover
//! mounts it afresh and it plays from the start. Off the hover, the rest glyph returns in the
//! same pose the motion glyph starts and ends in.
//!
//! Motion glyphs are hand-written SMIL, ported from the motion of lucide-animated (MIT). Add a
//! motion glyph with a `static` `Icon` whose body animates only `currentColor` shapes.

use rune::{Color, El, Icon, Tx, icon};

use crate::app::AppState;

/// A glyph that plays `motion` while hovered and shows `rest` otherwise. `name` identifies it in
/// `AppState::animated_icon_hover`, so it must be unique among the animated icons on screen.
pub(crate) struct AnimatedIcon {
    name: &'static str,
    rest: &'static Icon,
    motion: &'static Icon,
}

impl AnimatedIcon {
    pub(crate) const fn new(
        name: &'static str,
        rest: &'static Icon,
        motion: &'static Icon,
    ) -> Self {
        Self { name, rest, motion }
    }
}

/// Records the hovered animated icon on each pointer change. Leaving clears only the icon that
/// was hovered, so moving between two animated icons never drops the second one.
pub(crate) fn hover_change(
    animated: &'static AnimatedIcon,
) -> impl Fn(&mut AppState, bool, &mut Tx) + 'static {
    move |state, inside, tx| {
        if set_hovered(&mut state.animated_icon_hover, animated.name, inside) {
            tx.animated();
        }
    }
}

/// Updates `slot` for a pointer change on `name`, returning whether it changed.
fn set_hovered(slot: &mut Option<&'static str>, name: &'static str, inside: bool) -> bool {
    let next = if inside {
        Some(name)
    } else if *slot == Some(name) {
        None
    } else {
        return false;
    };
    let changed = *slot != next;
    *slot = next;
    changed
}

/// `icon` at `size`: its motion glyph while hovered (and not `reduced`), its rest glyph
/// otherwise. `key` names the element and is suffixed per variant, so the two keep separate
/// nodes and the motion glyph replays on each hover.
pub(crate) fn glyph(
    state: &AppState,
    animated: &'static AnimatedIcon,
    key: &str,
    size: f32,
    color: Color,
    reduced: bool,
) -> El {
    let hovered = !reduced && state.animated_icon_hover == Some(animated.name);
    let (glyph, variant) = if hovered {
        (animated.motion, "motion")
    } else {
        (animated.rest, "rest")
    };
    icon(glyph, size)
        .key(format!("{key}-{variant}"))
        .color(color)
}

/// The reload arrow turns once around its centre, and settles at the rest pose.
static RELOAD_MOTION: Icon = Icon {
    name: "reload-motion",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="0 8 8;360 8 8" keyTimes="0;1" calcMode="spline" keySplines="0.4 0 0.2 1" dur="0.6s"/><path d="M12.5 8A4.5 4.5 0 1 1 11.2 4.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M12.5 2.5V5.5H9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></g>"#,
};

pub(crate) static RELOAD: AnimatedIcon =
    AnimatedIcon::new("reload", &super::icons::RELOAD, &RELOAD_MOTION);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_tracks_one_icon_and_ignores_leaves_of_others() {
        let mut slot = None;
        assert!(set_hovered(&mut slot, "reload", true));
        assert_eq!(slot, Some("reload"));
        // Entering the same icon again changes nothing.
        assert!(!set_hovered(&mut slot, "reload", true));
        // Leaving a different icon keeps the hovered one.
        assert!(!set_hovered(&mut slot, "other", false));
        assert_eq!(slot, Some("reload"));
        assert!(set_hovered(&mut slot, "reload", false));
        assert_eq!(slot, None);
    }

    #[test]
    fn motion_glyphs_animate_under_the_text_system() {
        use rune::text::{IconKind, TextSystem};

        // Rune classes an icon as animated only if its SMIL parses, so a typo in a motion body
        // would otherwise fail silently to a static icon.
        let mut text = TextSystem::new(&[], ("Inter", "Inter"));
        assert_eq!(text.icon_kind(&RELOAD_MOTION), IconKind::AnimatedMono);
    }
}
