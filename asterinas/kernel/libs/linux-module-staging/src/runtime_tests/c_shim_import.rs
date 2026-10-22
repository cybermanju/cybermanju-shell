// SPDX-License-Identifier: MPL-2.0
//! Explicit C-shim import policy checks; data-only ELF preparation, never execution.
use super::{FIXTURE, MAX_SECTIONS, MAX_SPAN, relative_import_fixture};
use crate::{
    CShimExportRegistry, Error, OSTD_NUM_CPUS_EXPORT_NAME, PolicyNearTextModuleLayout,
    PreparationLimits,
};
use alloc::{sync::Arc, vec, vec::Vec};
use linux_module_elf::{Error as ElfError, page_layout_span_bound};
use ostd::{
    mm::{
        PAGE_SIZE,
        module_address::NearTextAddressReservation,
        module_staging::fault_injection::{FaultMode, FaultStats},
    },
    prelude::*,
};

fn rename_import(mut bytes: Vec<u8>, target: u16, offset: u64, name: &[u8]) -> Vec<u8> {
    let r32 = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let r64 = |b: &[u8], o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let table = r64(&bytes, 40) as usize;
    let count = u16::from_le_bytes(bytes[60..62].try_into().unwrap()) as usize;
    for i in 0..count {
        let rela = table + i * 64;
        if r32(&bytes, rela + 4) != 4 || r32(&bytes, rela + 44) != u32::from(target) {
            continue;
        }
        let entries = r64(&bytes, rela + 24) as usize;
        let size = r64(&bytes, rela + 32) as usize;
        for entry in (entries..entries + size).step_by(24) {
            let info = r64(&bytes, entry + 8);
            if r64(&bytes, entry) != offset || info as u32 == 0 {
                continue;
            }
            assert_eq!(
                r64(&bytes, entry + 16),
                0,
                "fixture import addend must be zero"
            );
            let symidx = (info >> 32) as usize;
            let symtab = table + r32(&bytes, rela + 40) as usize * 64;
            let symoff = r64(&bytes, symtab + 24) as usize + symidx * 24;
            assert_eq!(
                bytes[symoff + 4] >> 4,
                1,
                "referenced symbol must be global"
            );
            assert_eq!(
                u16::from_le_bytes(bytes[symoff + 6..symoff + 8].try_into().unwrap()),
                0,
                "referenced symbol must be undefined"
            );
            let strtab = table + r32(&bytes, symtab + 40) as usize * 64;
            let strings = r64(&bytes, strtab + 24) as usize;
            let n = r64(&bytes, strtab + 32) as usize;
            let mut new_strings = bytes[strings..strings + n].to_vec();
            let name_offset = new_strings.len();
            new_strings.extend_from_slice(name);
            new_strings.push(0);
            let new_start = bytes.len();
            bytes.extend_from_slice(&new_strings);
            bytes[strtab + 24..strtab + 32].copy_from_slice(&(new_start as u64).to_le_bytes());
            bytes[strtab + 32..strtab + 40]
                .copy_from_slice(&(new_strings.len() as u64).to_le_bytes());
            bytes[symoff..symoff + 4].copy_from_slice(&(name_offset as u32).to_le_bytes());
            return bytes;
        }
    }
    panic!("fixture import RELA not found")
}

fn imported(kind: u32, name: &[u8]) -> (Vec<u8>, u16, u64) {
    let (bytes, _, target, offset) = relative_import_fixture(kind);
    (rename_import(bytes, target, offset, name), target, offset)
}

fn limits(pages: usize, chunks: usize) -> PreparationLimits {
    PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages: pages,
        max_chunks: chunks,
    }
}

fn reclaim(base: usize, envelope: usize, pages: usize) {
    let owner = NearTextAddressReservation::new_signed32_at(base, envelope, pages).unwrap();
    assert_eq!(owner.range(), base..base + envelope);
    drop(owner);
}

