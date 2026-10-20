// SPDX-License-Identifier: MPL-2.0
//! Real QEMU multifunction PCIe/NVMe tests. This is not NVIDIA/GPU validation.
use core::sync::atomic::{Ordering, fence};

#[cfg(target_arch = "x86_64")]
use ostd::mm::dma::IsolatedDmaContext;
use ostd::{
    mm::{
        HasDaddr, PAGE_SIZE, VmIo, VmIoOnce,
        dma::{DeviceDmaError, DmaMask},
    },
    prelude::ktest,
};

use crate::{
    PCI_BUS, PciDeviceLocation,
    cfg_space::{Bar, Command},
    pcie_tests::init_pci_for_tests,
};

const NVME_VENDOR: u16 = 0x1b36;
const CONTROLLER_MODEL: &[u8] = b"QEMU NVMe Ctrl";

// Only this fixture programs ACS. Production requires it already configured.
struct AcsFixture {
    port: PciDeviceLocation,
    control_offset: u16,
    original: u16,
}

impl AcsFixture {
    fn enable(port: PciDeviceLocation) -> Self {
        let mut at = 0x100u16;
        let mut visited = [false; 1024];
        loop {
            assert!((0x100..=0xffc).contains(&at) && at & 3 == 0);
            assert!(!visited[at as usize / 4]);
            visited[at as usize / 4] = true;
            let header = crate::arch::read32(&port, u32::from(at)).unwrap();
            assert_ne!(header, 0, "root port must genuinely support ACS");
            assert_ne!(header, u32::MAX);
            if header as u16 == 0x0d {
                assert_eq!(port.read16(at + 4) & 0x1d, 0x1d);
                let original = port.read16(at + 6);
                port.write16(at + 6, original | 0x1d);
                assert_eq!(port.read16(at + 6) & 0x1d, 0x1d);
                return Self {
                    port,
                    control_offset: at + 6,
                    original,
                };
            }
            at = (header >> 20) as u16;
        }
    }
}

impl Drop for AcsFixture {
    fn drop(&mut self) {
        self.port.write16(self.control_offset, self.original);
    }
}

fn function(bus: u8, device: u8, function: u8) -> PciDeviceLocation {
    PciDeviceLocation {
        bus,
        device,
        function,
    }
}

fn quiesce_qemu_nvme_for_test(bus: &mut crate::bus::PciBus, loc: PciDeviceLocation) {
    let mut device = bus
        .test_take_unclaimed(loc)
        .expect("own real NVMe function before stop");
    assert_eq!(device.device_id().vendor_id, NVME_VENDOR);
    assert_eq!(device.device_id().class, 1);
    assert_eq!(device.device_id().subclass, 8);
    let command = loc.read16(4);
    assert_ne!(
        command & Command::MEMORY_SPACE.bits(),
        0,
        "firmware must decode NVMe BAR"
    );
    let Bar::Memory(bar) = device.bar_manager_mut().bar_mut(0).unwrap() else {
        panic!("NVMe BAR0 must be memory");
    };
    let registers = bar.acquire_owned().expect("test-only NVMe stop mapping");

    // Firmware may leave BME on. Reset the actual controller and wait until it
    // stops before disabling BME; clearing BME alone is not DMA quiescence.
    registers.write_once(0x14, &0u32).unwrap();
    let mut stopped = false;
    for _ in 0..20_000_000 {
        let status: u32 = registers.read_once(0x1c).unwrap();
        if status & 1 == 0 {
            stopped = true;
            break;
        }
        core::hint::spin_loop();
    }
    assert!(
        stopped,
        "firmware NVMe function did not stop; no group DMA context created"
    );
    device.write_command_raw(command & !Command::BUS_MASTER.bits());
    assert_eq!(loc.read16(4) & Command::BUS_MASTER.bits(), 0);
    drop(registers);
    bus.return_reserved(device);
}

