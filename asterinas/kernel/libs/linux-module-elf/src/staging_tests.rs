// SPDX-License-Identifier: MPL-2.0
//! Public API coverage for data-only staging plans.
use std::{vec, vec::Vec};

use crate::{Error, SymbolResolver, plan_layout, plan_page_layout};

fn p16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn p32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn p64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn hdr(i: usize) -> usize {
    64 + i * 64
}
fn section(b: &mut [u8], i: usize, kind: u32, flags: u64, offset: u64, size: u64) {
    let h = hdr(i);
    p32(b, h + 4, kind);
    p64(b, h + 8, flags);
    p64(b, h + 24, offset);
    p64(b, h + 32, size);
    p64(b, h + 48, 1);
}
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 400];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    p16(&mut b, 16, 1);
    p16(&mut b, 18, 62);
    p32(&mut b, 20, 1);
    p64(&mut b, 40, 64);
    p16(&mut b, 52, 64);
    p16(&mut b, 58, 64);
    p16(&mut b, 60, 4);
    section(&mut b, 1, 1, 2, 384, 4);
    section(&mut b, 2, 7, 2, 388, 4);
    section(&mut b, 3, 8, 2, u64::MAX, 8);
    b[384..388].copy_from_slice(b"data");
    b[388..392].copy_from_slice(b"note");
    b
}
fn prepared(b: &[u8], layout: &crate::Layout) -> crate::PreparedModule {
    SymbolResolver::new(b, layout, &[])
        .unwrap()
        .prepare_module(32, 0, 3)
        .unwrap()
}

#[test]
fn preserves_progbits_note_and_nobits_chunks_and_empty_image() {
    let b = fixture();
    let layout = plan_page_layout(&b, 0x1000, 3 * 4096, 3).unwrap();
    let image = prepared(&b, &layout);
    let plan = image.plan_staging(&layout, 3, 3).unwrap();
    assert_eq!((plan.span(), plan.page_count()), (3 * 4096, 3));
    assert_eq!(plan.chunks().len(), 3);
    assert_eq!(plan.chunks()[0].bytes, b"data");
    assert_eq!(plan.chunks()[1].bytes, b"note");
    assert_eq!(plan.chunks()[2].bytes, &[0; 8]);
    assert_eq!(plan.chunks()[0].offset, 0);
    assert_eq!(plan.chunks()[1].offset, 4096);
    assert_eq!(plan.chunks()[2].offset, 8192);

    let mut no_alloc = fixture();
    for i in 1..=3 {
        p64(&mut no_alloc, hdr(i) + 8, 0);
    }
    let empty_layout = plan_page_layout(&no_alloc, 0x1000, 0, 0).unwrap();
    let empty_image = prepared(&no_alloc, &empty_layout);
    let empty_plan = empty_image.plan_staging(&empty_layout, 0, 0).unwrap();
    assert_eq!((empty_plan.span(), empty_plan.page_count()), (0, 0));
    assert!(empty_plan.chunks().is_empty());
}

#[test]
fn rejects_compact_or_different_layouts() {
    let b = fixture();
    let compact = plan_layout(&b, 0x1000, 4096, 3).unwrap();
    let image = prepared(&b, &compact);
    assert_eq!(
        image.plan_staging(&compact, 9, 9).err(),
        Some(Error::PageAlignment)
    );
    let pages = plan_page_layout(&b, 0x1000, 3 * 4096, 3).unwrap();
    let image = prepared(&b, &pages);
    let changed_base = plan_page_layout(&b, 0x2000, 3 * 4096, 3).unwrap();
    assert_eq!(
        image.plan_staging(&changed_base, 3, 3).err(),
        Some(Error::LayoutMismatch)
    );
    let mut changed = b.clone();
    p64(&mut changed, hdr(2) + 48, 8192);
    let changed_layout = plan_page_layout(&changed, 0x1000, 4 * 4096, 3).unwrap();
    assert_eq!(
        image.plan_staging(&changed_layout, 4, 3).err(),
        Some(Error::LayoutMismatch)
    );
}

#[test]
fn enforces_page_span_and_chunk_budgets_including_empty_chunks() {
    let b = fixture();
    let layout = plan_page_layout(&b, 0x1000, 3 * 4096, 3).unwrap();
    let image = prepared(&b, &layout);
    assert_eq!(
        image.plan_staging(&layout, 2, 3).err(),
        Some(Error::ResourceLimit)
    );
    assert_eq!(
        image.plan_staging(&layout, 3, 2).err(),
        Some(Error::ResourceLimit)
    );
    assert_eq!(
        image.plan_staging(&layout, 3, 0).err(),
        Some(Error::ResourceLimit)
    );

    let mut empty_note = fixture();
    p64(&mut empty_note, hdr(2) + 32, 0);
    let layout = plan_page_layout(&empty_note, 0x1000, 3 * 4096, 3).unwrap();
    let image = prepared(&empty_note, &layout);
    let plan = image.plan_staging(&layout, 2, 3).unwrap();
    assert_eq!((plan.page_count(), plan.chunks().len()), (2, 3));
    assert!(plan.chunks()[1].bytes.is_empty());
    assert_eq!(plan.chunks()[1].offset, plan.chunks()[2].offset);
    assert_eq!(
        image.plan_staging(&layout, 2, 2).err(),
        Some(Error::ResourceLimit)
    );
}

#[test]
fn rejects_more_than_4096_chunks_even_when_caller_budget_is_larger() {
    let count = 4097usize;
    let mut b = vec![0; 64 + (count + 1) * 64];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    p16(&mut b, 16, 1);
    p16(&mut b, 18, 62);
    p32(&mut b, 20, 1);
    p64(&mut b, 40, 64);
    p16(&mut b, 52, 64);
    p16(&mut b, 58, 64);
    p16(&mut b, 60, u16::try_from(count + 1).unwrap());
    // Empty ALLOC NOTE sections avoid quadratic relocation work in preparation.
    for i in 1..=count {
        section(&mut b, i, 7, 2, 0, 0);
    }
    let layout = plan_page_layout(&b, 0x1000, 0, count).unwrap();
    let image = SymbolResolver::new(&b, &layout, &[])
        .unwrap()
        .prepare_module(0, 0, count)
        .unwrap();
    assert_eq!(
        image.plan_staging(&layout, 0, usize::MAX).err(),
        Some(Error::ResourceLimit)
    );
}
