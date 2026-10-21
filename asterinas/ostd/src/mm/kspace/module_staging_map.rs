// SPDX-License-Identifier: MPL-2.0

//! Single-CPU ktest prototype only. No production mapping or execution API.
//! This deliberately refuses SMP: local INVLPG is not a remote shootdown.

use core::ops::Range;

use super::{KERNEL_PAGE_TABLE, MappedItem, MappedItemRef};
use crate::{
    Error,
    arch::mm::tlb_flush_addr_range,
    cpu::num_cpus,
    irq::{self, DisabledLocalIrqGuard},
    mm::{
        Frame, FrameAllocOptions, HasPaddr, PAGE_SIZE, Vaddr, VmIo, VmReader, VmWriter,
        module_address::ModuleAddressReservation,
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags},
    },
};

// Bound the number of pages handled with local IRQs disabled, not all heap use.
const MAX_SCOPED_PAGES: usize = 1024;

struct ScopedMapping<'a> {
    irq_guard: DisabledLocalIrqGuard,
    _reservation: &'a ModuleAddressReservation,
    installed: Range<Vaddr>,
}

impl Drop for ScopedMapping<'_> {
    fn drop(&mut self) {
        if self.installed.is_empty() {
            return;
        }
        let table = KERNEL_PAGE_TABLE.get().unwrap();
        {
            let mut cursor = table.cursor_mut(&self.irq_guard, &self.installed).unwrap();
            while cursor.virt_addr() < self.installed.end {
                // SAFETY: only our preflighted, installed prefix is removed.
                // The caller retains all original staging frame handles for the
                // entire scope. No VA access occurs during cleanup. With one CPU,
                // no remote translation can outlive this scoped access.
                let Some(fragment) =
                    (unsafe { cursor.take_next(self.installed.end - cursor.virt_addr()) })
                else {
                    break;
                };
                drop(fragment);
            }
        }
        // Includes GLOBAL entries and runs before IRQs are restored or original
        // staging handles can be released. No whole-address-space CR3 shortcut.
        tlb_flush_addr_range(&self.installed);
    }
}

fn assert_unmapped(reservation: &ModuleAddressReservation) {
    let guard = irq::disable_local();
    let range = reservation.range();
    let mut cursor = KERNEL_PAGE_TABLE
        .get()
        .unwrap()
        .cursor(&guard, &range)
        .unwrap();
    for address in range.clone().step_by(PAGE_SIZE) {
        cursor.jump(address).unwrap();
        assert!(
            cursor.query().unwrap().1.is_none(),
            "scope must leave reservation unmapped"
        );
    }
}

/// Only called by the non-Clone staging owner with an exclusive borrow. The
/// iterator clones handles to its OWN private frames; originals remain alive.
/// This function and its caller exist only under cfg(ktest), x86-64.
pub(crate) fn read_scoped(
    reservation: &ModuleAddressReservation,
    span: usize,
    frames: impl ExactSizeIterator<Item = Frame<()>> + Clone,
    offset: usize,
    out: &mut [u8],
) -> Result<(), Error> {
    let end = offset.checked_add(out.len()).ok_or(Error::Overflow)?;
    let range = reservation.range();
    if span == 0
        || !span.is_multiple_of(PAGE_SIZE)
        || span > range.len()
        || span / PAGE_SIZE != frames.len()
        || frames.len() > MAX_SCOPED_PAGES
        || range.len() / PAGE_SIZE > MAX_SCOPED_PAGES
        || end > span
    {
        return Err(Error::InvalidArgs);
    }
    let mut scope = ScopedMapping {
        irq_guard: irq::disable_local(),
        _reservation: reservation,
        installed: range.start..range.start,
    };
    // The boot topology is initialized before ktests. No CPU hotplug is supported
    // by this prototype; do NOT relax this check after a single-CPU pass.
    if num_cpus() != 1 {
        return Err(Error::AccessDenied);
    }
    if out.is_empty() {
        return Ok(());
    }
    let table = KERNEL_PAGE_TABLE.get().ok_or(Error::AccessDenied)?;
    let expected_frames = frames.clone();
    {
        let mut cursor = table.cursor_mut(&scope.irq_guard, &range)?;
        if cursor.find_next(range.len()).is_some() {
            // No prefix installed yet: Drop must not remove this foreign item.
            return Err(Error::AccessDenied);
        }
        cursor.jump(range.start)?;
        let prop = PageProperty {
            flags: PageFlags::RW,
            cache: CachePolicy::Writeback,
            priv_flags: PrivilegedPageFlags::GLOBAL,
        };
        for frame in frames {
            if scope.installed.end >= range.start + span {
                return Err(Error::InvalidArgs);
            }
            // Include this already-preflighted slot in rollback even if a
            // lower-level panic occurs after installing the PTE. The cursor is
            // declared after scope, so its locks drop before scope's cleanup.
            scope.installed.end += PAGE_SIZE;
            // SAFETY: owned empty kernel VA, valid retained private frames,
            // supervisor WB/RW without X; no pointer or callback escapes.
            unsafe {
                cursor.map(MappedItem::Tracked(Frame::from_unsized(frame), prop));
            }
        }
    }
    if scope.installed.end != range.start + span {
        return Err(Error::InvalidArgs);
    }
    tlb_flush_addr_range(&scope.installed);
    // Mandatory ktest assertions inspect REAL PTEs while this mapping is live.
    {
        let mut cursor = table.cursor(&scope.irq_guard, &range)?;
        for (index, frame) in expected_frames.enumerate() {
            cursor.jump(range.start + index * PAGE_SIZE)?;
            let Some(MappedItemRef::Tracked(actual, prop)) = cursor.query()?.1 else {
                panic!("expected tracked module mapping");
            };
            assert_eq!(actual.paddr(), frame.paddr());
            assert!(prop.flags.contains(PageFlags::RW));
            assert!(!prop.flags.contains(PageFlags::X));
            assert!(!prop.priv_flags.contains(PrivilegedPageFlags::USER));
            assert_eq!(prop.cache, CachePolicy::Writeback);
        }
        for address in (range.start + span..range.end).step_by(PAGE_SIZE) {
            cursor.jump(address)?;
            assert!(
                cursor.query()?.1.is_none(),
                "envelope slack must stay unmapped"
            );
        }
    }
    {
        // SAFETY: validated offset/length lie within our installed, readable RAM
        // mapping, whose frames and VA stay owned. IRQ guard prevents migration;
        // local stale translations were invalidated. Reader cannot escape.
        let mut reader =
            unsafe { VmReader::from_kernel_space((range.start + offset) as *const u8, out.len()) };
        assert_eq!(reader.read(&mut VmWriter::from(out)), end - offset);
    }
    drop(scope); // unmap + local INVLPG before the caller can regain its owner
    assert_unmapped(reservation);
    Ok(())
}

