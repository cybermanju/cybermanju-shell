// SPDX-License-Identifier: MPL-2.0
//! ELF selective RO/RW non-executable leases. Data only; no module is executed.
use super::{MAX_SECTIONS, MAX_SPAN, PreparationLimits, c_shim_import::imported};
use crate::{CShimExportRegistry, PolicyNearTextModuleLayout};
use alloc::{sync::Arc, vec, vec::Vec};
use linux_module_elf::{IntendedPagePermission as P, page_layout_span_bound, plan_page_layout};
use ostd::{
    cpu, irq,
    mm::{
        PAGE_SIZE, module_address::NearTextAddressReservation,
        module_staging::fault_injection::FaultStats,
    },
    prelude::*,
};

fn limits(pages: usize) -> PreparationLimits {
    PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages: pages,
        max_chunks: MAX_SECTIONS,
    }
}
fn owner(bytes: &[u8], base: usize, pages: usize) -> PolicyNearTextModuleLayout {
    PolicyNearTextModuleLayout::new_signed32_c_shims_at(bytes, base, pages, MAX_SECTIONS, 65_536)
        .unwrap()
}
// Classify from flags and placements, not the production page plan/projector.
fn independent(layout: &linux_module_elf::Layout, pages: usize) -> Vec<P> {
    let mut out = vec![P::Inaccessible; pages];
    for section in layout.placements() {
        if section.size == 0 {
            continue;
        }
        let first = (section.address - layout.base()) as usize / PAGE_SIZE;
        let last =
            ((section.address - layout.base()) as usize + section.size as usize - 1) / PAGE_SIZE;
        let class = if section.flags & 1 != 0 {
            P::ReadWrite
        } else if section.flags & 4 != 0 {
            P::ReadExecute
        } else {
            P::ReadOnly
        };
        for slot in &mut out[first..=last] {
            assert_eq!(*slot, P::Inaccessible);
            *slot = class;
        }
    }
    out
}
fn reclaim(base: usize, envelope: usize, pages: usize) {
    let reservation = NearTextAddressReservation::new_signed32_at(base, envelope, pages).unwrap();
    assert_eq!(reservation.range(), base..base + envelope);
    drop(reservation);
}
// Actual ELF64 ET_REL: two one-byte allocated NOBITS sections, second aligned
// to 8192. The page-separated layout has RO / inaccessible / RW, not padding
// invented by a synthetic permission-plan hook. It has no imports/relocations.
fn gap_elf() -> Vec<u8> {
    let names = b"\0.shstrtab\0.ro\0.rw\0";
    let table = 64usize;
    let names_at = table + 4 * 64;
    let mut bytes = vec![0; names_at + names.len()];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[6] = 1;
    bytes[16..18].copy_from_slice(&1u16.to_le_bytes());
    bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
    bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
    bytes[40..48].copy_from_slice(&(table as u64).to_le_bytes());
    bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
    bytes[58..60].copy_from_slice(&64u16.to_le_bytes());
    bytes[60..62].copy_from_slice(&4u16.to_le_bytes());
    bytes[62..64].copy_from_slice(&1u16.to_le_bytes());
    let strtab = table + 64;
    bytes[strtab..strtab + 4].copy_from_slice(&1u32.to_le_bytes());
    bytes[strtab + 4..strtab + 8].copy_from_slice(&3u32.to_le_bytes());
    bytes[strtab + 24..strtab + 32].copy_from_slice(&(names_at as u64).to_le_bytes());
    bytes[strtab + 32..strtab + 40].copy_from_slice(&(names.len() as u64).to_le_bytes());
    bytes[strtab + 48..strtab + 56].copy_from_slice(&1u64.to_le_bytes());
    for (idx, name, flags, alignment) in [
        (2usize, 11u32, 2u64, PAGE_SIZE as u64),
        (3, 15, 3, (2 * PAGE_SIZE) as u64),
    ] {
        let s = table + idx * 64;
        bytes[s..s + 4].copy_from_slice(&name.to_le_bytes());
        bytes[s + 4..s + 8].copy_from_slice(&8u32.to_le_bytes());
        bytes[s + 8..s + 16].copy_from_slice(&flags.to_le_bytes());
        bytes[s + 32..s + 40].copy_from_slice(&1u64.to_le_bytes());
        bytes[s + 48..s + 56].copy_from_slice(&alignment.to_le_bytes());
    }
    bytes[names_at..].copy_from_slice(names);
    bytes
}

