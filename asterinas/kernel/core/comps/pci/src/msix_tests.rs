// SPDX-License-Identifier: MPL-2.0
//! Real QEMU NVMe MSI-X lifecycle and completion interrupt test; not GPU validation.
use alloc::{sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicUsize, Ordering, fence};

use ostd::{
    Error,
    arch::trap::TrapFrame,
    mm::{
        HasDaddr, PAGE_SIZE, VmIo, VmIoOnce,
        dma::{DeviceDmaError, DmaMask},
    },
    prelude::ktest,
};

use crate::{
    PCI_BUS, PciDeviceLocation,
    cfg_space::{Bar, Command},
    mmio::PciMmioGuard,
    pcie_tests::init_pci_for_tests,
};

const NVME_VENDOR: u16 = 0x1b36;
const NVME_MODEL: &[u8] = b"QEMU NVMe Ctrl";
const EXPECTED_SERIAL: &[u8] = b"ASTER-MSIX";

struct HandlerDropProbe(Arc<AtomicUsize>);
impl Drop for HandlerDropProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn function(bus: u8, device: u8, function: u8) -> PciDeviceLocation {
    PciDeviceLocation {
        bus,
        device,
        function,
    }
}

fn find_capability(loc: PciDeviceLocation, id: u8) -> Option<u8> {
    let mut at = loc.read8(0x34) & !3;
    let mut visited = [false; 64];
    while at != 0 {
        assert!((0x40..=0xfc).contains(&at) && at & 3 == 0);
        assert!(!visited[usize::from(at / 4)]);
        visited[usize::from(at / 4)] = true;
        let header = loc.read8(u16::from(at));
        if header == id {
            return Some(at);
        }
        at = loc.read8(u16::from(at) + 1) & !3;
    }
    None
}

fn find_ext_capability(loc: PciDeviceLocation, id: u16) -> Option<u16> {
    let mut at = 0x100u16;
    let mut visited = [false; 1024];
    loop {
        assert!((0x100..=0xffc).contains(&at) && at & 3 == 0);
        assert!(!visited[usize::from(at / 4)]);
        visited[usize::from(at / 4)] = true;
        let header = crate::arch::read32(&loc, u32::from(at)).expect("root-port ECAM read");
        if header == 0 || header == u32::MAX {
            return None;
        }
        if header as u16 == id {
            return Some(at);
        }
        at = ((header >> 20) & 0xfff) as u16;
        if at == 0 {
            return None;
        }
    }
}

/// This test-only fixture enables the ACS redirects that isolated DMA admission requires.
struct AcsFixture {
    port: PciDeviceLocation,
    control: u16,
    original: u16,
}

impl AcsFixture {
    fn enable(port: PciDeviceLocation) -> Self {
        let capability = find_ext_capability(port, 0x0d).expect("real root-port ACS capability");
        assert_eq!(port.read16(capability + 4) & 0x1d, 0x1d);
        let original = port.read16(capability + 6);
        port.write16(capability + 6, original | 0x1d);
        assert_eq!(port.read16(capability + 6) & 0x1d, 0x1d);
        Self {
            port,
            control: capability + 6,
            original,
        }
    }
}

impl Drop for AcsFixture {
    fn drop(&mut self) {
        self.port.write16(self.control, self.original);
    }
}

fn quiesce_qemu_nvme(bus: &mut crate::bus::PciBus, loc: PciDeviceLocation) {
    let mut device = bus
        .test_take_unclaimed(loc)
        .expect("own dedicated QEMU NVMe");
    assert_eq!(device.device_id().vendor_id, NVME_VENDOR);
    assert_eq!(device.device_id().class, 1);
    assert_eq!(device.device_id().subclass, 8);
    let command = loc.read16(4);
    assert_ne!(command & Command::MEMORY_SPACE.bits(), 0);
    let Bar::Memory(bar) = device.bar_manager_mut().bar_mut(0).unwrap() else {
        panic!("NVMe BAR0 must be memory");
    };
    let regs = bar.acquire_owned().expect("map NVMe for fixture reset");
    regs.write_once(0x14, &0u32).unwrap();
    let mut stopped = false;
    for _ in 0..20_000_000 {
        if regs.read_once::<u32>(0x1c).unwrap() & 1 == 0 {
            stopped = true;
            break;
        }
        core::hint::spin_loop();
    }
    assert!(stopped, "NVMe must reach CSTS.RDY=0 before test ownership");
    device.write_command_raw(command & !Command::BUS_MASTER.bits());
    assert_eq!(loc.read16(4) & Command::BUS_MASTER.bits(), 0);
    drop(regs);
    bus.return_reserved(device);
}

