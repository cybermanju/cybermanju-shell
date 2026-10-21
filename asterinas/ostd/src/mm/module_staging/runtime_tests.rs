// SPDX-License-Identifier: MPL-2.0
//! Real-frame integration tests for staging. These tests require initialized
//! OSTD allocation facilities. A single-CPU scoped mapping probe is ktest-only;
//! no test executes module bytes or creates persistent mappings.
#![forbid(unsafe_code)]

use alloc::vec;
use core::sync::atomic::{AtomicUsize, Ordering};

use super::{
    super::{ModuleChunk, ModuleStaging},
    Page, StagedPages,
};
use crate::{
    Error,
    mm::{Frame, FrameAllocOptions, PAGE_SIZE, VmIo, module_staging_test_meta::TrackedMetadata},
    prelude::*,
};

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
    println!("\nCYBERMANJU_STAGING_RUNTIME:real_wrapper_roundtrip_zero_gaps_tail_and_drop:PASS");
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

#[cfg(target_arch = "x86_64")]
fn reserved_owner_lifecycle() {
    use crate::mm::module_address::ModuleAddressReservation;
    let payload = [0xa1, 0xb2, 0xc3];
    let staged = ModuleStaging::new(
        PAGE_SIZE,
        &[ModuleChunk {
            offset: 0,
            bytes: &payload,
        }],
        1,
        1,
    )
    .unwrap();
    let reservation = ModuleAddressReservation::new(2 * PAGE_SIZE, 2).unwrap();
    let range = reservation.range();
    let joint = staged.with_reservation(reservation).unwrap();
    assert_eq!(joint.reserved_range(), range);
    assert_eq!((joint.span(), joint.page_count()), (PAGE_SIZE, 1));
    let mut actual = [0xff; 8];
    joint.read_bytes(0, &mut actual).unwrap();
    assert_eq!(&actual[..payload.len()], &payload);
    assert!(actual[payload.len()..].iter().all(|byte| *byte == 0));
    let mut byte = [0xff];
    assert!(joint.read_bytes(PAGE_SIZE, &mut byte).is_err());
    assert_eq!(byte, [0xff]);
    let other = ModuleAddressReservation::new(PAGE_SIZE, 1).unwrap();
    assert!(other.range().end <= range.start || range.end <= other.range().start);
    drop(other);
    drop(joint);
    let reused = ModuleAddressReservation::new(2 * PAGE_SIZE, 2).unwrap();
    assert_eq!(reused.range(), range);
    drop(reused);
    for span in [0, 2 * PAGE_SIZE] {
        let reservation = ModuleAddressReservation::new(PAGE_SIZE, 1).unwrap();
        let failed_range = reservation.range();
        let staged = ModuleStaging::new(span, &[], span / PAGE_SIZE, 0).unwrap();
        assert!(matches!(
            staged.with_reservation(reservation),
            Err(Error::InvalidArgs)
        ));
        let probe = ModuleAddressReservation::new(PAGE_SIZE, 1).unwrap();
        assert_eq!(probe.range(), failed_range);
        drop(probe);
    }
    println!("CYBERMANJU_STAGING_RUNTIME:reserved_owner_lifecycle:PASS");
}

#[cfg(target_arch = "x86_64")]
fn scoped_rw_mapping() {
    use crate::mm::module_address::ModuleAddressReservation;
    let first: Vec<u8> = (0..PAGE_SIZE + 13).map(|i| (i % 251) as u8).collect();
    let tail = [0x51, 0x82, 0xa3, 0xb4];
    let chunks = [
        ModuleChunk {
            offset: 0,
            bytes: &first,
        },
        ModuleChunk {
            offset: 2 * PAGE_SIZE,
            bytes: &tail,
        },
    ];
    let staged = ModuleStaging::new(3 * PAGE_SIZE, &chunks, 3, 2).unwrap();
    let reservation = ModuleAddressReservation::new(4 * PAGE_SIZE, 4).unwrap();
    let range = reservation.range();
    let mut joint = staged.with_reservation(reservation).unwrap();
    let mut expected = vec![0xff; joint.span()];
    joint.read_bytes(0, &mut expected).unwrap();
    let mut actual = vec![0xff; joint.span()];
    joint.read_mapped_for_test(0, &mut actual).unwrap();
    assert_eq!(actual, expected);
    let mut cross = vec![0xff; PAGE_SIZE + 7];
    joint
        .read_mapped_for_test(PAGE_SIZE - 3, &mut cross)
        .unwrap();
    assert_eq!(&cross, &expected[PAGE_SIZE - 3..2 * PAGE_SIZE + 4]);
    let mut byte = [0xa5];
    assert_eq!(
        joint.read_mapped_for_test(joint.span(), &mut byte),
        Err(Error::InvalidArgs)
    );
    assert_eq!(
        joint.read_mapped_for_test(usize::MAX, &mut byte),
        Err(Error::Overflow)
    );
    assert_eq!(byte, [0xa5]);
    joint.read_mapped_for_test(joint.span(), &mut []).unwrap();
    let mut stale = [0; 8];
    joint.probe_stale_read_for_test(&mut stale).unwrap();
    assert_eq!(
        &stale,
        &expected[..8],
        "stale GLOBAL translation must not supply foreign bytes"
    );
    drop(joint);
    let reused = ModuleAddressReservation::new(4 * PAGE_SIZE, 4).unwrap();
    assert_eq!(reused.range(), range);
    drop(reused);
    let oversized = ModuleAddressReservation::new(1025 * PAGE_SIZE, 1025).unwrap();
    let mut bounded = ModuleStaging::new(PAGE_SIZE, &[], 1, 0)
        .unwrap()
        .with_reservation(oversized)
        .unwrap();
    assert_eq!(
        bounded.read_mapped_for_test(0, &mut byte),
        Err(Error::InvalidArgs)
    );
    assert_eq!(byte, [0xa5]);
    println!("CYBERMANJU_STAGING_RUNTIME:scoped_rw_mapping:PASS");
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
    #[cfg(target_arch = "x86_64")]
    reserved_owner_lifecycle();
    #[cfg(target_arch = "x86_64")]
    scoped_rw_mapping();
}
