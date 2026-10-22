// SPDX-License-Identifier: MPL-2.0
//! Numeric symbol resolution only. This makes no ABI, CRC, GPL, or W^X claim.
use alloc::vec::Vec;

use super::{Error, Layout, SECTION_SIZE, file_range, inspect, u16_at, u32_at, u64_at};

const SYMBOL_SIZE: usize = 24;
const SHT_SYMTAB: u32 = 2;
const SHN_UNDEF: u16 = 0;
const SHN_ABS: u16 = 0xfff1;
const SHN_COMMON: u16 = 0xfff2;

/// An explicitly supplied numeric export, not a callable function or ABI proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Export<'a> {
    pub name: &'a [u8],
    pub value: u64,
}

/// Location of one failing ALLOC-target RELA entry, not a loaded address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelocationSite {
    pub rela_section: u16,
    pub entry_index: usize,
    pub target_section: u16,
    pub kind: u32,
    pub symbol_index: u32,
    pub offset: u64,
}

/// Preflight failure with entry context only when an entry was actually checked.
/// Budget and table-policy failures have no fabricated entry location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelocationReachError {
    pub cause: Error,
    pub site: Option<RelocationSite>,
}

impl From<Error> for RelocationReachError {
    fn from(cause: Error) -> Self {
        Self { cause, site: None }
    }
}

/// Intersected direct PC32/PLT32 placement constraints for undefined imports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relative32ImportConstraints {
    window: Option<crate::Relative32BaseWindow>,
    rela_entries: usize,
    import_relocations: usize,
    ignored_relative32_relocations: usize,
}

/// Intersected direct PC32/PLT32 placement constraints for imports and definitions.
/// Applies only to this layout's relative geometry; not a complete placement
/// plan, address reservation, ABI admission, permission or execution proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relative32PlacementConstraints {
    window: Option<crate::Relative32BaseWindow>,
    rela_entries: usize,
    import_relocations: usize,
    absolute_relocations: usize,
    section_relative_relocations: usize,
}

impl Relative32PlacementConstraints {
    /// Inclusive intersection, or no base for this relative geometry/empty span.
    /// With no selected entries, this has only the module-span bound.
    pub const fn window(self) -> Option<crate::Relative32BaseWindow> {
        self.window
    }
    /// All budgeted ALLOC-target RELA, including NONE and other relocation kinds.
    pub const fn rela_entries(self) -> usize {
        self.rela_entries
    }
    /// Undefined-symbol PC32/PLT32 entries, not distinct export names.
    pub const fn import_relocations(self) -> usize {
        self.import_relocations
    }
    /// SHN_ABS PC32/PLT32 entries constrained by their literal symbol values.
    pub const fn absolute_relocations(self) -> usize {
        self.absolute_relocations
    }
    /// PC32/PLT32 entries with symbols defined in planned sections, irrespective
    /// of LOCAL/GLOBAL binding. Their S+A-P delta includes both section offsets.
    pub const fn section_relative_relocations(self) -> usize {
        self.section_relative_relocations
    }
}

impl Relative32ImportConstraints {
    /// Inclusive intersection; no imports yields only the module-span bound.
    pub const fn window(self) -> Option<crate::Relative32BaseWindow> {
        self.window
    }
    /// All budgeted ALLOC-target RELA entries, including NONE and other kinds.
    pub const fn rela_entries(self) -> usize {
        self.rela_entries
    }
    /// Selected undefined-symbol relocation entries, not distinct symbol names.
    pub const fn import_relocations(self) -> usize {
        self.import_relocations
    }
    /// PC32/PLT32 entries with non-undefined symbols, including local and ABS.
    pub const fn ignored_relative32_relocations(self) -> usize {
        self.ignored_relative32_relocations
    }
}

/// Resolves requested symbols only; debug/non-ALLOC symbols need not resolve.
/// Layout matching is structural, not a cryptographic identity check of the ELF.
pub struct SymbolResolver<'a> {
    bytes: &'a [u8],
    table: &'a [u8],
    layout: &'a Layout,
    exports: Vec<Export<'a>>,
}

