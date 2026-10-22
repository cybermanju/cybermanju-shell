// SPDX-License-Identifier: MPL-2.0
//! Runtime proofs for persistent signed-32 RW/NX mapping leases.

use alloc::{vec, vec::Vec};
use core::ops::Range;

use super::{KERNEL_PAGE_TABLE, MappedItem, MappedItemRef};
use crate::{
    Error,
    arch::mm::tlb_flush_addr_range,
    cpu::num_cpus,
    irq::{self, DisabledLocalIrqGuard},
    mm::{
        Frame, FrameAllocOptions, HasPaddr, PAGE_SIZE, Vaddr, VmIo,
        module_address::NearTextAddressReservation,
        module_staging::{ModuleChunk, ModuleStaging},
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags},
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
            "lease rollback must leave PTE absent"
        );
    }
}
fn assert_reclaim(base: Vaddr, span: usize, pages: usize) {
    let recovered = NearTextAddressReservation::new_signed32_at(base, span, pages)
        .expect("lease rollback must release reservation");
    absent(recovered.range());
    drop(recovered);
}
struct ForeignGuard {
    irq_guard: DisabledLocalIrqGuard,
    range: Range<Vaddr>,
    installed: bool,
    _frame: Frame<()>,
}
impl Drop for ForeignGuard {
    fn drop(&mut self) {
        if !self.installed {
            return;
        }
        let table = KERNEL_PAGE_TABLE
            .get()
            .expect("kernel page table initialized");
        {
            let mut cursor = table
                .cursor_mut(&self.irq_guard, &self.range)
                .expect("foreign range valid");
            cursor.jump(self.range.start).expect("foreign slot aligned");
            if cursor.query().expect("foreign PTE query").1.is_some() {
                // SAFETY: fixture owns this tracked entry and retains its frame through GLOBAL flush.
                drop(unsafe { cursor.take_next(PAGE_SIZE) }.expect("foreign PTE exists"));
            }
        }
        tlb_flush_addr_range(&self.range);
        self.installed = false;
    }
}

