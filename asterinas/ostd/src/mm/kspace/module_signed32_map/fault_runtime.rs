// SPDX-License-Identifier: MPL-2.0
//! Runtime rollback proof for private Signed32 mapping fault injection.

use core::ops::Range;

use alloc::{vec, vec::Vec};

use super::{
    KERNEL_PAGE_TABLE, MappedItem, MappedItemRef, assert_unmapped, read_scoped, read_scoped_inner,
};
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

struct ForeignGuard {
    irq_guard: DisabledLocalIrqGuard,
    range: Range<Vaddr>,
    installed: bool,
    _frame: Frame<()>,
}

impl Drop for ForeignGuard {
    fn drop(&mut self) {
        if !self.installed {
            return;
        }
        let table = KERNEL_PAGE_TABLE
            .get()
            .expect("kernel page table initialized");
        {
            let mut cursor = table
                .cursor_mut(&self.irq_guard, &self.range)
                .expect("foreign range valid");
            cursor.jump(self.range.start).expect("foreign slot aligned");
            if cursor.query().expect("foreign PTE query").1.is_some() {
                // SAFETY: this fixture owns only this one injected tracked PTE;
                // its frame stays retained until after the GLOBAL flush below.
                drop(unsafe { cursor.take_next(PAGE_SIZE) }.expect("foreign PTE exists"));
            }
        }
        tlb_flush_addr_range(&self.range);
        self.installed = false;
    }
}

fn assert_all_absent(range: Range<Vaddr>) {
    let irq_guard = irq::disable_local();
    let mut cursor = KERNEL_PAGE_TABLE
        .get()
        .unwrap()
        .cursor(&irq_guard, &range)
        .unwrap();
    for va in range.clone().step_by(PAGE_SIZE) {
        cursor.jump(va).unwrap();
        assert!(
            cursor.query().unwrap().1.is_none(),
            "signed32 failed prefix must be unmapped"
        );
    }
}

