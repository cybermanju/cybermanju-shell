// SPDX-License-Identifier: MPL-2.0
//! Per-test metadata callback tracking for real untyped frames.
use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};

pub(super) struct TrackedMetadata {
    counter: Arc<AtomicUsize>,
}

impl TrackedMetadata {
    pub(super) fn new(counter: Arc<AtomicUsize>) -> Self {
        Self { counter }
    }
}

impl Drop for TrackedMetadata {
    fn drop(&mut self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }
}

// Keep the unsafe trait-implementation macro here, outside the staging module's
// `forbid(unsafe_code)` descendant. This tracks metadata callbacks, not frame
// allocator free-list reuse or physical-page reclamation.
crate::impl_untyped_frame_meta_for!(TrackedMetadata);
