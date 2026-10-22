// SPDX-License-Identifier: MPL-2.0
//! Trusted, embedded kbuild fixture. Deliberately never calls any module entry.
use alloc::{vec, vec::Vec};

use linux_module_elf::{Error as ElfError, Export, SymbolResolver, plan_layout, plan_page_layout};
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

// Clone-only fixture instrumentation: an existing global symbol becomes an
// undefined numeric import; section layout and symbol-table local boundary stay intact.
fn relative_import_fixture(kind: u32) -> (Vec<u8>, &'static [u8], u16, u64) {
    let r32 = |o| u32::from_le_bytes(FIXTURE[o..o + 4].try_into().unwrap());
    let r64 = |o| u64::from_le_bytes(FIXTURE[o..o + 8].try_into().unwrap());
    let mut bytes = fixture_with_u32_relocation();
    let changed = bytes.iter().zip(FIXTURE).position(|(a, b)| a != b).unwrap();
    let table = usize::try_from(r64(40)).unwrap();
    let count = u16::from_le_bytes(FIXTURE[60..62].try_into().unwrap()) as usize;
    for i in 0..count {
        let sh = table + i * 64;
        if r32(sh + 4) != 4 {
            continue;
        }
        let off = usize::try_from(r64(sh + 24)).unwrap();
        let size = usize::try_from(r64(sh + 32)).unwrap();
        if changed < off || changed >= off + size {
            continue;
        }
        assert_eq!((changed - off) % 24, 8);
        let entry = changed - 8;
        let symsh = table + r32(sh + 40) as usize * 64;
        let symbols = usize::try_from(r64(symsh + 24)).unwrap();
        let strsh = table + r32(symsh + 40) as usize * 64;
        let strings = usize::try_from(r64(strsh + 24)).unwrap();
        let strings_end = strings + usize::try_from(r64(strsh + 32)).unwrap();
        for symidx in 1..usize::try_from(r64(symsh + 32)).unwrap() / 24 {
            let symbol = symbols + symidx * 24;
            if FIXTURE[symbol + 4] >> 4 != 1 || r32(symbol) == 0 {
                continue;
            }
            let start = strings + r32(symbol) as usize;
            let end = start
                + FIXTURE[start..strings_end]
                    .iter()
                    .position(|b| *b == 0)
                    .unwrap();
            bytes[symbol + 4] = 0x10; // STB_GLOBAL/STT_NOTYPE
            bytes[symbol + 5] = 0;
            bytes[symbol + 6..symbol + 24].fill(0); // SHN_UNDEF/value/size
            bytes[entry + 8..entry + 16]
                .copy_from_slice(&(((symidx as u64) << 32) | u64::from(kind)).to_le_bytes());
            return (
                bytes,
                &FIXTURE[start..end],
                u16::try_from(r32(sh + 44)).unwrap(),
                r64(entry),
            );
        }
    }
    panic!("fixture needs an existing named global symbol and ALLOC RELA");
}

