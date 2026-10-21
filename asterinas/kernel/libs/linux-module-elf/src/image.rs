// SPDX-License-Identifier: MPL-2.0
//! All-or-error preparation of allocatable sections as ordinary heap data.
use alloc::vec::Vec;

use crate::{
    Error, Layout, SECTION_SIZE, SectionPlacement, SymbolResolver, file_range, u32_at, u64_at,
};

/// A private, complete section copy, never an executable mapping.
#[derive(Debug, Eq, PartialEq)]
pub struct PreparedSection {
    placement: SectionPlacement,
    bytes: Vec<u8>,
}

impl PreparedSection {
    pub fn placement(&self) -> &SectionPlacement {
        &self.placement
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Data-only preparation, NOT a loaded, trusted or ABI-compatible module.
/// No section escapes until every section has succeeded. Numeric addresses
/// describe the layout, not the addresses of these heap buffers.
#[derive(Debug, Eq, PartialEq)]
pub struct PreparedModule {
    sections: Vec<PreparedSection>,
    payload_bytes: usize,
    relocation_entries: usize,
}

impl PreparedModule {
    pub fn sections(&self) -> &[PreparedSection] {
        &self.sections
    }
    pub fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }
    pub fn relocation_entries(&self) -> usize {
        self.relocation_entries
    }
}

pub(super) fn prepare_module(
    bytes: &[u8],
    layout: &Layout,
    resolver: &SymbolResolver<'_>,
    max_bytes: usize,
    max_entries: usize,
    max_sections: usize,
) -> Result<PreparedModule, Error> {
    // Only called with the immutable ELF/layout already validated by resolver.
    // Preflight the entire operation before allocating any payload or patches.
    if layout.placements().len() > max_sections {
        return Err(Error::ResourceLimit);
    }
    let table = file_range(
        bytes,
        u64_at(bytes, 40),
        u64::from(crate::u16_at(bytes, 60)) * SECTION_SIZE as u64,
    )?;
    let section = |index: usize| &table[index * SECTION_SIZE..(index + 1) * SECTION_SIZE];
    let mut payload_bytes = 0usize;
    for placement in layout.placements() {
        let size = usize::try_from(placement.size).map_err(|_| Error::ResourceLimit)?;
        payload_bytes = payload_bytes
            .checked_add(size)
            .ok_or(Error::ResourceLimit)?;
        if payload_bytes > max_bytes {
            return Err(Error::ResourceLimit);
        }
    }
    let mut relocation_entries = 0usize;
    for rela in table.chunks_exact(SECTION_SIZE) {
        if u32_at(rela, 4) != 4 {
            continue;
        }
        let target = section(u32_at(rela, 44) as usize);
        if u64_at(target, 8) & 2 == 0 {
            continue; // Debug/non-ALLOC relocation tables are not applied.
        }
        // NOTE is opaque metadata: copy it verbatim, but reject ANY table
        // targeting it (including empty tables and NONE-only tables).
        if !matches!(u32_at(target, 4), 1 | 8) {
            return Err(Error::RelocationTarget);
        }
        let count = usize::try_from(u64_at(rela, 32) / 24).map_err(|_| Error::ResourceLimit)?;
        relocation_entries = relocation_entries
            .checked_add(count)
            .ok_or(Error::ResourceLimit)?;
        if relocation_entries > core::cmp::min(max_entries, 65_536) {
            return Err(Error::ResourceLimit);
        }
    }
    let mut sections = Vec::new();
    sections
        .try_reserve_exact(layout.placements().len())
        .map_err(|_| Error::ResourceLimit)?;
    for placement in layout.placements() {
        let header = section(usize::from(placement.section_index));
        let data = if u32_at(header, 4) == 7 {
            let source = file_range(bytes, u64_at(header, 24), placement.size)?;
            let mut copy = Vec::new();
            copy.try_reserve_exact(source.len())
                .map_err(|_| Error::ResourceLimit)?;
            copy.extend_from_slice(source);
            copy
        } else {
            resolver.relocate_target(placement.section_index, max_bytes, max_entries)?
        };
        sections.push(PreparedSection {
            placement: *placement,
            bytes: data,
        });
    }
    Ok(PreparedModule {
        sections,
        payload_bytes,
        relocation_entries,
    })
}
