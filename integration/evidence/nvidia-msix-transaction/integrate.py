from pathlib import Path
import shutil
R=Path('/workspace/asterinas-gpui/asterinas'); O=R.parent/'evidence/nvidia-msix-transaction'
def change(name,old,new):
 p=R/name
 before=O/'before'/name
 if not before.exists():
  before.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,before)
 s=p.read_text()
 if old not in s: raise RuntimeError('Missing context '+name+' '+old[:80])
 p.write_text(s.replace(old,new,1))
change('ostd/src/irq/top_half.rs','    /// Checks if there are no registered callbacks.','''    /// Disconnects this handle's callbacks synchronously and permanently reserves its IRQ.
    ///
    /// Call only from task context, not from a callback or with locks needed by one held.
    /// This waits for registry dispatch readers, destroys captures outside the lock, and
    /// leaves an empty dispatch/EOI sink for late device messages. It does not claim a
    /// cross-CPU APIC/IRTE drain or allow this IRQ number to be allocated again. Other
    /// cloned handles, if any, retain their own callbacks and must be stopped separately.
    pub fn retire(mut self) {
        self.callbacks.clear();
        core::mem::forget(self);
    }

    /// Checks if there are no registered callbacks.''')
change('kernel/core/comps/pci/src/lib.rs','pub mod mmio;','pub mod mmio;\npub mod msix_transaction;\n#[cfg(ktest)]\nmod msix_tests;')
change('kernel/core/comps/pci/src/capability/msix.rs','#[derive(Debug)]\npub(super) struct RawCapabilityMsix {\n    cap_ptr: u16,\n    msg_ctrl: u16,\n    table_info: u32,\n    pba_info: u32,','#[derive(Debug)]\npub(crate) struct RawCapabilityMsix {\n    pub(crate) cap_ptr: u16,\n    msg_ctrl: u16,\n    pub(crate) table_info: u32,\n    pub(crate) pba_info: u32,')
change('kernel/core/comps/pci/src/capability/mod.rs','    msix: Option<RawCapabilityMsix>,','    pub(crate) msix: Option<RawCapabilityMsix>,')
change('kernel/core/comps/pci/src/common_device.rs','use ostd::Result;','use core::sync::atomic::{AtomicBool, Ordering};\n\nuse ostd::Result;')
change('kernel/core/comps/pci/src/common_device.rs','    capabilities: RawCapabilities,','    capabilities: RawCapabilities,\n    irq_quarantined: AtomicBool,')
change('kernel/core/comps/pci/src/common_device.rs','    /// Returns the PCI device ID.','''    pub(crate) fn msix_descriptor(&self) -> Option<(u16, u16, u32, u32)> {
        let raw = self.capabilities.msix.as_ref()?;
        Some((raw.cap_ptr, self.location.read16(raw.cap_ptr + 2), raw.table_info, raw.pba_info))
    }

    pub(crate) fn quarantine_irqs(&self) { self.irq_quarantined.store(true, Ordering::Release); }
    pub(crate) fn irqs_quarantined(&self) -> bool { self.irq_quarantined.load(Ordering::Acquire) }

    /// Returns the PCI device ID.''')
change('kernel/core/comps/pci/src/common_device.rs','            capabilities,','            capabilities,\n            irq_quarantined: AtomicBool::new(false),')
change('kernel/core/comps/pci/src/common_device.rs','self.bars[idx as usize].as_ref()','self.bars.get(idx as usize)?.as_ref()')
change('kernel/core/comps/pci/src/common_device.rs','self.bars[idx as usize].as_mut()','self.bars.get_mut(idx as usize)?.as_mut()')
change('kernel/core/comps/pci/src/cfg_space.rs','pub fn acquire_owned(&mut self) -> Result<IoMem>','pub fn acquire_owned(&self) -> Result<IoMem>')
change('kernel/core/comps/pci/src/mmio.rs','use ostd::{Error, io::IoMem, mm::VmIoOnce};','use core::sync::atomic::{AtomicBool, Ordering};\n\nuse ostd::{Error, io::IoMem, mm::VmIoOnce};')
change('kernel/core/comps/pci/src/mmio.rs','    original_command: u16,','    original_command: u16,\n    bar_index: u8,\n    msix_owned: AtomicBool,')
change('kernel/core/comps/pci/src/mmio.rs','        let loc = device.location();','''        if device.irqs_quarantined() {
            return Err(MmioPrepareError::Mapping(Error::AccessDenied));
        }
        let loc = device.location();''')
change('kernel/core/comps/pci/src/mmio.rs','            original_command,','            original_command,\n            bar_index,\n            msix_owned: AtomicBool::new(false),')
change('kernel/core/comps/pci/src/mmio.rs','    /// Read a naturally aligned 32-bit register inside the mapped BAR.','''    /// Prepares masked MSI-X entries without enabling DMA or installing dummy handlers.
    /// Only one transaction may exist. It borrows this guard and therefore the reservation.
    pub fn prepare_msix(&self, count: u16) -> Result<crate::msix_transaction::PciMsixGuard<'_>, Error> {
        crate::msix_transaction::PciMsixGuard::prepare(self, count)
    }

    pub(crate) fn msix_descriptor(&self) -> Option<(u16, u16, u32, u32)> { self.device.msix_descriptor() }
    pub(crate) fn claim_msix(&self) -> Result<(), Error> {
        if self.device.irqs_quarantined() { return Err(Error::AccessDenied); }
        self.msix_owned.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ()).map_err(|_| Error::AccessDenied)
    }
    pub(crate) fn release_msix(&self) { self.msix_owned.store(false, Ordering::Release); }
    pub(crate) fn quarantine_irq_resources(&self) { self.device.quarantine_irqs(); }
    pub(crate) fn write_irq_command(&self, command: u16) { self.device.write_command_raw(command); }
    pub(crate) fn bar_size(&self, index: u8) -> Result<usize, Error> {
        match self.device.bar_manager().bar(index) {
            Some(Bar::Memory(bar)) => usize::try_from(bar.size()).map_err(|_| Error::Overflow),
            _ => Err(Error::InvalidArgs),
        }
    }
    pub(crate) fn map_msix_bar(&self, index: u8) -> Result<IoMem, Error> {
        if index == self.bar_index { return Ok(self.mapping.as_ref().unwrap().clone()); }
        match self.device.bar_manager().bar(index) {
            Some(Bar::Memory(bar)) => bar.acquire_owned(),
            _ => Err(Error::InvalidArgs),
        }
    }

    /// Read a naturally aligned 32-bit register inside the mapped BAR.''')
