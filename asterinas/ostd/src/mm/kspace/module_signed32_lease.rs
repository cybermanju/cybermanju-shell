// SPDX-License-Identifier: MPL-2.0
//! Experimental persistent single-CPU data mapping, not a Linux execution API.
//! Page-table bookkeeping OOM/panics remain outside Result. Unexpected cleanup
//! failure or topology change leaks the retained resources rather than reusing
//! frames/VA while a translation might survive. No CPU hotplug is supported.
use super::{KERNEL_PAGE_TABLE, MappedItem, MappedItemRef};
use crate::{
    Error,
    arch::mm::tlb_flush_addr_range,
    cpu::num_cpus,
    irq::{self, DisabledLocalIrqGuard},
    mm::{
        HasPaddr, PAGE_SIZE, Vaddr, VmReader, VmWriter,
        module_address::NearTextAddressReservation,
        module_staging::ModuleStaging,
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags},
    },
};
use core::{mem::ManuallyDrop, ops::Range};
const MAX_PAGES: usize = 1024;

/// Observed mapping counts; not evidence of physical W^X or remote coherence.
#[derive(Default)]
pub struct Signed32LeaseAudit {
    pub pte_pages: usize,
    pub frame_matches: usize,
    pub rw_nonx: usize,
    pub supervisor: usize,
    pub writeback: usize,
    pub global: usize,
    pub slack_absent: usize,
    pub ro_nonx: usize,
}
// Declaration order releases original frame references before the VA owner.
struct Owners {
    staging: ModuleStaging,
    reservation: NearTextAddressReservation,
}

