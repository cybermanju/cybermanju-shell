// SPDX-License-Identifier: MPL-2.0

//! Reserved address planning and coupled data-only staging, never mapping.

use core::ops::Range;

use linux_module_elf::{Layout, PreparedModule, page_layout_span_bound, plan_page_layout};
use ostd::mm::{
    PAGE_SIZE, Vaddr, module_address::ModuleAddressReservation,
    module_staging::ReservedModuleStaging,
};

use super::{Error, stage_prepared};

/// Holds a real VMALLOC reservation together with a layout replanned at its base.
///
/// The reservation may be larger than the layout because its alignment envelope
/// is conservative. These addresses are NOT mapped to staging frames. Keep this
/// owner alive when depending on them; cloned metadata does not retain ownership.
/// This neither prepares relocations nor guarantees they fit the high base.
pub struct ReservedModuleLayout {
    layout: Layout,
    reservation: ModuleAddressReservation,
}

impl ReservedModuleLayout {
    /// Validate and budget the envelope before reserving addresses, then replan.
    ///
    /// `max_pages` counts virtual pages including worst-case alignment gaps.
    /// No frames are allocated by this API. Metadata has separate heap overhead.
    /// Empty layouts and budgets that cannot represent their byte limit fail.
    /// Any replan error drops the reservation before returning; heap OOM in the
    /// existing VMALLOC bookkeeping is not recoverable through this Result.
    pub fn new(bytes: &[u8], max_pages: usize, max_sections: usize) -> Result<Self, Error> {
        if PAGE_SIZE != 4096 {
            return Err(Error::UnsupportedPageSize);
        }
        let max_span = max_pages
            .checked_mul(PAGE_SIZE)
            .ok_or(Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        let bound =
            page_layout_span_bound(bytes, max_span as u64, max_sections).map_err(Error::Plan)?;
        let span = usize::try_from(bound)
            .map_err(|_| Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        if span == 0 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }
        let reservation =
            ModuleAddressReservation::new(span, max_pages).map_err(Error::AddressSpace)?;
        let layout = plan_page_layout(bytes, reservation.range().start as u64, bound, max_sections)
            .map_err(Error::Plan)?;
        if layout.span() == 0 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }
        Ok(Self {
            layout,
            reservation,
        })
    }

    /// Layout at the owned base, not a mapping or permission certificate.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Entire owned virtual range, including conservative unused envelope space.
    pub fn range(&self) -> Range<Vaddr> {
        self.reservation.range()
    }

    /// Copy prepared bytes and consume the reservation into one coupled owner.
    ///
    /// Structural matching is not authentication of the input ELF. Any returned
    /// error drops this layout/reservation and any acquired frame handles. Heap
    /// OOM in existing allocator bookkeeping is outside this Result guarantee.
    /// This creates no mapping and does not make the prepared bytes executable.
    pub fn stage(
        self,
        module: &PreparedModule,
        max_pages: usize,
        max_chunks: usize,
    ) -> Result<ReservedPreparedModule, Error> {
        let staged = stage_prepared(module, &self.layout, max_pages, max_chunks)?;
        let storage = staged
            .with_reservation(self.reservation)
            .map_err(Error::Frames)?;
        Ok(ReservedPreparedModule {
            layout: self.layout,
            storage,
        })
    }
}

/// Data-only prepared storage with the virtual reservation kept alive by OSTD.
/// No Clone, into_parts, mutable handle, pointer or execution API is provided.
pub struct ReservedPreparedModule {
    layout: Layout,
    storage: ReservedModuleStaging,
}

impl ReservedPreparedModule {
    /// Metadata only; cloning it does not retain ownership of the reservation.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Entire reserved envelope, not a mapping to physical frames.
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.storage.reserved_range()
    }

    /// Actual staged span, excluding unused envelope slack.
    pub fn span(&self) -> usize {
        self.storage.span()
    }

    /// Staging pages, not the potentially larger virtual reservation.
    pub fn page_count(&self) -> usize {
        self.storage.page_count()
    }

    /// Reads the private frames; errors retain ModuleStaging's buffer semantics.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), ostd::Error> {
        self.storage.read_bytes(offset, out)
    }
}
