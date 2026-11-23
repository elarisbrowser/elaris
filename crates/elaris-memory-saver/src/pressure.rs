// SPDX-License-Identifier: GPL-3.0-only

//! How short the system is of memory: `Pressure::current()`. macOS reports a pressure level; on
//! Linux it is judged from `/proc/meminfo`. Where neither can be read, memory is taken as plentiful.

/// Linux reports no pressure level, so it is judged by the share of memory still available.
const ELEVATED_AVAILABLE_PERCENT: u64 = 20;
const CRITICAL_AVAILABLE_PERCENT: u64 = 10;

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

impl Pressure {
    /// The system's memory pressure now.
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        return macos::pressure_level().map_or(Self::Normal, Self::from_macos_level);
        #[cfg(target_os = "linux")]
        return std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|meminfo| Self::from_meminfo(&meminfo))
            .unwrap_or_default();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        Self::Normal
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_int, c_void};

    unsafe extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            old: *mut c_void,
            old_len: *mut usize,
            new: *mut c_void,
            new_len: usize,
        ) -> c_int;
    }

    /// `kern.memorystatus_vm_pressure_level`, as Activity Monitor shows it.
    pub(super) fn pressure_level() -> Option<i32> {
        let mut level: c_int = 0;
        let mut len = std::mem::size_of::<c_int>();
        // SAFETY: The name is NUL-terminated and `level` is a writable int whose size is `len`.
        let status = unsafe {
            sysctlbyname(
                c"kern.memorystatus_vm_pressure_level".as_ptr(),
                (&raw mut level).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        (status == 0).then_some(level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
