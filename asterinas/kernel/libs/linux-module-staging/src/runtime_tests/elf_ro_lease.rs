// SPDX-License-Identifier: MPL-2.0
//! Read-only/NX ELF lease coverage. This is data-only and never executes modules.
use super::{MAX_SECTIONS, MAX_SPAN, PreparationLimits, c_shim_import::imported};
use crate::{CShimExportRegistry, PolicyNearTextModuleLayout};
use alloc::{sync::Arc, vec, vec::Vec};
use linux_module_elf::{IntendedPagePermission, page_layout_span_bound, plan_page_layout};
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
fn fixture(kind: u32) -> (Vec<u8>, u16, u64) {
    imported(kind, b"memcmp")
}
fn reclaim(base: usize, envelope: usize, pages: usize) {
    let owner = NearTextAddressReservation::new_signed32_at(base, envelope, pages).unwrap();
    assert_eq!(owner.range(), base..base + envelope);
    drop(owner);
}
fn owner(bytes: &[u8], base: usize, pages: usize) -> crate::PolicyNearTextModuleLayout {
    PolicyNearTextModuleLayout::new_signed32_c_shims_at(bytes, base, pages, MAX_SECTIONS, 65_536)
        .unwrap()
}

#[ktest]
fn cybermanju_elf_ro_lease_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 1);
    let kinds = [1u32, 2, 4, 11];
    println!("CYBERMANJU_ELF_RO_LEASE:begin:PASS cpus=1 kinds=4");
    let base = NearTextAddressReservation::signed32_arena_range().end - 64 * PAGE_SIZE;
    let sample = fixture(1).0;
    let envelope = page_layout_span_bound(&sample, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let pages = envelope / PAGE_SIZE;
    let mut fault_prefix = 0usize;
    let mut fault_acquired = 0usize;
    let mut fault_dropped = 0usize;
    for after in [0usize, 1, pages] {
        let bytes = fixture(1).0;
        let stats = Arc::new(FaultStats::new());
        let staged = owner(&bytes, base, pages)
            .prepare_and_stage_observed_for_test(&bytes, limits(pages), &stats)
            .unwrap();
        let range = staged.reserved_range();
        let mut observed = 0;
        assert!(
            matches!(
                staged.into_signed32_ro_nx_with_seal_fault_for_test(after, &mut observed),
                Err(ostd::Error::IoError)
            ),
            "ELF RO seal fault must roll back"
        );
        assert_eq!(observed, after, "ELF RO prefix must match");
        let s = stats.snapshot();
        assert_eq!(s.dropped, s.acquired, "ELF RO handles must drop");
        fault_prefix += observed;
        fault_acquired += s.acquired;
        fault_dropped += s.dropped;
        reclaim(range.start, envelope, pages);
    }

    let mut budget_acquired = 0usize;
    let mut budget_dropped = 0usize;
    let mut public_budget_acquired = 0usize;
    let mut public_budget_dropped = 0usize;
    let mut budget_reclaims = 0usize;
    for budget in [0usize, pages - 1] {
        let bytes = fixture(1).0;
        let stats = Arc::new(FaultStats::new());
        let staged = owner(&bytes, base, pages)
            .prepare_and_stage_observed_for_test(&bytes, limits(pages), &stats)
            .unwrap();
        let range = staged.reserved_range();
        let mut entries = 0;
        assert!(
            matches!(
                staged.into_signed32_ro_nx_with_budget_observed_for_test(budget, &mut entries),
                Err(ostd::Error::NoMemory)
            ),
            "ELF RO budget must reject"
        );
        assert_eq!(entries, 0, "ELF RO budget must not enter mapper");
        let s = stats.snapshot();
        assert_eq!(s.dropped, s.acquired, "ELF RO handles must drop");
        budget_acquired += s.acquired;
        budget_dropped += s.dropped;
        reclaim(range.start, envelope, pages);
        budget_reclaims += 1;

        let public_bytes = fixture(1).0;
        let public_stats = Arc::new(FaultStats::new());
        let public_staged = owner(&public_bytes, base, pages)
            .prepare_and_stage_observed_for_test(&public_bytes, limits(pages), &public_stats)
            .unwrap();
        let public_range = public_staged.reserved_range();
        assert!(
            matches!(
                public_staged.into_signed32_ro_nx_with_page_budget(budget),
                Err(ostd::Error::NoMemory)
            ),
            "ELF RO public budget must reject"
        );
        let public_snapshot = public_stats.snapshot();
        assert_eq!(
            public_snapshot.dropped, public_snapshot.acquired,
            "ELF RO handles must drop"
        );
        public_budget_acquired += public_snapshot.acquired;
        public_budget_dropped += public_snapshot.dropped;
        reclaim(public_range.start, envelope, pages);
        budget_reclaims += 1;
    }

    let mut bounds = 0usize;
    let mut reclaims = 0usize;
    let mut acquired = 0usize;
    let mut dropped = 0usize;
    for kind in kinds {
        let (bytes, target, offset) = fixture(kind);
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
        let placement = layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let field = usize::try_from(placement.address + offset - base_u64).unwrap();
        let entry: ostd::linux_abi::MemcmpFn = ostd::linux_abi::memcmp_entry();
        let address = entry as usize as u64;
        let oracle = match kind {
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
        assert_eq!(&expected[field..field + oracle.len()], oracle.as_slice());
        let stats = Arc::new(FaultStats::new());
        let mut wanted = vec![0xff; span];
        let staged = layout_owner
            .prepare_and_stage_observed_for_test(&bytes, limits(pages), &stats)
            .unwrap();
        assert_eq!(stats.snapshot().dropped, 0);
        staged.read_bytes(0, &mut wanted).unwrap();
        assert_eq!(wanted, expected);
        let plan = staged.planned_permissions(pages).unwrap();
        let planned_rx = plan
            .pages()
            .iter()
            .filter(|p| **p == IntendedPagePermission::ReadExecute)
            .count();
        assert!(planned_rx > 0);
        let range = staged.reserved_range();
        let irq_before = irq::is_local_enabled();
        let mapped = if kind == 1 {
            staged
                .into_signed32_rw_nx()
                .unwrap()
                .into_read_only_nx()
                .unwrap()
        } else {
            staged.into_signed32_ro_nx().unwrap()
        };
        assert_eq!(irq::is_local_enabled(), irq_before);
        assert_eq!(
            mapped.permission_plan(),
            &plan,
            "ELF RO plan must survive handoff"
        );
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
        assert_eq!((mapped.span(), mapped.page_count()), (span, count));
        let audit = mapped.audit_for_test().unwrap();
        assert_eq!(audit.pte_pages, count, "ELF RO PTEs must be read-only");
        assert_eq!(audit.frame_matches, count);
        assert_eq!(audit.ro_nonx, count, "ELF RO PTEs must be read-only");
        assert_eq!(audit.rw_nonx, 0);
        assert_eq!(audit.supervisor, count);
        assert_eq!(audit.writeback, count);
        assert_eq!(audit.global, count);
        assert_eq!(stats.snapshot().acquired, count);
        assert_eq!(stats.snapshot().dropped, 0);
        let mut actual = vec![0xff; span];
        mapped.read_bytes(0, &mut actual).unwrap();
        assert_eq!(actual, expected, "ELF RO mapped bytes must match");
        let mut field_actual = vec![0xff; oracle.len()];
        mapped.read_bytes(field, &mut field_actual).unwrap();
        assert_eq!(field_actual, oracle, "ELF RO mapped bytes must match");
        for (off, mut sentinel, expected_error) in [
            (span, vec![0xca], ostd::Error::InvalidArgs),
            (usize::MAX, vec![0xca; 2], ostd::Error::Overflow),
        ] {
            let before = sentinel.clone();
            assert!(
                matches!(mapped.read_bytes(off, &mut sentinel), Err(error) if error == expected_error)
            );
            assert_eq!(sentinel, before);
            bounds += 1;
        }
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, pages),
            Err(ostd::Error::NoMemory)
        ));
        drop(mapped);
        let stats_after = stats.snapshot();
        assert_eq!(stats_after.acquired, count);
        assert_eq!(stats_after.dropped, count, "ELF RO handles must drop");
        reclaim(range.start, envelope, pages);
        reclaims += 1;
        acquired += count;
        dropped += count;
        println!(
            "CYBERMANJU_ELF_RO_LEASE:case:PASS kind={} span={} envelope={} pages={} bytes={} relocation_bytes={} frame_matches={} ro_nonx={} rw_nonx=0 planned_pages={} planned_rx={} acquired={} dropped={} retained=true layout_preserved=true plan_preserved=true reclaimed=true",
            kind,
            span,
            envelope,
            count,
            span + oracle.len(),
            oracle.len(),
            audit.frame_matches,
            audit.ro_nonx,
            plan.page_count(),
            planned_rx,
            count,
            count
        );
    }
    println!(
        "CYBERMANJU_ELF_RO_LEASE:faults:PASS cases=3 prefix_pages={} reclaims=3 acquired={} dropped={} live=0",
        fault_prefix, fault_acquired, fault_dropped
    );
    println!(
        "CYBERMANJU_ELF_RO_LEASE:budgets:PASS cases=2 public_cases=2 zero_budget=0 short_budget={} mapper_entries=0 reclaims={} acquired={} dropped={} live=0",
        pages - 1,
        budget_reclaims,
        budget_acquired + public_budget_acquired,
        budget_dropped + public_budget_dropped
    );
    println!(
        "CYBERMANJU_ELF_RO_LEASE:complete:PASS mappings=4 bounds_rejected={} reclaims={} acquired={} dropped={} live=0 elf_handoff=true plan_applied=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false",
        bounds,
        reclaims + 3 + budget_reclaims,
        acquired + fault_acquired + budget_acquired + public_budget_acquired,
        dropped + fault_dropped + budget_dropped + public_budget_dropped
    );
}

