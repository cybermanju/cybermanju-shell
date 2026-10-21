// SPDX-License-Identifier: MPL-2.0
//! Synthetic multi-table transactions, never executable module tests.
use crate::{Error, Export, SymbolResolver, plan_layout};
use std::{vec, vec::Vec};

const STRINGS: usize = 576; // Space for the header and eight section headers.
const SYMBOLS: usize = 608;
const ENTRIES1: usize = 680;
const ENTRIES2: usize = 704;
const DATA: usize = 728;
const STRINGS2: usize = 744;
const SYMBOLS2: usize = 760;
fn h(index: usize) -> usize {
    64 + index * 64
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
fn sec(b: &mut [u8], index: usize, kind: u32, flags: u64, offset: usize, size: usize) {
    let s = h(index);
    p32(b, s + 4, kind);
    p64(b, s + 8, flags);
    p64(b, s + 24, offset as u64);
    p64(b, s + 32, size as u64);
}
fn symtab(b: &mut [u8], index: usize, offset: usize, count: usize, strings: u32) {
    sec(b, index, 2, 0, offset, count * 24);
    p32(b, h(index) + 40, strings);
    p32(b, h(index) + 44, 1);
    p64(b, h(index) + 48, 8);
    p64(b, h(index) + 56, 24);
}
fn entry(b: &mut [u8], at: usize, offset: u64, symbol: u32, kind: u32, addend: i64) {
    p64(b, at, offset);
    p64(b, at + 8, (u64::from(symbol) << 32) | u64::from(kind));
    p64(b, at + 16, addend as u64);
}
fn fixture() -> Vec<u8> {
    // null, alloc data, strings, symtab, RELA1, RELA2; two optional headers.
    let mut b = vec![0; 832];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    p16(&mut b, 16, 1);
    p16(&mut b, 18, 62);
    p32(&mut b, 20, 1);
    p64(&mut b, 40, 64);
    p16(&mut b, 52, 64);
    p16(&mut b, 58, 64);
    p16(&mut b, 60, 6);
    sec(&mut b, 1, 1, 2, DATA, 16);
    p64(&mut b, h(1) + 48, 8);
    sec(&mut b, 2, 3, 0, STRINGS, 17);
    b[STRINGS..STRINGS + 17].copy_from_slice(b"\0defined\0missing\0");
    symtab(&mut b, 3, SYMBOLS, 3, 2);
    p32(&mut b, SYMBOLS + 24, 1);
    b[SYMBOLS + 28] = 0x10;
    p32(&mut b, SYMBOLS + 48, 9);
    b[SYMBOLS + 52] = 0x10;
    for (index, offset) in [(4, ENTRIES1), (5, ENTRIES2)] {
        sec(&mut b, index, 4, 0, offset, 24);
        p32(&mut b, h(index) + 40, 3);
        p32(&mut b, h(index) + 44, 1);
        p64(&mut b, h(index) + 48, 8);
        p64(&mut b, h(index) + 56, 24);
    }
    entry(&mut b, ENTRIES1, 0, 1, 1, 0);
    entry(&mut b, ENTRIES2, 8, 1, 1, 0);
    b[DATA..DATA + 16].fill(0xaa);
    crate::inspect(&b).expect("synthetic fixture must be valid before mutation");
    b
}
fn run(
    b: &[u8],
    exports: &[Export<'_>],
    max_bytes: usize,
    max_entries: usize,
) -> Result<Vec<u8>, Error> {
    let layout = plan_layout(b, 0x1000, 16, 1)?;
    SymbolResolver::new(b, &layout, exports)?.relocate_target(1, max_bytes, max_entries)
}
fn exports() -> [Export<'static>; 1] {
    [Export {
        name: b"defined",
        value: 0x1234,
    }]
}

#[test]
fn applies_distinct_writes_from_both_tables() {
    let b = fixture();
    let before = b.clone();
    let out = run(&b, &exports(), 16, 2).unwrap();
    assert_eq!(&out[..8], &0x1234_u64.to_le_bytes());
    assert_eq!(&out[8..], &0x1234_u64.to_le_bytes());
    assert_eq!(b, before);
}
#[test]
fn detects_overlap_between_tables() {
    let mut b = fixture();
    entry(&mut b, ENTRIES2, 4, 1, 1, 0);
    assert_eq!(run(&b, &exports(), 16, 2), Err(Error::RelocationOverlap));
}
#[test]
fn aggregate_budget_counts_none_and_enforces_hard_cap() {
    let mut b = fixture();
    entry(&mut b, ENTRIES1, 0, 0, 0, 0);
    entry(&mut b, ENTRIES2, 0, 0, 0, 0);
    assert_eq!(run(&b, &[], 16, 1), Err(Error::ResourceLimit));
    assert_eq!(run(&b, &[], 16, 2), Ok(vec![0xaa; 16]));
    let start = b.len();
    b.resize(start + 65_536 * 24, 0);
    p64(&mut b, h(4) + 24, start as u64);
    p64(&mut b, h(4) + 32, 65_536 * 24);
    assert_eq!(run(&b, &[], 16, usize::MAX), Err(Error::ResourceLimit));
}
#[test]
fn second_table_missing_export_keeps_input_unchanged() {
    let mut b = fixture();
    entry(&mut b, ENTRIES2, 8, 2, 1, 0);
    let before = b.clone();
    assert_eq!(run(&b, &exports(), 16, 2), Err(Error::MissingSymbol));
    assert_eq!(b, before);
}
#[test]
fn second_table_overflow_fails_without_exposing_partial_copy() {
    let mut b = fixture();
    entry(&mut b, ENTRIES2, 8, 1, 1, 1);
    let before = b.clone();
    let huge = [Export {
        name: b"defined",
        value: u64::MAX,
    }];
    assert_eq!(run(&b, &huge, 16, 2), Err(Error::RelocationOverflow));
    assert_eq!(b, before);
}
#[test]
fn nobits_target_starts_zeroed() {
    let mut b = fixture();
    sec(&mut b, 1, 8, 2, 0, 16);
    p64(&mut b, h(1) + 24, u64::MAX);
    entry(&mut b, ENTRIES2, 8, 0, 0, 0);
    let out = run(&b, &exports(), 16, 2).unwrap();
    assert_eq!(&out[..8], &0x1234_u64.to_le_bytes());
    assert_eq!(&out[8..], &[0; 8]);
}
#[test]
fn target_without_rela_copies_entire_progbits_payload() {
    let mut b = fixture();
    p64(&mut b, h(4) + 32, 0);
    p64(&mut b, h(5) + 32, 0);
    assert_eq!(run(&b, &[], 16, 0), Ok(vec![0xaa; 16]));
}
#[test]
fn rejects_invalid_nonalloc_and_note_targets() {
    let b = fixture();
    let layout = plan_layout(&b, 0x1000, 16, 1).unwrap();
    let r = SymbolResolver::new(&b, &layout, &[]).unwrap();
    assert_eq!(r.relocate_target(0, 16, 2), Err(Error::RelocationTarget));
    assert_eq!(
        r.relocate_target(u16::MAX, 16, 2),
        Err(Error::RelocationTarget)
    );
    let mut noalloc = fixture();
    p64(&mut noalloc, h(1) + 8, 0);
    let l = plan_layout(&noalloc, 0x1000, 0, 0).unwrap();
    let r = SymbolResolver::new(&noalloc, &l, &[]).unwrap();
    assert_eq!(r.relocate_target(1, 16, 2), Err(Error::RelocationTarget));
    let mut note = fixture();
    p32(&mut note, h(1) + 4, 7);
    let l = plan_layout(&note, 0x1000, 16, 1).unwrap();
    let r = SymbolResolver::new(&note, &l, &[]).unwrap();
    assert_eq!(r.relocate_target(1, 16, 2), Err(Error::RelocationTarget));
}
#[test]
fn enforces_target_byte_limit() {
    assert_eq!(
        run(&fixture(), &exports(), 15, 2),
        Err(Error::ResourceLimit)
    );
}
#[test]
fn each_rela_uses_its_own_symbol_table() {
    let mut b = fixture();
    p16(&mut b, 60, 8);
    sec(&mut b, 6, 3, 0, STRINGS2, 16);
    b[STRINGS2..STRINGS2 + 16].copy_from_slice(b"\0defined\0other\0\0");
    symtab(&mut b, 7, SYMBOLS2, 2, 6);
    p32(&mut b, SYMBOLS2 + 24, 9);
    b[SYMBOLS2 + 28] = 0x10;
    p32(&mut b, h(5) + 40, 7);
    let exports = [
        Export {
            name: b"defined",
            value: 0x1234,
        },
        Export {
            name: b"other",
            value: 0x5678,
        },
    ];
    let out = run(&b, &exports, 16, 2).unwrap();
    assert_eq!(&out[..8], &0x1234_u64.to_le_bytes());
    assert_eq!(&out[8..], &0x5678_u64.to_le_bytes());
}
#[test]
fn target_api_truncations_and_mutations_do_not_panic() {
    let b = fixture();
    for end in 0..b.len() {
        let _ = run(&b[..end], &exports(), 16, 2);
    }
    for index in 0..b.len() {
        let mut m = b.clone();
        m[index] ^= 0xff;
        let _ = run(&m, &exports(), 16, 2);
    }
}
