// SPDX-License-Identifier: MPL-2.0
//! Conservative page-span bound tests; these operate on metadata only.
use std::{vec, vec::Vec};

use crate::{Error, page_layout_span_bound, plan_page_layout};

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
fn page_only_bound_matches_span_and_enforces_exact_budget() {
    let mut b = fixture(2);
    sec(&mut b, 1, 8, 2, PAGE, 1);
    let plan = plan_page_layout(&b, 0, u64::MAX, 1).unwrap();
    assert_eq!(page_layout_span_bound(&b, u64::MAX, 1), Ok(plan.span()));
    assert_eq!(page_layout_span_bound(&b, plan.span(), 1), Ok(plan.span()));
    assert_eq!(
        page_layout_span_bound(&b, plan.span() - 1, 1),
        Err(Error::ResourceLimit)
    );
}

#[test]
fn bound_covers_swept_page_aligned_bases_with_large_and_empty_alignments() {
    let mut b = fixture(5);
    sec(&mut b, 1, 8, 2, 7, 1);
    sec(&mut b, 2, 8, 2, 0, 16 * 1024);
    sec(&mut b, 3, 8, 2, 5001, 64 * 1024);
    sec(&mut b, 4, 8, 2, 1, 8192);
    let bound = page_layout_span_bound(&b, u64::MAX, 4).unwrap();
    for base in (0..64 * 1024).step_by(PAGE as usize) {
        let layout = plan_page_layout(&b, base, u64::MAX, 4).unwrap();
        assert!(
            layout.span() <= bound,
            "base={base:#x}: span={} bound={bound}",
            layout.span()
        );
    }
}

#[test]
fn preserves_validation_limits_types_tls_wx_and_alignment() {
    let mut b = fixture(3);
    sec(&mut b, 1, 1, 2, 1, 1);
    sec(&mut b, 2, 1, 2, 1, 1);
    assert_eq!(
        page_layout_span_bound(&b, u64::MAX, 1),
        Err(Error::ResourceLimit)
    );
    assert_eq!(
        page_layout_span_bound(&b, PAGE, 2),
        Err(Error::ResourceLimit)
    );
    sec(&mut b, 1, 3, 2, 0, 1);
    assert_eq!(
        page_layout_span_bound(&b, u64::MAX, 2),
        Err(Error::UnsupportedSection)
    );
    sec(&mut b, 1, 1, 2 | 0x400, 0, 1);
    assert_eq!(
        page_layout_span_bound(&b, u64::MAX, 2),
        Err(Error::UnsupportedSection)
    );
    sec(&mut b, 1, 1, 2 | 1 | 4, 0, 1);
    assert_eq!(
        page_layout_span_bound(&b, u64::MAX, 2),
        Err(Error::WritableExecutableSection)
    );
    sec(&mut b, 1, 1, 2, 0, 3);
    assert_eq!(
        page_layout_span_bound(&b, u64::MAX, 2),
        Err(Error::SectionAlignment)
    );
}

#[test]
fn zero_alloc_and_empty_alloc_bound_are_zero() {
    let mut b = fixture(3);
    sec(&mut b, 1, 1, 0, 12, 1);
    sec(&mut b, 2, 8, 2, 0, 1);
    assert_eq!(page_layout_span_bound(&b, 0, 1), Ok(0));
    sec(&mut b, 2, 8, 0, 0, 1);
    assert_eq!(page_layout_span_bound(&b, 0, 0), Ok(0));
}

#[test]
fn checked_bound_accumulation_and_rounded_size_overflow() {
    let mut b = fixture(4);
    for i in 1..4 {
        sec(&mut b, i, 8, 2, 0, 1u64 << 63);
    }
    // The base-zero page planner succeeds; aggregate worst-case gaps overflow.
    assert!(plan_page_layout(&b, 0, u64::MAX, 3).is_ok());
    assert_eq!(
        page_layout_span_bound(&b, u64::MAX, 3),
        Err(Error::LayoutOverflow)
    );
    let mut huge = fixture(2);
    sec(&mut huge, 1, 8, 2, u64::MAX, 1);
    assert_eq!(
        page_layout_span_bound(&huge, u64::MAX, 1),
        Err(Error::LayoutOverflow)
    );
}
