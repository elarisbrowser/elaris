// SPDX-License-Identifier: GPL-3.0-only

//! Icons that play a short animation while hovered. An `AnimatedIcon` pairs a static rest glyph
//! with a motion glyph whose SMIL `<animate>` elements Rune plays on mount. While the pointer is
//! over the control, the motion glyph replaces the rest glyph under its own key, so each hover
//! mounts it afresh and it plays from the start. Off the hover, the rest glyph returns in the
//! same pose the motion glyph starts and ends in.
//!
//! Motion glyphs are hand-written SMIL, ported from the motion of lucide-animated (MIT). Add a
//! motion glyph with a `static` `Icon` whose body animates only `currentColor` shapes.

use rune::{Color, El, Icon, Transition, Tx, icon};

use crate::app::AppState;

/// A glyph that plays `motion` while its owner is hovered and shows `rest` otherwise. The owner
/// is the element that carries the hover handler, so the same icon can appear twice on screen
/// (a toolbar button and a menu row) and each plays only for its own hover.
pub(crate) struct AnimatedIcon {
    rest: &'static Icon,
    motion: &'static Icon,
}

impl AnimatedIcon {
    pub(crate) const fn new(rest: &'static Icon, motion: &'static Icon) -> Self {
        Self { rest, motion }
    }
}

/// Records `owner` as hovered on each pointer change. Leaving clears only the owner that was
/// hovered, so moving between two animated icons never drops the second one.
pub(crate) fn hover_change(
    owner: impl Into<String>,
) -> impl Fn(&mut AppState, bool, &mut Tx) + 'static {
    let owner = owner.into();
    move |state, inside, tx| {
        if set_hovered(&mut state.animated_icon_hover, &owner, inside) {
            tx.animated();
        }
    }
}

/// Updates `slot` for a pointer change on `owner`, returning whether it changed.
fn set_hovered(slot: &mut Option<String>, owner: &str, inside: bool) -> bool {
    let hovered = slot.as_deref() == Some(owner);
    if inside {
        if hovered {
            return false;
        }
        *slot = Some(owner.to_owned());
        true
    } else if hovered {
        *slot = None;
        true
    } else {
        false
    }
}

/// `icon` at `size`: its motion glyph while `owner` is hovered (and not `reduced`), its rest glyph
/// otherwise. `key` names the element and is suffixed per variant, so the two keep separate
/// nodes and the motion glyph replays on each hover.
pub(crate) fn glyph(
    state: &AppState,
    animated: &'static AnimatedIcon,
    owner: &str,
    key: &str,
    size: f32,
    color: Color,
    reduced: bool,
) -> El {
    // A menu row also plays while it is the keyboard-focused row, which is how a menu opened under
    // a still pointer shows its first row before the pointer moves.
    let hovered = !reduced
        && (state.animated_icon_hover.as_deref() == Some(owner)
            || state.context_menu_highlight.as_deref() == Some(owner));
    let (glyph, variant) = if hovered {
        (animated.motion, "motion")
    } else {
        (animated.rest, "rest")
    };
    // Swapping glyphs remounts the element, so it must not fade in: that flickers on every hover.
    icon(glyph, size)
        .key(format!("{key}-{variant}"))
        .color(color)
        .transition(Transition::none())
}

/// The reload arrow turns once around its centre, and settles at the rest pose.
static RELOAD_MOTION: Icon = Icon {
    name: "reload-motion",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="0 8 8;360 8 8" keyTimes="0;1" calcMode="spline" keySplines="0.4 0 0.2 1" dur="0.6s"/><path d="M12.5 8A4.5 4.5 0 1 1 11.2 4.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M12.5 2.5V5.5H9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></g>"#,
};

pub(crate) static RELOAD: AnimatedIcon = AnimatedIcon::new(&super::icons::RELOAD, &RELOAD_MOTION);

// The glyphs below are lucide-animated's hover motions (MIT, pqoqubbw), ported to SMIL on the
// 24-unit lucide grid. Each rest glyph is the lucide icon's static shapes; each motion glyph
// repeats them with the motion applied and returns to the rest pose, so the swap is seamless.

