// SPDX-License-Identifier: MPL-2.0
//! Executes real PCI config transactions and MMIO mappings, not NVIDIA emulation.
use ostd::{Error, io::IoMem, prelude::ktest};

use crate::{
    PCI_BUS,
    cfg_space::{Bar, Command},
    mmio::MmioPrepareError,
};

#[ktest]
fn qemu_pci_testdev_reservation_mmio_rollback_and_reacquisition() {
    // This suite requires QEMU's genuine pci-testdev (1b36:0005), never a spoofed NVIDIA ID.
    // Component initialization does not run in an OSDK unit-test kernel, so enumerate here.
    crate::pcie_tests::init_pci_for_tests();
    let mut bus = PCI_BUS.lock();
    let before_count = bus.unclaimed_devices().count();
    let location = *bus
        .unclaimed_devices()
        .find(|d| {
            let id = d.device_id();
            id.vendor_id == 0x1b36 && id.device_id == 0x0005
        })
        .expect("native PCI lifecycle suite requires -device pci-testdev,membar=4096")
        .location();
    for _ in 0..8 {
        {
            let mut reserved = bus
                .reserve_unclaimed(location)
                .expect("test device not available");
            let loc = *reserved.location();
            let original = loc.read16(4);
            let (base, size) = match reserved.bar_manager().bar(0).unwrap() {
                Bar::Memory(bar) => (bar.base() as usize, bar.size() as usize),
                _ => panic!("pci-testdev BAR0 must be MMIO"),
            };
            {
                let registers = reserved.prepare_mmio(0).expect("real BAR0 acquire");
                let command = loc.read16(4);
                assert_eq!(command & Command::BUS_MASTER.bits(), 0);
                assert_ne!(command & Command::MEMORY_SPACE.bits(), 0);
                assert_ne!(command & Command::INTERRUPT_DISABLE.bits(), 0);
                assert!(
                    IoMem::acquire(base..base + size).is_err(),
                    "MMIO reservation overlaps live mapping"
                );
                let _value = registers.read32(0).expect("real pci-testdev register read");
                assert_eq!(registers.read32(1), Err(Error::InvalidArgs));
                assert_eq!(registers.read32(size), Err(Error::InvalidArgs));
                assert_eq!(registers.read32(usize::MAX - 3), Err(Error::InvalidArgs));
            }
            assert_eq!(loc.read16(4), original, "command rollback after mapping");
            assert!(matches!(
                reserved.prepare_mmio(5),
                Err(MmioPrepareError::Mapping(Error::InvalidArgs))
            ));
            assert_eq!(
                loc.read16(4),
                original,
                "command rollback after missing BAR"
            );
            // This requires real allocator recycling, not just dropping a virtual mapping.
            let blocker =
                IoMem::acquire(base..base + size).expect("released MMIO range must be available");
            assert_eq!(
                reserved.prepare_mmio(0).err(),
                Some(MmioPrepareError::Mapping(Error::AccessDenied))
            );
            assert_eq!(
                loc.read16(4),
                original,
                "command rollback after occupied MMIO range"
            );
            let slice = blocker.slice(0..4);
            let last_owner = slice.clone();
            drop(blocker);
            drop(slice);
            assert!(
                IoMem::acquire(base..base + size).is_err(),
                "surviving slice clone owns the whole reservation"
            );
            drop(last_owner);
            drop(
                IoMem::acquire(base..base + size)
                    .expect("last real mapping owner must recycle BAR"),
            );
            {
                let _irq_guard = ostd::irq::disable_local();
                assert_eq!(
                    reserved.prepare_mmio(0).err(),
                    Some(MmioPrepareError::InvalidContext)
                );
                assert_eq!(
                    loc.read16(4),
                    original,
                    "IRQ rejection must not touch command"
                );
            }
        }
        assert_eq!(bus.unclaimed_devices().count(), before_count);
    }
    assert!(bus.unclaimed_devices().any(|d| *d.location() == location));
    ostd::early_println!(
        "PCI resource lifecycle passed: 8 real QEMU pci-testdev reserve/map/read/rollback/reacquire cycles; not NVIDIA/GPU proof"
    );
    qemu_edu_dma_round_trip(&mut bus);
}