#[ktest]
fn cybermanju_c_shim_import_runtime() {
    println!("");
    let cpus = ostd::cpu::num_cpus();
    assert_eq!(cpus, 1);
    let kinds = [1u32, 2, 4, 11];
    println!(
        "CYBERMANJU_C_SHIM_IMPORT:begin:PASS cpus={} kinds={}",
        cpus,
        kinds.len()
    );
    let pages = MAX_SPAN as usize / PAGE_SIZE;
    let base = NearTextAddressReservation::signed32_arena_range().end - 64 * PAGE_SIZE;
    let sample = imported(1, b"memcmp").0;
    let envelope = page_layout_span_bound(&sample, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    assert_eq!(envelope % PAGE_SIZE, 0);
    let max_pages = envelope / PAGE_SIZE;

    // Every rejection consumes and releases its exact reservation.
    let mut reclaimed = 0usize;
    let default_bytes = imported(1, b"memcmp").0;
    assert!(
        matches!(
            PolicyNearTextModuleLayout::new_signed32_at(
                &default_bytes,
                base,
                max_pages,
                MAX_SECTIONS,
                65_536
            ),
            Err(Error::Plan(ElfError::MissingSymbol))
        ),
        "C shim default policy must remain closed"
    );
    reclaim(base, envelope, pages);
    reclaimed += 1;
    let mut unknown_denied = 0;
    for n in [b"MEMCMP".as_slice(), b"memcpy", b"cybermanju_unknown"] {
        let bytes = imported(1, n).0;
        assert!(
            matches!(
                PolicyNearTextModuleLayout::new_signed32_c_shims_at(
                    &bytes,
                    base,
                    max_pages,
                    MAX_SECTIONS,
                    65_536
                ),
                Err(Error::Plan(ElfError::MissingSymbol))
            ),
            "C shim unregistered import must be denied"
        );
        reclaim(base, envelope, pages);
        reclaimed += 1;
        unknown_denied += 1;
    }
    let rust_bytes = imported(1, OSTD_NUM_CPUS_EXPORT_NAME).0;
    assert!(
        matches!(
            PolicyNearTextModuleLayout::new_signed32_c_shims_at(
                &rust_bytes,
                base,
                max_pages,
                MAX_SECTIONS,
                65_536
            ),
            Err(Error::Plan(ElfError::MissingSymbol))
        ),
        "C shim C-only policy must deny Rust exports"
    );
    reclaim(base, envelope, pages);
    reclaimed += 1;

    let current = imported(1, b"memcmp").0;
    let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
        &current,
        base,
        max_pages,
        MAX_SECTIONS,
        65_536,
    )
    .unwrap();
    let changed = imported(1, b"memcpy").0;
    let audit = Arc::new(FaultStats::new());
    assert!(
        matches!(
            owner.prepare_and_stage_with_fault(
                &changed,
                limits(max_pages, MAX_SECTIONS),
                FaultMode::AllocateAfter(0),
                &audit
            ),
            Err(Error::Plan(ElfError::MissingSymbol))
        ),
        "C shim current bytes must be revalidated"
    );
    assert_eq!(
        audit.snapshot().entries,
        0,
        "C shim backend must not run before rejection"
    );
    reclaim(base, envelope, pages);
    reclaimed += 1;

    let mut preflight_entries = audit.snapshot().entries;
    let mut budget_denied = 0;
    for chunks in [false, true] {
        let bytes = imported(1, b"memcmp").0;
        let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let pages_actual = owner.layout().span() as usize / PAGE_SIZE;
        let chunks_actual = owner.layout().placements().len();
        let stats = Arc::new(FaultStats::new());
        let l = if chunks {
            limits(max_pages, chunks_actual - 1)
        } else {
            limits(pages_actual - 1, MAX_SECTIONS)
        };
        assert!(matches!(
            owner.prepare_and_stage_with_fault(&bytes, l, FaultMode::AllocateAfter(0), &stats),
            Err(Error::Plan(ElfError::ResourceLimit))
        ));
        assert_eq!(
            stats.snapshot().entries,
            0,
            "C shim backend must not run before rejection"
        );
        reclaim(base, envelope, pages);
        reclaimed += 1;
        preflight_entries += stats.snapshot().entries;
        budget_denied += 1;
    }

    let mut fault_entries = 0usize;
    let mut acquired = 0usize;
    let mut dropped = 0usize;
    let mut writes = 0usize;
    let mut fault_reclaimed = 0usize;
    for (mode, allocation) in [
        (FaultMode::AllocateAfter(2), true),
        (FaultMode::CopyAfter(1), false),
    ] {
        let bytes = imported(1, b"memcmp").0;
        let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let p = owner.layout().span() as usize / PAGE_SIZE;
        let stats = Arc::new(FaultStats::new());
        let result = owner.prepare_and_stage_with_fault(
            &bytes,
            limits(max_pages, MAX_SECTIONS),
            mode,
            &stats,
        );
        assert!(if allocation {
            matches!(result, Err(Error::Frames(ostd::Error::NoMemory)))
        } else {
            matches!(result, Err(Error::Frames(ostd::Error::IoError)))
        });
        let s = stats.snapshot();
        assert_eq!(s.entries, 1);
        assert!(s.triggered);
        assert_eq!(s.acquired, if allocation { 2 } else { p });
        assert_eq!(s.dropped, s.acquired, "C shim fault handles must all drop");
        assert_eq!(s.writes, if allocation { 0 } else { 1 });
        fault_entries += s.entries;
        acquired += s.acquired;
        dropped += s.dropped;
        writes += s.writes;
        reclaim(base, envelope, pages);
        fault_reclaimed += 1;
    }

    for kind in kinds {
        let (bytes, target, offset) = imported(kind, b"memcmp");
        let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let span = owner.layout().span() as usize;
        let section_count = owner.layout().placements().len();
        let page_count = span / PAGE_SIZE;
        let entry: ostd::linux_abi::MemcmpFn = ostd::linux_abi::memcmp_entry();
        let address = entry as usize as u64;
        let placement = owner
            .layout()
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let place = placement.address + offset;
        let field = usize::try_from(place - owner.layout().base()).unwrap();
        let oracle = match kind {
            1 => address.to_le_bytes().to_vec(),
            11 => {
                let low = address as u32;
                assert_eq!(
                    (low as i32 as i64) as u64,
                    address,
                    "C shim independent relocation oracle"
                );
                low.to_le_bytes().to_vec()
            }
            2 | 4 => {
                let d = i128::from(address) - i128::from(place);
                let v = i32::try_from(d).expect("C shim independent relocation oracle");
                v.to_le_bytes().to_vec()
            }
            _ => unreachable!(),
        };
        let mut expected = vec![0; span];
        // Full-span reference uses the resolver; the selected field is separately
        // checked against the typed entry address and independent arithmetic.
        let registry = CShimExportRegistry::new();
        let resolver = registry
            .linux_symbol_resolver(&bytes, owner.layout())
            .unwrap();
        let prepared = resolver
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
            .unwrap();
        for section in prepared.sections() {
            let start =
                usize::try_from(section.placement().address - owner.layout().base()).unwrap();
            expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
        }
        assert_eq!(
            &expected[field..field + oracle.len()],
            oracle.as_slice(),
            "C shim independent relocation oracle"
        );
        let staged = owner
            .prepare_and_stage(&bytes, limits(max_pages, section_count))
            .unwrap();
        assert_eq!((staged.span(), staged.page_count()), (span, page_count));
        let mut actual = vec![0xff; span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(
            actual, expected,
            "C shim frames must match all prepared bytes"
        );
        assert_eq!(
            &actual[field..field + oracle.len()],
            oracle.as_slice(),
            "C shim independent relocation oracle"
        );
        let retained = matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, pages),
            Err(ostd::Error::NoMemory)
        );
        assert!(retained);
        let range = staged.reserved_range();
        drop(staged);
        let reservation =
            NearTextAddressReservation::new_signed32_at(base, envelope, pages).unwrap();
        let case_reclaimed = reservation.range() == range;
        assert!(case_reclaimed);
        drop(reservation);
        println!(
            "CYBERMANJU_C_SHIM_IMPORT:case:PASS kind={} span={} bytes={} relocation_bytes={} retained={} reclaimed={}",
            kind,
            span,
            actual.len(),
            oracle.len(),
            retained,
            case_reclaimed
        );
    }
    assert_eq!((unknown_denied, budget_denied, reclaimed), (3, 2, 8));
    assert_eq!(fault_entries, 2);
    assert_eq!(acquired, max_pages + 2);
    assert_eq!(dropped, acquired);
    assert_eq!(writes, 1);
    assert_eq!(fault_reclaimed, 2);
    println!(
        "CYBERMANJU_C_SHIM_IMPORT:policy:PASS default_denied=1 unknown_denied={} rust_denied=1 current_bytes_denied=1 budgets_denied={} backend_entries={} reclaimed={}",
        unknown_denied, budget_denied, preflight_entries, reclaimed
    );
    println!(
        "CYBERMANJU_C_SHIM_IMPORT:faults:PASS cases={} entries={} acquired={} dropped={} writes={} reclaimed={}",
        fault_reclaimed, fault_entries, acquired, dropped, writes, fault_reclaimed
    );
    println!(
        "CYBERMANJU_C_SHIM_IMPORT:complete:PASS elf_imports_prepared=true init_called=false modules_executed=0 production_mapping=false wx=false smp_mapping=false"
    );
}
