// SPDX-License-Identifier: MPL-2.0

//! Private, owned frame staging for data that will be consumed before a module
//! loader is integrated. This does not create mappings or executable memory.
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

#[cfg(target_arch = "x86_64")]
pub use reserved::ReservedModuleStaging;
pub use storage::ModuleChunk;
use storage::{Page, StagedPages};

use crate::{
    Error,
    mm::{Frame, FrameAllocOptions, VmIo},
};

struct OstdPage(Frame<()>);

impl Page for OstdPage {
    const SIZE: usize = crate::mm::PAGE_SIZE;

    fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        self.0.write_bytes(offset, bytes)
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

    /// Reads bytes from the staging span. Out-of-span requests are rejected.
    /// On a backend read error, the caller's output may already have a prefix.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), Error> {
        self.pages.read_bytes(offset, out)
    }
}