fn wait(regs: &PciMmioGuard<'_>, offset: usize, mask: u32, value: u32) -> Result<bool, Error> {
    for _ in 0..20_000_000 {
        if regs.read32(offset)? & mask == value {
            return Ok(true);
        }
        core::hint::spin_loop();
    }
    Ok(false)
}

fn quiesce_controller(
    regs: &PciMmioGuard<'_>,
    loc: PciDeviceLocation,
    prepared_command: u16,
) -> Result<(), Error> {
    regs.test_write32(0x14, 0)?;
    if !wait(regs, 0x1c, 1, 0)? {
        // Do not clear BME or release DMA resources unless the controller actually stopped.
        return Err(Error::IoError);
    }
    loc.write32(4, u32::from(prepared_command));
    if loc.read16(4) != prepared_command {
        return Err(Error::IoError);
    }
    Ok(())
}

/// Shared body for direct-DMA and isolated-DMA contexts (their buffer types differ).
macro_rules! run_nvme_msix_cycles {
    (
        $regs:ident,
        $loc:ident,
        $context:ident,
        $table_size:ident,
        $command:ident,
        $original_msix_control:ident
    ) => {{
        let calls = Arc::new(AtomicUsize::new(0));
        for cycle in 0..2 {
            assert_eq!($loc.read16(4), $command, "BME remains off between cycles");
            let sq = $context.alloc_coherent(1).expect("private admin SQ");
            let cq = $context.alloc_coherent(1).expect("private admin CQ");
            let identify = $context.alloc_coherent(1).expect("private Identify page");
            let mut command_bytes = [0u8; 64];
            command_bytes[0] = 6; // Identify Controller.
            command_bytes[2..4].copy_from_slice(&(cycle as u16 + 1).to_le_bytes());
            command_bytes[24..32].copy_from_slice(&(identify.daddr() as u64).to_le_bytes());
            command_bytes[40..44].copy_from_slice(&1u32.to_le_bytes()); // CNS=1.
            sq.write_bytes(0, &command_bytes).unwrap();

            let baseline = calls.load(Ordering::SeqCst);
            let mut msix = $regs
                .prepare_msix(1)
                .expect("prepare one real MSI-X vector");
            assert_eq!(msix.table_size() as u16, $table_size);
            assert_eq!(msix.vector_count(), 1);
            msix.mask_all()
                .expect("mask all MSI-X vectors before queue setup");
            assert!(
                msix.activate().is_err(),
                "activation must reject a missing callback"
            );

            // Once BME is enabled, every recoverable test failure must first stop the
            // controller; otherwise retain all DMA and MSI-X resources before panicking.
            macro_rules! stop_before_failure {
                ($reason:expr) => {{
                    match msix.shutdown(|| quiesce_controller(&$regs, $loc, $command)) {
                        Ok(()) => {
                            drop(identify);
                            drop(cq);
                            drop(sq);
                            panic!("{reason}", reason = $reason);
                        }
                        Err(stop_error) => {
                            core::mem::forget(identify);
                            core::mem::forget(cq);
                            core::mem::forget(sq);
                            core::mem::forget($context);
                            core::mem::forget(msix);
                            panic!(
                                "{reason}; controller quiesce failed ({stop_error:?}); DMA quarantined",
                                reason = $reason
                            );
                        }
                    }
                }};
            }

            // Register the actual completion callback before starting DMA. While masked,
            // the installed handler must remain silent even after a real CQ/PBA completion.
            let handler_drops = Arc::new(AtomicUsize::new(0));
            let probe = HandlerDropProbe(handler_drops.clone());
            let callback_calls = Arc::clone(&calls);
            if let Err(error) = msix.register_handler(0, move |_: &TrapFrame| {
                let _keep_probe_live = &probe;
                callback_calls.fetch_add(1, Ordering::SeqCst);
            }) {
                stop_before_failure!(format_args!("failed to register CQ callback: {error:?}"));
            }

            // Program actual QEMU NVMe admin queues and issue Identify with BME enabled only
            // after all request-specific DMA buffers and the masked MSI-X vector are ready.
            $regs.test_write32(0x24, (7 << 16) | 7).unwrap(); // AQA: 8-entry queues.
            $regs.test_write64(0x28, sq.daddr() as u64).unwrap();
            $regs.test_write64(0x30, cq.daddr() as u64).unwrap();
            $loc.write32(4, u32::from($command | Command::BUS_MASTER.bits()));
            if let Err(error) = $regs.test_write32(0x14, (6 << 16) | (4 << 20) | 1) {
                stop_before_failure!(format_args!("failed to enable NVMe: {error:?}"));
            }
            let ready = match wait(&$regs, 0x1c, 3, 1) {
                Ok(ready) => ready,
                Err(error) => {
                    stop_before_failure!(format_args!("failed to poll NVMe readiness: {error:?}"));
                }
            };
            let mut completion = [0u8; 16];
            let mut completed = false;
            if ready {
                fence(Ordering::SeqCst);
                if let Err(error) = $regs.test_write32(0x1000, 1) {
                    stop_before_failure!(format_args!("failed to ring NVMe SQ: {error:?}"));
                }
                for _ in 0..20_000_000 {
                    if let Err(error) = cq.read_bytes(0, &mut completion) {
                        stop_before_failure!(format_args!("failed to read NVMe CQ: {error:?}"));
                    }
                    if completion[14] & 1 != 0 {
                        completed = true;
                        break;
                    }
                    core::hint::spin_loop();
                }
            }
            fence(Ordering::SeqCst);

            // A completed CQ while MSI-X is masked must not invoke the installed callback.
            for _ in 0..100_000 {
                core::hint::spin_loop();
            }
            let remained_masked = calls.load(Ordering::SeqCst) == baseline;
            let actual_masked = msix.test_vector_masked(0);
            let actual_pending = msix.test_pending(0);

            let mut delivered = false;
            if completed {
                if let Err(error) = msix.activate() {
                    stop_before_failure!(format_args!("failed to activate MSI-X: {error:?}"));
                }
                for _ in 0..20_000_000 {
                    if calls.load(Ordering::SeqCst) > baseline {
                        delivered = true;
                        break;
                    }
                    core::hint::spin_loop();
                }
            }

            let pending_cleared = !msix.test_pending(0);
            // Test a failed real shutdown transaction: no callback/vector/resource release.
            assert!(msix.shutdown(|| Err(Error::IoError)).is_err());
            assert_eq!(msix.vector_count(), 1);
            assert!(msix.test_vector_masked(0));
            assert_eq!(handler_drops.load(Ordering::SeqCst), 0,
                "failed quiesce must retain callback captures");

            // Shutdown stops the controller, retires the callback, then drains deferred
            // work while MSI-X is still enabled-but-function-masked. Only then may restore
            // release/restore the transaction. A failed stop quarantines DMA memory.
            let mut drained_after_irq = false;
            let shutdown = msix.shutdown_with_post_irq(
                || quiesce_controller(&$regs, $loc, $command),
                || {
                    assert_eq!(handler_drops.load(Ordering::SeqCst), 1,
                        "deferred drain follows synchronous IRQ callback retirement");
                    let control = $loc.read16(msix_cap_offset($loc).unwrap() + 2);
                    assert_eq!(control & 0xc000, 0xc000,
                        "deferred drain precedes MSI-X disable/unmask restoration");
                    drained_after_irq = true;
                    Ok(())
                },
            );
            if let Err(error) = shutdown {
                core::mem::forget(sq);
                core::mem::forget(cq);
                core::mem::forget(identify);
                core::mem::forget($context);
                panic!("MSI-X NVMe shutdown failed ({error:?}); DMA resources quarantined");
            }
            assert!(drained_after_irq, "successful teardown invokes the deferred drain");
            assert_eq!(handler_drops.load(Ordering::SeqCst), 1,
                "successful shutdown must synchronously release the actual IRQ capture");
            assert_eq!(
                $loc.read16(4),
                $command,
                "shutdown restores prepared command"
            );
            assert_eq!(
                $loc.read16(msix_cap_offset($loc).unwrap() + 2),
                $original_msix_control,
                "shutdown restores the exact MSI-X control word"
            );

            // Controller is stopped, so the DMA buffers can now be inspected and reclaimed.
            let mut identify_data = [0u8; PAGE_SIZE];
            identify.read_bytes(0, &mut identify_data).unwrap();
            drop(identify);
            drop(cq);
            drop(sq);
            assert!(
                ready && completed,
                "NVMe Identify CQ completion in cycle {cycle}"
            );
            assert_eq!(
                u16::from_le_bytes(completion[12..14].try_into().unwrap()),
                cycle as u16 + 1,
                "real Identify command CID"
            );
            assert_eq!(
                u16::from_le_bytes(completion[14..16].try_into().unwrap()),
                1,
                "real NVMe CQ success and phase"
            );
            assert_eq!(u16::from_le_bytes(completion[8..10].try_into().unwrap()), 1);
            assert_eq!(
                u16::from_le_bytes(completion[10..12].try_into().unwrap()),
                0
            );
            assert_eq!(
                u16::from_le_bytes(identify_data[..2].try_into().unwrap()),
                NVME_VENDOR
            );
            assert!(
                identify_data[4..24].starts_with(EXPECTED_SERIAL),
                "actual NVMe serial"
            );
            assert!(
                identify_data[24..64].starts_with(NVME_MODEL),
                "actual NVMe model"
            );
            assert!(
                remained_masked,
                "CQ interrupt was delivered while function-masked"
            );
            assert!(actual_masked && actual_pending, "physical table mask and pending bit required");
            assert!(pending_cleared, "physical PBA clears after interrupt delivery");
            assert!(
                delivered,
                "real NVMe CQ interrupt reached the registered callback"
            );
        }
    }};
}

