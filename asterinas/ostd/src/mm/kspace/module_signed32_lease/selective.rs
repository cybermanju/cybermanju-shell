// SPDX-License-Identifier: MPL-2.0
//! Experimental selective supervisor data mapping; never adds execute permission.
use super::{
    KERNEL_PAGE_TABLE, MappedItemRef, Signed32LeaseAudit, Signed32RwMapping, valid_property,
};
use crate::{
    Error,
    arch::mm::tlb_flush_addr_range,
    cpu::num_cpus,
    irq::{self, DisabledLocalIrqGuard},
    mm::{HasPaddr, PAGE_SIZE, Vaddr},
};
use alloc::vec::Vec;
use core::ops::Range;

/// Selective RO/NX and RW/NX aliases. The original linear RW alias remains;
/// this is not physical W^X, SMP-safe, or a production execution API.
pub struct Signed32SelectiveNxMapping {
    inner: Signed32RwMapping,
    read_only: Vec<bool>,
}

impl Signed32RwMapping {
    /// Consume this lease, removing W only from pages selected by `read_only`.
    /// Every page remains NX and all other PTE attributes are preserved.
    /// The mask must cover exactly every staged page. Errors consume the lease
    /// and run its existing unmap/flush cleanup; unexpected cleanup stays fail-closed.
    pub fn into_selective_nx(
        self,
        read_only: &[bool],
    ) -> Result<Signed32SelectiveNxMapping, Error> {
        transition(self, read_only, None, &mut 0)
    }

    #[cfg(ktest)]
    pub fn into_selective_nx_with_fault_for_test(
        self,
        read_only: &[bool],
        after: usize,
        observed: &mut usize,
    ) -> Result<Signed32SelectiveNxMapping, Error> {
        transition(self, read_only, Some(after), observed)
    }
}

impl Signed32SelectiveNxMapping {
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.inner.reserved_range()
    }
    pub fn span(&self) -> usize {
        self.inner.span()
    }
    pub fn page_count(&self) -> usize {
        self.inner.page_count()
    }
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), Error> {
        self.inner.read_bytes(offset, out)
    }
    pub fn read_only_pages(&self) -> &[bool] {
        &self.read_only
    }

    #[cfg(ktest)]
    pub fn audit_for_test(&self) -> Result<Signed32LeaseAudit, Error> {
        let guard = irq::disable_local();
        if num_cpus() != 1 {
            return Err(Error::AccessDenied);
        }
        audit_mask(&self.inner, &self.read_only, &guard)
    }
}

fn audit_mask(
    lease: &Signed32RwMapping,
    mask: &[bool],
    guard: &DisabledLocalIrqGuard,
) -> Result<Signed32LeaseAudit, Error> {
    let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
    let range = lease.reserved_range();
    let mut cursor = table.cursor(guard, &range)?;
    let mut audit = Signed32LeaseAudit::default();
    for (i, (expected, &ro)) in lease
        .owners
        .staging
        .frames_for_signed32()
        .zip(mask)
        .enumerate()
    {
        cursor.jump(range.start + i * PAGE_SIZE)?;
        let Some(MappedItemRef::Tracked(actual, p)) = cursor.query()?.1 else {
            return Err(Error::AccessDenied);
        };
        audit.pte_pages += 1;
        audit.frame_matches += usize::from(actual.paddr() == expected.paddr());
        audit.rw_nonx += usize::from(
            p.flags.contains(crate::mm::page_prop::PageFlags::RW)
                && !p.flags.contains(crate::mm::page_prop::PageFlags::X),
        );
        audit.ro_nonx += usize::from(
            p.flags.contains(crate::mm::page_prop::PageFlags::R)
                && !p.flags.contains(crate::mm::page_prop::PageFlags::W)
                && !p.flags.contains(crate::mm::page_prop::PageFlags::X),
        );
        audit.supervisor += usize::from(
            !p.priv_flags
                .contains(crate::mm::page_prop::PrivilegedPageFlags::USER),
        );
        audit.writeback += usize::from(p.cache == crate::mm::page_prop::CachePolicy::Writeback);
        audit.global += usize::from(
            p.priv_flags
                .contains(crate::mm::page_prop::PrivilegedPageFlags::GLOBAL),
        );
        if actual.paddr() != expected.paddr() || !valid_property(p, ro) {
            return Err(Error::AccessDenied);
        }
    }
    for va in (lease.installed.end..range.end).step_by(PAGE_SIZE) {
        cursor.jump(va)?;
        if cursor.query()?.1.is_some() {
            return Err(Error::AccessDenied);
        }
        audit.slack_absent += 1;
    }
    Ok(audit)
}