change('kernel/core/comps/pci/src/mmio.rs','        // No DMA buffers, ISR callbacks, task queues or GPU commands are ever created here.','''        if self.msix_owned.load(Ordering::Acquire) || self.device.irqs_quarantined() {
            // An active/forgotten IRQ transaction has no proven device stop. Retain its
            // register mapping and prevent reservation reprobe, including group members.
            self.device.quarantine_irqs();
            core::mem::forget(self.mapping.take());
            return;
        }
        // No live IRQ transaction remains; callbacks have been synchronously disconnected.''')
change('kernel/core/comps/pci/src/bus.rs','    drivers: Vec<Arc<dyn PciDriver>>,','    drivers: Vec<Arc<dyn PciDriver>>,\n    quarantined_devices: Vec<PciCommonDevice>,')
change('kernel/core/comps/pci/src/bus.rs','        self.common_devices.push_back(device);','''        if device.irqs_quarantined() {
            self.quarantined_devices.push(device);
        } else {
            self.common_devices.push_back(device);
        }''')
change('kernel/core/comps/pci/src/bus.rs','            drivers: Vec::new(),','            drivers: Vec::new(),\n            quarantined_devices: Vec::new(),')
change('kernel/core/comps/pci/src/reservation.rs','        for device in self.devices.drain(..) {','''        if self.devices.iter().any(PciCommonDevice::irqs_quarantined) {
            for device in &self.devices { device.quarantine_irqs(); }
        }
        for device in self.devices.drain(..) {''')
change('kernel/comps/nvidia-platform/src/device.rs','    /// Read a register via the real bounded MMIO mapping.','''    /// Reserves real MSI-X vectors masked, without enabling DMA or inventing RM callbacks.
    /// The caller must register real device handlers before activation, and implement NVIDIA
    /// hardware/worker quiescence before shutdown. This is not probe or RM/GSP success.
    pub fn prepare_msix(&self, count: u16) -> Result<aster_pci::msix_transaction::PciMsixGuard<'_>, ostd::Error> {
        self.registers.prepare_msix(count)
    }

    /// Read a register via the real bounded MMIO mapping.''')
# Legacy API kept for existing drivers. DMA authorization is now explicit at driver call sites.
change('kernel/core/comps/pci/src/capability/msix.rs','(raw_cap.msg_ctrl & 0b11_1111_1111) + 1','(raw_cap.msg_ctrl & 0x7ff) + 1')
change('kernel/core/comps/pci/src/capability/msix.rs','        // Disable INTx. Enable bus master.\n        loc.write16(\n            PciCommonCfgOffset::Command as u16,\n            loc.read16(PciCommonCfgOffset::Command as u16)\n                | (Command::INTERRUPT_DISABLE | Command::BUS_MASTER).bits(),\n        );','''        // Disable INTx, but never authorize DMA as a side effect of IRQ acquisition.
        // Zero the adjacent W1C Status half rather than using config write16's RMW.
        loc.write32(
            PciCommonCfgOffset::Command as u16,
            u32::from(loc.read16(PciCommonCfgOffset::Command as u16) | Command::INTERRUPT_DISABLE.bits()),
        );''')
change('kernel/core/comps/pci/src/capability/msix.rs','self.irqs[index].as_mut()','self.irqs.get_mut(index)?.as_mut()')
change('kernel/core/comps/nvme/src/transport/pci/transport.rs','        info!("MSI-X enabled with {} vectors", manager.table_size());','''        // DMA is a driver decision, not an implicit effect of acquiring MSI-X.
        common_device.write_command_raw(common_device.location().read16(4) | aster_pci::cfg_space::Command::BUS_MASTER.bits());
        info!("MSI-X enabled with {} vectors", manager.table_size());''')
change('kernel/core/comps/virtio/src/transport/pci/device.rs','        let msix_manager = VirtioMsixManager::new(msix);','''        let msix_manager = VirtioMsixManager::new(msix);
        // Preserve the existing transport DMA policy, explicitly at the driver boundary.
        common_device.write_command_raw(common_device.location().read16(4) | aster_pci::cfg_space::Command::BUS_MASTER.bits());''')
change('tools/graphics/run_cpu_kernel_tests.sh','            pci_test_device+=" -device pcie-root-port,id=rp-group-bad','            pci_test_device+=" -device pcie-root-port,id=rp-msix,addr=0c.0,chassis=12,hotplug=off -device nvme,bus=rp-msix,addr=00.0,serial=ASTER-MSIX"\n            pci_test_device+=" -device pcie-root-port,id=rp-group-bad')
print('Integration complete; no Git index writes')
