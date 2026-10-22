// SPDX-License-Identifier: MPL-2.0

//! Reserved planning and data-only staging; temporary mapping probes are ktest-only.

use core::ops::Range;

use linux_module_elf::{
    Export, Layout, PreparedModule, SymbolResolver, page_layout_span_bound, plan_page_layout,
};
use ostd::mm::{
    PAGE_SIZE, Vaddr, module_address::ModuleAddressReservation,
    module_staging::ReservedModuleStaging,
};

use super::{Error, stage_prepared};

/// Independent limits for preparation payload, RELA entries and frame staging.
/// Payload excludes page padding; page budgets include gaps and page tails.
/// These do not bound allocator bookkeeping or authenticate the input ELF.
#[derive(Clone, Copy, Debug)]
pub struct PreparationLimits {
    /// Sum of ALLOC payload bytes, including NOBITS but excluding page padding.
    pub max_payload_bytes: usize,
    /// Aggregate ALLOC-target RELA entries, including NONE; hard ceiling 65,536.
    pub max_relocation_entries: usize,
    /// Prepared sections, including empty sections.
    pub max_sections: usize,
    /// Staging pages including alignment gaps and tails, not reservation slack.
    pub max_pages: usize,
    /// Staging descriptors, including empty sections.
    pub max_chunks: usize,
}

/// Holds a real VMALLOC reservation together with a layout replanned at its base.
///
/// The reservation may be larger than the layout because its alignment envelope
/// is conservative. These addresses are NOT mapped to staging frames. Keep this
/// owner alive when depending on them; cloned metadata does not retain ownership.
/// Construction alone does not prepare relocations or guarantee high-base reach;
/// [`Self::prepare_and_stage`] validates that reach before transferring ownership.
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
        Self::new_with_reservation(bytes, max_pages, max_sections, |span, pages| {
            ModuleAddressReservation::new(span, pages)
        })
    }

    /// Validate and budget the conservative envelope before reserving at `base`.
    ///
    /// The layout is replanned at the exact requested base; there is no fallback
    /// address. `max_pages` includes worst-case alignment gaps. No frames or
    /// mappings are created. Invalid ELF/layout or budget inputs fail before the
    /// address claim; a failed real-base replan drops the reservation. Existing
    /// VMALLOC bookkeeping OOM remains outside this Result guarantee.
    pub fn new_at(
        bytes: &[u8],
        base: Vaddr,
        max_pages: usize,
        max_sections: usize,
    ) -> Result<Self, Error> {
        Self::new_with_reservation(bytes, max_pages, max_sections, |span, pages| {
            ModuleAddressReservation::new_at(base, span, pages)
        })
    }

    fn new_with_reservation(
        bytes: &[u8],
        max_pages: usize,
        max_sections: usize,
        reserve: impl FnOnce(usize, usize) -> Result<ModuleAddressReservation, ostd::Error>,
    ) -> Result<Self, Error> {
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
        let reservation = reserve(span, max_pages).map_err(Error::AddressSpace)?;
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

    /// Resolve and relocate at this reservation's actual base, then stage data.
    ///
    /// Consumes the owner even on error. A returned error releases its virtual
    /// reservation and private temporary data; no partially staged owner escapes.
    /// Relocation reach is checked before staging frames, not at a synthetic base.
    /// ELF/layout matching remains structural, and exports are numeric values,
    /// not an ABI certificate. Allocator bookkeeping OOM remains outside Result.
    /// No mapping, executable permission or init/exit capability is created.
    pub fn prepare_and_stage(
        self,
        bytes: &[u8],
        exports: &[Export<'_>],
        limits: PreparationLimits,
    ) -> Result<ReservedPreparedModule, Error> {
        let prepared = SymbolResolver::new(bytes, &self.layout, exports)
            .map_err(Error::Plan)?
            .prepare_module(
                limits.max_payload_bytes,
                limits.max_relocation_entries,
                limits.max_sections,
            )
            .map_err(Error::Plan)?;
        self.stage(&prepared, limits.max_pages, limits.max_chunks)
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

    /// Temporary real-VA readback in the single-CPU ktest harness only.
    /// There is no persistent mapping or execution capability after this call.
    #[cfg(ktest)]
    pub fn read_mapped_for_test(
        &mut self,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), ostd::Error> {
        self.storage.read_mapped_for_test(offset, out)
    }
}