static COPY_REST: Icon = Icon {
    name: "copy",
    view: 24.0,
    body: r#"<rect x="8" y="8" width="14" height="14" rx="2" ry="2" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

static COPY_MOTION: Icon = Icon {
    name: "copy-motion",
    view: 24.0,
    body: r#"<g><animateTransform attributeName="transform" type="translate" values="0 0;-3 -3;0 0" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.4s"/><rect x="8" y="8" width="14" height="14" rx="2" ry="2" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></g><g><animateTransform attributeName="transform" type="translate" values="0 0;3 3;0 0" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.4s"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></g>"#,
};

pub(crate) static COPY: AnimatedIcon = AnimatedIcon::new(&COPY_REST, &COPY_MOTION);

static PLUS_REST: Icon = Icon {
    name: "plus",
    view: 24.0,
    body: r#"<path d="M5 12h14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M12 5v14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

/// A plus is symmetrical, so a half turn ends in the rest pose.
static PLUS_MOTION: Icon = Icon {
    name: "plus-motion",
    view: 24.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="0 12 12;180 12 12" keyTimes="0;1" calcMode="spline" keySplines="0.4 0 0.2 1" dur="0.45s"/><path d="M5 12h14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M12 5v14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></g>"#,
};

pub(crate) static PLUS: AnimatedIcon = AnimatedIcon::new(&PLUS_REST, &PLUS_MOTION);

static HISTORY_REST: Icon = Icon {
    name: "history",
    view: 24.0,
    body: r#"<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M3 3v5h5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><line x1="12" y1="12" x2="12" y2="7" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><line x1="12" y1="12" x2="16" y2="14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>"#,
};

/// The hour hand turns once and the minute hand flicks back, as the clock is wound.
static HISTORY_MOTION: Icon = Icon {
    name: "history-motion",
    view: 24.0,
    body: r#"<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M3 3v5h5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><g><animateTransform attributeName="transform" type="rotate" values="0 12 12;-360 12 12" keyTimes="0;1" calcMode="spline" keySplines="0.4 0 0.2 1" dur="0.6s"/><line x1="12" y1="12" x2="12" y2="7" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></g><g><animateTransform attributeName="transform" type="rotate" values="0 12 12;-45 12 12;0 12 12" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.5s"/><line x1="12" y1="12" x2="16" y2="14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></g>"#,
};

pub(crate) static HISTORY: AnimatedIcon = AnimatedIcon::new(&HISTORY_REST, &HISTORY_MOTION);

/// The arrow's shaft shortens and its head nudges toward the direction it points away from.
static BACK_REST: Icon = Icon {
    name: "back",
    view: 24.0,
    body: r#"<path d="m12 19-7-7 7-7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><line x1="19" y1="12" x2="5" y2="12" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>"#,
};

static BACK_MOTION: Icon = Icon {
    name: "back-motion",
    view: 24.0,
    body: r#"<g><animateTransform attributeName="transform" type="translate" values="0 0;3 0;0 0" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.4s"/><path d="m12 19-7-7 7-7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></g><line x1="19" y1="12" x2="5" y2="12" stroke="currentColor" stroke-width="2" stroke-linecap="round"><animate attributeName="x2" values="5;10;5" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.4s"/></line>"#,
};

pub(crate) static BACK: AnimatedIcon = AnimatedIcon::new(&BACK_REST, &BACK_MOTION);

static FORWARD_REST: Icon = Icon {
    name: "forward",
    view: 24.0,
    body: r#"<line x1="5" y1="12" x2="19" y2="12" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><path d="m12 5 7 7-7 7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

static FORWARD_MOTION: Icon = Icon {
    name: "forward-motion",
    view: 24.0,
    body: r#"<line x1="5" y1="12" x2="19" y2="12" stroke="currentColor" stroke-width="2" stroke-linecap="round"><animate attributeName="x2" values="19;14;19" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.4s"/></line><g><animateTransform attributeName="transform" type="translate" values="0 0;-3 0;0 0" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.4s"/><path d="m12 5 7 7-7 7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></g>"#,
};

pub(crate) static FORWARD: AnimatedIcon = AnimatedIcon::new(&FORWARD_REST, &FORWARD_MOTION);

/// The download arrow dips toward the tray and springs back.
static DOWNLOAD_MOTION: Icon = Icon {
    name: "download-motion",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="translate" values="0 0;0 2;0 0" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.5s"/><path d="M8 2.5V9M5.25 6.5L8 9.25L10.75 6.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></g><path d="M3 12.25H13" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

pub(crate) static DOWNLOAD: AnimatedIcon =
    AnimatedIcon::new(&super::icons::DOWNLOAD, &DOWNLOAD_MOTION);

/// The theme glyphs turn once as the appearance changes. The system half-disc and the sun turn a
/// full or symmetrical turn, so each ends in its rest pose; the moon rocks and settles.
static SYSTEM_MOTION: Icon = Icon {
    name: "system-motion",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="0 8 8;360 8 8" keyTimes="0;1" calcMode="spline" keySplines="0.4 0 0.2 1" dur="0.5s"/><circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M8 2.5A5.5 5.5 0 0 1 8 13.5Z" fill="currentColor"/></g>"#,
};

static LIGHT_MOTION: Icon = Icon {
    name: "light-motion",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="0 8 8;45 8 8" keyTimes="0;1" calcMode="spline" keySplines="0.4 0 0.2 1" dur="0.5s"/><circle cx="8" cy="8" r="2.75" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M8 1.75V3.25M8 12.75V14.25M1.75 8H3.25M12.75 8H14.25M3.6 3.6L4.65 4.65M11.35 11.35L12.4 12.4M3.6 12.4L4.65 11.35M11.35 4.65L12.4 3.6" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/></g>"#,
};

static DARK_MOTION: Icon = Icon {
    name: "dark-motion",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="0 8 8;-18 8 8;0 8 8" keyTimes="0;0.5;1" calcMode="spline" keySplines="0.4 0 0.2 1;0.4 0 0.2 1" dur="0.5s"/><path d="M13 9.6A5.25 5.25 0 1 1 6.4 3A4.25 4.25 0 0 0 13 9.6Z" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/></g>"#,
};

pub(crate) static SYSTEM: AnimatedIcon = AnimatedIcon::new(&super::icons::SYSTEM, &SYSTEM_MOTION);
pub(crate) static LIGHT: AnimatedIcon = AnimatedIcon::new(&super::icons::LIGHT, &LIGHT_MOTION);
pub(crate) static DARK: AnimatedIcon = AnimatedIcon::new(&super::icons::DARK, &DARK_MOTION);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_tracks_one_icon_and_ignores_leaves_of_others() {
        let mut slot = None;
        assert!(set_hovered(&mut slot, "reload", true));
        assert_eq!(slot.as_deref(), Some("reload"));
        // Entering the same owner again changes nothing.
        assert!(!set_hovered(&mut slot, "reload", true));
        // Leaving a different owner keeps the hovered one.
        assert!(!set_hovered(&mut slot, "other", false));
        assert_eq!(slot.as_deref(), Some("reload"));
        assert!(set_hovered(&mut slot, "reload", false));
        assert_eq!(slot, None);
    }

    #[test]
    fn two_owners_of_one_icon_hover_independently() {
        // The toolbar reload button and the Reload menu row share RELOAD; hovering one must not
        // play the other.
        let mut slot = None;
        assert!(set_hovered(&mut slot, "reload", true));
        assert!(set_hovered(&mut slot, "tab-context-item0", true));
        assert_eq!(slot.as_deref(), Some("tab-context-item0"));
        assert!(!set_hovered(&mut slot, "reload", false));
    }

    #[test]
    fn motion_glyphs_animate_under_the_text_system() {
        use rune::text::{IconKind, TextSystem};

        // Rune classes an icon as animated only if its SMIL parses, so a typo in a motion body
        // would otherwise fail silently to a static icon.
        let mut text = TextSystem::new(&[], ("Inter", "Inter"));
        for motion in [
            &RELOAD_MOTION,
            &COPY_MOTION,
            &PLUS_MOTION,
            &HISTORY_MOTION,
            &BACK_MOTION,
            &FORWARD_MOTION,
            &DOWNLOAD_MOTION,
            &SYSTEM_MOTION,
            &LIGHT_MOTION,
            &DARK_MOTION,
        ] {
            assert_eq!(
                text.icon_kind(motion),
                IconKind::AnimatedMono,
                "{}",
                motion.name
            );
        }
    }
}
