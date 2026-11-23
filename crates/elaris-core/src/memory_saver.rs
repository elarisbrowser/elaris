// SPDX-License-Identifier: GPL-3.0-only

//! Memory Saver: which background tabs to put to sleep, and when.
//!
//! Two things put a tab to sleep. Time: a tab left unseen for its mode's idle limit sleeps. Memory
//! pressure: while the system runs short, the limit drops, and the longest-unseen tab sleeps, one
//! per check, so each check can measure what the last one freed. A tab doing something that sleep
//! would break, such as playing audio or using the microphone, is never chosen.

use std::time::Duration;

/// How eagerly Memory Saver puts background tabs to sleep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemorySaver {
    Off,
    #[default]
    Balanced,
    Maximum,
}

/// How long a tab may go unseen before it sleeps, per mode and pressure.
struct Limits {
    normal: Duration,
    elevated: Duration,
    critical: Duration,
}

const BALANCED: Limits = Limits {
    normal: Duration::from_secs(60 * 60),
    elevated: Duration::from_secs(10 * 60),
    critical: Duration::from_secs(60),
};

const MAXIMUM: Limits = Limits {
    normal: Duration::from_secs(15 * 60),
    elevated: Duration::from_secs(2 * 60),
    critical: Duration::from_secs(30),
};

/// How often to check, by pressure: rarely while memory is plentiful, often while it is short.
const CHECK_NORMAL: Duration = Duration::from_secs(30);
const CHECK_UNDER_PRESSURE: Duration = Duration::from_secs(5);

/// Linux reports no pressure level, so it is judged by the share of memory still available.
const ELEVATED_AVAILABLE_PERCENT: u64 = 20;
const CRITICAL_AVAILABLE_PERCENT: u64 = 10;

impl MemorySaver {
    pub const ALL: [Self; 3] = [Self::Off, Self::Balanced, Self::Maximum];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Balanced => "Balanced",
            Self::Maximum => "Maximum",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Off => "Background tabs stay awake.",
            Self::Balanced => {
                "Tabs unused for an hour sleep, and sooner when your computer runs low on memory. \
                 Favourites stay awake unless memory runs critically low."
            }
            Self::Maximum => {
                "Tabs unused for 15 minutes sleep, and within minutes when memory runs low."
            }
        }
    }

    pub fn config_key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Balanced => "balanced",
            Self::Maximum => "maximum",
        }
    }

    pub fn from_config_key(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.config_key().eq_ignore_ascii_case(value))
    }

    fn limits(self) -> Option<&'static Limits> {
        match self {
            Self::Off => None,
            Self::Balanced => Some(&BALANCED),
            Self::Maximum => Some(&MAXIMUM),
        }
    }

    /// How long a tab may go unseen under `pressure` before it sleeps.
    pub fn idle_limit(self, pressure: Pressure) -> Option<Duration> {
        self.limits().map(|limits| match pressure {
            Pressure::Normal => limits.normal,
            Pressure::Elevated => limits.elevated,
            Pressure::Critical => limits.critical,
        })
    }

    /// Whether favourites, which often hold mail and chat, may sleep under `pressure`.
    fn sleeps_favourites(self, pressure: Pressure) -> bool {
        self == Self::Maximum || pressure == Pressure::Critical
    }

    /// How long to wait before the next check, or `None` while Memory Saver is off.
    pub fn check_interval(self, pressure: Pressure) -> Option<Duration> {
        self.limits().map(|_| match pressure {
            Pressure::Normal => CHECK_NORMAL,
            _ => CHECK_UNDER_PRESSURE,
        })
    }

    /// The tabs to put to sleep now. Every tab past its normal idle limit sleeps; under pressure,
    /// so does the longest-unseen tab past the lowered limit.
    pub fn choose<Id: Copy>(self, pressure: Pressure, candidates: &[Candidate<Id>]) -> Vec<Id> {
        let Some(limits) = self.limits() else {
            return Vec::new();
        };
        let eligible = |candidate: &&Candidate<Id>| {
            !candidate.busy && (!candidate.favourite || self.sleeps_favourites(pressure))
        };
        let mut chosen: Vec<Id> = candidates
            .iter()
            .filter(eligible)
            .filter(|candidate| candidate.idle >= limits.normal)
            .map(|candidate| candidate.id)
            .collect();
        if pressure != Pressure::Normal {
            let limit = self.idle_limit(pressure).unwrap_or(limits.normal);
            if let Some(oldest) = candidates
                .iter()
                .filter(eligible)
                .filter(|candidate| candidate.idle >= limit && candidate.idle < limits.normal)
                .max_by_key(|candidate| candidate.idle)
            {
                chosen.push(oldest.id);
            }
        }
        chosen
    }
}

