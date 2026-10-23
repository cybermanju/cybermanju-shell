// SPDX-License-Identifier: MPL-2.0
//! Hardware write denial of a data alias, not Linux module execution or W^X.
use super::KERNEL_PAGE_TABLE;
use crate::{
    cpu::num_cpus,
    irq,
    mm::{
        Frame, PAGE_SIZE, Vaddr, VmIo,
        fault::signed32_write_probe::{probe_store_byte, verify_context_rejections_for_test},
        module_address::NearTextAddressReservation,
        module_staging::{
            ModuleChunk,
            fault_injection::{FaultStats, stage_observed},
        },
    },
    prelude::{ktest, println},
};
use alloc::{sync::Arc, vec, vec::Vec};
use core::ops::Range;

const SPAN: usize = 3 * PAGE_SIZE;
const ENVELOPE: usize = 4 * PAGE_SIZE;

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
            "write-fault lease PTE must be absent"
        );
    }
}

// SAFETY: Caller retains the mapping covering this byte throughout the call.
unsafe fn successful_store(address: usize, byte: u8) {
    let before = irq::is_local_enabled();
    // SAFETY: Inherited mapped-byte lifetime; this is a disposable data fixture.
    let observation = unsafe { probe_store_byte(address, byte) }.unwrap();
    assert!(!observation.faulted, "RW probe must not fault");
    assert_eq!(observation.trap_count, 0);
    assert_eq!(observation.error_code, 0);
    assert_eq!(observation.fault_addr, 0);
    assert!(
        observation.write_protect_enabled,
        "CR0.WP must already be enabled"
    );
    assert_eq!(irq::is_local_enabled(), before);
}

