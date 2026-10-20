// SPDX-License-Identifier: MPL-2.0
//! OS callbacks for unmodified NVIDIA time/thread translation units.
use super::runtime;

const NV_OK: u32 = 0;
const NV_ERR_INVALID_POINTER: u32 = 0x3d;
const NV_ERR_INVALID_STATE: u32 = 0x40;
const NV_ERR_NOT_SUPPORTED: u32 = 0x56;

/// # Safety
/// Both output slots must be valid, aligned and nonoverlapping. A clock must be
/// installed before entering the upstream time API: its C body ignores status.
/// Consequently an unavailable/out-of-range clock halts rather than synthesizing
/// an epoch-zero timestamp that RM could mistake for a working clock.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_get_system_time(seconds: *mut u32, micros: *mut u32) -> u32 {
    if seconds.is_null() || micros.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    let ns = runtime::realtime_ns()
        .expect("NVIDIA realtime clock unavailable; refusing false timestamp");
    let sec = u32::try_from(ns / 1_000_000_000).expect("NVIDIA realtime seconds exceed ABI range");
    // SAFETY: Caller grants both output slots.
    unsafe {
        seconds.write(sec);
        micros.write(((ns % 1_000_000_000) / 1_000) as u32);
    }
    NV_OK
}

#[unsafe(no_mangle)]
extern "C" fn os_get_monotonic_time_ns() -> u64 {
    runtime::monotonic_ns().expect("NVIDIA monotonic clock unavailable; refusing false timestamp")
}
#[unsafe(no_mangle)]
extern "C" fn os_get_monotonic_time_ns_hr() -> u64 {
    // Same calibrated real clocksource; no claim of a second precision domain.
    os_get_monotonic_time_ns()
}
#[unsafe(no_mangle)]
extern "C" fn os_get_monotonic_tick_resolution_ns() -> u64 {
    runtime::monotonic_resolution_ns().expect("NVIDIA clock resolution unavailable")
}

/// # Safety
/// `identity` must be a valid writable u64 slot. Returned value is an opaque
/// identity for the currently live OSTD task, not a userspace/POSIX TID.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_get_current_thread(identity: *mut u64) -> u32 {
    if identity.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    // SAFETY: Valid output slot by contract, always cleared on failure.
    unsafe {
        identity.write(0);
    }
    match runtime::current_task_identity() {
        Ok(value) => {
            unsafe {
                identity.write(value);
            }
            NV_OK
        }
        Err(runtime::Error::Unsupported) => NV_ERR_NOT_SUPPORTED,
        Err(_) => NV_ERR_INVALID_STATE,
    }
}

#[unsafe(no_mangle)]
extern "C" fn os_schedule() -> u32 {
    match runtime::yield_current_task() {
        Ok(()) => NV_OK,
        Err(runtime::Error::Unsupported) => NV_ERR_NOT_SUPPORTED,
        Err(_) => NV_ERR_INVALID_STATE,
    }
}
