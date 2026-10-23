// SPDX-License-Identifier: MPL-2.0
//! Explicit C calling-convention helpers, not module execution permission.

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

/// Signature of the fixed non-overlapping byte-copy shim; this is not admitted
/// to the Linux export registry and does not define a linker symbol `memcpy`.
///
/// # Safety
/// For `n > 0`, `src` must point to initialized ordinary RAM readable for `n`
/// bytes and `dst` to ordinary RAM writable for `n` bytes, both within live
/// allocations, with disjoint ranges and `n <= isize::MAX`. No racing access or
/// MMIO is permitted. For `n == 0`, null is accepted and neither pointer is
/// accessed; the function returns `dst`.
pub type MemcpyFn = unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> *mut c_void;

#[cfg(ktest)]
static MEMCPY_SHIM_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn memcpy_impl(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    #[cfg(ktest)]
    MEMCPY_SHIM_CALLS.fetch_add(1, Ordering::Relaxed);
    for i in 0..n {
        // Volatile byte accesses prevent compiler lowering this loop to memcpy.
        // SAFETY: caller guarantees both complete ranges are valid and disjoint.
        let byte = unsafe { src.cast::<u8>().add(i).read_volatile() };
        // SAFETY: as above, each destination byte is writable.
        unsafe { dst.cast::<u8>().add(i).write_volatile(byte) };
    }
    dst
}

/// Returns the Rust-mangled callback only; it is not a linker/registry export.
/// The [`MemcpyFn`] contract applies. It allocates nothing, locks nothing,
/// and does not sleep.
pub fn memcpy_entry() -> MemcpyFn {
    memcpy_impl
}

#[cfg(ktest)]
#[repr(C)]
#[derive(Debug)]
struct MemcpyCReport {
    calls: u32,
    passed_mask: u32,
    source_preserved: u32,
    guards_preserved: u32,
    returns_destination: u32,
    destination_verified: u32,
}
#[cfg(ktest)]
const _: () = {
    assert!(core::mem::size_of::<MemcpyCReport>() == 24);
    assert!(core::mem::align_of::<MemcpyCReport>() == 4);
};

/// Results from the private 16-case C memcpy shim probe (not kernel ABI admission).
#[cfg(ktest)]
#[derive(Debug)]
pub struct MemcpyCProbeReport {
    /// Number of calls made through the C callback pointer.
    pub c_calls: u32,
    /// Counter delta in Rust; tests isolate unrelated calls.
    pub rust_calls: usize,
    /// One bit per fully matching independent C case.
    pub passed_mask: u32,
    /// All source buffers stayed byte-identical after every call.
    pub source_preserved: bool,
    /// All source/destination pre/post guards stayed unchanged.
    pub guards_preserved: bool,
    /// Every return value was exactly its destination argument.
    pub returns_destination: bool,
    /// Entire destinations matched the precomputed oracle, including untouched bytes.
    pub destination_verified: bool,
}

/// Runs trusted C with call-local non-overlapping buffers. This validates only
/// the callback shim, not a `.ko`, complete Linux ABI, RX/WX, SMP, or production use.
#[cfg(ktest)]
pub fn run_memcpy_c_probe_for_test() -> MemcpyCProbeReport {
    unsafe extern "C" {
        fn cybermanju_memcpy_c_probe(callback: MemcpyFn, report: *mut MemcpyCReport);
    }
    let mut report = MemcpyCReport {
        calls: 0,
        passed_mask: 0,
        source_preserved: 0,
        guards_preserved: 0,
        returns_destination: 0,
        destination_verified: 0,
    };
    let before = MEMCPY_SHIM_CALLS.load(Ordering::Relaxed);
    // SAFETY: C/Rust assert the same report layout; the C probe owns call-local
    // bounded, disjoint buffers and invokes only the fixed callback synchronously.
    unsafe { cybermanju_memcpy_c_probe(memcpy_entry(), &mut report) };
    let after = MEMCPY_SHIM_CALLS.load(Ordering::Relaxed);
    MemcpyCProbeReport {
        c_calls: report.calls,
        rust_calls: after.wrapping_sub(before),
        passed_mask: report.passed_mask,
        source_preserved: report.source_preserved == 1,
        guards_preserved: report.guards_preserved == 1,
        returns_destination: report.returns_destination == 1,
        destination_verified: report.destination_verified == 1,
    }
}