fn msix_cap_offset(loc: PciDeviceLocation) -> Option<u16> {
    find_capability(loc, 0x11).map(u16::from)
}

#[ktest]
fn qemu_nvme_msix_masked_pending_interrupt_and_shutdown_cycles() {
    init_pci_for_tests();
    let mut bus = PCI_BUS.lock();
    let root_port = function(0, 12, 0); // Dedicated runner fixture: chassis=12.
    assert_ne!(
        root_port.read16(0),
        0xffff,
        "dedicated QEMU root port 0c.0 required"
    );
    assert_eq!(root_port.read8(0x0b), 6);
    assert_eq!(root_port.read8(0x0a), 4);
    assert_eq!(root_port.read8(0x0e) & 0x7f, 1);
    let _acs = AcsFixture::enable(root_port);
    let downstream_bus = root_port.read8(0x19);
    assert_ne!(
        downstream_bus, 0,
        "root port must route to a downstream bus"
    );
    let loc = *bus
        .unclaimed_devices()
        .find(|device| {
            let id = device.device_id();
            id.vendor_id == NVME_VENDOR
                && id.class == 1
                && id.subclass == 8
                && device.location().bus == downstream_bus
        })
        .expect("runner NVMe nvme serial=ASTER-MSIX behind dedicated 0c.0")
        .location();
    assert_eq!(loc.device, 0, "NVMe fixture occupies downstream slot zero");
    let capability = msix_cap_offset(loc).expect("real NVMe MSI-X capability");
    quiesce_qemu_nvme(&mut bus, loc);

    let mut reserved = bus.reserve_unclaimed(loc).expect("reserve dedicated NVMe");
    let original_command = loc.read16(4);
    let original_msix_control = loc.read16(capability + 2);
    assert_eq!(
        original_msix_control & 0x8000,
        0,
        "firmware MSI-X initially disabled"
    );
    let regs = reserved
        .prepare_mmio(0)
        .expect("prepare NVMe BAR0 without BME");
    let prepared_command = loc.read16(4);
    assert_eq!(prepared_command & Command::BUS_MASTER.bits(), 0);
    let table_size = (original_msix_control & 0x07ff) + 1;
    assert!(table_size > 0);

    // The dedicated QEMU controller was stopped with RDY=0 and BME off above, so
    // these empty callbacks are valid only for this provably idle source. Exercise
    // the fallible post-IRQ path before any queues or device work are started.
    {
        let mut retired = regs.prepare_msix(1).expect("prepare idle NVMe vector");
        assert_eq!(retired.vector_count(), 1);
        let handler_drops = Arc::new(AtomicUsize::new(0));
        let probe = HandlerDropProbe(handler_drops.clone());
        retired
            .register_handler(0, move |_: &TrapFrame| {
                let _keep_probe_live = &probe;
            })
            .expect("register callback before activation");
        let failed = retired.shutdown_with_post_irq(
            || {
                assert_eq!(regs.read32(0x1c).unwrap() & 1, 0, "idle NVMe is not ready");
                assert_eq!(loc.read16(4) & Command::BUS_MASTER.bits(), 0);
                Ok(())
            },
            || {
                assert_eq!(
                    handler_drops.load(Ordering::SeqCst),
                    1,
                    "IRQ callback is retired before deferred drain"
                );
                assert_eq!(
                    loc.read16(capability + 2) & 0xc000,
                    0xc000,
                    "drain failure leaves MSI-X enabled and masked"
                );
                Err(Error::IoError)
            },
        );
        assert_eq!(failed, Err(Error::IoError));
        assert_eq!(
            retired.vector_count(),
            0,
            "failed drain keeps retired IRQs retired"
        );
        assert_eq!(
            retired.activate(),
            Err(Error::AccessDenied),
            "activation must fail closed after all owned IRQs were retired"
        );
        assert_eq!(
            loc.read16(capability + 2) & 0xc000,
            0xc000,
            "rejected activation leaves MSI-X masked"
        );
        retired
            .shutdown_with_post_irq(
                || {
                    assert_eq!(regs.read32(0x1c).unwrap() & 1, 0, "NVMe remains idle");
                    assert_eq!(loc.read16(4) & Command::BUS_MASTER.bits(), 0);
                    Ok(())
                },
                || Ok(()),
            )
            .expect("successful idle shutdown retry restores MSI-X state");
        assert_eq!(loc.read16(capability + 2), original_msix_control);
        assert_eq!(loc.read16(4), prepared_command);
    }

    // Invalid requests are transactional: neither MSI-X control nor PCI command may change.
    assert!(
        regs.prepare_msix(0).is_err(),
        "zero vector request is invalid"
    );
    assert_eq!(loc.read16(capability + 2), original_msix_control);
    assert_eq!(loc.read16(4), prepared_command);
    assert!(
        regs.prepare_msix(table_size + 1).is_err(),
        "request cannot exceed table size"
    );
    assert_eq!(loc.read16(capability + 2), original_msix_control);
    assert_eq!(loc.read16(4), prepared_command);

    // Both invalid contexts must be rejected before acquiring a lease or touching hardware.
    {
        let _preempt = ostd::task::disable_preempt();
        assert!(regs.prepare_msix(1).is_err());
    }
    {
        let _irq = ostd::irq::disable_local();
        assert!(regs.prepare_msix(1).is_err());
    }
    assert_eq!(loc.read16(capability + 2), original_msix_control);
    assert_eq!(loc.read16(4), prepared_command);

    // Exhaust the actual global IRQ allocator: failure after mapping must release its lease
    // and leave every hardware configuration bit unchanged, not just reject invalid input.
    let mut held = Vec::new();
    while let Ok(line) = ostd::irq::IrqLine::alloc() {
        held.push(line);
    }
    assert!(regs.prepare_msix(1).is_err());
    assert!(table_size >= 2, "QEMU NVMe fixture needs two table entries");
    drop(held.pop().expect("at least one exhausted IRQ"));
    assert!(
        regs.prepare_msix(2).is_err(),
        "second IRQ allocation must fail"
    );
    let recovered = ostd::irq::IrqLine::alloc().expect("partial vector allocation rolls back");
    assert!(ostd::irq::IrqLine::alloc().is_err());
    drop(recovered);
    assert_eq!(loc.read16(capability + 2), original_msix_control);
    assert_eq!(loc.read16(4), prepared_command);
    drop(held);

    // Missing-handler activation and dropping an unactivated guard also roll back exactly.
    {
        let mut probe = regs.prepare_msix(1).expect("prepare probe vector");
        assert_eq!(probe.table_size() as u16, table_size);
        assert_eq!(probe.vector_count(), 1);
        assert!(
            probe.activate().is_err(),
            "missing callbacks must reject activation"
        );
        assert_eq!(loc.read16(4), prepared_command);
        drop(probe);
    }
    assert_eq!(loc.read16(capability + 2), original_msix_control);
    assert_eq!(loc.read16(4), prepared_command);

    let mask = DmaMask::new(32).unwrap();
    #[cfg(target_arch = "x86_64")]
    {
        match regs.isolated_dma_context(mask) {
            Ok(context) => run_nvme_msix_cycles!(
                regs,
                loc,
                context,
                table_size,
                prepared_command,
                original_msix_control
            ),
            Err(DeviceDmaError::UnsupportedPlatform) => match regs.dma_context(mask) {
                Ok(context) => {
                    ostd::early_println!(
                        "PCI MSI-X test using direct DMA; no IOMMU isolation claim"
                    );
                    run_nvme_msix_cycles!(
                        regs,
                        loc,
                        context,
                        table_size,
                        prepared_command,
                        original_msix_control
                    );
                }
                Err(error) => panic!("Required real NVMe MSI-X DMA unavailable: {error:?}"),
            },
            Err(error) => panic!("NVMe isolated DMA context failed: {error:?}"),
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        match regs.dma_context(mask) {
            Ok(context) => run_nvme_msix_cycles!(
                regs,
                loc,
                context,
                table_size,
                prepared_command,
                original_msix_control
            ),
            Err(error) => panic!("Required real NVMe MSI-X DMA unavailable: {error:?}"),
        }
    }

    assert_eq!(loc.read16(capability + 2), original_msix_control);
    drop(regs);
    assert_eq!(
        loc.read16(4),
        original_command,
        "MMIO drop restores exact PCI command"
    );
    drop(reserved);
    ostd::early_println!(
        "PCI real NVMe MSI-X masked-pending IRQ and quiesced shutdown cycles passed; not GPU validation"
    );
}

/// No GPU identity spoofing: use another real QEMU NVMe endpoint for the legacy API.
#[ktest]
fn qemu_legacy_msix_never_grants_dma_and_checks_indices() {
    init_pci_for_tests();
    let mut bus = PCI_BUS.lock();
    let port = function(0, 13, 0);
    assert_ne!(port.read16(0), 0xffff);
    let loc = *bus
        .unclaimed_devices()
        .find(|d| d.location().bus == port.read8(0x19) && d.device_id().vendor_id == NVME_VENDOR)
        .expect("dedicated legacy MSI-X NVMe")
        .location();
    quiesce_qemu_nvme(&mut bus, loc);
    let mut device = bus.test_take_unclaimed(loc).unwrap();
    let command = loc.read16(4);
    let cap = msix_cap_offset(loc).unwrap();
    let control = loc.read16(cap + 2);
    let mut msix = device.acquire_msix_capability().unwrap().unwrap();
    assert_eq!(
        loc.read16(4) & Command::BUS_MASTER.bits(),
        0,
        "MSI-X acquisition must not authorize DMA"
    );
    assert!(msix.irq_mut(usize::MAX).is_none());
    assert!(msix.irq_mut(usize::from(msix.table_size())).is_none());
    assert!(msix.enable_vector(0).is_err());
    let line = ostd::irq::IrqLine::alloc().unwrap();
    msix.set_interrupt_vector(line, 0).unwrap();
    assert!(
        msix.enable_vector(0).is_err(),
        "no handler must remain masked"
    );
    assert!(
        msix.set_interrupt_vector(ostd::irq::IrqLine::alloc().unwrap(), 0)
            .is_err()
    );
    drop(msix);
    // Fixture stopped before acquisition; restore control without pretending generic Drop reset it.
    loc.write16(cap + 2, control);
    device.write_command_raw(command);
    bus.return_reserved(device);
    ostd::early_println!(
        "PCI legacy MSI-X passed: no implicit BME, checked indices, no handler unmask, no vector replacement"
    );
}

#[ktest]
fn qemu_active_msix_abandonment_quarantines_function() {
    init_pci_for_tests();
    let mut bus = PCI_BUS.lock();
    let port = function(0, 14, 0);
    assert_ne!(port.read16(0), 0xffff);
    let loc = *bus
        .unclaimed_devices()
        .find(|d| d.location().bus == port.read8(0x19) && d.device_id().vendor_id == NVME_VENDOR)
        .expect("dedicated quarantine NVMe")
        .location();
    quiesce_qemu_nvme(&mut bus, loc);
    let mut reserved = bus.reserve_unclaimed(loc).unwrap();
    let regs = reserved.prepare_mmio(0).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut msix = regs.prepare_msix(1).unwrap();
    let callback_calls = calls.clone();
    msix.register_handler(0, move |_| {
        callback_calls.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();
    msix.activate().unwrap();
    // No commands/DMA started in this negative lifecycle fixture. This tests abandonment,
    // not hardware completion; the separate Identify test requires actual device IRQs.
    drop(msix);
    assert!(regs.prepare_msix(1).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(regs);
    drop(reserved);
    assert!(
        bus.reserve_unclaimed(loc).is_none(),
        "quarantined device must not be reprobed"
    );
    ostd::early_println!(
        "PCI MSI-X abandonment passed: function and MMIO quarantined, no reprobe; not a hardware completion test"
    );
}