#[ktest]
fn cybermanju_signed32_lease_runtime() {
    println!("");
    assert_eq!(num_cpus(), 1, "lease runtime requires one CPU");
    let arena = NearTextAddressReservation::signed32_arena_range();
    let base = arena.start + 8 * PAGE_SIZE;
    let full = pattern_a();
    let other = pattern_b();
    println!("CYBERMANJU_SIGNED32_LEASE:begin:PASS cpus=1");

    // Inject faults before any success marker; every failed prefix must roll back fully.
    let mut fault_prefix = 0usize;
    let mut fault_reclaims = 0usize;
    for after in 0..3 {
        let owner = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap();
        let range = owner.range();
        let mut observed = usize::MAX;
        let result =
            stage(&full).map_signed32_rw_nx_with_fault_for_test(owner, after, &mut observed);
        assert!(
            matches!(result, Err(Error::IoError)),
            "lease fault must report IoError"
        );
        assert_eq!(observed, after, "lease fault must report installed prefix");
        fault_prefix += observed;
        assert_reclaim(base, ENVELOPE, 3);
        absent(range);
        fault_reclaims += 1;
    }

    // Bad span joins consume and return both owners.
    let mut validation_reclaims = 0;
    let mut invalid_joins = 0;
    let owner = NearTextAddressReservation::new_signed32_at(base, PAGE_SIZE, 1).unwrap();
    let r = owner.range();
    assert!(matches!(
        stage(&full).map_signed32_rw_nx(owner),
        Err(Error::InvalidArgs)
    ));
    assert_reclaim(base, PAGE_SIZE, 1);
    validation_reclaims += 1;
    invalid_joins += 1;
    absent(r);
    let near_base = NearTextAddressReservation::arena_range().start;
    let owner = NearTextAddressReservation::new_at(near_base, 3 * PAGE_SIZE, 3).unwrap();
    let near_range = owner.range();
    assert!(matches!(
        stage(&full).map_signed32_rw_nx(owner),
        Err(Error::InvalidArgs)
    ));
    let recovered = NearTextAddressReservation::new_at(near_base, 3 * PAGE_SIZE, 3).unwrap();
    assert_eq!(recovered.range(), near_range);
    drop(recovered);
    validation_reclaims += 1;
    invalid_joins += 1;

    // Pre-existing tracked foreign PTE remains untouched on the full-envelope preflight.
    let collision_owner = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap();
    let collision_range = collision_owner.range();
    let slot = collision_range.start + PAGE_SIZE..collision_range.start + 2 * PAGE_SIZE;
    let foreign = FrameAllocOptions::new().zeroed(true).alloc_frame().unwrap();
    let sentinel = [0xA7; 32];
    foreign.write_bytes(0, &sentinel).unwrap();
    let foreign_prop = PageProperty {
        flags: PageFlags::RW,
        cache: CachePolicy::Writeback,
        priv_flags: PrivilegedPageFlags::GLOBAL,
    };
    let mut foreign_guard = ForeignGuard {
        irq_guard: irq::disable_local(),
        range: slot.clone(),
        installed: false,
        _frame: foreign.clone(),
    };
    {
        let mut cursor = KERNEL_PAGE_TABLE
            .get()
            .unwrap()
            .cursor_mut(&foreign_guard.irq_guard, &slot)
            .unwrap();
        assert!(cursor.query().unwrap().1.is_none());
        foreign_guard.installed = true;
        // SAFETY: this test owns the empty slot under its IRQ guard; tracked frame retained.
        unsafe {
            cursor.map(MappedItem::Tracked(
                Frame::from_unsized(foreign.clone()),
                foreign_prop,
            ));
        }
    }
    let attempt = stage(&full).map_signed32_rw_nx(collision_owner);
    assert!(
        matches!(attempt, Err(Error::AccessDenied)),
        "lease collision must preserve foreign PTE"
    );
    {
        let guard = irq::disable_local();
        let mut cursor = KERNEL_PAGE_TABLE
            .get()
            .unwrap()
            .cursor(&guard, &collision_range)
            .unwrap();
        cursor.jump(slot.start).unwrap();
        let Some(MappedItemRef::Tracked(actual, prop)) = cursor.query().unwrap().1 else {
            panic!("foreign tracked PTE was lost")
        };
        let mut expected_prop = foreign_prop;
        expected_prop.priv_flags |= PrivilegedPageFlags::AVAIL1;
        assert_eq!(actual.paddr(), foreign.paddr());
        assert_eq!(prop, expected_prop);
        let mut bytes = [0; 32];
        foreign.read_bytes(0, &mut bytes).unwrap();
        assert_eq!(bytes, sentinel);
        for va in collision_range.clone().step_by(PAGE_SIZE) {
            if va == slot.start {
                continue;
            }
            cursor.jump(va).unwrap();
            assert!(cursor.query().unwrap().1.is_none());
        }
    }
    drop(foreign_guard); // flushes GLOBAL translation before the retained frame drops
    absent(collision_range);
    drop(foreign);
    assert_reclaim(base, ENVELOPE, 3);
    validation_reclaims += 1;

    // Hold A's independent frame references while the lease is dropped and B is staged.
    let irq_before = irq::is_local_enabled();
    let stage_a = stage(&full);
    let refs_a: Vec<Frame<()>> = stage_a.frames_for_signed32().collect();
    let owner_a = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap();
    let range = owner_a.range();
    let mapping_a = stage_a.map_signed32_rw_nx(owner_a).unwrap();
    assert_eq!(irq::is_local_enabled(), irq_before);
    let audit = mapping_a.audit_for_test().unwrap();
    assert_eq!(audit.pte_pages, 2);
    assert_eq!(audit.frame_matches, 2);
    assert_eq!(audit.rw_nonx, 2);
    assert_eq!(audit.supervisor, 2);
    assert_eq!(audit.writeback, 2);
    assert_eq!(audit.global, 2);
    assert_eq!(audit.slack_absent, 1);
    assert!(
        matches!(
            NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3),
            Err(Error::NoMemory)
        ),
        "lease must be exclusive while alive"
    );
    let mut complete = vec![0; SPAN];
    mapping_a.read_bytes(0, &mut complete).unwrap();
    assert_eq!(complete, full, "lease persistent bytes must match");
    let mut crossing = vec![0; 20];
    mapping_a.read_bytes(PAGE_SIZE - 10, &mut crossing).unwrap();
    assert_eq!(
        crossing,
        full[PAGE_SIZE - 10..PAGE_SIZE + 10],
        "lease persistent bytes must match"
    );
    mapping_a.read_bytes(SPAN, &mut []).unwrap();
    let mut bounds = 0;
    for (offset, len, expected_error) in [
        (SPAN, 1, Error::InvalidArgs),
        (SPAN - 1, 2, Error::InvalidArgs),
        (usize::MAX, 2, Error::Overflow),
    ] {
        let mut destination = [0xc7; 2];
        assert_eq!(
            mapping_a.read_bytes(offset, &mut destination[..len]),
            Err(expected_error)
        );
        assert_eq!(
            destination, [0xc7; 2],
            "lease invalid reads must preserve output"
        );
        bounds += 1;
    }
    let irq_preserved = irq::is_local_enabled() == irq_before;
    assert!(irq_preserved);
    println!(
        "CYBERMANJU_SIGNED32_LEASE:persistent:PASS span={} pages={} envelope_pages=3 bytes={} reads=3 pte_pages={} frame_matches={} rw_nonx={} supervisor={} writeback={} global={} slack_absent={} irq_preserved=true exclusive=true",
        SPAN,
        mapping_a.page_count(),
        SPAN + 20,
        audit.pte_pages,
        audit.frame_matches,
        audit.rw_nonx,
        audit.supervisor,
        audit.writeback,
        audit.global,
        audit.slack_absent
    );
    println!(
        "CYBERMANJU_SIGNED32_LEASE:faults:PASS cases={} prefix_pages={} reclaims={}",
        fault_reclaims, fault_prefix, fault_reclaims
    );
    println!(
        "CYBERMANJU_SIGNED32_LEASE:validation:PASS bounds=3 invalid_joins={} collision=1 reclaims={} foreign_preserved=true irq_preserved=true",
        invalid_joins, validation_reclaims
    );
    let mut normal_reclaims = 0;
    drop(mapping_a);
    assert_eq!(irq::is_local_enabled(), irq_before);
    absent(range);
    assert_reclaim(base, ENVELOPE, 3);
    normal_reclaims += 1;

    let stage_b = stage(&other);
    let refs_b: Vec<Frame<()>> = stage_b.frames_for_signed32().collect();
    let frame_changes = refs_a
        .iter()
        .zip(&refs_b)
        .filter(|(a, b)| a.paddr() != b.paddr())
        .count();
    assert_eq!(
        frame_changes, 2,
        "lease remap must use different retained frames"
    );
    let owner_b = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 3).unwrap();
    let mapping_b = stage_b.map_signed32_rw_nx(owner_b).unwrap();
    let mut bytes_b = vec![0; SPAN];
    mapping_b.read_bytes(0, &mut bytes_b).unwrap();
    assert_eq!(bytes_b, other, "lease remap bytes must match");
    let audit_b = mapping_b.audit_for_test().unwrap();
    assert_eq!(audit_b.pte_pages, 2);
    assert_eq!(audit_b.frame_matches, 2);
    drop(mapping_b);
    assert_eq!(irq::is_local_enabled(), irq_before);
    assert_reclaim(base, ENVELOPE, 3);
    normal_reclaims += 1;
    println!(
        "CYBERMANJU_SIGNED32_LEASE:remap:PASS bytes={} frame_changes={} reclaims={} irq_preserved=true",
        SPAN, frame_changes, normal_reclaims
    );
    drop(refs_a);
    drop(refs_b);
    println!(
        "CYBERMANJU_SIGNED32_LEASE:complete:PASS persistent_mappings={} reclaims={} module_execution=false physical_wx=false smp_mapping=false",
        normal_reclaims,
        fault_reclaims + validation_reclaims + normal_reclaims
    );
}

