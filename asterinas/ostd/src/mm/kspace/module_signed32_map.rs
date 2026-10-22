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
        Frame, HasPaddr, PAGE_SIZE, Vaddr, VmIo, VmReader, VmWriter,
        module_address::NearTextAddressReservation,
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags},
    },
};

const MAX_SCOPED_PAGES: usize = 1024;

struct ScopedMapping<'a> {
    irq_guard: DisabledLocalIrqGuard,
    _reservation: &'a NearTextAddressReservation,
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

fn assert_unmapped(reservation: &NearTextAddressReservation) {
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
            "signed32 scope must leave reservation unmapped"
        );
    }
}

/// Test-only temporary read mapping for the separate signed-32 arena.
pub(crate) fn read_scoped(
    reservation: &NearTextAddressReservation,
    span: usize,
    frames: impl ExactSizeIterator<Item = Frame<()>> + Clone,
    offset: usize,
    out: &mut [u8],
) -> Result<(), Error> {
    read_scoped_inner(reservation, span, frames, offset, out, None, &mut 0)
}

fn read_scoped_inner(
    reservation: &NearTextAddressReservation,
    span: usize,
    frames: impl ExactSizeIterator<Item = Frame<()>> + Clone,
    offset: usize,
    out: &mut [u8],
    fault_after: Option<usize>,
    observed_installed: &mut usize,
) -> Result<(), Error> {
    let end = offset.checked_add(out.len()).ok_or(Error::Overflow)?;
    let range = reservation.range();
    let arena = NearTextAddressReservation::signed32_arena_range();
    if range.start < arena.start
        || range.end > arena.end
        || range.start >= range.end
        || span == 0
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
            return Err(Error::AccessDenied);
        }
        cursor.jump(range.start)?;
        let prop = PageProperty {
            flags: PageFlags::RW,
            cache: CachePolicy::Writeback,
            priv_flags: PrivilegedPageFlags::GLOBAL,
        };
        macro_rules! audit_fault_prefix {
            ($pages:expr) => {{
                let mut observed = 0;
                for (page, expected_frame) in expected_frames.clone().take($pages).enumerate() {
                    cursor.jump(range.start + page * PAGE_SIZE)?;
                    let Some(MappedItemRef::Tracked(actual, actual_prop)) = cursor.query()?.1
                    else {
                        panic!("injected fault prefix PTE missing");
                    };
                    assert_eq!(actual.paddr(), expected_frame.paddr());
                    let mut expected_prop = prop;
                    expected_prop.priv_flags |= PrivilegedPageFlags::AVAIL1;
                    assert_eq!(actual_prop, expected_prop);
                    observed += 1;
                }
                assert_eq!(
                    observed, $pages,
                    "signed32 fault must follow expected installed prefix"
                );
                for address in (range.start + observed * PAGE_SIZE..range.end).step_by(PAGE_SIZE) {
                    cursor.jump(address)?;
                    assert!(
                        cursor.query()?.1.is_none(),
                        "fault checkpoint suffix/slack must be unmapped"
                    );
                }
                *observed_installed = observed;
            }};
        }
        if fault_after == Some(0) {
            audit_fault_prefix!(0);
            return Err(Error::IoError);
        }
        for (index, frame) in frames.enumerate() {
            if scope.installed.end >= range.start + span {
                return Err(Error::InvalidArgs);
            }
            scope.installed.end += PAGE_SIZE;
            // SAFETY: owned empty kernel VA, valid retained private frames,
            // supervisor WB/RW without X; no pointer or callback escapes.
            unsafe {
                cursor.map(MappedItem::Tracked(Frame::from_unsized(frame), prop));
            }
            if fault_after == Some(index + 1) {
                audit_fault_prefix!(index + 1);
                return Err(Error::IoError);
            }
        }
    }
    if scope.installed.end != range.start + span {
        return Err(Error::InvalidArgs);
    }
    tlb_flush_addr_range(&scope.installed);
    {
        let mut cursor = table.cursor(&scope.irq_guard, &range)?;
        for (index, frame) in expected_frames.enumerate() {
            cursor.jump(range.start + index * PAGE_SIZE)?;
            let Some(MappedItemRef::Tracked(actual, prop)) = cursor.query()?.1 else {
                panic!("expected tracked module mapping");
            };
            assert_eq!(actual.paddr(), frame.paddr());
            assert!(prop.flags.contains(PageFlags::RW));
            assert!(
                !prop.flags.contains(PageFlags::X),
                "signed32 mapping must stay non-executable"
            );
            assert!(!prop.priv_flags.contains(PrivilegedPageFlags::USER));
            assert_eq!(prop.cache, CachePolicy::Writeback);
            assert!(prop.priv_flags.contains(PrivilegedPageFlags::GLOBAL));
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
    drop(scope);
    assert_unmapped(reservation);
    Ok(())
}

#[cfg(ktest)]
mod fault_runtime;
