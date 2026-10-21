// SPDX-License-Identifier: MPL-2.0
//! Test-only inspection and preparation. NEVER calls init or exit.
use linux_module_elf::{Error, SymbolResolver, inspect, plan_layout};
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let bytes = std::fs::read(path).expect("read fixture");
    let original = bytes.clone();
    let inspected = inspect(&bytes).expect("real ELF64 x86-64 ET_REL preflight");
    let headers = u64_at(&bytes, 40) as usize;
    let section = |i: usize| &bytes[headers + i * 64..headers + (i + 1) * 64];
    let layout = plan_layout(&bytes, 0x200000, 16 * 1024 * 1024, 1024).expect("bounded layout");
    let resolver = SymbolResolver::new(&bytes, &layout, &[]).expect("EMPTY export list");
    let prepared = resolver
        .prepare_module(16 * 1024 * 1024, 65_536, 1024)
        .expect("complete preparation without synthetic exports");
    assert_eq!(prepared.sections().len(), layout.placements().len());
    let mut entries = Vec::new();
    for table_index in 1..usize::from(inspected.section_count) {
        let table = section(table_index);
        if u32_at(table, 4) != 2 {
            continue;
        }
        let strings_header = section(u32_at(table, 40) as usize);
        let start = u64_at(strings_header, 24) as usize;
        let strings = &bytes[start..start + u64_at(strings_header, 32) as usize];
        let symbols_start = u64_at(table, 24) as usize;
        let count = u64_at(table, 32) as usize / 24;
        for index in 1..count {
            let at = symbols_start + index * 24;
            let symbol = &bytes[at..at + 24];
            assert_ne!(
                u16_at(symbol, 6),
                0,
                "no undefined symbols, even unreferenced ones"
            );
            let suffix = &strings[u32_at(symbol, 0) as usize..];
            let name = &suffix[..suffix.iter().position(|&v| v == 0).unwrap()];
            if !matches!(name, b"init_module" | b"cleanup_module") {
                continue;
            }
            assert_eq!(symbol[4], 0x12, "global FUNC entry required");
            let image = prepared
                .sections()
                .iter()
                .find(|s| s.placement().section_index == u16_at(symbol, 6))
                .expect("entry must be in a prepared section");
            assert_eq!(
                image.placement().flags & 5,
                4,
                "entry section marked executable and not writable; NOT actual page permissions"
            );
            let offset = u64_at(symbol, 8);
            let size = u64_at(symbol, 16);
            assert!(size > 0 && offset < image.bytes().len() as u64);
            assert!(offset.checked_add(size).unwrap() <= image.bytes().len() as u64);
            let value = resolver.resolve(table_index as u16, index as u32).unwrap();
            assert_eq!(
                value,
                image.placement().address.checked_add(offset).unwrap()
            );
            entries.push((name, table_index as u16, index as u32, at));
            println!(
                "Entry {}: numeric address={value:#x}, size={size}; NOT called",
                std::str::from_utf8(name).unwrap()
            );
        }
    }
    assert_eq!(entries.iter().filter(|e| e.0 == b"init_module").count(), 1);
    assert_eq!(
        entries.iter().filter(|e| e.0 == b"cleanup_module").count(),
        1
    );
    let init = entries.iter().find(|e| e.0 == b"init_module").unwrap();
    let mut init_referenced = false;
    for index in 1..usize::from(inspected.section_count) {
        let rela = section(index);
        if u32_at(rela, 4) != 4 || u32_at(rela, 40) != u32::from(init.1) {
            continue;
        }
        if u64_at(section(u32_at(rela, 44) as usize), 8) & 2 == 0 {
            continue;
        }
        let start = u64_at(rela, 24) as usize;
        for entry in bytes[start..start + u64_at(rela, 32) as usize].chunks_exact(24) {
            let info = u64_at(entry, 8);
            init_referenced |= info as u32 != 0 && (info >> 32) as u32 == init.2;
        }
    }
    assert!(
        init_referenced,
        "negative check must affect an applied relocation"
    );
    let mut missing = bytes.clone();
    missing[init.3 + 6..init.3 + 8].copy_from_slice(&0_u16.to_le_bytes());
    missing[init.3 + 8..init.3 + 24].fill(0);
    let negative = SymbolResolver::new(&missing, &layout, &[])
        .expect("mutated import remains structurally valid");
    assert_eq!(
        negative.prepare_module(16 * 1024 * 1024, 65_536, 1024),
        Err(Error::MissingSymbol)
    );
    assert_eq!(bytes, original);
    println!(
        "No-import fixture: {} sections, {} payload bytes, {} ALLOC RELA entries; exports=0; missing-import rejection passed; NOT mapped, loaded or executed",
        prepared.sections().len(),
        prepared.payload_bytes(),
        prepared.relocation_entries()
    );
}
