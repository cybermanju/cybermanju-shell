// SPDX-License-Identifier: MPL-2.0
//! Runtime validation of metadata-only ELF page permission plans.
use alloc::{vec, vec::Vec};

use linux_module_elf::{
    Error as ElfError, IntendedPagePermission as Permission, plan_layout, plan_page_permissions,
};
use ostd::{cpu, irq, mm::PAGE_SIZE, prelude::*};

use super::{MAX_SECTIONS, MAX_SPAN, PreparationLimits, c_shim_import::imported};
use crate::PolicyNearTextModuleLayout;

fn limits(pages: usize) -> PreparationLimits {
    PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages: pages,
        max_chunks: MAX_SECTIONS,
    }
}

fn fixture(kind: u32) -> Vec<u8> {
    imported(kind, b"memcmp").0
}

// A minimal ELF64 relocatable with three one-byte allocated NOBITS sections.
// Compact layout intentionally shares one page to exercise permission conflicts.
fn tiny_elf(flags: [u64; 3]) -> Vec<u8> {
    let names = b"\0.shstrtab\0.ro\0.a\0.b\0";
    let table = 64usize;
    let names_at = table + 5 * 64;
    let mut bytes = vec![0u8; names_at + names.len()];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[6] = 1;
    bytes[16..18].copy_from_slice(&1u16.to_le_bytes()); // ET_REL
    bytes[18..20].copy_from_slice(&62u16.to_le_bytes()); // x86-64
    bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
    bytes[40..48].copy_from_slice(&(table as u64).to_le_bytes());
    bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
    bytes[58..60].copy_from_slice(&64u16.to_le_bytes());
    bytes[60..62].copy_from_slice(&5u16.to_le_bytes());
    bytes[62..64].copy_from_slice(&1u16.to_le_bytes());
    let put = |b: &mut [u8], idx: usize, name: u32, typ: u32, flag: u64, size: u64| {
        let o = table + idx * 64;
        b[o..o + 4].copy_from_slice(&name.to_le_bytes());
        b[o + 4..o + 8].copy_from_slice(&typ.to_le_bytes());
        b[o + 8..o + 16].copy_from_slice(&flag.to_le_bytes());
        b[o + 32..o + 40].copy_from_slice(&size.to_le_bytes());
        b[o + 48..o + 56].copy_from_slice(&1u64.to_le_bytes());
    };
    put(&mut bytes, 1, 1, 3, 0, names.len() as u64); // SHT_STRTAB
    bytes[table + 1 * 64 + 24..table + 1 * 64 + 32]
        .copy_from_slice(&(names_at as u64).to_le_bytes());
    for i in 0..3 {
        put(&mut bytes, i + 2, [11, 15, 18][i], 8, flags[i], 1); // SHT_NOBITS
    }
    bytes[names_at..].copy_from_slice(names);
    bytes
}

fn independently_classify(layout: &linux_module_elf::Layout, pages: usize) -> Vec<Permission> {
    let mut expected = vec![Permission::Inaccessible; pages];
    for section in layout.placements() {
        if section.size == 0 {
            continue;
        }
        let first = usize::try_from(section.address - layout.base()).unwrap() / PAGE_SIZE;
        let last_byte = usize::try_from(section.address - layout.base())
            .unwrap()
            .checked_add(usize::try_from(section.size).unwrap())
            .unwrap()
            - 1;
        let last = last_byte / PAGE_SIZE;
        assert!(last < pages);
        let flags = section.flags;
        let perm = if flags & 1 != 0 && flags & 4 != 0 {
            panic!("fixture contains a writable-executable section")
        } else if flags & 1 != 0 {
            Permission::ReadWrite
        } else if flags & 4 != 0 {
            Permission::ReadExecute
        } else {
            Permission::ReadOnly
        };
        for page in first..=last {
            assert_eq!(
                expected[page],
                Permission::Inaccessible,
                "canonical layout page overlap"
            );
            expected[page] = perm;
        }
    }
    expected
}

