// SPDX-License-Identifier: MPL-2.0
//! Data-only near-text preparation coverage; never maps or executes.
use alloc::vec;

use linux_module_elf::{Error as ElfError, Export};
use ostd::{
    cpu,
    mm::{PAGE_SIZE, module_address::NearTextAddressReservation},
    prelude::*,
};

use super::{
    FIXTURE, MAX_SECTIONS, MAX_SPAN, PreparationLimits, fixture_with_u32_relocation,
    relative_import_fixture,
};
use crate::{Error, NearTextModuleLayout};

#[ktest]
fn cybermanju_near_text_prepared_runtime() {
    println!("");
    println!(
        "CYBERMANJU_NEAR_TEXT_PREPARED:begin:PASS cpus={} kinds=2 rejected=6",
        cpu::num_cpus()
    );
    assert_eq!(cpu::num_cpus(), 1);
    let envelope =
        linux_module_elf::page_layout_span_bound(FIXTURE, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let arena = NearTextAddressReservation::arena_range();
    let base = arena.end.checked_sub(64 * PAGE_SIZE).unwrap();
    let limits = |max_pages| PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages,
        max_chunks: MAX_SECTIONS,
    };
    let export = ostd::cpu::num_cpus as *const () as usize as u64;
    assert!(export >= 0xffff_ffff_8000_0000);
    let mut reclaimed = 0;

    for kind in [2u32, 4] {
        let (bytes, name, target, offset) = relative_import_fixture(kind);
        let exports = [Export {
            name,
            value: export,
        }];
        let plan = NearTextModuleLayout::new_at(
            &bytes,
            &exports,
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            MAX_SECTIONS,
        )
        .unwrap();
        let range = plan.range();
        assert_eq!(
            range,
            base..base + envelope,
            "near-text layout must own exact requested range"
        );
        let layout = plan.layout();
        let placement = layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let relative = usize::try_from(placement.address - base as u64 + offset).unwrap();
        let page_layout =
            linux_module_elf::plan_page_layout(&bytes, base as u64, MAX_SPAN, MAX_SECTIONS)
                .unwrap();
        let resolver =
            linux_module_elf::SymbolResolver::new(&bytes, &page_layout, &exports).unwrap();
        let prepared = resolver
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
            .unwrap();
        let span = usize::try_from(layout.span()).unwrap();
        let mut expected = vec![0; span];
        for section in prepared.sections() {
            let start = usize::try_from(section.placement().address - base as u64).unwrap();
            expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
        }
        let delta =
            i32::try_from((export as i128) - (placement.address as i128 + offset as i128)).unwrap();
        assert_eq!(&expected[relative..relative + 4], &delta.to_le_bytes());
        let staged = plan
            .prepare_and_stage(&bytes, &exports, limits(envelope / PAGE_SIZE))
            .unwrap();
        assert_eq!(
            staged.reserved_range(),
            range,
            "near-text staged owner must retain original reservation"
        );
        assert_eq!(staged.layout().base(), base as u64);
        assert_eq!(staged.span(), span);
        assert_eq!(staged.page_count(), span / PAGE_SIZE);
        assert!(matches!(
            NearTextAddressReservation::new_at(range.start, range.len(), range.len() / PAGE_SIZE),
            Err(ostd::Error::NoMemory)
        ));
        let mut actual = vec![0xff; span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(
            actual, expected,
            "all prepared bytes, including gaps and tails"
        );
        assert_eq!(&actual[relative..relative + 4], &delta.to_le_bytes());
        assert!(matches!(
            NearTextAddressReservation::new_at(range.start, range.len(), range.len() / PAGE_SIZE),
            Err(ostd::Error::NoMemory)
        ));
        println!(
            "CYBERMANJU_NEAR_TEXT_PREPARED:staged:PASS kind={} base={:#x} reserved={} span={} pages={} payload={} relas={} export={:#x} compared={}",
            kind,
            base,
            range.len(),
            span,
            staged.page_count(),
            prepared.payload_bytes(),
            prepared.relocation_entries(),
            export,
            span
        );
        drop(staged);
        let reclaimed_owner =
            NearTextAddressReservation::new_at(range.start, range.len(), range.len() / PAGE_SIZE)
                .unwrap();
        assert_eq!(reclaimed_owner.range(), range);
        drop(reclaimed_owner);
        reclaimed += 1;
        drop(prepared);
        drop(resolver);
    }

    let mut rejected = 0;
    let mut reject_preflight =
        |bytes: &[u8], exports: &[Export<'_>], expected: ElfError, max_entries: usize| {
            let result = NearTextModuleLayout::new_at(
                bytes,
                exports,
                base,
                MAX_SPAN as usize / PAGE_SIZE,
                MAX_SECTIONS,
                max_entries,
            );
            assert!(
                matches!(result, Err(Error::Plan(e)) if e == expected),
                "near-text constructor preflight rejection case={rejected}"
            );
            let reclaimed_owner =
                NearTextAddressReservation::new_at(base, envelope, envelope / PAGE_SIZE).unwrap();
            drop(reclaimed_owner);
            reclaimed += 1;
            rejected += 1;
        };
    let mut bad32 = fixture_with_u32_relocation();
    let changed = bad32.iter().zip(FIXTURE).position(|(a, b)| a != b).unwrap();
    bad32[changed..changed + 4].copy_from_slice(&11u32.to_le_bytes());
    reject_preflight(&bad32, &[], ElfError::RelocationOverflow, 65_536);
    let u32_self_relative = fixture_with_u32_relocation();
    reject_preflight(
        &u32_self_relative,
        &[],
        ElfError::RelocationOverflow,
        65_536,
    );
    let (pc32, name, _, _) = relative_import_fixture(2);
    reject_preflight(
        &pc32,
        &[Export { name, value: 0 }],
        ElfError::RelocationOverflow,
        65_536,
    );
    reject_preflight(&pc32, &[], ElfError::MissingSymbol, 65_536);
    let exports = [Export {
        name,
        value: export,
    }];
    reject_preflight(&pc32, &exports, ElfError::ResourceLimit, 0);
    let plan = NearTextModuleLayout::new_at(
        &pc32,
        &exports,
        base,
        MAX_SPAN as usize / PAGE_SIZE,
        MAX_SECTIONS,
        MAX_SECTIONS,
    )
    .unwrap();
    let range = plan.range();
    let mut page_limit = limits(envelope / PAGE_SIZE);
    page_limit.max_pages = 0;
    assert!(matches!(
        plan.prepare_and_stage(&pc32, &exports, page_limit),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    let owner =
        NearTextAddressReservation::new_at(range.start, range.len(), range.len() / PAGE_SIZE)
            .unwrap();
    drop(owner);
    reclaimed += 1;
    rejected += 1;
    assert_eq!((rejected, reclaimed), (6, 8));
    println!(
        "CYBERMANJU_NEAR_TEXT_PREPARED:complete:PASS staged=2 rejected=6 reclaimed=8 compared_full_span=true actual_kernel_address=true linux_export_lookup=false pages_mapped=false execution=false"
    );
}
