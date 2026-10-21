// SPDX-License-Identifier: MPL-2.0
//! RELA structural checks only. No symbol resolution or memory writes.
use super::{Error, SECTION_SIZE, file_range, u32_at, u64_at};

const RELA_SIZE: u64 = 24;
const SYMBOL_SIZE: u64 = 24;

fn section_at(table: &[u8], index: u32) -> Option<&[u8]> {
    if index == 0 {
        return None;
    }
    let start = usize::try_from(index).ok()?.checked_mul(SECTION_SIZE)?;
    table.get(start..start.checked_add(SECTION_SIZE)?)
}

/// Widths for a deliberately narrow subset of x86-64 relocations.
/// Recognizing a width does not implement its relocation formula.
fn width(kind: u32) -> Result<u64, Error> {
    match kind {
        0 => Ok(0),               // R_X86_64_NONE (no write)
        1 | 24 => Ok(8),          // R_X86_64_64, R_X86_64_PC64
        2 | 4 | 10 | 11 => Ok(4), // PC32, PLT32, 32, 32S
        _ => Err(Error::UnsupportedRelocation),
    }
}

pub(super) fn validate(bytes: &[u8], table: &[u8]) -> Result<(), Error> {
    // Called only after all section ranges and symbol tables are validated.
    for section in table.chunks_exact(SECTION_SIZE).skip(1) {
        match u32_at(section, 4) {
            9 | 19 => return Err(Error::UnsupportedRelocation), // REL and RELR
            4 => validate_rela(bytes, table, section)?,
            _ => {}
        }
    }
    Ok(())
}