#[ktest]
fn cybermanju_permission_plan_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 1);
    let kinds = [1u32, 2, 4, 11];
    println!("CYBERMANJU_PERMISSION_PLAN:begin:PASS cpus=1 kinds=4");
    let arena = ostd::mm::module_address::NearTextAddressReservation::signed32_arena_range();
    let base = arena.end - 64 * PAGE_SIZE;
    let mut budgets_denied = 0usize;
    let mut conflicts_denied = 0usize;
    let mut reclaims = 0usize;
    let mut cases = 0usize;
    for kind in kinds {
        let bytes = fixture(kind);
        let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let staged = owner
            .prepare_and_stage(&bytes, limits(MAX_SPAN as usize / PAGE_SIZE))
            .unwrap();
        let layout = staged.layout();
        let pages_bound = usize::try_from(layout.span()).unwrap() / PAGE_SIZE;
        let base_u64 = layout.base();
        let planned = staged.planned_permissions(pages_bound).unwrap();
        let page_count = planned.page_count();
        assert!(page_count > 0);
        let span = usize::try_from(planned.span()).unwrap();
        assert_eq!(planned.base(), base_u64);
        assert_eq!(span, usize::try_from(layout.span()).unwrap());
        assert_eq!(span % PAGE_SIZE, 0);
        assert_eq!(page_count, span / PAGE_SIZE);
        assert!(
            matches!(
                staged.planned_permissions(page_count - 1),
                Err(ElfError::ResourceLimit)
            ),
            "permission budget must be denied"
        );
        budgets_denied += 1;
        let expected = independently_classify(layout, page_count);
        assert_eq!(
            expected.as_slice(),
            planned.pages(),
            "permission plan must match independent section oracle"
        );
        let mut ro = 0;
        let mut rw = 0;
        let mut rx = 0;
        let mut gap = 0;
        for p in planned.pages() {
            match p {
                Permission::Inaccessible => gap += 1,
                Permission::ReadOnly => ro += 1,
                Permission::ReadWrite => rw += 1,
                Permission::ReadExecute => rx += 1,
            }
        }
        assert!(ro > 0 && rw > 0 && rx > 0);
        assert_eq!(ro + rw + rx + gap, page_count);
        let mut expected_bytes = vec![0; span];
        let irq_before = irq::is_local_enabled();
        staged.read_bytes(0, &mut expected_bytes).unwrap();
        let mapped = staged
            .into_signed32_rw_nx()
            .expect("signed32 RW/NX handoff");
        assert_eq!(irq::is_local_enabled(), irq_before);
        assert_eq!(
            mapped.permission_plan(),
            &planned,
            "permission plan must survive handoff"
        );
        let audit = mapped.audit_for_test().unwrap();
        assert_eq!(audit.pte_pages, page_count);
        assert_eq!(audit.frame_matches, page_count);
        assert_eq!(audit.supervisor, page_count);
        assert_eq!(audit.writeback, page_count);
        assert_eq!(audit.global, page_count);
        assert_eq!(
            audit.rw_nonx, page_count,
            "permission metadata must not apply RX"
        );
        let mut actual = vec![0; span];
        mapped.read_bytes(0, &mut actual).unwrap();
        assert_eq!(
            actual, expected_bytes,
            "read-only plan metadata must not mutate or apply permissions to module bytes"
        );
        let envelope = mapped.reserved_range().len();
        let range = mapped.reserved_range();
        let retained = matches!(
            ostd::mm::module_address::NearTextAddressReservation::new_signed32_at(
                range.start,
                envelope,
                envelope / PAGE_SIZE
            ),
            Err(ostd::Error::NoMemory)
        );
        assert!(
            retained,
            "permission-plan lease must retain its exact reservation"
        );
        drop(mapped);
        assert_eq!(irq::is_local_enabled(), irq_before);
        let reclaimed = ostd::mm::module_address::NearTextAddressReservation::new_signed32_at(
            range.start,
            envelope,
            envelope / PAGE_SIZE,
        )
        .unwrap();
        assert_eq!(reclaimed.range(), range);
        drop(reclaimed);
        reclaims += 1;
        println!(
            "CYBERMANJU_PERMISSION_PLAN:case:PASS kind={} span={} pages={} ro={} rw={} rx={} gap={} actual_nonx={} bytes={} retained=true reclaimed=true",
            kind,
            span,
            page_count,
            ro,
            rw,
            rx,
            gap,
            audit.rw_nonx,
            actual.len()
        );
        cases += 1;
    }
    for flags in [[2 | 4, 2 | 1, 2], [2, 2 | 1, 2], [2, 2 | 4, 2]] {
        let bytes = tiny_elf(flags);
        let layout = plan_layout(&bytes, 0x4000_0000, MAX_SPAN, MAX_SECTIONS).unwrap();
        let result = plan_page_permissions(&layout, 1);
        if flags[0] == 6 {
            assert!(
                matches!(result, Err(ElfError::WritableExecutablePage)),
                "permission plan must reject mixed RX/RW pages"
            );
        } else {
            assert!(
                matches!(result, Err(ElfError::PermissionPageConflict)),
                "permission plan must reject distinct permission classes"
            );
        }
        conflicts_denied += 1;
    }
    assert_eq!(
        (cases, conflicts_denied, budgets_denied, reclaims),
        (4, 3, 4, 4)
    );
    println!(
        "CYBERMANJU_PERMISSION_PLAN:complete:PASS cases={} conflicts_denied={} budgets_denied={} reclaims={} metadata_only=true rx_applied=false module_execution=false physical_wx=false smp_mapping=false",
        cases, conflicts_denied, budgets_denied, reclaims
    );
}
