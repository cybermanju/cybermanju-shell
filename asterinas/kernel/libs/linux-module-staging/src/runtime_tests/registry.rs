// SPDX-License-Identifier: MPL-2.0
//! Runtime coverage for the explicit OSTD export registry; never maps or executes.
use alloc::{sync::Arc, vec, vec::Vec};

use linux_module_elf::{Error as ElfError, SymbolResolver, plan_page_layout};
use ostd::{
    cpu,
    mm::{
        PAGE_SIZE,
        module_address::NearTextAddressReservation,
        module_staging::fault_injection::{FaultMode, FaultStats},
    },
    prelude::*,
};

use super::{FIXTURE, MAX_SECTIONS, MAX_SPAN, PreparationLimits, relative_import_fixture};
use crate::{
    Error, ExportAbi, ExportSignature, LinuxExportAdmissionError, NearTextModuleLayout,
    OSTD_NUM_CPUS_EXPORT_NAME, OstdExportRegistry, PolicyNearTextModuleLayout,
};

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
    let described = registry.describe(OSTD_NUM_CPUS_EXPORT_NAME).unwrap();
    assert_eq!(described.name(), OSTD_NUM_CPUS_EXPORT_NAME);
    let independent_function: fn() -> usize = cpu::num_cpus;
    let independent_value = independent_function as *const () as usize as u64;
    assert_eq!(described.value(), independent_value);
    assert_eq!(described.abi(), ExportAbi::RustInternal);
    assert_eq!(described.signature(), ExportSignature::NoArgsUsize);
    assert_eq!(
        registry.admit_linux_export(OSTD_NUM_CPUS_EXPORT_NAME),
        Err(LinuxExportAdmissionError::AbiNotAdmitted),
        "internal Rust ABI must not be admitted as Linux"
    );
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
        assert_eq!(
            registry.describe(name),
            None,
            "descriptor lookup must reject unregistered names"
        );
        assert_eq!(
            registry.admit_linux_export(name),
            Err(LinuxExportAdmissionError::UnregisteredName),
            "unknown export must retain name rejection"
        );
    }

    println!(
        "CYBERMANJU_INTERNAL_EXPORTS:admission:PASS described=1 rust_internal=1 rejected_abi=1 rejected_names=5 numeric_export_preserved=true no_admitted_linux_exports=true"
    );

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
    let local_layout = plan_page_layout(FIXTURE, base as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
    let local_prepared = registry
        .linux_symbol_resolver(FIXTURE, &local_layout)
        .unwrap()
        .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
        .expect("local-only fixture must prepare with the policy resolver");
    assert_eq!(
        (
            local_layout.span(),
            local_prepared.payload_bytes(),
            local_prepared.relocation_entries()
        ),
        (40_960, 1_721, 6)
    );
    let mut local_only_prepared = 0usize;
    local_only_prepared += 1;
    drop(local_prepared);
    let mut checked_unknown = 0usize;
    let mut checked_abi = 0usize;
    let mut preflight_rejections = 0usize;
    let mut compared = 0usize;
    let mut missing = 0usize;
    let mut reclaimed = 0usize;
    let mut policy_reclaimed = 0usize;
    let mut ctor_abi = 0usize;
    let mut ctor_unknown = 0usize;
    let mut prepare_abi = 0usize;
    let mut occupied_precedence = 0usize;
    for kind in [2u32, 4] {
        let (original, _alias, target, offset) = relative_import_fixture(kind);
        let checked_layout =
            plan_page_layout(&original, base as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
        assert!(
            matches!(
                registry
                    .linux_symbol_resolver(&original, &checked_layout)
                    .unwrap()
                    .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS),
                Err(ElfError::MissingSymbol)
            ),
            "policy resolver must reject unknown imports"
        );
        checked_unknown += 1;
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
        let checked = registry
            .linux_symbol_resolver(&bytes, &page_layout)
            .unwrap();
        let failure = checked
            .diagnose_relocation_reach(65_536)
            .expect_err("policy preflight must reject internal Rust ABI");
        assert_eq!(
            failure.cause,
            ElfError::ExportNotAdmitted,
            "policy preflight must preserve admission denial"
        );
        let site = failure.site.expect("denial should identify its relocation");
        assert_eq!(
            (site.target_section, site.kind, site.offset),
            (target, kind, offset)
        );
        preflight_rejections += 1;
        assert!(
            matches!(
                checked.prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS),
                Err(ElfError::ExportNotAdmitted)
            ),
            "policy resolver must not fall back to numeric exports"
        );
        checked_abi += 1;
        let held = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        assert!(
            matches!(
                PolicyNearTextModuleLayout::new_signed32_at(
                    &bytes,
                    base,
                    max_pages,
                    MAX_SECTIONS,
                    65_536
                ),
                Err(Error::AddressSpace(ostd::Error::NoMemory))
            ),
            "policy owner must claim before admission"
        );
        drop(held);
        occupied_precedence += 1;
        assert!(
            matches!(
                PolicyNearTextModuleLayout::new_signed32_at(
                    &bytes,
                    base,
                    max_pages,
                    MAX_SECTIONS,
                    65_536
                ),
                Err(Error::Plan(ElfError::ExportNotAdmitted))
            ),
            "policy owner constructor must enforce admission"
        );
        ctor_abi += 1;
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        policy_reclaimed += 1;
        assert!(
            matches!(
                PolicyNearTextModuleLayout::new_signed32_at(
                    &original,
                    base,
                    max_pages,
                    MAX_SECTIONS,
                    65_536
                ),
                Err(Error::Plan(ElfError::MissingSymbol))
            ),
            "policy owner constructor must reject unknown imports"
        );
        ctor_unknown += 1;
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        policy_reclaimed += 1;
        let owner = PolicyNearTextModuleLayout::new_signed32_at(
            FIXTURE,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        assert_eq!(owner.range(), base..base + envelope);
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ));
        assert!(
            matches!(
                owner.prepare_and_stage(&bytes, limits),
                Err(Error::Plan(ElfError::ExportNotAdmitted))
            ),
            "policy owner prepare must revalidate admission"
        );
        prepare_abi += 1;
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        policy_reclaimed += 1;
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
    let policy_owner =
        PolicyNearTextModuleLayout::new_signed32_at(FIXTURE, base, max_pages, MAX_SECTIONS, 65_536)
            .unwrap();
    assert_eq!(policy_owner.range(), base..base + envelope);
    let fixture_layout = policy_owner.layout();
    let expected_prepared = SymbolResolver::new(FIXTURE, fixture_layout, &[])
        .unwrap()
        .prepare_module(
            limits.max_payload_bytes,
            limits.max_relocation_entries,
            limits.max_sections,
        )
        .unwrap();
    let span = usize::try_from(fixture_layout.span()).unwrap();
    let chunks = fixture_layout.placements().len();
    assert!(chunks > 0);
    let mut expected = vec![0; span];
    for section in expected_prepared.sections() {
        let start = usize::try_from(section.placement().address - base as u64).unwrap();
        expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
    }
    let staged = policy_owner.prepare_and_stage(FIXTURE, limits).unwrap();
    assert_eq!((staged.span(), staged.page_count()), (40_960, 10));
    let mut actual = vec![0xff; span];
    staged.read_bytes(0, &mut actual).unwrap();
    assert_eq!(actual, expected);
    assert!(matches!(
        NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
        Err(ostd::Error::NoMemory)
    ));
    drop(staged);
    let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
    drop(raw);
    policy_reclaimed += 1;
    let mut budget_rejected = 0usize;
    let mut early_precedence = 0usize;
    let mut budget_reclaimed = 0usize;
    let budget_limits = [
        PreparationLimits {
            max_payload_bytes: 1_720,
            ..limits
        },
        PreparationLimits {
            max_relocation_entries: 5,
            ..limits
        },
        PreparationLimits {
            max_sections: 0,
            ..limits
        },
        PreparationLimits {
            max_pages: 9,
            ..limits
        },
        PreparationLimits {
            max_chunks: chunks - 1,
            ..limits
        },
    ];
    for restricted in budget_limits {
        let owner = PolicyNearTextModuleLayout::new_signed32_at(
            FIXTURE,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ));
        assert!(
            matches!(
                owner.prepare_and_stage(FIXTURE, restricted),
                Err(Error::Plan(ElfError::ResourceLimit))
            ),
            "module policy budget must reject"
        );
        budget_rejected += 1;
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        budget_reclaimed += 1;
    }
    let (denied_bytes, _, _) = renamed_import_fixture(2);
    for (restricted, expected_message) in [
        (
            PreparationLimits {
                max_pages: 9,
                ..limits
            },
            "policy staging page budget must precede payload resolution",
        ),
        (
            PreparationLimits {
                max_chunks: chunks - 1,
                ..limits
            },
            "policy staging chunk budget must precede payload resolution",
        ),
    ] {
        // The generated PC32 fixture names the known but ABI-denied OSTD export.
        let owner = PolicyNearTextModuleLayout::new_signed32_at(
            FIXTURE,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        assert!(
            matches!(
                owner.prepare_and_stage(&denied_bytes, restricted),
                Err(Error::Plan(ElfError::ResourceLimit))
            ),
            "{}",
            expected_message
        );
        early_precedence += 1;
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        budget_reclaimed += 1;
    }
    let exact_limits = PreparationLimits {
        max_pages: 10,
        max_chunks: chunks,
        max_payload_bytes: 1_721,
        max_relocation_entries: 6,
        max_sections: MAX_SECTIONS,
    };
    let exact_owner =
        PolicyNearTextModuleLayout::new_signed32_at(FIXTURE, base, max_pages, MAX_SECTIONS, 65_536)
            .unwrap();
    let exact_layout = exact_owner.layout();
    let expected_exact = SymbolResolver::new(FIXTURE, exact_layout, &[])
        .unwrap()
        .prepare_module(1_721, 6, MAX_SECTIONS)
        .unwrap();
    let exact_span = usize::try_from(exact_layout.span()).unwrap();
    let mut expected_exact_bytes = vec![0; exact_span];
    for section in expected_exact.sections() {
        let start = usize::try_from(section.placement().address - base as u64).unwrap();
        expected_exact_bytes[start..start + section.bytes().len()].copy_from_slice(section.bytes());
    }
    let exact_staged = exact_owner
        .prepare_and_stage(FIXTURE, exact_limits)
        .unwrap();
    assert_eq!(
        (exact_staged.page_count(), exact_staged.span()),
        (10, 40_960)
    );
    let mut actual_exact_bytes = vec![0xff; exact_span];
    exact_staged.read_bytes(0, &mut actual_exact_bytes).unwrap();
    assert_eq!(actual_exact_bytes, expected_exact_bytes);
    let exact_compared = actual_exact_bytes.len();
    assert_eq!(exact_compared, 40_960);
    assert!(matches!(
        NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
        Err(ostd::Error::NoMemory)
    ));
    drop(exact_staged);
    let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
    drop(raw);
    budget_reclaimed += 1;
    assert_eq!(
        (budget_rejected, early_precedence, budget_reclaimed),
        (5, 2, 8)
    );
    assert_eq!(
        (ctor_abi, ctor_unknown, prepare_abi, policy_reclaimed),
        (2, 2, 2, 7)
    );
    assert_eq!(occupied_precedence, 2);
    assert_eq!((compared, missing, reclaimed), (2 * 40_960, 2, 4));
    assert_eq!(
        (checked_abi, checked_unknown, preflight_rejections),
        (2, 2, 2)
    );
    assert_eq!(local_only_prepared, 1);
    assert_eq!(denied.len(), 5);
    println!(
        "CYBERMANJU_INTERNAL_EXPORTS:resolver:PASS checked_abi_rejections=2 checked_unknown_rejections=2 preflight_rejections=2 local_only_prepared={} numeric_fallback=false",
        local_only_prepared
    );
    println!(
        "CYBERMANJU_INTERNAL_EXPORTS:owner:PASS ctor_abi_rejections=2 ctor_unknown_rejections=2 prepare_abi_rejections=2 occupied_precedence=2 reclaimed_ranges=7 local_only_staged=1 compared=40960 typed_policy=true pages_mapped=false"
    );
    println!(
        "CYBERMANJU_INTERNAL_EXPORTS:budget:PASS rejected={} early_precedence={} reclaimed={} exact_limit_staged=1 pages=10 chunks={} compared={} pages_mapped=false",
        budget_rejected, early_precedence, budget_reclaimed, chunks, exact_compared
    );
    // Injected backend-boundary failures use real frames, but are neither real OOM nor mapped pages.
    // Counters are fresh per attempt; aggregate snapshots below audit exact cleanup.
    let mut alloc_cases = 0;
    let mut copy_cases = 0;
    let mut total_acquired = 0;
    let mut total_dropped = 0;
    let mut successful_writes = 0;
    let mut triggered_cases = 0;
    let mut reclaimed = 0;
    for n in [0, 1, 9] {
        let stats = Arc::new(FaultStats::new());
        let owner = PolicyNearTextModuleLayout::new_signed32_at(
            FIXTURE,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ));
        let result = owner.prepare_and_stage_with_fault(
            FIXTURE,
            limits,
            FaultMode::AllocateAfter(n),
            &stats,
        );
        assert!(
            matches!(result, Err(Error::Frames(ostd::Error::NoMemory))),
            "fault allocation must not return a partial owner"
        );
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.entries, 1, "fault backend must be entered once");
        assert!(snapshot.triggered);
        assert_eq!(snapshot.acquired, n);
        assert_eq!(
            snapshot.dropped, n,
            "fault allocation must release acquired frames"
        );
        assert_eq!(snapshot.writes, 0);
        alloc_cases += 1;
        total_acquired += snapshot.acquired;
        total_dropped += snapshot.dropped;
        triggered_cases += usize::from(snapshot.triggered);
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        reclaimed += 1;
    }
    for n in [0, 1] {
        let stats = Arc::new(FaultStats::new());
        let owner = PolicyNearTextModuleLayout::new_signed32_at(
            FIXTURE,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ));
        let result =
            owner.prepare_and_stage_with_fault(FIXTURE, limits, FaultMode::CopyAfter(n), &stats);
        assert!(
            matches!(result, Err(Error::Frames(ostd::Error::IoError))),
            "fault copy must not return a partial owner"
        );
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.entries, 1, "fault backend must be entered once");
        assert!(snapshot.triggered);
        assert_eq!(snapshot.acquired, 10);
        assert_eq!(
            snapshot.dropped, 10,
            "fault copy must release acquired frames"
        );
        assert_eq!(snapshot.writes, n);
        copy_cases += 1;
        total_acquired += snapshot.acquired;
        total_dropped += snapshot.dropped;
        successful_writes += snapshot.writes;
        triggered_cases += usize::from(snapshot.triggered);
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        reclaimed += 1;
    }
    assert_eq!(
        (
            alloc_cases,
            copy_cases,
            total_acquired,
            total_dropped,
            successful_writes,
            reclaimed
        ),
        (3, 2, 30, 30, 1, 5)
    );
    assert_eq!(triggered_cases, 5);
    println!(
        "CYBERMANJU_INTERNAL_EXPORTS:fault:PASS alloc_cases=3 copy_cases=2 acquired=30 dropped=30 writes=1 reclaimed=5 real_frames=true production_hook=false actual_oom=false pages_mapped=false"
    );
    let preflight_budgets = [
        PreparationLimits {
            max_payload_bytes: 1_720,
            ..limits
        },
        PreparationLimits {
            max_relocation_entries: 5,
            ..limits
        },
        PreparationLimits {
            max_sections: 0,
            ..limits
        },
        PreparationLimits {
            max_pages: 9,
            ..limits
        },
        PreparationLimits {
            max_chunks: chunks - 1,
            ..limits
        },
    ];
    let mut preflight_reclaimed = 0;
    for restricted in preflight_budgets {
        let stats = Arc::new(FaultStats::new());
        let owner = PolicyNearTextModuleLayout::new_signed32_at(
            FIXTURE,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ));
        assert!(
            matches!(
                owner.prepare_and_stage_with_fault(
                    FIXTURE,
                    restricted,
                    FaultMode::AllocateAfter(0),
                    &stats,
                ),
                Err(Error::Plan(ElfError::ResourceLimit))
            ),
            "policy rejection must precede fault backend"
        );
        let snapshot = stats.snapshot();
        assert_eq!(
            (
                snapshot.entries,
                snapshot.acquired,
                snapshot.dropped,
                snapshot.writes,
                snapshot.triggered
            ),
            (0, 0, 0, 0, false),
            "staging backend must remain untouched"
        );
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        preflight_reclaimed += 1;
    }
    let denied_bytes = renamed_import_fixture(2).0;
    let mut missing_bytes = renamed_import_fixture(4).0;
    let name_offset = missing_bytes.len() - OSTD_NUM_CPUS_EXPORT_NAME.len() - 1;
    assert_eq!(
        &missing_bytes[name_offset..missing_bytes.len() - 1],
        OSTD_NUM_CPUS_EXPORT_NAME
    );
    missing_bytes[name_offset] = b'x';
    for (bytes, expected) in [
        (&denied_bytes, ElfError::ExportNotAdmitted),
        (&missing_bytes, ElfError::MissingSymbol),
    ] {
        let stats = Arc::new(FaultStats::new());
        let owner = PolicyNearTextModuleLayout::new_signed32_at(
            FIXTURE,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
            Err(ostd::Error::NoMemory)
        ));
        assert!(
            matches!(
                owner.prepare_and_stage_with_fault(
                    bytes, limits, FaultMode::AllocateAfter(0), &stats,
                ),
                Err(Error::Plan(cause)) if cause == expected
            ),
            "policy rejection must precede fault backend"
        );
        let snapshot = stats.snapshot();
        assert_eq!(
            (
                snapshot.entries,
                snapshot.acquired,
                snapshot.dropped,
                snapshot.writes,
                snapshot.triggered
            ),
            (0, 0, 0, 0, false),
            "staging backend must remain untouched"
        );
        let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        drop(raw);
        preflight_reclaimed += 1;
    }
    assert_eq!(preflight_reclaimed, 7);
    let recovery_owner =
        PolicyNearTextModuleLayout::new_signed32_at(FIXTURE, base, max_pages, MAX_SECTIONS, 65_536)
            .unwrap();
    assert_eq!(recovery_owner.range(), base..base + envelope);
    let recovery_staged = recovery_owner
        .prepare_and_stage(FIXTURE, limits)
        .expect("post-fault production staging must recover");
    assert_eq!(
        (recovery_staged.page_count(), recovery_staged.span()),
        (10, 40_960)
    );
    let mut recovery_bytes = vec![0xff; 40_960];
    recovery_staged.read_bytes(0, &mut recovery_bytes).unwrap();
    assert_eq!(
        recovery_bytes, expected_exact_bytes,
        "post-fault production staging must match independent bytes"
    );
    assert!(matches!(
        NearTextAddressReservation::new_signed32_at(base, envelope, max_pages),
        Err(ostd::Error::NoMemory)
    ));
    drop(recovery_staged);
    let raw = NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
    drop(raw);
    println!(
        "CYBERMANJU_INTERNAL_EXPORTS:recovery:PASS budget_rejections=5 import_rejections=2 untouched_backends=7 preflight_reclaimed=7 recovered_staged=1 compared=40960 recovered_reclaimed=1 production_path=true entry_observed=true pages_mapped=false"
    );
    println!(
        "CYBERMANJU_INTERNAL_EXPORTS:complete:PASS staged=2 denied_names=5 missing_imports=2 available_checks=4 compared_full_span=true pages_mapped=false linux_abi=false execution=false abi_rejection=true policy_resolver=true policy_owner=true budget_checks=true fault_cleanup=true recovery=true"
    );
}