impl<'a> SymbolResolver<'a> {
    /// Validate and copy at most 4096 named exports, rejecting duplicates/NULs.
    /// Missing weak exports are errors rather than implicit zero resolutions.
    pub fn new(bytes: &'a [u8], layout: &'a Layout, exports: &[Export<'a>]) -> Result<Self, Error> {
        let inspection = inspect(bytes)?;
        let planned = layout.replan(bytes).map_err(|_| Error::LayoutMismatch)?;
        if planned != *layout || usize::from(inspection.section_count) * SECTION_SIZE > bytes.len()
        {
            return Err(Error::LayoutMismatch);
        }
        if exports.len() > 4096 {
            return Err(Error::ResourceLimit);
        }
        let mut sorted = Vec::new();
        sorted
            .try_reserve_exact(exports.len())
            .map_err(|_| Error::ResourceLimit)?;
        sorted.extend_from_slice(exports);
        if sorted
            .iter()
            .any(|export| export.name.is_empty() || export.name.contains(&0))
        {
            return Err(Error::SymbolName);
        }
        sorted.sort_unstable_by(|a, b| a.name.cmp(b.name));
        if sorted.windows(2).any(|pair| pair[0].name == pair[1].name) {
            return Err(Error::DuplicateExport);
        }
        let table_offset = usize::try_from(u64_at(bytes, 40)).map_err(|_| Error::LayoutMismatch)?;
        let table_len = usize::from(inspection.section_count)
            .checked_mul(SECTION_SIZE)
            .ok_or(Error::LayoutMismatch)?;
        let table = bytes
            .get(
                table_offset
                    ..table_offset
                        .checked_add(table_len)
                        .ok_or(Error::LayoutMismatch)?,
            )
            .ok_or(Error::LayoutMismatch)?;
        Ok(Self {
            bytes,
            table,
            layout,
            exports: sorted,
        })
    }

    /// Apply every RELA targeting one planned section to a fresh data-only copy.
    /// The budget counts all entries across all tables, including NONE entries.
    /// Cross-table overlapping writes fail; no partial result or ELF mutation is
    /// exposed. This neither maps memory nor executes module code.
    pub fn relocate_target(
        &self,
        target_index: u16,
        max_bytes: usize,
        max_entries: usize,
    ) -> Result<Vec<u8>, Error> {
        crate::apply::relocate_target(
            self.bytes,
            self.layout,
            self,
            target_index,
            max_bytes,
            max_entries,
        )
    }

    /// Prepare ALL planned ALLOC sections, returning complete data or an error.
    /// `max_bytes` caps summed payload (including NOTE and NOBITS), not padding
    /// or allocator overhead. Layout span is bounded separately by plan_layout.
    /// `max_entries` counts all ALLOC-target RELA entries, including NONE, with
    /// a hard aggregate ceiling of 65,536. `max_sections` includes empty sections.
    /// NOTE is copied verbatim; any RELA targeting NOTE is rejected. Non-ALLOC
    /// tables are not applied. All budgets are checked before payload allocation.
    /// On failure private copies are dropped; input and earlier results remain
    /// unchanged. No mapping, execution, trust, W^X or Linux ABI is established.
    pub fn prepare_module(
        &self,
        max_bytes: usize,
        max_entries: usize,
        max_sections: usize,
    ) -> Result<crate::PreparedModule, Error> {
        crate::image::prepare_module(
            self.bytes,
            self.layout,
            self,
            max_bytes,
            max_entries,
            max_sections,
        )
    }

    /// Check numeric resolution and arithmetic reach before payload allocation.
    /// Counts ALLOC-target RELA including NONE, with a hard cap of 65,536.
    /// Debug/non-ALLOC targets are ignored; NOTE-target tables are rejected.
    /// No payload or patch vector is allocated; resolver metadata already exists.
    /// Not a full write-range/overlap, trust, ABI or permission check:
    /// preparation is still required before staging any bytes.
    pub fn check_relocation_reach(&self, max_entries: usize) -> Result<usize, Error> {
        crate::apply::check_relocation_reach(self.bytes, self.layout, self, max_entries)
    }

    /// Same preflight as check_relocation_reach, with the first failing entry's
    /// section/index/type/symbol/offset. Budget and table failures have no site.
    /// This adds no allocation, mapping, execution or ABI permission.
    pub fn diagnose_relocation_reach(
        &self,
        max_entries: usize,
    ) -> Result<usize, RelocationReachError> {
        crate::apply::diagnose_relocation_reach(self.bytes, self.layout, self, max_entries)
    }

    /// Intersect numeric windows for undefined-symbol direct PC32/PLT32 imports.
    /// Counts all ALLOC-target RELA, including NONE, before resolving imports;
    /// the hard aggregate cap is 65,536. Rejects NOTE-target tables and ignores
    /// debug/non-ALLOC targets. Local/ABS relative32 symbols and other kinds do
    /// not constrain the window and still require full relocation validation.
    /// No imports gives the module-span-only window; None means an empty span
    /// or no common base. Even an empty window does not mask later bad imports.
    /// No payload, patch, or VA allocation; resolver metadata already exists.
    /// This is not a full write-overlap, canonicality, arena, ABI or W^X check.
    /// After choosing a candidate, replan (offsets/span can change), recollect
    /// constraints and rerun complete preflight/preparation before staging.
    pub fn relative32_import_constraints(
        &self,
        max_entries: usize,
    ) -> Result<Relative32ImportConstraints, Error> {
        let table = file_range(
            self.bytes,
            u64_at(self.bytes, 40),
            u64::from(u16_at(self.bytes, 60)) * SECTION_SIZE as u64,
        )?;
        let section = |i: usize| -> Result<&[u8], Error> {
            let start = i.checked_mul(SECTION_SIZE).ok_or(Error::SectionTable)?;
            table
                .get(start..start.checked_add(SECTION_SIZE).ok_or(Error::SectionTable)?)
                .ok_or(Error::SectionTable)
        };
        let mut total = 0usize;
        for rela in table.chunks_exact(SECTION_SIZE) {
            if u32_at(rela, 4) != 4 {
                continue;
            }
            let target =
                section(usize::try_from(u32_at(rela, 44)).map_err(|_| Error::RelocationTarget)?)?;
            if u64_at(target, 8) & 2 == 0 {
                continue;
            }
            if !matches!(u32_at(target, 4), 1 | 8) {
                return Err(Error::RelocationTarget);
            }
            total = total
                .checked_add(
                    usize::try_from(u64_at(rela, 32) / 24).map_err(|_| Error::ResourceLimit)?,
                )
                .ok_or(Error::ResourceLimit)?;
            if total > core::cmp::min(max_entries, 65_536) {
                return Err(Error::ResourceLimit);
            }
        }
        let mut window = crate::Relative32BaseWindow::for_module_span(self.layout.span());
        let mut imports = 0usize;
        let mut ignored = 0usize;
        for rela in table.chunks_exact(SECTION_SIZE) {
            if u32_at(rela, 4) != 4 {
                continue;
            }
            let target_index = u32_at(rela, 44);
            let target =
                section(usize::try_from(target_index).map_err(|_| Error::RelocationTarget)?)?;
            if u64_at(target, 8) & 2 == 0 {
                continue;
            }
            if u32_at(target, 4) == 7 {
                return Err(Error::RelocationTarget);
            }
            let placement = self
                .layout
                .placements()
                .iter()
                .find(|p| u32::from(p.section_index) == target_index)
                .ok_or(Error::RelocationTarget)?;
            let symtab = u16::try_from(u32_at(rela, 40)).map_err(|_| Error::RelocationLink)?;
            for entry in
                file_range(self.bytes, u64_at(rela, 24), u64_at(rela, 32))?.chunks_exact(24)
            {
                let info = u64_at(entry, 8);
                let kind = info as u32;
                if !matches!(kind, 2 | 4) {
                    continue;
                }
                let symbol_index = (info >> 32) as u32;
                let symtab_off = usize::from(symtab)
                    .checked_mul(SECTION_SIZE)
                    .ok_or(Error::SymbolTable)?;
                let symtab_data = self
                    .table
                    .get(
                        symtab_off
                            ..symtab_off
                                .checked_add(SECTION_SIZE)
                                .ok_or(Error::SymbolTable)?,
                    )
                    .ok_or(Error::SymbolTable)?;
                let symbols =
                    file_range(self.bytes, u64_at(symtab_data, 24), u64_at(symtab_data, 32))?;
                let symoff = usize::try_from(symbol_index)
                    .map_err(|_| Error::SymbolTable)?
                    .checked_mul(SYMBOL_SIZE)
                    .ok_or(Error::SymbolTable)?;
                let symbol = symbols
                    .get(symoff..symoff.checked_add(SYMBOL_SIZE).ok_or(Error::SymbolTable)?)
                    .ok_or(Error::SymbolTable)?;
                if u16_at(symbol, 6) != SHN_UNDEF {
                    ignored = ignored.checked_add(1).ok_or(Error::ResourceLimit)?;
                    continue;
                }
                // resolve() performs all name, binding, visibility, and export checks.
                let value = self.resolve(symtab, symbol_index)?;
                imports = imports.checked_add(1).ok_or(Error::ResourceLimit)?;
                let offset = u64_at(entry, 0);
                if offset.checked_add(4).ok_or(Error::RelocationRange)? > placement.size {
                    return Err(Error::RelocationRange);
                }
                let place_offset = placement
                    .address
                    .checked_sub(self.layout.base())
                    .and_then(|base_offset| base_offset.checked_add(offset))
                    .ok_or(Error::RelocationRange)?;
                let candidate = crate::relative32_base_window(
                    value,
                    u64_at(entry, 16) as i64,
                    place_offset,
                    self.layout.span(),
                );
                window = match (window, candidate) {
                    (Some(a), Some(b)) => a.intersect(b),
                    _ => None,
                };
            }
        }
        Ok(Relative32ImportConstraints {
            window,
            rela_entries: total,
            import_relocations: imports,
            ignored_relative32_relocations: ignored,
        })
    }

    /// Numeric constraints from all direct PC32/PLT32 in ALLOC sections.
    /// UNDEF/ABS values constrain the base; section-defined symbols use the
    /// base-cancelling delta S+A-P, including both sections' relative offsets.
    /// Counts all ALLOC RELA (including NONE/other kinds) before resolving any
    /// selected symbols, hard cap 65,536; NOTE rejected, non-ALLOC ignored.
    /// All selected symbols are resolved for validation, even after window=None.
    /// COMMON/TLS/IFUNC and unsupported bindings/visibility remain errors.
    /// No payload/patch/frame/VA allocation; resolver metadata already exists.
    /// None means no base for THIS relative geometry, not every possible replan.
    /// Replan/recollect at the chosen base and run complete preflight/preparation.
    /// Other relocation kinds, write overlaps, canonicality, arena availability,
    /// ABI, permissions and execution are not established by this partial scan.
    pub fn relative32_placement_constraints(
        &self,
        max_entries: usize,
    ) -> Result<Relative32PlacementConstraints, Error> {
        let table = file_range(
            self.bytes,
            u64_at(self.bytes, 40),
            u64::from(u16_at(self.bytes, 60)) * SECTION_SIZE as u64,
        )?;
        let section = |i: usize| -> Result<&[u8], Error> {
            let start = i.checked_mul(SECTION_SIZE).ok_or(Error::SectionTable)?;
            table
                .get(start..start.checked_add(SECTION_SIZE).ok_or(Error::SectionTable)?)
                .ok_or(Error::SectionTable)
        };
        let mut total = 0usize;
        for rela in table.chunks_exact(SECTION_SIZE) {
            if u32_at(rela, 4) != 4 {
                continue;
            }
            let target =
                section(usize::try_from(u32_at(rela, 44)).map_err(|_| Error::RelocationTarget)?)?;
            if u64_at(target, 8) & 2 == 0 {
                continue;
            }
            if !matches!(u32_at(target, 4), 1 | 8) {
                return Err(Error::RelocationTarget);
            }
            total = total
                .checked_add(
                    usize::try_from(u64_at(rela, 32) / 24).map_err(|_| Error::ResourceLimit)?,
                )
                .ok_or(Error::ResourceLimit)?;
            if total > core::cmp::min(max_entries, 65536) {
                return Err(Error::ResourceLimit);
            }
        }
        let mut window = crate::Relative32BaseWindow::for_module_span(self.layout.span());
        let (mut imports, mut absolute, mut locals) = (0usize, 0usize, 0usize);
        for rela in table.chunks_exact(SECTION_SIZE) {
            if u32_at(rela, 4) != 4 {
                continue;
            }
            let target_index = u32_at(rela, 44);
            let target =
                section(usize::try_from(target_index).map_err(|_| Error::RelocationTarget)?)?;
            if u64_at(target, 8) & 2 == 0 {
                continue;
            }
            if u32_at(target, 4) == 7 {
                return Err(Error::RelocationTarget);
            }
            let placement = self
                .layout
                .placements()
                .iter()
                .find(|p| u32::from(p.section_index) == target_index)
                .ok_or(Error::RelocationTarget)?;
            let symtab = u16::try_from(u32_at(rela, 40)).map_err(|_| Error::RelocationLink)?;
            let symtab_data = section(usize::from(symtab))?;
            let symbols = file_range(self.bytes, u64_at(symtab_data, 24), u64_at(symtab_data, 32))?;
            for entry in
                file_range(self.bytes, u64_at(rela, 24), u64_at(rela, 32))?.chunks_exact(24)
            {
                let info = u64_at(entry, 8);
                let kind = info as u32;
                if !matches!(kind, 2 | 4) {
                    continue;
                }
                let si = (info >> 32) as u32;
                let off = u64_at(entry, 0);
                if off.checked_add(4).ok_or(Error::RelocationRange)? > placement.size {
                    return Err(Error::RelocationRange);
                }
                let sy = usize::try_from(si)
                    .map_err(|_| Error::SymbolTable)?
                    .checked_mul(SYMBOL_SIZE)
                    .ok_or(Error::SymbolTable)?;
                let sym = symbols
                    .get(sy..sy.checked_add(SYMBOL_SIZE).ok_or(Error::SymbolTable)?)
                    .ok_or(Error::SymbolTable)?;
                let shndx = u16_at(sym, 6);
                if shndx == SHN_UNDEF {
                    let value = self.resolve(symtab, si)?;
                    imports += 1;
                    let po = placement
                        .address
                        .checked_sub(self.layout.base())
                        .and_then(|x| x.checked_add(off))
                        .ok_or(Error::RelocationRange)?;
                    let c = crate::relative32_base_window(
                        value,
                        u64_at(entry, 16) as i64,
                        po,
                        self.layout.span(),
                    );
                    window = match (window, c) {
                        (Some(a), Some(b)) => a.intersect(b),
                        _ => None,
                    };
                } else if shndx == SHN_ABS {
                    let value = self.resolve(symtab, si)?;
                    absolute += 1;
                    let po = placement
                        .address
                        .checked_sub(self.layout.base())
                        .and_then(|x| x.checked_add(off))
                        .ok_or(Error::RelocationRange)?;
                    let c = crate::relative32_base_window(
                        value,
                        u64_at(entry, 16) as i64,
                        po,
                        self.layout.span(),
                    );
                    window = match (window, c) {
                        (Some(a), Some(b)) => a.intersect(b),
                        _ => None,
                    };
                } else {
                    // Resolve enforces symbol class/binding/visibility/section validity.
                    let resolved = self.resolve(symtab, si)?;
                    locals += 1;
                    let place = placement
                        .address
                        .checked_add(off)
                        .ok_or(Error::RelocationRange)?;
                    // S and P include their different section offsets. Only the
                    // common module base cancels; st_value - r_offset is not enough.
                    let delta = i128::from(resolved) + i128::from(u64_at(entry, 16) as i64)
                        - i128::from(place);
                    if i32::try_from(delta).is_err() {
                        window = None
                    }
                }
            }
        }
        Ok(Relative32PlacementConstraints {
            window,
            rela_entries: total,
            import_relocations: imports,
            absolute_relocations: absolute,
            section_relative_relocations: locals,
        })
    }

    pub fn resolve(&self, symtab_index: u16, symbol_index: u32) -> Result<u64, Error> {
        let section_offset = usize::from(symtab_index)
            .checked_mul(SECTION_SIZE)
            .ok_or(Error::SymbolTable)?;
        let section = self
            .table
            .get(
                section_offset
                    ..section_offset
                        .checked_add(SECTION_SIZE)
                        .ok_or(Error::SymbolTable)?,
            )
            .ok_or(Error::SymbolTable)?;
        if u32_at(section, 4) != SHT_SYMTAB {
            return Err(Error::SymbolTable);
        }
        let sym_size = u64_at(section, 32);
        let symbols = file_range(self.bytes, u64_at(section, 24), sym_size)?;
        let index = usize::try_from(symbol_index).map_err(|_| Error::SymbolTable)?;
        let start = index.checked_mul(SYMBOL_SIZE).ok_or(Error::SymbolTable)?;
        let symbol = symbols
            .get(start..start.checked_add(SYMBOL_SIZE).ok_or(Error::SymbolTable)?)
            .ok_or(Error::SymbolTable)?;
        if symbol_index == 0 {
            return Err(Error::MissingSymbol);
        }
        let info = symbol[4];
        let binding = info >> 4;
        let kind = info & 0xf;
        if !matches!(binding, 0..=2) || !matches!(kind, 0..=3) {
            return Err(Error::UnsupportedSymbol);
        }
        let name_offset = u32_at(symbol, 0);
        let strings_index = u32_at(section, 40);
        let string_section_offset = usize::try_from(strings_index)
            .map_err(|_| Error::SymbolSection)?
            .checked_mul(SECTION_SIZE)
            .ok_or(Error::SymbolSection)?;
        let strings_section = self
            .table
            .get(
                string_section_offset
                    ..string_section_offset
                        .checked_add(SECTION_SIZE)
                        .ok_or(Error::SymbolSection)?,
            )
            .ok_or(Error::SymbolSection)?;
        if u32_at(strings_section, 4) != 3 {
            return Err(Error::SymbolTable);
        }
        let strings = file_range(
            self.bytes,
            u64_at(strings_section, 24),
            u64_at(strings_section, 32),
        )?;
        let name_start = usize::try_from(name_offset).map_err(|_| Error::SymbolName)?;
        let suffix = strings.get(name_start..).ok_or(Error::SymbolName)?;
        let name_end = suffix
            .iter()
            .position(|&byte| byte == 0)
            .ok_or(Error::SymbolName)?;
        let name = &suffix[..name_end];
        let value = u64_at(symbol, 8);
        match u16_at(symbol, 6) {
            SHN_UNDEF => {
                if binding == 0 || symbol[5] != 0 {
                    return Err(Error::UnsupportedSymbol);
                }
                self.exports
                    .binary_search_by(|entry| entry.name.cmp(name))
                    .map(|index| self.exports[index].value)
                    .map_err(|_| Error::MissingSymbol)
            }
            SHN_ABS => Ok(value),
            SHN_COMMON => Err(Error::UnsupportedSymbol),
            section_index => {
                let placement = self
                    .layout
                    .placements()
                    .iter()
                    .find(|p| p.section_index == section_index)
                    .ok_or(Error::SymbolSection)?;
                placement
                    .address
                    .checked_add(value)
                    .ok_or(Error::SymbolRange)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{vec, vec::Vec};

    use super::*;
    use crate::layout::plan_layout;

    // Compact ELF fixture: null, allocatable data, string table, symbol table.
    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 512];
        b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        put16(&mut b, 16, 1);
        put16(&mut b, 18, 62);
        put32(&mut b, 20, 1);
        put64(&mut b, 40, 64);
        put16(&mut b, 52, 64);
        put16(&mut b, 58, 64);
        put16(&mut b, 60, 4);
        sec(&mut b, 1, 1, 2, 16);
        put64(&mut b, 128 + 48, 8);
        sec(&mut b, 2, 3, 0, 16);
        put64(&mut b, 192 + 24, 320);
        put64(&mut b, 192 + 32, 16);
        b[320..336].copy_from_slice(b"\0defined\0undef\0\0");
        sec(&mut b, 3, 2, 0, 96);
        put64(&mut b, 256 + 24, 336);
        put64(&mut b, 256 + 32, 96);
        put32(&mut b, 256 + 40, 2);
        put32(&mut b, 256 + 44, 1);
        put64(&mut b, 256 + 48, 8);
        put64(&mut b, 256 + 56, 24);
        // null; defined global object in section 1; undefined global; undefined weak
        put32(&mut b, 360, 1);
        b[364] = 0x11;
        put16(&mut b, 366, 1);
        put64(&mut b, 368, 4);
        put32(&mut b, 384, 9);
        b[388] = 0x10;
        put32(&mut b, 408, 9);
        b[412] = 0x20;
        b
    }
    fn put16(b: &mut [u8], o: usize, v: u16) {
        b[o..o + 2].copy_from_slice(&v.to_le_bytes())
    }
    fn put32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes())
    }
    fn put64(b: &mut [u8], o: usize, v: u64) {
        b[o..o + 8].copy_from_slice(&v.to_le_bytes())
    }
    fn sec(b: &mut [u8], i: usize, t: u32, f: u64, size: u64) {
        let o = 64 + i * 64;
        put32(b, o + 4, t);
        put64(b, o + 8, f);
        put64(b, o + 32, size)
    }
    fn resolver<'a>(b: &'a [u8], ex: &[Export<'a>]) -> SymbolResolver<'a> {
        let l = std::boxed::Box::leak(std::boxed::Box::new(plan_layout(b, 0x1000, 16, 1).unwrap()));
        SymbolResolver::new(b, l, ex).unwrap()
    }

    #[test]
    fn resolves_defined_and_exported_undefined() {
        let b = fixture();
        let r = resolver(
            &b,
            &[Export {
                name: b"undef",
                value: 77,
            }],
        );
        assert_eq!(r.resolve(3, 1), Ok(0x1004));
        assert_eq!(r.resolve(3, 2), Ok(77));
    }
    #[test]
    fn weak_missing_and_explicit_zero() {
        let b = fixture();
        let r = resolver(&b, &[]);
        assert_eq!(r.resolve(3, 3), Err(Error::MissingSymbol));
        let r = resolver(
            &b,
            &[Export {
                name: b"undef",
                value: 0,
            }],
        );
        assert_eq!(r.resolve(3, 2), Ok(0));
    }
    #[test]
    fn absolute_and_common() {
        let mut b = fixture();
        put16(&mut b, 366, 0xfff1);
        put64(&mut b, 368, 99);
        let r = resolver(&b, &[]);
        assert_eq!(r.resolve(3, 1), Ok(99));
        put16(&mut b, 366, 0xfff2);
        let r = resolver(&b, &[]);
        assert_eq!(r.resolve(3, 1), Err(Error::UnsupportedSymbol));
    }
    #[test]
    fn rejects_tls_ifunc_reserved_other_bits_and_nul_export_names() {
        for kind in [6, 10] {
            let mut b = fixture();
            b[364] = 0x10 | kind;
            assert_eq!(
                resolver(&b, &[]).resolve(3, 1),
                Err(Error::UnsupportedSymbol)
            );
        }
        let mut b = fixture();
        b[389] = 0x80;
        assert_eq!(
            resolver(
                &b,
                &[Export {
                    name: b"undef",
                    value: 9
                }]
            )
            .resolve(3, 2),
            Err(Error::UnsupportedSymbol)
        );
        let b = fixture();
        let layout = plan_layout(&b, 0x1000, 16, 1).unwrap();
        assert_eq!(
            SymbolResolver::new(
                &b,
                &layout,
                &[Export {
                    name: b"bad\0name",
                    value: 1
                }]
            )
            .err(),
            Some(Error::SymbolName)
        );
    }
    #[test]
    fn index_checks_and_null_symbol() {
        let b = fixture();
        let r = resolver(&b, &[]);
        assert_eq!(r.resolve(9, 1), Err(Error::SymbolTable));
        assert_eq!(r.resolve(3, 8), Err(Error::SymbolTable));
        assert_eq!(r.resolve(3, 0), Err(Error::MissingSymbol));
    }
    #[test]
    fn unsupported_types_binding_and_undefined_visibility() {
        let mut b = fixture();
        b[364] = 0x14;
        assert_eq!(
            resolver(&b, &[]).resolve(3, 1),
            Err(Error::UnsupportedSymbol)
        );
        let mut b = fixture();
        b[388] = 0x30;
        assert_eq!(
            resolver(&b, &[]).resolve(3, 2),
            Err(Error::UnsupportedSymbol)
        );
        let mut b = fixture();
        b[389] = 1;
        assert_eq!(
            resolver(
                &b,
                &[Export {
                    name: b"undef",
                    value: 4
                }]
            )
            .resolve(3, 2),
            Err(Error::UnsupportedSymbol)
        );
    }
    #[test]
    fn undefined_local_does_not_resolve_from_global_exports() {
        let mut b = fixture();
        put32(&mut b, 256 + 44, 2);
        b[364] = 0;
        put16(&mut b, 366, SHN_UNDEF);
        let r = resolver(
            &b,
            &[Export {
                name: b"defined",
                value: 9,
            }],
        );
        assert_eq!(r.resolve(3, 1), Err(Error::UnsupportedSymbol));
    }
    #[test]
    fn duplicate_and_bad_names() {
        let b = fixture();
        let l = plan_layout(&b, 0x1000, 16, 1).unwrap();
        assert_eq!(
            SymbolResolver::new(
                &b,
                &l,
                &[
                    Export {
                        name: b"x",
                        value: 1
                    },
                    Export {
                        name: b"x",
                        value: 1
                    }
                ]
            )
            .err(),
            Some(Error::DuplicateExport)
        );
        assert_eq!(
            SymbolResolver::new(
                &b,
                &l,
                &[Export {
                    name: b"",
                    value: 1
                }]
            )
            .err(),
            Some(Error::SymbolName)
        );
    }
    #[test]
    fn layout_must_match_structurally() {
        let b = fixture();
        let mut other = fixture();
        put64(&mut other, 128 + 32, 8);
        let wrong = plan_layout(&other, 0x1000, 16, 1).unwrap();
        assert_eq!(
            SymbolResolver::new(&b, &wrong, &[]).err(),
            Some(Error::LayoutMismatch)
        );
    }
    #[test]
    fn unallocated_definition() {
        let mut b = fixture();
        sec(&mut b, 1, 1, 0, 16);
        let l = plan_layout(&b, 0x1000, 0, 0).unwrap();
        let r = SymbolResolver::new(&b, &l, &[]).unwrap();
        assert_eq!(r.resolve(3, 1), Err(Error::SymbolSection));
    }
    #[test]
    fn export_cap_and_truncation_mutations_do_not_panic() {
        let b = fixture();
        let l = plan_layout(&b, 0x1000, 16, 1).unwrap();
        let many = vec![
            Export {
                name: b"x",
                value: 1
            };
            4097
        ];
        assert_eq!(
            SymbolResolver::new(&b, &l, &many).err(),
            Some(Error::ResourceLimit)
        );
        for n in 0..b.len() {
            if let Ok(r) = SymbolResolver::new(&b[..n], &l, &[]) {
                let _ = r.resolve(3, 1);
                let _ = r.resolve(u16::MAX, u32::MAX);
            }
        }
        for i in 0..b.len() {
            let mut m = b.clone();
            m[i] ^= 0xff;
            if let Ok(r) = SymbolResolver::new(&m, &l, &[]) {
                for symbol in [0, 1, 2, 3, u32::MAX] {
                    let _ = r.resolve(3, symbol);
                }
            }
        }
    }
}
