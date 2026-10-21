// SPDX-License-Identifier: MPL-2.0

//! Owned x86-64 VMALLOC reservation for future module placement.
//!
//! This holds virtual addresses only: no payload frames, page-table mappings,
//! relocations, execution, permissions or TLB changes. The high base may not fit
//! a module's relocations. Replan its layout at this base before preparing it;
//! alignment gaps can change the required span. Existing linear aliases remain.

#![forbid(unsafe_code)]

use core::ops::Range;

use crate::{
    Error,
    mm::{PAGE_SIZE, Vaddr, kspace::kvirt_area::KVirtReservation},
};

#[cfg(ktest)]
mod runtime_tests;

/// Unique owner of a nonempty, page-aligned, unmapped virtual address range.
///
/// Not Clone. Keep it alive while depending on its addresses; Drop returns the
/// range to the shared VMALLOC allocator. There is no conversion to a pointer
/// or executable memory, and no API that installs pages in this reservation.
#[derive(Debug)]
pub struct ModuleAddressReservation {
    reservation: KVirtReservation,
}

impl ModuleAddressReservation {
    /// Reserves exactly `span` bytes with a budget of `max_pages` virtual pages.
    ///
    /// `span` must be nonzero and a multiple of PAGE_SIZE, with every gap and
    /// rounded tail counted. Invalid input returns InvalidArgs before reserving
    /// anything; insufficient contiguous virtual space returns NoMemory.
    /// Requires initialized OSTD/heap. The shared allocator's BTreeMap bookkeeping
    /// is not fallible under heap OOM. This budget limits neither heap nor RAM.
    pub fn new(span: usize, max_pages: usize) -> Result<Self, Error> {
        if span == 0 || !span.is_multiple_of(PAGE_SIZE) || span / PAGE_SIZE > max_pages {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            reservation: KVirtReservation::new(span)?,
        })
    }

    /// Real kernel virtual addresses, not proof that any byte is mapped.
    pub fn range(&self) -> Range<Vaddr> {
        self.reservation.range()
    }

    /// Reserved virtual span, including gaps if included by the caller.
    pub fn span(&self) -> usize {
        self.range().len()
    }

    /// Virtual pages only, not allocated payload frames.
    pub fn page_count(&self) -> usize {
        self.span() / PAGE_SIZE
    }
}
