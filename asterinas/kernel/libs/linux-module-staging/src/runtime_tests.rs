// SPDX-License-Identifier: MPL-2.0
//! Trusted, embedded kbuild fixture. Deliberately never calls any module entry.
use alloc::{vec, vec::Vec};

use linux_module_elf::{Error as ElfError, SymbolResolver, plan_layout, plan_page_layout};
use ostd::{mm::PAGE_SIZE, prelude::*};

use super::{Error, PreparationLimits, ReservedModuleLayout, stage_prepared};

// Only expanded for the selected kernel test; normal builds need no fixture.
const FIXTURE: &[u8] = include_bytes!(env!("CYBERMANJU_STAGING_FIXTURE"));
const BASE: u64 = 0x1000_0000; // Numeric relocation model, NOT a mapped address.
const MAX_SPAN: u64 = 4 * 1024 * 1024;
const MAX_SECTIONS: usize = 1024;

// Trusted-fixture mutation only: select a local section R_X86_64_64 RELA
// targeting ALLOC, with zero symbol value/addend, and narrow it to unsigned 32.
// No fixed build-specific file offsets and no production parser/API changes.
fn fixture_with_u32_relocation() -> Vec<u8> {
    fn u32_at(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }
    fn u64_at(b: &[u8], o: usize) -> u64 {
        u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
    }
    assert_eq!(&FIXTURE[..6], b"\x7fELF\x02\x01");
    let table = usize::try_from(u64_at(FIXTURE, 40)).unwrap();
    let count = u16::from_le_bytes(FIXTURE[60..62].try_into().unwrap()) as usize;
    assert_eq!(u16::from_le_bytes(FIXTURE[58..60].try_into().unwrap()), 64);
    for i in 0..count {
        let sh = table + i * 64;
        if u32_at(FIXTURE, sh + 4) != 4 {
            continue;
        } // SHT_RELA
        assert_eq!(u64_at(FIXTURE, sh + 56), 24);
        let target = u32_at(FIXTURE, sh + 44) as usize;
        let symtab = u32_at(FIXTURE, sh + 40) as usize;
        assert!(target < count && symtab < count);
        if u64_at(FIXTURE, table + target * 64 + 8) & 2 == 0 {
            continue;
        }
        let symsh = table + symtab * 64;
        assert_eq!(u32_at(FIXTURE, symsh + 4), 2); // SHT_SYMTAB
        assert_eq!(u64_at(FIXTURE, symsh + 56), 24);
        let off = usize::try_from(u64_at(FIXTURE, sh + 24)).unwrap();
        let size = usize::try_from(u64_at(FIXTURE, sh + 32)).unwrap();
        assert_eq!(size % 24, 0);
        for entry in (off..off + size).step_by(24) {
            let info = u64_at(FIXTURE, entry + 8);
            if info as u32 != 1 || u64_at(FIXTURE, entry + 16) != 0 {
                continue;
            }
            let symbol_index = (info >> 32) as usize;
            assert!(symbol_index < usize::try_from(u64_at(FIXTURE, symsh + 32)).unwrap() / 24);
            let symbol = usize::try_from(u64_at(FIXTURE, symsh + 24)).unwrap() + symbol_index * 24;
            if FIXTURE[symbol + 4] != 3 || u64_at(FIXTURE, symbol + 8) != 0 {
                continue;
            }
            let section =
                u16::from_le_bytes(FIXTURE[symbol + 6..symbol + 8].try_into().unwrap()) as usize;
            if section == 0
                || section >= count
                || u64_at(FIXTURE, table + section * 64 + 8) & 2 == 0
            {
                continue;
            }
            let mut changed = FIXTURE.to_vec();
            changed[entry + 8..entry + 12].copy_from_slice(&10u32.to_le_bytes());
            assert_eq!(&changed[..entry + 8], &FIXTURE[..entry + 8]);
            assert_eq!(&changed[entry + 12..], &FIXTURE[entry + 12..]);
            return changed;
        }
    }
    panic!("fixture needs a zero-addend local-section R_X86_64_64 in ALLOC");
}