#[ktest]
fn cybermanju_signed32_write_fault_runtime() {
    println!("");
    assert_eq!(num_cpus(), 1);
    println!("CYBERMANJU_SIGNED32_WRITE_FAULT:begin:PASS cpus=1");
    let rejections = verify_context_rejections_for_test();
    assert!(rejections >= 10);
    println!(
        "CYBERMANJU_SIGNED32_WRITE_FAULT:guard:PASS rejected={} production_hook=false exact_site=true exact_cr2=true exact_error=true supervisor_only=true single_cpu=true",
        rejections
    );
    let entry_irq = irq::is_local_enabled();
    assert!(
        entry_irq,
        "write-fault profile must start with IRQs enabled"
    );
    let base = NearTextAddressReservation::signed32_arena_range().start + 48 * PAGE_SIZE;
    let mut expected: Vec<u8> = (0..SPAN).map(|i| ((i * 23 + 7) % 251) as u8).collect();
    let stats = Arc::new(FaultStats::new());
    let staged = stage_observed(
        SPAN,
        &[ModuleChunk {
            offset: 0,
            bytes: &expected,
        }],
        3,
        1,
        &stats,
    )
    .unwrap();
    let refs: Vec<Frame<()>> = staged.frames_for_signed32().collect();
    let reservation = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap();
    let range = reservation.range();
    let rw = staged.map_signed32_rw_nx(reservation).unwrap();
    for page in 0..3 {
        let offset = page * PAGE_SIZE;
        let byte = 0xa1 + page as u8;
        // SAFETY: `rw` owns all three pages and remains live.
        unsafe { successful_store(base + offset, byte) };
        expected[offset] = byte;
    }
    let mut readback = vec![0; SPAN];
    rw.read_bytes(0, &mut readback).unwrap();
    assert_eq!(
        readback, expected,
        "RW probe must perform the real byte stores"
    );
    println!(
        "CYBERMANJU_SIGNED32_WRITE_FAULT:rw_before:PASS stores=3 traps=0 wp_enabled=true irq_preserved=true"
    );
    let mapping = rw.into_selective_nx(&[true, false, true]).unwrap();
    let audit = mapping.audit_for_test().unwrap();
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
    println!(
        "CYBERMANJU_SIGNED32_WRITE_FAULT:sealed:PASS pte_pages=3 frame_matches=3 ro_nonx=2 rw_nonx=1 supervisor=3 writeback=3 global=3 slack_absent=1"
    );
    let mut fault_count = 0;
    for (i, offset) in [0, PAGE_SIZE - 1, 2 * PAGE_SIZE, SPAN - 1]
        .into_iter()
        .enumerate()
    {
        let disabled = if i % 2 == 1 {
            Some(irq::disable_local())
        } else {
            None
        };
        let before = irq::is_local_enabled();
        // SAFETY: `mapping` owns the byte; the attempted change is rejected by
        // its RO supervisor PTE while CR0.WP remains enabled. Never an exec probe.
        let observation =
            unsafe { probe_store_byte(base + offset, expected[offset] ^ 0xff) }.unwrap();
        assert!(
            observation.faulted,
            "RO alias byte store must cause a hardware write fault"
        );
        assert_eq!(
            observation.trap_count, 1,
            "exactly one page fault must recover"
        );
        assert_eq!(observation.error_code, 3);
        assert_eq!(observation.fault_addr, base + offset);
        assert!(observation.write_protect_enabled);
        assert_eq!(irq::is_local_enabled(), before);
        let mut actual = [0];
        mapping.read_bytes(offset, &mut actual).unwrap();
        assert_eq!(
            actual[0], expected[offset],
            "faulting byte store must leave data unchanged"
        );
        println!(
            "CYBERMANJU_SIGNED32_WRITE_FAULT:fault:PASS offset={} traps={} error_code={} cr2_matches=true unchanged=true wp_enabled=true caller_irq_disabled={} irq_preserved=true",
            offset, observation.trap_count, observation.error_code, !before
        );
        drop(disabled);
        assert_eq!(irq::is_local_enabled(), entry_irq);
        fault_count += observation.trap_count;
    }
    for (i, offset) in [PAGE_SIZE, 2 * PAGE_SIZE - 1].into_iter().enumerate() {
        let disabled = if i == 1 {
            Some(irq::disable_local())
        } else {
            None
        };
        let byte = 0xb1 + i as u8;
        // SAFETY: The middle page remains RW; mapping owns it throughout.
        unsafe { successful_store(base + offset, byte) };
        expected[offset] = byte;
        drop(disabled);
        assert_eq!(irq::is_local_enabled(), entry_irq);
    }
    mapping.read_bytes(0, &mut readback).unwrap();
    assert_eq!(
        readback, expected,
        "RW page must stay writable after selective sealing"
    );
    println!(
        "CYBERMANJU_SIGNED32_WRITE_FAULT:rw_after:PASS stores=2 traps=0 wp_enabled=true irq_preserved=true"
    );
    // This existing RW linear alias deliberately demonstrates why this is not
    // immutable data, physical W^X support, or sufficient permission to add RX.
    refs[0].write_bytes(0, &[0xc9]).unwrap();
    expected[0] = 0xc9;
    mapping.read_bytes(0, &mut readback).unwrap();
    assert_eq!(readback, expected);
    println!(
        "CYBERMANJU_SIGNED32_WRITE_FAULT:linear:PASS bytes=1 changed_readback=true linear_rw_alias=true immutable_data=false"
    );
    assert_eq!(
        (stats.snapshot().acquired, stats.snapshot().dropped),
        (3, 0)
    );
    drop(refs);
    drop(mapping);
    assert_eq!(irq::is_local_enabled(), entry_irq);
    assert_eq!(
        (stats.snapshot().acquired, stats.snapshot().dropped),
        (3, 3)
    );
    absent(range);
    let reclaimed = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap();
    absent(reclaimed.range());
    drop(reclaimed);
    println!(
        "CYBERMANJU_SIGNED32_WRITE_FAULT:complete:PASS faults={} rw_stores=5 bytes={} acquired=3 dropped=3 live=0 reclaims=1 pte_absent=4 irq_preserved=true write_fault_probed=true recovery_ktest_only=true physical_wx=false rx_enabled=false elf_execution=false smp_mapping=false production_qualified=false",
        fault_count,
        readback.len()
    );
}
