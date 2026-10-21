// SPDX-License-Identifier: MPL-2.0
//! Structural preflight for uncompressed ELF64 little-endian x86-64 ET_REL files.
//!
//! Success does NOT mean Linux module compatibility or authorization to execute.
//! Offline relocation arithmetic uses explicit symbol values and returns heap data.
//! Kernel symbol resolution, Linux ABI/versioning, signatures and executable
//! mappings remain future work. This crate neither loads nor executes code.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod apply;
mod relocations;
mod symbols;

pub use apply::relocate_section;

const HEADER_SIZE: usize = 64;
const SECTION_SIZE: usize = 64;

/// A structural error; unsupported encodings fail closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Truncated,
    Identification,
    ObjectType,
    Architecture,
    Header,
    ExtendedNumbering,
    SectionTable,
    NullSection,
    SectionRange,
    SectionAlignment,
    CompressedSection,
    StringTable,
    SectionName,
    SymbolTable,
    SymbolName,
    SymbolSection,
    SymbolRange,
    SymbolBinding,
    UnsupportedSymbolTable,
    RelocationTable,
    RelocationLink,
    RelocationTarget,
    RelocationSymbol,
    RelocationRange,
    UnsupportedRelocation,
    MissingSymbol,
    RelocationOverflow,
    RelocationOverlap,
    ResourceLimit,
}

/// An inspected object, not a loaded or trusted Linux module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Inspection {
    pub section_count: u16,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn file_range(bytes: &[u8], offset: u64, size: u64) -> Result<&[u8], Error> {
    let end = offset.checked_add(size).ok_or(Error::SectionRange)?;
    let start = usize::try_from(offset).map_err(|_| Error::SectionRange)?;
    let end = usize::try_from(end).map_err(|_| Error::SectionRange)?;
    bytes.get(start..end).ok_or(Error::SectionRange)
}

