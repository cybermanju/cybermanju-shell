// SPDX-License-Identifier: MPL-2.0
//! Ownership-core tests for staged module pages; these deliberately do not
//! model mappings, execution, or physical page-table behavior.
use super::{ModuleChunk, Page, StagedPages};
use crate::error::Error;
use std::cell::Cell;
use std::rc::Rc;

const SIZE: usize = 4096;
#[derive(Default)]
struct Counts {
    alive: Cell<usize>,
    allocated: Cell<usize>,
    dropped: Cell<usize>,
    writes: Cell<usize>,
    fail_write: Cell<Option<usize>>,
    fail_read: Cell<bool>,
}
struct FakePage {
    data: Vec<u8>,
    counts: Rc<Counts>,
}
impl FakePage {
    fn new(counts: Rc<Counts>) -> Self {
        counts.alive.set(counts.alive.get() + 1);
        counts.allocated.set(counts.allocated.get() + 1);
        Self {
            data: vec![0; SIZE],
            counts,
        }
    }
}
impl Drop for FakePage {
    fn drop(&mut self) {
        self.counts.alive.set(self.counts.alive.get() - 1);
        self.counts.dropped.set(self.counts.dropped.get() + 1);
    }
}
impl Page for FakePage {
    const SIZE: usize = SIZE;
    fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        let n = self.counts.writes.get();
        self.counts.writes.set(n + 1);
        if self.counts.fail_write.get() == Some(n) {
            return Err(Error::IoError);
        }
        let end = offset.checked_add(bytes.len()).ok_or(Error::Overflow)?;
        if end > SIZE {
            return Err(Error::InvalidArgs);
        }
        self.data[offset..end].copy_from_slice(bytes);
        Ok(())
    }
    fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<(), Error> {
        if self.counts.fail_read.get() {
            return Err(Error::IoError);
        }
        let end = offset.checked_add(bytes.len()).ok_or(Error::Overflow)?;
        if end > SIZE {
            return Err(Error::InvalidArgs);
        }
        bytes.copy_from_slice(&self.data[offset..end]);
        Ok(())
    }
}
fn setup() -> (Rc<Counts>, impl FnMut() -> Result<FakePage, Error>) {
    let c = Rc::new(Counts::default());
    let c2 = c.clone();
    (c, move || Ok(FakePage::new(c2.clone())))
}