#[ktest]
fn cybermanju_signed32_mapping_fault_runtime() {
    println!("");
    assert_eq!(num_cpus(), 1, "signed32 fault proof requires one CPU");
    const PAGES: usize = 3;
    const SPAN: usize = PAGES * PAGE_SIZE;
    const ENVELOPE: usize = 4 * PAGE_SIZE;
    let arena = NearTextAddressReservation::signed32_arena_range();
    let base = arena.end - 16 * PAGE_SIZE;
    let owner = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap();
    let range = owner.range();
    println!("CYBERMANJU_SIGNED32_MAPPING_FAULT:begin:PASS cpus=1 pages=3 envelope_pages=4");

    let frames: Vec<Frame<()>> = (0..PAGES)
        .map(|_| FrameAllocOptions::new().zeroed(true).alloc_frame().unwrap())
        .collect();
    let expected: Vec<Vec<u8>> = (0..PAGES)
        .map(|page| {
            (0..PAGE_SIZE)
                .map(|i| ((i * 17 + page * 53) % 251) as u8)
                .collect()
        })
        .collect();
    for (frame, bytes) in frames.iter().zip(&expected) {
        frame.write_bytes(0, bytes).unwrap();
    }
    let mut failures = 0;
    let mut installed_prefix_pages = 0usize;
    for injected in 0..PAGES {
        let mut output = vec![0xa5; SPAN];
        let mut observed_installed = usize::MAX;
        let result = read_scoped_inner(
            &owner,
            SPAN,
            frames.iter().cloned(),
            0,
            &mut output,
            Some(injected),
            &mut observed_installed,
        );
        assert!(
            matches!(result, Err(Error::IoError)),
            "signed32 partial mapping fault must be reported"
        );
        failures += 1;
        assert_eq!(
            observed_installed, injected,
            "signed32 fault must follow expected installed prefix"
        );
        installed_prefix_pages += observed_installed;
        assert!(
            output.iter().all(|b| *b == 0xa5),
            "failed read must preserve destination"
        );
        assert_all_absent(range.clone());
        for (frame, wanted) in frames.iter().zip(&expected) {
            let mut bytes = vec![0; PAGE_SIZE];
            frame.read_bytes(0, &mut bytes).unwrap();
            assert_eq!(&bytes, wanted, "staging frame contents changed");
        }
        assert_eq!(
            NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap_err(),
            Error::NoMemory,
            "reservation remains exclusive during fault test"
        );
    }

    let foreign = FrameAllocOptions::new().zeroed(true).alloc_frame().unwrap();
    let foreign_bytes = [0x6d; 32];
    foreign.write_bytes(0, &foreign_bytes).unwrap();
    let foreign_prop = PageProperty {
        flags: PageFlags::RW,
        cache: CachePolicy::Writeback,
        priv_flags: PrivilegedPageFlags::GLOBAL,
    };
    let slot = range.start + PAGE_SIZE..range.start + 2 * PAGE_SIZE;
    let mut guard = ForeignGuard {
        irq_guard: irq::disable_local(),
        range: slot.clone(),
        installed: false,
        _frame: foreign.clone(),
    };
    {
        let mut cursor = KERNEL_PAGE_TABLE
            .get()
            .unwrap()
            .cursor_mut(&guard.irq_guard, &slot)
            .unwrap();
        assert!(cursor.query().unwrap().1.is_none());
        guard.installed = true;
        // SAFETY: one private IRQ-excluded PTE, tracked frame retained; test never dereferences its VA.
        unsafe {
            cursor.map(MappedItem::Tracked(
                Frame::from_unsized(foreign.clone()),
                foreign_prop,
            ));
        }
    }
    let mut output = vec![0xa5; SPAN];
    assert!(
        matches!(
            read_scoped_inner(
                &owner,
                SPAN,
                frames.iter().cloned(),
                0,
                &mut output,
                None,
                &mut 0
            ),
            Err(Error::AccessDenied)
        ),
        "signed32 foreign PTE must stay intact"
    );
    assert!(output.iter().all(|b| *b == 0xa5));
    {
        let irq_guard = irq::disable_local();
        let mut cursor = KERNEL_PAGE_TABLE
            .get()
            .unwrap()
            .cursor(&irq_guard, &range)
            .unwrap();
        cursor.jump(slot.start).unwrap();
        let Some(MappedItemRef::Tracked(actual, prop)) = cursor.query().unwrap().1 else {
            panic!("foreign tracked PTE was lost");
        };
        let mut expected_prop = foreign_prop;
        expected_prop.priv_flags |= PrivilegedPageFlags::AVAIL1;
        assert_eq!(actual.paddr(), foreign.paddr());
        assert_eq!(prop, expected_prop);
        let mut bytes = [0; 32];
        foreign.read_bytes(0, &mut bytes).unwrap();
        assert_eq!(bytes, foreign_bytes);
        for va in range.clone().step_by(PAGE_SIZE) {
            if va == slot.start {
                continue;
            }
            cursor.jump(va).unwrap();
            assert!(
                cursor.query().unwrap().1.is_none(),
                "only injected foreign slot may remain mapped"
            );
        }
    }
    drop(guard);
    assert_all_absent(range.clone());
    let mut recovered = vec![0; SPAN];
    read_scoped(&owner, SPAN, frames.iter().cloned(), 0, &mut recovered).unwrap();
    let wanted: Vec<u8> = expected.iter().flatten().copied().collect();
    assert_eq!(
        recovered, wanted,
        "signed32 recovery bytes must match frames"
    );
    assert_unmapped(&owner);
    assert_eq!(
        NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4).unwrap_err(),
        Error::NoMemory,
        "reservation remains exclusive before drop"
    );
    println!(
        "CYBERMANJU_SIGNED32_MAPPING_FAULT:rollback:PASS failures={} installed_prefix_pages={} buffers_preserved={} foreign_rejected=1 foreign_preserved=true slack_unmapped=true",
        failures, installed_prefix_pages, failures
    );
    drop(owner);
    drop(foreign);
    let reclaimed = NearTextAddressReservation::new_signed32_at(base, ENVELOPE, 4)
        .expect("same envelope reclaimed");
    assert_eq!(reclaimed.range(), range);
    assert_all_absent(reclaimed.range());
    drop(reclaimed);
    println!(
        "CYBERMANJU_SIGNED32_MAPPING_FAULT:complete:PASS recovered_bytes={} reclaimed=1 all_unmapped=true production=false execution=false",
        recovered.len()
    );
}