fn mask_observed(
    lease: &Signed32RwMapping,
    mask: &[bool],
    protected_true_count: usize,
    guard: &DisabledLocalIrqGuard,
) -> Result<usize, Error> {
    let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
    let mut cursor = table.cursor(guard, &lease.installed)?;
    let mut protected = 0;
    let mut true_seen = 0;
    for (i, (frame, &ro)) in lease
        .owners
        .staging
        .frames_for_signed32()
        .zip(mask)
        .enumerate()
    {
        cursor.jump(lease.installed.start + i * PAGE_SIZE)?;
        let Some(MappedItemRef::Tracked(actual, p)) = cursor.query()?.1 else {
            return Err(Error::AccessDenied);
        };
        let expected_ro = ro && true_seen < protected_true_count;
        if actual.paddr() != frame.paddr() || !valid_property(p, expected_ro) {
            return Err(Error::AccessDenied);
        }
        true_seen += usize::from(ro);
        protected += usize::from(expected_ro);
    }
    Ok(protected)
}

fn transition(
    lease: Signed32RwMapping,
    mask: &[bool],
    fault: Option<usize>,
    observed: &mut usize,
) -> Result<Signed32SelectiveNxMapping, Error> {
    if mask.len() != lease.page_count() {
        return Err(Error::InvalidArgs);
    }
    // The persistent bookkeeping allocation intentionally remains outside Result
    // (allocator OOM follows the kernel's existing allocation policy).
    let owned_mask = mask.to_vec();
    let guard = irq::disable_local();
    if num_cpus() != 1 {
        return Err(Error::AccessDenied);
    }
    // Validate identity, slack and all original flags before the first protection.
    lease.audit_expected(&guard, false)?;
    let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
    let mut changed = 0;
    if fault == Some(0) {
        *observed = mask_observed(&lease, mask, 0, &guard)?;
        return Err(Error::IoError);
    }
    for (i, (&ro, expected)) in mask
        .iter()
        .zip(lease.owners.staging.frames_for_signed32())
        .enumerate()
    {
        if !ro {
            continue;
        }
        let va = lease.installed.start + i * PAGE_SIZE;
        {
            let slot = va..va + PAGE_SIZE;
            let mut cursor = table.cursor_mut(&guard, &slot)?;
            let Some(MappedItemRef::Tracked(actual, p)) = cursor.query()?.1 else {
                return Err(Error::AccessDenied);
            };
            if actual.paddr() != expected.paddr() || !valid_property(p, false) {
                return Err(Error::AccessDenied);
            }
            // SAFETY: exact retained-frame PTE; deleting W preserves all other
            // attributes. IRQs disabled on the sole CPU; cleanup remains in lease.
            let protected = unsafe {
                cursor.protect_next(PAGE_SIZE, &mut |prop| {
                    prop.flags.remove(crate::mm::page_prop::PageFlags::W)
                })
            }
            .ok_or(Error::AccessDenied)?;
            if protected != slot {
                return Err(Error::AccessDenied);
            }
        }
        changed += 1;
        if fault == Some(changed) {
            *observed = mask_observed(&lease, mask, changed, &guard)?;
            return Err(Error::IoError);
        }
    }
    tlb_flush_addr_range(&lease.installed);
    audit_mask(&lease, mask, &guard)?;
    Ok(Signed32SelectiveNxMapping {
        inner: lease,
        read_only: owned_mask,
    })
}
