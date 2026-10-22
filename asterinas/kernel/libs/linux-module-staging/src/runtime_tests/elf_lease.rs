// SPDX-License-Identifier: MPL-2.0
//! Persistent signed-32 lease coverage; mapped modules are never executed.
use alloc::{vec, vec::Vec};

use linux_module_elf::{page_layout_span_bound, plan_page_layout};
use ostd::{
    cpu,
    mm::{PAGE_SIZE, module_address::NearTextAddressReservation},
    prelude::*,
};

use super::{MAX_SECTIONS, MAX_SPAN, PreparationLimits, c_shim_import::imported};
use crate::{CShimExportRegistry, PolicyNearTextModuleLayout};

fn limits(pages: usize, chunks: usize) -> PreparationLimits {
    PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages: pages,
        max_chunks: chunks,
    }
}
fn fixture(kind: u32) -> (Vec<u8>, u16, u64) {
    imported(kind, b"memcmp")
}
fn irq_enabled() -> bool {
    ostd::irq::is_local_enabled()
}

fn recover(base: usize, envelope: usize, pages: usize) {
    let owner = NearTextAddressReservation::new_signed32_at(base, envelope, pages).unwrap();
    assert_eq!(owner.range(), base..base + envelope);
    drop(owner);
}

#[ktest]
fn cybermanju_elf_lease_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 1);
    let kinds = [1u32, 2, 4, 11];
    println!("CYBERMANJU_ELF_LEASE:begin:PASS cpus=1 kinds=4");
    let base = NearTextAddressReservation::signed32_arena_range().end - 64 * PAGE_SIZE;
    let sample = fixture(1).0;
    let envelope = page_layout_span_bound(&sample, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let pages = envelope / PAGE_SIZE;
    let mut faults = 0usize;
    let mut prefix_pages = 0usize;
    let mut reclaims = 0usize;
    for after in 0..3 {
        let bytes = fixture(1).0;
        let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let staged = owner
            .prepare_and_stage(&bytes, limits(pages, MAX_SECTIONS))
            .unwrap();
        let range = staged.reserved_range();
        let mut observed = 0usize;
        assert!(
            matches!(
                staged.into_signed32_rw_nx_with_fault_for_test(after, &mut observed),
                Err(ostd::Error::IoError)
            ),
            "ELF lease mapping fault must roll back"
        );
        assert_eq!(observed, after);
        prefix_pages += observed;
        faults += 1;
        recover(range.start, envelope, pages);
        reclaims += 1;
    }

    let mut mappings = 0usize;
    let mut bounds_rejected = 0usize;
    let mut completed_reclaims = 0usize;
    for kind in kinds {
        let (bytes, target, offset) = fixture(kind);
        let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let layout = owner.layout();
        let base_u64 = layout.base();
        let span = layout.span() as usize;
        let page_count = span / PAGE_SIZE;
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

        let staged = owner
            .prepare_and_stage(&bytes, limits(pages, MAX_SECTIONS))
            .unwrap();
        let range = staged.reserved_range();
        assert_eq!(range, base..base + envelope);
        assert_eq!((staged.span(), staged.page_count()), (span, page_count));
        assert_eq!(staged.layout().base(), base_u64);
        assert_eq!(staged.layout().span() as usize, span);
        assert_eq!(
            staged
                .layout()
                .placements()
                .iter()
                .map(|p| (p.section_index, p.address, p.size))
                .collect::<Vec<_>>(),
            placements
        );
        let irq_before = irq_enabled();
        let mapped = staged.into_signed32_rw_nx().expect("ELF lease handoff");
        assert_eq!(irq_enabled(), irq_before);
        assert_eq!(
            mapped.layout().base(),
            base_u64,
            "ELF lease layout must survive handoff"
        );
        assert_eq!(mapped.layout().span() as usize, span);
        assert_eq!(
            mapped
                .layout()
                .placements()
                .iter()
                .map(|p| (p.section_index, p.address, p.size))
                .collect::<Vec<_>>(),
            placements,
            "ELF lease layout must survive handoff"
        );
        assert_eq!(mapped.reserved_range(), range);
        assert_eq!((mapped.span(), mapped.page_count()), (span, page_count));
        let audit = mapped.audit_for_test().unwrap();
        let slack = (envelope - span) / PAGE_SIZE;
        assert_eq!(audit.pte_pages, page_count);
        assert_eq!(audit.frame_matches, page_count);
        assert_eq!(audit.rw_nonx, page_count);
        assert_eq!(audit.supervisor, page_count);
        assert_eq!(audit.writeback, page_count);
        assert_eq!(audit.global, page_count);
        assert_eq!(audit.slack_absent, slack);
        let mut actual = vec![0xff; span];
        mapped.read_bytes(0, &mut actual).unwrap();
        assert_eq!(actual, expected, "ELF lease mapped bytes must match");
        let mut field_actual = vec![0xff; oracle.len()];
        mapped.read_bytes(field, &mut field_actual).unwrap();
        assert_eq!(
            field_actual, oracle,
            "ELF lease relocation field must match"
        );
        let mut sentinel = [0xca; 1];
        assert!(matches!(
            mapped.read_bytes(span, &mut sentinel),
            Err(ostd::Error::InvalidArgs)
        ));
        assert_eq!(sentinel, [0xca; 1]);
        let mut sentinel = [0xca; 2];
        assert!(matches!(
            mapped.read_bytes(usize::MAX, &mut sentinel),
            Err(ostd::Error::Overflow)
        ));
        assert_eq!(sentinel, [0xca; 2]);
        bounds_rejected += 2;
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(base, envelope, pages),
            Err(ostd::Error::NoMemory)
        ));
        drop(mapped);
        assert_eq!(irq_enabled(), irq_before);
        recover(range.start, envelope, pages);
        completed_reclaims += 1;
        println!(
            "CYBERMANJU_ELF_LEASE:case:PASS kind={} span={} envelope={} pages={} bytes={} relocation_bytes={} frame_matches={} nonx_pages={} slack_absent={} retained=true layout_preserved=true reclaimed=true",
            kind,
            span,
            envelope,
            page_count,
            actual.len() + field_actual.len(),
            oracle.len(),
            audit.frame_matches,
            audit.rw_nonx,
            audit.slack_absent
        );
        mappings += 1;
    }
    println!(
        "CYBERMANJU_ELF_LEASE:faults:PASS cases={} prefix_pages={} reclaims={}",
        faults, prefix_pages, reclaims
    );
    println!(
        "CYBERMANJU_ELF_LEASE:complete:PASS mappings={} bounds_rejected={} reclaims={} elf_handoff=true module_execution=false physical_wx=false smp_mapping=false",
        mappings,
        bounds_rejected,
        reclaims + completed_reclaims
    );
}

