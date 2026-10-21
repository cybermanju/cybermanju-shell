// SPDX-License-Identifier: MPL-2.0
//! Real-frame integration tests for staging. These tests require initialized
//! OSTD allocation facilities; they do not create mappings or execute bytes.
#![forbid(unsafe_code)]

use super::super::{ModuleChunk, ModuleStaging};
use super::{Page, StagedPages};
use crate::{
    Error,
    mm::{Frame, FrameAllocOptions, PAGE_SIZE, VmIo, module_staging_test_meta::TrackedMetadata},
    prelude::*,
};
use alloc::vec;
use core::sync::atomic::{AtomicUsize, Ordering};

struct TrackedPage(Frame<TrackedMetadata>);

impl TrackedPage {
    fn allocate(counter: Arc<AtomicUsize>) -> Result<Self, Error> {
        Ok(Self(
            FrameAllocOptions::new()
                .zeroed(true)
                .alloc_frame_with(TrackedMetadata::new(counter))?,
        ))
    }
}

impl Page for TrackedPage {
    const SIZE: usize = PAGE_SIZE;
    fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        self.0.write_bytes(offset, bytes)
    }
    fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<(), Error> {
        self.0.read_bytes(offset, bytes)
    }
}

fn real_wrapper_roundtrip_zero_gaps_tail_and_drop() {
    let first: Vec<u8> = (0..PAGE_SIZE + 13).map(|i| (i % 251) as u8).collect();
    let second = [0xa1, 0xb2, 0xc3, 0xd4, 0xe5];
    let chunks = [
        ModuleChunk {
            offset: 0,
            bytes: &first,
        },
        ModuleChunk {
            offset: PAGE_SIZE * 3,
            bytes: &second,
        },
    ];
    {
        let staged = ModuleStaging::new(PAGE_SIZE * 5, &chunks, 5, 2).unwrap();
        assert_eq!(staged.span(), PAGE_SIZE * 5);
        assert_eq!(staged.page_count(), 5);
        let mut readback = vec![0xff; PAGE_SIZE * 5];
        staged.read_bytes(0, &mut readback).unwrap();
        assert_eq!(&readback[..first.len()], &first);
        assert!(readback[first.len()..PAGE_SIZE * 3].iter().all(|b| *b == 0));
        assert_eq!(
            &readback[PAGE_SIZE * 3..PAGE_SIZE * 3 + second.len()],
            &second
        );
        assert!(
            readback[PAGE_SIZE * 3 + second.len()..]
                .iter()
                .all(|b| *b == 0)
        );
    }
    println!("CYBERMANJU_STAGING_RUNTIME:real_wrapper_roundtrip_zero_gaps_tail_and_drop:PASS");
}

fn empty_and_invalid_budget() {
    let empty = ModuleStaging::new(0, &[], 0, 0).unwrap();
    assert_eq!((empty.span(), empty.page_count()), (0, 0));
    let chunk = ModuleChunk {
        offset: 0,
        bytes: &[1],
    };
    assert!(matches!(
        ModuleStaging::new(PAGE_SIZE, &[chunk], 0, 1),
        Err(Error::InvalidArgs)
    ));
    assert!(matches!(
        ModuleStaging::new(PAGE_SIZE * 2, &[], 1, 0),
        Err(Error::InvalidArgs)
    ));
    println!("CYBERMANJU_STAGING_RUNTIME:empty_and_invalid_budget:PASS");
}

fn allocation_failure_drops_all_acquired_real_frames() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut attempts = 0;
    let result = StagedPages::<TrackedPage>::new(PAGE_SIZE * 4, &[], 4, 0, || {
        if attempts == 2 {
            return Err(Error::NoMemory);
        }
        attempts += 1;
        TrackedPage::allocate(dropped.clone())
    });
    assert!(matches!(result, Err(Error::NoMemory)));
    assert_eq!(attempts, 2);
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
    println!("CYBERMANJU_STAGING_RUNTIME:allocation_failure_drops_all_acquired_real_frames:PASS");
}

struct FailFirstWrite(TrackedPage);
impl Page for FailFirstWrite {
    const SIZE: usize = PAGE_SIZE;
    fn write(&mut self, _offset: usize, _bytes: &[u8]) -> Result<(), Error> {
        Err(Error::IoError)
    }
    fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<(), Error> {
        self.0.read(offset, bytes)
    }
}

fn copy_failure_drops_every_frame_acquired_before_copy() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let bytes = [0x5a];
    let chunks = [ModuleChunk {
        offset: 0,
        bytes: &bytes,
    }];
    let mut allocations = 0;
    let result = StagedPages::<FailFirstWrite>::new(PAGE_SIZE * 4, &chunks, 4, 1, || {
        allocations += 1;
        Ok(FailFirstWrite(TrackedPage::allocate(dropped.clone())?))
    });
    assert!(matches!(result, Err(Error::IoError)));
    assert_eq!(
        allocations, 4,
        "all frames must be acquired before first copy"
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 4);
    println!("CYBERMANJU_STAGING_RUNTIME:copy_failure_drops_every_frame_acquired_before_copy:PASS");
}

fn metadata_drop_counter_stays_zero_while_owned_then_counts_all_frames() {
    let dropped = Arc::new(AtomicUsize::new(0));
    {
        let pages = StagedPages::<TrackedPage>::new(PAGE_SIZE * 4, &[], 4, 0, || {
            TrackedPage::allocate(dropped.clone())
        })
        .unwrap();
        assert_eq!(pages.page_count(), 4);
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
    }
    assert_eq!(dropped.load(Ordering::SeqCst), 4);
    println!(
        "CYBERMANJU_STAGING_RUNTIME:metadata_drop_counter_stays_zero_while_owned_then_counts_all_frames:PASS"
    );
}

// The runner currently selects exact path suffixes, despite the CLI help saying
// substring. Keep one uniquely named entry and require every scenario marker.
#[ktest]
fn cybermanju_staging_runtime() {
    real_wrapper_roundtrip_zero_gaps_tail_and_drop();
    empty_and_invalid_budget();
    allocation_failure_drops_all_acquired_real_frames();
    copy_failure_drops_every_frame_acquired_before_copy();
    metadata_drop_counter_stays_zero_while_owned_then_counts_all_frames();
}
