// SPDX-License-Identifier: MPL-2.0
//! Data-only, page-relative copy plan for a prepared module image.
use alloc::vec::Vec;

use crate::{Error, Layout, PreparedModule};

const PAGE_SIZE: usize = 4096;
const MAX_CHUNKS: usize = 4096;

/// Borrowed chunks and their offsets within a page-aligned staging span.
#[derive(Debug, Eq, PartialEq)]
pub struct StagingChunk<'a> {
    pub offset: usize,
    pub bytes: &'a [u8],
}

/// Validated metadata for copying a prepared image into caller-owned memory.
/// This contains no frames, pointers, permissions, or executable output.
#[derive(Debug, Eq, PartialEq)]
pub struct StagingPlan<'a> {
    span: usize,
    page_count: usize,
    chunks: Vec<StagingChunk<'a>>,
}

impl<'a> StagingPlan<'a> {
    pub fn span(&self) -> usize {
        self.span
    }
    pub fn page_count(&self) -> usize {
        self.page_count
    }
    pub fn chunks(&self) -> &[StagingChunk<'a>] {
        &self.chunks
    }
}

impl PreparedModule {
    /// Build a borrowed copy plan only when this module exactly matches a
    /// fixed-4096-byte-page layout. Bytes are not cryptographically bound to
    /// the ELF from which the layout was produced; the check is structural.
    pub fn plan_staging<'a>(
        &'a self,
        layout: &Layout,
        max_pages: usize,
        max_chunks: usize,
    ) -> Result<StagingPlan<'a>, Error> {
        if layout.page_size() != Some(PAGE_SIZE as u64) {
            return Err(Error::PageAlignment);
        }
        let base = usize::try_from(layout.base()).map_err(|_| Error::LayoutOverflow)?;
        let span = usize::try_from(layout.span()).map_err(|_| Error::LayoutOverflow)?;
        if base % PAGE_SIZE != 0 || span % PAGE_SIZE != 0 {
            return Err(Error::PageAlignment);
        }
        let end = base.checked_add(span).ok_or(Error::LayoutOverflow)?;
        let pages = span.checked_div(PAGE_SIZE).ok_or(Error::LayoutOverflow)?;
        if pages > max_pages {
            return Err(Error::ResourceLimit);
        }
        let sections = self.sections();
        let placements = layout.placements();
        let chunk_limit = core::cmp::min(max_chunks, MAX_CHUNKS);
        if sections.len() != placements.len() || sections.len() > chunk_limit {
            return Err(if sections.len() > chunk_limit {
                Error::ResourceLimit
            } else {
                Error::LayoutMismatch
            });
        }

        // Complete preflight before allocating metadata.
        let mut previous_start = base;
        let mut previous_end = base;
        for (section, placement) in sections.iter().zip(placements) {
            if section.placement() != placement {
                return Err(Error::LayoutMismatch);
            }
            let address = usize::try_from(placement.address).map_err(|_| Error::LayoutOverflow)?;
            let size = usize::try_from(placement.size).map_err(|_| Error::LayoutOverflow)?;
            let chunk_end = address.checked_add(size).ok_or(Error::LayoutOverflow)?;
            if address < base
                || address < previous_start
                || address < previous_end
                || chunk_end > end
            {
                return Err(Error::LayoutMismatch);
            }
            let offset = address.checked_sub(base).ok_or(Error::LayoutOverflow)?;
            if offset % PAGE_SIZE != 0 {
                return Err(Error::PageAlignment);
            }
            if section.bytes().len() != size {
                return Err(Error::LayoutMismatch);
            }
            previous_start = address;
            previous_end = chunk_end;
        }
        let mut chunks = Vec::new();
        chunks
            .try_reserve_exact(sections.len())
            .map_err(|_| Error::ResourceLimit)?;
        for section in sections {
            let address =
                usize::try_from(section.placement().address).map_err(|_| Error::LayoutOverflow)?;
            let offset = address.checked_sub(base).ok_or(Error::LayoutOverflow)?;
            chunks.push(StagingChunk {
                offset,
                bytes: section.bytes(),
            });
        }
        Ok(StagingPlan {
            span,
            page_count: pages,
            chunks,
        })
    }
}