/// Experimental persistent supervisor/WB/RW/non-X alias on a fixed one-CPU boot.
/// No raw pointer, write or execution capability is exposed. The existing linear
/// RW alias remains; this does not certify physical W^X or production readiness.
/// Unlike a scoped IRQ guard, this owner may live across ordinary API calls.
pub struct Signed32RwMapping {
    owners: ManuallyDrop<Owners>,
    installed: Range<Vaddr>,
}
fn valid_property(p: PageProperty, read_only: bool) -> bool {
    p.flags.contains(PageFlags::R)
        && (if read_only {
            !p.flags.contains(PageFlags::W)
        } else {
            p.flags.contains(PageFlags::W)
        })
        && !p.flags.contains(PageFlags::X)
        && !p.priv_flags.contains(PrivilegedPageFlags::USER)
        && p.cache == CachePolicy::Writeback
        && p.priv_flags.contains(PrivilegedPageFlags::GLOBAL)
}
impl Signed32RwMapping {
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.owners.reservation.range()
    }
    pub fn span(&self) -> usize {
        self.owners.staging.span()
    }
    pub fn page_count(&self) -> usize {
        self.owners.staging.page_count()
    }

    fn audit_expected(
        &self,
        guard: &DisabledLocalIrqGuard,
        read_only: bool,
    ) -> Result<Signed32LeaseAudit, Error> {
        let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
        let range = self.reserved_range();
        let mut cursor = table.cursor(guard, &range)?;
        let mut audit = Signed32LeaseAudit::default();
        for (index, expected) in self.owners.staging.frames_for_signed32().enumerate() {
            cursor.jump(range.start + index * PAGE_SIZE)?;
            let Some(MappedItemRef::Tracked(actual, p)) = cursor.query()?.1 else {
                return Err(Error::AccessDenied);
            };
            audit.pte_pages += 1;
            audit.frame_matches += usize::from(actual.paddr() == expected.paddr());
            audit.rw_nonx +=
                usize::from(p.flags.contains(PageFlags::RW) && !p.flags.contains(PageFlags::X));
            audit.ro_nonx += usize::from(
                p.flags.contains(PageFlags::R)
                    && !p.flags.contains(PageFlags::W)
                    && !p.flags.contains(PageFlags::X),
            );
            audit.supervisor += usize::from(!p.priv_flags.contains(PrivilegedPageFlags::USER));
            audit.writeback += usize::from(p.cache == CachePolicy::Writeback);
            audit.global += usize::from(p.priv_flags.contains(PrivilegedPageFlags::GLOBAL));
            if actual.paddr() != expected.paddr() || !valid_property(p, read_only) {
                return Err(Error::AccessDenied);
            }
        }
        for va in (self.installed.end..range.end).step_by(PAGE_SIZE) {
            cursor.jump(va)?;
            if cursor.query()?.1.is_some() {
                return Err(Error::AccessDenied);
            }
            audit.slack_absent += 1;
        }
        Ok(audit)
    }
    #[cfg(ktest)]
    pub fn audit_for_test(&self) -> Result<Signed32LeaseAudit, Error> {
        let guard = irq::disable_local();
        if num_cpus() != 1 {
            return Err(Error::AccessDenied);
        }
        self.audit_expected(&guard, false)
    }
    /// Read through the reserved alias, with checked bounds and a per-call IRQ
    /// guard. The output is untouched on validation/topology errors.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), Error> {
        let end = offset.checked_add(out.len()).ok_or(Error::Overflow)?;
        if end > self.span() {
            return Err(Error::InvalidArgs);
        }
        let _guard = irq::disable_local();
        if num_cpus() != 1 {
            return Err(Error::AccessDenied);
        }
        if out.is_empty() {
            return Ok(());
        }
        let len = out.len();
        // SAFETY: validated readable RAM owned for this entire borrow; no pointer
        // escapes and the sole CPU cannot migrate while this local guard lives.
        let mut reader = unsafe {
            VmReader::from_kernel_space((self.installed.start + offset) as *const u8, len)
        };
        assert_eq!(reader.read(&mut VmWriter::from(out)), len);
        Ok(())
    }
    #[cfg(ktest)]
    pub(crate) fn audit_ro_for_test(&self) -> Result<Signed32LeaseAudit, Error> {
        let guard = irq::disable_local();
        if num_cpus() != 1 {
            return Err(Error::AccessDenied);
        }
        self.audit_expected(&guard, true)
    }
    /// Consume this RW lease and remove W from each owned alias PTE, retaining NX.
    /// Errors consume the owner and run its unmap/flush cleanup. Only fixed one-CPU
    /// boots are supported. The linear RW alias remains: data is not immutable,
    /// physical W^X is not established, and no execution capability is returned.
    pub fn into_read_only_nx(self) -> Result<Signed32RoMapping, Error> {
        transition(self, None, &mut 0)
    }
    #[cfg(ktest)]
    pub fn into_read_only_nx_with_fault_for_test(
        self,
        after: usize,
        observed: &mut usize,
    ) -> Result<Signed32RoMapping, Error> {
        transition(self, Some(after), observed)
    }
    fn cleanup(&self, guard: &DisabledLocalIrqGuard) -> Result<(), Error> {
        if self.installed.is_empty() {
            return Ok(());
        }
        let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
        {
            let mut cursor = table.cursor_mut(guard, &self.installed)?;
            for (va, expected) in self
                .installed
                .clone()
                .step_by(PAGE_SIZE)
                .zip(self.owners.staging.frames_for_signed32())
            {
                cursor.jump(va)?;
                match cursor.query()?.1 {
                    None => continue, // map may have panicked before completing this slot
                    Some(MappedItemRef::Tracked(actual, _))
                        if actual.paddr() == expected.paddr() => {}
                    _ => return Err(Error::AccessDenied), // never remove an unexpected foreign entry
                }
                // SAFETY: this exact slot is ours and originals remain retained
                // even if removal/flush/audit unexpectedly errors or panics.
                drop(unsafe { cursor.take_next(PAGE_SIZE) }.ok_or(Error::AccessDenied)?);
            }
        }
        tlb_flush_addr_range(&self.installed);
        let mut cursor = table.cursor(guard, &self.installed)?;
        for va in self.installed.clone().step_by(PAGE_SIZE) {
            cursor.jump(va)?;
            if cursor.query()?.1.is_some() {
                return Err(Error::AccessDenied);
            }
        }
        Ok(())
    }
}
impl Drop for Signed32RwMapping {
    fn drop(&mut self) {
        let guard = irq::disable_local();
        if num_cpus() != 1 || self.cleanup(&guard).is_err() {
            return;
        }
        // SAFETY: unmap + GLOBAL-inclusive local flush + absence audit completed
        // on the sole CPU. Taking once suppresses automatic drop on every failed
        // cleanup path. Owners drops staging references before the reservation.
        let owners = unsafe { ManuallyDrop::take(&mut self.owners) };
        drop(owners);
    }
}
pub(crate) fn new(
    stage: ModuleStaging,
    reservation: NearTextAddressReservation,
) -> Result<Signed32RwMapping, Error> {
    new_inner(stage, reservation, None, &mut 0)
}
#[cfg(ktest)]
pub(crate) fn new_with_fault(
    stage: ModuleStaging,
    reservation: NearTextAddressReservation,
    fault: Option<usize>,
    observed: &mut usize,
) -> Result<Signed32RwMapping, Error> {
    new_inner(stage, reservation, fault, observed)
}
fn new_inner(
    stage: ModuleStaging,
    reservation: NearTextAddressReservation,
    fault: Option<usize>,
    observed: &mut usize,
) -> Result<Signed32RwMapping, Error> {
    let owners = Owners {
        staging: stage,
        reservation,
    };
    let range = owners.reservation.range();
    let arena = NearTextAddressReservation::signed32_arena_range();
    let span = owners.staging.span();
    if range.start < arena.start
        || range.end > arena.end
        || range.start >= range.end
        || span == 0
        || !span.is_multiple_of(PAGE_SIZE)
        || span > range.len()
        || span / PAGE_SIZE != owners.staging.page_count()
        || owners.staging.page_count() > MAX_PAGES
        || range.len() / PAGE_SIZE > MAX_PAGES
    {
        return Err(Error::InvalidArgs);
    }
    let guard = irq::disable_local();
    if num_cpus() != 1 {
        return Err(Error::AccessDenied);
    }
    let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
    // Early rejection has no own PTEs: ordinary Owners drop returns frames/VA
    // without touching a foreign mapping. IRQs stay disabled across both phases.
    {
        let mut cursor = table.cursor(&guard, &range)?;
        for va in range.clone().step_by(PAGE_SIZE) {
            cursor.jump(va)?;
            if cursor.query()?.1.is_some() {
                return Err(Error::AccessDenied);
            }
        }
    }
    let mut lease = Signed32RwMapping {
        owners: ManuallyDrop::new(owners),
        installed: range.start..range.start,
    };
    let prop = PageProperty {
        flags: PageFlags::RW,
        cache: CachePolicy::Writeback,
        priv_flags: PrivilegedPageFlags::GLOBAL,
    };
    {
        let mut cursor = table.cursor_mut(&guard, &range)?;
        macro_rules! fault_checkpoint {
            ($count:expr) => {{
                let mut count = 0;
                for (i, expected) in lease
                    .owners
                    .staging
                    .frames_for_signed32()
                    .take($count)
                    .enumerate()
                {
                    cursor.jump(range.start + i * PAGE_SIZE)?;
                    let Some(MappedItemRef::Tracked(actual, p)) = cursor.query()?.1 else {
                        return Err(Error::AccessDenied);
                    };
                    if actual.paddr() != expected.paddr() || !valid_property(p, false) {
                        return Err(Error::AccessDenied);
                    }
                    count += 1;
                }
                for va in (range.start + count * PAGE_SIZE..range.end).step_by(PAGE_SIZE) {
                    cursor.jump(va)?;
                    if cursor.query()?.1.is_some() {
                        return Err(Error::AccessDenied);
                    }
                }
                *observed = count;
            }};
        }
        if fault == Some(0) {
            fault_checkpoint!(0);
            return Err(Error::IoError);
        }
        for (i, frame) in lease.owners.staging.frames_for_signed32().enumerate() {
            cursor.jump(range.start + i * PAGE_SIZE)?;
            // Include this slot before map: a panic after partial installation
            // must still retain originals and attempt removal of the slot.
            lease.installed.end += PAGE_SIZE;
            // SAFETY: empty exclusively reserved VA and retained private frame,
            // sole CPU with IRQs disabled, supervisor WB/RW without X.
            unsafe {
                cursor.map(MappedItem::Tracked(
                    crate::mm::Frame::from_unsized(frame),
                    prop,
                ));
            }
            if fault == Some(i + 1) {
                fault_checkpoint!(i + 1);
                return Err(Error::IoError);
            }
        }
    }
    tlb_flush_addr_range(&lease.installed);
    lease.audit_expected(&guard, false)?;
    Ok(lease)
}
#[cfg(target_arch = "x86_64")]
mod readonly;
#[cfg(target_arch = "x86_64")]
pub use readonly::Signed32RoMapping;
#[cfg(target_arch = "x86_64")]
mod selective;
#[cfg(target_arch = "x86_64")]
pub use selective::Signed32SelectiveNxMapping;
#[cfg(ktest)]
mod readonly_runtime;
#[cfg(ktest)]
mod runtime;