/// How short the system is of memory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pressure {
    #[default]
    Normal,
    Elevated,
    Critical,
}

impl Pressure {
    /// From macOS's `kern.memorystatus_vm_pressure_level`: 1 normal, 2 warning, 4 critical.
    pub fn from_macos_level(level: i32) -> Self {
        match level {
            4.. => Self::Critical,
            2..=3 => Self::Elevated,
            _ => Self::Normal,
        }
    }

    /// From the text of Linux's `/proc/meminfo`, by the share of memory still available.
    pub fn from_meminfo(meminfo: &str) -> Option<Self> {
        let field = |name: &str| {
            meminfo.lines().find_map(|line| {
                line.strip_prefix(name)?
                    .strip_prefix(':')?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        };
        let total = field("MemTotal").filter(|total| *total > 0)?;
        let available = field("MemAvailable")?;
        let percent = available.saturating_mul(100) / total;
        Some(if percent < CRITICAL_AVAILABLE_PERCENT {
            Self::Critical
        } else if percent < ELEVATED_AVAILABLE_PERCENT {
            Self::Elevated
        } else {
            Self::Normal
        })
    }
}

/// A background tab that could sleep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate<Id> {
    pub id: Id,
    /// How long since the tab was last shown.
    pub idle: Duration,
    pub favourite: bool,
    /// Doing something that sleep would break, such as playing audio or using the microphone.
    pub busy: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    fn tab(id: u32, minutes: u64) -> Candidate<u32> {
        Candidate {
            id,
            idle: MINUTE * minutes as u32,
            favourite: false,
            busy: false,
        }
    }

    #[test]
    fn off_never_sleeps_a_tab() {
        let tabs = [tab(1, 600)];
        assert!(
            MemorySaver::Off
                .choose(Pressure::Critical, &tabs)
                .is_empty()
        );
        assert_eq!(MemorySaver::Off.check_interval(Pressure::Normal), None);
    }

    #[test]
    fn tabs_past_the_idle_limit_sleep() {
        let tabs = [tab(1, 5), tab(2, 20), tab(3, 90)];
        assert_eq!(MemorySaver::Balanced.choose(Pressure::Normal, &tabs), [3]);
        assert_eq!(MemorySaver::Maximum.choose(Pressure::Normal, &tabs), [2, 3]);
    }

    #[test]
    fn pressure_sleeps_the_longest_unseen_tab_one_at_a_time() {
        let tabs = [tab(1, 12), tab(2, 30), tab(3, 0)];
        assert_eq!(MemorySaver::Balanced.choose(Pressure::Elevated, &tabs), [2]);
        assert_eq!(
            MemorySaver::Balanced.choose(Pressure::Normal, &tabs),
            [0; 0]
        );
    }

    #[test]
    fn busy_tabs_never_sleep() {
        let mut playing = tab(1, 600);
        playing.busy = true;
        assert!(
            MemorySaver::Maximum
                .choose(Pressure::Critical, &[playing])
                .is_empty()
        );
    }

    #[test]
    fn balanced_spares_favourites_until_memory_is_critical() {
        let mut favourite = tab(1, 600);
        favourite.favourite = true;
        assert!(
            MemorySaver::Balanced
                .choose(Pressure::Elevated, &[favourite])
                .is_empty()
        );
        assert_eq!(
            MemorySaver::Balanced.choose(Pressure::Critical, &[favourite]),
            [1]
        );
        assert_eq!(
            MemorySaver::Maximum.choose(Pressure::Normal, &[favourite]),
            [1]
        );
    }

    #[test]
    fn pressure_is_read_from_macos_and_linux() {
        assert_eq!(Pressure::from_macos_level(1), Pressure::Normal);
        assert_eq!(Pressure::from_macos_level(2), Pressure::Elevated);
        assert_eq!(Pressure::from_macos_level(4), Pressure::Critical);
        let meminfo = |available| {
            format!("MemTotal:       1000 kB\nMemFree: 1 kB\nMemAvailable:   {available} kB\n")
        };
        assert_eq!(
            Pressure::from_meminfo(&meminfo(500)),
            Some(Pressure::Normal)
        );
        assert_eq!(
            Pressure::from_meminfo(&meminfo(150)),
            Some(Pressure::Elevated)
        );
        assert_eq!(
            Pressure::from_meminfo(&meminfo(50)),
            Some(Pressure::Critical)
        );
        assert_eq!(Pressure::from_meminfo("MemTotal: 1000 kB"), None);
    }

    #[test]
    fn modes_round_trip_through_their_config_keys() {
        for mode in MemorySaver::ALL {
            assert_eq!(MemorySaver::from_config_key(mode.config_key()), Some(mode));
        }
        assert_eq!(MemorySaver::from_config_key("eco"), None);
    }
}