/// Validate only the container structure of a possible module.
///
/// Supports no program headers, extended section numbering, or compressed
/// sections. SHT_NOBITS has no file payload and is deliberately not read.
/// OSABI and Linux-specific metadata are not interpreted at this stage.
pub fn inspect(bytes: &[u8]) -> Result<Inspection, Error> {
    if bytes.len() < HEADER_SIZE {
        return Err(Error::Truncated);
    }
    if bytes[..7] != *b"\x7fELF\x02\x01\x01" {
        return Err(Error::Identification);
    }
    if u16_at(bytes, 16) != 1 {
        return Err(Error::ObjectType);
    }
    if u16_at(bytes, 18) != 62 {
        return Err(Error::Architecture);
    }
    if u32_at(bytes, 20) != 1
        || u64_at(bytes, 24) != 0
        || u64_at(bytes, 32) != 0
        || u32_at(bytes, 48) != 0
        || u16_at(bytes, 52) != HEADER_SIZE as u16
        || u16_at(bytes, 54) != 0
        || u16_at(bytes, 56) != 0
        || u16_at(bytes, 58) != SECTION_SIZE as u16
    {
        return Err(Error::Header);
    }
    let section_count = u16_at(bytes, 60);
    let names_index = u16_at(bytes, 62);
    if section_count == 0 || section_count >= 0xff00 || names_index >= 0xff00 {
        return Err(Error::ExtendedNumbering);
    }
    let table_offset = u64_at(bytes, 40);
    if table_offset < HEADER_SIZE as u64 || table_offset % 8 != 0 {
        return Err(Error::SectionTable);
    }
    let table = file_range(
        bytes,
        table_offset,
        u64::from(section_count) * SECTION_SIZE as u64,
    )
    .map_err(|_| Error::SectionTable)?;
    if table[..SECTION_SIZE].iter().any(|&byte| byte != 0) {
        return Err(Error::NullSection);
    }
    for section in table.chunks_exact(SECTION_SIZE).skip(1) {
        let alignment = u64_at(section, 48);
        if alignment != 0 && !alignment.is_power_of_two() {
            return Err(Error::SectionAlignment);
        }
        if u64_at(section, 8) & 0x800 != 0 {
            return Err(Error::CompressedSection);
        }
        if u32_at(section, 4) == 0 {
            return Err(Error::NullSection);
        }
        if u32_at(section, 4) != 8 {
            file_range(bytes, u64_at(section, 24), u64_at(section, 32))?;
        }
    }
    if names_index == 0 {
        if table.chunks_exact(SECTION_SIZE).any(|s| u32_at(s, 0) != 0) {
            return Err(Error::SectionName);
        }
    } else {
        if names_index >= section_count {
            return Err(Error::StringTable);
        }
        let offset = usize::from(names_index) * SECTION_SIZE;
        let section = &table[offset..offset + SECTION_SIZE];
        if u32_at(section, 4) != 3 {
            return Err(Error::StringTable);
        }
        let names = file_range(bytes, u64_at(section, 24), u64_at(section, 32))?;
        if names.first() != Some(&0) || names.last() != Some(&0) {
            return Err(Error::StringTable);
        }
        for section in table.chunks_exact(SECTION_SIZE) {
            let start = usize::try_from(u32_at(section, 0)).map_err(|_| Error::SectionName)?;
            // The last byte was checked to be NUL, so every in-range offset
            // terminates. Avoid an attacker-controlled scan for every section.
            if start >= names.len() {
                return Err(Error::SectionName);
            }
        }
    }
    symbols::validate(bytes, table)?;
    relocations::validate(bytes, table)?;
    Ok(Inspection { section_count })
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{vec, vec::Vec};

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
        let mut bytes = vec![0; 200];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        put16(&mut bytes, 16, 1);
        put16(&mut bytes, 18, 62);
        put32(&mut bytes, 20, 1);
        put64(&mut bytes, 40, 64);
        put16(&mut bytes, 52, 64);
        put16(&mut bytes, 58, 64);
        put16(&mut bytes, 60, 2);
        put16(&mut bytes, 62, 1);
        put32(&mut bytes, 128 + 4, 3);
        put64(&mut bytes, 128 + 24, 192);
        put64(&mut bytes, 128 + 32, 8);
        put64(&mut bytes, 128 + 48, 1);
        bytes[192..].copy_from_slice(b"\0.names\0");
        bytes
    }

    #[test]
    fn minimal_container_is_not_a_module_compatibility_claim() {
        assert_eq!(inspect(&fixture()), Ok(Inspection { section_count: 2 }));
    }

    #[test]
    fn every_truncated_prefix_is_rejected() {
        let bytes = fixture();
        for end in 0..bytes.len() {
            assert!(inspect(&bytes[..end]).is_err(), "prefix {end}");
        }
    }

    #[test]
    fn rejects_wrong_class_endianness_magic_and_version() {
        for offset in 0..7 {
            let mut bytes = fixture();
            bytes[offset] ^= 0xff;
            assert_eq!(inspect(&bytes), Err(Error::Identification));
        }
    }

    #[test]
    fn rejects_executables_and_other_architectures() {
        let mut bytes = fixture();
        put16(&mut bytes, 16, 2);
        assert_eq!(inspect(&bytes), Err(Error::ObjectType));
        put16(&mut bytes, 16, 1);
        put16(&mut bytes, 18, 183);
        assert_eq!(inspect(&bytes), Err(Error::Architecture));
    }

    #[test]
    fn rejects_header_inconsistencies() {
        for offset in [20, 24, 32, 48, 52, 54, 56, 58] {
            let mut bytes = fixture();
            bytes[offset] ^= 0x80;
            assert_eq!(inspect(&bytes), Err(Error::Header), "offset {offset}");
        }
    }

    #[test]
    fn rejects_extended_numbering() {
        let mut bytes = fixture();
        put16(&mut bytes, 60, 0);
        assert_eq!(inspect(&bytes), Err(Error::ExtendedNumbering));
        let mut bytes = fixture();
        put16(&mut bytes, 60, 0xff00);
        assert_eq!(inspect(&bytes), Err(Error::ExtendedNumbering));
        let mut bytes = fixture();
        put16(&mut bytes, 62, 0xffff);
        assert_eq!(inspect(&bytes), Err(Error::ExtendedNumbering));
    }

    #[test]
    fn rejects_invalid_section_table_ranges() {
        for offset in [0, 63, 65, u64::MAX - 7] {
            let mut bytes = fixture();
            put64(&mut bytes, 40, offset);
            assert_eq!(inspect(&bytes), Err(Error::SectionTable));
        }
    }

    #[test]
    fn rejects_nonzero_null_section() {
        let mut bytes = fixture();
        bytes[64 + 8] = 1;
        assert_eq!(inspect(&bytes), Err(Error::NullSection));
    }

    #[test]
    fn rejects_overflowing_or_out_of_file_payload() {
        for (offset, size) in [(u64::MAX, 2), (199, 2), (201, 0)] {
            let mut bytes = fixture();
            put64(&mut bytes, 128 + 24, offset);
            put64(&mut bytes, 128 + 32, size);
            assert_eq!(inspect(&bytes), Err(Error::SectionRange));
        }
    }

    #[test]
    fn nobits_is_not_read_as_file_payload() {
        let mut bytes = fixture();
        put16(&mut bytes, 62, 0);
        put32(&mut bytes, 128 + 4, 8);
        put64(&mut bytes, 128 + 24, u64::MAX);
        put64(&mut bytes, 128 + 32, 4096);
        assert!(inspect(&bytes).is_ok());
    }

    #[test]
    fn rejects_bad_alignment_and_compression() {
        let mut bytes = fixture();
        put64(&mut bytes, 128 + 48, 3);
        assert_eq!(inspect(&bytes), Err(Error::SectionAlignment));
        put64(&mut bytes, 128 + 48, 0);
        put64(&mut bytes, 128 + 8, 0x800);
        assert_eq!(inspect(&bytes), Err(Error::CompressedSection));
    }

    #[test]
    fn validates_name_table_index_type_and_termination() {
        let mut bytes = fixture();
        put16(&mut bytes, 62, 2);
        assert_eq!(inspect(&bytes), Err(Error::StringTable));
        put16(&mut bytes, 62, 1);
        put32(&mut bytes, 128 + 4, 1);
        assert_eq!(inspect(&bytes), Err(Error::StringTable));
        put32(&mut bytes, 128 + 4, 3);
        bytes[199] = 1;
        assert_eq!(inspect(&bytes), Err(Error::StringTable));
        bytes[199] = 0;
        bytes[192] = 1;
        assert_eq!(inspect(&bytes), Err(Error::StringTable));
    }

    #[test]
    fn rejects_names_outside_table_or_without_a_table() {
        let mut bytes = fixture();
        put32(&mut bytes, 128, 8);
        assert_eq!(inspect(&bytes), Err(Error::SectionName));
        put16(&mut bytes, 62, 0);
        assert_eq!(inspect(&bytes), Err(Error::SectionName));
    }

    #[test]
    fn all_single_byte_mutations_and_short_inputs_do_not_panic() {
        let original = fixture();
        for index in 0..original.len() {
            for value in [0, 1, 0x7f, 0xff] {
                let mut bytes = original.clone();
                bytes[index] = value;
                let _ = inspect(&bytes);
            }
        }
        for length in 0..256 {
            let _ = inspect(&vec![0xff; length]);
        }
    }
}
