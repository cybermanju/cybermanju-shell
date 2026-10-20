// SPDX-License-Identifier: MPL-2.0
//! Real QEMU PCIe topology and NVMe DMA. This is not NVIDIA/GPU validation.
use core::sync::atomic::{Ordering, fence};

use ostd::{
    mm::{
        HasDaddr, PAGE_SIZE, VmIo,
        dma::{DeviceDmaError, DmaMask},
    },
    prelude::ktest,
};

use crate::{PCI_BUS, PciDeviceLocation, cfg_space::Command, mmio::PciMmioGuard};

pub(crate) fn init_pci_for_tests() {
    static INIT: spin::Once<()> = spin::Once::new();
    INIT.call_once(crate::init);
}

fn ext_cap(loc: PciDeviceLocation, id: u16) -> Option<u16> {
    let mut at = 0x100u16;
    let mut visited = [false; 1024];
    loop {
        assert!((0x100..=0xffc).contains(&at) && at & 3 == 0);
        assert!(!visited[at as usize / 4]);
        visited[at as usize / 4] = true;
        let h = crate::arch::read32(&loc, u32::from(at)).expect("ECAM test read");
        if h == 0 || h == u32::MAX {
            return None;
        }
        if h as u16 == id {
            return Some(at);
        }
        at = (h >> 20) as u16;
        if at == 0 {
            return None;
        }
    }
}