fn wait(regs: &crate::mmio::PciMmioGuard<'_>, offset: usize, mask: u32, value: u32) -> bool {
    for _ in 0..20_000_000 {
        if regs.read32(offset).expect("NVMe MMIO poll") & mask == value {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// Run a real Identify Controller command and return the shared domain for the
/// next function. If completion or shutdown times out, leak the buffers and
/// context: the device may still be DMA-capable, so reclaiming RAM is unsafe.
fn identify_with_shared_context<'a>(
    regs: &crate::mmio::PciMmioGuard<'_>,
    loc: PciDeviceLocation,
    expected_serial: &[u8],
    context: crate::dma::PciIsolatedDmaGroupContext<'a>,
) -> crate::dma::PciIsolatedDmaGroupContext<'a> {
    let sq = context.alloc_coherent(1).expect("private NVMe admin SQ");
    let cq = context.alloc_coherent(1).expect("private NVMe admin CQ");
    let identify = context
        .alloc_coherent(1)
        .expect("private NVMe Identify page");
    assert_ne!(sq.daddr(), cq.daddr());
    assert_ne!(sq.daddr(), identify.daddr());
    assert_ne!(cq.daddr(), identify.daddr());

    assert_eq!(
        regs.read32(0x14).unwrap() & 1,
        0,
        "controller starts disabled"
    );
    let cap = u64::from(regs.read32(0).unwrap()) | (u64::from(regs.read32(4).unwrap()) << 32);
    assert!(
        cap & 0xffff >= 7,
        "controller must support 8-entry admin queues"
    );
    assert_eq!(
        (cap >> 48) & 15,
        0,
        "4096-byte minimum memory page required"
    );
    assert_ne!(cap & (1 << 37), 0, "NVM command set required");

    let mut command = [0u8; 64];
    command[0] = 6; // Identify
    command[2..4].copy_from_slice(&1u16.to_le_bytes()); // CID
    command[24..32].copy_from_slice(&(identify.daddr() as u64).to_le_bytes());
    command[40..44].copy_from_slice(&1u32.to_le_bytes()); // CNS=Identify Controller
    sq.write_bytes(0, &command).unwrap();
    regs.test_write32(0x24, (7 << 16) | 7).unwrap(); // AQA
    regs.test_write64(0x28, sq.daddr() as u64).unwrap(); // ASQ
    regs.test_write64(0x30, cq.daddr() as u64).unwrap(); // ACQ

    let prepared_command = loc.read16(4);
    assert_eq!(prepared_command & Command::BUS_MASTER.bits(), 0);
    loc.write32(4, u32::from(prepared_command | Command::BUS_MASTER.bits()));
    regs.test_write32(0x14, (6 << 16) | (4 << 20) | 1).unwrap(); // CC: IOSQES=6, IOCQES=4, EN
    let mut success = wait(regs, 0x1c, 3, 1);
    let mut completion = [0u8; 16];
    if success {
        fence(Ordering::SeqCst);
        regs.test_write32(0x1000, 1).unwrap(); // Admin SQ tail
        success = false;
        for _ in 0..20_000_000 {
            cq.read_bytes(0, &mut completion).unwrap();
            if completion[14] & 1 != 0 {
                success = true;
                break;
            }
            core::hint::spin_loop();
        }
    }
    fence(Ordering::SeqCst);

    // Stop the controller before clearing BME and before releasing any DMA RAM.
    regs.test_write32(0x14, 0).unwrap();
    let stopped = wait(regs, 0x1c, 1, 0);
    loc.write32(4, u32::from(prepared_command));
    if !success || !stopped {
        core::mem::forget(sq);
        core::mem::forget(cq);
        core::mem::forget(identify);
        core::mem::forget(context);
        panic!(
            "multifunction NVMe timeout; DMA storage/domain quarantined; completed={success} stopped={stopped}"
        );
    }

    assert_eq!(loc.read16(4), prepared_command);
    assert_eq!(
        u16::from_le_bytes(completion[12..14].try_into().unwrap()),
        1,
        "real CID"
    );
    assert_eq!(
        u16::from_le_bytes(completion[14..16].try_into().unwrap()),
        1,
        "real success status/phase"
    );
    assert_eq!(
        u16::from_le_bytes(completion[8..10].try_into().unwrap()),
        1,
        "SQ head"
    );
    assert_eq!(
        u16::from_le_bytes(completion[10..12].try_into().unwrap()),
        0,
        "admin SQ ID"
    );
    let mut data = [0u8; PAGE_SIZE];
    identify.read_bytes(0, &mut data).unwrap();
    assert_eq!(
        u16::from_le_bytes(data[..2].try_into().unwrap()),
        NVME_VENDOR
    );
    assert!(
        data[4..24].starts_with(expected_serial),
        "actual controller serial"
    );
    assert!(
        data[24..64].starts_with(CONTROLLER_MODEL),
        "actual controller model"
    );

    drop(identify);
    drop(cq);
    drop(sq);
    context
}

#[ktest]
fn qemu_multifunction_nvme_group_reservation_and_shared_dma() {
    init_pci_for_tests();
    let mut bus = PCI_BUS.lock();
    let root_port = function(0, 10, 0);
    let _group_acs = AcsFixture::enable(root_port);
    assert_ne!(
        root_port.read16(0),
        0xffff,
        "real QEMU multifunction root port required"
    );
    assert_eq!(root_port.read8(0x0b), 6);
    assert_eq!(root_port.read8(0x0a), 4);
    assert_eq!(root_port.read8(0x0e) & 0x7f, 1);
    let downstream_bus = root_port.read8(0x19);
    assert_ne!(downstream_bus, 0, "root port must route to a secondary bus");
    let fn0 = function(downstream_bus, 0, 0);
    let fn1 = function(downstream_bus, 0, 1);
    assert_ne!(fn0.read16(0), 0xffff, "real NVMe function zero required");
    assert_ne!(fn1.read16(0), 0xffff, "real NVMe function one required");
    assert_eq!(fn0.read16(0), NVME_VENDOR);
    assert_eq!(fn1.read16(0), NVME_VENDOR);
    assert_ne!(
        fn0.read8(0x0e) & 0x80,
        0,
        "QEMU function zero must advertise multifunction"
    );
    assert_eq!(
        fn1.read8(0x0e) & 0x7f,
        0,
        "function one must be an endpoint"
    );

    // Firmware is permitted to leave either NVMe controller active. Establish
    // device-specific quiescence first, then exercise the production ownership API.
    quiesce_qemu_nvme_for_test(&mut bus, fn0);
    quiesce_qemu_nvme_for_test(&mut bus, fn1);
    assert_eq!(fn0.read16(4) & Command::BUS_MASTER.bits(), 0);
    assert_eq!(fn1.read16(4) & Command::BUS_MASTER.bits(), 0);

    // A genuine conventional-PCI EDU sibling must not be admitted as PCIe just
    // because function zero is a PCIe NVMe endpoint. Keep EDU idle with BME off.
    let bad_root_port = function(0, 11, 0);
    assert_ne!(
        bad_root_port.read16(0),
        0xffff,
        "real bad-group root port required"
    );
    assert_eq!(bad_root_port.read16(0), root_port.read16(0));
    assert_eq!(bad_root_port.read16(2), root_port.read16(2));
    assert_eq!(bad_root_port.read8(0x0b), 6);
    assert_eq!(bad_root_port.read8(0x0a), 4);
    assert_eq!(bad_root_port.read8(0x0e) & 0x7f, 1);
    let _bad_group_acs = AcsFixture::enable(bad_root_port);
    let bad_downstream_bus = bad_root_port.read8(0x19);
    assert_ne!(bad_downstream_bus, 0);
    let bad_fn0 = function(bad_downstream_bus, 0, 0);
    let bad_fn1 = function(bad_downstream_bus, 0, 1);
    assert_eq!(bad_fn0.read16(0), NVME_VENDOR);
    assert_eq!(bad_fn0.read16(2), fn0.read16(2));
    assert_eq!(bad_fn1.read16(0), 0x1234, "genuine QEMU EDU vendor");
    assert_eq!(bad_fn1.read16(2), 0x11e8, "genuine QEMU EDU device");
    assert_ne!(
        bad_fn0.read8(0x0e) & 0x80,
        0,
        "NVMe function zero is multifunction"
    );
    assert_eq!(
        bad_fn1.read8(0x0e) & 0x7f,
        0,
        "EDU function one is an endpoint"
    );
    quiesce_qemu_nvme_for_test(&mut bus, bad_fn0);
    assert_eq!(
        bad_fn1.read16(4) & Command::BUS_MASTER.bits(),
        0,
        "idle EDU BME off"
    );
    assert!(
        matches!(
            crate::topology::admit_group(&[bad_fn0]),
            Err(DeviceDmaError::InvalidRequester)
        ),
        "same-slot incomplete group rejected while real function one is present"
    );
    assert!(
        matches!(
            crate::topology::admit_group(&[bad_fn0, bad_fn1]),
            Err(DeviceDmaError::UnsupportedPlatform)
        ),
        "complete group rejected because the genuine EDU sibling lacks PCIe"
    );
    {
        let mut bad_group = bus
            .reserve_unclaimed_group(bad_fn0)
            .expect("reserve NVMe and actual EDU sibling together");
        let locations: alloc::vec::Vec<_> = bad_group.locations().collect();
        assert_eq!(locations.as_slice(), &[bad_fn0, bad_fn1]);
        assert_eq!(bad_fn0.read16(4) & Command::BUS_MASTER.bits(), 0);
        assert_eq!(bad_fn1.read16(4) & Command::BUS_MASTER.bits(), 0);
        let bad_mmio = bad_group
            .prepare_mmio(&[0, 0])
            .expect("map BAR0 of real NVMe and EDU functions");
        assert!(bad_mmio.function(0).is_some());
        assert!(bad_mmio.function(1).is_some());
        assert!(
            matches!(
                bad_mmio.isolated_dma_context(DmaMask::new(32).unwrap()),
                Err(DeviceDmaError::UnsupportedPlatform)
            ),
            "conventional PCI EDU sibling must fail closed as unsupported"
        );
        drop(bad_mmio);
    }
    ostd::early_println!(
        "PCI multifunction unsupported sibling correctly rejected: real conventional PCI EDU function one lacks PCIe"
    );

    // All physically present siblings must be available before reservation.
    let missing = bus
        .test_take_unclaimed(fn1)
        .expect("unowned sibling fixture");
    assert!(bus.reserve_unclaimed_group(fn0).is_err());
    bus.return_reserved(missing);

    // A live BME bit on one sibling rejects the whole group and is never
    // cleared by production code; the fixture clears it only after observing it.
    fn1.write32(4, u32::from(fn1.read16(4) | Command::BUS_MASTER.bits()));
    let active_command = fn1.read16(4);
    assert_ne!(active_command & Command::BUS_MASTER.bits(), 0);
    assert!(
        bus.reserve_unclaimed_group(fn0).is_err(),
        "active sibling must reject group"
    );
    assert_eq!(
        fn1.read16(4) & Command::BUS_MASTER.bits(),
        Command::BUS_MASTER.bits()
    );
    fn1.write32(4, u32::from(active_command & !Command::BUS_MASTER.bits()));

    // Repeated whole-slot ownership cycles must return both functions without
    // probing or leaving the slot partially reserved.
    for _ in 0..2 {
        let group = bus
            .reserve_unclaimed_group(fn0)
            .expect("repeat full-slot reservation");
        let locations: alloc::vec::Vec<_> = group.locations().collect();
        assert_eq!(locations.as_slice(), &[fn0, fn1]);
        drop(group);
    }

    // The one-function path cannot silently collapse a multifunction slot into
    // one requester/domain. This is tested before taking the whole group.
    {
        let mut one = bus
            .reserve_unclaimed(fn0)
            .expect("reserve function zero for negative case");
        let regs = one
            .prepare_mmio(0)
            .expect("prepare real NVMe function zero");
        assert!(
            regs.isolated_dma_context(DmaMask::new(32).unwrap())
                .is_err(),
            "single-function isolated DMA must reject a multifunction slot"
        );
        assert_eq!(fn0.read16(4) & Command::BUS_MASTER.bits(), 0);
    }

    {
        let mut group = bus
            .reserve_unclaimed_group(fn0)
            .expect("reserve every present slot function");
        let locations: alloc::vec::Vec<_> = group.locations().collect();
        assert_eq!(
            locations.as_slice(),
            &[fn0, fn1],
            "complete ordered function set"
        );
        for loc in [fn0, fn1] {
            assert_eq!(
                loc.read16(4) & Command::BUS_MASTER.bits(),
                0,
                "all group BMEs off"
            );
        }

        let before = [fn0.read16(4), fn1.read16(4)];
        assert!(
            group.prepare_mmio(&[0, 6]).is_err(),
            "invalid sibling BAR must fail transactionally"
        );
        assert_eq!(
            [fn0.read16(4), fn1.read16(4)],
            before,
            "failed mapping restores both command words"
        );

        let mmio = group
            .prepare_mmio(&[0, 0])
            .expect("prepare both genuine NVMe BARs");
        for (index, loc) in [fn0, fn1].into_iter().enumerate() {
            assert_eq!(loc.read16(4) & Command::BUS_MASTER.bits(), 0);
            assert!(mmio.function(index).is_some());
        }
        assert!(mmio.function(2).is_none());

        crate::topology::admit_group(&locations)
            .expect("every real NVMe function must pass the trusted ACS route");
        let direct_platform = mmio
            .function(0)
            .unwrap()
            .dma_context(DmaMask::new(32).unwrap())
            .is_ok();
        let context = match mmio.isolated_dma_context(DmaMask::new(32).unwrap()) {
            Ok(context) => context,
            Err(DeviceDmaError::UnsupportedPlatform) if direct_platform => {
                drop(mmio);
                drop(group);
                ostd::early_println!(
                    "PCI multifunction group correctly rejected without IOMMU; no group DMA issued"
                );
                ostd::early_println!(
                    "PCI multifunction group reservation passed: missing sibling, active BME preserved, invalid-BAR rollback, and single-function rejection verified"
                );
                return;
            }
            Err(error) => panic!("trusted multifunction group isolated context failed: {error:?}"),
        };
        assert_eq!(
            context.requesters().len(),
            2,
            "one requester for each real function"
        );
        for (requester, expected_function) in context.requesters().iter().zip([0, 1]) {
            assert_eq!(requester.segment(), 0);
            assert_eq!(requester.bus(), downstream_bus);
            assert_eq!(requester.device(), 0);
            assert_eq!(requester.function(), expected_function);
        }
        let mut context = Some(context);
        let mut domain_id = None;
        for _ in 0..2 {
            let context = match context.take() {
                Some(context) => context,
                None => mmio
                    .isolated_dma_context(DmaMask::new(32).unwrap())
                    .expect("reacquire complete NVMe requester group"),
            };
            assert_eq!(context.requesters().len(), 2);
            let current_domain = context.domain_id();
            assert_ne!(current_domain, 0, "real shared IOMMU domain ID");
            if let Some(previous_domain) = domain_id {
                assert_eq!(
                    current_domain, previous_domain,
                    "domain ID reclaimed after drop"
                );
            }
            domain_id = Some(current_domain);

            // A raw OSTD attempt with the identical live requesters and reviewed
            // routes must collide with the complete reservation, not just fn0.
            let routes = crate::topology::admit_group(&locations)
                .expect("live group retains both validated PCIe routes");
            let raw_members: alloc::vec::Vec<_> =
                context.requesters().iter().copied().zip(routes).collect();
            assert!(
                matches!(
                    IsolatedDmaContext::new_group_with_paths(
                        &raw_members,
                        DmaMask::new(32).unwrap()
                    ),
                    Err(DeviceDmaError::DomainBusy)
                ),
                "raw OSTD constructor must observe every live group requester"
            );

            let regs0 = mmio.function(0).unwrap();
            let context = identify_with_shared_context(regs0, fn0, b"ASTER-GROUP-F0", context);
            let regs1 = mmio.function(1).unwrap();
            let context = identify_with_shared_context(regs1, fn1, b"ASTER-GROUP-F1", context);
            assert_eq!(context.domain_id(), current_domain);
            assert_eq!(fn0.read16(4) & Command::BUS_MASTER.bits(), 0);
            assert_eq!(fn1.read16(4) & Command::BUS_MASTER.bits(), 0);
            drop(context);
        }
        drop(mmio);
        drop(group);
    }

    ostd::early_println!(
        "PCI multifunction group reservation passed: missing sibling, active BME preserved, invalid-BAR rollback, and single-function rejection verified"
    );
    ostd::early_println!(
        "PCI multifunction isolated DMA passed: two real NVMe functions, shared requester domain, actual Identify CID/status/vendor/serial/model verified; both controllers stopped and BME off"
    );
}
