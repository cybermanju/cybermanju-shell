// SPDX-License-Identifier: MPL-2.0
//! Runtime checks for selective supervisor/WB/NX signed32 aliases.
use super::{KERNEL_PAGE_TABLE, MappedItemRef};
use crate::{
    Error,
    cpu::num_cpus,
    irq,
    mm::{
        Frame, HasPaddr, PAGE_SIZE, Vaddr,
        module_address::NearTextAddressReservation,
        module_staging::{
            ModuleChunk, ModuleStaging,
            fault_injection::{FaultStats, stage_observed},
        },
        page_prop::{PageFlags, PrivilegedPageFlags},
    },
    prelude::{ktest, println},
};
use alloc::{sync::Arc, vec, vec::Vec};
use core::ops::Range;
const SPAN: usize = 3 * PAGE_SIZE;
const ENVELOPE: usize = 4 * PAGE_SIZE;
fn pattern_a() -> Vec<u8> {
    (0..SPAN).map(|i| ((i * 19 + 3) % 251) as u8).collect()
}
fn pattern_b() -> Vec<u8> {
    (0..SPAN)
        .map(|i| 255u8.wrapping_sub(((i * 31 + 11) % 251) as u8))
        .collect()
}
fn stage(data: &[u8], stats: &Arc<FaultStats>) -> ModuleStaging {
    stage_observed(
        SPAN,
        &[ModuleChunk {
            offset: 0,
            bytes: data,
        }],
        3,
        1,
        stats,
    )
    .unwrap()
}
fn absent(range: Range<Vaddr>) {
    let guard = irq::disable_local();
    let mut cursor = KERNEL_PAGE_TABLE
        .get()
        .unwrap()
        .cursor(&guard, &range)
        .unwrap();
    for va in range.step_by(PAGE_SIZE) {
        cursor.jump(va).unwrap();
        assert!(
            cursor.query().unwrap().1.is_none(),
            "selective lease PTE absent"
        );
    }
}
fn reclaim(base: Vaddr) {
    let r = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap();
    absent(r.range());
    drop(r);
}
#[ktest]
fn cybermanju_signed32_selective_nx_runtime() {
    println!("");
    assert_eq!(num_cpus(), 1);
    let base = NearTextAddressReservation::signed32_arena_range().start + 40 * PAGE_SIZE;
    let full = pattern_a();
    let mask = [true, false, true];
    println!("CYBERMANJU_SIGNED32_SELECTIVE_NX:begin:PASS cpus=1");
    let mut fault_prefix = 0;
    let mut acquired = 0;
    let mut dropped = 0;
    let mut reclaims = 0;
    for after in 0..=2 {
        let stats = Arc::new(FaultStats::new());
        let staged = stage(&full, &stats);
        let owner = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap();
        let range = owner.range();
        let rw = staged.map_signed32_rw_nx(owner).unwrap();
        let irq_before = irq::is_local_enabled();
        let mut seen = usize::MAX;
        assert!(
            matches!(
                rw.into_selective_nx_with_fault_for_test(&mask, after, &mut seen),
                Err(Error::IoError)
            ),
            "selective fault must fail"
        );
        assert_eq!(irq::is_local_enabled(), irq_before);
        assert_eq!(seen, after, "selective fault prefix must match");
        fault_prefix += seen;
        let s = stats.snapshot();
        assert_eq!(
            (s.acquired, s.dropped),
            (3, 3),
            "selective fault handles must drop"
        );
        acquired += s.acquired;
        dropped += s.dropped;
        absent(range);
        reclaim(base);
        reclaims += 1;
    }
    assert_eq!((fault_prefix, acquired, dropped), (3, 9, 9));
    let mut malformed = 0;
    for bad in [&[][..], &[true, false][..], &[true, false, true, false][..]] {
        let stats = Arc::new(FaultStats::new());
        let owner = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap();
        let rw = stage(&full, &stats).map_signed32_rw_nx(owner).unwrap();
        let irq_before = irq::is_local_enabled();
        assert!(
            matches!(rw.into_selective_nx(bad), Err(Error::InvalidArgs)),
            "selective malformed mask must reject"
        );
        assert_eq!(irq::is_local_enabled(), irq_before);
        let s = stats.snapshot();
        assert_eq!(
            (s.acquired, s.dropped),
            (3, 3),
            "selective malformed handles must drop"
        );
        acquired += s.acquired;
        dropped += s.dropped;
        reclaim(base);
        reclaims += 1;
        malformed += 1;
    }
    assert_eq!(malformed, 3);
    let irq_before = irq::is_local_enabled();
    let stats = Arc::new(FaultStats::new());
    let stage_a = stage(&full, &stats);
    let frames: Vec<Frame<()>> = stage_a.frames_for_signed32().collect();
    let rw = stage_a
        .map_signed32_rw_nx(NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap())
        .unwrap();
    let mapping = rw.into_selective_nx(&mask).unwrap();
    assert_eq!(irq::is_local_enabled(), irq_before);
    let audit = mapping.audit_for_test().unwrap();
    assert_eq!((mapping.page_count(), mapping.span()), (3, SPAN));
    assert_eq!(
        (
            audit.pte_pages,
            audit.frame_matches,
            audit.ro_nonx,
            audit.rw_nonx
        ),
        (3, 3, 2, 1)
    );
    assert_eq!(
        (
            audit.supervisor,
            audit.writeback,
            audit.global,
            audit.slack_absent
        ),
        (3, 3, 3, 1)
    );
    assert_eq!(mapping.read_only_pages(), &mask);
    // Independent PTE oracle: count-only audit cannot hide an RO/RW permutation.
    {
        let guard = irq::disable_local();
        let range = mapping.reserved_range();
        let mut cursor = KERNEL_PAGE_TABLE
            .get()
            .unwrap()
            .cursor(&guard, &range)
            .unwrap();
        for (i, (expected, ro)) in frames.iter().zip(mask).enumerate() {
            cursor.jump(range.start + i * PAGE_SIZE).unwrap();
            let Some(MappedItemRef::Tracked(actual, prop)) = cursor.query().unwrap().1 else {
                panic!("expected tracked PTE")
            };
            assert_eq!(actual.paddr(), expected.paddr());
            assert!(prop.flags.contains(PageFlags::R));
            assert_eq!(prop.flags.contains(PageFlags::W), !ro);
            assert!(!prop.flags.contains(PageFlags::X));
            assert!(!prop.priv_flags.contains(PrivilegedPageFlags::USER));
            assert_eq!(prop.cache, crate::mm::page_prop::CachePolicy::Writeback);
            assert!(prop.priv_flags.contains(PrivilegedPageFlags::GLOBAL));
        }
        cursor.jump(range.start + 3 * PAGE_SIZE).unwrap();
        assert!(cursor.query().unwrap().1.is_none());
    }
    let mut bytes = vec![0; SPAN];
    mapping.read_bytes(0, &mut bytes).unwrap();
    assert_eq!(bytes, full);
    let mut bounds = 0;
    for (offset, len, error) in [
        (SPAN, 1, Error::InvalidArgs),
        (SPAN - 1, 2, Error::InvalidArgs),
        (usize::MAX, 2, Error::Overflow),
    ] {
        let mut out = [0xc7; 2];
        assert_eq!(mapping.read_bytes(offset, &mut out[..len]), Err(error));
        assert_eq!(out, [0xc7; 2]);
        bounds += 1;
    }
    assert_eq!(irq::is_local_enabled(), irq_before);
    assert_eq!(
        (stats.snapshot().acquired, stats.snapshot().dropped),
        (3, 0)
    );
    println!(
        "CYBERMANJU_SIGNED32_SELECTIVE_NX:sealed:PASS span=12288 pages=3 envelope_pages=4 pte_pages=3 frame_matches=3 ro_nonx=2 rw_nonx=1 supervisor=3 writeback=3 global=3 slack_absent=1 bytes=12288 mask_retained=true irq_preserved=true"
    );
    println!(
        "CYBERMANJU_SIGNED32_SELECTIVE_NX:faults:PASS cases=3 prefix_pages=3 reclaims=3 acquired=9 dropped=9 live=0"
    );
    println!(
        "CYBERMANJU_SIGNED32_SELECTIVE_NX:validation:PASS bounds={} output_preserved=true irq_preserved=true malformed_masks={} cleanup=true",
        bounds, malformed
    );
    drop(mapping);
    let s = stats.snapshot();
    assert_eq!(
        (s.acquired, s.dropped),
        (3, 3),
        "selective success handles must drop"
    );
    acquired += s.acquired;
    dropped += s.dropped;
    reclaim(base);
    reclaims += 1;
    drop(frames);
    let other = pattern_b();
    for (boundary_mask, data) in [([true; 3], &full), ([false; 3], &other)] {
        let stats = Arc::new(FaultStats::new());
        let rw = stage(data, &stats)
            .map_signed32_rw_nx(
                NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap(),
            )
            .unwrap();
        let mapped = rw.into_selective_nx(&boundary_mask).unwrap();
        assert_eq!(mapped.read_only_pages(), &boundary_mask);
        let audit = mapped.audit_for_test().unwrap();
        let ro = boundary_mask.iter().filter(|v| **v).count();
        assert_eq!(
            (
                audit.pte_pages,
                audit.frame_matches,
                audit.ro_nonx,
                audit.rw_nonx
            ),
            (3, 3, ro, 3 - ro)
        );
        assert_eq!(
            (
                audit.supervisor,
                audit.writeback,
                audit.global,
                audit.slack_absent
            ),
            (3, 3, 3, 1)
        );
        mapped.read_bytes(0, &mut bytes).unwrap();
        assert_eq!(&bytes, data);
        assert_eq!(irq::is_local_enabled(), irq_before);
        drop(mapped);
        let s = stats.snapshot();
        assert_eq!(
            (s.acquired, s.dropped),
            (3, 3),
            "selective boundary handles must drop"
        );
        acquired += s.acquired;
        dropped += s.dropped;
        reclaim(base);
        reclaims += 1;
    }
    assert_eq!((acquired, dropped, reclaims), (27, 27, 9));
    println!(
        "CYBERMANJU_SIGNED32_SELECTIVE_NX:extremes:PASS cases=2 all_ro=3 all_rw=3 bytes=24576 irq_preserved=true"
    );
    println!(
        "CYBERMANJU_SIGNED32_SELECTIVE_NX:complete:PASS protected_mappings=3 total_reclaims={} acquired={} dropped={} live=0 elf_plan_applied=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false",
        reclaims, acquired, dropped
    );
}
#[ktest]
fn cybermanju_signed32_selective_nx_smp_runtime() {
    println!("");
    assert_eq!(num_cpus(), 2);
    let base = NearTextAddressReservation::signed32_arena_range().start + 48 * PAGE_SIZE;
    let data = pattern_a();
    let enabled = irq::is_local_enabled();
    println!("CYBERMANJU_SIGNED32_SELECTIVE_NX_SMP:begin:PASS cpus=2");
    let mut attempts = 0;
    for disabled in [false, true] {
        let stats = Arc::new(FaultStats::new());
        let observed = stage(&data, &stats);
        let range = base..base + ENVELOPE;
        let result = if disabled {
            let guard = irq::disable_local();
            let r = observed.map_signed32_rw_nx(
                NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap(),
            );
            assert!(!irq::is_local_enabled());
            drop(guard);
            r
        } else {
            observed.map_signed32_rw_nx(
                NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap(),
            )
        };
        assert!(matches!(result, Err(Error::AccessDenied)));
        assert_eq!(irq::is_local_enabled(), enabled);
        absent(range);
        reclaim(base);
        let s = stats.snapshot();
        assert_eq!((s.acquired, s.dropped), (3, 3));
        attempts += 1;
    }
    assert_eq!(attempts, 2);
    println!(
        "CYBERMANJU_SIGNED32_SELECTIVE_NX_SMP:denied:PASS attempts=2 reclaims=2 acquired=6 dropped=6 live=0 irq_preserved=true factory_rw_denied_before_conversion=true"
    );
    println!(
        "CYBERMANJU_SIGNED32_SELECTIVE_NX_SMP:complete:PASS selective_mapping=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false"
    );
}