#[test]
fn copies_across_pages_and_preserves_zero_gaps_and_tail() {
    let (c, alloc) = setup();
    let a = vec![0x31; SIZE + 7];
    let b = [0x72, 0x73, 0x74];
    let chunks = [
        ModuleChunk {
            offset: 0,
            bytes: &a,
        },
        ModuleChunk {
            offset: SIZE * 2,
            bytes: &b,
        },
    ];
    let s = StagedPages::new(SIZE * 3, &chunks, 3, 2, alloc).unwrap();
    assert_eq!((s.span(), s.page_count()), (SIZE * 3, 3));
    let mut out = vec![9; SIZE * 3];
    s.read_bytes(0, &mut out).unwrap();
    assert_eq!(&out[..a.len()], &a);
    assert!(out[a.len()..SIZE * 2].iter().all(|x| *x == 0));
    assert_eq!(&out[SIZE * 2..SIZE * 2 + 3], &b);
    assert!(out[SIZE * 2 + 3..].iter().all(|x| *x == 0));
    assert_eq!(c.alive.get(), 3);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn ordered_exact_iterator_clones_borrow_pages_without_changing_ownership() {
    let (counts, alloc) = setup();
    let mut bytes = vec![0x31; SIZE * 2];
    bytes[SIZE..].fill(0xa9);
    let staged = StagedPages::new(
        SIZE * 2,
        &[ModuleChunk {
            offset: 0,
            bytes: &bytes,
        }],
        2,
        1,
        alloc,
    )
    .unwrap();
    let mut original = staged.iter();
    let mut cloned = original.clone();
    assert_eq!(original.len(), 2);
    assert_eq!(cloned.len(), 2);
    let first = original.next().unwrap();
    assert_eq!(first.data, bytes[..SIZE]);
    assert!(core::ptr::eq(first, cloned.next().unwrap()));
    assert_eq!(original.len(), 1);
    assert_eq!(cloned.len(), 1);
    let second = original.next().unwrap();
    assert_eq!(second.data, bytes[SIZE..]);
    assert!(core::ptr::eq(second, cloned.next().unwrap()));
    assert_eq!(original.len(), 0);
    assert_eq!(cloned.len(), 0);
    assert!(original.next().is_none());
    assert!(cloned.next().is_none());
    assert_eq!(counts.alive.get(), 2);
    assert_eq!(counts.dropped.get(), 0);
    drop(staged);
    assert_eq!(counts.alive.get(), 0);
    assert_eq!(counts.dropped.get(), 2);
}

#[test]
fn empty_span_and_empty_chunks() {
    let (c, alloc) = setup();
    let s = StagedPages::new(0, &[], 0, 0, alloc).unwrap();
    assert_eq!(s.page_count(), 0);
    drop(s);
    assert_eq!(c.allocated.get(), 0);
    let empty = [
        ModuleChunk {
            offset: 0,
            bytes: &[],
        },
        ModuleChunk {
            offset: 0,
            bytes: &[],
        },
    ];
    let (c, alloc) = setup();
    let s = StagedPages::new(SIZE, &empty, 1, 2, alloc).unwrap();
    assert_eq!(s.page_count(), 1);
    assert_eq!(c.allocated.get(), 1);
}
#[test]
fn invalid_span_alignment_and_chunk_count_are_rejected_before_allocation() {
    for (span, chunks, pages, maxc) in [(1, vec![], 2, 0)] {
        let (c, alloc) = setup();
        let r = StagedPages::<FakePage>::new(span, &chunks, pages, maxc, alloc);
        assert_eq!(r.err(), Some(Error::InvalidArgs));
        assert_eq!(c.allocated.get(), 0);
    }
    let (c, alloc) = setup();
    let one = [ModuleChunk {
        offset: 0,
        bytes: &[],
    }];
    assert_eq!(
        StagedPages::<FakePage>::new(SIZE, &one, 1, 0, alloc).err(),
        Some(Error::InvalidArgs)
    );
    assert_eq!(c.allocated.get(), 0);
}
#[test]
fn malformed_late_chunk_rejected_before_any_allocation() {
    let data = [1u8; 2];
    let chunks = [
        ModuleChunk {
            offset: 0,
            bytes: &data,
        },
        ModuleChunk {
            offset: 0,
            bytes: &data,
        },
    ];
    let (c, alloc) = setup();
    assert_eq!(
        StagedPages::<FakePage>::new(SIZE * 2, &chunks, 2, 2, alloc).err(),
        Some(Error::InvalidArgs)
    );
    assert_eq!(c.allocated.get(), 0);
}
#[test]
fn validates_offsets_ranges_order_reserved_page_overlap_and_budgets() {
    let data = [1u8; 4];
    let cases: Vec<(usize, Vec<ModuleChunk<'_>>, usize, usize)> = vec![
        (
            SIZE,
            vec![ModuleChunk {
                offset: 1,
                bytes: &data,
            }],
            3,
            1,
        ),
        (
            SIZE,
            vec![ModuleChunk {
                offset: SIZE,
                bytes: &data,
            }],
            3,
            1,
        ),
        (
            SIZE * 2,
            vec![ModuleChunk {
                offset: 0,
                bytes: &[0u8; SIZE * 2 + 1],
            }],
            3,
            1,
        ),
        (
            SIZE * 3,
            vec![
                ModuleChunk {
                    offset: SIZE,
                    bytes: &data,
                },
                ModuleChunk {
                    offset: 0,
                    bytes: &data,
                },
            ],
            3,
            2,
        ),
        (
            SIZE * 2,
            vec![
                ModuleChunk {
                    offset: 0,
                    bytes: &data,
                },
                ModuleChunk {
                    offset: 0,
                    bytes: &data,
                },
            ],
            3,
            2,
        ),
        (SIZE * 2, vec![], 1, 0),
    ];
    for (span, chunks, max_pages, max_chunks) in cases {
        let (c, alloc) = setup();
        assert_eq!(
            StagedPages::<FakePage>::new(span, &chunks, max_pages, max_chunks, alloc).err(),
            Some(Error::InvalidArgs)
        );
        assert_eq!(c.allocated.get(), 0);
    }
}
#[test]
fn all_allocation_failure_positions_drop_every_acquired_page_once() {
    for fail_at in 0..4 {
        let (c, _) = setup();
        let c2 = c.clone();
        let mut idx = 0;
        let r = StagedPages::<FakePage>::new(SIZE * 4, &[], 4, 0, move || {
            let i = idx;
            idx += 1;
            if i == fail_at {
                return Err(Error::NoMemory);
            }
            Ok(FakePage::new(c2.clone()))
        });
        assert_eq!(r.err(), Some(Error::NoMemory));
        assert_eq!(c.allocated.get(), fail_at);
        assert_eq!(c.dropped.get(), fail_at);
        assert_eq!(c.alive.get(), 0);
    }
}
#[test]
fn write_failure_drops_all_preallocated_pages() {
    let (c, alloc) = setup();
    c.fail_write.set(Some(1));
    let data = vec![0x55; SIZE + 2];
    let chunks = [ModuleChunk {
        offset: 0,
        bytes: &data,
    }];
    assert_eq!(
        StagedPages::<FakePage>::new(SIZE * 2, &chunks, 2, 1, alloc).err(),
        Some(Error::IoError)
    );
    assert_eq!(c.allocated.get(), 2);
    assert_eq!(c.dropped.get(), 2);
    assert_eq!(c.alive.get(), 0);
}
#[test]
fn dropping_successful_storage_releases_all_pages_once() {
    let (c, alloc) = setup();
    let s = StagedPages::<FakePage>::new(SIZE * 3, &[], 3, 0, alloc).unwrap();
    assert_eq!(c.alive.get(), 3);
    drop(s);
    assert_eq!(c.dropped.get(), 3);
    assert_eq!(c.alive.get(), 0);
}
#[test]
fn input_chunks_are_read_only_and_unchanged() {
    let data = [3u8, 4, 5];
    let before = data;
    let chunks = [ModuleChunk {
        offset: 0,
        bytes: &data,
    }];
    let (c, alloc) = setup();
    let _s = StagedPages::new(SIZE, &chunks, 1, 1, alloc).unwrap();
    assert_eq!(data, before);
    assert_eq!(chunks[0].bytes, &before);
    assert_eq!(c.allocated.get(), 1);
}
#[test]
fn read_bounds_and_arithmetic_overflow_are_checked() {
    let (c, alloc) = setup();
    let s = StagedPages::new(SIZE, &[], 1, 0, alloc).unwrap();
    assert_eq!(s.read_bytes(SIZE, &mut [0; 1]), Err(Error::InvalidArgs));
    assert_eq!(s.read_bytes(usize::MAX, &mut [0; 2]), Err(Error::Overflow));
    assert_eq!(s.read_bytes(0, &mut []), Ok(()));
    drop(s);
    assert_eq!(c.alive.get(), 0);
}
#[test]
fn read_backend_error_propagates() {
    let (c, alloc) = setup();
    let s = StagedPages::new(SIZE, &[], 1, 0, alloc).unwrap();
    c.fail_read.set(true);
    assert_eq!(s.read_bytes(0, &mut [7; 2]), Err(Error::IoError));
}

#[test]
fn constructor_overflow_is_rejected_before_reserving_or_allocating() {
    let (counts, allocate) = setup();
    let span = usize::MAX & !(SIZE - 1);
    let bytes = vec![1; SIZE * 2];
    let chunks = [ModuleChunk {
        offset: span,
        bytes: &bytes,
    }];
    let error = StagedPages::<FakePage>::new(span, &chunks, span / SIZE, 1, allocate).err();
    assert_eq!(error, Some(Error::Overflow));
    assert_eq!(counts.allocated.get(), 0);
}

struct InvalidSize<const N: usize>;
impl<const N: usize> Page for InvalidSize<N> {
    const SIZE: usize = N;
    fn write(&mut self, _: usize, _: &[u8]) -> Result<(), Error> {
        Err(Error::IoError)
    }
    fn read(&self, _: usize, _: &mut [u8]) -> Result<(), Error> {
        Err(Error::IoError)
    }
}

#[test]
fn invalid_backend_page_size_cannot_divide_by_zero_or_allocate() {
    let calls = Cell::new(0);
    let zero = StagedPages::<InvalidSize<0>>::new(0, &[], 0, 0, || {
        calls.set(calls.get() + 1);
        Ok(InvalidSize::<0>)
    });
    assert_eq!(zero.err(), Some(Error::InvalidArgs));
    let not_power_two = StagedPages::<InvalidSize<3>>::new(3, &[], 1, 0, || {
        calls.set(calls.get() + 1);
        Ok(InvalidSize::<3>)
    });
    assert_eq!(not_power_two.err(), Some(Error::InvalidArgs));
    assert_eq!(calls.get(), 0);
}

#[test]
fn all_write_failure_positions_drop_all_pages_acquired_before_first_copy() {
    let data = vec![0x41; SIZE * 3 + 1];
    for fail_at in 0..4 {
        let (counts, allocate) = setup();
        counts.fail_write.set(Some(fail_at));
        let chunks = [ModuleChunk {
            offset: 0,
            bytes: &data,
        }];
        let error = StagedPages::<FakePage>::new(SIZE * 4, &chunks, 4, 1, allocate).err();
        assert_eq!(error, Some(Error::IoError));
        assert_eq!(
            counts.allocated.get(),
            4,
            "even failure on first copy must acquire all pages first"
        );
        assert_eq!(counts.dropped.get(), 4);
        assert_eq!(counts.alive.get(), 0);
        assert_eq!(counts.writes.get(), fail_at + 1);
    }
}

#[test]
fn empty_chunks_can_share_or_revisit_addresses_but_stay_within_span() {
    let (counts, allocate) = setup();
    let chunks = [
        ModuleChunk {
            offset: SIZE,
            bytes: &[7],
        },
        ModuleChunk {
            offset: 0,
            bytes: &[],
        },
        ModuleChunk {
            offset: SIZE * 3,
            bytes: &[],
        },
        ModuleChunk {
            offset: SIZE * 2,
            bytes: &[8],
        },
    ];
    let staged = StagedPages::new(SIZE * 3, &chunks, 3, 4, allocate).unwrap();
    let mut out = [0; 1];
    staged.read_bytes(SIZE, &mut out).unwrap();
    assert_eq!(out, [7]);
    staged.read_bytes(SIZE * 2, &mut out).unwrap();
    assert_eq!(out, [8]);
    assert_eq!(staged.read_bytes(SIZE * 3, &mut []), Ok(()));
    drop(staged);
    assert_eq!(counts.alive.get(), 0);
    let (counts, allocate) = setup();
    let outside = [ModuleChunk {
        offset: SIZE * 2,
        bytes: &[],
    }];
    assert_eq!(
        StagedPages::<FakePage>::new(SIZE, &outside, 1, 1, allocate).err(),
        Some(Error::InvalidArgs)
    );
    assert_eq!(counts.allocated.get(), 0);
}