#[ktest]
fn cybermanju_elf_lease_smp_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 2);
    let kinds = [1u32, 2, 4, 11];
    println!("CYBERMANJU_ELF_LEASE_SMP:begin:PASS cpus=2 kinds=4");
    let base = NearTextAddressReservation::signed32_arena_range().end - 64 * PAGE_SIZE;
    let sample = fixture(1).0;
    let envelope = page_layout_span_bound(&sample, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let pages = envelope / PAGE_SIZE;
    let mut attempts = 0usize;
    let mut reclaims = 0usize;
    for kind in kinds {
        let bytes = fixture(kind).0;
        let owner = PolicyNearTextModuleLayout::new_signed32_c_shims_at(
            &bytes,
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            65_536,
        )
        .unwrap();
        let staged = owner
            .prepare_and_stage(&bytes, limits(pages, MAX_SECTIONS))
            .unwrap();
        let range = staged.reserved_range();
        assert!(
            matches!(staged.into_signed32_rw_nx(), Err(ostd::Error::AccessDenied)),
            "ELF lease SMP handoff must stay denied"
        );
        recover(range.start, envelope, pages);
        attempts += 1;
        reclaims += 1;
    }
    println!(
        "CYBERMANJU_ELF_LEASE_SMP:denied:PASS attempts={} reclaims={}",
        attempts, reclaims
    );
    println!(
        "CYBERMANJU_ELF_LEASE_SMP:complete:PASS elf_handoff=false module_execution=false physical_wx=false smp_mapping=false"
    );
}
