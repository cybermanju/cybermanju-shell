// SPDX-License-Identifier: MPL-2.0
//! Trusted, embedded kbuild fixture. Deliberately never calls any module entry.
use alloc::{vec, vec::Vec};

use linux_module_elf::{Error as ElfError, SymbolResolver, plan_layout, plan_page_layout};
use ostd::{mm::PAGE_SIZE, prelude::*};

use super::{Error, ReservedModuleLayout, stage_prepared};

// Only expanded for the selected kernel test; normal builds need no fixture.
const FIXTURE: &[u8] = include_bytes!(env!("CYBERMANJU_STAGING_FIXTURE"));
const BASE: u64 = 0x1000_0000; // Numeric relocation model, NOT a mapped address.
const MAX_SPAN: u64 = 4 * 1024 * 1024;
const MAX_SECTIONS: usize = 1024;

#[ktest]
fn cybermanju_prepared_module_runtime() {
    let layout = plan_page_layout(FIXTURE, BASE, MAX_SPAN, MAX_SECTIONS).unwrap();
    let resolver = SymbolResolver::new(FIXTURE, &layout, &[]).unwrap();
    let prepared = resolver
        .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
        .unwrap();
    let span = usize::try_from(layout.span()).unwrap();
    let sections = prepared.sections().len();
    let pages = span / PAGE_SIZE;
    assert!(sections > 0 && pages > 0 && prepared.relocation_entries() > 0);

    // Independent relative offsets, not the bridge's descriptor array.
    let mut expected = vec![0; span];
    let before: Vec<_> = prepared
        .sections()
        .iter()
        .map(|s| s.bytes().to_vec())
        .collect();
    for section in prepared.sections() {
        let start =
            usize::try_from(section.placement().address.checked_sub(BASE).unwrap()).unwrap();
        let end = start.checked_add(section.bytes().len()).unwrap();
        expected[start..end].copy_from_slice(section.bytes());
    }
    {
        let staged = stage_prepared(&prepared, &layout, pages, sections).unwrap();
        assert_eq!((staged.span(), staged.page_count()), (span, pages));
        let mut actual = vec![0xff; span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(
            actual, expected,
            "payload, gaps and page tails must match exactly"
        );
        let mut byte = [0];
        assert!(staged.read_bytes(span, &mut byte).is_err());
    }
    for (section, original) in prepared.sections().iter().zip(before) {
        assert_eq!(
            section.bytes(),
            original.as_slice(),
            "prepared input remains immutable"
        );
    }
    println!(
        "\nCYBERMANJU_PREPARED_RUNTIME:roundtrip:PASS sections={} span={} payload={} relocations={} pages={}",
        sections,
        span,
        prepared.payload_bytes(),
        prepared.relocation_entries(),
        pages
    );

    assert!(matches!(
        stage_prepared(&prepared, &layout, pages - 1, sections),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    assert!(matches!(
        stage_prepared(&prepared, &layout, pages, sections - 1),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    let different =
        plan_page_layout(FIXTURE, BASE + PAGE_SIZE as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
    assert!(matches!(
        stage_prepared(&prepared, &different, pages, sections),
        Err(Error::Plan(ElfError::LayoutMismatch))
    ));
    let compact = plan_layout(FIXTURE, BASE, MAX_SPAN, MAX_SECTIONS).unwrap();
    assert!(matches!(
        stage_prepared(&prepared, &compact, pages, sections),
        Err(Error::Plan(ElfError::PageAlignment))
    ));
    println!("CYBERMANJU_PREPARED_RUNTIME:rejected_budgets_and_mismatched_layout:PASS");

    // Replan at a real owned base, not by shifting the old numeric plan.
    let envelope =
        linux_module_elf::page_layout_span_bound(FIXTURE, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    assert!(envelope > 0 && envelope.is_multiple_of(PAGE_SIZE));
    for budget in [0, envelope / PAGE_SIZE - 1, usize::MAX] {
        assert!(matches!(
            ReservedModuleLayout::new(FIXTURE, budget, MAX_SECTIONS),
            Err(Error::Plan(ElfError::ResourceLimit))
        ));
    }
    assert!(matches!(
        ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, 0),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    let owned =
        ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS).unwrap();
    let actual_layout = owned.layout();
    let base = owned.range().start as u64;
    assert_eq!(actual_layout.base(), base);
    assert_eq!(owned.range().len(), envelope);
    assert!(base >= 0xffff_8000_0000_0000);
    assert!(actual_layout.span() > 0 && actual_layout.span() <= envelope as u64);
    for section in actual_layout.placements() {
        assert!(section.address >= base);
        assert!(section.address.checked_add(section.size).unwrap() <= owned.range().end as u64);
    }
    let real_resolver = SymbolResolver::new(FIXTURE, actual_layout, &[]).unwrap();
    let real_prepared = real_resolver
        .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
        .unwrap();
    assert!(
        real_prepared
            .sections()
            .iter()
            .zip(prepared.sections())
            .any(|(real, numeric)| real.bytes() != numeric.bytes()),
        "this fixture's absolute relocations must change bytes at the reserved base"
    );
    let actual_span = actual_layout.span() as usize;
    let actual_pages = actual_span / PAGE_SIZE;
    let mut real_expected = vec![0; actual_span];
    for section in real_prepared.sections() {
        let start = (section.placement().address - base) as usize;
        real_expected[start..start + section.bytes().len()].copy_from_slice(section.bytes());
    }
    drop(real_resolver);
    let reserved_range = owned.range();
    let mut real_staged = owned
        .stage(&real_prepared, actual_pages, real_prepared.sections().len())
        .unwrap();
    assert_eq!(real_staged.reserved_range(), reserved_range);
    assert_eq!(real_staged.layout().base(), base);
    assert_eq!(
        (real_staged.span(), real_staged.page_count()),
        (actual_span, actual_pages)
    );
    let mut real_actual = vec![0xff; actual_span];
    real_staged.read_bytes(0, &mut real_actual).unwrap();
    assert_eq!(
        real_actual, real_expected,
        "reserved-base prepared bytes must roundtrip"
    );
    let mut byte = [0xff];
    assert!(real_staged.read_bytes(actual_span, &mut byte).is_err());
    assert_eq!(byte, [0xff]);
    let other = ostd::mm::module_address::ModuleAddressReservation::new(PAGE_SIZE, 1).unwrap();
    assert!(other.range().end <= reserved_range.start || reserved_range.end <= other.range().start);
    drop(other);
    let mut mapped_actual = vec![0xff; actual_span];
    real_staged
        .read_mapped_for_test(0, &mut mapped_actual)
        .unwrap();
    assert_eq!(
        mapped_actual, real_expected,
        "real VA must read the same prepared bytes"
    );
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:scoped_rw_mapping:PASS pages={}",
        actual_pages
    );
    println!(
        "CYBERMANJU_PREPARED_RUNTIME:reserved_layout:PASS base={:#x} reserved={} span={} sections={} payload={} relocations={} pages={}",
        base,
        envelope,
        actual_span,
        real_prepared.sections().len(),
        real_prepared.payload_bytes(),
        real_prepared.relocation_entries(),
        actual_pages
    );
    drop(real_staged);
    let reused =
        ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS).unwrap();
    assert_eq!(reused.range(), reserved_range);
    drop(reused);
    for bad_layout in [false, true] {
        let failed_owner =
            ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
                .unwrap();
        let failed_range = failed_owner.range();
        if bad_layout {
            assert!(matches!(
                failed_owner.stage(&prepared, actual_pages, sections),
                Err(Error::Plan(ElfError::LayoutMismatch))
            ));
        } else {
            assert!(matches!(
                failed_owner.stage(&real_prepared, 0, sections),
                Err(Error::Plan(ElfError::ResourceLimit))
            ));
        }
        let probe = ReservedModuleLayout::new(FIXTURE, MAX_SPAN as usize / PAGE_SIZE, MAX_SECTIONS)
            .unwrap();
        assert_eq!(
            probe.range(),
            failed_range,
            "error must return the consumed reservation"
        );
        drop(probe);
    }
    println!("CYBERMANJU_PREPARED_RUNTIME:reserved_owner_lifecycle:PASS cases=3");
    // No pointer conversion, function pointer, call to an ELF symbol or mapping.
    println!("CYBERMANJU_PREPARED_RUNTIME:no_init_or_exit_called:PASS");
}
