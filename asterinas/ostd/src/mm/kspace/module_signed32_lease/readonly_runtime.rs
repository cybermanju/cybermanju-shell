// SPDX-License-Identifier: MPL-2.0
//! Runtime checks for signed-32 read-only/non-executable lease transitions.
use alloc::{sync::Arc, vec, vec::Vec};
use core::ops::Range;

use super::{KERNEL_PAGE_TABLE, MappedItemRef};
use crate::{
    Error,
    cpu::num_cpus,
    irq,
    mm::{
        Frame, HasPaddr, PAGE_SIZE, Vaddr, VmIo,
        module_address::NearTextAddressReservation,
        module_staging::{
            ModuleChunk, ModuleStaging,
            fault_injection::{FaultStats, stage_observed},
        },
        page_prop::PageFlags,
    },
    prelude::{ktest, println},
};

const SPAN: usize = 2 * PAGE_SIZE;
const ENVELOPE: usize = 3 * PAGE_SIZE;

fn pattern_a() -> Vec<u8> {
    (0..SPAN).map(|i| ((i * 17) % 251) as u8).collect()
}
fn pattern_b() -> Vec<u8> {
    (0..SPAN)
        .map(|i| 255u8.wrapping_sub(((i * 29 + 7) % 251) as u8))
        .collect()
}
fn stage(data: &[u8]) -> ModuleStaging {
    ModuleStaging::new(
        SPAN,
        &[ModuleChunk {
            offset: 0,
            bytes: data,
        }],
        2,
        1,
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
    for va in range.clone().step_by(PAGE_SIZE) {
        cursor.jump(va).unwrap();
        assert!(
            cursor.query().unwrap().1.is_none(),
            "lease PTE must be absent"
        );
    }
}
fn reclaim(base: Vaddr, span: usize, pages: usize) {
    let owner = NearTextAddressReservation::new_signed32_at(base, span, pages)
        .expect("reservation must be reclaimable");
    absent(owner.range());
    drop(owner);
}
fn frame_audit(range: &Range<Vaddr>, frames: &[Frame<()>]) -> (usize, usize, usize) {
    let guard = irq::disable_local();
    let mut cursor = KERNEL_PAGE_TABLE
        .get()
        .unwrap()
        .cursor(&guard, range)
        .unwrap();
    let mut pages = 0;
    let mut matches = 0;
    let mut rw_nonx = 0;
    for (i, frame) in frames.iter().enumerate() {
        cursor.jump(range.start + i * PAGE_SIZE).unwrap();
        let Some(MappedItemRef::Tracked(actual, prop)) = cursor.query().unwrap().1 else {
            panic!("expected tracked mapping");
        };
        pages += 1;
        matches += usize::from(actual.paddr() == frame.paddr());
        rw_nonx +=
            usize::from(prop.flags.contains(PageFlags::RW) && !prop.flags.contains(PageFlags::X));
    }
    for va in (range.start + frames.len() * PAGE_SIZE..range.end).step_by(PAGE_SIZE) {
        cursor.jump(va).unwrap();
        assert!(
            cursor.query().unwrap().1.is_none(),
            "reservation slack must remain absent"
        );
    }
    (pages, matches, rw_nonx)
}

#[ktest]
fn cybermanju_signed32_ro_lease_runtime() {
    println!("");
    assert_eq!(num_cpus(), 1);
    let base = NearTextAddressReservation::signed32_arena_range().start + 32 * PAGE_SIZE;
    let full = pattern_a();
    let other = pattern_b();
    println!("CYBERMANJU_SIGNED32_RO_LEASE:begin:PASS cpus=1");

    let mut fault_prefix = 0usize;
    let mut fault_reclaims = 0usize;
    let mut acquired_total = 0usize;
    let mut dropped_total = 0usize;
    for after in 0..3 {
        let stats = Arc::new(FaultStats::new());
        let observed_stage = stage_observed(
            SPAN,
            &[ModuleChunk {
                offset: 0,
                bytes: &full,
            }],
            2,
            1,
            &stats,
        )
        .unwrap();
        let before = stats.snapshot();
        assert_eq!(before.acquired, 2);
        assert_eq!(before.dropped, 0);
        let owner = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap();
        let range = owner.range();
        let rw = observed_stage.map_signed32_rw_nx(owner).unwrap();
        let mut observed = usize::MAX;
        assert!(matches!(
            rw.into_read_only_nx_with_fault_for_test(after, &mut observed),
            Err(Error::IoError)
        ));
        assert_eq!(
            observed, after,
            "RO transition fault must report protected prefix"
        );
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.acquired, 2);
        assert_eq!(
            snapshot.dropped, 2,
            "RO transition must release tracked handles"
        );
        fault_prefix += observed;
        acquired_total += snapshot.acquired;
        dropped_total += snapshot.dropped;
        absent(range);
        reclaim(base, ENVELOPE, 3);
        fault_reclaims += 1;
    }
    assert_eq!(fault_reclaims, 3);
    assert_eq!(fault_prefix, 3);
    assert_eq!(acquired_total, 6);
    assert_eq!(dropped_total, 6);

    let irq_before = irq::is_local_enabled();
    let stage_a = stage(&full);
    let refs_a: Vec<Frame<()>> = stage_a.frames_for_signed32().collect();
    let owner_a = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap();
    let range_a = owner_a.range();
    let rw = stage_a.map_signed32_rw_nx(owner_a).unwrap();
    let (before_pages, before_matches, before_rw) = frame_audit(&range_a, &refs_a);
    assert_eq!(before_pages, 2);
    assert_eq!(before_matches, 2);
    assert_eq!(before_rw, 2);
    let ro = rw.into_read_only_nx().unwrap();
    assert_eq!(irq::is_local_enabled(), irq_before);
    let audit = ro.audit_for_test().unwrap();
    assert_eq!(ro.span(), SPAN);
    assert_eq!(ro.page_count(), 2);
    assert_eq!(audit.pte_pages, 2);
    assert_eq!(audit.frame_matches, 2);
    assert_eq!(audit.ro_nonx, 2);
    assert_eq!(audit.rw_nonx, 0);
    assert_eq!(audit.supervisor, 2);
    assert_eq!(audit.writeback, 2);
    assert_eq!(audit.global, 2);
    assert_eq!(audit.slack_absent, 1);
    assert!(matches!(
        NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3),
        Err(Error::NoMemory)
    ));
    let mut all = vec![0; SPAN];
    ro.read_bytes(0, &mut all).unwrap();
    assert_eq!(all, full);
    ro.read_bytes(SPAN, &mut []).unwrap();
    let mut bounds = 0;
    for (offset, len, error) in [
        (SPAN, 1, Error::InvalidArgs),
        (SPAN - 1, 2, Error::InvalidArgs),
        (usize::MAX, 2, Error::Overflow),
    ] {
        let mut output = [0xc7; 2];
        assert_eq!(ro.read_bytes(offset, &mut output[..len]), Err(error));
        assert_eq!(output, [0xc7; 2]);
        bounds += 1;
    }
    assert_eq!(irq::is_local_enabled(), irq_before);
    assert_eq!(bounds, 3);
    println!(
        "CYBERMANJU_SIGNED32_RO_LEASE:sealed:PASS span={} pages={} envelope_pages={} before_rw={} pte_pages={} frame_matches={} ro_nonx={} rw_nonx={} supervisor={} writeback={} global={} slack_absent={} bytes={} irq_preserved=true exclusive=true",
        ro.span(),
        ro.page_count(),
        ENVELOPE / PAGE_SIZE,
        before_rw,
        audit.pte_pages,
        audit.frame_matches,
        audit.ro_nonx,
        audit.rw_nonx,
        audit.supervisor,
        audit.writeback,
        audit.global,
        audit.slack_absent,
        all.len()
    );

    let sentinel = [0xa9; 32];
    refs_a[0].write_bytes(0, &sentinel).unwrap();
    let mut changed = [0; 32];
    ro.read_bytes(0, &mut changed).unwrap();
    assert_eq!(changed, sentinel);
    assert_ne!(changed.as_slice(), &full[..32]);
    println!(
        "CYBERMANJU_SIGNED32_RO_LEASE:alias:PASS bytes={} linear_rw_alias=true changed_readback=true immutable_data=false",
        changed.len()
    );

    println!(
        "CYBERMANJU_SIGNED32_RO_LEASE:faults:PASS cases={} prefix_pages={} reclaims={} acquired={} dropped={} live={}",
        fault_reclaims,
        fault_prefix,
        fault_reclaims,
        acquired_total,
        dropped_total,
        acquired_total - dropped_total
    );
    println!(
        "CYBERMANJU_SIGNED32_RO_LEASE:validation:PASS bounds={} output_preserved=true irq_preserved=true",
        bounds
    );
    let mut normal_reclaims = 0usize;
    drop(ro);
    assert_eq!(irq::is_local_enabled(), irq_before);
    absent(range_a);
    reclaim(base, ENVELOPE, 3);
    normal_reclaims += 1;
    let stage_b = stage(&other);
    let refs_b: Vec<Frame<()>> = stage_b.frames_for_signed32().collect();
    let frame_changes = refs_a
        .iter()
        .zip(&refs_b)
        .filter(|(a, b)| a.paddr() != b.paddr())
        .count();
    assert_eq!(frame_changes, 2);
    let owner_b = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap();
    let ro_b = stage_b.map_signed32_ro_nx(owner_b).unwrap();
    assert_eq!(ro_b.audit_for_test().unwrap().ro_nonx, 2);
    let mut bytes_b = vec![0; SPAN];
    ro_b.read_bytes(0, &mut bytes_b).unwrap();
    assert_eq!(bytes_b, other);
    drop(ro_b);
    reclaim(base, ENVELOPE, 3);
    normal_reclaims += 1;
    println!(
        "CYBERMANJU_SIGNED32_RO_LEASE:remap:PASS bytes={} frame_changes={} reclaims={} factory_ro=true",
        bytes_b.len(),
        frame_changes,
        normal_reclaims
    );
    drop(refs_a);
    drop(refs_b);
    println!(
        "CYBERMANJU_SIGNED32_RO_LEASE:complete:PASS protected_mappings={} total_reclaims={} module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false",
        normal_reclaims,
        fault_reclaims + normal_reclaims
    );
}

