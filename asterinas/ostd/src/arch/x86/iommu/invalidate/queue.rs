// SPDX-License-Identifier: MPL-2.0

use crate::{
    mm::{FrameAllocOptions, PAGE_SIZE, Segment, VmIo},
    prelude::*,
};

pub(in crate::arch::iommu) struct Queue {
    segment: Segment<()>,
    queue_size: usize,
    tail: usize,
}

impl Queue {
    pub(in crate::arch::iommu) fn append_descriptor(&mut self, descriptor: u128) {
        if self.tail == self.queue_size {
            self.tail = 0;
        }
        self.segment
            .write_val(self.tail * size_of::<u128>(), &descriptor)
            .unwrap();
        self.tail += 1;
    }

    pub(in crate::arch::iommu) fn flush_for_dma(&self) {
        super::super::dma_remapping::isolated::flush(self.segment.paddr(), self.segment.size());
    }

    pub(in crate::arch::iommu) fn tail(&self) -> usize {
        self.tail % self.queue_size
    }

    pub(in crate::arch::iommu) fn size(&self) -> usize {
        self.queue_size
    }

    pub(in crate::arch::iommu) fn base_paddr(&self) -> Paddr {
        self.segment.paddr()
    }

    pub(super) fn new() -> Self {
        const DEFAULT_PAGES: usize = 1;
        let segment = FrameAllocOptions::new()
            .alloc_segment(DEFAULT_PAGES)
            .unwrap();
        Self {
            segment,
            queue_size: (DEFAULT_PAGES * PAGE_SIZE) / size_of::<u128>(),
            tail: 0,
        }
    }
}

#[cfg(ktest)]
mod tests {
    use super::*;
    #[ktest]
    fn queue_tail_is_always_inside_ring() {
        let mut queue = Queue::new();
        for index in 0..queue.size() * 3 {
            queue.append_descriptor(index as u128);
            assert_eq!(queue.tail(), (index + 1) % queue.size());
        }
        assert_eq!(queue.tail(), 0);
    }
}
