// SPDX-License-Identifier: MPL-2.0
//! Test-only two-CPU byte readback. No production module-mapping API.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use x86_64::registers::control::{Cr4, Cr4Flags};

use super::{KERNEL_PAGE_TABLE, MappedItem, MappedItemRef};
use crate::{
    cpu::{self, AtomicCpuSet, CpuSet, PinCurrentCpu},
    irq,
    mm::{
        Frame, FrameAllocOptions, HasPaddr, PAGE_SIZE, VmIo, VmReader, VmWriter,
        module_address::ModuleAddressReservation,
        page_prop::{CachePolicy, PageFlags, PageProperty, PrivilegedPageFlags},
        tlb::{TlbFlushOp, TlbFlusher},
    },
    prelude::*,
    task::disable_preempt,
    util::id_set::Id,
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static ADDRESS: AtomicUsize = AtomicUsize::new(0);
static OBSERVED: AtomicU64 = AtomicU64::new(0);
static READ_CPU: AtomicUsize = AtomicUsize::new(usize::MAX);
static READ_BYTES: AtomicUsize = AtomicUsize::new(0);
static PGE_SAVED: AtomicBool = AtomicBool::new(false);
static ORIGINAL_PGE: AtomicBool = AtomicBool::new(false);
static PGE_READY: AtomicBool = AtomicBool::new(false);
static PGE_RESTORED: AtomicBool = AtomicBool::new(false);

struct Exclusive;
impl Exclusive {
    fn new() -> Self {
        assert!(
            ACTIVE
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        );
        ADDRESS.store(0, Ordering::Release);
        PGE_SAVED.store(false, Ordering::Release);
        PGE_RESTORED.store(false, Ordering::Release);
        OBSERVED.store(0, Ordering::Release);
        READ_CPU.store(usize::MAX, Ordering::Release);
        READ_BYTES.store(0, Ordering::Release);
        PGE_READY.store(false, Ordering::Release);
        Self
    }
}
impl Drop for Exclusive {
    fn drop(&mut self) {
        ADDRESS.store(0, Ordering::Release);
        ACTIVE.store(false, Ordering::Release);
    }
}

// Executed on the selected remote CPU, with IRQ disabled by the IPI handler.
fn prepare_and_read() {
    ORIGINAL_PGE.store(
        Cr4::read().contains(Cr4Flags::PAGE_GLOBAL),
        Ordering::Release,
    );
    PGE_SAVED.store(true, Ordering::Release);
    // SAFETY: test-only local feature transition under IRQ; saved bit is restored
    // after teardown/ack by the owning guard. No frames or mappings are changed.
    unsafe {
        Cr4::update(|flags| flags.insert(Cr4Flags::PAGE_GLOBAL));
    }
    read_remote();
}

fn read_remote() {
    let address = ADDRESS.load(Ordering::Acquire);
    // No assertions/panics or flushes in the callback. The owner publishes a
    // readable mapping and retains both possible frames until all callbacks end.
    if address == 0 {
        READ_BYTES.store(0, Ordering::Release);
        return;
    }
    let mut bytes = [0; 8];
    // SAFETY: the exclusive test owns this live kernel VA; current and stale
    // translations refer to retained private RAM. No reader or pointer escapes.
    let mut reader = unsafe { VmReader::from_kernel_space(address as *const u8, bytes.len()) };
    let count = reader.read(&mut VmWriter::from(&mut bytes[..]));
    OBSERVED.store(u64::from_le_bytes(bytes), Ordering::Release);
    READ_BYTES.store(count, Ordering::Release);
    READ_CPU.store(cpu::CpuId::current_racy().as_usize(), Ordering::Release);
    PGE_READY.store(
        Cr4::read().contains(Cr4Flags::PAGE_GLOBAL),
        Ordering::Release,
    );
}

fn restore_remote_pge() {
    if PGE_SAVED.swap(false, Ordering::AcqRel) {
        let original = ORIGINAL_PGE.load(Ordering::Acquire);
        // SAFETY: only saved PGE is restored, on the same remote CPU under IRQ.
        unsafe {
            Cr4::update(|flags| flags.set(Cr4Flags::PAGE_GLOBAL, original));
        }
        PGE_RESTORED.store(
            Cr4::read().contains(Cr4Flags::PAGE_GLOBAL) == original,
            Ordering::Release,
        );
    }
}

struct Probe {
    reservation: ModuleAddressReservation,
    foreign: Frame<()>,
    replacement: Frame<()>,
    remote: CpuSet,
    all: AtomicCpuSet,
    installed: bool,
    remote_started: bool,
    // Release exclusivity last, after the frames and reservation have dropped.
    _exclusive: Exclusive,
}

impl Probe {
    fn flush(&self) {
        let mut flusher = TlbFlusher::new(&self.all, disable_preempt());
        flusher.issue_tlb_flush(TlbFlushOp::for_single(self.reservation.range().start));
        flusher.dispatch_tlb_flush();
        // Cursor/IRQ guards must never be live across this wait.
        flusher.sync_tlb_flush();
    }

    fn cleanup(&mut self) {
        if self.installed {
            let retired = {
                let guard = irq::disable_local();
                let range = self.reservation.range();
                let mut cursor = KERNEL_PAGE_TABLE
                    .get()
                    .unwrap()
                    .cursor_mut(&guard, &range)
                    .unwrap();
                if cursor.query().unwrap().1.is_some() {
                    // SAFETY: only this probe's preflighted slot. Retain the
                    // entire removed fragment until the following shootdown.
                    unsafe { cursor.take_next(PAGE_SIZE) }
                } else {
                    None
                }
            }; // no cursor/IRQ guard lives across an IPI wait
            self.flush();
            drop(retired);
            self.installed = false;
        }
        if self.remote_started {
            // Also drains preceding read callbacks before their frames can die.
            crate::smp::inter_processor_call(&self.remote, restore_remote_pge).wait();
            self.remote_started = false;
        }
        ADDRESS.store(0, Ordering::Release);
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[ktest]
fn cybermanju_tlb_remote_readback_runtime() {
    assert_eq!(cpu::num_cpus(), 2);
    assert!(crate::smp::IPI_SENDER.get().is_some());
    assert!(irq::is_local_enabled());
    let pin = disable_preempt();
    let source = pin.current_cpu();
    let target = cpu::all_cpus().find(|cpu| *cpu != source).unwrap();
    let exclusive = Exclusive::new();
    let foreign = FrameAllocOptions::new().zeroed(true).alloc_frame().unwrap();
    let replacement = FrameAllocOptions::new().zeroed(true).alloc_frame().unwrap();
    foreign.write_bytes(0, &[0xe1; 8]).unwrap();
    replacement.write_bytes(0, &[0x42; 8]).unwrap();
    assert_ne!(foreign.paddr(), replacement.paddr());
    // Declared before probe: unwinding cleans/flushes probe before dropping
    // any fragment retired by the PTE replacement. Reserve before mapping.
    let mut retired = Vec::with_capacity(1);
    let mut probe = Probe {
        reservation: ModuleAddressReservation::new(PAGE_SIZE, 1).unwrap(),
        foreign,
        replacement,
        remote: CpuSet::from(target),
        all: AtomicCpuSet::new(CpuSet::new_full()),
        installed: false,
        remote_started: false,
        _exclusive: exclusive,
    };
    let range = probe.reservation.range();
    let prop = PageProperty {
        flags: PageFlags::RW,
        cache: CachePolicy::Writeback,
        priv_flags: PrivilegedPageFlags::GLOBAL,
    };
    {
        let guard = irq::disable_local();
        let mut cursor = KERNEL_PAGE_TABLE
            .get()
            .unwrap()
            .cursor_mut(&guard, &range)
            .unwrap();
        assert!(cursor.query().unwrap().1.is_none());
        probe.installed = true; // rollback includes a partially installed own PTE
        // SAFETY: empty owned VA, retained RAM, supervisor RW/NX; test only.
        unsafe {
            cursor.map(MappedItem::Tracked(
                Frame::from_unsized(probe.foreign.clone()),
                prop,
            ));
        }
    }
    probe.flush(); // remove translations left by any prior use of this VA
    ADDRESS.store(range.start, Ordering::Release);
    probe.remote_started = true; // restoration also occurs after a sender panic
    crate::smp::inter_processor_call(&probe.remote, prepare_and_read).wait();
    assert_eq!(READ_CPU.load(Ordering::Acquire), target.as_usize());
    assert_eq!(READ_BYTES.load(Ordering::Acquire), 8);
    assert!(PGE_READY.load(Ordering::Acquire));
    assert_eq!(
        OBSERVED.load(Ordering::Acquire),
        u64::from_le_bytes([0xe1; 8])
    );
    println!(
        "\nCYBERMANJU_TLB_READBACK:topology:PASS cpus=2 source={} target={}",
        source.as_usize(),
        target.as_usize()
    );
    {
        let guard = irq::disable_local();
        let table = KERNEL_PAGE_TABLE.get().unwrap();
        let mut cursor = table.cursor_mut(&guard, &range).unwrap();
        let Some(MappedItemRef::Tracked(actual, flags)) = cursor.query().unwrap().1 else {
            panic!("tracked GLOBAL seed required");
        };
        assert_eq!(actual.paddr(), probe.foreign.paddr());
        assert!(flags.priv_flags.contains(PrivilegedPageFlags::GLOBAL));
        assert!(!flags.priv_flags.contains(PrivilegedPageFlags::USER));
        assert!(!flags.flags.contains(PageFlags::X));
        // SAFETY: replace only our slot, retaining both candidate frames.
        retired.push(unsafe { cursor.take_next(PAGE_SIZE) }.unwrap());
        cursor.jump(range.start).unwrap();
        // SAFETY: same owned VA, second retained private RAM frame, no X/USER.
        unsafe {
            cursor.map(MappedItem::Tracked(
                Frame::from_unsized(probe.replacement.clone()),
                prop,
            ));
        }
    }
    // No local INVLPG or CR4 change between replacement and this operation.
    probe.flush(); // operation under test; cleanup has its own unconditional flush
    crate::smp::inter_processor_call(&probe.remote, read_remote).wait();
    assert_eq!(READ_CPU.load(Ordering::Acquire), target.as_usize());
    assert_eq!(READ_BYTES.load(Ordering::Acquire), 8);
    assert!(PGE_READY.load(Ordering::Acquire));
    assert_eq!(
        OBSERVED.load(Ordering::Acquire),
        u64::from_le_bytes([0x42; 8]),
        "remote stale bytes survived"
    );
    println!(
        "CYBERMANJU_TLB_READBACK:bytes:PASS old=225 new=66 bytes=8 pge=true global=true retained_frames=2"
    );
    probe.cleanup();
    {
        let guard = irq::disable_local();
        let mut cursor = KERNEL_PAGE_TABLE
            .get()
            .unwrap()
            .cursor(&guard, &range)
            .unwrap();
        assert!(cursor.query().unwrap().1.is_none());
    }
    assert_eq!(ADDRESS.load(Ordering::Acquire), 0);
    assert!(!PGE_SAVED.load(Ordering::Acquire));
    assert!(PGE_RESTORED.load(Ordering::Acquire));
    drop(probe);
    drop(retired);
    println!(
        "CYBERMANJU_TLB_READBACK:cleanup:PASS unmapped=true slot_reset=true pge_restored=true init_called=false"
    );
}
