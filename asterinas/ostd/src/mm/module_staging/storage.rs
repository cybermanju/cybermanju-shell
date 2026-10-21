// SPDX-License-Identifier: MPL-2.0

//! Generic owned page storage for module staging. This core only depends on
//! `alloc` and `Error`, so it can also be compiled in a host test harness.

use crate::error::Error;
use alloc::vec::Vec;

/// A page-like storage backend used by [`StagedPages`].
/// The trusted internal allocator must return owned, zero-filled pages of SIZE.
/// This safe abstraction does not enforce physical aliasing or MMU properties.
pub(super) trait Page {
    const SIZE: usize;
    fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error>;
    fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<(), Error>;
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

#[cfg(ktest)]
#[path = "runtime_tests.rs"]
mod runtime_tests;

/// A borrowed input range. Empty chunks do not reserve pages.
pub struct ModuleChunk<'a> {
    /// Page-aligned relative offset, not a virtual address or a physical pointer.
    pub offset: usize,
    /// Bytes to copy. Nonempty chunks must be in sorted, page-disjoint order.
    pub bytes: &'a [u8],
}

/// Private owned pages covering the complete span, including gaps.
pub(super) struct StagedPages<P: Page> {
    span: usize,
    pages: Vec<P>,
}

impl<P: Page> StagedPages<P> {
    pub(super) fn new<F>(
        span: usize,
        chunks: &[ModuleChunk<'_>],
        max_pages: usize,
        max_chunks: usize,
        mut allocate: F,
    ) -> Result<Self, Error>
    where
        F: FnMut() -> Result<P, Error>,
    {
        let size = P::SIZE;
        if size == 0 || !size.is_power_of_two() || span % size != 0 || chunks.len() > max_chunks {
            return Err(Error::InvalidArgs);
        }
        let total_pages = span / size;
        if total_pages > max_pages {
            return Err(Error::InvalidArgs);
        }

        // Validate every input and the nonempty reserved intervals before any
        // allocator call; also compute their total physical page budget.
        let mut previous_end = 0usize;
        let mut have_nonempty = false;
        for chunk in chunks {
            if chunk.offset % size != 0 || chunk.offset > span {
                return Err(Error::InvalidArgs);
            }
            let len = chunk.bytes.len();
            let end = chunk.offset.checked_add(len).ok_or(Error::Overflow)?;
            if end > span {
                return Err(Error::InvalidArgs);
            }
            if len != 0 {
                let rounded = len.checked_add(size - 1).ok_or(Error::Overflow)? / size;
                let reserved = rounded.checked_mul(size).ok_or(Error::Overflow)?;
                let reserved_end = chunk.offset.checked_add(reserved).ok_or(Error::Overflow)?;
                if reserved_end > span {
                    return Err(Error::InvalidArgs);
                }
                if have_nonempty && chunk.offset < previous_end {
                    return Err(Error::InvalidArgs);
                }
                previous_end = reserved_end;
                have_nonempty = true;
            }
        }

        let mut pages = Vec::new();
        pages
            .try_reserve_exact(total_pages)
            .map_err(|_| Error::NoMemory)?;
        // Acquire all pages before copying any input. On any backend error, Vec
        // drops all already-acquired pages and no staged object escapes.
        for _ in 0..total_pages {
            pages.push(allocate()?);
        }

        for chunk in chunks {
            let mut consumed = 0usize;
            while consumed < chunk.bytes.len() {
                let absolute = chunk.offset + consumed;
                let page_index = absolute / size;
                let in_page = absolute % size;
                let count = (size - in_page).min(chunk.bytes.len() - consumed);
                pages[page_index].write(in_page, &chunk.bytes[consumed..consumed + count])?;
                consumed += count;
            }
        }
        Ok(Self { span, pages })
    }

    pub(super) fn span(&self) -> usize {
        self.span
    }
    pub(super) fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub(super) fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), Error> {
        let end = offset.checked_add(out.len()).ok_or(Error::Overflow)?;
        if end > self.span {
            return Err(Error::InvalidArgs);
        }
        let size = P::SIZE;
        let mut copied = 0usize;
        while copied < out.len() {
            let absolute = offset + copied;
            let page_index = absolute / size;
            let in_page = absolute % size;
            let count = (size - in_page).min(out.len() - copied);
            self.pages[page_index].read(in_page, &mut out[copied..copied + count])?;
            copied += count;
        }
        Ok(())
    }
}
