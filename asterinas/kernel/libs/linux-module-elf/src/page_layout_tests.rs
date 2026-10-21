// SPDX-License-Identifier: MPL-2.0
//! Numeric page-layout policy tests; no mapping, permission enforcement, or execution.
use crate::{Error, SymbolResolver, plan_layout, plan_page_layout};
use std::{vec, vec::Vec};

const PAGE: u64 = 4096;
fn p16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn p32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn p64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn fixture(count: u16) -> Vec<u8> {
    let mut b = vec![0; 64 + usize::from(count) * 64];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    p16(&mut b, 16, 1);
    p16(&mut b, 18, 62);
    p32(&mut b, 20, 1);
    p64(&mut b, 40, 64);
    p16(&mut b, 52, 64);
    p16(&mut b, 58, 64);
    p16(&mut b, 60, count);
    b
}
fn sec(b: &mut [u8], i: usize, ty: u32, flags: u64, size: u64, align: u64) {
    let o = 64 + i * 64;
    p32(b, o + 4, ty);
    p64(b, o + 8, flags);
    p64(b, o + 32, size);
    p64(b, o + 48, align);
}

#[test]
fn separates_nonempty_rx_rw_and_readonly_sections_by_reserved_pages() {
    let mut b = fixture(4);
    sec(&mut b, 1, 1, 2 | 4, 1, 1);
    sec(&mut b, 2, 8, 2 | 1, 5000, 8);
    sec(&mut b, 3, 7, 2, 20, 1);
    let l = plan_page_layout(&b, 0x10000, PAGE * 4, 3).unwrap();
    assert_eq!(l.page_size(), Some(PAGE));
    assert_eq!(l.span(), PAGE * 4);
    assert_eq!(
        l.placements().iter().map(|p| p.address).collect::<Vec<_>>(),
        [0x10000, 0x11000, 0x13000]
    );
}

#[test]
fn charges_initial_and_tail_padding_and_accepts_exact_budget_only() {
    let mut b = fixture(2);
    sec(&mut b, 1, 1, 2, 1, 1);
    let l = plan_page_layout(&b, PAGE, PAGE, 1).unwrap();
    assert_eq!(l.span(), PAGE);
    assert_eq!(
        plan_page_layout(&b, PAGE, PAGE - 1, 1),
        Err(Error::ResourceLimit)
    );
    assert_eq!(
        plan_page_layout(&b, PAGE + 1, 0, 1),
        Err(Error::PageAlignment)
    );
}

#[test]
fn honors_section_alignment_larger_than_page() {
    let mut b = fixture(3);
    sec(&mut b, 1, 8, 2, 1, 1);
    sec(&mut b, 2, 8, 2, 1, 8192);
    let l = plan_page_layout(&b, PAGE, PAGE * 3, 2).unwrap();
    assert_eq!(l.placements()[0].address, PAGE);
    assert_eq!(l.placements()[1].address, PAGE * 2);
    assert_eq!(l.span(), PAGE * 2);
}

#[test]
fn rejects_writable_executable_alloc_sections_even_when_empty() {
    for size in [0, 1] {
        let mut b = fixture(2);
        sec(&mut b, 1, 1, 2 | 1 | 4, size, 1);
        assert_eq!(
            plan_page_layout(&b, 0, 0, 1),
            Err(Error::WritableExecutableSection)
        );
    }
}

#[test]
fn ignores_nonalloc_writable_executable_section() {
    let mut b = fixture(2);
    sec(&mut b, 1, 1, 1 | 4, 0, 1);
    let l = plan_page_layout(&b, 0, 0, 0).unwrap();
    assert_eq!(l.span(), 0);
    assert!(l.placements().is_empty());
}

#[test]
fn zero_sized_alloc_sections_consume_no_pages_and_may_share_addresses() {
    let mut b = fixture(4);
    sec(&mut b, 1, 1, 2, 0, 1);
    sec(&mut b, 2, 7, 2, 0, 8);
    sec(&mut b, 3, 8, 2, 0, 1);
    let l = plan_page_layout(&b, PAGE, 0, 3).unwrap();
    assert_eq!(l.span(), 0);
    assert!(l.placements().iter().all(|p| p.address == PAGE));
}

#[test]
fn enforces_section_and_span_caps() {
    let mut b = fixture(3);
    sec(&mut b, 1, 8, 2, 1, 1);
    sec(&mut b, 2, 8, 2, 1, 1);
    assert_eq!(
        plan_page_layout(&b, 0, PAGE * 2, 1),
        Err(Error::ResourceLimit)
    );
    assert_eq!(plan_page_layout(&b, 0, PAGE, 2), Err(Error::ResourceLimit));
}

#[test]
fn checked_alignment_end_and_tail_rounding_overflows() {
    let mut b = fixture(2);
    sec(&mut b, 1, 8, 2, 0, 8192);
    assert_eq!(
        plan_page_layout(&b, u64::MAX - PAGE + 1, u64::MAX, 1),
        Err(Error::LayoutOverflow)
    );
    sec(&mut b, 1, 8, 2, PAGE, 1);
    assert_eq!(
        plan_page_layout(&b, u64::MAX - PAGE + 1, u64::MAX, 1),
        Err(Error::LayoutOverflow)
    );
    sec(&mut b, 1, 8, 2, 1, 1);
    assert_eq!(
        plan_page_layout(&b, u64::MAX - PAGE + 1, u64::MAX, 1),
        Err(Error::LayoutOverflow)
    );
}

