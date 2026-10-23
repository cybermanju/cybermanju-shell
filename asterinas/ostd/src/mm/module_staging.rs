// SPDX-License-Identifier: MPL-2.0

//! Private owned data frames, with an opt-in experimental signed-32 lease on
//! fixed single-CPU x86-64 boots. Staging itself has no reserved alias; consuming
//! it into the separate lease creates persistent RW/non-X page-table mappings.
//! Neither API authorizes execution or qualifies a Linux module loader.
//!
//! The real frame allocator is initialized only after boot. This wrapper uses
//! OSTD's existing linear RW mapping via `VmIo`; it does not establish unmapped
//! memory or certify NX. The linear RW alias remains and therefore blocks any
//! future claim of physical W^X when another alias is made executable.

#![forbid(unsafe_code)]

#[cfg(all(ktest, target_arch = "x86_64"))]
pub mod fault_injection;

#[cfg(target_arch = "x86_64")]
mod reserved;
mod storage;

#[cfg(all(ktest, target_arch = "x86_64"))]
pub use crate::mm::kspace::module_signed32_lease::Signed32LeaseAudit;
#[cfg(target_arch = "x86_64")]
pub use crate::mm::kspace::module_signed32_lease::Signed32RoMapping;
#[cfg(target_arch = "x86_64")]
pub use crate::mm::kspace::module_signed32_lease::Signed32RwMapping;
#[cfg(target_arch = "x86_64")]
pub use crate::mm::kspace::module_signed32_lease::Signed32SelectiveNxMapping;
#[cfg(target_arch = "x86_64")]
pub use reserved::ReservedModuleStaging;
pub use storage::ModuleChunk;
use storage::{Page, StagedPages};

use crate::{
    Error,
    mm::{Frame, FrameAllocOptions, VmIo},
};

struct OstdPage(
    Frame<()>,
    #[cfg(all(ktest, target_arch = "x86_64"))] Option<fault_injection::ReleaseProbe>,
);

impl Page for OstdPage {
    const SIZE: usize = crate::mm::PAGE_SIZE;

    fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        let result = self.0.write_bytes(offset, bytes);
        #[cfg(all(ktest, target_arch = "x86_64"))]
        if result.is_ok()
            && let Some(probe) = &self.1
        {
            probe.record_write();
        }
        result
    }

    fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<(), Error> {
        self.0.read_bytes(offset, bytes)
    }
}

/// Owner of private frame handles covering a staging span (gaps included).
/// Its API exposes reads only; this is NOT read-only hardware protection.
/// Dropping the owner releases its references, without explicit data scrubbing.
pub struct ModuleStaging {
    pages: StagedPages<OstdPage>,
}

impl ModuleStaging {
    /// Validates all chunks and budgets before allocating any frames, then
    /// acquires every frame before copying input data.
    /// The span includes alignment gaps and must be a multiple of PAGE_SIZE.
    /// `max_pages` bounds all frames, including gaps; `max_chunks` includes empty
    /// chunks. Any returned allocation/copy error drops all acquired handles.
    /// Requires OSTD's heap and physical allocator to have been initialized.
    pub fn new(
        span: usize,
        chunks: &[ModuleChunk<'_>],
        max_pages: usize,
        max_chunks: usize,
    ) -> Result<Self, Error> {
        let pages = StagedPages::new(span, chunks, max_pages, max_chunks, || {
            Ok(OstdPage(
                FrameAllocOptions::new().zeroed(true).alloc_frame()?,
                #[cfg(all(ktest, target_arch = "x86_64"))]
                None,
            ))
        })?;
        Ok(Self { pages })
    }

    /// Total staged address span in bytes.
    pub fn span(&self) -> usize {
        self.pages.span()
    }

    /// Number of privately owned frames, including frames for gaps.
    pub fn page_count(&self) -> usize {
        self.pages.page_count()
    }

    /// Establish a persistent supervisor RW/NX signed-32 mapping. Requires
    /// initialized boot memory and fixed single-CPU topology. This experimental
    /// API bounds the reservation to 1024 pages, returns no pointer or execution
    /// capability, and preserves the existing linear RW alias (not physical W^X).
    /// Unexpected cleanup failures leak retained resources rather than reuse them;
    /// page-table bookkeeping OOM/panics remain outside Result.
    #[cfg(target_arch = "x86_64")]
    pub fn map_signed32_rw_nx(
        self,
        reservation: crate::mm::module_address::NearTextAddressReservation,
    ) -> Result<Signed32RwMapping, Error> {
        crate::mm::kspace::module_signed32_lease::new(self, reservation)
    }

    /// Consume staging into an experimental read-only/non-X reserved alias.
    /// Uses the same bounded single-CPU RW mapping preflight, then removes W.
    /// The existing linear RW alias remains mutable; this is not physical W^X,
    /// an immutable snapshot, or a Linux execution/production-mapping guarantee.
    /// Errors consume the staging/reservation and run existing fail-closed cleanup.
    #[cfg(target_arch = "x86_64")]
    pub fn map_signed32_ro_nx(
        self,
        reservation: crate::mm::module_address::NearTextAddressReservation,
    ) -> Result<Signed32RoMapping, Error> {
        self.map_signed32_rw_nx(reservation)?.into_read_only_nx()
    }

    #[cfg(target_arch = "x86_64")]
    pub(crate) fn frames_for_signed32(
        &self,
    ) -> impl ExactSizeIterator<Item = Frame<()>> + Clone + '_ {
        self.pages.iter().map(|page| page.0.clone())
    }

    #[cfg(all(ktest, target_arch = "x86_64"))]
    pub fn map_signed32_rw_nx_with_fault_for_test(
        self,
        reservation: crate::mm::module_address::NearTextAddressReservation,
        after: usize,
        observed: &mut usize,
    ) -> Result<Signed32RwMapping, Error> {
        crate::mm::kspace::module_signed32_lease::new_with_fault(
            self,
            reservation,
            Some(after),
            observed,
        )
    }

    /// Reads bytes from the staging span. Out-of-span requests are rejected.
    /// On a backend read error, the caller's output may already have a prefix.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), Error> {
        self.pages.read_bytes(offset, out)
    }

    /// Temporarily map the owned frames in the signed-32 arena for ktests only.
    #[cfg(all(ktest, target_arch = "x86_64"))]
    pub fn read_signed32_mapped_for_test(
        &mut self,
        reservation: &mut crate::mm::module_address::NearTextAddressReservation,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), Error> {
        crate::mm::kspace::module_signed32_map::read_scoped(
            reservation,
            self.pages.span(),
            self.pages.iter().map(|page| page.0.clone()),
            offset,
            out,
        )
    }
}