// Cross-check diagnostic metadata against the cloned ELF, not a guessed section index.
fn assert_reach_site(
    bytes: &[u8],
    failure: linux_module_elf::RelocationReachError,
    target: u16,
    offset: u64,
    kind: u32,
) {
    let r32 = |o| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let r64 = |o| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
    let site = failure.site.expect("entry failure needs a diagnostic site");
    assert_eq!(
        (site.target_section, site.offset, site.kind),
        (target, offset, kind)
    );
    let sh = usize::try_from(r64(40)).unwrap() + usize::from(site.rela_section) * 64;
    assert_eq!(r32(sh + 4), 4);
    assert_eq!(r32(sh + 44), u32::from(target));
    assert!(site.entry_index < usize::try_from(r64(sh + 32)).unwrap() / 24);
    let entry = usize::try_from(r64(sh + 24)).unwrap() + site.entry_index * 24;
    let info = r64(entry + 8);
    assert_eq!(
        (site.symbol_index, site.kind, site.offset),
        ((info >> 32) as u32, info as u32, r64(entry))
    );
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
    for kind in [2, 4] {
        let (bytes, name, target, offset) = relative_import_fixture(kind);
        for delta in [
            i128::from(i32::MIN),
            i128::from(i32::MAX),
            i128::from(i32::MIN) - 1,
            i128::from(i32::MAX) + 1,
        ] {
            let owner =
                ReservedModuleLayout::new(&bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                    .unwrap();
            let range = owner.range();
            let placement = owner
                .layout()
                .placements()
                .iter()
                .find(|p| p.section_index == target)
                .unwrap();
            let place = placement.address.checked_add(offset).unwrap();
            let relative_offset = usize::try_from(place - owner.layout().base()).unwrap();
            let exports = [Export {
                name,
                value: u64::try_from(i128::from(place) + delta).unwrap(),
            }];
            let preflight = SymbolResolver::new(&bytes, owner.layout(), &exports)
                .unwrap()
                .check_relocation_reach(65_536);
            if i32::try_from(delta).is_ok() {
                assert_eq!(preflight.unwrap(), real_prepared.relocation_entries());
                let staged = owner.prepare_and_stage(&bytes, &exports, limits).unwrap();
                let mut actual = [0; 4];
                staged.read_bytes(relative_offset, &mut actual).unwrap();
                assert_eq!(actual, i32::try_from(delta).unwrap().to_le_bytes());
                drop(staged);
            } else {
                assert_eq!(preflight, Err(ElfError::RelocationOverflow));
                assert!(matches!(
                    owner.prepare_and_stage(&bytes, &exports, limits),
                    Err(Error::Plan(ElfError::RelocationOverflow))
                ));
            }
            let probe =
                ReservedModuleLayout::new(&bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                    .unwrap();
            assert_eq!(probe.range(), range);
            drop(probe);
        }
        let owner =
            ReservedModuleLayout::new(&bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS).unwrap();
        let far = [Export {
            name,
            value: 0xffff_ffff_8000_0000,
        }];
        let failure = SymbolResolver::new(&bytes, owner.layout(), &far)
            .unwrap()
            .diagnose_relocation_reach(65_536)
            .unwrap_err();
        assert_eq!(failure.cause, ElfError::RelocationOverflow);
        assert_reach_site(&bytes, failure, target, offset, kind);
        assert_eq!(
            SymbolResolver::new(&bytes, owner.layout(), &far)
                .unwrap()
                .check_relocation_reach(65_536),
            Err(ElfError::RelocationOverflow)
        );
        assert!(matches!(
            owner.prepare_and_stage(&bytes, &far, limits),
            Err(Error::Plan(ElfError::RelocationOverflow))
        ));
        // Real linked kernel function address, but a synthetic import name/binding.
        // This number is never converted back to a callable module pointer.
        let owner =
            ReservedModuleLayout::new(&bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS).unwrap();
        let code_address = ostd::cpu::num_cpus as *const () as usize as u64;
        assert!(code_address >= 0xffff_ffff_8000_0000);
        let real = [Export {
            name,
            value: code_address,
        }];
        let failure = SymbolResolver::new(&bytes, owner.layout(), &real)
            .unwrap()
            .diagnose_relocation_reach(65_536)
            .unwrap_err();
        assert_eq!(failure.cause, ElfError::RelocationOverflow);
        assert_reach_site(&bytes, failure, target, offset, kind);
        assert!(matches!(
            owner.prepare_and_stage(&bytes, &real, limits),
            Err(Error::Plan(ElfError::RelocationOverflow))
        ));
        // The failed transaction consumed its owner. A fresh reservation is
        // observed only; the numeric candidate is never reserved or mapped.
        let owner =
            ReservedModuleLayout::new(&bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS).unwrap();
        let current_layout = owner.layout();
        let current_placement = current_layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let place_offset = current_placement
            .address
            .checked_sub(current_layout.base())
            .unwrap()
            .checked_add(offset)
            .unwrap();
        // relative_import_fixture preserves the original zero-addend RELA.
        let expected_window = linux_module_elf::relative32_base_window(
            code_address,
            0,
            place_offset,
            current_layout.span(),
        )
        .expect("a 32-bit reachability interval exists for the numeric import");
        let collected = SymbolResolver::new(&bytes, current_layout, &real)
            .unwrap()
            .relative32_import_constraints(65_536)
            .unwrap();
        assert_eq!(collected.import_relocations(), 1);
        assert_eq!(collected.rela_entries(), real_prepared.relocation_entries());
        assert_eq!(collected.ignored_relative32_relocations(), 0);
        let window = collected.window().unwrap();
        assert_eq!(window, expected_window);
        assert!(!window.contains(owner.range().start as u64));
        assert_eq!(
            SymbolResolver::new(&bytes, current_layout, &real)
                .unwrap()
                .check_relocation_reach(65_536),
            Err(ElfError::RelocationOverflow)
        );
        let candidate = window
            .first_aligned(PAGE_SIZE as u64)
            .expect("reachable page-aligned numeric base");
        assert!(window.contains(candidate));
        let candidate_layout = plan_page_layout(&bytes, candidate, MAX_SPAN, MAX_SECTIONS).unwrap();
        assert_eq!(
            SymbolResolver::new(&bytes, &candidate_layout, &real)
                .unwrap()
                .check_relocation_reach(65_536)
                .unwrap(),
            real_prepared.relocation_entries()
        );
        let candidate_placement = candidate_layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let candidate_place_offset = candidate_placement
            .address
            .checked_sub(candidate_layout.base())
            .unwrap()
            .checked_add(offset)
            .unwrap();
        let displacement =
            i128::from(code_address) - i128::from(candidate) - i128::from(candidate_place_offset);
        assert!(i32::try_from(displacement).is_ok());
        drop(owner);
        if kind == 2 {
            let owner =
                ReservedModuleLayout::new(&bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                    .unwrap();
            let failure = SymbolResolver::new(&bytes, owner.layout(), &[])
                .unwrap()
                .diagnose_relocation_reach(65_536)
                .unwrap_err();
            assert_eq!(failure.cause, ElfError::MissingSymbol);
            assert_reach_site(&bytes, failure, target, offset, kind);
            assert_eq!(
                SymbolResolver::new(&bytes, owner.layout(), &[])
                    .unwrap()
                    .check_relocation_reach(65_536),
                Err(ElfError::MissingSymbol)
            );
            assert!(matches!(
                owner.prepare_and_stage(&bytes, &[], limits),
                Err(Error::Plan(ElfError::MissingSymbol))
            ));
        }
    }
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:relative32_reach:PASS types=2 boundary_cases=8 missing_exports=1 far_exports=2"
    );
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:relative32_placement_window:PASS kinds=2 rejected_current_base=2 candidate_preflight=2 reserved_near_text=false execution=false"
    );
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:relative32_import_collection:PASS kinds=2 collected_imports=2 candidate_preflight=2 reserved_near_text=false execution=false"
    );
    // Defined section-relative PC32/PLT32 entries cancel the module base.
    for kind in [2, 4] {
        let (mut defined, _, target, offset) = relative_import_fixture(kind);
        let r32 = |b: &[u8], o| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let r64 = |b: &[u8], o| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        let table = r64(&defined, 40) as usize;
        let count = u16::from_le_bytes(defined[60..62].try_into().unwrap()) as usize;
        let mut picked = None;
        for si in 0..count {
            let rela = table + si * 64;
            if r32(&defined, rela + 4) != 4 || r32(&defined, rela + 44) != u32::from(target) {
                continue;
            }
            let off = r64(&defined, rela + 24) as usize;
            let size = r64(&defined, rela + 32) as usize;
            for entry in (off..off + size).step_by(24) {
                let info = r64(&defined, entry + 8);
                if info as u32 == kind && r64(&defined, entry) == offset {
                    picked = Some(((info >> 32) as usize, r32(&defined, rela + 40) as usize));
                    break;
                }
            }
            if picked.is_some() {
                break;
            }
        }
        let (symbol_index, symtab_index) = picked.expect("selected relative RELA");
        let symsh = table + symtab_index * 64;
        let symoff = r64(&defined, symsh + 24) as usize + symbol_index * 24;
        defined[symoff..symoff + 24].copy_from_slice(&FIXTURE[symoff..symoff + 24]);
        let owner =
            ReservedModuleLayout::new(&defined, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                .unwrap();
        let current_layout = owner.layout();
        let base = SymbolResolver::new(&defined, current_layout, &[])
            .unwrap()
            .relative32_placement_constraints(65_536)
            .unwrap();
        assert_eq!(
            (
                base.section_relative_relocations(),
                base.import_relocations(),
                base.absolute_relocations()
            ),
            (1, 0, 0)
        );
        assert_eq!(base.rela_entries(), real_prepared.relocation_entries());
        let window = base.window().unwrap();
        assert_eq!(
            window,
            linux_module_elf::Relative32BaseWindow::for_module_span(current_layout.span()).unwrap()
        );
        assert!(window.contains(owner.range().start as u64));
        assert_eq!(
            SymbolResolver::new(&defined, current_layout, &[])
                .unwrap()
                .check_relocation_reach(65_536)
                .unwrap(),
            base.rela_entries()
        );
        drop(owner);

        let mut absolute = defined.clone();
        let address = ostd::cpu::num_cpus as *const () as usize as u64;
        assert!(address >= 0xffff_ffff_8000_0000);
        absolute[symoff + 6..symoff + 8].copy_from_slice(&0xfff1u16.to_le_bytes());
        absolute[symoff + 8..symoff + 16].copy_from_slice(&address.to_le_bytes());
        let owner =
            ReservedModuleLayout::new(&absolute, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                .unwrap();
        let current_layout = owner.layout();
        let constraints = SymbolResolver::new(&absolute, current_layout, &[])
            .unwrap()
            .relative32_placement_constraints(65_536)
            .unwrap();
        assert_eq!(
            (
                constraints.absolute_relocations(),
                constraints.section_relative_relocations(),
                constraints.import_relocations()
            ),
            (1, 0, 0)
        );
        assert_eq!(
            constraints.rela_entries(),
            real_prepared.relocation_entries()
        );
        assert!(
            !constraints
                .window()
                .unwrap()
                .contains(owner.range().start as u64)
        );
        let old = SymbolResolver::new(&absolute, current_layout, &[])
            .unwrap()
            .relative32_import_constraints(65_536)
            .unwrap();
        assert_eq!(old.ignored_relative32_relocations(), 1);
        let candidate = constraints
            .window()
            .unwrap()
            .first_aligned(PAGE_SIZE as u64)
            .unwrap();
        let candidate_layout =
            plan_page_layout(&absolute, candidate, MAX_SPAN, MAX_SECTIONS).unwrap();
        assert_eq!(
            SymbolResolver::new(&absolute, &candidate_layout, &[])
                .unwrap()
                .check_relocation_reach(65_536)
                .unwrap(),
            constraints.rela_entries()
        );
        drop(owner);
    }
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:relative32_defined_collection:PASS types=2 section_relative=2 absolute=2 kernel_address_numeric=true reserved_near_text=false execution=false"
    );
    let owner =
        ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS).unwrap();
    let failure = SymbolResolver::new(FIXTURE, owner.layout(), &[])
        .unwrap()
        .diagnose_relocation_reach(0)
        .unwrap_err();
    assert_eq!(failure.cause, ElfError::ResourceLimit);
    assert_eq!(failure.site, None);
    drop(owner);
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:reach_diagnostics:PASS overflow_sites=4 missing_sites=1 budget_no_site=1 real_code_exports=2"
    );
    // R_X86_64_32S accepts sign-extended high-half absolute addresses, unlike
    // PC-relative relocations. The imported kernel address is data only.
    let (signed_bytes, signed_name, signed_target, signed_offset) = relative_import_fixture(11);
    let signed_owner =
        ReservedModuleLayout::new(&signed_bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
            .unwrap();
    let signed_range = signed_owner.range();
    let signed_place = signed_owner
        .layout()
        .placements()
        .iter()
        .find(|p| p.section_index == signed_target)
        .unwrap()
        .address
        .checked_add(signed_offset)
        .unwrap();
    let signed_relative = usize::try_from(signed_place - signed_owner.layout().base()).unwrap();
    let kernel_code_address = ostd::cpu::num_cpus as *const () as usize as u64;
    assert!(kernel_code_address >= 0xffff_ffff_8000_0000);
    assert_ne!(kernel_code_address, 0xffff_ffff_8000_0000);
    let accepted_values = [0xffff_ffff_8000_0000, kernel_code_address];
    drop(signed_owner);
    for value in accepted_values {
        let signed_owner =
            ReservedModuleLayout::new(&signed_bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                .unwrap();
        assert_eq!(signed_owner.range(), signed_range);
        let exports = [Export {
            name: signed_name,
            value,
        }];
        assert_eq!(
            SymbolResolver::new(&signed_bytes, signed_owner.layout(), &exports)
                .unwrap()
                .check_relocation_reach(65_536)
                .unwrap(),
            real_prepared.relocation_entries()
        );
        let staged = signed_owner
            .prepare_and_stage(&signed_bytes, &exports, limits)
            .unwrap();
        let mut actual = [0; 4];
        staged.read_bytes(signed_relative, &mut actual).unwrap();
        assert_eq!(actual, (value as u32).to_le_bytes());
        drop(staged);
    }
    let rejected_values = [
        0x8000_0000,
        0xffff_ffff_7fff_ffff,
        signed_range.start as u64,
    ];
    for value in rejected_values {
        let owner =
            ReservedModuleLayout::new(&signed_bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                .unwrap();
        let range = owner.range();
        assert_eq!(range, signed_range);
        let exports = [Export {
            name: signed_name,
            value,
        }];
        let failure = SymbolResolver::new(&signed_bytes, owner.layout(), &exports)
            .unwrap()
            .diagnose_relocation_reach(65_536)
            .unwrap_err();
        assert_eq!(failure.cause, ElfError::RelocationOverflow);
        assert_reach_site(&signed_bytes, failure, signed_target, signed_offset, 11);
        assert_eq!(
            SymbolResolver::new(&signed_bytes, owner.layout(), &exports)
                .unwrap()
                .check_relocation_reach(65_536),
            Err(ElfError::RelocationOverflow)
        );
        assert!(matches!(
            owner.prepare_and_stage(&signed_bytes, &exports, limits),
            Err(Error::Plan(ElfError::RelocationOverflow))
        ));
        let probe =
            ReservedModuleLayout::new(&signed_bytes, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                .unwrap();
        assert_eq!(
            probe.range(),
            range,
            "failed owner must release its reservation"
        );
        drop(probe);
    }
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:signed32_absolute:PASS accepted=2 rejected=3 kernel_export_numeric=true execution=false"
    );
    requested_layout_runtime(envelope, limits);
    relative32_reservation_runtime(envelope, limits);
    // No module bytes converted to function pointers and no ELF symbol called.
    println!("CYBERMANJU_PREPARED_RUNTIME:no_init_or_exit_called:PASS");
}

// The candidate is deliberately not the allocator's ordinary choice. This
// catches a constructor that silently falls back to automatic placement.
fn requested_layout_runtime(envelope: usize, mut limits: PreparationLimits) {
    use ostd::mm::module_address::ModuleAddressReservation;
    let max_pages = MAX_SPAN as usize / PAGE_SIZE;
    let automatic = ModuleAddressReservation::new(envelope, max_pages).unwrap();
    let automatic_base = automatic.range().start;
    drop(automatic);
    let probe_span = envelope.checked_add(2 * PAGE_SIZE).unwrap();
    let probe = ModuleAddressReservation::new(probe_span, max_pages).unwrap();
    let base = probe.range().start;
    drop(probe);
    assert_ne!(
        base, automatic_base,
        "candidate must differ from automatic placement"
    );
    let requested_range = base..base.checked_add(envelope).unwrap();

    // Independent numeric plan at the requested address, not the owner's layout.
    let expected_layout = plan_page_layout(FIXTURE, base as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
    let expected_module = SymbolResolver::new(FIXTURE, &expected_layout, &[])
        .unwrap()
        .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
        .unwrap();
    let span = expected_layout.span() as usize;
    let pages = span / PAGE_SIZE;
    limits.max_pages = pages;
    let mut expected = vec![0; span];
    for section in expected_module.sections() {
        let start = usize::try_from(section.placement().address - base as u64).unwrap();
        expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
    }
    let owner = ReservedModuleLayout::new_at(FIXTURE, base, max_pages, MAX_SECTIONS).unwrap();
    assert_eq!(
        owner.range(),
        requested_range,
        "requested ELF plan must own the exact base"
    );
    assert_eq!(owner.layout().base(), base as u64);
    assert_eq!(owner.layout().span(), expected_layout.span());
    let staged = owner.prepare_and_stage(FIXTURE, &[], limits).unwrap();
    assert_eq!(staged.reserved_range(), requested_range);
    assert_eq!(staged.layout().base(), base as u64);
    assert_eq!((staged.span(), staged.page_count()), (span, pages));
    let mut actual = vec![0xff; span];
    staged.read_bytes(0, &mut actual).unwrap();
    assert_eq!(
        actual, expected,
        "all payload, relocation, gap and tail bytes at requested base"
    );
    assert!(
        matches!(
            ModuleAddressReservation::new_at(base, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ),
        "staged owner must retain its exact reservation"
    );
    let other = ModuleAddressReservation::new(PAGE_SIZE, 1).unwrap();
    assert!(
        other.range().end <= requested_range.start || requested_range.end <= other.range().start
    );
    drop(other);
    drop(staged);
    let reclaim = ModuleAddressReservation::new_at(base, envelope, max_pages).unwrap();
    assert_eq!(reclaim.range(), requested_range);
    drop(reclaim);

    // Eight invalid constructors: no owner is returned and the exact candidate
    // remains reusable after each failure. Address validation is not ELF reach.
    for case in 0..8 {
        let result = match case {
            0 => ReservedModuleLayout::new_at(FIXTURE, base, 0, MAX_SECTIONS),
            1 => {
                ReservedModuleLayout::new_at(FIXTURE, base, envelope / PAGE_SIZE - 1, MAX_SECTIONS)
            }
            2 => ReservedModuleLayout::new_at(FIXTURE, base, usize::MAX, MAX_SECTIONS),
            3 => ReservedModuleLayout::new_at(FIXTURE, base, max_pages, 0),
            4 => ReservedModuleLayout::new_at(&FIXTURE[..64], base, max_pages, MAX_SECTIONS),
            5 => ReservedModuleLayout::new_at(FIXTURE, base + 1, max_pages, MAX_SECTIONS),
            6 => ReservedModuleLayout::new_at(
                FIXTURE,
                ostd::cpu::num_cpus as *const () as usize,
                max_pages,
                MAX_SECTIONS,
            ),
            7 => ReservedModuleLayout::new_at(
                FIXTURE,
                usize::MAX & !(PAGE_SIZE - 1),
                max_pages,
                MAX_SECTIONS,
            ),
            _ => unreachable!(),
        };
        match case {
            0..=3 => assert!(matches!(result, Err(Error::Plan(ElfError::ResourceLimit)))),
            4 => assert!(matches!(result, Err(Error::Plan(_)))),
            _ => assert!(matches!(
                result,
                Err(Error::AddressSpace(ostd::Error::InvalidArgs))
            )),
        }
        let reclaim = ModuleAddressReservation::new_at(base, envelope, max_pages).unwrap();
        assert_eq!(reclaim.range(), requested_range);
        drop(reclaim);
    }
    // A single occupied interior page must prevent claiming the full envelope.
    let occupied = ModuleAddressReservation::new_at(base + PAGE_SIZE, PAGE_SIZE, 1).unwrap();
    assert!(matches!(
        ReservedModuleLayout::new_at(FIXTURE, base, max_pages, MAX_SECTIONS),
        Err(Error::AddressSpace(ostd::Error::NoMemory))
    ));
    assert_eq!(occupied.range(), base + PAGE_SIZE..base + 2 * PAGE_SIZE);
    drop(occupied);
    let occupied = ReservedModuleLayout::new_at(FIXTURE, base, max_pages, MAX_SECTIONS).unwrap();
    assert!(matches!(
        ReservedModuleLayout::new_at(FIXTURE, base, max_pages, MAX_SECTIONS),
        Err(Error::AddressSpace(ostd::Error::NoMemory))
    ));
    assert_eq!(occupied.range(), requested_range);
    drop(occupied);

    // Both preparation and staging failures consume/release the exact owner.
    // Narrowing a real absolute relocation is valid at BASE but not at VMALLOC.
    let narrowed = fixture_with_u32_relocation();
    for case in 0..3 {
        let owner = ReservedModuleLayout::new_at(FIXTURE, base, max_pages, MAX_SECTIONS).unwrap();
        assert_eq!(owner.range(), requested_range);
        let mut limited = limits;
        let bytes = if case == 0 {
            narrowed.as_slice()
        } else {
            FIXTURE
        };
        match case {
            1 => limited.max_payload_bytes = 0,
            2 => limited.max_pages = 0,
            _ => (),
        }
        let failed = owner.prepare_and_stage(bytes, &[], limited);
        if case == 0 {
            assert!(matches!(
                failed,
                Err(Error::Plan(ElfError::RelocationOverflow))
            ));
        } else {
            assert!(matches!(failed, Err(Error::Plan(ElfError::ResourceLimit))));
        }
        let reclaim = ReservedModuleLayout::new_at(FIXTURE, base, max_pages, MAX_SECTIONS).unwrap();
        assert_eq!(
            reclaim.range(),
            requested_range,
            "failed exact-base transaction must release its reservation"
        );
        drop(reclaim);
    }
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:requested_layout:PASS base={:#x} reserved={} span={} pages={} invalid_cases=8 conflict_cases=2 rollback_cases=3 exact_reclaim=true mapping=false execution=false",
        base, envelope, span, pages
    );
}

// Search is only a bounded prefix in the existing VMALLOC pool. Synthetic
// near-by imports exercise PC32/PLT32 without claiming Linux ABI or kernel exports.
fn relative32_reservation_runtime(envelope: usize, mut limits: PreparationLimits) {
    use linux_module_elf::Relative32BaseWindow;
    use ostd::mm::module_address::ModuleAddressReservation;
    let max_pages = MAX_SPAN as usize / PAGE_SIZE;
    let probe = ModuleAddressReservation::new(envelope + 3 * PAGE_SIZE, max_pages).unwrap();
    let base = probe.range().start;
    drop(probe);
    let second = base + PAGE_SIZE;
    let filter = Relative32BaseWindow::for_module_span(envelope as u64)
        .unwrap()
        .restrict(base as u64, second as u64)
        .unwrap();
    for kind in [2, 4] {
        let (bytes, name, target, offset) = relative_import_fixture(kind);
        let initial = plan_page_layout(&bytes, base as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
        let field = initial
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap()
            .address
            .checked_add(offset)
            .unwrap();
        let exports = [Export {
            name,
            value: field.checked_add(29).unwrap(),
        }];
        let constraints = SymbolResolver::new(&bytes, &initial, &exports)
            .unwrap()
            .relative32_placement_constraints(65_536)
            .unwrap();
        assert_eq!(constraints.import_relocations(), 1);
        let window = constraints.window().unwrap().intersect(filter).unwrap();
        assert!(window.contains(base as u64) && window.contains(second as u64));
        // Only the first candidate's initial page is held. The next base must
        // remain available even though whole module envelopes overlap.
        let occupied = ModuleAddressReservation::new_at(base, PAGE_SIZE, 1).unwrap();
        let owner = ReservedModuleLayout::new_in_relative32_window(
            &bytes,
            &exports,
            window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            2,
        )
        .unwrap();
        assert_eq!(
            owner.range().start,
            second,
            "occupied first candidate must be skipped"
        );
        assert_eq!(owner.layout().base(), second as u64);
        assert_eq!(occupied.range(), base..second);
        let selected = plan_page_layout(&bytes, second as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
        let prepared = SymbolResolver::new(&bytes, &selected, &exports)
            .unwrap()
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
            .unwrap();
        let span = selected.span() as usize;
        limits.max_pages = span / PAGE_SIZE;
        let mut expected = vec![0; span];
        for section in prepared.sections() {
            let start = usize::try_from(section.placement().address - second as u64).unwrap();
            expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
        }
        // Also compare the direct displacement against an independent scalar.
        let selected_field = selected
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap()
            .address
            .checked_add(offset)
            .unwrap();
        let delta =
            i32::try_from(i128::from(exports[0].value) - i128::from(selected_field)).unwrap();
        let relative = usize::try_from(selected_field - second as u64).unwrap();
        assert_eq!(&expected[relative..relative + 4], &delta.to_le_bytes());
        let staged = owner.prepare_and_stage(&bytes, &exports, limits).unwrap();
        let range = staged.reserved_range();
        assert_eq!(range, second..second + envelope);
        let mut actual = vec![0xff; span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(
            actual, expected,
            "selected candidate's complete prepared span"
        );
        assert!(matches!(
            ModuleAddressReservation::new_at(second, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ));
        drop(staged);
        let reclaim = ModuleAddressReservation::new_at(second, envelope, max_pages).unwrap();
        assert_eq!(reclaim.range(), range);
        drop(reclaim);
        drop(occupied);
    }
    let narrowed = fixture_with_u32_relocation();
    let (imported, name, _, _) = relative_import_fixture(2);
    let far_exports = [Export {
        name,
        value: ostd::cpu::num_cpus as *const () as usize as u64,
    }];
    for case in 0..8 {
        let occupied = if case == 3 {
            Some(ModuleAddressReservation::new_at(base, PAGE_SIZE, 1).unwrap())
        } else {
            None
        };
        let bytes = match case {
            4 => narrowed.as_slice(),
            5 | 7 => imported.as_slice(),
            _ => FIXTURE,
        };
        let exports = if case == 7 {
            far_exports.as_slice()
        } else {
            &[]
        };
        let window = if case == 2 {
            filter.restrict(base as u64 + 1, second as u64 - 1).unwrap()
        } else {
            filter
        };
        let attempts = match case {
            0 => 0,
            1 => 65,
            3 => 1,
            _ => 2,
        };
        let entries = if case == 6 { 0 } else { 65_536 };
        let result = ReservedModuleLayout::new_in_relative32_window(
            bytes,
            exports,
            window,
            max_pages,
            MAX_SECTIONS,
            entries,
            attempts,
        );
        match case {
            0 | 1 | 6 => assert!(matches!(result, Err(Error::Plan(ElfError::ResourceLimit)))),
            2 | 3 => assert!(matches!(
                result,
                Err(Error::AddressSpace(ostd::Error::NoMemory))
            )),
            4 | 7 => assert!(matches!(
                result,
                Err(Error::Plan(ElfError::RelocationOverflow))
            )),
            5 => assert!(matches!(result, Err(Error::Plan(ElfError::MissingSymbol)))),
            _ => unreachable!(),
        }
        if let Some(occupied) = occupied {
            assert_eq!(occupied.range(), base..second);
            // Failure of a one-attempt prefix says nothing about the next free base.
            let next =
                ReservedModuleLayout::new_at(FIXTURE, second, max_pages, MAX_SECTIONS).unwrap();
            assert_eq!(next.range().start, second);
            drop(next);
            drop(occupied);
        }
        let reclaim = ModuleAddressReservation::new_at(base, envelope, max_pages).unwrap();
        assert_eq!(reclaim.range(), base..base + envelope);
        drop(reclaim);
    }
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:relative32_reservation:PASS kinds=2 occupied_candidates=2 staged_cases=2 rejection_cases=8 reclaimed_cases=4 attempts_bounded=true reserved_near_text=false mapping=false execution=false"
    );
}

mod c_abi;
mod near_text;
mod registry;
mod signed32;

mod signed32_search;