#[test]
fn compact_layout_policy_remains_compact_and_page_base_is_required() {
    let mut b = fixture(2);
    sec(&mut b, 1, 8, 2, 1, 1);
    let compact = plan_layout(&b, 0, PAGE, 1).unwrap();
    assert_eq!(compact.page_size(), None);
    let page = plan_page_layout(&b, 0, PAGE, 1).unwrap();
    assert_eq!(page.page_size(), Some(PAGE));
    assert_eq!(
        plan_page_layout(&b, PAGE - 1, u64::MAX, 1),
        Err(Error::PageAlignment)
    );
}

#[test]
fn resolver_and_prepare_module_preserve_the_page_policy() {
    let mut b = fixture(2);
    sec(&mut b, 1, 8, 2, 0, 1);
    let page = plan_page_layout(&b, 0, 0, 1).unwrap();
    let resolver = SymbolResolver::new(&b, &page, &[]).unwrap();
    assert_eq!(
        resolver.prepare_module(0, 0, 1).unwrap().sections().len(),
        1
    );
    assert_eq!(page.page_size(), Some(PAGE));
    let compact = plan_layout(&b, 0, 0, 1).unwrap();
    assert_eq!(
        SymbolResolver::new(&b, &compact, &[])
            .unwrap()
            .prepare_module(0, 0, 1)
            .unwrap()
            .sections()
            .len(),
        1
    );
}

#[test]
fn resolver_rejects_structurally_mismatched_page_layout() {
    let mut b = fixture(2);
    sec(&mut b, 1, 8, 2, 1, 1);
    let page = plan_page_layout(&b, 0, PAGE, 1).unwrap();
    let mut changed = b.clone();
    p64(&mut changed, 64 + 64 + 8, 0);
    assert_eq!(
        SymbolResolver::new(&changed, &page, &[]).err(),
        Some(Error::LayoutMismatch)
    );
}

#[test]
fn truncations_and_single_byte_mutations_never_panic() {
    let mut b = fixture(3);
    sec(&mut b, 1, 1, 2 | 4, 0, 1);
    sec(&mut b, 2, 8, 2 | 1, 0, 1);
    for end in 0..b.len() {
        let _ = plan_page_layout(&b[..end], 0, u64::MAX, 8);
    }
    for i in 0..b.len() {
        for byte in [0, 1, 0x7f, 0xff] {
            let mut m = b.clone();
            m[i] = byte;
            let _ = plan_page_layout(&m, 0, u64::MAX, 8);
        }
    }
}

#[test]
fn initial_alignment_gap_is_charged_even_before_the_first_payload() {
    let mut b = fixture(2);
    sec(&mut b, 1, 8, 2, 1, 4 * PAGE);
    let base = PAGE;
    let span = (4 * PAGE + PAGE) - base;
    let plan = plan_page_layout(&b, base, span, 1).unwrap();
    assert_eq!(plan.placements()[0].address, 4 * PAGE);
    assert_eq!(plan.span(), span);
    assert_eq!(
        plan_page_layout(&b, base, span - 1, 1),
        Err(Error::ResourceLimit)
    );
}

#[test]
fn nonempty_data_images_keep_paged_placements_without_becoming_mappings() {
    let mut b = fixture(3);
    sec(&mut b, 1, 1, 2 | 4, 1, 1);
    sec(&mut b, 2, 8, 2 | 1, 2, 1);
    let page = plan_page_layout(&b, PAGE, 2 * PAGE, 2).unwrap();
    let resolver = SymbolResolver::new(&b, &page, &[]).unwrap();
    let data = resolver.prepare_module(3, 0, 2).unwrap();
    assert_eq!(data.sections()[0].placement().address, PAGE);
    assert_eq!(data.sections()[1].placement().address, 2 * PAGE);
    assert_eq!(data.sections()[0].bytes(), &[0x7f]);
    assert_eq!(data.sections()[1].bytes(), &[0, 0]);
    assert_eq!(data.payload_bytes(), 3);
}

#[test]
fn equality_does_not_depend_on_unused_caller_budget_but_records_policy() {
    let mut b = fixture(2);
    sec(&mut b, 1, 8, 2, 1, 1);
    assert_eq!(
        plan_layout(&b, 0, u64::MAX, 64).unwrap(),
        plan_layout(&b, 0, 1, 1).unwrap()
    );
    assert_eq!(
        plan_page_layout(&b, 0, u64::MAX, 64).unwrap(),
        plan_page_layout(&b, 0, PAGE, 1).unwrap()
    );
    sec(&mut b, 1, 8, 2, 0, 1);
    assert_ne!(
        plan_page_layout(&b, 0, 0, 1).unwrap(),
        plan_layout(&b, 0, 0, 1).unwrap()
    );
}
