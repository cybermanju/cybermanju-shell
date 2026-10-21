// SPDX-License-Identifier: MPL-2.0
//! Trusted, embedded kbuild fixture. Deliberately never calls any module entry.
use alloc::{vec, vec::Vec};

use linux_module_elf::{Error as ElfError, SymbolResolver, plan_layout, plan_page_layout};
use ostd::{mm::PAGE_SIZE, prelude::*};

use super::{Error, stage_prepared};

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
        "CYBERMANJU_PREPARED_RUNTIME:roundtrip:PASS sections={} span={} payload={} relocations={} pages={}",
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
    // No pointer conversion, function pointer, call to an ELF symbol or mapping.
    println!("CYBERMANJU_PREPARED_RUNTIME:no_init_or_exit_called:PASS");
}
