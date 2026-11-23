// SPDX-License-Identifier: GPL-3.0-only

//! The kernel's memory pressure level, as Activity Monitor shows it.

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

/// `kern.memorystatus_vm_pressure_level`: 1 normal, 2 warning, 4 critical.
pub(crate) fn pressure_level() -> Option<i32> {
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
