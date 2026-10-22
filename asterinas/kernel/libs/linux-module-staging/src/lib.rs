// SPDX-License-Identifier: MPL-2.0
//! Data-only staging and owned virtual-address planning, never execution.
//!
//! Ordinary layouts may use synthetic addresses. ReservedModuleLayout instead
//! holds real but unmapped addresses, NOT the locations of staging frames.
//! This API must never be used as authorization to call init/exit.
//! It creates no executable mappings, removes no aliases and certifies no ABI.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod exports;
use alloc::vec::Vec;

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
pub use near_text::{NearTextModuleLayout, NearTextPreparedModule};

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

#[cfg(all(ktest, target_arch = "x86_64"))]
mod runtime_tests;