/// Actual DMA controller, not a CPU memcpy masquerading as device completion.
/// https://www.qemu.org/docs/master/specs/edu.html (1234:11e8).
fn qemu_edu_dma_round_trip(bus: &mut crate::bus::PciBus) {
    use core::sync::atomic::{Ordering, fence};

    use ostd::mm::{
        HasDaddr, HasSize, PAGE_SIZE, VmIo,
        dma::{DeviceDmaError, DmaMask},
    };

    let location = *bus
        .unclaimed_devices()
        .find(|device| {
            let id = device.device_id();
            id.vendor_id == 0x1234 && id.device_id == 0x11e8
        })
        .expect("PCI DMA suite requires -device edu,dma_mask=4294967295")
        .location();
    let before_count = bus.unclaimed_devices().count();
    for cycle in 0..8 {
        let mut reserved = bus.reserve_unclaimed(location).expect("reserve real EDU");
        let original = location.read16(4);
        {
            let registers = reserved.prepare_mmio(0).expect("map real EDU BAR0");
            let prepared = location.read16(4);
            assert_eq!(prepared & Command::BUS_MASTER.bits(), 0);
            let mask = DmaMask::new(32).unwrap();
            let context = match registers.dma_context(mask) {
                Ok(context) => context,
                Err(DeviceDmaError::UnsupportedPlatform) => {
                    assert_eq!(location.read16(4), prepared);
                    ostd::early_println!(
                        "PCI device DMA correctly rejected unsupported IOMMU/CVM platform; zero DMA issued"
                    );
                    // A detected IOMMU disallows the legacy direct-address API;
                    // test the actual requester-isolated domain path instead.
                    drop(registers);
                    drop(reserved);
                    qemu_edu_isolated_dma_round_trip(bus, location, before_count);
                    return;
                }
                Err(error) => panic!("unexpected device DMA preparation failure: {error:?}"),
            };
            let expected_requester = ostd::mm::dma::PciDmaRequester::new(
                0,
                location.bus,
                location.device,
                location.function,
            )
            .unwrap();
            assert_eq!(context.requester(), expected_requester);
            assert_eq!(context.mask(), mask);
            assert_eq!(location.read16(4), prepared, "context cannot enable BME");
            // Deliberately impossible physical mask: allocations must fail/free rather than
            // truncating the address or falling back to a global IOMMU mapping.
            let tiny = registers.dma_context(DmaMask::new(1).unwrap()).unwrap();
            for _ in 0..8 {
                assert!(matches!(
                    tiny.alloc_coherent(1),
                    Err(DeviceDmaError::AddressOutOfRange)
                ));
            }
            assert_eq!(
                location.read16(4),
                prepared,
                "failed allocation cannot mutate command"
            );
            let source = context.alloc_coherent(1).expect("32-bit source allocation");
            let destination = context
                .alloc_coherent(1)
                .expect("32-bit destination allocation");
            assert_eq!(source.size(), PAGE_SIZE);
            assert!(mask.contains_range(source.daddr() as u64, PAGE_SIZE));
            assert!(mask.contains_range(destination.daddr() as u64, PAGE_SIZE));
            let mut pattern = alloc::vec![0u8; PAGE_SIZE];
            let mut actual = alloc::vec![0u8; PAGE_SIZE];
            destination.read_bytes(0, &mut actual).unwrap();
            assert!(actual.iter().all(|byte| *byte == 0));
            for (index, byte) in pattern.iter_mut().enumerate() {
                *byte = (index.wrapping_mul(73) ^ (index >> 3) ^ (cycle * 31)) as u8;
            }
            source.write_bytes(0, &pattern).unwrap();
            destination
                .write_bytes(0, &alloc::vec![0xa5u8; PAGE_SIZE])
                .unwrap();
            // Only this known educational controller is enabled, AFTER both allocations.
            // The production resource/NVIDIA API deliberately has no BME enable operation.
            let master = EduBusMaster { location, prepared };
            location.write32(4, u32::from(prepared | Command::BUS_MASTER.bits()));
            assert_ne!(location.read16(4) & Command::BUS_MASTER.bits(), 0);
            fence(Ordering::SeqCst);
            let sent = edu_transfer(&registers, source.daddr() as u64, 0x40000, PAGE_SIZE, 1);
            let received = sent
                && edu_transfer(
                    &registers,
                    0x40000,
                    destination.daddr() as u64,
                    PAGE_SIZE,
                    3,
                );
            fence(Ordering::SeqCst);
            // A timeout is not quiescence. Keep both physical allocations permanently alive
            // before failing; merely clearing BME must not justify reusing in-flight memory.
            if !received {
                core::mem::forget(source);
                core::mem::forget(destination);
                drop(master);
                let command = location.read16(4);
                let bme_disabled = command != 0xffff && command & Command::BUS_MASTER.bits() == 0;
                panic!(
                    "real EDU DMA timed out; buffers quarantined, not freed; BME disabled readback={bme_disabled}"
                );
            }
            drop(master);
            assert_eq!(location.read16(4), prepared);
            destination.read_bytes(0, &mut actual).unwrap();
            assert_eq!(actual, pattern, "EDU DMA full-page readback mismatch");
            // Completed controller transfers, then BME off, then buffers/context/MMIO drop.
        }
        assert_eq!(location.read16(4), original);
        drop(reserved);
        assert_eq!(bus.unclaimed_devices().count(), before_count);
    }
    ostd::early_println!(
        "PCI device DMA passed: 8 real EDU 4096-byte RAM->device->RAM cycles; requester/mask/cleanup verified; no NVIDIA or IOMMU isolation proof"
    );
}

