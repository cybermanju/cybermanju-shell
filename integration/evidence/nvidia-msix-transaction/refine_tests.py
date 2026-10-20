from pathlib import Path
import shutil
R=Path('/workspace/asterinas-gpui/asterinas'); O=R.parent/'evidence/nvidia-msix-transaction'
p=R/'kernel/core/comps/virtio/src/transport/pci/legacy.rs'; b=O/'before'/p.relative_to(R)
b.parent.mkdir(parents=True,exist_ok=True)
if not b.exists():shutil.copyfile(p,b)
s=p.read_text().replace('        let msix_manager = VirtioMsixManager::new(msix);','''        let msix_manager = VirtioMsixManager::new(msix);
        // DMA remains an explicit transport policy, not an IRQ-acquisition side effect.
        common_device.write_command_raw(common_device.location().read16(4) | aster_pci::cfg_space::Command::BUS_MASTER.bits());''',1)
s=s.replace('''            .write_once(QUEUE_MSIX_VECTOR_OFFSET, vector)
            .unwrap();
        Ok(())''','''            .write_once(QUEUE_MSIX_VECTOR_OFFSET, vector)
            .unwrap();
        self.msix_manager.enable_vector(vector).map_err(|_| VirtioTransportError::InvalidArgs)?;
        Ok(())''',1)
s=s.replace('''            .write_once(CONFIG_MSIX_VECTOR_OFFSET, vector)
            .unwrap();
        Ok(())''','''            .write_once(CONFIG_MSIX_VECTOR_OFFSET, vector)
            .unwrap();
        self.msix_manager.enable_vector(vector).map_err(|_| VirtioTransportError::InvalidArgs)?;
        Ok(())''',1)
p.write_text(s)
p=R/'kernel/core/comps/pci/src/msix_tests.rs'; s=p.read_text()
s=s.replace('use alloc::sync::Arc;', 'use alloc::{sync::Arc, vec::Vec};')
s=s.replace('loc.read8(at)', 'loc.read8(u16::from(at))').replace('loc.read8(at + 1)', 'loc.read8(u16::from(at) + 1)')
s=s.replace('''                Err(error) => {
                    assert_eq!(loc.read16(capability + 2), original_msix_control);
                    assert_eq!(loc.read16(4), prepared_command);
                    ostd::early_println!(
                        "PCI MSI-X preparation passed; DMA unavailable: {error:?}"
                    );
                }''','''                Err(error) => panic!("Required real NVMe MSI-X DMA unavailable: {error:?}"),''')
s=s.replace('''            Err(error) => {
                ostd::early_println!("PCI MSI-X preparation passed; DMA unavailable: {error:?}")
            }''','''            Err(error) => panic!("Required real NVMe MSI-X DMA unavailable: {error:?}"),''')
s=s.replace('''    // Missing-handler activation and dropping an unactivated guard also roll back exactly.''','''    // Exhaust the actual global IRQ allocator: failure after mapping must release its lease
    // and leave every hardware configuration bit unchanged, not just reject invalid input.
    let mut held = Vec::new();
    while let Ok(line) = ostd::irq::IrqLine::alloc() { held.push(line); }
    assert!(regs.prepare_msix(1).is_err());
    assert_eq!(loc.read16(capability + 2), original_msix_control);
    assert_eq!(loc.read16(4), prepared_command);
    drop(held);

    // Missing-handler activation and dropping an unactivated guard also roll back exactly.''')
s=s.replace('''            let remained_masked = calls.load(Ordering::SeqCst) == baseline;''','''            let remained_masked = calls.load(Ordering::SeqCst) == baseline;
            let actual_masked = msix.test_vector_masked(0);
            let actual_pending = msix.test_pending(0);''')
s=s.replace('''            // Shutdown invokes the device-specific stop and waits for RDY=0 before the''','''            let pending_cleared = !msix.test_pending(0);
            // Test a failed real shutdown transaction: no callback/vector/resource release.
            assert!(msix.shutdown(|| Err(Error::IoError)).is_err());
            assert_eq!(msix.vector_count(), 1);
            assert!(msix.test_vector_masked(0));

            // Shutdown invokes the device-specific stop and waits for RDY=0 before the''')
s=s.replace('''            assert!(
                delivered,
                "real NVMe CQ interrupt reached the registered callback"
            );''','''            assert!(actual_masked && actual_pending, "physical table mask and pending bit required");
            assert!(pending_cleared, "physical PBA clears after interrupt delivery");
            assert!(
                delivered,
                "real NVMe CQ interrupt reached the registered callback"
            );''')
p.write_text(s)
print('Refinements applied')