#[ktest]
fn cybermanju_signed32_lease_smp_runtime() {
    println!("");
    assert_eq!(num_cpus(), 2, "lease SMP test requires two CPUs");
    let arena = NearTextAddressReservation::signed32_arena_range();
    let base = arena.start + 16 * PAGE_SIZE;
    let range = base..base + 2 * PAGE_SIZE;
    let enabled = irq::is_local_enabled();
    println!("CYBERMANJU_SIGNED32_LEASE_SMP:begin:PASS cpus=2");
    let mut attempts = 0;
    let mut reclaims = 0;
    for disabled in [false, true] {
        let owner = NearTextAddressReservation::new_signed32_at(base, 2 * PAGE_SIZE, 2).unwrap();
        let mut owner = Some(owner);
        let result = if disabled {
            let guard = irq::disable_local();
            let r = stage(&pattern_a()).map_signed32_rw_nx(owner.take().unwrap());
            assert_eq!(
                irq::is_local_enabled(),
                enabled && false,
                "lease SMP must stay denied"
            );
            drop(guard);
            assert_eq!(irq::is_local_enabled(), enabled);
            r
        } else {
            let r = stage(&pattern_a()).map_signed32_rw_nx(owner.take().unwrap());
            assert_eq!(irq::is_local_enabled(), enabled);
            r
        };
        assert!(
            matches!(result, Err(Error::AccessDenied)),
            "lease SMP must stay denied"
        );
        assert_reclaim(base, 2 * PAGE_SIZE, 2);
        absent(range.clone());
        attempts += 1;
        reclaims += 1;
    }
    println!(
        "CYBERMANJU_SIGNED32_LEASE_SMP:denied:PASS attempts={} reclaims={} irq_preserved=true",
        attempts, reclaims
    );
    println!(
        "CYBERMANJU_SIGNED32_LEASE_SMP:complete:PASS persistent_mapping=false module_execution=false physical_wx=false smp_mapping=false"
    );
}
