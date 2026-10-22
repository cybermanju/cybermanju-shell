// SPDX-License-Identifier: MPL-2.0
//! Runtime coverage for the explicit OSTD export registry; never maps or executes.
use alloc::{vec, vec::Vec};

use linux_module_elf::{Error as ElfError, SymbolResolver, plan_page_layout};
use ostd::{
    cpu,
    mm::{PAGE_SIZE, module_address::NearTextAddressReservation},
    prelude::*,
};

use super::{FIXTURE, MAX_SECTIONS, MAX_SPAN, PreparationLimits, relative_import_fixture};
use crate::{Error, NearTextModuleLayout, OSTD_NUM_CPUS_EXPORT_NAME, OstdExportRegistry};

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// Rename the selected undefined import in a cloned fixture, retaining every other ELF region.
fn renamed_import_fixture(kind: u32) -> (Vec<u8>, u16, u64) {
    let (mut bytes, _old_name, target, relocation_offset) = relative_import_fixture(kind);
    let shoff = usize::try_from(u64_at(&bytes, 40)).unwrap();
    let shnum = usize::from(u16::from_le_bytes(bytes[60..62].try_into().unwrap()));
    let mut selected = None;
    for index in 0..shnum {
        let rela = shoff + index * 64;
        if u32_at(&bytes, rela + 4) != 4 || u32_at(&bytes, rela + 44) != u32::from(target) {
            continue;
        }
        let entry_off = usize::try_from(u64_at(&bytes, rela + 24)).unwrap();
        let entry_size = usize::try_from(u64_at(&bytes, rela + 32)).unwrap();
        for entry in (entry_off..entry_off + entry_size).step_by(24) {
            let info = u64_at(&bytes, entry + 8);
            if u64_at(&bytes, entry) == relocation_offset && info as u32 == kind {
                selected = Some((
                    entry,
                    (info >> 32) as usize,
                    u32_at(&bytes, rela + 40) as usize,
                ));
                break;
            }
        }
    }
    let (_entry, symbol_index, symtab_index) = selected.expect("selected RELA import exists");
    let symtab = shoff + symtab_index * 64;
    assert_eq!(u32_at(&bytes, symtab + 4), 2);
    let symbol = usize::try_from(u64_at(&bytes, symtab + 24)).unwrap() + symbol_index * 24;
    assert_eq!(
        bytes[symbol + 4] >> 4,
        1,
        "fixture symbol must be STB_GLOBAL"
    );
    assert_eq!(
        u16::from_le_bytes(bytes[symbol + 6..symbol + 8].try_into().unwrap()),
        0,
        "fixture symbol must be SHN_UNDEF"
    );
    let strtab_index = u32_at(&bytes, symtab + 40) as usize;
    let strtab = shoff + strtab_index * 64;
    let old_string_offset = usize::try_from(u64_at(&bytes, strtab + 24)).unwrap();
    let old_string_size = usize::try_from(u64_at(&bytes, strtab + 32)).unwrap();
    let mut new_strings = bytes[old_string_offset..old_string_offset + old_string_size].to_vec();
    let new_name_offset = new_strings.len();
    new_strings.extend_from_slice(OSTD_NUM_CPUS_EXPORT_NAME);
    new_strings.push(0);
    let new_table_offset = bytes.len();
    bytes.extend_from_slice(&new_strings);
    put_u64(&mut bytes, strtab + 24, new_table_offset as u64);
    put_u64(&mut bytes, strtab + 32, new_strings.len() as u64);
    put_u32(&mut bytes, symbol, u32::try_from(new_name_offset).unwrap());
    (bytes, target, relocation_offset)
}

