// SPDX-License-Identifier: MPL-2.0
//! Bounded signed-32 candidate search; data-only and never maps or executes.
use alloc::{vec, vec::Vec};

use linux_module_elf::{Error as ElfError, Export, Relative32BaseWindow, plan_page_layout};
use ostd::{
    cpu,
    mm::{PAGE_SIZE, module_address::NearTextAddressReservation},
    prelude::*,
};

use super::{
    FIXTURE, MAX_SECTIONS, MAX_SPAN, relative_import_fixture, signed32::signed_self_fixture,
};
use crate::{Error, NearTextModuleLayout, PreparationLimits};

#[ktest]
fn cybermanju_signed32_search_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 1);
    println!("CYBERMANJU_SIGNED32_SEARCH:begin:PASS cpus=1 kinds=3");
    let pool = NearTextAddressReservation::signed32_arena_range();
    let first = pool.start;
    let selected = first + PAGE_SIZE;
    let envelope =
        linux_module_elf::page_layout_span_bound(FIXTURE, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let span = envelope as u64;
    let window = Relative32BaseWindow::for_module_span(span)
        .unwrap()
        .restrict((first - PAGE_SIZE) as u64, selected as u64)
        .unwrap();
    let max_pages = MAX_SPAN as usize / PAGE_SIZE;
    let limits = |pages| PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages: pages,
        max_chunks: MAX_SECTIONS,
    };
    let export_value = cpu::num_cpus as *const () as usize as u64;
    let mut prefix_exhaustions = 0usize;
    let mut staged_count = 0usize;
    let mut rejected = 0usize;
    let mut available_checks = 0usize;

    // Deliberately occupy only the first address; exhaustion of one candidate is
    // a prefix result, not evidence that the signed arena is full.
    let blocker = NearTextAddressReservation::new_signed32_at(first, PAGE_SIZE, 1).unwrap();

    for kind in [11u32, 2, 4] {
        let (bytes, exports, target, offset, symbol, self_defined) = if kind == 11 {
            let (bytes, target, offset, defined) = signed_self_fixture();
            let planned =
                plan_page_layout(&bytes, selected as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
            let value = planned
                .placements()
                .iter()
                .find(|p| p.section_index == defined)
                .unwrap()
                .address;
            (bytes, Vec::new(), target, offset, value, true)
        } else {
            let (bytes, name, target, offset) = relative_import_fixture(kind);
            (
                bytes,
                vec![Export {
                    name,
                    value: export_value,
                }],
                target,
                offset,
                export_value,
                false,
            )
        };
        assert!(
            matches!(
                NearTextModuleLayout::new_signed32_in_relative32_window(
                    &bytes,
                    &exports,
                    window,
                    max_pages,
                    MAX_SECTIONS,
                    65_536,
                    1,
                ),
                Err(Error::AddressSpace(ostd::Error::NoMemory))
            ),
            "signed32 search must honor one-candidate prefix"
        );
        prefix_exhaustions += 1;
        let selected_probe =
            NearTextAddressReservation::new_signed32_at(selected, envelope, envelope / PAGE_SIZE)
                .unwrap();
        drop(selected_probe);
        let reference_layout =
            plan_page_layout(&bytes, selected as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
        let expected_range = selected..selected + envelope;
        assert_eq!(reference_layout.span() as usize, envelope);
        let candidate_plan = NearTextModuleLayout::new_signed32_in_relative32_window(
            &bytes,
            &exports,
            window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            2,
        )
        .unwrap();
        assert_eq!(
            candidate_plan.range(),
            expected_range,
            "search must choose the second free candidate"
        );
        let layout = candidate_plan.layout();
        let place = layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let field = place.address + offset;
        // Relocation expectations are computed using numeric ELF addresses, all
        // as u64; convert to usize only when indexing a materialized byte vector.
        let expected_field = if kind == 11 {
            assert_eq!(
                (symbol as u32 as i32 as i64) as u64,
                symbol,
                "signed self must fit R_X86_64_32S"
            );
            (symbol as u32).to_le_bytes()
        } else {
            let delta = i128::from(symbol) - i128::from(field);
            i32::try_from(delta).unwrap().to_le_bytes()
        };
        let page_layout = plan_page_layout(&bytes, layout.base(), MAX_SPAN, MAX_SECTIONS).unwrap();
        let resolver =
            linux_module_elf::SymbolResolver::new(&bytes, &page_layout, &exports).unwrap();
        let prepared = resolver
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
            .unwrap();
        assert_eq!(reference_layout.span(), layout.span());
        let full_span = layout.span() as usize;
        let relative = usize::try_from(field - layout.base()).unwrap();
        let mut expected_all = vec![0; full_span];
        for section in prepared.sections() {
            let start = usize::try_from(section.placement().address - layout.base()).unwrap();
            expected_all[start..start + section.bytes().len()].copy_from_slice(section.bytes());
        }
        assert_eq!(&expected_all[relative..relative + 4], &expected_field);
        let staged = candidate_plan
            .prepare_and_stage(&bytes, &exports, limits(envelope / PAGE_SIZE))
            .unwrap();
        let reserved = staged.reserved_range();
        let pages = staged.page_count();
        assert_eq!(reserved, expected_range);
        assert_eq!(staged.span(), full_span);
        assert_eq!(pages, full_span / PAGE_SIZE);
        let payload = prepared.payload_bytes();
        let relas = prepared.relocation_entries();
        let mut actual = vec![0xff; full_span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(actual, expected_all, "compare every byte in the full span");
        assert_eq!(&actual[relative..relative + 4], &expected_field);
        assert!(
            matches!(
                NearTextAddressReservation::new_signed32_at(
                    reserved.start,
                    reserved.len(),
                    reserved.len() / PAGE_SIZE
                ),
                Err(ostd::Error::NoMemory)
            ),
            "staged owner must retain search reservation"
        );
        drop(staged);
        let reclaim = NearTextAddressReservation::new_signed32_at(
            reserved.start,
            reserved.len(),
            reserved.len() / PAGE_SIZE,
        )
        .unwrap();
        assert_eq!(reclaim.range(), expected_range);
        drop(reclaim);
        available_checks += 1;
        staged_count += 1;
        println!(
            "CYBERMANJU_SIGNED32_SEARCH:staged:PASS kind={} first={:#x} selected={:#x} reserved={} span={} pages={} payload={} relas={} symbol={:#x} field={:#x} compared={} self_defined={}",
            kind,
            first,
            selected,
            reserved.len(),
            full_span,
            pages,
            payload,
            relas,
            symbol,
            field,
            full_span,
            self_defined
        );
    }
    drop(blocker);
    let reclaim_first = NearTextAddressReservation::new_signed32_at(first, PAGE_SIZE, 1).unwrap();
    drop(reclaim_first);

    // Each rejected call is followed by an exact first-range reservation probe.
    let mut prove_available = || {
        let probe =
            NearTextAddressReservation::new_signed32_at(first, envelope, envelope / PAGE_SIZE)
                .unwrap();
        drop(probe);
        available_checks += 1;
    };
    let empty_window = Relative32BaseWindow::for_module_span(span).unwrap();
    assert!(matches!(
        NearTextModuleLayout::new_signed32_in_relative32_window(
            FIXTURE,
            &[],
            empty_window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            0
        ),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    rejected += 1;
    prove_available();
    assert!(matches!(
        NearTextModuleLayout::new_signed32_in_relative32_window(
            FIXTURE,
            &[],
            empty_window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            65
        ),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    rejected += 1;
    prove_available();
    let old = NearTextAddressReservation::arena_range();
    let old_window = Relative32BaseWindow::for_module_span(span)
        .unwrap()
        .restrict(old.start as u64, old.end as u64)
        .unwrap();
    assert!(matches!(
        NearTextModuleLayout::new_signed32_in_relative32_window(
            FIXTURE,
            &[],
            old_window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            2
        ),
        Err(Error::AddressSpace(ostd::Error::NoMemory))
    ));
    rejected += 1;
    prove_available();
    let near_end = Relative32BaseWindow::for_module_span(span)
        .unwrap()
        .restrict((pool.end - PAGE_SIZE) as u64, pool.end as u64 - 1)
        .unwrap();
    assert!(matches!(
        NearTextModuleLayout::new_signed32_in_relative32_window(
            FIXTURE,
            &[],
            near_end,
            max_pages,
            MAX_SECTIONS,
            65_536,
            2
        ),
        Err(Error::AddressSpace(ostd::Error::NoMemory))
    ));
    rejected += 1;
    prove_available();
    let (pc32, name, _, _) = relative_import_fixture(2);
    assert!(matches!(
        NearTextModuleLayout::new_signed32_in_relative32_window(
            &pc32,
            &[],
            empty_window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            2
        ),
        Err(Error::Plan(ElfError::MissingSymbol))
    ));
    rejected += 1;
    prove_available();
    let zero = [Export { name, value: 0 }];
    assert!(matches!(
        NearTextModuleLayout::new_signed32_in_relative32_window(
            &pc32,
            &zero,
            empty_window,
            max_pages,
            MAX_SECTIONS,
            65_536,
            2
        ),
        Err(Error::Plan(ElfError::RelocationOverflow))
    ));
    rejected += 1;
    prove_available();
    // Existing unsigned-32 mutation helper is shared with runtime_tests.
    let bad_u32 = super::fixture_with_u32_relocation();
    assert!(
        matches!(
            NearTextModuleLayout::new_signed32_in_relative32_window(
                &bad_u32,
                &[],
                empty_window,
                max_pages,
                MAX_SECTIONS,
                65_536,
                2
            ),
            Err(Error::Plan(ElfError::RelocationOverflow))
        ),
        "unsigned search must fail full preflight"
    );
    rejected += 1;
    prove_available();
    let exports = [Export {
        name,
        value: export_value,
    }];
    assert!(matches!(
        NearTextModuleLayout::new_signed32_in_relative32_window(
            &pc32,
            &exports,
            empty_window,
            max_pages,
            MAX_SECTIONS,
            0,
            2
        ),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    rejected += 1;
    prove_available();
    let valid = NearTextModuleLayout::new_signed32_in_relative32_window(
        &pc32,
        &exports,
        empty_window,
        max_pages,
        MAX_SECTIONS,
        65_536,
        2,
    )
    .unwrap();
    assert!(matches!(
        valid.prepare_and_stage(&pc32, &exports, limits(0)),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    rejected += 1;
    prove_available();
    assert_eq!(
        (staged_count, prefix_exhaustions, rejected, available_checks),
        (3, 3, 9, 12)
    );
    println!(
        "CYBERMANJU_SIGNED32_SEARCH:complete:PASS staged=3 prefix_exhaustions=3 rejected=9 available_checks=12 compared_full_span=true pages_mapped=false linux_export_lookup=false execution=false"
    );
}
