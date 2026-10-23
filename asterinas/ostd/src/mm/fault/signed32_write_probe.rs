// SPDX-License-Identifier: MPL-2.0
//! Narrow ktest-only hardware probe for a supervisor write-protection fault.
#![cfg(all(ktest, target_arch = "x86_64"))]

use crate::{
    Error,
    arch::cpu::context::CpuException,
    arch::trap::TrapFrame,
    cpu, irq,
    mm::{Vaddr, module_address::NearTextAddressReservation},
};
use core::sync::atomic::{AtomicUsize, Ordering};

const IDLE: usize = 0;
const ARMED: usize = 1;
static STATE: AtomicUsize = AtomicUsize::new(IDLE);
static ADDRESS: AtomicUsize = AtomicUsize::new(0);
static TRAPS: AtomicUsize = AtomicUsize::new(0);
static ERROR: AtomicUsize = AtomicUsize::new(0);
static CR2: AtomicUsize = AtomicUsize::new(0);
static WP: AtomicUsize = AtomicUsize::new(0);

struct ArmedProbe;
impl Drop for ArmedProbe {
    fn drop(&mut self) {
        ADDRESS.store(0, Ordering::Relaxed);
        STATE.store(IDLE, Ordering::Release);
    }
}
fn write_protect_enabled() -> bool {
    x86_64::registers::control::Cr0::read_raw() & (1 << 16) != 0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StoreObservation {
    pub faulted: bool,
    pub trap_count: usize,
    pub error_code: usize,
    pub fault_addr: usize,
    pub write_protect_enabled: bool,
}

/// Attempt one byte store to a caller-owned, live, allocated signed32 address.
///
/// # Safety
/// Caller guarantees `address` is mapped, allocated, live for this call, and
/// that the mapping is supervisor-accessible data (RO or RW), not executable code.
/// Permissions must not be concurrently changed. A successful store may mutate it.
pub(crate) unsafe fn probe_store_byte(
    address: usize,
    value: u8,
) -> Result<StoreObservation, Error> {
    let arena = NearTextAddressReservation::signed32_arena_range();
    if cpu::num_cpus() != 1
        || !arena.contains(&(address as Vaddr))
        || STATE.load(Ordering::Acquire) != IDLE
    {
        return Err(Error::InvalidArgs);
    }
    let _irq = irq::disable_local();
    if cpu::num_cpus() != 1
        || STATE
            .compare_exchange(IDLE, ARMED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return Err(Error::AccessDenied);
    }
    let _armed = ArmedProbe;
    let before_wp = write_protect_enabled();
    if !before_wp {
        return Err(Error::AccessDenied);
    }
    ADDRESS.store(address, Ordering::Relaxed);
    TRAPS.store(0, Ordering::Relaxed);
    ERROR.store(0, Ordering::Relaxed);
    CR2.store(0, Ordering::Relaxed);
    WP.store(usize::from(before_wp), Ordering::Relaxed);
    // SAFETY: Caller retains the mapped data byte. The sole faulting opcode is
    // recovered only by the exact-site/address/error ktest handler below.
    let result = unsafe { __cybermanju_signed32_ktest_store(address, value) };
    let count = TRAPS.load(Ordering::Relaxed);
    let observation = StoreObservation {
        faulted: result == 1,
        trap_count: count,
        error_code: ERROR.load(Ordering::Relaxed),
        fault_addr: CR2.load(Ordering::Relaxed),
        write_protect_enabled: WP.load(Ordering::Relaxed) != 0,
    };
    let expected_fault = result == 1
        && count == 1
        && observation.error_code == 3
        && observation.fault_addr == address;
    let expected_store =
        result == 0 && count == 0 && observation.error_code == 0 && observation.fault_addr == 0;
    if write_protect_enabled()
        && observation.write_protect_enabled
        && (expected_fault || expected_store)
    {
        Ok(observation)
    } else {
        Err(Error::IoError)
    }
}

unsafe extern "C" {
    fn __cybermanju_signed32_ktest_store(address: usize, value: u8) -> usize;
    static __cybermanju_signed32_ktest_store_site: u8;
    static __cybermanju_signed32_ktest_store_fixup: u8;
}

core::arch::global_asm!(
    r#"
    .section .text
    .global __cybermanju_signed32_ktest_store
    .global __cybermanju_signed32_ktest_store_site
    .global __cybermanju_signed32_ktest_store_fixup
    .type __cybermanju_signed32_ktest_store,@function
__cybermanju_signed32_ktest_store:
    movb %sil, (%rdi)
    xor %eax, %eax
    ret
__cybermanju_signed32_ktest_store_fixup:
    mov $1, %eax
    ret
    .size __cybermanju_signed32_ktest_store, .-__cybermanju_signed32_ktest_store
    .set __cybermanju_signed32_ktest_store_site, __cybermanju_signed32_ktest_store+0
    .section .note.GNU-stack,"",@progbits
"#,
    options(att_syntax)
);

#[derive(Clone, Copy)]
struct Candidate {
    armed: bool,
    address_ok: bool,
    ip_ok: bool,
    error: usize,
    cs: usize,
    cpus: usize,
    wp: bool,
    traps: usize,
}
fn accepts(c: Candidate) -> bool {
    c.armed
        && c.address_ok
        && c.ip_ok
        && c.error == 0b11
        && c.cs & 3 == 0
        && c.cpus == 1
        && c.wp
        && c.traps == 0
}

pub(crate) fn verify_context_rejections_for_test() -> usize {
    let good = Candidate {
        armed: true,
        address_ok: true,
        ip_ok: true,
        error: 3,
        cs: 8,
        cpus: 1,
        wp: true,
        traps: 0,
    };
    let mut n = 0;
    for bad in [
        Candidate {
            armed: false,
            ..good
        },
        Candidate {
            address_ok: false,
            ..good
        },
        Candidate {
            ip_ok: false,
            ..good
        },
        Candidate { error: 1, ..good },
        Candidate { error: 2, ..good },
        Candidate { error: 7, ..good },
        Candidate {
            error: 0b1011,
            ..good
        },
        Candidate {
            error: 0b10011,
            ..good
        },
        Candidate { cs: 0x1b, ..good },
        Candidate { cpus: 2, ..good },
        Candidate { wp: false, ..good },
        Candidate { traps: 1, ..good },
    ] {
        assert!(!accepts(bad));
        n += 1;
    }
    assert!(accepts(good));
    assert!(n >= 6);
    n
}

pub(super) fn try_recover(f: &mut TrapFrame, exception: &CpuException, fault_addr: Vaddr) -> bool {
    let CpuException::PageFault(info) = exception else {
        return false;
    };
    let site = core::ptr::addr_of!(__cybermanju_signed32_ktest_store_site) as usize;
    let fixup = core::ptr::addr_of!(__cybermanju_signed32_ktest_store_fixup) as usize;
    let cs = f.cs;
    let wp = write_protect_enabled();
    let c = Candidate {
        armed: STATE.load(Ordering::Acquire) == ARMED,
        address_ok: ADDRESS.load(Ordering::Relaxed) == fault_addr && info.addr == fault_addr,
        ip_ok: f.rip == site,
        error: info.error_code.bits(),
        cs,
        cpus: cpu::num_cpus(),
        wp,
        traps: TRAPS.load(Ordering::Relaxed),
    };
    if !accepts(c) {
        return false;
    }
    if TRAPS
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    ERROR.store(info.error_code.bits(), Ordering::Relaxed);
    CR2.store(info.addr, Ordering::Relaxed);
    WP.store(usize::from(wp), Ordering::Relaxed);
    f.rip = fixup;
    true
}
