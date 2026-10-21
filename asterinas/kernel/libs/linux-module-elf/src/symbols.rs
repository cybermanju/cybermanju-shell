// SPDX-License-Identifier: MPL-2.0
//! Symbol structure only: no export lookup, relocation or ABI compatibility.
use super::{Error, SECTION_SIZE, file_range, u16_at, u32_at, u64_at};

const SYMBOL_SIZE: usize = 24;

fn section_at(table: &[u8], index: u32) -> Result<&[u8], Error> {
    let index = usize::try_from(index).map_err(|_| Error::SymbolSection)?;
    let start = index
        .checked_mul(SECTION_SIZE)
        .ok_or(Error::SymbolSection)?;
    let end = start
        .checked_add(SECTION_SIZE)
        .ok_or(Error::SymbolSection)?;
    table.get(start..end).ok_or(Error::SymbolSection)
}

pub(super) fn validate(bytes: &[u8], table: &[u8]) -> Result<(), Error> {
    for section in table.chunks_exact(SECTION_SIZE).skip(1) {
        match u32_at(section, 4) {
            // Dynamic and extended-index symbol tables are not supported.
            11 | 18 => return Err(Error::UnsupportedSymbolTable),
            2 => validate_table(bytes, table, section)?,
            _ => {}
        }
    }
    Ok(())
}

