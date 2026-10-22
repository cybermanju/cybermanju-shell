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