#[ktest]
fn cybermanju_prepared_module_runtime() {
    let layout = plan_page_layout(FIXTURE, BASE, MAX_SPAN, MAX_SECTIONS).unwrap();
    let resolver = SymbolResolver::new(FIXTURE, &layout, &[]).unwrap();
    let prepared = resolver
        .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
        .unwrap();
    let span = usize::try_from(layout.span()).unwrap();
    let sections = prepared.sections().len();
    let pages = span / PAGE_SIZE;
    assert!(sections > 0 && pages > 0 && prepared.relocation_entries() > 0);

    // Independent relative offsets, not the bridge's descriptor array.
    let mut expected = vec![0; span];
    let before: Vec<_> = prepared
        .sections()
        .iter()
        .map(|s| s.bytes().to_vec())
        .collect();
    for section in prepared.sections() {
        let start =
            usize::try_from(section.placement().address.checked_sub(BASE).unwrap()).unwrap();
        let end = start.checked_add(section.bytes().len()).unwrap();
        expected[start..end].copy_from_slice(section.bytes());
    }
    {
        let staged = stage_prepared(&prepared, &layout, pages, sections).unwrap();
        assert_eq!((staged.span(), staged.page_count()), (span, pages));
        let mut actual = vec![0xff; span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(
            actual, expected,
            "payload, gaps and page tails must match exactly"
        );
        let mut byte = [0];
        assert!(staged.read_bytes(span, &mut byte).is_err());
    }
    for (section, original) in prepared.sections().iter().zip(before) {
        assert_eq!(
            section.bytes(),
            original.as_slice(),
            "prepared input remains immutable"
        );
    }
    println!(
        "\nCYBERMANJU_PREPARED_RUNTIME:roundtrip:PASS sections={} span={} payload={} relocations={} pages={}",
        sections,
        span,
        prepared.payload_bytes(),
        prepared.relocation_entries(),
        pages
    );

    assert!(matches!(
        stage_prepared(&prepared, &layout, pages - 1, sections),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    assert!(matches!(
        stage_prepared(&prepared, &layout, pages, sections - 1),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    let different =
        plan_page_layout(FIXTURE, BASE + PAGE_SIZE as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
    assert!(matches!(
        stage_prepared(&prepared, &different, pages, sections),
        Err(Error::Plan(ElfError::LayoutMismatch))
    ));
    let compact = plan_layout(FIXTURE, BASE, MAX_SPAN, MAX_SECTIONS).unwrap();
    assert!(matches!(
        stage_prepared(&prepared, &compact, pages, sections),
        Err(Error::Plan(ElfError::PageAlignment))
    ));
    println!("CYBERMANJU_PREPARED_RUNTIME:rejected_budgets_and_mismatched_layout:PASS");

    // Replan at a real owned base, not by shifting the old numeric plan.
    let envelope =
        linux_module_elf::page_layout_span_bound(FIXTURE, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    assert!(envelope > 0 && envelope.is_multiple_of(PAGE_SIZE));
    for budget in [0, envelope / PAGE_SIZE - 1, usize::MAX] {
        assert!(matches!(
            ReservedModuleLayout::new(FIXTURE, budget, MAX_SECTIONS),
            Err(Error::Plan(ElfError::ResourceLimit))
        ));
    }
    assert!(matches!(
        ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, 0),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    let owned =
        ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS).unwrap();
    let actual_layout = owned.layout();
    let base = owned.range().start as u64;
    assert_eq!(actual_layout.base(), base);
    assert_eq!(owned.range().len(), envelope);
    assert!(base >= 0xffff_8000_0000_0000);
    assert!(actual_layout.span() > 0 && actual_layout.span() <= envelope as u64);
    for section in actual_layout.placements() {
        assert!(section.address >= base);
        assert!(section.address.checked_add(section.size).unwrap() <= owned.range().end as u64);
    }
    let real_resolver = SymbolResolver::new(FIXTURE, actual_layout, &[]).unwrap();
    let real_prepared = real_resolver
        .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
        .unwrap();
    assert!(
        real_prepared
            .sections()
            .iter()
            .zip(prepared.sections())
            .any(|(real, numeric)| real.bytes() != numeric.bytes()),
        "this fixture's absolute relocations must change bytes at the reserved base"
    );
    let actual_span = actual_layout.span() as usize;
    let actual_pages = actual_span / PAGE_SIZE;
    let mut real_expected = vec![0; actual_span];
    for section in real_prepared.sections() {
        let start = (section.placement().address - base) as usize;
        real_expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
    }
    drop(real_resolver);
    let reserved_range = owned.range();
    let limits = PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages: actual_pages,
        max_chunks: real_prepared.sections().len(),
    };
    let mut real_staged = owned.prepare_and_stage(FIXTURE, &[], limits).unwrap();
    assert_eq!(real_staged.reserved_range(), reserved_range);
    assert_eq!(real_staged.layout().base(), base);
    assert_eq!(
        (real_staged.span(), real_staged.page_count()),
        (actual_span, actual_pages)
    );
    let mut real_actual = vec![0xff; actual_span];
    real_staged.read_bytes(0, &mut real_actual).unwrap();
    assert_eq!(
        real_actual, real_expected,
        "reserved-base prepared bytes must roundtrip"
    );
    let mut byte = [0xff];
    assert!(real_staged.read_bytes(actual_span, &mut byte).is_err());
    assert_eq!(byte, [0xff]);
    let other = ostd::mm::module_address::ModuleAddressReservation::new(PAGE_SIZE, 1).unwrap();
    assert!(other.range().end <= reserved_range.start || reserved_range.end <= other.range().start);
    drop(other);
    let mut mapped_actual = vec![0xff; actual_span];
    real_staged
        .read_mapped_for_test(0, &mut mapped_actual)
        .unwrap();
    assert_eq!(
        mapped_actual, real_expected,
        "real VA must read the same prepared bytes"
    );
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:scoped_rw_mapping:PASS pages={}",
        actual_pages
    );
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:reserved_layout:PASS base={:#x} reserved={} span={} sections={} payload={} relocations={} pages={}",
        base,
        envelope,
        actual_span,
        real_prepared.sections().len(),
        real_prepared.payload_bytes(),
        real_prepared.relocation_entries(),
        actual_pages
    );
    drop(real_staged);
    let reused =
        ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS).unwrap();
    assert_eq!(reused.range(), reserved_range);
    drop(reused);
    for bad_layout in [false, true] {
        let failed_owner =
            ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                .unwrap();
        let failed_range = failed_owner.range();
        if bad_layout {
            assert!(matches!(
                failed_owner.stage(&prepared, actual_pages, sections),
                Err(Error::Plan(ElfError::LayoutMismatch))
            ));
        } else {
            assert!(matches!(
                failed_owner.stage(&real_prepared, 0, sections),
                Err(Error::Plan(ElfError::ResourceLimit))
            ));
        }
        let probe = ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
            .unwrap();
        assert_eq!(
            probe.range(),
            failed_range,
            "error must return the consumed reservation"
        );
        drop(probe);
    }
    println!("CYBERMANJU_PREPARED_RUNTIME:reserved_owner_lifecycle:PASS cases=3");
    let narrowed = fixture_with_u32_relocation();
    let low_layout = plan_page_layout(&narrowed, BASE, MAX_SPAN, MAX_SECTIONS).unwrap();
    SymbolResolver::new(&narrowed, &low_layout, &[])
        .unwrap()
        .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
        .unwrap();
    for case in 0..7 {
        let failed_owner =
            ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                .unwrap();
        let failed_range = failed_owner.range();
        assert!(failed_range.start as u64 > u32::MAX as u64);
        let mut limited = limits;
        match case {
            1 => limited.max_payload_bytes = 0,
            2 => limited.max_relocation_entries = 0,
            3 => limited.max_sections = 0,
            4 => limited.max_pages = 0,
            5 => limited.max_chunks = 0,
            _ => (),
        }
        let input = match case {
            0 => &FIXTURE[..64],
            6 => narrowed.as_slice(),
            _ => FIXTURE,
        };
        let result = failed_owner.prepare_and_stage(input, &[], limited);
        match case {
            0 => assert!(matches!(result, Err(Error::Plan(_)))),
            6 => assert!(matches!(
                result,
                Err(Error::Plan(ElfError::RelocationOverflow))
            )),
            _ => assert!(matches!(result, Err(Error::Plan(ElfError::ResourceLimit)))),
        }
        let probe = ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
            .unwrap();
        assert_eq!(
            probe.range(),
            failed_range,
            "failed transaction must release its consumed reservation"
        );
        drop(probe);
    }
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:prepared_transaction:PASS rejection_cases=7 range_overflow=true reservation_released=true"
    );
    // No pointer conversion, function pointer or call to an ELF symbol.
    println!("CYBERMANJU_PREPARED_RUNTIME:no_init_or_exit_called:PASS");
}
