// SPDX-License-Identifier: MPL-2.0
//! Data-only staging and owned virtual-address planning, never execution.
//!
//! Ordinary layouts may use synthetic addresses. ReservedModuleLayout instead
//! holds real but unmapped addresses, NOT the locations of staging frames.
//! This API must never be used as authorization to call init/exit.
//! It creates no executable mappings, removes no aliases and certifies no ABI.
//! Selective-NX mappings project intended RO/RX pages to RO/NX and RW pages to
//! RW/NX; inaccessible gaps are rejected, and no execution or ABI is implied.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod exports;
use alloc::vec::Vec;

#[cfg(target_arch = "x86_64")]
pub use exports::{CShimExportRegistry, LINUX_MEMCMP_EXPORT_NAME};
pub use exports::{
    ExportAbi, ExportSignature, LinuxExportAdmissionError, OSTD_NUM_CPUS_EXPORT_NAME,
    OstdExportDescriptor, OstdExportRegistry,
};
use linux_module_elf::{Layout, PreparedModule};
use ostd::mm::{
    PAGE_SIZE,
    module_staging::{ModuleChunk, ModuleStaging},
};

#[cfg(target_arch = "x86_64")]
mod reserved;
#[cfg(target_arch = "x86_64")]
pub use reserved::{PreparationLimits, ReservedModuleLayout, ReservedPreparedModule};
#[cfg(target_arch = "x86_64")]
mod near_text;
#[cfg(target_arch = "x86_64")]
pub use near_text::{
    NearTextMappedModule, NearTextModuleLayout, NearTextPreparedModule, NearTextReadOnlyModule,
    NearTextSelectiveNxModule,
};
#[cfg(target_arch = "x86_64")]
pub use near_text::{
    PolicyNearTextMappedModule, PolicyNearTextModuleLayout, PolicyNearTextPreparedModule,
    PolicyNearTextReadOnlyModule, PolicyNearTextSelectiveNxModule,
};

/// A failed transfer never returns a partially staged owner.
#[derive(Debug)]
pub enum Error {
    /// ELF validation, numeric resolution, relocation or staging-plan failure.
    Plan(linux_module_elf::Error),
    /// This bridge currently accepts only 4 KiB OSTD pages.
    UnsupportedPageSize,
    /// The bounded array of borrowed chunk descriptors could not be reserved.
    Resources,
    /// The initialized OSTD allocator or frame copy returned an error.
    Frames(ostd::Error),
    /// A VMALLOC or near-text virtual reservation could not be claimed.
    AddressSpace(ostd::Error),
}

/// Copy all prepared sections, including NOTE/NOBITS data, to private frames.
///
/// Requires initialized OSTD heap/physical allocation. Layout/section matching
/// is structural, not a cryptographic binding to an input ELF. `max_pages`
/// includes alignment gaps and tails; `max_chunks` includes empty sections.
/// Metadata vectors and previously prepared payloads have additional overhead.
/// Returned read access is not hardware read-only protection, and dropping the
/// owner releases references without explicit scrubbing or a reuse guarantee.
pub fn stage_prepared(
    module: &PreparedModule,
    layout: &Layout,
    max_pages: usize,
    max_chunks: usize,
) -> Result<ModuleStaging, Error> {
    if PAGE_SIZE != 4096 {
        return Err(Error::UnsupportedPageSize);
    }
    let plan = module
        .plan_staging(layout, max_pages, max_chunks)
        .map_err(Error::Plan)?;
    let mut chunks = Vec::new();
    chunks
        .try_reserve_exact(plan.chunks().len())
        .map_err(|_| Error::Resources)?;
    for chunk in plan.chunks() {
        chunks.push(ModuleChunk {
            offset: chunk.offset,
            bytes: chunk.bytes,
        });
    }
    ModuleStaging::new(plan.span(), &chunks, max_pages, max_chunks).map_err(Error::Frames)
}

/// Test-only staging adapter for deterministic OSTD allocation/copy fault tests.
/// It duplicates only the bounded descriptor bridge; it does not simulate real
/// allocator OOM and is not a production staging hook.
#[cfg(all(ktest, target_arch = "x86_64"))]
pub(crate) fn stage_prepared_with_fault(
    module: &PreparedModule,
    layout: &Layout,
    max_pages: usize,
    max_chunks: usize,
    mode: ostd::mm::module_staging::fault_injection::FaultMode,
    stats: &alloc::sync::Arc<ostd::mm::module_staging::fault_injection::FaultStats>,
) -> Result<ModuleStaging, Error> {
    if PAGE_SIZE != 4096 {
        return Err(Error::UnsupportedPageSize);
    }
    let plan = module
        .plan_staging(layout, max_pages, max_chunks)
        .map_err(Error::Plan)?;
    let mut chunks = Vec::new();
    chunks
        .try_reserve_exact(plan.chunks().len())
        .map_err(|_| Error::Resources)?;
    for chunk in plan.chunks() {
        chunks.push(ModuleChunk {
            offset: chunk.offset,
            bytes: chunk.bytes,
        });
    }
    ostd::mm::module_staging::fault_injection::stage_with_fault(
        plan.span(),
        &chunks,
        max_pages,
        max_chunks,
        mode,
        stats,
    )
    .map_err(Error::Frames)
}

/// Test-only successful staging adapter with per-handle lifetime observation.
/// The descriptor bridge remains bounded; no allocation/copy failure is injected.
#[cfg(all(ktest, target_arch = "x86_64"))]
pub(crate) fn stage_prepared_observed(
    module: &PreparedModule,
    layout: &Layout,
    max_pages: usize,
    max_chunks: usize,
    stats: &alloc::sync::Arc<ostd::mm::module_staging::fault_injection::FaultStats>,
) -> Result<ModuleStaging, Error> {
    if PAGE_SIZE != 4096 {
        return Err(Error::UnsupportedPageSize);
    }
    let plan = module
        .plan_staging(layout, max_pages, max_chunks)
        .map_err(Error::Plan)?;
    let mut chunks = Vec::new();
    chunks
        .try_reserve_exact(plan.chunks().len())
        .map_err(|_| Error::Resources)?;
    for chunk in plan.chunks() {
        chunks.push(ModuleChunk {
            offset: chunk.offset,
            bytes: chunk.bytes,
        });
    }
    ostd::mm::module_staging::fault_injection::stage_observed(
        plan.span(),
        &chunks,
        max_pages,
        max_chunks,
        stats,
    )
    .map_err(Error::Frames)
}

#[cfg(all(ktest, target_arch = "x86_64"))]
mod runtime_tests;
