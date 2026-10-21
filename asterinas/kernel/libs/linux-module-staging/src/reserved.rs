// SPDX-License-Identifier: MPL-2.0

//! Address ownership and metadata only: no mapping or executable memory.

use core::ops::Range;

use linux_module_elf::{Layout, page_layout_span_bound, plan_page_layout};
use ostd::mm::{PAGE_SIZE, Vaddr, module_address::ModuleAddressReservation};

use super::Error;

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
}
