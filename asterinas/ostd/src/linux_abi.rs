// SPDX-License-Identifier: MPL-2.0
//! One explicit C calling-convention helper, not module execution permission.

use core::ffi::c_void;
#[cfg(ktest)]
use core::sync::atomic::{AtomicUsize, Ordering};

/// Signature of the fixed unsigned-byte comparison shim.
///
/// # Safety
/// Calling this function pointer with `n > 0` requires two initialized ordinary
/// RAM buffers readable for `n` bytes, within their allocations, with
/// `n <= isize::MAX`. Their bytes must remain stable during the call: no racing
/// writes or MMIO. The buffers may overlap. With `n == 0`, null pointers are
/// accepted and no buffer is accessed. This is not a validated-pointer API.
pub type MemcmpFn = unsafe extern "C" fn(*const c_void, *const c_void, usize) -> i32;

#[cfg(ktest)]
static C_SHIM_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn memcmp_impl(lhs: *const c_void, rhs: *const c_void, n: usize) -> i32 {
    #[cfg(ktest)]
    C_SHIM_CALLS.fetch_add(1, Ordering::Relaxed);
    if n == 0 {
        return 0;
    }
    for i in 0..n {
        // SAFETY: the function pointer's caller contract covers both stable,
        // readable allocations and every byte offset in this bounded loop.
        let left = unsafe { lhs.cast::<u8>().add(i).read_volatile() };
        // SAFETY: the same caller contract applies to the second buffer.
        let right = unsafe { rhs.cast::<u8>().add(i).read_volatile() };
        if left != right {
            return i32::from(left) - i32::from(right);
        }
    }
    0
}

/// Returns the fixed, Rust-mangled implementation; it exports no linker symbol
/// named `memcmp`. The [`MemcmpFn`] caller contract applies to every invocation.
/// The shim allocates nothing, acquires no locks and does not sleep.
pub fn memcmp_entry() -> MemcmpFn {
    memcmp_impl
}

#[cfg(ktest)]
#[repr(C)]
#[derive(Debug)]
struct CReport {
    calls: u32,
    passed_mask: u32,
    buffers_preserved: u32,
}
#[cfg(ktest)]
const _: () = {
    assert!(core::mem::size_of::<CReport>() == 12);
    assert!(core::mem::align_of::<CReport>() == 4);
};

/// Summary of the C ABI probe's 16 callback cases, not Linux module execution.
#[cfg(ktest)]
#[derive(Debug)]
pub struct MemcmpCProbeReport {
    /// Number of callback invocations made by the C probe.
    pub c_calls: u32,
    /// Delta observed by the Rust callback; concurrent callers may add to it.
    pub rust_calls: usize,
    /// Cases matching the independent C oracle computed before each callback.
    pub passed_mask: u32,
    /// Every C buffer and guard remained unchanged after every callback.
    pub buffers_preserved: bool,
}

/// Runs trusted freestanding C with call-local buffers and a fixed callback.
/// The test caller must isolate unrelated shim calls for an exact call count;
/// concurrency affects diagnostics, not the safety of the private C buffers.
#[cfg(ktest)]
pub fn run_memcmp_c_probe_for_test() -> MemcmpCProbeReport {
    unsafe extern "C" {
        fn cybermanju_memcmp_c_probe(callback: MemcmpFn, report: *mut CReport);
    }
    let mut c_report = CReport {
        calls: 0,
        passed_mask: 0,
        buffers_preserved: 0,
    };
    let before = C_SHIM_CALLS.load(Ordering::Relaxed);
    // SAFETY: C and Rust assert the same report layout. The trusted C routine
    // initializes its own bounded arrays and passes only valid ranges (or the
    // explicitly supported null/zero case) to our fixed callback. Its report
    // pointer is live and exclusively borrowed until the synchronous call ends.
    unsafe { cybermanju_memcmp_c_probe(memcmp_entry(), &mut c_report) };
    let after = C_SHIM_CALLS.load(Ordering::Relaxed);
    MemcmpCProbeReport {
        c_calls: c_report.calls,
        rust_calls: after.wrapping_sub(before),
        passed_mask: c_report.passed_mask,
        buffers_preserved: c_report.buffers_preserved == 1,
    }
}
