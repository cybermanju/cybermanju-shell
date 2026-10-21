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
    let layout = linux_module_elf::plan_layout(&bytes, 0x20_0000, 16 * 1024 * 1024, 256)
        .expect("plan a numeric-only layout of SHF_ALLOC sections");
    let mut cursor = u128::from(layout.base());
    let mut placements = layout.placements().iter();
    for index in 1..usize::from(result.section_count) {
        let header = section(index);
        let flags = u64_at(header, 8);
        if flags & 2 == 0 {
            continue;
        }
        let placement = placements.next().expect("every allocated section planned");
        let alignment = u64_at(header, 48).max(1);
        // Independent arithmetic oracle: division in u128, not the planner's
        // checked u64 bit-mask implementation. Addresses are never pointers.
        cursor = cursor.div_ceil(u128::from(alignment)) * u128::from(alignment);
        assert_eq!(placement.section_index as usize, index);
        assert_eq!(u128::from(placement.address), cursor);
        assert_eq!(placement.alignment, alignment);
        assert_eq!(placement.size, u64_at(header, 32));
        assert_eq!(placement.flags, flags);
        cursor += u128::from(placement.size);
    }
    assert!(placements.next().is_none());
    assert!(!layout.placements().is_empty());
    assert_eq!(
        cursor - u128::from(layout.base()),
        u128::from(layout.span())
    );
    assert_eq!(
        linux_module_elf::plan_layout(&bytes, layout.base(), layout.span() - 1, 256),
        Err(linux_module_elf::Error::ResourceLimit)
    );
    assert_eq!(
        linux_module_elf::plan_layout(
            &bytes,
            layout.base(),
            layout.span(),
            layout.placements().len() - 1
        ),
        Err(linux_module_elf::Error::ResourceLimit)
    );
    println!(
        "Offline SHF_ALLOC layout passed: {} sections, {} bytes span; numeric addresses only; NOT mapped or executed",
        layout.placements().len(),
        layout.span()
    );
    // Test-only export values, NOT functions, real kernel exports or pointers.
    let exports = [
        linux_module_elf::Export {
            name: b"unresolved_kernel_symbol",
            value: 0x80_0000,
        },
        linux_module_elf::Export {
            name: b"__fentry__",
            value: 0x80_0100,
        },
        linux_module_elf::Export {
            name: b"__x86_return_thunk",
            value: 0x80_0200,
        },
    ];
    let resolver = linux_module_elf::SymbolResolver::new(&bytes, &layout, &exports)
        .expect("construct numeric resolver with explicit synthetic exports");
    let without_exports = linux_module_elf::SymbolResolver::new(&bytes, &layout, &[])
        .expect("construct resolver without any external exports");
    let mut planned_sections = 0;
    let mut planned_writes = 0;
    let mut absent_exports = 0;
    for index in 1..usize::from(result.section_count) {
        let rela = section(index);
        if u32_at(rela, 4) != 4 {
            continue;
        }
        let target = u32_at(rela, 44) as u16;
        let Some(placement) = layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
        else {
            continue; // Debug/non-ALLOC relocations are outside this joint test.
        };
        let symtab_index = u32_at(rela, 40) as u16;
        let symtab = section(usize::from(symtab_index));
        let symbols_start = u64_at(symtab, 24) as usize;
        let mut values = vec![None; u64_at(symtab, 32) as usize / 24];
        let entries_start = u64_at(rela, 24) as usize;
        let entries_size = u64_at(rela, 32) as usize;
        let entries = &bytes[entries_start..entries_start + entries_size];
        for entry in entries.chunks_exact(24) {
            let info = u64_at(entry, 8);
            if info as u32 == 0 {
                continue;
            }
            let symbol_index = (info >> 32) as u32;
            let symbol = &bytes[symbols_start + symbol_index as usize * 24
                ..symbols_start + (symbol_index as usize + 1) * 24];
            let shndx = u16_at(symbol, 6);
            let expected = if shndx == 0 {
                assert_eq!(
                    without_exports.resolve(symtab_index, symbol_index),
                    Err(linux_module_elf::Error::MissingSymbol)
                );
                absent_exports += 1;
                let strings = section(u32_at(symtab, 40) as usize);
                let start = u64_at(strings, 24) as usize + u32_at(symbol, 0) as usize;
                let end = start + bytes[start..].iter().position(|&b| b == 0).unwrap();
                exports
                    .iter()
                    .find(|e| e.name == &bytes[start..end])
                    .expect("known fixture export")
                    .value
            } else if shndx == 0xfff1 {
                u64_at(symbol, 8)
            } else {
                let defined_in = layout
                    .placements()
                    .iter()
                    .find(|p| p.section_index == shndx)
                    .expect("referenced symbol is in a planned section");
                defined_in.address.checked_add(u64_at(symbol, 8)).unwrap()
            };
            let value = resolver
                .resolve(symtab_index, symbol_index)
                .expect("resolve requested fixture symbol");
            assert_eq!(value, expected);
            values[symbol_index as usize] = Some(value);
        }
        let output = linux_module_elf::relocate_section(
            &bytes,
            index as u16,
            placement.address,
            &values,
            16 * 1024 * 1024,
        )
        .expect("apply resolved numeric values to a non-executable section copy");
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
                expected -= i128::from(placement.address) + offset as i128;
            }
            let width = if matches!(kind, 1 | 24) { 8 } else { 4 };
            assert_eq!(
                &output[offset..offset + width],
                &expected.to_le_bytes()[..width]
            );
            planned_writes += 1;
        }
        planned_sections += 1;
    }
    assert!(planned_writes > 0 && planned_sections > 0 && absent_exports > 0);
    println!(
        "Planned symbol resolution + RELA passed: {planned_writes} writes in {planned_sections} ALLOC section copies; {absent_exports} missing-export rejections; exports SYNTHETIC; NOT loaded or executed"
    );
    let mut atomic_targets = 0;
    let mut atomic_writes = 0;
    let mut expected_sections = Vec::new();
    let mut total_entries = 0usize;
    let mut total_payload = 0usize;
    for placement in layout.placements() {
        let target = section(usize::from(placement.section_index));
        let kind = u32_at(target, 4);
        total_payload += placement.size as usize;
        if !matches!(kind, 1 | 8) {
            assert_eq!(kind, 7, "only opaque NOTE besides PROGBITS/NOBITS");
            let start = u64_at(target, 24) as usize;
            expected_sections.push((
                *placement,
                bytes[start..start + placement.size as usize].to_vec(),
            ));
            continue;
        } // NOTE remains opaque metadata.
        let size = placement.size as usize;
        let mut expected = vec![0; size];
        if kind == 1 {
            let start = u64_at(target, 24) as usize;
            expected.copy_from_slice(&bytes[start..start + size]);
        }
        let mut entries_count = 0;
        let mut requires_export = false;
        for index in 1..usize::from(result.section_count) {
            let rela = section(index);
            if u32_at(rela, 4) != 4 || u32_at(rela, 44) != u32::from(placement.section_index) {
                continue;
            }
            let symtab_index = u32_at(rela, 40) as u16;
            let symtab = section(usize::from(symtab_index));
            let symbols_start = u64_at(symtab, 24) as usize;
            let start = u64_at(rela, 24) as usize;
            let end = start + u64_at(rela, 32) as usize;
            for entry in bytes[start..end].chunks_exact(24) {
                entries_count += 1;
                let info = u64_at(entry, 8);
                let kind = info as u32;
                if kind == 0 {
                    continue;
                }
                let symbol_index = (info >> 32) as u32;
                let symbol_at = symbols_start + symbol_index as usize * 24;
                requires_export |= u16_at(&bytes[symbol_at..symbol_at + 24], 6) == 0;
                let value = resolver.resolve(symtab_index, symbol_index).unwrap();
                let offset = u64_at(entry, 0) as usize;
                let mut encoded = i128::from(value) + i128::from(u64_at(entry, 16) as i64);
                if matches!(kind, 2 | 4 | 24) {
                    encoded -= i128::from(placement.address) + offset as i128;
                }
                let width = if matches!(kind, 1 | 24) { 8 } else { 4 };
                expected[offset..offset + width].copy_from_slice(&encoded.to_le_bytes()[..width]);
                atomic_writes += 1;
            }
        }
        let output = resolver
            .relocate_target(placement.section_index, 16 * 1024 * 1024, 65_536)
            .expect("atomically relocate all tables of a planned target into data");
        assert_eq!(
            output, expected,
            "full target bytes, including untouched ranges"
        );
        expected_sections.push((*placement, expected));
        total_entries += entries_count;
        if entries_count > 0 {
            assert_eq!(
                resolver.relocate_target(placement.section_index, size, entries_count - 1),
                Err(linux_module_elf::Error::ResourceLimit)
            );
        }
        if size > 0 {
            assert_eq!(
                resolver.relocate_target(placement.section_index, size - 1, 65_536),
                Err(linux_module_elf::Error::ResourceLimit)
            );
        }
        if requires_export {
            assert_eq!(
                without_exports.relocate_target(placement.section_index, size, 65_536),
                Err(linux_module_elf::Error::MissingSymbol)
            );
        }
        atomic_targets += 1;
    }
    assert!(atomic_targets > 0 && atomic_writes == planned_writes);
    println!(
        "Atomic target RELA passed: {atomic_writes} writes across {atomic_targets} PROGBITS/NOBITS copies; full-byte oracle and budgets checked; exports SYNTHETIC; NOT executed"
    );
    let image = resolver
        .prepare_module(total_payload, total_entries, expected_sections.len())
        .expect("prepare every ALLOC section as one data-only transaction");
    assert_eq!(image.payload_bytes(), total_payload);
    assert_eq!(image.relocation_entries(), total_entries);
    assert_eq!(image.sections().len(), expected_sections.len());
    for (actual, (placement, expected)) in image.sections().iter().zip(&expected_sections) {
        assert_eq!(actual.placement(), placement);
        assert_eq!(actual.bytes(), expected);
    }
    assert_eq!(
        resolver.prepare_module(total_payload - 1, total_entries, expected_sections.len()),
        Err(linux_module_elf::Error::ResourceLimit)
    );
    assert_eq!(
        resolver.prepare_module(total_payload, total_entries - 1, expected_sections.len()),
        Err(linux_module_elf::Error::ResourceLimit)
    );
    assert_eq!(
        resolver.prepare_module(total_payload, total_entries, expected_sections.len() - 1),
        Err(linux_module_elf::Error::ResourceLimit)
    );
    assert_eq!(
        without_exports.prepare_module(total_payload, total_entries, expected_sections.len()),
        Err(linux_module_elf::Error::MissingSymbol)
    );
    println!(
        "Whole-module data transaction passed: {} ALLOC sections (opaque NOTE included), {total_payload} payload bytes, {total_entries} RELA entries; full-byte oracle, aggregate budgets and missing exports checked; NOT mapped, loaded or executed",
        image.sections().len()
    );
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