#[ktest]
fn cybermanju_signed32_ro_lease_smp_runtime() {
    println!("");
    assert_eq!(num_cpus(), 2);
    let base = NearTextAddressReservation::signed32_arena_range().start + 32 * PAGE_SIZE;
    let data = pattern_a();
    let enabled = irq::is_local_enabled();
    let mut acquired_total = 0;
    let mut dropped_total = 0;
    let mut reclaims = 0;
    println!("CYBERMANJU_SIGNED32_RO_LEASE_SMP:begin:PASS cpus=2");
    for disabled in [false, true] {
        let stats = Arc::new(FaultStats::new());
        let observed_stage = stage_observed(
            SPAN,
            &[ModuleChunk {
                offset: 0,
                bytes: &data,
            }],
            2,
            1,
            &stats,
        )
        .unwrap();
        let range = base..base + ENVELOPE;
        if disabled {
            let guard = irq::disable_local();
            assert!(matches!(
                observed_stage.map_signed32_ro_nx(
                    NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap()
                ),
                Err(Error::AccessDenied)
            ));
            assert!(!irq::is_local_enabled());
            drop(guard);
        } else {
            assert!(matches!(
                observed_stage.map_signed32_ro_nx(
                    NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap()
                ),
                Err(Error::AccessDenied)
            ));
            assert_eq!(irq::is_local_enabled(), enabled);
        }
        assert_eq!(irq::is_local_enabled(), enabled);
        absent(range);
        reclaim(base, ENVELOPE, 3);
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.acquired, 2);
        assert_eq!(snapshot.dropped, 2);
        acquired_total += snapshot.acquired;
        dropped_total += snapshot.dropped;
        reclaims += 1;
    }
    assert_eq!(reclaims, 2);
    assert_eq!(acquired_total, 4);
    assert_eq!(dropped_total, 4);
    println!(
        "CYBERMANJU_SIGNED32_RO_LEASE_SMP:denied:PASS attempts={} reclaims={} acquired={} dropped={} live={} irq_preserved=true",
        reclaims,
        reclaims,
        acquired_total,
        dropped_total,
        acquired_total - dropped_total
    );
    println!(
        "CYBERMANJU_SIGNED32_RO_LEASE_SMP:complete:PASS ro_mapping=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false"
    );
}
