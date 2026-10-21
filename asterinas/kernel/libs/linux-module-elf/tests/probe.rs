// SPDX-License-Identifier: MPL-2.0
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn main() {
    let path = std::env::args().nth(1).expect("expected ELF object path");
    let bytes = std::fs::read(path).expect("read fixture");
    let original = bytes.clone();
    let result = linux_module_elf::inspect(&bytes).expect("inspect ELF ET_REL structure");
    assert!(result.section_count > 2);
    println!(
        "ELF ET_REL structural inspection passed: {} sections; NOT loaded or ABI-validated",
        result.section_count
    );
    let table = u64_at(&bytes, 40) as usize;
    let section = |index: usize| &bytes[table + index * 64..table + (index + 1) * 64];
    // Deliberately synthetic addresses, NOT kernel exports or callable pointers.
    let base = |index: usize| 0x10_0000_u64 + index as u64 * 0x1_0000;
    let mut applied_sections = 0;
    let mut applied_entries = 0;
    let mut missing_symbol_checks = 0;
    for index in 1..usize::from(result.section_count) {
        let rela = section(index);
        if u32_at(rela, 4) != 4 {
            continue;
        }
        let target = u32_at(rela, 44) as usize;
        let symtab = section(u32_at(rela, 40) as usize);
        let symbols_start = u64_at(symtab, 24) as usize;
        let count = u64_at(symtab, 32) as usize / 24;
        let mut values = vec![None; count];
        let mut unresolved = vec![None; count];
        for symbol_index in 1..count {
            let symbol =
                &bytes[symbols_start + symbol_index * 24..symbols_start + (symbol_index + 1) * 24];
            let shndx = u16_at(symbol, 6);
            let value = u64_at(symbol, 8);
            values[symbol_index] = Some(match shndx {
                0 => 0x80_0000 + symbol_index as u64 * 16, // fake external value
                0xfff1 => value,                           // SHN_ABS
                0xfff2 => {
                    eprintln!("offline fixture probe does not allocate COMMON symbols");
                    std::process::exit(1);
                }
                _ => base(usize::from(shndx)).checked_add(value).unwrap(),
            });
            if shndx != 0 {
                unresolved[symbol_index] = values[symbol_index];
            }
        }
        let entries_start = u64_at(rela, 24) as usize;
        let entries_size = u64_at(rela, 32) as usize;
        let entries = &bytes[entries_start..entries_start + entries_size];
        let has_missing = entries.chunks_exact(24).any(|entry| {
            let info = u64_at(entry, 8);
            info as u32 != 0 && unresolved[(info >> 32) as usize].is_none()
        });
        if has_missing {
            assert_eq!(
                linux_module_elf::relocate_section(
                    &bytes,
                    index as u16,
                    base(target),
                    &unresolved,
                    16 * 1024 * 1024
                ),
                Err(linux_module_elf::Error::MissingSymbol)
            );
            missing_symbol_checks += 1;
        }
        let output = linux_module_elf::relocate_section(
            &bytes,
            index as u16,
            base(target),
            &values,
            16 * 1024 * 1024,
        )
        .expect("apply RELA to heap data using synthetic symbol addresses");
        assert_eq!(output.len() as u64, u64_at(section(target), 32));
        // Independently check each written field against the numeric formula.
        for entry in entries.chunks_exact(24) {
            let info = u64_at(entry, 8);
            let kind = info as u32;
            if kind == 0 {
                continue;
            }
            let offset = u64_at(entry, 0) as usize;
            let mut expected = i128::from(values[(info >> 32) as usize].unwrap())
                + i128::from(u64_at(entry, 16) as i64);
            if matches!(kind, 2 | 4 | 24) {
                expected -= i128::from(base(target)) + offset as i128;
            }
            let width = if matches!(kind, 1 | 24) { 8 } else { 4 };
            assert_eq!(
                &output[offset..offset + width],
                &expected.to_le_bytes()[..width]
            );
            applied_entries += 1;
        }
        applied_sections += 1;
    }
    assert!(applied_sections > 0 && applied_entries > 0 && missing_symbol_checks > 0);
    assert_eq!(bytes, original);
    println!(
        "Offline RELA passed: {applied_entries} writes in {applied_sections} data-only section copies; {missing_symbol_checks} missing-symbol checks; SYNTHETIC addresses; NOT loaded or executed"
    );
}