#[ktest]
fn cybermanju_internal_exports_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 1);
    println!("CYBERMANJU_INTERNAL_EXPORTS:begin:PASS cpus=1 entries=1");
    assert_eq!(OSTD_NUM_CPUS_EXPORT_NAME, b"cybermanju_ostd_num_cpus");
    let registry = OstdExportRegistry::new();
    let exports = registry.exports();
    let value = cpu::num_cpus as *const () as usize as u64;
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, OSTD_NUM_CPUS_EXPORT_NAME);
    assert_eq!(
        exports[0].value, value,
        "registry address must match OSTD function"
    );
    assert_eq!(registry.lookup(OSTD_NUM_CPUS_EXPORT_NAME), Some(value));
    let denied: [&[u8]; 5] = [
        b"__fentry__",
        b"kmalloc",
        b"num_cpus",
        b"cybermanju_ostd_num_cpus\0",
        b"",
    ];
    for name in denied {
        assert_eq!(
            registry.lookup(name),
            None,
            "registry must reject unregistered names"
        );
    }

    let pool = NearTextAddressReservation::signed32_arena_range();
    let base = pool.start.checked_add(16 * PAGE_SIZE).unwrap();
    let envelope = usize::try_from(
        linux_module_elf::page_layout_span_bound(FIXTURE, MAX_SPAN, MAX_SECTIONS).unwrap(),
    )
    .unwrap();
    let window = linux_module_elf::Relative32BaseWindow::for_module_span(envelope as u64)
        .unwrap()
        .restrict(base as u64, base as u64)
        .unwrap();
    let max_pages = envelope / PAGE_SIZE;
    let limits = PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages,
        max_chunks: MAX_SECTIONS,
    };
    let mut compared = 0usize;
    let mut missing = 0usize;
    let mut reclaimed = 0usize;
    for kind in [2u32, 4] {
        let (original, _alias, target, offset) = relative_import_fixture(kind);
        let missing_result = NearTextModuleLayout::new_signed32_in_relative32_window(
            &original,
            exports,
            window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            1,
        );
        assert!(
            matches!(missing_result, Err(Error::Plan(ElfError::MissingSymbol))),
            "registry must not bind the old fixture alias"
        );
        missing += 1;
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        assert_eq!(raw.range(), base..base + envelope);
        drop(raw);
        reclaimed += 1;

        let (bytes, renamed_target, renamed_offset) = renamed_import_fixture(kind);
        assert_eq!((renamed_target, renamed_offset), (target, offset));
        let page_layout = plan_page_layout(&bytes, base as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
        let expected_prepared = SymbolResolver::new(&bytes, &page_layout, exports)
            .unwrap()
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
            .unwrap();
        let span = usize::try_from(page_layout.span()).unwrap();
        let pages = span / PAGE_SIZE;
        let payload = expected_prepared.payload_bytes();
        let relas = expected_prepared.relocation_entries();
        assert_eq!((span, pages, payload, relas), (40_960, 10, 1_721, 6));
        let placement = page_layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let field = placement.address + offset;
        let expected_symbol = i128::from(value) - i128::from(field);
        let expected_scalar = i32::try_from(expected_symbol).unwrap().to_le_bytes();
        let mut expected = vec![0; span];
        for section in expected_prepared.sections() {
            let start = usize::try_from(section.placement().address - base as u64).unwrap();
            expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
        }
        let scalar_offset = usize::try_from(field - base as u64).unwrap();
        assert_eq!(
            &expected[scalar_offset..scalar_offset + 4],
            &expected_scalar
        );

        let owner = NearTextModuleLayout::new_signed32_in_relative32_window(
            &bytes,
            exports,
            window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            1,
        )
        .unwrap();
        assert_eq!(owner.layout().base(), base as u64);
        assert_eq!(owner.range(), base..base + envelope);
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ));
        let staged = owner.prepare_and_stage(&bytes, exports, limits).unwrap();
        assert_eq!(staged.reserved_range(), base..base + envelope);
        assert_eq!((staged.span(), staged.page_count()), (span, pages));
        let mut actual = vec![0xff; span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(
            actual, expected,
            "all bytes across the complete reserved-base span"
        );
        assert_eq!(&actual[scalar_offset..scalar_offset + 4], &expected_scalar);
        assert!(
            matches!(
                NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
                Err(ostd::Error::NoMemory)
            ),
            "registry staging must retain address reservation"
        );
        println!(
            "CYBERMANJU_INTERNAL_EXPORTS:staged:PASS kind={} base={:#x} reserved={} span={} pages={} payload={} relas={} symbol={:#x} field={:#x} compared={} registry_match=true",
            kind, base, envelope, span, pages, payload, relas, value, field, span
        );
        compared += span;
        drop(staged);
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        assert_eq!(raw.range(), base..base + envelope);
        drop(raw);
        reclaimed += 1;
    }
    assert_eq!((compared, missing, reclaimed), (2 * 40_960, 2, 4));
    assert_eq!(denied.len(), 5);
    println!(
        "CYBERMANJU_INTERNAL_EXPORTS:complete:PASS staged=2 denied_names=5 missing_imports=2 available_checks=4 compared_full_span=true pages_mapped=false linux_abi=false execution=false"
    );
}
