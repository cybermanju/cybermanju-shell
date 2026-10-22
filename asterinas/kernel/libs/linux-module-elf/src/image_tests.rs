// SPDX-License-Identifier: MPL-2.0
//! Multi-section data transactions, not executable module tests.
use std::{vec, vec::Vec};

use crate::{Error, Export, PreparedModule, SymbolResolver, plan_layout};

const DATA: usize = 704;
const STRINGS: usize = 724;
const SYMBOLS: usize = 744;
const RELA1: usize = 816;
const RELA2: usize = 840;
const NOTE: usize = 864;
fn h(i: usize) -> usize {
    64 + i * 64
}
fn p16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn p32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn p64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn sec(b: &mut [u8], i: usize, kind: u32, flags: u64, offset: usize, size: usize) {
    p32(b, h(i) + 4, kind);
    p64(b, h(i) + 8, flags);
    p64(b, h(i) + 24, offset as u64);
    p64(b, h(i) + 32, size as u64);
}
fn entry(b: &mut [u8], o: usize, offset: u64, symbol: u32, kind: u32, addend: i64) {
    p64(b, o, offset);
    p64(b, o + 8, (u64::from(symbol) << 32) | u64::from(kind));
    p64(b, o + 16, addend as u64);
}
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 872];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    p16(&mut b, 16, 1);
    p16(&mut b, 18, 62);
    p32(&mut b, 20, 1);
    p64(&mut b, 40, 64);
    p16(&mut b, 52, 64);
    p16(&mut b, 58, 64);
    p16(&mut b, 60, 10);
    sec(&mut b, 1, 1, 2, DATA, 16);
    p64(&mut b, h(1) + 48, 8);
    sec(&mut b, 2, 8, 3, 0, 16);
    p64(&mut b, h(2) + 24, u64::MAX);
    sec(&mut b, 3, 7, 2, NOTE, 4);
    sec(&mut b, 4, 1, 2, 0, 0);
    sec(&mut b, 5, 3, 0, STRINGS, 9);
    b[STRINGS..STRINGS + 9].copy_from_slice(b"\0missing\0");
    sec(&mut b, 6, 2, 0, SYMBOLS, 72);
    p32(&mut b, h(6) + 40, 5);
    p32(&mut b, h(6) + 44, 1);
    p64(&mut b, h(6) + 48, 8);
    p64(&mut b, h(6) + 56, 24);
    b[SYMBOLS + 28] = 0x11;
    p16(&mut b, SYMBOLS + 30, 2);
    p64(&mut b, SYMBOLS + 32, 8);
    b[SYMBOLS + 52] = 0x10;
    p32(&mut b, SYMBOLS + 48, 1);
    for (i, o, target) in [(7, RELA1, 1), (8, RELA2, 2)] {
        sec(&mut b, i, 4, 0, o, 24);
        p32(&mut b, h(i) + 40, 6);
        p32(&mut b, h(i) + 44, target);
        p64(&mut b, h(i) + 48, 8);
        p64(&mut b, h(i) + 56, 24);
    }
    sec(&mut b, 9, 1, 0, 868, 4);
    entry(&mut b, RELA1, 0, 1, 1, 0);
    entry(&mut b, RELA2, 8, 2, 1, 0);
    b[DATA..DATA + 16].fill(0xaa);
    b[NOTE..NOTE + 4].copy_from_slice(b"note");
    crate::inspect(&b).unwrap();
    b
}
fn exports() -> [Export<'static>; 1] {
    [Export {
        name: b"missing",
        value: 0x1234,
    }]
}
fn run(
    b: &[u8],
    ex: &[Export<'_>],
    bytes: usize,
    entries: usize,
    sections: usize,
) -> Result<PreparedModule, Error> {
    let layout = plan_layout(b, 0x1000, 1024, 10)?;
    SymbolResolver::new(b, &layout, ex)?.prepare_module(bytes, entries, sections)
}
#[test]
fn complete_image_includes_cross_section_rela_zero_fill_note_and_empty() {
    let b = fixture();
    let before = b.clone();
    let image = run(&b, &exports(), 36, 2, 4).unwrap();
    assert_eq!(image.payload_bytes(), 36);
    assert_eq!(image.relocation_entries(), 2);
    let s = image.sections();
    assert_eq!(s.len(), 4);
    assert_eq!(
        s.iter()
            .map(|s| s.placement().section_index)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(&s[0].bytes()[..8], &0x1018u64.to_le_bytes());
    assert_eq!(&s[0].bytes()[8..], &[0xaa; 8]);
    assert_eq!(&s[1].bytes()[..8], &[0; 8]);
    assert_eq!(&s[1].bytes()[8..], &0x1234u64.to_le_bytes());
    assert_eq!(s[2].bytes(), b"note");
    assert!(s[3].bytes().is_empty());
    assert_eq!(b, before);
}
#[test]
fn aggregate_payload_includes_note_bss_and_empty_section_count() {
    let b = fixture();
    for (bytes, entries, sections) in [(35, 2, 4), (36, 1, 4), (36, 2, 3)] {
        assert_eq!(
            run(&b, &exports(), bytes, entries, sections),
            Err(Error::ResourceLimit)
        );
    }
    assert!(run(&b, &exports(), 36, 2, 4).is_ok());
}
#[test]
fn late_missing_symbol_exposes_no_image_and_keeps_prior_result() {
    let b = fixture();
    let before = b.clone();
    let layout = plan_layout(&b, 0x1000, 36, 4).unwrap();
    let r = SymbolResolver::new(&b, &layout, &[]).unwrap();
    let first = r.relocate_target(1, 16, 1).unwrap();
    assert_eq!(r.prepare_module(36, 2, 4), Err(Error::MissingSymbol));
    assert_eq!(&first[..8], &0x1018u64.to_le_bytes());
    assert_eq!(b, before);
    assert!(run(&b, &exports(), 36, 2, 4).is_ok());
}
#[test]
fn late_arithmetic_overflow_rolls_back_all_private_sections() {
    let mut b = fixture();
    entry(&mut b, RELA2, 8, 2, 1, 1);
    let before = b.clone();
    let ex = [Export {
        name: b"missing",
        value: u64::MAX,
    }];
    assert_eq!(run(&b, &ex, 36, 2, 4), Err(Error::RelocationOverflow));
    assert_eq!(b, before);
}
#[test]
fn late_cross_table_overlap_rejects_whole_image() {
    let mut b = fixture();
    p32(&mut b, h(7) + 44, 2);
    entry(&mut b, RELA1, 4, 1, 1, 0);
    assert_eq!(run(&b, &exports(), 36, 2, 4), Err(Error::RelocationOverlap));
}
#[test]
fn note_relocation_is_rejected_even_empty_or_none() {
    for size in [0, 24] {
        let mut b = fixture();
        p32(&mut b, h(8) + 44, 3);
        p64(&mut b, h(8) + 32, size);
        entry(&mut b, RELA2, 0, 0, 0, 0);
        assert_eq!(run(&b, &exports(), 36, 2, 4), Err(Error::RelocationTarget));
    }
}
#[test]
fn entry_budget_includes_none_across_different_targets_and_hard_cap() {
    let mut b = fixture();
    entry(&mut b, RELA1, 0, 0, 0, 0);
    entry(&mut b, RELA2, 0, 0, 0, 0);
    assert_eq!(run(&b, &[], 36, 1, 4), Err(Error::ResourceLimit));
    assert_eq!(run(&b, &[], 36, 2, 4).unwrap().relocation_entries(), 2);
    let at = b.len();
    b.resize(at + 65_535 * 24, 0);
    p64(&mut b, h(7) + 24, at as u64);
    p64(&mut b, h(7) + 32, 65_535 * 24);
    assert_eq!(
        run(&b, &[], 36, usize::MAX, 4)
            .unwrap()
            .relocation_entries(),
        65_536
    );
    b.resize(at + 65_536 * 24, 0);
    p64(&mut b, h(7) + 32, 65_536 * 24);
    assert_eq!(run(&b, &[], 36, usize::MAX, 4), Err(Error::ResourceLimit));
}
#[test]
fn budgets_are_preflighted_before_symbol_resolution() {
    let b = fixture();
    assert_eq!(run(&b, &[], 35, 2, 4), Err(Error::ResourceLimit));
    assert_eq!(run(&b, &[], 36, 1, 4), Err(Error::ResourceLimit));
    assert_eq!(run(&b, &[], 36, 2, 3), Err(Error::ResourceLimit));
}
#[test]
fn nonalloc_rela_is_not_applied_or_charged() {
    let mut b = fixture();
    p32(&mut b, h(8) + 44, 9);
    entry(&mut b, RELA2, 0, 2, 10, 0);
    let image = run(&b, &[], 36, 1, 4).unwrap();
    assert_eq!(image.relocation_entries(), 1);
    assert_eq!(image.sections()[1].bytes(), &[0; 16]);
}
#[test]
fn empty_plan_succeeds_without_payload_or_exports() {
    let mut b = fixture();
    for i in 1..=4 {
        p64(&mut b, h(i) + 8, 0);
    }
    let image = run(&b, &[], 0, 0, 0).unwrap();
    assert!(image.sections().is_empty());
    assert_eq!(image.payload_bytes(), 0);
    assert_eq!(image.relocation_entries(), 0);
}
#[test]
fn padding_is_not_payload_and_layout_span_remains_separately_bounded() {
    let mut b = fixture();
    p64(&mut b, h(2) + 48, 256);
    let layout = plan_layout(&b, 0x1000, 512, 4).unwrap();
    assert!(layout.span() > 36);
    let image = SymbolResolver::new(&b, &layout, &exports())
        .unwrap()
        .prepare_module(36, 2, 4)
        .unwrap();
    assert_eq!(image.payload_bytes(), 36);
    assert_eq!(plan_layout(&b, 0x1000, 36, 4), Err(Error::ResourceLimit));
}
#[test]
fn public_transaction_truncations_and_mutations_never_panic() {
    let b = fixture();
    for end in 0..b.len() {
        let _ = run(&b[..end], &exports(), 36, 2, 4);
    }
    for i in 0..b.len() {
        for v in [0, 1, 0x7f, 0xff] {
            let mut m = b.clone();
            m[i] = v;
            let _ = run(&m, &exports(), 36, 2, 4);
        }
    }
}

#[test]
fn many_empty_alloc_targets_remain_budgeted_and_complete() {
    // Exercising many placements is a regression, not a CPU-time guarantee.
    let count = 1024usize;
    let mut b = vec![0; 64 + (count + 1) * 64];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    p16(&mut b, 16, 1);
    p16(&mut b, 18, 62);
    p32(&mut b, 20, 1);
    p64(&mut b, 40, 64);
    p16(&mut b, 52, 64);
    p16(&mut b, 58, 64);
    p16(&mut b, 60, (count + 1) as u16);
    for i in 1..=count {
        sec(&mut b, i, 1, 2, 0, 0);
    }
    let layout = plan_layout(&b, 0x1000, 0, count).unwrap();
    let resolver = SymbolResolver::new(&b, &layout, &[]).unwrap();
    assert_eq!(
        resolver.prepare_module(0, 0, count - 1),
        Err(Error::ResourceLimit)
    );
    let image = resolver.prepare_module(0, 0, count).unwrap();
    assert_eq!(image.sections().len(), count);
    assert_eq!(image.payload_bytes(), 0);
    for (i, s) in image.sections().iter().enumerate() {
        assert_eq!(usize::from(s.placement().section_index), i + 1);
        assert!(s.bytes().is_empty());
    }
}

#[test]
fn malformed_nonalloc_rela_is_still_structurally_rejected() {
    let mut b = fixture();
    p32(&mut b, h(8) + 44, 9); // Four-byte non-ALLOC debug target.
    entry(&mut b, RELA2, 2, 2, 10, 0); // Four-byte write runs beyond it.
    let before = b.clone();
    assert_eq!(run(&b, &[], 36, 1, 4), Err(Error::RelocationRange));
    assert_eq!(b, before);
}

#[test]
fn reach_preflight_checks_relative32_edges_before_preparing_data() {
    for kind in [2, 4] {
        let mut b = fixture();
        entry(&mut b, RELA2, 8, 2, kind, 0);
        let before = b.clone();
        let layout = plan_layout(&b, 0xffff_c000_0000_0000, 1024, 10).unwrap();
        let place = layout
            .placements()
            .iter()
            .find(|p| p.section_index == 2)
            .unwrap()
            .address
            + 8;
        for delta in [
            i128::from(i32::MIN),
            i128::from(i32::MAX),
            i128::from(i32::MIN) - 1,
            i128::from(i32::MAX) + 1,
        ] {
            let ex = [Export {
                name: b"missing",
                value: u64::try_from(i128::from(place) + delta).unwrap(),
            }];
            let resolver = SymbolResolver::new(&b, &layout, &ex).unwrap();
            if let Ok(encoded) = i32::try_from(delta) {
                assert_eq!(resolver.check_relocation_reach(2), Ok(2));
                let image = resolver.prepare_module(36, 2, 4).unwrap();
                assert_eq!(&image.sections()[1].bytes()[8..12], &encoded.to_le_bytes());
            } else {
                assert_eq!(
                    resolver.check_relocation_reach(2),
                    Err(Error::RelocationOverflow)
                );
                assert_eq!(
                    resolver.prepare_module(36, 2, 4),
                    Err(Error::RelocationOverflow)
                );
            }
        }
        assert_eq!(b, before);
    }
}

#[test]
fn reach_preflight_budgets_before_resolution_and_counts_none() {
    let mut b = fixture();
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let resolver = SymbolResolver::new(&b, &layout, &[]).unwrap();
    assert_eq!(
        resolver.check_relocation_reach(1),
        Err(Error::ResourceLimit)
    );
    assert_eq!(
        resolver.check_relocation_reach(2),
        Err(Error::MissingSymbol)
    );
    entry(&mut b, RELA1, 0, 0, 0, 0);
    entry(&mut b, RELA2, 0, 0, 0, 0);
    let resolver = SymbolResolver::new(&b, &layout, &[]).unwrap();
    assert_eq!(
        resolver.check_relocation_reach(1),
        Err(Error::ResourceLimit)
    );
    assert_eq!(resolver.check_relocation_reach(2), Ok(2));
}

#[test]
fn reach_preflight_ignores_nonalloc_but_rejects_note_even_empty() {
    let mut b = fixture();
    p32(&mut b, h(8) + 44, 9);
    entry(&mut b, RELA2, 0, 2, 10, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    assert_eq!(
        SymbolResolver::new(&b, &layout, &[])
            .unwrap()
            .check_relocation_reach(1),
        Ok(1)
    );
    p32(&mut b, h(8) + 44, 3);
    p64(&mut b, h(8) + 32, 0);
    assert_eq!(
        SymbolResolver::new(&b, &layout, &[])
            .unwrap()
            .check_relocation_reach(2),
        Err(Error::RelocationTarget)
    );
}

#[test]
fn reach_diagnostics_preserve_cause_and_pinpoint_the_failing_rela() {
    let b = fixture();
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let resolver = SymbolResolver::new(&b, &layout, &[]).unwrap();
    let issue = resolver.diagnose_relocation_reach(2).unwrap_err();
    assert_eq!(issue.cause, Error::MissingSymbol);
    assert_eq!(
        issue.site,
        Some(crate::RelocationSite {
            rela_section: 8,
            entry_index: 0,
            target_section: 2,
            kind: 1,
            symbol_index: 2,
            offset: 8
        })
    );
    assert_eq!(resolver.check_relocation_reach(2), Err(issue.cause));
    let budget = resolver.diagnose_relocation_reach(1).unwrap_err();
    assert_eq!(budget.cause, Error::ResourceLimit);
    assert_eq!(budget.site, None);
}

#[test]
fn reach_diagnostics_distinguish_overflow_and_policy_without_changing_input() {
    let mut b = fixture();
    entry(&mut b, RELA2, 8, 2, 2, 0);
    let before = b.clone();
    let layout = plan_layout(&b, 0xffff_c000_0000_0000, 1024, 10).unwrap();
    let ex = exports();
    let resolver = SymbolResolver::new(&b, &layout, &ex).unwrap();
    let issue = resolver.diagnose_relocation_reach(2).unwrap_err();
    assert_eq!(issue.cause, Error::RelocationOverflow);
    assert_eq!(issue.site.unwrap().kind, 2);
    assert_eq!(resolver.check_relocation_reach(2), Err(issue.cause));
    assert_eq!(b, before);
    p32(&mut b, h(8) + 44, 3);
    p64(&mut b, h(8) + 32, 0);
    let issue = SymbolResolver::new(&b, &layout, &ex)
        .unwrap()
        .diagnose_relocation_reach(2)
        .unwrap_err();
    assert_eq!(issue.cause, Error::RelocationTarget);
    assert_eq!(issue.site, None);
}

#[test]
fn reach_diagnostics_success_matches_legacy_preflight() {
    let b = fixture();
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let ex = exports();
    let resolver = SymbolResolver::new(&b, &layout, &ex).unwrap();
    assert_eq!(resolver.diagnose_relocation_reach(2), Ok(2));
    assert_eq!(resolver.check_relocation_reach(2), Ok(2));
}

#[test]
fn reach_diagnostics_count_none_when_locating_a_later_entry() {
    let mut b = fixture();
    let off = b.len();
    b.resize(off + 48, 0);
    p64(&mut b, h(8) + 24, off as u64);
    p64(&mut b, h(8) + 32, 48);
    entry(&mut b, off, 0, 0, 0, 0);
    entry(&mut b, off + 24, 8, 2, 1, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let resolver = SymbolResolver::new(&b, &layout, &[]).unwrap();
    let issue = resolver.diagnose_relocation_reach(3).unwrap_err();
    assert_eq!(issue.cause, Error::MissingSymbol);
    assert_eq!(issue.site.unwrap().entry_index, 1);
    assert_eq!(
        resolver.diagnose_relocation_reach(2).unwrap_err().site,
        None
    );
}

#[test]
fn r_x86_64_32s_accepts_valid_sign_extended_words_and_rejects_others() {
    for (symbol, encoded) in [
        (0, 0u32),
        (i32::MAX as u64, i32::MAX as u32),
        (0xffff_ffff_8000_0000, 0x8000_0000),
        (u64::MAX, u32::MAX),
    ] {
        let mut b = fixture();
        p64(&mut b, RELA1 + 8, (2u64 << 32) | 11);
        let image = run(
            &b,
            &[Export {
                name: b"missing",
                value: symbol,
            }],
            36,
            2,
            4,
        )
        .unwrap();
        assert_eq!(&image.sections()[0].bytes()[..4], &encoded.to_le_bytes());
    }
    for addend in [-1, i32::MIN as i64] {
        let mut b = fixture();
        p64(&mut b, RELA1 + 8, (2u64 << 32) | 11);
        p64(&mut b, RELA1 + 16, addend as u64);
        let image = run(
            &b,
            &[Export {
                name: b"missing",
                value: 0,
            }],
            36,
            2,
            4,
        )
        .unwrap();
        assert_eq!(
            &image.sections()[0].bytes()[..4],
            &(addend as i32).to_le_bytes()
        );
    }
    for (symbol, addend) in [
        (0x0000_0000_8000_0000, 0),
        (0, i32::MIN as i64 - 1),
        (u64::MAX, 1),
    ] {
        let mut b = fixture();
        p64(&mut b, RELA1 + 8, (2u64 << 32) | 11);
        p64(&mut b, RELA1 + 16, addend as u64);
        assert_eq!(
            run(
                &b,
                &[Export {
                    name: b"missing",
                    value: symbol
                }],
                36,
                2,
                4
            ),
            Err(Error::RelocationOverflow)
        );
    }
}

#[test]
fn r_32_and_pc32_keep_their_strict_unsigned_and_relative_ranges() {
    let mut b = fixture();
    p64(&mut b, RELA1 + 8, (2u64 << 32) | 10);
    assert_eq!(
        run(
            &b,
            &[Export {
                name: b"missing",
                value: 0x1_0000_0000
            }],
            36,
            2,
            4
        ),
        Err(Error::RelocationOverflow)
    );
    let mut b = fixture();
    p64(&mut b, RELA1 + 8, (2u64 << 32) | 2);
    assert_eq!(
        run(
            &b,
            &[Export {
                name: b"missing",
                value: 0xffff_ffff_8000_0000
            }],
            36,
            2,
            4
        ),
        Err(Error::RelocationOverflow)
    );
}

#[test]
fn relative32_import_constraints_intersect_imports_and_report_counters() {
    let mut b = fixture();
    entry(&mut b, RELA1, 0, 2, 2, 0);
    entry(&mut b, RELA2, 8, 2, 4, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let resolver = SymbolResolver::new(&b, &layout, &exports()).unwrap();
    let constraints = resolver.relative32_import_constraints(2).unwrap();
    assert_eq!(constraints.rela_entries(), 2);
    assert_eq!(constraints.import_relocations(), 2);
    assert_eq!(constraints.ignored_relative32_relocations(), 0);
    assert!(constraints.window().unwrap().contains(layout.base()));
}

#[test]
fn relative32_import_constraints_full_window_without_imports_and_counts_locals() {
    let mut b = fixture();
    entry(&mut b, RELA1, 0, 1, 2, 0);
    entry(&mut b, RELA2, 0, 0, 0, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let resolver = SymbolResolver::new(&b, &layout, &[]).unwrap();
    let constraints = resolver.relative32_import_constraints(2).unwrap();
    assert_eq!(constraints.rela_entries(), 2);
    assert_eq!(constraints.import_relocations(), 0);
    assert_eq!(constraints.ignored_relative32_relocations(), 1);
    assert_eq!(
        constraints.window(),
        crate::Relative32BaseWindow::for_module_span(layout.span())
    );
}

#[test]
fn relative32_import_constraints_report_disjoint_import_windows() {
    let mut b = fixture();
    entry(&mut b, RELA1, 0, 2, 2, 0);
    entry(&mut b, RELA2, 8, 2, 4, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let p1 = layout
        .placements()
        .iter()
        .find(|p| p.section_index == 1)
        .unwrap()
        .address
        - layout.base();
    let p2 = layout
        .placements()
        .iter()
        .find(|p| p.section_index == 2)
        .unwrap()
        .address
        - layout.base()
        + 8;
    entry(&mut b, RELA2, 8, 2, 4, (1i64 << 32) + p2 as i64 - p1 as i64);
    let resolver = SymbolResolver::new(
        &b,
        &layout,
        &[Export {
            name: b"missing",
            value: 1 << 32,
        }],
    )
    .unwrap();
    let constraints = resolver.relative32_import_constraints(2).unwrap();
    assert_eq!(constraints.rela_entries(), 2);
    assert_eq!(constraints.import_relocations(), 2);
    assert_eq!(constraints.ignored_relative32_relocations(), 0);
    assert_eq!(constraints.window(), None);
}

#[test]
fn relative32_import_constraints_missing_symbol_after_empty_window() {
    let mut b = fixture();
    b[STRINGS..STRINGS + 16].copy_from_slice(b"\0missing\0absent\0");
    p64(&mut b, h(5) + 32, 16);
    p32(&mut b, SYMBOLS + 24, 9);
    p16(&mut b, SYMBOLS + 30, 0);
    p64(&mut b, SYMBOLS + 32, 0);
    entry(&mut b, RELA1, 0, 2, 2, i64::MIN);
    entry(&mut b, RELA2, 8, 1, 2, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let provided = [Export {
        name: b"missing",
        value: 0,
    }];
    assert!(crate::relative32_base_window(0, i64::MIN, 0, layout.span()).is_none());
    let resolver = SymbolResolver::new(&b, &layout, &provided).unwrap();
    assert_eq!(
        resolver.relative32_import_constraints(2),
        Err(Error::MissingSymbol)
    );
}

#[test]
fn relative32_constraints_budget_precedes_missing_symbols_and_counts_none() {
    let mut b = fixture();
    b[STRINGS..STRINGS + 16].copy_from_slice(b"\0missing\0absent\0");
    p64(&mut b, h(5) + 32, 16);
    p32(&mut b, SYMBOLS + 24, 9);
    p16(&mut b, SYMBOLS + 30, 0);
    p64(&mut b, SYMBOLS + 32, 0);
    entry(&mut b, RELA1, 0, 2, 2, i64::MIN);
    entry(&mut b, RELA2, 8, 1, 2, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let resolver = SymbolResolver::new(&b, &layout, &[]).unwrap();
    assert_eq!(
        resolver.relative32_import_constraints(0),
        Err(Error::ResourceLimit)
    );
    assert_eq!(
        resolver.relative32_import_constraints(1),
        Err(Error::ResourceLimit)
    );
    assert_eq!(
        resolver.relative32_import_constraints(2),
        Err(Error::MissingSymbol)
    );

    entry(&mut b, RELA1, 0, 0, 0, 0);
    entry(&mut b, RELA2, 0, 0, 0, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let constraints = SymbolResolver::new(&b, &layout, &[])
        .unwrap()
        .relative32_import_constraints(2)
        .unwrap();
    assert_eq!(constraints.rela_entries(), 2);
    assert_eq!(constraints.import_relocations(), 0);
}

#[test]
fn relative32_constraints_ignore_debug_target_but_reject_note_target() {
    let mut b = fixture();
    p32(&mut b, h(8) + 44, 9);
    entry(&mut b, RELA1, 0, 0, 0, 0);
    entry(&mut b, RELA2, 0, 2, 2, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let constraints = SymbolResolver::new(&b, &layout, &exports())
        .unwrap()
        .relative32_import_constraints(2)
        .unwrap();
    assert_eq!(constraints.rela_entries(), 1);
    assert_eq!(constraints.import_relocations(), 0);
    assert_eq!(constraints.ignored_relative32_relocations(), 0);
    p32(&mut b, h(8) + 44, 3);
    assert_eq!(
        SymbolResolver::new(&b, &layout, &exports())
            .unwrap()
            .relative32_import_constraints(2),
        Err(Error::RelocationTarget)
    );
}

#[test]
fn relative32_constraints_validate_offset_ranges_and_checked_overflow() {
    for offset in [13, u64::MAX] {
        let mut b = fixture();
        entry(&mut b, RELA1, offset, 2, 2, 0);
        assert_eq!(crate::inspect(&b), Err(Error::RelocationRange));
    }
}

#[test]
fn relative32_constraints_count_absolute_and_local_as_ignored_but_preflight_others() {
    let mut b = fixture();
    entry(&mut b, RELA1, 0, 1, 2, 0);
    p16(&mut b, SYMBOLS + 54, 0xfff1);
    entry(&mut b, RELA2, 0, 2, 2, 0);
    let layout = plan_layout(&b, 0x1000, 1024, 10).unwrap();
    let constraints = SymbolResolver::new(&b, &layout, &[])
        .unwrap()
        .relative32_import_constraints(2)
        .unwrap();
    assert_eq!(constraints.import_relocations(), 0);
    assert_eq!(constraints.ignored_relative32_relocations(), 2);

    p16(&mut b, SYMBOLS + 54, 0);
    entry(&mut b, RELA2, 8, 2, 1, 0);
    let constraints = SymbolResolver::new(&b, &layout, &[])
        .unwrap()
        .relative32_import_constraints(2)
        .unwrap();
    assert_eq!(constraints.rela_entries(), 2);
    assert_eq!(constraints.import_relocations(), 0);
    assert_eq!(constraints.ignored_relative32_relocations(), 1);
}
