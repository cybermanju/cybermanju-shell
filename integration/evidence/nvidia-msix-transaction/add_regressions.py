from pathlib import Path
R=Path('/workspace/asterinas-gpui/asterinas')
p=R/'kernel/core/comps/pci/src/msix_tests.rs'
s=p.read_text()
s += '''
/// No GPU identity spoofing: use another real QEMU NVMe endpoint for the legacy API.
#[ktest]
fn qemu_legacy_msix_never_grants_dma_and_checks_indices() {
    init_pci_for_tests();
    let mut bus = PCI_BUS.lock();
    let port = function(0, 13, 0);
    assert_ne!(port.read16(0), 0xffff);
    let loc = *bus.unclaimed_devices().find(|d| d.location().bus == port.read8(0x19)
        && d.device_id().vendor_id == NVME_VENDOR).expect("dedicated legacy MSI-X NVMe").location();
    quiesce_qemu_nvme(&mut bus, loc);
    let mut device = bus.test_take_unclaimed(loc).unwrap();
    let command = loc.read16(4);
    let cap = msix_cap_offset(loc).unwrap();
    let control = loc.read16(cap + 2);
    let mut msix = device.acquire_msix_capability().unwrap().unwrap();
    assert_eq!(loc.read16(4) & Command::BUS_MASTER.bits(), 0,
        "MSI-X acquisition must not authorize DMA");
    assert!(msix.irq_mut(usize::MAX).is_none());
    assert!(msix.irq_mut(usize::from(msix.table_size())).is_none());
    assert!(msix.enable_vector(0).is_err());
    let line = ostd::irq::IrqLine::alloc().unwrap();
    msix.set_interrupt_vector(line, 0).unwrap();
    assert!(msix.enable_vector(0).is_err(), "no handler must remain masked");
    assert!(msix.set_interrupt_vector(ostd::irq::IrqLine::alloc().unwrap(), 0).is_err());
    drop(msix);
    // Fixture stopped before acquisition; restore control without pretending generic Drop reset it.
    loc.write16(cap + 2, control);
    device.write_command_raw(command);
    bus.return_reserved(device);
    ostd::early_println!("PCI legacy MSI-X passed: no implicit BME, checked indices, no handler unmask, no vector replacement");
}

#[ktest]
fn qemu_active_msix_abandonment_quarantines_function() {
    init_pci_for_tests();
    let mut bus = PCI_BUS.lock();
    let port = function(0, 14, 0);
    assert_ne!(port.read16(0), 0xffff);
    let loc = *bus.unclaimed_devices().find(|d| d.location().bus == port.read8(0x19)
        && d.device_id().vendor_id == NVME_VENDOR).expect("dedicated quarantine NVMe").location();
    quiesce_qemu_nvme(&mut bus, loc);
    let mut reserved = bus.reserve_unclaimed(loc).unwrap();
    let regs = reserved.prepare_mmio(0).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut msix = regs.prepare_msix(1).unwrap();
    let callback_calls = calls.clone();
    msix.register_handler(0, move |_| { callback_calls.fetch_add(1, Ordering::SeqCst); }).unwrap();
    msix.activate().unwrap();
    // No commands/DMA started in this negative lifecycle fixture. This tests abandonment,
    // not hardware completion; the separate Identify test requires actual device IRQs.
    drop(msix);
    assert!(regs.prepare_msix(1).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(regs);
    drop(reserved);
    assert!(bus.reserve_unclaimed(loc).is_none(), "quarantined device must not be reprobed");
    ostd::early_println!("PCI MSI-X abandonment passed: function and MMIO quarantined, no reprobe; not a hardware completion test");
}
'''
p.write_text(s)
p=R/'tools/graphics/run_cpu_kernel_tests.sh';s=p.read_text(); needle='            pci_test_device+=" -device pcie-root-port,id=rp-msix,addr=0c.0,chassis=12,hotplug=off -device nvme,bus=rp-msix,addr=00.0,serial=ASTER-MSIX"'
s=s.replace(needle,needle+'\n            pci_test_device+=" -device pcie-root-port,id=rp-msix-legacy,addr=0d.0,chassis=13,hotplug=off -device nvme,bus=rp-msix-legacy,addr=00.0,serial=ASTER-MSIX-LEGACY"\n            pci_test_device+=" -device pcie-root-port,id=rp-msix-quarantine,addr=0e.0,chassis=14,hotplug=off -device nvme,bus=rp-msix-quarantine,addr=00.0,serial=ASTER-MSIX-QUARANTINE"')
s=s.replace('    if [[ "$suite" == pci ]]; then','''    if [[ "$suite" == pci || "$suite" == pci-iommu ]]; then
        grep -Fq 'PCI real NVMe MSI-X masked-pending IRQ and quiesced shutdown cycles passed; not GPU validation' "$log"
        grep -Fq 'PCI legacy MSI-X passed: no implicit BME, checked indices, no handler unmask, no vector replacement' "$log"
        grep -Fq 'PCI MSI-X abandonment passed: function and MMIO quarantined, no reprobe; not a hardware completion test' "$log"
    fi
    if [[ "$suite" == pci ]]; then''')
p.write_text(s)
print('Hardware fixture and rollback regressions added')