fn wait(registers: &PciMmioGuard<'_>, offset: usize, mask: u32, value: u32) -> bool {
    for _ in 0..20_000_000 {
        let status = registers.read32(offset).expect("NVMe MMIO poll");
        if status & mask == value {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn quiesce_qemu_nvme_for_test(bus: &mut crate::bus::PciBus, loc: PciDeviceLocation) {
    use ostd::mm::VmIoOnce;
    let mut device = bus
        .test_take_unclaimed(loc)
        .expect("own real NVMe before stop");
    assert_eq!(device.device_id().vendor_id, 0x1b36);
    assert_eq!(device.device_id().class, 1);
    assert_eq!(device.device_id().subclass, 8);
    let command = loc.read16(4);
    assert_ne!(
        command & Command::MEMORY_SPACE.bits(),
        0,
        "firmware must decode NVMe BAR"
    );
    let crate::cfg_space::Bar::Memory(bar) = device.bar_manager_mut().bar_mut(0).unwrap() else {
        panic!("NVMe BAR0 must be memory");
    };
    let registers = bar.acquire_owned().expect("test-only NVMe stop mapping");
    // Firmware may leave BME enabled. Clearing it alone is NOT quiescence:
    // perform the actual NVMe controller reset and wait for CSTS.RDY=0 first.
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
        "firmware NVMe controller did not stop; no DMA context created"
    );
    device.write_command_raw(command & !Command::BUS_MASTER.bits());
    assert_eq!(loc.read16(4) & Command::BUS_MASTER.bits(), 0);
    drop(registers);
    bus.return_reserved(device);
}

#[ktest]
fn qemu_real_pcie_topology_and_nvme_identify_dma() {
    init_pci_for_tests();
    let mut bus = PCI_BUS.lock();
    let port = PciDeviceLocation {
        bus: 0,
        device: 7,
        function: 0,
    };
    let no_acs_port = PciDeviceLocation {
        bus: 0,
        device: 8,
        function: 0,
    };
    let legacy_bridge = PciDeviceLocation {
        bus: 0,
        device: 9,
        function: 0,
    };
    for p in [port, no_acs_port, legacy_bridge] {
        assert_ne!(p.read16(0), 0xffff, "real QEMU bridge required");
        assert_eq!(p.read8(0x0b), 6);
        assert_eq!(p.read8(0x0a), 4);
        assert_eq!(p.read8(0x0e) & 0x7f, 1);
        assert_eq!(p.read8(0x18), 0);
        assert_ne!(p.read8(0x19), 0);
    }
    assert!(
        ext_cap(no_acs_port, 0x0d).is_none(),
        "negative root port must genuinely lack ACS"
    );
    let acs = ext_cap(port, 0x0d).expect("real root-port ACS");
    let original_acs = port.read16(acs + 6);
    let capabilities = port.read16(acs + 4);
    assert_eq!(capabilities & 0x1d, 0x1d);
    let mask = DmaMask::new(32).unwrap();
    // A conventional PCI endpoint behind a real PCI bridge is an unsupported alias path.
    let legacy_loc = *bus
        .unclaimed_devices()
        .find(|d| {
            d.location().bus == legacy_bridge.read8(0x19)
                && d.device_id().vendor_id == 0x1234
                && d.device_id().device_id == 0x11e8
        })
        .expect("real EDU behind legacy bridge")
        .location();
    {
        let mut reserved = bus.reserve_unclaimed(legacy_loc).unwrap();
        let regs = reserved.prepare_mmio(0).unwrap();
        assert_eq!(
            regs.isolated_dma_context(mask).err().unwrap(),
            DeviceDmaError::UnsupportedPlatform
        );
        assert_eq!(legacy_loc.read16(4) & Command::BUS_MASTER.bits(), 0);
    }
    // Genuine PCIe NVMe behind an ACS-disabled root port must fail before any DMA.
    let bad_loc = *bus
        .unclaimed_devices()
        .find(|d| {
            d.location().bus == no_acs_port.read8(0x19)
                && d.device_id().class == 1
                && d.device_id().subclass == 8
        })
        .expect("negative real NVMe endpoint")
        .location();
    quiesce_qemu_nvme_for_test(&mut bus, bad_loc);
    {
        let mut reserved = bus.reserve_unclaimed(bad_loc).unwrap();
        let regs = reserved.prepare_mmio(0).unwrap();
        assert_eq!(
            regs.isolated_dma_context(mask).err().unwrap(),
            DeviceDmaError::UnsupportedPlatform
        );
        assert_eq!(bad_loc.read16(4) & Command::BUS_MASTER.bits(), 0);
    }
    let loc = *bus
        .unclaimed_devices()
        .find(|d| {
            d.location().bus == port.read8(0x19)
                && d.device_id().class == 1
                && d.device_id().subclass == 8
        })
        .expect("real NVMe behind root port")
        .location();
    quiesce_qemu_nvme_for_test(&mut bus, loc);
    let mut reserved = bus.reserve_unclaimed(loc).unwrap();
    let regs = reserved.prepare_mmio(0).unwrap();
    let prepared = loc.read16(4);
    assert_eq!(prepared & Command::BUS_MASTER.bits(), 0);
    assert_eq!(
        regs.read32(0x14).unwrap() & 1,
        0,
        "controller must start disabled"
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
    // Test-fixture-only ACS writes: production never changes root-port controls.
    port.write16(acs + 6, original_acs & !0x1d);
    assert_eq!(
        regs.isolated_dma_context(mask).err().unwrap(),
        DeviceDmaError::UnsupportedPlatform
    );
    port.write16(acs + 6, original_acs | 0x1d);
    assert_eq!(port.read16(acs + 6) & 0x1d, 0x1d);
    let direct_platform = regs.dma_context(mask).is_ok();
    let context = match regs.isolated_dma_context(mask) {
        Ok(context) => context,
        Err(DeviceDmaError::UnsupportedPlatform) if direct_platform => {
            port.write16(acs + 6, original_acs);
            ostd::early_println!(
                "PCIe topology negative cases passed without IOMMU; no PCIe DMA issued"
            );
            return;
        }
        Err(error) => panic!("PCIe isolated context failed: {error:?}"),
    };
    assert_eq!(context.requester().bus(), loc.bus);
    let sq = context.alloc_coherent(1).expect("NVMe private admin SQ");
    let cq = context.alloc_coherent(1).expect("NVMe private admin CQ");
    let identify = context
        .alloc_coherent(1)
        .expect("NVMe private identify page");
    assert_eq!(sq.daddr(), PAGE_SIZE);
    assert_eq!(cq.daddr(), 2 * PAGE_SIZE);
    assert_eq!(identify.daddr(), 3 * PAGE_SIZE);
    let mut command = [0u8; 64];
    command[0] = 6; // Identify, not a fabricated successful completion.
    command[2..4].copy_from_slice(&1u16.to_le_bytes());
    command[24..32].copy_from_slice(&(identify.daddr() as u64).to_le_bytes());
    command[40..44].copy_from_slice(&1u32.to_le_bytes()); // CNS Identify Controller
    sq.write_bytes(0, &command).unwrap();
    regs.test_write32(0x24, (7 << 16) | 7).unwrap();
    regs.test_write64(0x28, sq.daddr() as u64).unwrap();
    regs.test_write64(0x30, cq.daddr() as u64).unwrap();
    loc.write32(4, u32::from(prepared | Command::BUS_MASTER.bits()));
    regs.test_write32(0x14, (6 << 16) | (4 << 20) | 1).unwrap();
    let mut success = wait(&regs, 0x1c, 3, 1);
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
    // Disable/ack controller before reclaiming ANY queue or payload RAM.
    regs.test_write32(0x14, 0).unwrap();
    let stopped = wait(&regs, 0x1c, 1, 0);
    loc.write32(4, u32::from(prepared));
    if !success || !stopped {
        core::mem::forget(sq);
        core::mem::forget(cq);
        core::mem::forget(identify);
        core::mem::forget(context);
        panic!(
            "NVMe timeout; DMA storage/domain quarantined; completed={success} stopped={stopped}"
        );
    }
    assert_eq!(loc.read16(4), prepared);
    assert_eq!(
        u16::from_le_bytes(completion[12..14].try_into().unwrap()),
        1
    );
    assert_eq!(
        u16::from_le_bytes(completion[14..16].try_into().unwrap()),
        1,
        "real NVMe status/phase"
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
    assert_eq!(u16::from_le_bytes(data[..2].try_into().unwrap()), 0x1b36);
    assert!(data[4..24].starts_with(b"ASTER-PCIE-DMA"));
    assert!(data[24..64].starts_with(b"QEMU NVMe Ctrl"));
    drop(identify);
    drop(cq);
    drop(sq);
    drop(context);
    port.write16(acs + 6, original_acs);
    ostd::early_println!(
        "PCIe NVMe isolated DMA passed: real root-port route, private SQ/CQ/Identify DMA, actual CID/status/vendor/serial/model verified; controller stopped and BME off; NOT NVIDIA/GPU proof"
    );
    ostd::early_println!(
        "PCIe topology negative cases passed: ACS controls off, ACS absent, legacy bridge alias rejected before BME; no spoofed PCI IDs"
    );
}
