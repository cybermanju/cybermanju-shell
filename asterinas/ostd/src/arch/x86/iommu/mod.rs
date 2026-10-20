// SPDX-License-Identifier: MPL-2.0

//! The IOMMU support.

// Set this module's log prefix for `ostd::log`.
macro_rules! __log_prefix {
    () => {
        "iommu: "
    };
}

mod dma_remapping;
mod fault;
mod interrupt_remapping;
mod invalidate;
mod registers;

pub(crate) use dma_remapping::{DomainHandle, IommuPtConfig, has_dma_remapping, map, unmap};
pub(in crate::arch) use interrupt_remapping::{
    IrtEntryHandle, alloc_irt_entry, has_interrupt_remapping,
};

use crate::{io::IoMemAllocatorBuilder, mm::page_table::PageTableError};

/// An enumeration representing possible errors related to IOMMU.
#[derive(Debug)]
pub(crate) enum IommuError {
    /// No IOMMU is available.
    NoIommu,
    /// Error encountered during modification of the page table.
    #[expect(dead_code)]
    ModificationError(PageTableError),
}

pub(crate) fn init(io_mem_builder: &IoMemAllocatorBuilder) -> Result<(), IommuError> {
    registers::init(io_mem_builder)?;
    invalidate::init();
    dma_remapping::init();
    interrupt_remapping::init();
    Ok(())
}

/// Returns whether direct DMA is safe with respect to the currently known
/// IOMMU configuration. Missing ACPI state, an initialized IOMMU, or any
/// malformed/unreadable ACPI root or child table is treated as unsupported.
/// The DMAR/IVRS bodies are not parsed here.
pub(crate) fn direct_dma_supported() -> bool {
    if has_dma_remapping() || registers::IOMMU_REGS.get().is_some() {
        return false;
    }
    matches!(
        crate::arch::kernel::acpi::dma_policy::has_no_dma_remapping_tables(),
        Ok(true)
    )
}