fn validate_table(bytes: &[u8], table: &[u8], section: &[u8]) -> Result<(), Error> {
    let size = u64_at(section, 32);
    if u64_at(section, 56) != SYMBOL_SIZE as u64 || size == 0 || size % SYMBOL_SIZE as u64 != 0 {
        return Err(Error::SymbolTable);
    }
    let symbols = file_range(bytes, u64_at(section, 24), size)?;
    let first_nonlocal = u32_at(section, 44);
    if first_nonlocal == 0 || u64::from(first_nonlocal) > size / SYMBOL_SIZE as u64 {
        return Err(Error::SymbolBinding);
    }
    let strings_section = section_at(table, u32_at(section, 40))?;
    if u32_at(strings_section, 4) != 3 {
        return Err(Error::SymbolTable);
    }
    let strings = file_range(
        bytes,
        u64_at(strings_section, 24),
        u64_at(strings_section, 32),
    )?;
    if strings.first() != Some(&0) || strings.last() != Some(&0) {
        return Err(Error::SymbolTable);
    }
    if symbols[..SYMBOL_SIZE].iter().any(|&byte| byte != 0) {
        return Err(Error::SymbolTable);
    }
    for (index, symbol) in symbols.chunks_exact(SYMBOL_SIZE).enumerate() {
        let name = usize::try_from(u32_at(symbol, 0)).map_err(|_| Error::SymbolName)?;
        // Every in-range suffix terminates because the last byte is NUL.
        if name >= strings.len() {
            return Err(Error::SymbolName);
        }
        let is_local = symbol[4] >> 4 == 0;
        if is_local != (index < first_nonlocal as usize) {
            return Err(Error::SymbolBinding);
        }
        let section_index = u16_at(symbol, 6);
        match section_index {
            // Undefined, absolute and common symbols have no section-relative
            // byte range. Their value/size semantics are deferred to the loader.
            0 | 0xfff1 | 0xfff2 => {}
            0xff00..=0xffff => return Err(Error::SymbolSection),
            _ => {
                let target = section_at(table, u32::from(section_index))?;
                let end = u64_at(symbol, 8)
                    .checked_add(u64_at(symbol, 16))
                    .ok_or(Error::SymbolRange)?;
                if end > u64_at(target, 32) {
                    return Err(Error::SymbolRange);
                }
            }
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
    const SYMBOLS: usize = 400;

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
        let mut bytes = vec![0; SYMBOLS + 3 * 24];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        put16(&mut bytes, 16, 1);
        put16(&mut bytes, 18, 62);
        put32(&mut bytes, 20, 1);
        put64(&mut bytes, 40, 64);
        put16(&mut bytes, 52, 64);
        put16(&mut bytes, 58, 64);
        put16(&mut bytes, 60, 4);
        put32(&mut bytes, TEXT + 4, 1);
        put64(&mut bytes, TEXT + 24, 320);
        put64(&mut bytes, TEXT + 32, 16);
        put32(&mut bytes, STRINGS + 4, 3);
        put64(&mut bytes, STRINGS + 24, 336);
        let names = b"\0local\0external\0";
        put64(&mut bytes, STRINGS + 32, names.len() as u64);
        bytes[336..336 + names.len()].copy_from_slice(names);
        put32(&mut bytes, SYMTAB + 4, 2);
        put64(&mut bytes, SYMTAB + 24, SYMBOLS as u64);
        put64(&mut bytes, SYMTAB + 32, 3 * 24);
        put32(&mut bytes, SYMTAB + 40, 2);
        put32(&mut bytes, SYMTAB + 44, 2);
        put64(&mut bytes, SYMTAB + 48, 8);
        put64(&mut bytes, SYMTAB + 56, 24);
        put32(&mut bytes, SYMBOLS + 24, 1);
        bytes[SYMBOLS + 24 + 4] = 2; // local function
        put16(&mut bytes, SYMBOLS + 24 + 6, 1);
        put64(&mut bytes, SYMBOLS + 24 + 16, 4);
        put32(&mut bytes, SYMBOLS + 48, 7);
        bytes[SYMBOLS + 48 + 4] = 0x10; // global undefined
        bytes
    }

    #[test]
    fn accepts_local_defined_and_global_undefined_symbols() {
        assert!(inspect(&fixture()).is_ok());
    }

    #[test]
    fn rejects_bad_entry_size_empty_and_partial_tables() {
        for (field, value) in [(56, 0), (56, 16), (32, 0), (32, 71)] {
            let mut bytes = fixture();
            put64(&mut bytes, SYMTAB + field, value);
            assert_eq!(inspect(&bytes), Err(Error::SymbolTable));
        }
    }

    #[test]
    fn rejects_missing_and_wrong_string_links() {
        for (index, expected) in [
            (0, Error::SymbolTable),
            (1, Error::SymbolTable),
            (4, Error::SymbolSection),
            (u32::MAX, Error::SymbolSection),
        ] {
            let mut bytes = fixture();
            put32(&mut bytes, SYMTAB + 40, index);
            assert_eq!(inspect(&bytes), Err(expected));
        }
    }

    #[test]
    fn rejects_bad_symbol_string_tables_and_names() {
        for offset in [336, 336 + b"\0local\0external\0".len() - 1] {
            let mut bytes = fixture();
            bytes[offset] = 1;
            assert_eq!(inspect(&bytes), Err(Error::SymbolTable));
        }
        let mut bytes = fixture();
        put32(&mut bytes, SYMBOLS + 24, u32::MAX);
        assert_eq!(inspect(&bytes), Err(Error::SymbolName));
    }

    #[test]
    fn rejects_nonzero_first_symbol() {
        for offset in 0..24 {
            let mut bytes = fixture();
            bytes[SYMBOLS + offset] = 1;
            assert_eq!(inspect(&bytes), Err(Error::SymbolTable));
        }
    }

    #[test]
    fn rejects_bad_local_symbol_partition() {
        for first in [0, 1, 3, 4, u32::MAX] {
            let mut bytes = fixture();
            put32(&mut bytes, SYMTAB + 44, first);
            assert_eq!(inspect(&bytes), Err(Error::SymbolBinding));
        }
        let mut bytes = fixture();
        bytes[SYMBOLS + 24 + 4] = 0x12;
        assert_eq!(inspect(&bytes), Err(Error::SymbolBinding));
    }

    #[test]
    fn rejects_invalid_and_extended_symbol_section_indexes() {
        for index in [4, 0xff00, 0xffff] {
            let mut bytes = fixture();
            put16(&mut bytes, SYMBOLS + 24 + 6, index);
            assert_eq!(inspect(&bytes), Err(Error::SymbolSection));
        }
    }

    #[test]
    fn rejects_symbol_overflow_and_out_of_section_ranges() {
        for (value, size) in [(15, 2), (u64::MAX, 1), (17, 0)] {
            let mut bytes = fixture();
            put64(&mut bytes, SYMBOLS + 24 + 8, value);
            put64(&mut bytes, SYMBOLS + 24 + 16, size);
            assert_eq!(inspect(&bytes), Err(Error::SymbolRange));
        }
        let mut bytes = fixture();
        put64(&mut bytes, SYMBOLS + 24 + 8, 16);
        put64(&mut bytes, SYMBOLS + 24 + 16, 0);
        assert!(inspect(&bytes).is_ok());
    }

    #[test]
    fn accepts_absolute_and_common_without_claiming_resolution() {
        for index in [0xfff1, 0xfff2] {
            let mut bytes = fixture();
            put16(&mut bytes, SYMBOLS + 24 + 6, index);
            put64(&mut bytes, SYMBOLS + 24 + 8, u64::MAX);
            assert!(inspect(&bytes).is_ok());
        }
    }

    #[test]
    fn rejects_dynamic_and_extended_index_tables() {
        for kind in [11, 18] {
            let mut bytes = fixture();
            put32(&mut bytes, SYMTAB + 4, kind);
            assert_eq!(inspect(&bytes), Err(Error::UnsupportedSymbolTable));
        }
    }

    #[test]
    fn nobits_symbol_ranges_use_memory_size() {
        let mut bytes = fixture();
        put32(&mut bytes, TEXT + 4, 8);
        put64(&mut bytes, TEXT + 24, u64::MAX);
        assert!(inspect(&bytes).is_ok());
    }

    #[test]
    fn symbol_fixture_mutations_and_truncations_never_panic() {
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
