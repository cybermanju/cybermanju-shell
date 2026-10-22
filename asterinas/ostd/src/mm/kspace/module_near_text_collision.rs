// SPDX-License-Identifier: MPL-2.0
//! Fault-injection-only collision proof for the near-text reservation audit.
//!
//! This test seeds real tracked PTEs but never dereferences the near-text VA.
//! The fixture assumes the dedicated pool has no other users and SMP is off;
//! IRQ exclusion prevents local interleaving while a slot is temporarily seeded.

use core::ops::Range;

use super::{KERNEL_PAGE_TABLE, MappedItem, MappedItemRef};
use crate::{
    Error,
    arch::mm::tlb_flush_addr_range,
    cpu::num_cpus,
    irq::{self, DisabledLocalIrqGuard},
    mm::{
        Frame, FrameAllocOptions, HasPaddr, PAGE_SIZE, Vaddr, VmIo,
        module_address::NearTextAddressReservation,
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags},
    },
    prelude::{ktest, println},
};

/// Owns exactly the injected PTE and its IRQ exclusion. The frame handle is
/// retained both here and in the caller, so PTE retirement + GLOBAL flush
/// precede releasing the last frame reference on every normal cleanup path.
struct ForeignMapping {
    irq_guard: DisabledLocalIrqGuard,
    range: Range<Vaddr>,
    installed: bool,
    _frame: Frame<()>,
}

impl Drop for ForeignMapping {
    fn drop(&mut self) {
        if !self.installed {
            return;
        }
        let table = KERNEL_PAGE_TABLE
            .get()
            .expect("kernel page table remains initialized");
        {
            let mut cursor = table
                .cursor_mut(&self.irq_guard, &self.range)
                .expect("fixture page-table range remains valid");
            cursor.jump(self.range.start).expect("fixture page aligned");
            if cursor
                .query()
                .expect("fixture PTE query succeeds")
                .1
                .is_some()
            {
                // SAFETY: this guard exclusively owns its single injected slot;
                // removed tracked frame is retained through the GLOBAL flush.
                drop(unsafe { cursor.take_next(PAGE_SIZE) }.expect("our foreign PTE exists"));
            }
        }
        tlb_flush_addr_range(&self.range);
        self.installed = false;
    }
}

fn assert_three_pages(base: Vaddr, expected: [Option<(usize, PageProperty)>; 3]) {
    let guard = irq::disable_local();
    let range = base..base + 3 * PAGE_SIZE;
    let mut cursor = KERNEL_PAGE_TABLE
        .get()
        .unwrap()
        .cursor(&guard, &range)
        .unwrap();
    for index in 0..3 {
        cursor.jump(base + index * PAGE_SIZE).unwrap();
        match (cursor.query().unwrap().1, expected[index]) {
            (None, None) => {}
            (Some(MappedItemRef::Tracked(frame, prop)), Some((paddr, wanted))) => {
                assert_eq!(frame.paddr(), paddr);
                // KernelPtConfig marks tracked PTEs with AVAIL1 on insertion.
                let mut stored = wanted;
                stored.priv_flags |= PrivilegedPageFlags::AVAIL1;
                assert_eq!(prop, stored);
            }
            _ => panic!("PTE state differs from collision fixture at page {index}"),
        }
    }
}

#[ktest]
fn cybermanju_near_text_collision_runtime() {
    println!(""); // separate protocol markers from the runner's test-name prefix
    assert_eq!(num_cpus(), 1, "foreign-PTE fixture requires one CPU");
    const SPAN: usize = 3 * PAGE_SIZE;
    let pool = NearTextAddressReservation::arena_range();
    let base = pool.end - 8 * PAGE_SIZE;
    assert!(base >= pool.start && base + SPAN <= pool.end);

    // One foreign frame only, initialized before the loop and held past every
    // guard cleanup + GLOBAL flush. The fixture VA is never read or written.
    let foreign = FrameAllocOptions::new().zeroed(true).alloc_frame().unwrap();
    let bytes = [0x6d; 32];
    foreign.write_bytes(0, &bytes).unwrap();
    let foreign_prop = PageProperty {
        flags: PageFlags::RW,
        cache: CachePolicy::Writeback,
        priv_flags: PrivilegedPageFlags::GLOBAL,
    };
    println!(
        "CYBERMANJU_NEAR_TEXT_COLLISION:begin:PASS cpus=1 cases=3 span_pages=3 fixture_frames=1"
    );

    for index in 0..3 {
        let owner = NearTextAddressReservation::new_at(base, SPAN, 3).unwrap();
        let range = owner.range();
        assert_eq!(range, base..base + SPAN);
        assert_three_pages(range.start, [None, None, None]);

        let irq_guard = irq::disable_local();
        let slot = range.start + index * PAGE_SIZE..range.start + (index + 1) * PAGE_SIZE;
        let mut fixture = ForeignMapping {
            irq_guard,
            range: slot,
            installed: false,
            _frame: foreign.clone(),
        };
        {
            let mut cursor = KERNEL_PAGE_TABLE
                .get()
                .unwrap()
                .cursor_mut(&fixture.irq_guard, &fixture.range)
                .unwrap();
            assert!(
                cursor.query().unwrap().1.is_none(),
                "seed slot must initially be absent"
            );
            fixture.installed = true;
            // SAFETY: exclusive single-CPU fault-injection slot, retained frame,
            // supervisor WB/RW/NX PTE; no user can access or execute this VA.
            unsafe {
                cursor.map(MappedItem::Tracked(
                    Frame::from_unsized(foreign.clone()),
                    foreign_prop,
                ));
            }
        }
        // Adversarial release: only after seeding and solely in this IRQ-disabled
        // single-CPU fixture, the ordinary reservation owner stops tracking it.
        drop(owner);

        for attempt in 0..2 {
            assert!(
                matches!(
                    NearTextAddressReservation::new_at(base, SPAN, 3),
                    Err(Error::AccessDenied)
                ),
                "foreign PTE must be rejected case={index} attempt={attempt}"
            );
            let mut expected = [None, None, None];
            expected[index] = Some((foreign.paddr(), foreign_prop));
            assert_three_pages(base, expected);
            let mut observed = [0; 32];
            foreign.read_bytes(0, &mut observed).unwrap();
            assert_eq!(observed, bytes, "foreign frame bytes changed");
        }

        drop(fixture); // remove only injected PTE + flush before owner/frame release
        assert_three_pages(base, [None, None, None]);
        let reclaimed = NearTextAddressReservation::new_at(base, SPAN, 3)
            .expect("collision cleanup must reclaim exact address");
        assert_eq!(reclaimed.range(), base..base + SPAN);
        let reclaimed_range = reclaimed.range();
        assert_three_pages(reclaimed_range.start, [None, None, None]);
        drop(reclaimed);
        println!(
            "CYBERMANJU_NEAR_TEXT_COLLISION:case:PASS index={} start={:#x} end={:#x} foreign={:#x} denied=2 preserved=2 reclaimed=true",
            index,
            range.start,
            range.end,
            range.start + index * PAGE_SIZE
        );
    }
    drop(foreign);
    println!(
        "CYBERMANJU_NEAR_TEXT_COLLISION:complete:PASS cases=3 denied=6 preserved=6 reclaimed=3 seeded_pages=3 fixture_frames=1 va_dereferences=0 fault_injection_mapping=true production_mapping=false execution=false"
    );
}
