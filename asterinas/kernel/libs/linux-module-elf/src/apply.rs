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
        let (width, encoded) = encode_resolved(
            kind,
            symbol,
            u64_at(entry, 16) as i64,
            target_address,
            offset,
        )?;
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

/// Apply every SHT_RELA section that targets one planned allocatable section.
/// The input ELF is read-only; patches are accumulated and committed only after
/// all tables, symbols, bounds, and overlaps have been validated.
pub(super) fn relocate_target(
    bytes: &[u8],
    layout: &crate::Layout,
    resolver: &crate::SymbolResolver<'_>,
    target_index: u16,
    max_bytes: usize,
    max_entries: usize,
) -> Result<Vec<u8>, Error> {
    // This private helper is reached only through SymbolResolver, which has
    // already validated and immutably borrows this exact ELF and layout. Do not
    // revalidate every symbol/relocation globally for each prepared section.
    let section_count = crate::u16_at(bytes, 60);
    if target_index == 0 || target_index >= section_count {
        return Err(Error::RelocationTarget);
    }
    let table = file_range(
        bytes,
        u64_at(bytes, 40),
        u64::from(section_count) * SECTION_SIZE as u64,
    )?;
    let section = |index: usize| &table[index * SECTION_SIZE..(index + 1) * SECTION_SIZE];
    let target = section(usize::from(target_index));
    if !matches!(u32_at(target, 4), 1 | 8) || u64_at(target, 8) & 2 == 0 {
        return Err(Error::RelocationTarget);
    }
    let placement = layout
        .placements()
        .iter()
        .find(|p| p.section_index == target_index)
        .ok_or(Error::RelocationTarget)?;
    if placement.size != u64_at(target, 32) || placement.flags & 2 == 0 {
        return Err(Error::RelocationTarget);
    }
    let size = usize::try_from(u64_at(target, 32)).map_err(|_| Error::ResourceLimit)?;
    if size > max_bytes {
        return Err(Error::ResourceLimit);
    }
    let target_address = placement.address;
    target_address
        .checked_add(u64_at(target, 32))
        .ok_or(Error::RelocationOverflow)?;

    // Count first, across every matching table (including NONE entries), before
    // allocating patches or target payload. Inspect validates links and ranges.
    let mut total = 0usize;
    for index in 1..usize::from(section_count) {
        let rela = section(index);
        if u32_at(rela, 4) == 4 && u32_at(rela, 44) == u32::from(target_index) {
            let count = usize::try_from(u64_at(rela, 32) / 24).map_err(|_| Error::ResourceLimit)?;
            total = total.checked_add(count).ok_or(Error::ResourceLimit)?;
            if total > core::cmp::min(max_entries, 65_536) {
                return Err(Error::ResourceLimit);
            }
        }
    }
    let mut patches = Vec::new();
    patches
        .try_reserve_exact(total)
        .map_err(|_| Error::ResourceLimit)?;
    for index in 1..usize::from(section_count) {
        let rela = section(index);
        if u32_at(rela, 4) != 4 || u32_at(rela, 44) != u32::from(target_index) {
            continue;
        }
        let symtab_index = u16::try_from(u32_at(rela, 40)).map_err(|_| Error::RelocationLink)?;
        let entries = file_range(bytes, u64_at(rela, 24), u64_at(rela, 32))?;
        for entry in entries.chunks_exact(24) {
            let info = u64_at(entry, 8);
            let kind = info as u32;
            if kind == 0 {
                continue;
            }
            let symbol = resolver.resolve(symtab_index, (info >> 32) as u32)?;
            let offset = u64_at(entry, 0);
            let (width, encoded) = encode_resolved(
                kind,
                symbol,
                u64_at(entry, 16) as i64,
                target_address,
                offset,
            )?;
            let offset = usize::try_from(offset).map_err(|_| Error::RelocationRange)?;
            let end = offset.checked_add(width).ok_or(Error::RelocationRange)?;
            if end > size {
                return Err(Error::RelocationRange);
            }
            patches.push(Patch {
                offset,
                width,
                value: encoded.to_le_bytes(),
            });
        }
    }
    patches.sort_unstable_by_key(|patch| patch.offset);
    for pair in patches.windows(2) {
        if pair[0]
            .offset
            .checked_add(pair[0].width)
            .ok_or(Error::RelocationRange)?
            > pair[1].offset
        {
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

// Only reached through a resolver that already validated this immutable ELF/layout.
pub(super) fn check_relocation_reach(
    bytes: &[u8],
    layout: &crate::Layout,
    resolver: &crate::SymbolResolver<'_>,
    max_entries: usize,
) -> Result<usize, Error> {
    diagnose_relocation_reach(bytes, layout, resolver, max_entries).map_err(|failure| failure.cause)
}

pub(super) fn diagnose_relocation_reach(
    bytes: &[u8],
    layout: &crate::Layout,
    resolver: &crate::SymbolResolver<'_>,
    max_entries: usize,
) -> Result<usize, crate::RelocationReachError> {
    let table = file_range(
        bytes,
        u64_at(bytes, 40),
        u64::from(crate::u16_at(bytes, 60)) * SECTION_SIZE as u64,
    )?;
    let section = |i: usize| &table[i * SECTION_SIZE..(i + 1) * SECTION_SIZE];
    let mut total = 0usize;
    for rela in table.chunks_exact(SECTION_SIZE) {
        if u32_at(rela, 4) != 4 {
            continue;
        }
        let target = section(u32_at(rela, 44) as usize);
        if u64_at(target, 8) & 2 == 0 {
            continue;
        }
        if !matches!(u32_at(target, 4), 1 | 8) {
            return Err(Error::RelocationTarget.into());
        }
        total = total
            .checked_add(usize::try_from(u64_at(rela, 32) / 24).map_err(|_| Error::ResourceLimit)?)
            .ok_or(Error::ResourceLimit)?;
        if total > core::cmp::min(max_entries, 65_536) {
            return Err(Error::ResourceLimit.into());
        }
    }
    for (rela_index, rela) in table.chunks_exact(SECTION_SIZE).enumerate() {
        if u32_at(rela, 4) != 4 {
            continue;
        }
        let index = u32_at(rela, 44);
        if u64_at(section(index as usize), 8) & 2 == 0 {
            continue;
        }
        let placement = layout
            .placements()
            .iter()
            .find(|p| u32::from(p.section_index) == index)
            .ok_or(Error::RelocationTarget)?;
        let symtab = u16::try_from(u32_at(rela, 40)).map_err(|_| Error::RelocationLink)?;
        for (entry_index, entry) in file_range(bytes, u64_at(rela, 24), u64_at(rela, 32))?
            .chunks_exact(24)
            .enumerate()
        {
            let info = u64_at(entry, 8);
            let kind = info as u32;
            if kind == 0 {
                continue;
            }
            let site = crate::RelocationSite {
                rela_section: u16::try_from(rela_index).map_err(|_| Error::RelocationTable)?,
                entry_index,
                target_section: u16::try_from(index).map_err(|_| Error::RelocationTarget)?,
                kind,
                symbol_index: (info >> 32) as u32,
                offset: u64_at(entry, 0),
            };
            let contextual = |cause| crate::RelocationReachError {
                cause,
                site: Some(site),
            };
            let symbol = resolver
                .resolve(symtab, (info >> 32) as u32)
                .map_err(contextual)?;
            encode_resolved(
                kind,
                symbol,
                u64_at(entry, 16) as i64,
                placement.address,
                u64_at(entry, 0),
            )
            .map_err(contextual)?;
        }
    }
    Ok(total)
}

fn encode_resolved(
    kind: u32,
    symbol: u64,
    addend: i64,
    address: u64,
    offset: u64,
) -> Result<(usize, u64), Error> {
    let mut value = i128::from(symbol) + i128::from(addend);
    if matches!(kind, 2 | 4 | 24) {
        value -= i128::from(address) + i128::from(offset);
    }
    encode_relocation(kind, value)
}

// The common x86-64 RELA arithmetic/width semantics used by both APIs.
fn encode_relocation(kind: u32, value: i128) -> Result<(usize, u64), Error> {
    match kind {
        1 => Ok((
            8,
            u64::try_from(value).map_err(|_| Error::RelocationOverflow)?,
        )),
        24 => Ok((
            8,
            i64::try_from(value).map_err(|_| Error::RelocationOverflow)? as u64,
        )),
        10 => Ok((
            4,
            u64::from(u32::try_from(value).map_err(|_| Error::RelocationOverflow)?),
        )),
        2 | 4 => Ok((
            4,
            i32::try_from(value).map_err(|_| Error::RelocationOverflow)? as u32 as u64,
        )),
        11 => {
            // R_X86_64_32S stores the low word, but the ABI requires that
            // sign-extending it reproduces the original 64-bit value. Keep
            // mathematical negatives distinct from unsigned address aliases.
            let bits = if value < 0 {
                i64::try_from(value).map_err(|_| Error::RelocationOverflow)? as u64
            } else {
                u64::try_from(value).map_err(|_| Error::RelocationOverflow)?
            };
            let word = bits as u32;
            if (word as i32 as i64 as u64) != bits {
                return Err(Error::RelocationOverflow);
            }
            Ok((4, u64::from(word)))
        }
        _ => Err(Error::UnsupportedRelocation),
    }
}