#[ktest]
fn cybermanju_elf_selective_nx_runtime() {
    assert_eq!(cpu::num_cpus(), 1);
    println!("");
    println!("CYBERMANJU_ELF_SELECTIVE_NX:begin:PASS cpus=1");
    let base = (NearTextAddressReservation::signed32_arena_range().end - 64 * PAGE_SIZE)
        & !(2 * PAGE_SIZE - 1);
    let sample = imported(1, b"memcmp").0;
    let envelope = page_layout_span_bound(&sample, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let pages = envelope / PAGE_SIZE;
    let data_pages = plan_page_layout(&sample, base as u64, MAX_SPAN, MAX_SECTIONS)
        .unwrap()
        .span() as usize
        / PAGE_SIZE;
    assert_eq!(data_pages, 10);
    let before_irq = irq::is_local_enabled();
    let mut fault_acquired = 0;
    let mut fault_dropped = 0;
    let mut fault_reclaims = 0;
    let mut fault_prefix = 0;
    for after in [0, 1, 6] {
        let bytes = imported(1, b"memcmp").0;
        let stats = Arc::new(FaultStats::new());
        let staged = owner(&bytes, base, pages)
            .prepare_and_stage_observed_for_test(&bytes, limits(pages), &stats)
            .unwrap();
        let range = staged.reserved_range();
        let mut observed = 0;
        assert!(
            matches!(
                staged.into_signed32_selective_nx_with_fault_for_test(after, &mut observed),
                Err(ostd::Error::IoError)
            ),
            "ELF selective seal fault must fail"
        );
        assert_eq!(observed, after, "ELF selected prefix must be observed");
        assert_eq!(irq::is_local_enabled(), before_irq);
        let s = stats.snapshot();
        assert_eq!(s.acquired, data_pages);
        assert_eq!(
            s.dropped, s.acquired,
            "ELF selective fault handles must drop"
        );
        fault_acquired += s.acquired;
        fault_dropped += s.dropped;
        fault_prefix += observed;
        reclaim(range.start, envelope, pages);
        fault_reclaims += 1;
    }
    println!(
        "CYBERMANJU_ELF_SELECTIVE_NX:faults:PASS cases={} prefix={} acquired={} dropped={} reclaims={} irq_preserved=true",
        fault_reclaims, fault_prefix, fault_acquired, fault_dropped, fault_reclaims
    );
    let mut budget_acquired = 0;
    let mut budget_dropped = 0;
    let mut budget_reclaims = 0;
    let mut budget_public = 0;
    let mut budget_observed = 0;
    let mut budget_entries = 0;
    for budget in [0, data_pages - 1] {
        for observed_mode in [false, true] {
            let bytes = imported(1, b"memcmp").0;
            let stats = Arc::new(FaultStats::new());
            let staged = owner(&bytes, base, pages)
                .prepare_and_stage_observed_for_test(&bytes, limits(pages), &stats)
                .unwrap();
            let range = staged.reserved_range();
            let mut entries = 0;
            let result = if observed_mode {
                budget_observed += 1;
                staged
                    .into_signed32_selective_nx_with_budget_observed_for_test(budget, &mut entries)
            } else {
                budget_public += 1;
                staged.into_signed32_selective_nx_with_page_budget(budget)
            };
            assert!(
                matches!(result, Err(ostd::Error::NoMemory)),
                "ELF selective budget must reject"
            );
            assert_eq!(entries, 0, "budget denial must precede mapper");
            budget_entries += entries;
            assert_eq!(irq::is_local_enabled(), before_irq);
            let s = stats.snapshot();
            assert_eq!(s.acquired, data_pages);
            assert_eq!(s.acquired, s.dropped);
            budget_acquired += s.acquired;
            budget_dropped += s.dropped;
            reclaim(range.start, envelope, pages);
            budget_reclaims += 1;
        }
    }
    println!(
        "CYBERMANJU_ELF_SELECTIVE_NX:budgets:PASS public={} observed={} mapper_entries={} acquired={} dropped={} reclaims={} irq_preserved=true",
        budget_public,
        budget_observed,
        budget_entries,
        budget_acquired,
        budget_dropped,
        budget_reclaims
    );
    let gap = gap_elf();
    let gap_envelope = page_layout_span_bound(&gap, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let gap_budget = gap_envelope / PAGE_SIZE;
    let mut gap_acquired = 0;
    let mut gap_dropped = 0;
    let mut gap_reclaims = 0;
    let mut gap_entries = 0;
    for observed_mode in [false, true] {
        let layout_owner = PolicyNearTextModuleLayout::new_signed32_at(
            &gap,
            base,
            gap_budget,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let oracle = independent(layout_owner.layout(), 3);
        assert_eq!(
            oracle,
            [P::ReadOnly, P::Inaccessible, P::ReadWrite],
            "gap fixture must really contain an inaccessible page"
        );
        let stats = Arc::new(FaultStats::new());
        let staged = layout_owner
            .prepare_and_stage_observed_for_test(&gap, limits(gap_budget), &stats)
            .unwrap();
        assert_eq!(staged.page_count(), 3);
        assert_eq!(staged.planned_permissions(3).unwrap().pages(), oracle);
        let range = staged.reserved_range();
        let mut entries = 0;
        let result = if observed_mode {
            staged.into_signed32_selective_nx_with_budget_observed_for_test(3, &mut entries)
        } else {
            staged.into_signed32_selective_nx()
        };
        assert!(
            matches!(result, Err(ostd::Error::AccessDenied)),
            "ELF inaccessible gap must reject before mapping"
        );
        assert_eq!(entries, 0, "gap denial must precede mapper");
        gap_entries += entries;
        assert_eq!(irq::is_local_enabled(), before_irq);
        let s = stats.snapshot();
        assert_eq!(s.acquired, 3);
        assert_eq!(s.dropped, s.acquired);
        gap_acquired += s.acquired;
        gap_dropped += s.dropped;
        reclaim(range.start, gap_envelope, gap_budget);
        gap_reclaims += 1;
    }
    println!(
        "CYBERMANJU_ELF_SELECTIVE_NX:gaps:PASS public=1 observed=1 inaccessible_pages=1 mapper_entries={} acquired={} dropped={} reclaims={} irq_preserved=true",
        gap_entries, gap_acquired, gap_dropped, gap_reclaims
    );
    let mut acquired = 0;
    let mut dropped = 0;
    let mut reclaims = 0;
    let mut total_bytes = 0;
    let mut bounds = 0;
    for kind in [1u32, 2, 4, 11] {
        let (bytes, target, offset) = imported(kind, b"memcmp");
        let layout_owner = owner(&bytes, base, pages);
        let layout = layout_owner.layout();
        let base_u64 = layout.base();
        let span = layout.span() as usize;
        let count = span / PAGE_SIZE;
        let placements: Vec<_> = layout
            .placements()
            .iter()
            .map(|p| (p.section_index, p.address, p.size))
            .collect();
        let page_oracle = independent(layout, count);
        assert_eq!(page_oracle.iter().filter(|p| **p == P::ReadOnly).count(), 4);
        assert_eq!(
            page_oracle.iter().filter(|p| **p == P::ReadWrite).count(),
            4
        );
        assert_eq!(
            page_oracle.iter().filter(|p| **p == P::ReadExecute).count(),
            2
        );
        assert!(!page_oracle.contains(&P::Inaccessible));
        let placement = layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let field = usize::try_from(placement.address + offset - base_u64).unwrap();
        let address = ostd::linux_abi::memcmp_entry() as usize as u64;
        let relocation_oracle = match kind {
            1 => address.to_le_bytes().to_vec(),
            11 => {
                let low = address as u32;
                assert_eq!((low as i32 as i64) as u64, address);
                low.to_le_bytes().to_vec()
            }
            2 | 4 => i32::try_from(i128::from(address) - i128::from(placement.address + offset))
                .unwrap()
                .to_le_bytes()
                .to_vec(),
            _ => unreachable!(),
        };
        let numeric = plan_page_layout(&bytes, base_u64, MAX_SPAN, MAX_SECTIONS).unwrap();
        let registry = CShimExportRegistry::new();
        let resolver = registry.linux_symbol_resolver(&bytes, &numeric).unwrap();
        let prepared = resolver
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
            .unwrap();
        let mut expected = vec![0; span];
        for section in prepared.sections() {
            let start = (section.placement().address - base_u64) as usize;
            expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
        }
        assert_eq!(
            &expected[field..field + relocation_oracle.len()],
            relocation_oracle
        );
        let stats = Arc::new(FaultStats::new());
        let staged = layout_owner
            .prepare_and_stage_observed_for_test(&bytes, limits(pages), &stats)
            .unwrap();
        let mut actual = vec![0xff; span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(actual, expected);
        let intended = staged.planned_permissions(pages).unwrap();
        assert_eq!(page_oracle, intended.pages());
        let range = staged.reserved_range();
        let mapped = if kind == 1 {
            staged
                .into_signed32_rw_nx()
                .unwrap()
                .into_selective_nx()
                .unwrap()
        } else {
            staged.into_signed32_selective_nx().unwrap()
        };
        assert_eq!(irq::is_local_enabled(), before_irq);
        assert_eq!(mapped.layout().base(), base_u64);
        assert_eq!(mapped.layout().span() as usize, span);
        assert_eq!(
            mapped
                .layout()
                .placements()
                .iter()
                .map(|p| (p.section_index, p.address, p.size))
                .collect::<Vec<_>>(),
            placements
        );
        assert_eq!(mapped.reserved_range(), range);
        assert_eq!(mapped.permission_plan(), &intended);
        let expected_mask: Vec<bool> = page_oracle
            .iter()
            .map(|p| matches!(p, P::ReadOnly | P::ReadExecute))
            .collect();
        assert_eq!(
            mapped.read_only_pages(),
            expected_mask,
            "ELF NX mask must match independent section oracle"
        );
        assert_eq!(mapped.page_count(), count);
        assert_eq!(mapped.span(), span);
        let audit = mapped.audit_for_test().unwrap();
        assert_eq!(audit.pte_pages, count);
        assert_eq!(audit.frame_matches, count);
        assert_eq!(audit.ro_nonx, 6);
        assert_eq!(audit.rw_nonx, 4);
        assert_eq!(audit.supervisor, count);
        assert_eq!(audit.writeback, count);
        assert_eq!(audit.global, count);
        let mut readback = vec![0; span];
        mapped.read_bytes(0, &mut readback).unwrap();
        assert_eq!(readback, expected);
        let mut relocated = vec![0; relocation_oracle.len()];
        mapped.read_bytes(field, &mut relocated).unwrap();
        assert_eq!(relocated, relocation_oracle);
        total_bytes += readback.len() + relocated.len();
        for (off, mut sentinel, error) in [
            (span, vec![0xca], ostd::Error::InvalidArgs),
            (usize::MAX, vec![0xca; 2], ostd::Error::Overflow),
        ] {
            let before = sentinel.clone();
            assert!(matches!(mapped.read_bytes(off, &mut sentinel), Err(found) if found == error));
            assert_eq!(sentinel, before);
            bounds += 1;
        }
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, pages),
            Err(ostd::Error::NoMemory)
        ));
        println!(
            "CYBERMANJU_ELF_SELECTIVE_NX:case:PASS kind={} pages={} intended_ro=4 intended_rw=4 intended_rx=2 ro_nonx={} rw_nonx={} pte_pages={} frame_matches={} supervisor={} writeback={} global={} plan_retained=true mask_oracle=true irq_preserved=true",
            kind,
            count,
            audit.ro_nonx,
            audit.rw_nonx,
            audit.pte_pages,
            audit.frame_matches,
            audit.supervisor,
            audit.writeback,
            audit.global
        );
        drop(mapped);
        assert_eq!(irq::is_local_enabled(), before_irq);
        let s = stats.snapshot();
        assert_eq!(s.acquired, count);
        assert_eq!(s.dropped, s.acquired);
        acquired += s.acquired;
        dropped += s.dropped;
        reclaim(range.start, envelope, pages);
        reclaims += 1;
    }
    println!(
        "CYBERMANJU_ELF_SELECTIVE_NX:complete:PASS kinds={} bytes={} bounds={} fault_cases={} fault_prefix={} budget_public={} budget_observed={} gap_public=1 gap_observed=1 reclaims={} acquired={} dropped={} live=0 nx_projection_applied=true elf_rx_applied=false gap_mapping_supported=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false production_qualified=false",
        reclaims,
        total_bytes,
        bounds,
        fault_reclaims,
        fault_prefix,
        budget_public,
        budget_observed,
        reclaims + fault_reclaims + budget_reclaims + gap_reclaims,
        acquired + fault_acquired + budget_acquired + gap_acquired,
        dropped + fault_dropped + budget_dropped + gap_dropped
    );
}

#[ktest]
fn cybermanju_elf_selective_nx_smp_runtime() {
    assert_eq!(cpu::num_cpus(), 2);
    println!("");
    println!("CYBERMANJU_ELF_SELECTIVE_NX_SMP:begin:PASS cpus=2");
    let base = NearTextAddressReservation::signed32_arena_range().end - 64 * PAGE_SIZE;
    let bytes = imported(1, b"memcmp").0;
    let envelope = page_layout_span_bound(&bytes, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let pages = envelope / PAGE_SIZE;
    let stats = Arc::new(FaultStats::new());
    let staged = owner(&bytes, base, pages)
        .prepare_and_stage_observed_for_test(&bytes, limits(pages), &stats)
        .unwrap();
    let range = staged.reserved_range();
    let before = irq::is_local_enabled();
    assert!(matches!(
        staged.into_signed32_selective_nx(),
        Err(ostd::Error::AccessDenied)
    ));
    assert_eq!(irq::is_local_enabled(), before);
    let s = stats.snapshot();
    assert_eq!(s.acquired, 10);
    assert_eq!(s.acquired, s.dropped);
    reclaim(range.start, envelope, pages);
    println!(
        "CYBERMANJU_ELF_SELECTIVE_NX_SMP:complete:PASS attempts=1 reclaims=1 acquired={} dropped={} live=0 irq_preserved=true factory_rw_denied_before_conversion=true selective_mapping=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false production_qualified=false",
        s.acquired, s.dropped
    );
}