fn validate_rela(bytes: &[u8], table: &[u8], section: &[u8]) -> Result<(), Error> {
    let size = u64_at(section, 32);
    if u64_at(section, 56) != RELA_SIZE || size % RELA_SIZE != 0 {
        return Err(Error::RelocationTable);
    }
    let symtab = section_at(table, u32_at(section, 40)).ok_or(Error::RelocationLink)?;
    if u32_at(symtab, 4) != 2 {
        return Err(Error::RelocationLink);
    }
    let symbol_count = u64_at(symtab, 32) / SYMBOL_SIZE;
    let target = section_at(table, u32_at(section, 44)).ok_or(Error::RelocationTarget)?;
    let target_size = u64_at(target, 32);
    let entries = file_range(bytes, u64_at(section, 24), size)?;
    for entry in entries.chunks_exact(RELA_SIZE as usize) {
        let info = u64_at(entry, 8);
        if info >> 32 >= symbol_count {
            return Err(Error::RelocationSymbol);
        }
        let bytes_to_write = width(info as u32)?;
        let end = u64_at(entry, 0)
            .checked_add(bytes_to_write)
            .ok_or(Error::RelocationRange)?;
        // NONE is conservatively bounded as a zero-byte operation. SHT_NOBITS
        // uses its memory size, not a file range. Addends are not evaluated.
        if end > target_size {
            return Err(Error::RelocationRange);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Error, inspect};
    use std::{vec, vec::Vec};

    const TEXT: usize = 128;
    const STRINGS: usize = 192;
    const SYMTAB: usize = 256;
    const RELA: usize = 320;
    const SYMBOLS: usize = 416;
    const ENTRIES: usize = 464;

    fn put16(bytes: &mut [u8], at: usize, value: u16) {
        bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn put32(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn put64(bytes: &mut [u8], at: usize, value: u64) {
        bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; ENTRIES + 24];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        put16(&mut bytes, 16, 1);
        put16(&mut bytes, 18, 62);
        put32(&mut bytes, 20, 1);
        put64(&mut bytes, 40, 64);
        put16(&mut bytes, 52, 64);
        put16(&mut bytes, 58, 64);
        put16(&mut bytes, 60, 5);
        put32(&mut bytes, TEXT + 4, 1);
        put64(&mut bytes, TEXT + 24, 384);
        put64(&mut bytes, TEXT + 32, 16);
        put32(&mut bytes, STRINGS + 4, 3);
        put64(&mut bytes, STRINGS + 24, 400);
        put64(&mut bytes, STRINGS + 32, 10);
        bytes[400..410].copy_from_slice(b"\0external\0");
        put32(&mut bytes, SYMTAB + 4, 2);
        put64(&mut bytes, SYMTAB + 24, SYMBOLS as u64);
        put64(&mut bytes, SYMTAB + 32, 48);
        put32(&mut bytes, SYMTAB + 40, 2);
        put32(&mut bytes, SYMTAB + 44, 1);
        put64(&mut bytes, SYMTAB + 48, 8);
        put64(&mut bytes, SYMTAB + 56, 24);
        put32(&mut bytes, SYMBOLS + 24, 1);
        bytes[SYMBOLS + 24 + 4] = 0x10; // global undefined symbol
        put32(&mut bytes, RELA + 4, 4);
        put64(&mut bytes, RELA + 24, ENTRIES as u64);
        put64(&mut bytes, RELA + 32, 24);
        put32(&mut bytes, RELA + 40, 3);
        put32(&mut bytes, RELA + 44, 1);
        put64(&mut bytes, RELA + 48, 8);
        put64(&mut bytes, RELA + 56, 24);
        put64(&mut bytes, ENTRIES, 12);
        put64(&mut bytes, ENTRIES + 8, (1_u64 << 32) | 2);
        put64(&mut bytes, ENTRIES + 16, (-4_i64) as u64);
        bytes
    }

    fn apply(kind: u32, symbol: u64, addend: i64, base: u64) -> Result<Vec<u8>, Error> {
        let mut bytes = fixture();
        put64(&mut bytes, ENTRIES, 0);
        put64(&mut bytes, ENTRIES + 8, (1_u64 << 32) | u64::from(kind));
        put64(&mut bytes, ENTRIES + 16, addend as u64);
        crate::relocate_section(&bytes, 4, base, &[None, Some(symbol)], 16)
    }

    #[test]
    fn applies_all_formulas_with_explicit_numeric_addresses() {
        for (kind, expected, width) in [
            (1, 0x1ffc_u64, 8),
            (2, 0xffc, 4),
            (4, 0xffc, 4),
            (10, 0x1ffc, 4),
            (11, 0x1ffc, 4),
            (24, 0xffc, 8),
        ] {
            let result = apply(kind, 0x2000, -4, 0x1000).unwrap();
            assert_eq!(&result[..width], &expected.to_le_bytes()[..width]);
            assert!(result[width..].iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn pc_relative_place_includes_entry_offset() {
        let bytes = fixture();
        let before = bytes.clone();
        let result = crate::relocate_section(&bytes, 4, 0x1000, &[None, Some(0x1000)], 16).unwrap();
        assert_eq!(&result[12..], &(-16_i32).to_le_bytes());
        assert_eq!(bytes, before);
    }

    #[test]
    fn signed_and_unsigned_boundaries_are_not_truncated() {
        for (kind, symbol, addend, base) in [
            (1, u64::MAX, 1, 0),
            (1, 0, -1, 0),
            (10, u32::MAX as u64 + 1, 0, 0),
            (10, 0, -1, 0),
            (11, i32::MAX as u64 + 1, 0, 0),
            (11, 0, i32::MIN as i64 - 1, 0),
            (2, i32::MAX as u64 + 1, 0, 0),
            (4, 0, 0, i32::MAX as u64 + 2),
            (24, u64::MAX, 0, 0),
            (24, 0, -1, i64::MAX as u64 + 1),
        ] {
            assert_eq!(
                apply(kind, symbol, addend, base),
                Err(Error::RelocationOverflow)
            );
        }
        assert!(apply(1, u64::MAX, 0, 0).is_ok());
        assert!(apply(10, u32::MAX as u64, 0, 0).is_ok());
        assert!(apply(11, 0, i32::MIN as i64, 0).is_ok());
        assert!(apply(2, i32::MAX as u64, 0, 0).is_ok());
        assert!(apply(24, 0, i64::MIN, 0).is_ok());
    }

    #[test]
    fn missing_symbols_fail_without_modifying_input() {
        let bytes = fixture();
        let before = bytes.clone();
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[None, None], 16),
            Err(Error::MissingSymbol)
        );
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[], 16),
            Err(Error::RelocationSymbol)
        );
        assert_eq!(bytes, before);
    }

    #[test]
    fn none_needs_no_symbol_and_preserves_target_data() {
        let mut bytes = fixture();
        bytes[384] = 0xaa;
        put64(&mut bytes, ENTRIES + 8, 0);
        let result = crate::relocate_section(&bytes, 4, 0, &[None, None], 16).unwrap();
        assert_eq!(&result, &bytes[384..400]);
    }

    #[test]
    fn enforces_entry_cap_and_extreme_none_offset_without_allocating_target() {
        let mut bytes = fixture();
        bytes.resize(ENTRIES + 65_537 * 24, 0);
        bytes[ENTRIES..].fill(0); // NONE entries, no symbol resolution or writes
        put64(&mut bytes, RELA + 32, 65_536 * 24);
        assert!(crate::relocate_section(&bytes, 4, 0, &[None, None], 16).is_ok());
        put64(&mut bytes, RELA + 32, 65_537 * 24);
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[None, None], 16),
            Err(Error::ResourceLimit)
        );
        put64(&mut bytes, RELA + 32, 24);
        put64(&mut bytes, ENTRIES, u64::MAX);
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[None, None], 16),
            Err(Error::RelocationRange)
        );
        put64(&mut bytes, ENTRIES, 0);
        put32(&mut bytes, TEXT + 4, 8);
        put64(&mut bytes, TEXT + 32, u64::MAX);
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[None, None], 16),
            Err(Error::ResourceLimit)
        );
    }

    #[test]
    fn weak_absolute_and_common_symbols_require_explicit_values() {
        for shndx in [0, 0xfff1, 0xfff2] {
            let mut bytes = fixture();
            bytes[SYMBOLS + 24 + 4] = 0x20; // STB_WEAK
            put16(&mut bytes, SYMBOLS + 24 + 6, shndx);
            assert_eq!(
                crate::relocate_section(&bytes, 4, 0, &[None, None], 16),
                Err(Error::MissingSymbol)
            );
            let result = crate::relocate_section(&bytes, 4, 0, &[None, Some(20)], 16).unwrap();
            assert_eq!(&result[12..], &4_i32.to_le_bytes());
        }
    }

    #[test]
    fn nobits_output_is_zero_initialized_before_patching() {
        let mut bytes = fixture();
        put32(&mut bytes, TEXT + 4, 8);
        put64(&mut bytes, TEXT + 24, u64::MAX);
        let result = crate::relocate_section(&bytes, 4, 0, &[None, Some(20)], 16).unwrap();
        assert_eq!(&result[12..], &4_i32.to_le_bytes());
        assert_eq!(&result[..12], &[0; 12]);
    }

    #[test]
    fn rejects_bad_application_target_index_size_and_address() {
        let bytes = fixture();
        for index in [0, 1, 5, u16::MAX] {
            assert_eq!(
                crate::relocate_section(&bytes, index, 0, &[None, Some(0)], 16),
                Err(Error::RelocationTable)
            );
        }
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[None, Some(0)], 15),
            Err(Error::ResourceLimit)
        );
        assert_eq!(
            crate::relocate_section(&bytes, 4, u64::MAX - 15, &[None, Some(0)], 16),
            Err(Error::RelocationOverflow)
        );
        let mut bytes = bytes;
        put32(&mut bytes, TEXT + 4, 7); // SHT_NOTE: valid container, unsupported target
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[None, Some(0)], 16),
            Err(Error::RelocationTarget)
        );
    }

    #[test]
    fn rejects_overlapping_patches_but_accepts_adjacent_unsorted_patches() {
        let mut bytes = fixture();
        bytes.extend_from_within(ENTRIES..ENTRIES + 24);
        put64(&mut bytes, RELA + 32, 48);
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[None, Some(20)], 16),
            Err(Error::RelocationOverlap)
        );
        put64(&mut bytes, ENTRIES + 24, 8);
        let result = crate::relocate_section(&bytes, 4, 0, &[None, Some(20)], 16).unwrap();
        assert_eq!(&result[8..12], &8_i32.to_le_bytes());
        assert_eq!(&result[12..], &4_i32.to_le_bytes());
        put64(&mut bytes, ENTRIES + 24 + 16, i64::MIN as u64);
        assert_eq!(
            crate::relocate_section(&bytes, 4, 0, &[None, Some(20)], 16),
            Err(Error::RelocationOverflow)
        );
    }

    #[test]
    fn application_mutations_and_truncations_do_not_panic() {
        let original = fixture();
        for end in 0..original.len() {
            assert!(
                crate::relocate_section(&original[..end], 4, 0, &[None, Some(20)], 16).is_err()
            );
        }
        for index in 0..original.len() {
            for value in [0, 1, 0x7f, 0xff] {
                let mut bytes = original.clone();
                bytes[index] = value;
                let _ = crate::relocate_section(&bytes, 4, 0, &[None, Some(20)], 16);
            }
        }
    }

    #[test]
    fn accepts_pc32_at_boundary_without_resolving_or_writing() {
        assert!(inspect(&fixture()).is_ok());
    }

    #[test]
    fn accepts_each_recognized_width_and_negative_addend() {
        for (kind, width) in [(0, 0), (1, 8), (2, 4), (4, 4), (10, 4), (11, 4), (24, 8)] {
            let mut bytes = fixture();
            put64(&mut bytes, ENTRIES, 16 - width);
            put64(&mut bytes, ENTRIES + 8, (1_u64 << 32) | kind);
            assert!(inspect(&bytes).is_ok(), "kind {kind}");
        }
    }

    #[test]
    fn rejects_rela_entry_size_and_partial_entry() {
        for (field, value) in [(56, 0), (56, 16), (32, 23)] {
            let mut bytes = fixture();
            put64(&mut bytes, RELA + field, value);
            assert_eq!(inspect(&bytes), Err(Error::RelocationTable));
        }
    }

    #[test]
    fn permits_empty_rela_with_valid_references() {
        let mut bytes = fixture();
        put64(&mut bytes, RELA + 32, 0);
        assert!(inspect(&bytes).is_ok());
    }

    #[test]
    fn rejects_missing_wrong_or_out_of_range_symbol_table_link() {
        for index in [0, 1, 2, 5, u32::MAX] {
            let mut bytes = fixture();
            put32(&mut bytes, RELA + 40, index);
            assert_eq!(inspect(&bytes), Err(Error::RelocationLink));
        }
    }

    #[test]
    fn rejects_missing_or_out_of_range_target_section() {
        for index in [0, 5, u32::MAX] {
            let mut bytes = fixture();
            put32(&mut bytes, RELA + 44, index);
            assert_eq!(inspect(&bytes), Err(Error::RelocationTarget));
        }
    }

    #[test]
    fn rejects_symbol_index_at_count_and_overflow_index() {
        for index in [2_u64, u32::MAX as u64] {
            let mut bytes = fixture();
            put64(&mut bytes, ENTRIES + 8, (index << 32) | 2);
            assert_eq!(inspect(&bytes), Err(Error::RelocationSymbol));
        }
    }

    #[test]
    fn validates_references_even_for_empty_tables() {
        let mut bytes = fixture();
        put64(&mut bytes, RELA + 32, 0);
        put32(&mut bytes, RELA + 40, 0);
        assert_eq!(inspect(&bytes), Err(Error::RelocationLink));
    }

    #[test]
    fn rejects_write_past_target_and_overflow() {
        for offset in [13, 16, u64::MAX] {
            let mut bytes = fixture();
            put64(&mut bytes, ENTRIES, offset);
            assert_eq!(inspect(&bytes), Err(Error::RelocationRange));
        }
    }

    #[test]
    fn none_is_zero_width_but_offset_still_bounded_by_policy() {
        let mut bytes = fixture();
        put64(&mut bytes, ENTRIES + 8, 0);
        put64(&mut bytes, ENTRIES, 16);
        assert!(inspect(&bytes).is_ok());
        put64(&mut bytes, ENTRIES, 17);
        assert_eq!(inspect(&bytes), Err(Error::RelocationRange));
    }

    #[test]
    fn rejects_unsupported_relocation_kinds_and_encodings() {
        for kind in [3, 9, 19, u32::MAX as u64] {
            let mut bytes = fixture();
            put64(&mut bytes, ENTRIES + 8, (1_u64 << 32) | kind);
            assert_eq!(inspect(&bytes), Err(Error::UnsupportedRelocation));
        }
        for section_type in [9, 19] {
            let mut bytes = fixture();
            put32(&mut bytes, RELA + 4, section_type);
            assert_eq!(inspect(&bytes), Err(Error::UnsupportedRelocation));
        }
    }

    #[test]
    fn accepts_nobits_target_using_memory_bounds_not_file_bytes() {
        let mut bytes = fixture();
        put32(&mut bytes, TEXT + 4, 8);
        put64(&mut bytes, TEXT + 24, u64::MAX);
        assert!(inspect(&bytes).is_ok());
    }

    #[test]
    fn inspection_does_not_modify_input_or_evaluate_addends() {
        let mut bytes = fixture();
        put64(&mut bytes, ENTRIES + 16, i64::MIN as u64);
        let before = bytes.clone();
        assert!(inspect(&bytes).is_ok());
        assert_eq!(before, bytes);
    }

    #[test]
    fn relocation_fixture_mutations_and_truncations_never_panic() {
        let original = fixture();
        for end in 0..original.len() {
            assert!(inspect(&original[..end]).is_err());
        }
        for index in 0..original.len() {
            for value in [0, 1, 0x7f, 0xff] {
                let mut bytes = original.clone();
                bytes[index] = value;
                let _ = inspect(&bytes);
            }
        }
    }
}