// Fresh PTE observations, never a counter advanced merely by the protect loop.
fn read_only_prefix(
    lease: &Signed32RwMapping,
    guard: &DisabledLocalIrqGuard,
) -> Result<usize, Error> {
    let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
    let mut cursor = table.cursor(guard, &lease.installed)?;
    let mut prefix = 0;
    let mut rw_seen = false;
    for (i, frame) in lease.owners.staging.frames_for_signed32().enumerate() {
        cursor.jump(lease.installed.start + i * PAGE_SIZE)?;
        let Some(MappedItemRef::Tracked(actual, p)) = cursor.query()?.1 else {
            return Err(Error::AccessDenied);
        };
        if actual.paddr() != frame.paddr() {
            return Err(Error::AccessDenied);
        }
        if valid_property(p, true) && !rw_seen {
            prefix += 1;
        } else if valid_property(p, false) {
            rw_seen = true;
        } else {
            return Err(Error::AccessDenied);
        }
    }
    Ok(prefix)
}
fn transition(
    lease: Signed32RwMapping,
    fault: Option<usize>,
    observed: &mut usize,
) -> Result<Signed32RoMapping, Error> {
    let guard = irq::disable_local();
    if num_cpus() != 1 {
        return Err(Error::AccessDenied);
    }
    lease.audit_expected(&guard, false)?;
    let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
    if fault == Some(0) {
        *observed = read_only_prefix(&lease, &guard)?;
        return Err(Error::IoError);
    }
    for (i, expected) in lease.owners.staging.frames_for_signed32().enumerate() {
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
            // SAFETY: this tracked 4-KiB slot matches our retained frame. Removing
            // only W preserves R, NX, cache policy, GLOBAL, AVAIL1 and other flags.
            // Sole CPU, local IRQs disabled; no pointer escapes. On error, Drop
            // removes all owned slots and flushes before releasing frames/VA.
            let protected = unsafe {
                cursor.protect_next(PAGE_SIZE, &mut |prop| prop.flags.remove(PageFlags::W))
            }
            .ok_or(Error::AccessDenied)?;
            if protected != slot {
                return Err(Error::AccessDenied);
            }
        } // The mutable cursor must be gone before a fresh read cursor or Drop.
        if fault == Some(i + 1) {
            *observed = read_only_prefix(&lease, &guard)?;
            return Err(Error::IoError);
        }
    }
    tlb_flush_addr_range(&lease.installed);
    lease.audit_expected(&guard, true)?;
    Ok(readonly::wrap(lease))
}

#[cfg(ktest)]
mod selective_runtime;

#[cfg(ktest)]
mod write_fault_runtime;