/// Exercise real EDU DMA through a per-requester IOMMU domain. Each iteration
/// drops and recreates the domain for the same requester to cover detach/reacquire.
fn qemu_edu_isolated_dma_round_trip(
    bus: &mut crate::bus::PciBus,
    location: crate::PciDeviceLocation,
    before_count: usize,
) {
    use core::sync::atomic::{Ordering, fence};

    use ostd::mm::{
        HasDaddr, HasSize, PAGE_SIZE, VmIo,
        dma::{DeviceDmaError, DmaMask},
    };

    for cycle in 0..8 {
        let mut reserved = bus.reserve_unclaimed(location).expect("reserve real EDU");
        let original = location.read16(4);
        {
            let registers = reserved.prepare_mmio(0).expect("map real EDU BAR0");
            let prepared = location.read16(4);
            assert_eq!(
                prepared & Command::BUS_MASTER.bits(),
                0,
                "BME off before domain"
            );

            // A one-bit mask cannot contain normal RAM. Failure must roll back
            // the mapping and release the temporary domain before the real one.
            let tiny_mask = DmaMask::new(1).unwrap();
            let tiny = registers
                .isolated_dma_context(tiny_mask)
                .expect("create temporary isolated domain");
            for _ in 0..8 {
                assert_eq!(
                    tiny.alloc_coherent(1).err(),
                    Some(DeviceDmaError::AddressOutOfRange)
                );
            }
            drop(tiny);
            assert_eq!(location.read16(4), prepared);

            let mask = DmaMask::new(32).unwrap();
            // SAFETY: EDU is exclusively reserved and idle, BME is clear, and
            // it has no bridge/alias/translation-service path.
            let context = registers
                .isolated_dma_context(mask)
                .expect("create requester-isolated EDU domain");
            let expected_requester = ostd::mm::dma::PciDmaRequester::new(
                0,
                location.bus,
                location.device,
                location.function,
            )
            .unwrap();
            assert_eq!(context.requester(), &expected_requester);
            assert_eq!(context.mask(), &mask);
            let domain_id = context.domain_id();
            assert_ne!(domain_id, 0, "isolated context has a hardware domain id");
            assert!(
                matches!(
                    registers.isolated_dma_context(mask),
                    Err(DeviceDmaError::DomainBusy)
                ),
                "duplicate live domain for one requester must be rejected"
            );
            assert_eq!(
                location.read16(4),
                prepared,
                "domain creation never enables BME"
            );

            let source = context.alloc_coherent(1).expect("isolated source page");
            let destination = context
                .alloc_coherent(1)
                .expect("isolated destination page");
            assert_eq!(source.size(), PAGE_SIZE);
            assert_eq!(destination.size(), PAGE_SIZE);
            assert!(mask.contains_range(source.daddr() as u64, PAGE_SIZE));
            assert!(mask.contains_range(destination.daddr() as u64, PAGE_SIZE));
            let mut pattern = alloc::vec![0u8; PAGE_SIZE];
            let mut actual = alloc::vec![0u8; PAGE_SIZE];
            destination.read_bytes(0, &mut actual).unwrap();
            assert!(actual.iter().all(|byte| *byte == 0));
            for (index, byte) in pattern.iter_mut().enumerate() {
                *byte = (index.wrapping_mul(73) ^ (index >> 3) ^ (cycle * 31)) as u8;
            }
            source.write_bytes(0, &pattern).unwrap();
            destination
                .write_bytes(0, &alloc::vec![0xa5u8; PAGE_SIZE])
                .unwrap();

            // Only this known educational controller is enabled, after both
            // buffers are mapped into its isolated domain.
            let master = EduBusMaster { location, prepared };
            location.write32(4, u32::from(prepared | Command::BUS_MASTER.bits()));
            assert_ne!(location.read16(4) & Command::BUS_MASTER.bits(), 0);
            fence(Ordering::SeqCst);
            let sent = edu_transfer(&registers, source.daddr() as u64, 0x40000, PAGE_SIZE, 1);
            let received = sent
                && edu_transfer(
                    &registers,
                    0x40000,
                    destination.daddr() as u64,
                    PAGE_SIZE,
                    3,
                );
            fence(Ordering::SeqCst);
            if !received {
                // BME clear alone does not prove quiescence. Keep all memory and
                // the IOMMU domain pinned before reporting the timeout.
                core::mem::forget(source);
                core::mem::forget(destination);
                drop(master);
                let command = location.read16(4);
                let bme_disabled = command != 0xffff && command & Command::BUS_MASTER.bits() == 0;
                core::mem::forget(context);
                panic!(
                    "isolated EDU DMA timed out; buffers and domain quarantined; BME disabled readback={bme_disabled}"
                );
            }
            drop(master);
            assert_eq!(location.read16(4), prepared);
            destination.read_bytes(0, &mut actual).unwrap();
            assert_eq!(
                actual, pattern,
                "isolated EDU DMA full-page readback mismatch"
            );
            // Retain the exact physical destination while removing its IOVA.
            // A real stale device write must fault, not modify this CPU-owned page.
            let revoked_iova = destination.daddr();
            let cpu_page = destination.revoke().expect("completed IOTLB revocation");
            cpu_page
                .write_bytes(0, &alloc::vec![0x6du8; PAGE_SIZE])
                .unwrap();
            let stale_master = EduBusMaster { location, prepared };
            location.write32(4, u32::from(prepared | Command::BUS_MASTER.bits()));
            fence(Ordering::SeqCst);
            let blocked_transfer_completed =
                edu_transfer(&registers, 0x40000, revoked_iova as u64, PAGE_SIZE, 3);
            fence(Ordering::SeqCst);
            if !blocked_transfer_completed {
                core::mem::forget(cpu_page);
                core::mem::forget(source);
                drop(stale_master);
                core::mem::forget(context);
                panic!("EDU stale-IOVA probe timed out; storage/domain quarantined");
            }
            drop(stale_master);
            cpu_page.read_bytes(0, &mut actual).unwrap();
            assert!(
                actual.iter().all(|byte| *byte == 0x6d),
                "revoked IOVA modified CPU-owned RAM"
            );
            drop(cpu_page);
            drop(source);
            drop(context);
        }
        assert_eq!(location.read16(4), original);
        drop(reserved);
        assert_eq!(bus.unclaimed_devices().count(), before_count);
    }
    ostd::early_println!(
        "PCI isolated DMA revocation passed: 8 stale IOVA device writes blocked while physical pages remained owned"
    );
    qemu_edu_simultaneous_isolated_dma(bus);
    ostd::early_println!(
        "PCI isolated DMA passed: 8 real EDU 4096-byte RAM->device->RAM cycles; domain detach/reacquire verified; bounded type-0 PCI isolation only"
    );
}

