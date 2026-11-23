// SPDX-License-Identifier: GPL-3.0-only

//! How short the system is of memory: `memory::pressure()`. macOS reports a pressure level; on
//! Linux it is judged from `/proc/meminfo`. Where neither can be read, memory is taken as plentiful.

use elaris_core::memory_saver::Pressure;

pub(crate) fn pressure() -> Pressure {
    #[cfg(target_os = "macos")]
    return super::macos::memory::pressure_level()
        .map_or(Pressure::Normal, Pressure::from_macos_level);
    #[cfg(target_os = "linux")]
    return std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|meminfo| Pressure::from_meminfo(&meminfo))
        .unwrap_or_default();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    Pressure::Normal
}
