// SPDX-License-Identifier: MPL-2.0
//! Offline relocation arithmetic. Never maps memory or calls module code.
use alloc::vec::Vec;

use crate::{Error, SECTION_SIZE, file_range, inspect, u32_at, u64_at};

#[derive(Debug)]
struct Patch {
    offset: usize,
    width: usize,
    value: [u8; 8],
}

/// Apply one SHT_RELA section to a fresh data-only copy of its target.
///
/// `symbol_values` is indexed by the linked symbol table and must have exactly
/// its length. Every referenced non-NONE symbol needs an explicit value, even
/// undefined/weak/common symbols; no kernel export lookup or ABI check occurs.
/// For PLT32 the caller supplies the direct destination (no PLT is synthesized).
/// Addresses are numeric model addresses, never dereferenced host pointers.
///
/// Only PROGBITS/NOBITS targets are supported. `max_bytes` caps the target;
/// at most 65,536 relocation entries are accepted. Overlapping writes fail
/// closed. Errors never modify the ELF or expose a partially relocated result.
/// The returned Vec is ordinary heap data, not an executable mapping. A caller
/// must not reinterpret it as callable code. This is NOT a Linux module loader.
pub fn relocate_section(
    bytes: &[u8],
    rela_index: u16,
    target_address: u64,
    symbol_values: &[Option<u64>],
    max_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let inspected = inspect(bytes)?;
    if rela_index == 0 || rela_index >= inspected.section_count {
        return Err(Error::RelocationTable);
    }
    let table = file_range(
        bytes,
        u64_at(bytes, 40),
        u64::from(inspected.section_count) * SECTION_SIZE as u64,
    )?;
    let section = |index: usize| &table[index * SECTION_SIZE..(index + 1) * SECTION_SIZE];
    let rela = section(usize::from(rela_index));
    if u32_at(rela, 4) != 4 {
        return Err(Error::RelocationTable);
    }
    // inspect validated these indices, sizes, encodings and file ranges.
    let target = section(u32_at(rela, 44) as usize);
    if !matches!(u32_at(target, 4), 1 | 8) {
        return Err(Error::RelocationTarget);
    }
    let symtab = section(u32_at(rela, 40) as usize);
    if symbol_values.len() as u64 != u64_at(symtab, 32) / 24 {
        return Err(Error::RelocationSymbol);
    }
    let size = usize::try_from(u64_at(target, 32)).map_err(|_| Error::ResourceLimit)?;
    if size > max_bytes || u64_at(rela, 32) / 24 > 65_536 {
        return Err(Error::ResourceLimit);
    }
    target_address
        .checked_add(u64_at(target, 32))
        .ok_or(Error::RelocationOverflow)?;
    let entries = file_range(bytes, u64_at(rela, 24), u64_at(rela, 32))?;
    let mut patches = Vec::new();
    patches
        .try_reserve_exact(entries.len() / 24)
        .map_err(|_| Error::ResourceLimit)?;
    for entry in entries.chunks_exact(24) {
        let info = u64_at(entry, 8);
        let kind = info as u32;
        if kind == 0 {
            continue;
        }
        let symbol = symbol_values[(info >> 32) as usize].ok_or(Error::MissingSymbol)?;
        let offset = u64_at(entry, 0);
        let place = i128::from(target_address) + i128::from(offset);
        let mut value = i128::from(symbol) + i128::from(u64_at(entry, 16) as i64);
        if matches!(kind, 2 | 4 | 24) {
            value -= place;
        }
        let (width, encoded) = match kind {
            1 => (
                8,
                u64::try_from(value).map_err(|_| Error::RelocationOverflow)?,
            ),
            24 => (
                8,
                i64::try_from(value).map_err(|_| Error::RelocationOverflow)? as u64,
            ),
            10 => (
                4,
                u64::from(u32::try_from(value).map_err(|_| Error::RelocationOverflow)?),
            ),
            2 | 4 | 11 => (
                4,
                i32::try_from(value).map_err(|_| Error::RelocationOverflow)? as u32 as u64,
            ),
            _ => return Err(Error::UnsupportedRelocation),
        };
        patches.push(Patch {
            offset: usize::try_from(offset).map_err(|_| Error::RelocationRange)?,
            width,
            value: encoded.to_le_bytes(),
        });
    }
    patches.sort_unstable_by_key(|patch| patch.offset);
    for pair in patches.windows(2) {
        if pair[0].offset + pair[0].width > pair[1].offset {
            return Err(Error::RelocationOverlap);
        }
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(size)
        .map_err(|_| Error::ResourceLimit)?;
    output.resize(size, 0);
    if u32_at(target, 4) == 1 {
        output.copy_from_slice(file_range(bytes, u64_at(target, 24), u64_at(target, 32))?);
    }
    for patch in patches {
        output[patch.offset..patch.offset + patch.width]
            .copy_from_slice(&patch.value[..patch.width]);
    }
    Ok(output)
}
