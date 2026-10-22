// SPDX-License-Identifier: MPL-2.0

//! Owned reservation in the x86 near-kernel-text virtual-address pool.
//!
//! This reserves virtual addresses only. It does not install PTEs, allocate
//! frames, or change permissions or TLB state. The distinct owner type is
//! intentionally not interchangeable with a VMALLOC module reservation.

#![forbid(unsafe_code)]

use core::ops::Range;

use crate::{
    Error,
    boot::memory_region::MemoryRegionType,
    irq,
    mm::{PAGE_SIZE, Vaddr},
    util::range_alloc::RangeAllocator,
};

static NEAR_TEXT_ALLOCATOR: RangeAllocator =
    RangeAllocator::new(crate::mm::kspace::MODULE_NEAR_TEXT_VADDR_RANGE);

const MAX_CLAIM_PAGES: usize = 1024;

/// Unique owner of an unmapped, page-aligned range near kernel text.
///
/// The type is deliberately distinct from [`super::ModuleAddressReservation`]:
/// it cannot be cloned or converted to a VMALLOC reservation. Keep it alive
/// while depending on its addresses; dropping it releases the exact range.
#[derive(Debug)]
pub struct NearTextAddressReservation {
    range: Range<Vaddr>,
}

impl NearTextAddressReservation {
    /// Reserves exactly `span` bytes at `base` from the near-text pool.
    ///
    /// Both values must be page aligned, `span` must be nonzero, and the checked
    /// range must fit wholly in the pool. The caller's page budget must cover
    /// the range; no claim may exceed 1024 pages, keeping each IRQ-disabled
    /// page-table audit bounded. Invalid arguments return `InvalidArgs`; an
    /// occupied range returns `NoMemory`; boot-layout conflicts, unavailable
    /// page tables, or existing mappings return `AccessDenied`.
    ///
    /// No mapping, PTE, frame, permission, or TLB is changed. This does not
    /// establish relocation suitability or canonicality independently of the
    /// architecture's configured address width. The range allocator's
    /// bookkeeping may allocate from the heap and has no fallible heap-OOM
    /// guarantee, so `max_pages` bounds the address-space audit, not heap work.
    /// Requires initialized OSTD heap, boot information and global page table.
    /// Page-table writers must respect this dedicated pool's ownership. An
    /// absent PTE does not prove absent stale TLB entries; no VA access is exposed.
    pub fn new_at(base: Vaddr, span: usize, max_pages: usize) -> Result<Self, Error> {
        let range = validate_layout(base, span, max_pages)?;

        // Reserve exactly the requested interval. Keep interrupts disabled for
        // both allocator mutation and the subsequent bounded mapping audit.
        let irq_guard = irq::disable_local();
        NEAR_TEXT_ALLOCATOR
            .alloc_specific(&range)
            .map_err(|_| Error::NoMemory)?;
        let owner = Self { range };

        let page_table = crate::mm::kspace::KERNEL_PAGE_TABLE
            .get()
            .ok_or(Error::AccessDenied)?;
        let mut cursor = page_table
            .cursor(&irq_guard, &owner.range)
            .map_err(|_| Error::AccessDenied)?;
        if cursor.find_next(owner.range.len()).is_some() {
            return Err(Error::AccessDenied);
        }
        drop(cursor);
        drop(irq_guard);
        Ok(owner)
    }

    /// Declared dedicated pool, not an ownership claim or proof of availability.
    pub fn arena_range() -> Range<Vaddr> {
        crate::mm::kspace::MODULE_NEAR_TEXT_VADDR_RANGE
    }

    /// Returns the exact reserved virtual-address interval.
    pub fn range(&self) -> Range<Vaddr> {
        self.range.clone()
    }

    /// Returns the exact reserved span in bytes.
    pub fn span(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Returns the number of base pages reserved.
    pub fn page_count(&self) -> usize {
        self.span() / PAGE_SIZE
    }
}

impl Drop for NearTextAddressReservation {
    fn drop(&mut self) {
        let _irq_guard = irq::disable_local();
        NEAR_TEXT_ALLOCATOR.free(self.range.clone());
    }
}

fn validate_layout(base: Vaddr, span: usize, max_pages: usize) -> Result<Range<Vaddr>, Error> {
    if span == 0
        || !base.is_multiple_of(PAGE_SIZE)
        || !span.is_multiple_of(PAGE_SIZE)
        || span / PAGE_SIZE > max_pages
        || span / PAGE_SIZE > MAX_CLAIM_PAGES
    {
        return Err(Error::InvalidArgs);
    }
    let end = base.checked_add(span).ok_or(Error::InvalidArgs)?;
    let range = base..end;
    let pool = crate::mm::kspace::MODULE_NEAR_TEXT_VADDR_RANGE;
    if !pool_contains(&pool, &range) {
        return Err(Error::InvalidArgs);
    }
    if !crate::mm::kspace::KERNEL_VADDR_RANGE.contains(&pool.start)
        || pool.end > crate::mm::kspace::KERNEL_VADDR_RANGE.end
        || !ranges_disjoint(&pool, &crate::mm::kspace::LINEAR_MAPPING_VADDR_RANGE)
        || !ranges_disjoint(&pool, &crate::mm::kspace::VMALLOC_VADDR_RANGE)
        || !ranges_disjoint(&pool, &crate::mm::kspace::FRAME_METADATA_RANGE)
    {
        return Err(Error::AccessDenied);
    }

    let early_info = crate::boot::EARLY_INFO.get().ok_or(Error::AccessDenied)?;
    let offset = crate::mm::kspace::kernel_loaded_offset();
    let mut found_kernel = false;
    for region in early_info.memory_regions.iter() {
        if region.typ() != MemoryRegionType::Kernel {
            continue;
        }
        found_kernel = true;
        let physical_end = region
            .base()
            .checked_add(region.len())
            .ok_or(Error::AccessDenied)?;
        let virtual_start = region
            .base()
            .checked_add(offset)
            .ok_or(Error::AccessDenied)?;
        let virtual_end = physical_end
            .checked_add(offset)
            .ok_or(Error::AccessDenied)?;
        if virtual_start >= virtual_end || !ranges_disjoint(&pool, &(virtual_start..virtual_end)) {
            return Err(Error::AccessDenied);
        }
    }
    if !found_kernel {
        return Err(Error::AccessDenied);
    }
    Ok(range)
}

fn pool_contains(pool: &Range<Vaddr>, requested: &Range<Vaddr>) -> bool {
    pool.start < pool.end && requested.start >= pool.start && requested.end <= pool.end
}

fn ranges_disjoint(left: &Range<Vaddr>, right: &Range<Vaddr>) -> bool {
    left.end <= right.start || right.end <= left.start
}
