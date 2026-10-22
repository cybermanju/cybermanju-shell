// SPDX-License-Identifier: MPL-2.0
//! Metadata-only intended permissions for pages described by a checked layout.

use alloc::vec::Vec;

use crate::{Error, layout::Layout};

const PAGE_SIZE: u64 = 4096;
const SHF_ALLOC: u64 = 0x2;
const SHF_WRITE: u64 = 0x1;
const SHF_EXECINSTR: u64 = 0x4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntendedPagePermission {
    Inaccessible,
    ReadOnly,
    ReadWrite,
    ReadExecute,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PagePermissionPlan {
    base: u64,
    span: u64,
    pages: Vec<IntendedPagePermission>,
}

impl PagePermissionPlan {
    pub fn base(&self) -> u64 {
        self.base
    }
    pub fn span(&self) -> u64 {
        self.span
    }
    pub fn pages(&self) -> &[IntendedPagePermission] {
        &self.pages
    }
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}

/// Derive intended permissions from a previously checked section layout.
/// This allocates metadata only: it neither maps frames nor applies permissions.
pub fn plan_page_permissions(
    layout: &Layout,
    max_pages: usize,
) -> Result<PagePermissionPlan, Error> {
    let base = layout.base();
    let source_span = layout.span();
    if base % PAGE_SIZE != 0 {
        return Err(Error::PageAlignment);
    }
    let source_end = base.checked_add(source_span).ok_or(Error::LayoutOverflow)?;
    // Round the absolute end, then express the page-rounded extent relative to base.
    let span = if source_span == 0 {
        0
    } else {
        (source_end
            .checked_add(PAGE_SIZE - 1)
            .ok_or(Error::LayoutOverflow)?
            & !(PAGE_SIZE - 1))
            .checked_sub(base)
            .ok_or(Error::LayoutOverflow)?
    };
    let count_u64 = span / PAGE_SIZE;
    let count = usize::try_from(count_u64).map_err(|_| Error::ResourceLimit)?;
    if count > max_pages {
        return Err(Error::ResourceLimit);
    }

    // Validate every placement against the unrounded original source span before allocating.
    for item in layout.placements() {
        if item.flags & SHF_ALLOC != SHF_ALLOC {
            return Err(Error::LayoutMismatch);
        }
        if !item.alignment.is_power_of_two() || item.address % item.alignment != 0 {
            return Err(Error::SectionAlignment);
        }
        let end = item
            .address
            .checked_add(item.size)
            .ok_or(Error::LayoutOverflow)?;
        if item.address < base || end > source_end {
            return Err(Error::LayoutMismatch);
        }
        if item.flags & (SHF_WRITE | SHF_EXECINSTR) == (SHF_WRITE | SHF_EXECINSTR) {
            return Err(Error::WritableExecutableSection);
        }
    }

    let mut pages = Vec::new();
    pages
        .try_reserve_exact(count)
        .map_err(|_| Error::ResourceLimit)?;
    pages.resize(count, IntendedPagePermission::Inaccessible);
    for item in layout.placements() {
        if item.size == 0 {
            continue;
        }
        let end = item
            .address
            .checked_add(item.size)
            .ok_or(Error::LayoutOverflow)?;
        let first = (item.address - base) / PAGE_SIZE;
        let last = (end - base - 1) / PAGE_SIZE;
        let desired = match item.flags & (SHF_WRITE | SHF_EXECINSTR) {
            SHF_WRITE => IntendedPagePermission::ReadWrite,
            SHF_EXECINSTR => IntendedPagePermission::ReadExecute,
            _ => IntendedPagePermission::ReadOnly,
        };
        for index in first..=last {
            let slot = &mut pages[usize::try_from(index).map_err(|_| Error::ResourceLimit)?];
            if *slot == IntendedPagePermission::Inaccessible || *slot == desired {
                *slot = desired;
            } else if (*slot == IntendedPagePermission::ReadWrite
                && desired == IntendedPagePermission::ReadExecute)
                || (*slot == IntendedPagePermission::ReadExecute
                    && desired == IntendedPagePermission::ReadWrite)
            {
                return Err(Error::WritableExecutablePage);
            } else {
                return Err(Error::PermissionPageConflict);
            }
        }
    }
    Ok(PagePermissionPlan { base, span, pages })
}
