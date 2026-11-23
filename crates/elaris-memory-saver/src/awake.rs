// SPDX-License-Identifier: GPL-3.0-only

//! Why a tab stays awake, and the sites the user always keeps awake.

/// Something that keeps a tab from sleeping, in the words the interface shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AwakeReason {
    PlayingAudio,
    PlayedAudioRecently,
    UsingMicrophone,
    UsingLocation,
    Loading,
    Dialog,
    Fullscreen,
    Crashed,
    OpenPopup,
    Notifications,
    KeepAwakeSite,
    Favourite,
    PrivateWindow,
    PictureInPicture,
    DevTools,
    UnsavedEdits,
    Downloading,
    UpdatingInBackground,
    NotRestorable,
}

impl AwakeReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::PlayingAudio => "Playing audio",
            Self::PlayedAudioRecently => "Played audio recently",
            Self::UsingMicrophone => "Using the microphone",
            Self::UsingLocation => "Using your location",
            Self::Loading => "Loading",
            Self::Dialog => "Waiting for you to answer",
            Self::Fullscreen => "In full screen",
            Self::Crashed => "Crashed",
            Self::OpenPopup => "Has an open pop-up",
            Self::Notifications => "May show notifications",
            Self::KeepAwakeSite => "Always kept awake",
            Self::Favourite => "A favourite",
            Self::PrivateWindow => "In a private window",
            Self::PictureInPicture => "Showing Picture in Picture",
            Self::DevTools => "Open in DevTools",
            Self::UnsavedEdits => "Has unsaved changes",
            Self::Downloading => "Downloading",
            Self::UpdatingInBackground => "Updating in the background",
            Self::NotRestorable => "Can't be restored",
        }
    }

    /// The reasons Ladybird gave for keeping a tab loaded, from its `DiscardBlockers` bits. The
    /// tab being shown, which Elaris knows itself, is left out.
    pub fn from_ladybird(blockers: u32) -> Vec<Self> {
        const REASONS: [(u32, AwakeReason); 15] = [
            (1 << 1, AwakeReason::NotRestorable),
            (1 << 5, AwakeReason::Loading),
            (1 << 6, AwakeReason::Crashed),
            (1 << 7, AwakeReason::PlayingAudio),
            (1 << 8, AwakeReason::PlayedAudioRecently),
            (1 << 9, AwakeReason::UsingMicrophone),
            (1 << 10, AwakeReason::UsingLocation),
            (1 << 11, AwakeReason::PictureInPicture),
            (1 << 12, AwakeReason::DevTools),
            (1 << 13, AwakeReason::UnsavedEdits),
            (1 << 14, AwakeReason::Downloading),
            (1 << 15, AwakeReason::Dialog),
            (1 << 16, AwakeReason::UpdatingInBackground),
            (1 << 17, AwakeReason::UpdatingInBackground),
            (1 << 18, AwakeReason::Notifications),
        ];
        let mut reasons = Vec::new();
        for (bit, reason) in REASONS {
            if blockers & bit != 0 && !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        reasons
    }
}

/// The site a keep-awake entry names, from whatever the user typed: `https://www.Example.com/a`
/// becomes `example.com`. `None` when it names no site.
pub fn site_of(input: &str) -> Option<String> {
    let input = input.trim().to_ascii_lowercase();
    let rest = input
        .split_once("://")
        .map_or(input.as_str(), |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    let host = host.strip_prefix("www.").unwrap_or(host);
    let valid = !host.is_empty()
        && (host.contains('.') || host == "localhost")
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'));
    valid.then(|| host.to_owned())
}

/// Whether a page's host is one of the sites the user keeps awake, or a subdomain of one.
pub fn keeps_awake(sites: &[String], host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    sites.iter().any(|site| {
        host == *site
            || host
                .strip_suffix(site.as_str())
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_is_read_from_whatever_names_it() {
        assert_eq!(
            site_of("https://www.Example.com/inbox?x"),
            Some("example.com".into())
        );
        assert_eq!(
            site_of(" mail.google.com:443 "),
            Some("mail.google.com".into())
        );
        assert_eq!(site_of("localhost"), Some("localhost".into()));
        for invalid in ["", "example", "exa mple.com", ".com", "https://"] {
            assert_eq!(site_of(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn a_kept_site_covers_its_subdomains_only() {
        let sites = vec!["example.com".to_owned()];
        assert!(keeps_awake(&sites, "example.com"));
        assert!(keeps_awake(&sites, "mail.Example.com"));
        assert!(!keeps_awake(&sites, "notexample.com"));
        assert!(!keeps_awake(&sites, "example.org"));
    }

    #[test]
    fn ladybirds_reasons_are_named_once_each() {
        let reasons = AwakeReason::from_ladybird((1 << 2) | (1 << 7) | (1 << 16) | (1 << 17));
        assert_eq!(
            reasons,
            [AwakeReason::PlayingAudio, AwakeReason::UpdatingInBackground]
        );
        assert_eq!(reasons[0].label(), "Playing audio");
    }
}