/// Keep two real requesters and their isolated domains live together. Matching IOVAs
/// deliberately map different pages so a shared/incorrect requester translation is exposed.
fn qemu_edu_simultaneous_isolated_dma(bus: &mut crate::bus::PciBus) {
    use core::sync::atomic::{Ordering, fence};

    use ostd::mm::{HasDaddr, HasSize, PAGE_SIZE, VmIo, dma::DmaMask};

    let before_count = bus.unclaimed_devices().count();
    let locations: [crate::PciDeviceLocation; 2] = [
        *bus.unclaimed_devices()
            .find(|device| {
                let id = device.device_id();
                id.vendor_id == 0x1234
                    && id.device_id == 0x11e8
                    && device.location().bus == 0
                    && device.location().device == 5
                    && device.location().function == 0
            })
            .expect("simultaneous IOMMU suite requires QEMU EDU at 00:05.0")
            .location(),
        *bus.unclaimed_devices()
            .find(|device| {
                let id = device.device_id();
                id.vendor_id == 0x1234
                    && id.device_id == 0x11e8
                    && device.location().bus == 0
                    && device.location().device == 6
                    && device.location().function == 0
            })
            .expect("simultaneous IOMMU suite requires QEMU EDU at 00:06.0")
            .location(),
    ];
    assert_ne!(locations[0], locations[1]);
    let mut device_a = bus
        .test_take_unclaimed(locations[0])
        .expect("take first real EDU from PCI queue");
    let mut device_b = bus
        .test_take_unclaimed(locations[1])
        .expect("take second real EDU from PCI queue");
    let original_a = locations[0].read16(4);
    let original_b = locations[1].read16(4);

    let registers_a =
        crate::mmio::PciMmioGuard::prepare(&mut device_a, 0).expect("map first real EDU BAR0");
    let registers_b =
        crate::mmio::PciMmioGuard::prepare(&mut device_b, 0).expect("map second real EDU BAR0");
    let prepared_a = locations[0].read16(4);
    let prepared_b = locations[1].read16(4);
    assert_eq!(prepared_a & Command::BUS_MASTER.bits(), 0);
    assert_eq!(prepared_b & Command::BUS_MASTER.bits(), 0);

    let mask = DmaMask::new(32).unwrap();
    let context_a = registers_a
        .isolated_dma_context(mask)
        .expect("create first requester-isolated domain");
    let context_b = registers_b
        .isolated_dma_context(mask)
        .expect("create second requester-isolated domain");
    let domain_a = context_a.domain_id();
    let domain_b = context_b.domain_id();
    assert_ne!(domain_a, 0, "first domain has a hardware domain id");
    assert_ne!(domain_b, 0, "second domain has a hardware domain id");
    assert_ne!(
        domain_a, domain_b,
        "simultaneous requesters need distinct domains"
    );

    let source_a = context_a.alloc_coherent(1).expect("first EDU source page");
    let destination_a = context_a
        .alloc_coherent(1)
        .expect("first EDU destination page");
    let source_b = context_b.alloc_coherent(1).expect("second EDU source page");
    let destination_b = context_b
        .alloc_coherent(1)
        .expect("second EDU destination page");
    for buffer in [&source_a, &destination_a, &source_b, &destination_b] {
        assert_eq!(buffer.size(), PAGE_SIZE);
    }
    assert_eq!(source_a.daddr(), 4096);
    assert_eq!(source_b.daddr(), source_a.daddr());
    assert_eq!(destination_a.daddr(), 8192);
    assert_eq!(destination_b.daddr(), destination_a.daddr());
    assert_ne!(source_a.daddr(), destination_a.daddr());
    assert!(mask.contains_range(source_a.daddr() as u64, PAGE_SIZE));
    assert!(mask.contains_range(destination_a.daddr() as u64, PAGE_SIZE));

    let mut pattern_a = alloc::vec![0u8; PAGE_SIZE];
    let mut pattern_b = alloc::vec![0u8; PAGE_SIZE];
    let mut actual_a = alloc::vec![0u8; PAGE_SIZE];
    let mut actual_b = alloc::vec![0u8; PAGE_SIZE];
    for index in 0..PAGE_SIZE {
        pattern_a[index] = (index.wrapping_mul(73) ^ (index >> 3)) as u8;
        pattern_b[index] = pattern_a[index] ^ 0x5a;
    }
    source_a.write_bytes(0, &pattern_a).unwrap();
    source_b.write_bytes(0, &pattern_b).unwrap();
    destination_a
        .write_bytes(0, &alloc::vec![0xa5u8; PAGE_SIZE])
        .unwrap();
    destination_b
        .write_bytes(0, &alloc::vec![0x3cu8; PAGE_SIZE])
        .unwrap();

    // Both domains and all four mappings are live before either controller is enabled.
    let master_a = EduBusMaster {
        location: locations[0],
        prepared: prepared_a,
    };
    let master_b = EduBusMaster {
        location: locations[1],
        prepared: prepared_b,
    };
    locations[0].write32(4, u32::from(prepared_a | Command::BUS_MASTER.bits()));
    locations[1].write32(4, u32::from(prepared_b | Command::BUS_MASTER.bits()));
    assert_ne!(locations[0].read16(4) & Command::BUS_MASTER.bits(), 0);
    assert_ne!(locations[1].read16(4) & Command::BUS_MASTER.bits(), 0);
    fence(Ordering::SeqCst);
    let completed_a = edu_transfer(&registers_a, source_a.daddr() as u64, 0x40000, PAGE_SIZE, 1)
        && edu_transfer(
            &registers_a,
            0x40000,
            destination_a.daddr() as u64,
            PAGE_SIZE,
            3,
        );
    let completed_b = edu_transfer(&registers_b, source_b.daddr() as u64, 0x40000, PAGE_SIZE, 1)
        && edu_transfer(
            &registers_b,
            0x40000,
            destination_b.daddr() as u64,
            PAGE_SIZE,
            3,
        );
    fence(Ordering::SeqCst);
    if !completed_a || !completed_b {
        // BME is disabled before any in-flight memory or translation domain is pinned.
        drop(master_b);
        drop(master_a);
        core::mem::forget(source_a);
        core::mem::forget(destination_a);
        core::mem::forget(source_b);
        core::mem::forget(destination_b);
        core::mem::forget(context_a);
        core::mem::forget(context_b);
        drop(registers_b);
        drop(registers_a);
        bus.return_reserved(device_a);
        bus.return_reserved(device_b);
        panic!("simultaneous isolated EDU DMA timed out; buffers and domains quarantined");
    }
    drop(master_b);
    drop(master_a);
    assert_eq!(locations[0].read16(4), prepared_a);
    assert_eq!(locations[1].read16(4), prepared_b);
    destination_a.read_bytes(0, &mut actual_a).unwrap();
    destination_b.read_bytes(0, &mut actual_b).unwrap();
    assert_eq!(actual_a, pattern_a, "first EDU isolated full-page readback");
    assert_eq!(
        actual_b, pattern_b,
        "second EDU isolated full-page readback"
    );

    drop(source_a);
    drop(destination_a);
    drop(source_b);
    drop(destination_b);
    drop(context_a);
    drop(context_b);
    drop(registers_b);
    drop(registers_a);
    assert_eq!(locations[0].read16(4), original_a);
    assert_eq!(locations[1].read16(4), original_b);
    bus.return_reserved(device_a);
    bus.return_reserved(device_b);
    assert_eq!(bus.unclaimed_devices().count(), before_count);
    ostd::early_println!(
        "PCI isolated DMA simultaneous requester isolation passed: 2 real EDU devices, distinct live domains, matching 4096/8192 source/destination IOVAs, full-page readbacks verified"
    );
}

struct EduBusMaster {
    location: crate::PciDeviceLocation,
    prepared: u16,
}

impl Drop for EduBusMaster {
    fn drop(&mut self) {
        self.location.write32(4, u32::from(self.prepared));
        let _ = self.location.read16(4);
    }
}

fn edu_transfer(
    registers: &crate::mmio::PciMmioGuard<'_>,
    source: u64,
    destination: u64,
    size: usize,
    command: u32,
) -> bool {
    for (offset, value) in [(0x80, source), (0x88, destination), (0x90, size as u64)] {
        registers.test_write64(offset, value).unwrap();
    }
    registers.test_write32(0x98, command).unwrap();
    for _ in 0..50_000_000 {
        if registers.read32(0x98).unwrap() & 1 == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}
