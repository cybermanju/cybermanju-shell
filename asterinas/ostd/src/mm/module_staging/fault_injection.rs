// SPDX-License-Identifier: MPL-2.0

//! Test-only failure injection at the owned-frame staging backend boundary.
//!
//! This uses real allocated frames and injects allocation/copy errors; it does
//! not simulate actual OOM, MMU faults, or hardware failures. It exposes neither
//! pointers nor mappings. A successful staging operation is deliberately
//! discarded and reported as `InvalidArgs`, because this backend exists only to
//! exercise failure paths. If the configured failure point is not reached, the
//! caller receives that sentinel error and can check `FaultStats::snapshot()`.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::{
    ModuleChunk, ModuleStaging,
    storage::{Page, StagedPages},
};
use crate::{
    Error,
    mm::{Frame, FrameAllocOptions, VmIo},
};

/// Failure point selected for a staging attempt.
#[derive(Clone, Copy, Debug)]
pub enum FaultMode {
    /// Fail allocation when this many frames have already been acquired.
    AllocateAfter(usize),
    /// Fail a write when this many writes have already succeeded.
    CopyAfter(usize),
}

/// Thread-safe counters shared with the caller for one staging attempt.
pub struct FaultStats {
    entries: AtomicUsize,
    acquired: AtomicUsize,
    dropped: AtomicUsize,
    writes: AtomicUsize,
    triggered: AtomicBool,
}

/// A point-in-time snapshot of backend activity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultSnapshot {
    pub entries: usize,
    pub acquired: usize,
    pub dropped: usize,
    pub writes: usize,
    pub triggered: bool,
}

impl FaultStats {
    /// Creates an empty set of counters.
    pub fn new() -> Self {
        Self {
            entries: AtomicUsize::new(0),
            acquired: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            triggered: AtomicBool::new(false),
        }
    }

    /// Returns the current activity counters.
    pub fn snapshot(&self) -> FaultSnapshot {
        FaultSnapshot {
            entries: self.entries.load(Ordering::Relaxed),
            acquired: self.acquired.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            writes: self.writes.load(Ordering::Relaxed),
            triggered: self.triggered.load(Ordering::Relaxed),
        }
    }
}

struct TrackedPage {
    frame: Option<Frame<()>>,
    stats: Arc<FaultStats>,
    mode: FaultMode,
}

impl Page for TrackedPage {
    const SIZE: usize = 4096;

    fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        if let FaultMode::CopyAfter(n) = self.mode {
            if self.stats.writes.load(Ordering::Relaxed) == n {
                self.stats.triggered.store(true, Ordering::Relaxed);
                return Err(Error::IoError);
            }
        }
        let result = self
            .frame
            .as_ref()
            .expect("tracked frame remains owned until drop")
            .write_bytes(offset, bytes);
        result?;
        self.stats.writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<(), Error> {
        self.frame
            .as_ref()
            .expect("tracked frame remains owned until drop")
            .read_bytes(offset, bytes)
    }
}

impl Drop for TrackedPage {
    fn drop(&mut self) {
        drop(self.frame.take());
        self.stats.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

/// Runs the production staging validations/copy logic with failure injection.
///
/// `stats` must be fresh (all counters zero and not triggered), otherwise this
/// returns `InvalidArgs`. Every call increments `entries`, even when validation
/// rejects it before an allocator call. `AllocateAfter(n)` fails after `n` successful frame
/// allocations; `CopyAfter(n)` fails before the write following `n` successful
/// writes. Successful staging is dropped and returns `InvalidArgs` by design.
pub fn stage_with_fault(
    span: usize,
    chunks: &[ModuleChunk<'_>],
    max_pages: usize,
    max_chunks: usize,
    mode: FaultMode,
    stats: &Arc<FaultStats>,
) -> Result<ModuleStaging, Error> {
    let prior_entries = stats.entries.fetch_add(1, Ordering::Relaxed);
    let snapshot = stats.snapshot();
    if prior_entries != 0
        || snapshot.acquired != 0
        || snapshot.dropped != 0
        || snapshot.writes != 0
        || snapshot.triggered
    {
        return Err(Error::InvalidArgs);
    }

    let pages = StagedPages::<TrackedPage>::new(span, chunks, max_pages, max_chunks, || {
        if let FaultMode::AllocateAfter(n) = mode {
            if stats.acquired.load(Ordering::Relaxed) == n {
                stats.triggered.store(true, Ordering::Relaxed);
                return Err(Error::NoMemory);
            }
        }
        let frame = FrameAllocOptions::new().zeroed(true).alloc_frame()?;
        stats.acquired.fetch_add(1, Ordering::Relaxed);
        Ok(TrackedPage {
            frame: Some(frame),
            stats: Arc::clone(stats),
            mode,
        })
    })?;
    drop(pages);
    Err(Error::InvalidArgs)
}