/// Test-only adversarial setup: retain a foreign frame, seed a GLOBAL TLB entry,
/// remove its PTE without flushing, then require read_scoped to replace the
/// translation. The foreign frame stays alive until the final local flush.
pub(crate) fn probe_stale_read(
    reservation: &ModuleAddressReservation,
    span: usize,
    frames: impl ExactSizeIterator<Item = Frame<()>> + Clone,
    out: &mut [u8],
) -> Result<(), Error> {
    if num_cpus() != 1 || out.len() != 8 {
        return Err(Error::AccessDenied);
    }
    assert_unmapped(reservation);
    let foreign = FrameAllocOptions::new().zeroed(true).alloc_frame()?;
    foreign.write_bytes(0, &[0xe7; 8])?;
    let range = reservation.range();
    let fake = ScopedMapping {
        irq_guard: irq::disable_local(),
        _reservation: reservation,
        installed: range.start..range.start + PAGE_SIZE,
    };
    let table = KERNEL_PAGE_TABLE.get().unwrap();
    {
        let mut cursor = table.cursor_mut(&fake.irq_guard, &fake.installed)?;
        let prop = PageProperty {
            flags: PageFlags::RW,
            cache: CachePolicy::Writeback,
            priv_flags: PrivilegedPageFlags::GLOBAL,
        };
        // SAFETY: test owns this empty reserved slot and retains the foreign
        // frame. Fake scope unmaps/flushes before releasing that frame.
        unsafe {
            cursor.map(MappedItem::Tracked(
                Frame::from_unsized(foreign.clone()),
                prop,
            ));
        }
    }
    out.fill(0xa5);
    assert_eq!(
        read_scoped(reservation, span, frames.clone(), 0, out),
        Err(Error::AccessDenied)
    );
    assert_eq!(
        out, &[0xa5; 8],
        "collision must not copy or alter the caller buffer"
    );
    {
        let mut cursor = table.cursor(&fake.irq_guard, &fake.installed)?;
        let Some(MappedItemRef::Tracked(actual, _)) = cursor.query()?.1 else {
            panic!("foreign collision must remain mapped");
        };
        assert_eq!(actual.paddr(), foreign.paddr());
    }
    tlb_flush_addr_range(&fake.installed);
    let mut seed = [0; 8];
    {
        // SAFETY: the test owns a live readable foreign mapping and frame;
        // IRQs remain disabled and its GLOBAL translation was invalidated.
        let mut reader =
            unsafe { VmReader::from_kernel_space(range.start as *const u8, seed.len()) };
        assert_eq!(reader.read(&mut VmWriter::from(&mut seed[..])), seed.len());
    }
    assert_eq!(seed, [0xe7; 8]);
    {
        let mut cursor = table.cursor_mut(&fake.irq_guard, &fake.installed)?;
        // SAFETY: remove only the test's slot. No VA access until read_scoped's
        // invalidation; the foreign frame remains retained even on an error.
        drop(unsafe { cursor.take_next(PAGE_SIZE) }.expect("seed mapping exists"));
    }
    // Deliberately no INVLPG here. read_scoped must invalidate this stale entry
    // before copying through the same real VA to the original staging frames.
    let result = read_scoped(reservation, span, frames, 0, out);
    drop(fake); // also flushes on an early error before foreign can be released
    assert_unmapped(reservation);
    result
}
