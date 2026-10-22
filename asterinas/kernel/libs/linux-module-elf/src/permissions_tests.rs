// SPDX-License-Identifier: MPL-2.0
use std::vec;
use std::vec::Vec;

use crate::{
    Error, IntendedPagePermission as P, plan_layout, plan_page_layout, plan_page_permissions,
};

fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn fixture(sections: &[(u64, u64, u64)]) -> Vec<u8> {
    let mut b = vec![0; 64 + (sections.len() + 1) * 64];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    put16(&mut b, 16, 1);
    put16(&mut b, 18, 62);
    put32(&mut b, 20, 1);
    put64(&mut b, 40, 64);
    put16(&mut b, 52, 64);
    put16(&mut b, 58, 64);
    put16(&mut b, 60, (sections.len() + 1) as u16);
    for (i, (flags, size, align)) in sections.iter().enumerate() {
        let o = 128 + i * 64;
        put32(&mut b, o + 4, 8);
        put64(&mut b, o + 8, *flags | 2);
        put64(&mut b, o + 24, u64::MAX);
        put64(&mut b, o + 32, *size);
        put64(&mut b, o + 48, *align);
    }
    b
}
fn compact(spec: &[(u64, u64, u64)], base: u64) -> crate::Layout {
    plan_layout(&fixture(spec), base, u64::MAX, spec.len()).unwrap()
}

#[test]
fn gap_and_read_only_page() {
    let p = plan_page_permissions(&compact(&[(0, 3, 1)], 4096), 2).unwrap();
    assert_eq!(p.pages(), [P::ReadOnly]);
}
#[test]
fn writable_page_is_read_write() {
    let p = plan_page_permissions(&compact(&[(1, 3, 1)], 0), 1).unwrap();
    assert_eq!(p.pages(), [P::ReadWrite]);
}
#[test]
fn executable_page_is_read_execute() {
    let p = plan_page_permissions(&compact(&[(4, 1, 1)], 0), 1).unwrap();
    assert_eq!(p.pages(), [P::ReadExecute]);
}
#[test]
fn empty_span_has_no_pages() {
    let p = plan_page_permissions(&compact(&[], 0), 0).unwrap();
    assert_eq!(p.span(), 0);
    assert!(p.pages().is_empty());
}
#[test]
fn fixed_page_layout_leaves_full_gap_inaccessible() {
    let l = plan_page_layout(&fixture(&[(0, 1, 1), (4, 1, 1)]), 0, 8192, 2).unwrap();
    let p = plan_page_permissions(&l, 2).unwrap();
    assert_eq!(p.pages(), [P::ReadOnly, P::ReadExecute]);
}
#[test]
fn exact_page_budget_succeeds_and_exhausted_fails() {
    let l = compact(&[(0, 1, 1)], 0);
    assert_eq!(plan_page_permissions(&l, 1).unwrap().page_count(), 1);
    assert_eq!(plan_page_permissions(&l, 0), Err(Error::ResourceLimit));
}
#[test]
fn rejects_unaligned_base() {
    assert_eq!(
        plan_page_permissions(&compact(&[], 1), 1),
        Err(Error::PageAlignment)
    );
}
#[test]
fn rejects_rounded_end_overflow() {
    assert_eq!(
        plan_page_permissions(&compact(&[(0, 1, 1)], u64::MAX - 4095), 1),
        Err(Error::LayoutOverflow)
    );
}
#[test]
fn compact_rw_rx_same_page_fails_wx() {
    let l = compact(&[(1, 1, 1), (4, 1, 1)], 0);
    assert_eq!(
        plan_page_permissions(&l, 2),
        Err(Error::WritableExecutablePage)
    );
}
#[test]
fn compact_ro_rx_same_page_conflicts() {
    let l = compact(&[(0, 1, 1), (4, 1, 1)], 0);
    assert_eq!(
        plan_page_permissions(&l, 2),
        Err(Error::PermissionPageConflict)
    );
}
#[test]
fn compact_ro_rw_same_page_conflicts() {
    let l = compact(&[(0, 1, 1), (1, 1, 1)], 0);
    assert_eq!(
        plan_page_permissions(&l, 2),
        Err(Error::PermissionPageConflict)
    );
}
#[test]
fn same_class_sections_merge_in_page() {
    let l = compact(&[(0, 1, 1), (0, 1, 1)], 0);
    assert_eq!(plan_page_permissions(&l, 1).unwrap().pages(), [P::ReadOnly]);
}
#[test]
fn empty_wx_section_is_rejected_by_page_layout() {
    assert_eq!(
        plan_page_layout(&fixture(&[(5, 0, 1)]), 0, 0, 1),
        Err(Error::WritableExecutableSection)
    );
}
#[test]
fn large_budget_does_not_expand_empty_plan() {
    assert_eq!(
        plan_page_permissions(&compact(&[], 0), usize::MAX)
            .unwrap()
            .pages(),
        []
    );
}
#[test]
fn planning_does_not_mutate_source_fixture() {
    let b = fixture(&[(0, 4, 1)]);
    let before = b.clone();
    let l = plan_layout(&b, 0, u64::MAX, 1).unwrap();
    let _ = plan_page_permissions(&l, 1).unwrap();
    assert_eq!(b, before);
}