#[ktest]
fn cybermanju_elf_ro_lease_smp_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 2);
    let base = NearTextAddressReservation::signed32_arena_range().end - 64 * PAGE_SIZE;
    let bytes = fixture(1).0;
    let envelope = page_layout_span_bound(&bytes, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let pages = envelope / PAGE_SIZE;
    println!(
        "CYBERMANJU_ELF_RO_LEASE_SMP:begin:PASS cpus=2 kinds=4 pages={}",
        pages
    );
    let mut acquired = 0;
    let mut dropped = 0;
    for (index, kind) in [1u32, 2, 4, 11].into_iter().enumerate() {
        let bytes = fixture(kind).0;
        let irq_before = irq::is_local_enabled();
        let irq_guard = if index % 2 == 0 {
            Some(irq::disable_local())
        } else {
            None
        };
        let expected_irq = if irq_guard.is_some() {
            false
        } else {
            irq_before
        };
        let stats = Arc::new(FaultStats::new());
        let staged = owner(&bytes, base, pages)
            .prepare_and_stage_observed_for_test(&bytes, limits(pages), &stats)
            .unwrap();
        let range = staged.reserved_range();
        assert!(matches!(
            staged.into_signed32_ro_nx(),
            Err(ostd::Error::AccessDenied)
        ));
        assert_eq!(irq::is_local_enabled(), expected_irq);
        let s = stats.snapshot();
        assert_eq!(s.dropped, s.acquired, "ELF RO handles must drop");
        acquired += s.acquired;
        dropped += s.dropped;
        reclaim(range.start, envelope, pages);
        drop(irq_guard);
        assert_eq!(irq::is_local_enabled(), irq_before);
    }
    println!(
        "CYBERMANJU_ELF_RO_LEASE_SMP:denied:PASS attempts=4 reclaims=4 acquired={} dropped={} live=0 irq_preserved=true",
        acquired, dropped
    );
    println!(
        "CYBERMANJU_ELF_RO_LEASE_SMP:complete:PASS elf_handoff=false plan_applied=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false"
    );
}
