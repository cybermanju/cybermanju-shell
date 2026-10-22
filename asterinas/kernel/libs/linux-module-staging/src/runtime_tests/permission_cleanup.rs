// SPDX-License-Identifier: MPL-2.0
//! Permission-budget preflight cleanup and recovery; mapped bytes are never executed.
use alloc::{sync::Arc, vec};

use linux_module_elf::page_layout_span_bound;
use ostd::{
    cpu, irq,
    mm::{
        PAGE_SIZE, module_address::NearTextAddressReservation,
        module_staging::fault_injection::FaultStats,
    },
    prelude::*,
};

use super::{MAX_SECTIONS, MAX_SPAN, c_shim_import::imported};
use crate::{PolicyNearTextModuleLayout, PreparationLimits};

fn limits(pages: usize, chunks: usize) -> PreparationLimits {
    PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages: pages,
        max_chunks: chunks,
    }
}

#[ktest]
fn cybermanju_permission_cleanup_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 1);
    let cpus = cpu::num_cpus();
    println!(
        "CYBERMANJU_PERMISSION_CLEANUP:begin:PASS cpus={} denied_cases=2",
        cpus
    );
    let bytes = imported(1, b"memcmp").0;
    let envelope = page_layout_span_bound(&bytes, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    assert!(envelope > 0 && envelope.is_multiple_of(PAGE_SIZE));
    let max_pages = envelope / PAGE_SIZE;
    let base = NearTextAddressReservation::signed32_arena_range().end - 64 * PAGE_SIZE;
    let mut irq_before = irq::is_local_enabled();
    let mut denied = 0usize;
    let mut public_denied = 0usize;
    let mut public_reclaims = 0usize;
    let mut total_acquired = 0usize;
    let mut total_dropped = 0usize;
    let mut total_writes = 0usize;
    let mut reclaims = 0usize;

    for id in 0..2 {
        let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let span = owner.layout().span() as usize;
        let pages = span / PAGE_SIZE;
        let budget = if id == 0 { 0 } else { pages - 1 };
        let stats = Arc::new(FaultStats::new());
        let mut mapper_entries = 0usize;
        let staged = owner
            .prepare_and_stage_observed_for_test(&bytes, limits(pages, MAX_SECTIONS), &stats)
            .unwrap();
        assert_eq!(irq::is_local_enabled(), irq_before);
        let before = stats.snapshot();
        assert_eq!(
            (before.entries, before.acquired, before.dropped),
            (1, pages, 0)
        );
        assert!(before.writes > 0 && !before.triggered);
        let result =
            staged.into_signed32_rw_nx_with_budget_observed_for_test(budget, &mut mapper_entries);
        assert!(
            matches!(result, Err(ostd::Error::NoMemory)),
            "permission preflight consumer must reject budget"
        );
        assert_eq!(
            mapper_entries, 0,
            "permission preflight must not enter mapper"
        );
        let s = stats.snapshot();
        assert_eq!(s.entries, 1);
        assert_eq!(s.acquired, pages);
        assert_eq!(
            s.dropped, s.acquired,
            "permission preflight must drop all tracked handles"
        );
        assert!(s.writes > 0 && !s.triggered);
        total_acquired += s.acquired;
        total_dropped += s.dropped;
        total_writes += s.writes;
        let reclaimed =
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        assert_eq!(reclaimed.range(), base..base + envelope);
        drop(reclaimed);
        reclaims += 1;
        // Exercise the public wrapper with the normal, unobserved staging backend too.
        let public_owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            max_pages,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let public_staged = public_owner
            .prepare_and_stage(&bytes, limits(pages, MAX_SECTIONS))
            .unwrap();
        assert!(
            matches!(
                public_staged.into_signed32_rw_nx_with_page_budget(budget),
                Err(ostd::Error::NoMemory)
            ),
            "public permission budget must be enforced"
        );
        public_denied += 1;
        let public_reclaimed =
            NearTextAddressReservation::new_signed32_at(base, envelope, max_pages).unwrap();
        assert_eq!(public_reclaimed.range(), base..base + envelope);
        drop(public_reclaimed);
        public_reclaims += 1;
        reclaims += 1;
        assert_eq!(irq::is_local_enabled(), irq_before);
        println!(
            "CYBERMANJU_PERMISSION_CLEANUP:denied:PASS id={} budget={} span={} pages={} acquired={} dropped={} mapper_entries={} writes={} reclaimed=true",
            id, budget, span, pages, s.acquired, s.dropped, mapper_entries, s.writes
        );
        denied += 1;
    }

    let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
        &bytes,
        base,
        max_pages,
        MAX_SECTIONS,
        65_536,
    )
    .unwrap();
    let span = owner.layout().span() as usize;
    let pages = span / PAGE_SIZE;
    let chunks = owner.layout().placements().len();
    let stats = Arc::new(FaultStats::new());
    let staged = owner
        .prepare_and_stage_observed_for_test(&bytes, limits(pages, chunks), &stats)
        .unwrap();
    let mut expected = vec![0u8; span];
    staged.read_bytes(0, &mut expected).unwrap();
    let mut mapper_entries = 0usize;
    irq_before = irq::is_local_enabled();
    let mapped = staged
        .into_signed32_rw_nx_with_budget_observed_for_test(pages, &mut mapper_entries)
        .unwrap();
    assert_eq!(irq::is_local_enabled(), irq_before);
    assert_eq!(mapper_entries, 1);
    let s = stats.snapshot();
    assert_eq!(s.entries, 1);
    assert_eq!(s.acquired, pages);
    assert_eq!(s.dropped, 0);
    assert!(s.writes > 0 && !s.triggered);
    let plan = mapped.permission_plan();
    assert_eq!(plan.span() as usize, span);
    assert_eq!(plan.pages().len(), pages);
    let audit = mapped.audit_for_test().unwrap();
    assert_eq!(audit.pte_pages, pages);
    assert_eq!(audit.frame_matches, pages);
    assert_eq!(audit.supervisor, pages);
    assert_eq!(audit.writeback, pages);
    assert_eq!(audit.global, pages);
    assert_eq!(audit.rw_nonx, pages);
    let mut actual = vec![0u8; span];
    mapped.read_bytes(0, &mut actual).unwrap();
    assert_eq!(
        actual, expected,
        "permission recovery mapping must preserve bytes"
    );
    let range = mapped.reserved_range();
    let retained = matches!(
        NearTextAddressReservation::new_signed32_at(range.start, envelope, max_pages),
        Err(ostd::Error::NoMemory)
    );
    assert!(retained);
    let before_drop = stats.snapshot();
    assert_eq!(before_drop.dropped, 0);
    drop(mapped);
    assert_eq!(irq::is_local_enabled(), irq_before);
    let after_drop = stats.snapshot();
    assert_eq!(after_drop.dropped, after_drop.acquired);
    total_acquired += after_drop.acquired;
    total_dropped += after_drop.dropped;
    total_writes += after_drop.writes;
    let reclaimed =
        NearTextAddressReservation::new_signed32_at(range.start, envelope, max_pages).unwrap();
    assert_eq!(reclaimed.range(), range);
    drop(reclaimed);
    reclaims += 1;
    println!(
        "CYBERMANJU_PERMISSION_CLEANUP:success:PASS budget={} span={} pages={} acquired={} dropped={} mapper_entries={} writes={} bytes={} nonx_pages={} retained=true reclaimed=true",
        pages,
        span,
        pages,
        after_drop.acquired,
        after_drop.dropped,
        mapper_entries,
        after_drop.writes,
        actual.len(),
        audit.rw_nonx
    );
    assert_eq!(
        (
            denied,
            public_denied,
            public_reclaims,
            mapper_entries,
            total_acquired,
            total_dropped,
            reclaims
        ),
        (2, 2, 2, 1, pages * 3, pages * 3, 5)
    );
    println!(
        "CYBERMANJU_PERMISSION_CLEANUP:complete:PASS denied={} public_denied={} public_reclaims={} recovered=1 mapper_entries={} acquired={} dropped={} reclaims={} injected_heap_oom=false module_execution=false physical_wx=false smp_mapping=false",
        denied,
        public_denied,
        public_reclaims,
        mapper_entries,
        total_acquired,
        total_dropped,
        reclaims
    );
    assert!(total_writes > 0);
}
